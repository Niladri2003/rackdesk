#!/bin/sh
# Installs or updates RackDesk on a Linux server with systemd.
#
#   curl -fsSL https://github.com/Niladri2003/rackdesk/releases/latest/download/install.sh | sudo sh
#
# With options:
#   curl -fsSL .../install.sh | sudo sh -s -- --shell-user deploy
#
# What it does:
#   1. Downloads the release for this machine and the signed list of checksums.
#   2. Checks the signature of that list with the key in this file, then checks
#      the download against the list. Nothing is installed if a check fails.
#   3. Installs 2 programs, 2 systemd units, a settings file and an uninstall
#      command. A list of every path is printed by `rackdesk-uninstall --list`.
#   4. Creates the admin account and prints its password once.
#   5. Starts the server on 127.0.0.1:7171.
#
# Running it again updates the programs and keeps the settings and the data.
set -eu

REPO="Niladri2003/rackdesk"

# The public half of the key that signs releases. The private half is held by
# the project and is not in the repository. `scripts/make-release-key.sh`
# writes this block.
# BEGIN RELEASE KEY
RELEASE_KEY='-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE63sKeDrXbSE2dovmIWOinWewt7GI
Ev2X7p7byTrhSL2pWiyg0O3oL0Kx+nBnuImtWpTHD3iMVNXXu2UqR/JuSA==
-----END PUBLIC KEY-----'
# END RELEASE KEY

BIN_DIR=/usr/local/bin
SBIN_DIR=/usr/local/sbin
UNIT_DIR=/etc/systemd/system
ETC_DIR=/etc/rackdesk
DATA_DIR=/var/lib/rackdesk
ENV_FILE="$ETC_DIR/rackdesk.env"

VERSION=""
FROM_DIR=""
LISTEN=""
RUN_USER=""
SHELL_USER=""
WITH_ROOT=0
ALLOW_ROOT_SHELL=0

usage() {
  cat <<'EOF'
Usage: install.sh [options]

  --version VERSION   Install this release (for example v1.0.0). Default: the newest.
  --listen ADDRESS    The address the server listens on. Default: 127.0.0.1:7171.
                      On an address that is not 127.0.0.1 the server uses HTTPS.
  --user NAME         Run the server as this existing Linux user. The file browser
                      then has the rights of that user. Default: a new system user
                      named rackdesk, which can read what every user can read.
  --with-root         Enable rackdesk-root, which does a fixed list of actions as
                      root: file details, permissions, owner, reading a protected file.
  --shell-user NAME   Turn the terminal on. Its shells run as this Linux user.
                      This also enables rackdesk-root, which starts the shells.
  --allow-root-shell  Allow --shell-user root.
  --from FOLDER       Take the release files from a folder and download nothing.
                      The signature and the checksum are checked in the same way.
  -h, --help          Show this text.
EOF
}

fail() {
  printf '\nError: %s\n' "$*" >&2
  exit 1
}

step() {
  printf '%-34s' "$1"
}

ok() {
  printf 'ok\n'
}

need_value() {
  [ $# -ge 2 ] || fail "$1 needs a value"
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) need_value "$@"; VERSION="$2"; shift 2 ;;
    --listen) need_value "$@"; LISTEN="$2"; shift 2 ;;
    --user) need_value "$@"; RUN_USER="$2"; shift 2 ;;
    --shell-user) need_value "$@"; SHELL_USER="$2"; shift 2 ;;
    --from) need_value "$@"; FROM_DIR="$2"; shift 2 ;;
    --with-root) WITH_ROOT=1; shift ;;
    --allow-root-shell) ALLOW_ROOT_SHELL=1; shift ;;
    -h | --help) usage; exit 0 ;;
    *) usage >&2; fail "unknown option: $1" ;;
  esac
done

# ---------- Checks before anything is changed ----------

[ "$(uname -s)" = "Linux" ] || fail "RackDesk runs on Linux. This is $(uname -s). To try it here, download the program and run: rackdeskd --demo"
[ "$(id -u)" -eq 0 ] || fail "run this as root, for example: curl -fsSL ... | sudo sh"

case "$(uname -m)" in
  x86_64 | amd64) ARCH=x86_64 ;;
  aarch64 | arm64) ARCH=aarch64 ;;
  *) fail "no release for this processor: $(uname -m). Releases exist for x86_64 and aarch64" ;;
esac

for tool in openssl sha256sum tar mktemp; do
  command -v "$tool" >/dev/null 2>&1 || fail "the program '$tool' is needed and was not found. Install it and run this again"
done

[ -n "$RELEASE_KEY" ] || fail "this copy of the installer holds no release key, so it can't check a download. Use the install.sh of a published release"

# A name that Linux tools accept, and that can't be read as an option.
valid_name() {
  case "$1" in
    "" | -* | *[!a-zA-Z0-9_.-]*) return 1 ;;
    *) return 0 ;;
  esac
}

if [ -n "$RUN_USER" ]; then
  valid_name "$RUN_USER" || fail "--user: '$RUN_USER' is not a user name"
  id "$RUN_USER" >/dev/null 2>&1 || fail "--user: the user '$RUN_USER' does not exist. Create it first, or leave the option out"
  [ "$(id -u "$RUN_USER")" -ne 0 ] || fail "--user root is not allowed: the server must not run as root. Use --with-root for actions that need root"
fi

if [ -n "$SHELL_USER" ]; then
  valid_name "$SHELL_USER" || fail "--shell-user: '$SHELL_USER' is not a user name"
  id "$SHELL_USER" >/dev/null 2>&1 || fail "--shell-user: the user '$SHELL_USER' does not exist"
  if [ "$(id -u "$SHELL_USER")" -eq 0 ] && [ "$ALLOW_ROOT_SHELL" -ne 1 ]; then
    fail "--shell-user root gives the terminal every right on this machine. Add --allow-root-shell if that is what you want"
  fi
  WITH_ROOT=1
fi

if [ -n "$LISTEN" ]; then
  case "$LISTEN" in
    *[!]0-9a-fA-F.:[]* | "") fail "--listen: '$LISTEN' is not an address. Example: 127.0.0.1:7171" ;;
  esac
fi

HAVE_SYSTEMD=0
if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
  HAVE_SYSTEMD=1
fi

# ---------- Download and check ----------

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT INT TERM

ARCHIVE="rackdesk-linux-$ARCH.tar.gz"

fetch() {
  # fetch NAME: puts the release file NAME into $WORK.
  if [ -n "$FROM_DIR" ]; then
    [ -f "$FROM_DIR/$1" ] || fail "$FROM_DIR/$1 is missing"
    cp "$FROM_DIR/$1" "$WORK/$1"
    return
  fi
  if [ -n "$VERSION" ]; then
    url="https://github.com/$REPO/releases/download/$VERSION/$1"
  else
    url="https://github.com/$REPO/releases/latest/download/$1"
  fi
  if command -v curl >/dev/null 2>&1; then
    curl -fsSL --proto '=https' --tlsv1.2 -o "$WORK/$1" "$url" || fail "could not download $url"
  elif command -v wget >/dev/null 2>&1; then
    wget -q --https-only -O "$WORK/$1" "$url" || fail "could not download $url"
  else
    fail "curl or wget is needed to download the release"
  fi
}

if [ -n "$VERSION" ]; then
  case "$VERSION" in
    v[0-9]*) ;;
    *) fail "--version: write it as in the release name, for example v1.0.0" ;;
  esac
  case "$VERSION" in
    *[!a-zA-Z0-9.-]*) fail "--version: '$VERSION' is not a release name" ;;
  esac
fi

step "Checking the release signature"
fetch SHA256SUMS
fetch SHA256SUMS.sig
fetch "$ARCHIVE"

printf '%s\n' "$RELEASE_KEY" >"$WORK/release-key.pem"
openssl dgst -sha256 -verify "$WORK/release-key.pem" -signature "$WORK/SHA256SUMS.sig" "$WORK/SHA256SUMS" >/dev/null 2>&1 \
  || fail "the signature of the checksum list is wrong. The download was changed or is not a RackDesk release. Nothing was installed"

# The list is trusted now. The archive must be in it, with the same checksum.
expected="$(awk -v name="$ARCHIVE" '$2 == name || $2 == "*" name { print $1 }' "$WORK/SHA256SUMS")"
[ -n "$expected" ] || fail "the checksum list has no line for $ARCHIVE. Nothing was installed"
actual="$(sha256sum "$WORK/$ARCHIVE" | awk '{ print $1 }')"
[ "$expected" = "$actual" ] || fail "the checksum of $ARCHIVE is wrong. Nothing was installed"
ok

# ---------- Install ----------

step "Installing rackdeskd"
tar -xzf "$WORK/$ARCHIVE" -C "$WORK"
SRC="$WORK/rackdesk-linux-$ARCH"
[ -x "$SRC/bin/rackdeskd" ] || fail "the archive does not hold the program. Nothing was installed"

UPDATE=0
[ -x "$BIN_DIR/rackdeskd" ] && UPDATE=1

# The user that runs the server. A setting from an earlier install is kept.
OLD_USER=""
if [ -f "$ENV_FILE" ]; then
  OLD_USER="$(sed -n 's/^RACKDESK_USER=//p' "$ENV_FILE" | tail -n 1)"
fi
if [ -z "$RUN_USER" ]; then
  RUN_USER="${OLD_USER:-rackdesk}"
fi

mkdir -p "$ETC_DIR"
if [ "$RUN_USER" = "rackdesk" ] && ! id rackdesk >/dev/null 2>&1; then
  # A system user with no password, no shell and no home folder.
  if command -v useradd >/dev/null 2>&1; then
    useradd --system --user-group --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin rackdesk
  else
    adduser -S -D -H -h /nonexistent -s /sbin/nologin -G nogroup rackdesk
  fi
  # The uninstall command removes the user only if this file exists.
  : >"$ETC_DIR/created-user"
fi
RUN_GROUP="$(id -gn "$RUN_USER")"

install -d -m 0755 "$BIN_DIR" "$SBIN_DIR"
# Written under a temporary name and renamed, so a running program is
# replaced in 1 step.
for program in rackdeskd rackdesk-root; do
  install -m 0755 "$SRC/bin/$program" "$BIN_DIR/.$program.new"
  mv -f "$BIN_DIR/.$program.new" "$BIN_DIR/$program"
done
install -m 0755 "$SRC/uninstall.sh" "$SBIN_DIR/rackdesk-uninstall"

if [ ! -f "$ENV_FILE" ]; then
  install -m 0644 "$SRC/rackdesk.env" "$ENV_FILE"
fi

# set_env KEY VALUE: sets 1 setting in the settings file.
set_env() {
  if grep -q "^$1=" "$ENV_FILE"; then
    sed -i "s|^$1=.*|$1=$2|" "$ENV_FILE"
  else
    printf '%s=%s\n' "$1" "$2" >>"$ENV_FILE"
  fi
}

set_env RACKDESK_USER "$RUN_USER"
[ -z "$LISTEN" ] || set_env RACKDESK_LISTEN "$LISTEN"
if [ "$WITH_ROOT" -eq 1 ]; then
  set_env RACKDESK_ROOT_SOCKET /run/rackdesk/root.sock
fi
if [ -n "$SHELL_USER" ]; then
  set_env RACKDESK_SHELL_USER "$SHELL_USER"
  set_env RACKDESK_TERMINAL as-user
  [ "$ALLOW_ROOT_SHELL" -ne 1 ] || set_env RACKDESK_ALLOW_ROOT_SHELL true
fi

install -d -m 0700 -o "$RUN_USER" -g "$RUN_GROUP" "$DATA_DIR"
# After a change of user, the data must belong to the new one.
chown -R "$RUN_USER:$RUN_GROUP" "$DATA_DIR"

if [ "$HAVE_SYSTEMD" -eq 1 ] || [ -d "$UNIT_DIR" ]; then
  install -d -m 0755 "$UNIT_DIR"
  install -m 0644 "$SRC/systemd/rackdeskd.service" "$UNIT_DIR/rackdeskd.service"
  install -m 0644 "$SRC/systemd/rackdesk-root.service" "$UNIT_DIR/rackdesk-root.service"
  if [ "$RUN_USER" = "rackdesk" ]; then
    rm -f "$UNIT_DIR/rackdeskd.service.d/user.conf"
    rmdir "$UNIT_DIR/rackdeskd.service.d" 2>/dev/null || true
  else
    install -d -m 0755 "$UNIT_DIR/rackdeskd.service.d"
    printf '# Written by the RackDesk installer (--user).\n[Service]\nUser=%s\nGroup=%s\n' "$RUN_USER" "$RUN_GROUP" \
      >"$UNIT_DIR/rackdeskd.service.d/user.conf"
  fi
fi
ok

# ---------- The admin account ----------

# Run as the service user, so the account file belongs to it. The password
# goes to this terminal and to no log.
as_user() {
  if command -v runuser >/dev/null 2>&1; then
    runuser -u "$RUN_USER" -- "$@"
  else
    su -s /bin/sh "$RUN_USER" -c 'exec "$@"' -- sh "$@"
  fi
}
ACCOUNT="$(as_user "$BIN_DIR/rackdeskd" --data-dir "$DATA_DIR" init)" \
  || fail "could not create the admin account"

# ---------- Start ----------

SCHEME=http
ADDRESS="$(sed -n 's/^RACKDESK_LISTEN=//p' "$ENV_FILE" | tail -n 1)"
ADDRESS="${ADDRESS:-127.0.0.1:7171}"
case "$ADDRESS" in
  127.* | "[::1]"* | localhost*) ;;
  *) SCHEME=https ;;
esac

if [ "$HAVE_SYSTEMD" -eq 1 ]; then
  step "Starting the service"
  systemctl daemon-reload
  if grep -q '^RACKDESK_ROOT_SOCKET=' "$ENV_FILE"; then
    systemctl enable rackdesk-root >/dev/null 2>&1
    systemctl restart rackdesk-root
  fi
  systemctl enable rackdeskd >/dev/null 2>&1
  systemctl restart rackdeskd
  # The service is up when it answers. Wait up to 10 s.
  tries=0
  until systemctl is-active --quiet rackdeskd; do
    tries=$((tries + 1))
    [ "$tries" -lt 20 ] || fail "the service did not start. See: journalctl -u rackdeskd -n 50"
    sleep 0.5
  done
  ok
fi

case "$ACCOUNT" in
  *"password: "*) printf '%s\n\n' "$ACCOUNT" ;;
  *) printf '\n' ;;
esac

if [ "$HAVE_SYSTEMD" -eq 1 ]; then
  if [ "$UPDATE" -eq 1 ]; then
    printf 'RackDesk %s is running at %s://%s\n' "$("$BIN_DIR/rackdeskd" --version | awk '{ print $2 }')" "$SCHEME" "$ADDRESS"
  else
    printf 'RackDesk is running at %s://%s\n' "$SCHEME" "$ADDRESS"
  fi
else
  printf 'RackDesk is installed. This machine has no systemd, so no service was started.\n'
  printf 'Start it with:\n  %s\n' "runuser -u $RUN_USER -- $BIN_DIR/rackdeskd --data-dir $DATA_DIR"
fi

case "$ADDRESS" in
  127.* | "[::1]"* | localhost*)
    port="${ADDRESS##*:}"
    printf '\nIt listens on this machine only. To open it from your computer, run there:\n'
    printf '  ssh -L %s:127.0.0.1:%s <user>@<this-server>\n' "$port" "$port"
    printf 'and open http://127.0.0.1:%s in a browser.\n' "$port"
    ;;
  *)
    printf '\nThe server made its own HTTPS certificate, so the browser will warn once.\n'
    printf 'Compare the fingerprint the browser shows with the one in: journalctl -u rackdeskd\n'
    ;;
esac
printf '\nSettings: %s\nRemove RackDesk: sudo rackdesk-uninstall\n' "$ENV_FILE"
