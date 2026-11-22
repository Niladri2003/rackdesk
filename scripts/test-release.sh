#!/usr/bin/env bash
# Builds a release and tests it from end to end in containers. Needs Docker.
#
#   scripts/test-release.sh           build, then test
#   scripts/test-release.sh --no-build   test the files that are in dist/
#
# What it tests, on a Debian container that runs systemd:
#   - the installer refuses a changed archive and a changed list of checksums
#   - a first install: user, files, running service, sign-in with the printed
#     password, and no password in the journal
#   - a second run of the installer keeps the account
#   - root is refused on the data folder of the service user
#   - the root program and the terminal option
#   - rackdesk-uninstall leaves nothing behind
#   - the .deb package installs, starts and is purged
#
# scripts/local-release.sh builds the release and signs it with a throwaway
# key. The real key is never used and never needed.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD_FLAG=""
[ "${1:-}" = "--no-build" ] && BUILD_FLAG="--no-build"

case "$(uname -m)" in
  arm64 | aarch64) ARCH=aarch64 ;;
  x86_64 | amd64) ARCH=x86_64 ;;
  *) echo "unknown processor type" >&2; exit 1 ;;
esac
ARCHIVE="rackdesk-linux-$ARCH.tar.gz"

WORK="$(mktemp -d)"
CONTAINER="rackdesk-release-test-$$"
trap 'docker rm -f "$CONTAINER" >/dev/null 2>&1 || true; rm -rf "$WORK"' EXIT

# A release folder for this processor type, signed with a throwaway key.
REL="$WORK/release"
# shellcheck disable=SC2086  # the flag is 1 word or nothing
RELEASE_DIR="$REL" scripts/local-release.sh $BUILD_FLAG "$ARCH"

# ---------- The test, as a script that runs in the container ----------
cat >"$WORK/test.sh" <<'TEST'
#!/bin/bash
set -euo pipefail
ARCHIVE="$1"
REL=/release
URL=http://127.0.0.1:7171

say() { printf '\n== %s\n' "$*"; }
wait_up() {
  for _ in $(seq 1 40); do
    curl -fsS "$URL/healthz" >/dev/null 2>&1 && return 0
    sleep 0.25
  done
  echo "the server did not answer"; journalctl -u rackdeskd --no-pager -n 30; exit 1
}
nothing_installed() {
  for path in /usr/local/bin/rackdeskd /usr/local/bin/rackdesk-root /usr/local/sbin/rackdesk-uninstall \
    /etc/systemd/system/rackdeskd.service /etc/systemd/system/rackdesk-root.service \
    /etc/systemd/system/rackdeskd.service.d /etc/rackdesk /var/lib/rackdesk /run/rackdesk; do
    if [ -e "$path" ]; then echo "left behind: $path"; exit 1; fi
  done
  if id rackdesk >/dev/null 2>&1; then echo "the user rackdesk is left behind"; exit 1; fi
}

say "A changed archive is refused"
cp -r "$REL" /changed-archive
printf 'x' >>"/changed-archive/$ARCHIVE"
if sh /changed-archive/install.sh --from /changed-archive >/tmp/out 2>&1; then
  echo "installed a changed archive"; exit 1
fi
grep -q "checksum of $ARCHIVE is wrong" /tmp/out
nothing_installed

say "A changed list of checksums is refused"
cp -r "$REL" /changed-list
printf 'x' >>"/changed-list/$ARCHIVE"
(cd /changed-list && sha256sum -- "$ARCHIVE" >SHA256SUMS)
if sh /changed-list/install.sh --from /changed-list >/tmp/out 2>&1; then
  echo "installed with a changed list"; exit 1
fi
grep -q "signature of the checksum list is wrong" /tmp/out
nothing_installed

say "A first install"
sh "$REL/install.sh" --from "$REL" | tee /tmp/install.out
password="$(sed -n 's/^ *password: //p' /tmp/install.out)"
[ "${#password}" -eq 20 ]
wait_up
[ "$(systemctl is-active rackdeskd)" = active ]
[ "$(systemctl is-enabled rackdeskd)" = enabled ]
[ "$(systemctl is-active rackdesk-root || true)" != active ]
[ "$(ps -o user= -C rackdeskd | tr -d ' ')" = rackdesk ]
[ "$(stat -c '%a %U' /var/lib/rackdesk)" = "700 rackdesk" ]
[ "$(stat -c '%a %U' /var/lib/rackdesk/credentials.json)" = "600 rackdesk" ]
[ "$(cat /var/lib/rackdesk/version)" = 1 ]
grep -q NoNewPrivs:.*1 "/proc/$(pidof rackdeskd)/status"

say "The password is in no log"
if journalctl --no-pager | grep -q "$password"; then echo "the password is in the journal"; exit 1; fi

say "Sign-in with the printed password"
code="$(curl -s -o /dev/null -w '%{http_code}' -H "Origin: $URL" -H 'Content-Type: application/json' \
  -d "{\"username\":\"admin\",\"password\":\"$password\"}" "$URL/api/v1/session")"
[ "$code" = 200 ]

say "A second run keeps the account"
before="$(sha256sum /var/lib/rackdesk/credentials.json)"
sh "$REL/install.sh" --from "$REL" | tee /tmp/update.out
if grep -q "password: " /tmp/update.out; then echo "an update printed a password"; exit 1; fi
[ "$(sha256sum /var/lib/rackdesk/credentials.json)" = "$before" ]
wait_up

say "Root is refused on the data folder of the service user"
if rackdeskd --data-dir /var/lib/rackdesk reset-password >/tmp/out 2>&1; then
  echo "root could reset the password"; exit 1
fi
grep -q "runs as root" /tmp/out
runuser -u rackdesk -- rackdeskd --data-dir /var/lib/rackdesk reset-password | grep -q "password: "

say "The root program and the terminal"
useradd --create-home deploy
sh "$REL/install.sh" --from "$REL" --shell-user deploy >/tmp/out
wait_up
[ "$(systemctl is-active rackdesk-root)" = active ]
[ "$(stat -c '%a %U' /run/rackdesk/root.sock)" = "600 rackdesk" ]
grep -q '^RACKDESK_TERMINAL=as-user$' /etc/rackdesk/rackdesk.env
grep -q '^RACKDESK_SHELL_USER=deploy$' /etc/rackdesk/rackdesk.env
if sh "$REL/install.sh" --from "$REL" --shell-user root >/tmp/out 2>&1; then
  echo "a root terminal was allowed without --allow-root-shell"; exit 1
fi
if sh "$REL/install.sh" --from "$REL" --user root >/tmp/out 2>&1; then
  echo "the server was allowed to run as root"; exit 1
fi

say "The limits on the service, as systemd rates them"
systemd-analyze security rackdeskd --no-pager | tail -n 1

say "Uninstall"
if rackdesk-uninstall </dev/null >/tmp/out 2>&1; then
  echo "the data was deleted without a question"; exit 1
fi
rackdesk-uninstall --yes
nothing_installed
id deploy >/dev/null

say "Install with --user keeps that user at uninstall"
sh "$REL/install.sh" --from "$REL" --user deploy >/tmp/out
wait_up
[ "$(ps -o user= -C rackdeskd | tr -d ' ')" = deploy ]
rackdesk-uninstall --yes >/dev/null
nothing_installed
id deploy >/dev/null

if ls "$REL"/*.deb >/dev/null 2>&1; then
  say "The .deb package"
  apt-get install -y "$REL"/*.deb >/tmp/apt.out 2>&1 || { cat /tmp/apt.out; exit 1; }
  grep -q "sudo -u rackdesk rackdeskd" /tmp/apt.out
  [ "$(systemctl is-active rackdeskd || true)" != active ]
  runuser -u rackdesk -- rackdeskd --data-dir /var/lib/rackdesk init | grep -q "password: "
  systemctl enable --now rackdeskd >/dev/null 2>&1
  wait_up
  [ "$(ps -o user= -C rackdeskd | tr -d ' ')" = rackdesk ]
  apt-get purge -y rackdesk >/dev/null 2>&1
  for path in /usr/bin/rackdeskd /usr/bin/rackdesk-root /usr/lib/systemd/system/rackdeskd.service /etc/rackdesk /var/lib/rackdesk; do
    if [ -e "$path" ]; then echo "left behind after purge: $path"; exit 1; fi
  done
  if id rackdesk >/dev/null 2>&1; then echo "the user is left after purge"; exit 1; fi
fi

if ls "$REL"/*.rpm >/dev/null 2>&1; then
  say "The .rpm package holds the files"
  rpm -qpl "$REL"/*.rpm | grep -q '^/usr/bin/rackdeskd$'
  rpm -qpl "$REL"/*.rpm | grep -q '^/usr/lib/systemd/system/rackdeskd.service$'
fi

printf '\nAll release tests passed.\n'
TEST

# ---------- A container that runs systemd ----------
docker build -q -t rackdesk-systemd-test - >/dev/null <<'DOCKER'
FROM debian:12-slim
RUN apt-get update && apt-get install -y --no-install-recommends \
      systemd systemd-sysv openssl curl ca-certificates procps rpm \
    && rm -rf /var/lib/apt/lists/*
STOPSIGNAL SIGRTMIN+3
CMD ["/sbin/init"]
DOCKER

docker run -d --name "$CONTAINER" --privileged --cgroupns=host \
  -v /sys/fs/cgroup:/sys/fs/cgroup:rw --tmpfs /run --tmpfs /run/lock \
  rackdesk-systemd-test >/dev/null
docker cp "$REL" "$CONTAINER:/release"
docker cp "$WORK/test.sh" "$CONTAINER:/test.sh"
# Wait until systemd in the container is ready.
for _ in $(seq 1 40); do
  state="$(docker exec "$CONTAINER" systemctl is-system-running 2>/dev/null || true)"
  case "$state" in running | degraded) break ;; esac
  sleep 0.5
done
docker exec "$CONTAINER" bash /test.sh "$ARCHIVE"
