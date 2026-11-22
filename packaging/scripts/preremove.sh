#!/bin/sh
# Runs before the package is removed. On an update it does nothing: the
# service keeps running and is restarted by the new package.
set -e

case "$1" in
  remove | purge | 0)
    if [ -d /run/systemd/system ]; then
      systemctl disable --now rackdeskd rackdesk-root >/dev/null 2>&1 || true
    fi
    ;;
esac
