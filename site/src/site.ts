// Values that more than 1 file of the site uses.

export const REPO_URL = 'https://github.com/Niladri2003/rackdesk'
// The installer needs root: it adds a system user and a systemd unit.
export const INSTALL_COMMAND = `curl -fsSL ${REPO_URL}/releases/latest/download/install.sh | sudo sh`
