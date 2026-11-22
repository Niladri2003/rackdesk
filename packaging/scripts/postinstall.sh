#!/bin/sh
# Runs after the package is installed or updated.
#
# It creates the service user and the data folder. It does not create the
# admin account and does not start the service on a first install: the
# output of a package manager is kept in its log, and the admin password
# must not be written there.
set -e

if ! id rackdesk >/dev/null 2>&1; then
  useradd --system --user-group --no-create-home --home-dir /nonexistent --shell /usr/sbin/nologin rackdesk
fi
install -d -m 0700 -o rackdesk -g rackdesk /var/lib/rackdesk

if [ -d /run/systemd/system ]; then
  systemctl daemon-reload >/dev/null 2>&1 || true
  # An update: start the new program if the service was running.
  systemctl try-restart rackdeskd rackdesk-root >/dev/null 2>&1 || true
fi

if [ ! -f /var/lib/rackdesk/credentials.json ]; then
  cat <<'TEXT'

RackDesk is installed. 2 steps are left:

  1. Create the admin account. This prints the password once:
       sudo -u rackdesk rackdeskd --data-dir /var/lib/rackdesk init

  2. Start the server on 127.0.0.1:7171:
       sudo systemctl enable --now rackdeskd

Settings are in /etc/rackdesk/rackdesk.env.

TEXT
fi
