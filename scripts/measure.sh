#!/usr/bin/env bash
# Prints the size of the rackdeskd program and the memory it uses.
#
#   scripts/measure.sh path/to/rackdeskd
#
# It starts the program with an empty data folder and reads the memory from
# /proc at 2 moments. Linux only. Needs curl.
#
#   after start:  5 s after the first answer, with nothing connected
#   after use:    after a sign-in, a password change, 3 file listings and
#                 10 s with no request. This is the idle memory of a server
#                 that has been used.
#
# The number is the resident set size: the RAM the process holds.
set -euo pipefail

BINARY="${1:?usage: measure.sh path/to/rackdeskd}"
PORT=17171
URL="http://127.0.0.1:$PORT"
WORK="$(mktemp -d)"
trap 'kill "$PID" 2>/dev/null || true; rm -rf "$WORK"' EXIT
mkdir -p "$WORK/files/etc" "$WORK/files/var/log"
for n in $(seq 1 200); do : >"$WORK/files/var/log/file-$n.log"; done

"$BINARY" --data-dir "$WORK/data" --files-root "$WORK/files" --listen "127.0.0.1:$PORT" >"$WORK/out" 2>/dev/null &
PID=$!
for _ in $(seq 1 50); do
  curl -fsS "$URL/healthz" >/dev/null 2>&1 && break
  sleep 0.1
done

rss() {
  awk '/^VmRSS:/ { printf "%d.%d", $2 / 1024, ($2 % 1024) * 10 / 1024 }' "/proc/$PID/status"
}

sleep 5
after_start="$(rss)"

password="$(sed -n 's/^ *password: //p' "$WORK/out")"
new_password="a phrase used for measuring only"
post() {
  curl -fsS -o /dev/null -b "$WORK/cookies" -c "$WORK/cookies" \
    -H "Origin: $URL" -H 'Content-Type: application/json' -d "$2" "$URL$1"
}
post /api/v1/session "{\"username\":\"admin\",\"password\":\"$password\"}"
post /api/v1/session/password "{\"current_password\":\"$password\",\"new_password\":\"$new_password\"}"
for path in / /var/log /etc; do
  curl -fsS -o /dev/null -b "$WORK/cookies" "$URL/api/v1/fs/list?path=$path"
done
sleep 10
after_use="$(rss)"

size_bytes="$(wc -c <"$BINARY")"
printf '%s (%s)\n' "$("$BINARY" --version)" "$(uname -m)"
printf 'program size:        %d.%d MB\n' $((size_bytes / 1000000)) $(((size_bytes % 1000000) / 100000))
printf 'memory after start:  %s MB\n' "$after_start"
printf 'memory after use:    %s MB\n' "$after_use"
printf 'threads:             %s\n' "$(awk '/^Threads:/ { print $2 }' "/proc/$PID/status")"
