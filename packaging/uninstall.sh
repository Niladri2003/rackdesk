#!/bin/sh
# Removes everything the RackDesk installer added to this machine.
#
#   sudo rackdesk-uninstall              asks before it deletes the data
#   sudo rackdesk-uninstall --yes        deletes the data without a question
#   sudo rackdesk-uninstall --keep-data  keeps /var/lib/rackdesk
#   rackdesk-uninstall --list            prints what would be removed
#
# The data folder holds the admin account, the audit log and the HTTPS
# certificate. Files that you changed through RackDesk are not touched.
set -eu

BIN_DIR=/usr/local/bin
SBIN_DIR=/usr/local/sbin
UNIT_DIR=/etc/systemd/system
ETC_DIR=/etc/rackdesk
DATA_DIR=/var/lib/rackdesk
RUN_DIR=/run/rackdesk

ASSUME_YES=0
KEEP_DATA=0
LIST_ONLY=0

while [ $# -gt 0 ]; do
  case "$1" in
    --yes | -y) ASSUME_YES=1 ;;
    --keep-data) KEEP_DATA=1 ;;
    --list) LIST_ONLY=1 ;;
    -h | --help) sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) printf 'Error: unknown option: %s\n' "$1" >&2; exit 1 ;;
  esac
  shift
done

# Every path the installer writes. Keep this list the same as install.sh.
paths() {
  printf '%s\n' \
    "$BIN_DIR/rackdeskd" \
    "$BIN_DIR/rackdesk-root" \
    "$SBIN_DIR/rackdesk-uninstall" \
    "$UNIT_DIR/rackdeskd.service" \
    "$UNIT_DIR/rackdesk-root.service" \
    "$UNIT_DIR/rackdeskd.service.d/user.conf" \
    "$ETC_DIR" \
    "$RUN_DIR"
}

if [ "$LIST_ONLY" -eq 1 ]; then
  paths
  printf '%s  (the data: account, audit log, certificate)\n' "$DATA_DIR"
  if [ -f "$ETC_DIR/created-user" ]; then
    printf 'the Linux user and group "rackdesk"\n'
  fi
  exit 0
fi

if [ "$(id -u)" -ne 0 ]; then
  printf 'Error: run this as root: sudo rackdesk-uninstall\n' >&2
  exit 1
fi

if [ "$KEEP_DATA" -eq 0 ] && [ "$ASSUME_YES" -eq 0 ] && [ -d "$DATA_DIR" ]; then
  if [ ! -t 0 ]; then
    printf 'Error: %s holds the admin account and the audit log.\n' "$DATA_DIR" >&2
    printf 'Say what to do with it: add --yes to delete it, or --keep-data to keep it.\n' >&2
    exit 1
  fi
  printf 'Delete %s (the admin account, the audit log, the certificate)? [y/N] ' "$DATA_DIR"
  read -r answer
  case "$answer" in
    y | Y | yes | YES) ;;
    *) KEEP_DATA=1 ;;
  esac
fi

if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
  systemctl disable --now rackdeskd rackdesk-root >/dev/null 2>&1 || true
fi

# The user is removed only if the installer created it.
REMOVE_USER=0
[ -f "$ETC_DIR/created-user" ] && REMOVE_USER=1

rm -f "$BIN_DIR/rackdeskd" "$BIN_DIR/rackdesk-root"
rm -f "$UNIT_DIR/rackdeskd.service" "$UNIT_DIR/rackdesk-root.service"
rm -f "$UNIT_DIR/rackdeskd.service.d/user.conf"
rmdir "$UNIT_DIR/rackdeskd.service.d" 2>/dev/null || true
rm -rf "$ETC_DIR" "$RUN_DIR"

if command -v systemctl >/dev/null 2>&1 && [ -d /run/systemd/system ]; then
  systemctl daemon-reload
  systemctl reset-failed rackdeskd rackdesk-root >/dev/null 2>&1 || true
fi

if [ "$KEEP_DATA" -eq 1 ]; then
  printf 'Kept %s. Delete it with: sudo rm -rf %s\n' "$DATA_DIR" "$DATA_DIR"
else
  rm -rf "$DATA_DIR"
fi

if [ "$REMOVE_USER" -eq 1 ] && id rackdesk >/dev/null 2>&1; then
  if [ "$KEEP_DATA" -eq 1 ]; then
    printf 'Kept the Linux user "rackdesk", because it owns %s.\n' "$DATA_DIR"
  elif command -v userdel >/dev/null 2>&1; then
    userdel rackdesk >/dev/null 2>&1 || true
    # On some systems the group stays after the user is gone.
    if command -v groupdel >/dev/null 2>&1; then
      groupdel rackdesk >/dev/null 2>&1 || true
    fi
  else
    deluser rackdesk >/dev/null 2>&1 || true
  fi
fi

# This script deletes itself last.
rm -f "$SBIN_DIR/rackdesk-uninstall"
printf 'RackDesk is removed.\n'
