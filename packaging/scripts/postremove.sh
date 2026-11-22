#!/bin/sh
# Runs after the package is removed.
#
# "apt remove" keeps the data and the settings. "apt purge" deletes them and
# the service user. rpm has no purge: after "dnf remove", delete
# /var/lib/rackdesk and the user "rackdesk" by hand if you want them gone.
set -e

if [ -d /run/systemd/system ]; then
  systemctl daemon-reload >/dev/null 2>&1 || true
fi

if [ "$1" = "purge" ]; then
  rm -rf /var/lib/rackdesk /etc/rackdesk /run/rackdesk
  if id rackdesk >/dev/null 2>&1; then
    userdel rackdesk >/dev/null 2>&1 || true
  fi
fi
