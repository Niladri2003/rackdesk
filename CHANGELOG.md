# Changelog

Every release is listed here, newest first. A release note says what changed for a person who runs RackDesk, not how the code changed.

Version numbers follow `MAJOR.MINOR.PATCH`. A change that needs an action from the admin is marked **Action needed**.

## Unreleased

The first public release.

### Added
- **Desktop in the browser.** Windows that move, resize, minimise and maximise, a taskbar, and search with Ctrl K. Light and dark themes.
- **File browser.** List, upload (in parts, with resume), download, copy, move, rename, delete to a trash, and restore. Zip archives: create, extract and download a folder as zip. Preview of pictures, video, audio and PDF.
- **Text editor** with syntax colours. A save never overwrites a change made by someone else.
- **Terminal.** A shell on the server. A reload of the page returns to the same shell. Off by default.
- **Permissions editor.** Change the mode and the owner of a file.
- **Activity view.** Every change, who made it, from which address, and if it was refused.
- **Root program** (`rackdesk-root`). An optional second program that does a fixed list of actions as root, so the server itself runs with no special rights.
- **Installer.** `install.sh` checks the signature of the release before it installs anything. `rackdesk-uninstall` removes everything the installer added.
- **Packages.** `.deb` and `.rpm` files, and a static program for x86_64 and aarch64.
- **Demo mode.** `rackdeskd --demo` runs with sample files and no access to the real files of the machine.
- **`rackdeskd init`** creates the admin account and prints the password, without starting the server.

### Security
- No default password. A random password of 20 characters is made at the first start and must be changed at the first sign-in.
- The server listens on 127.0.0.1 unless another address is set. On any other address it uses HTTPS.
- Failed sign-ins lock the address and the account for a growing time.
- All file access is confined beneath 1 folder. No path and no symlink leads outside it.
- When systemd starts the server, the first password is not printed, because that output is kept in the system journal.
- RackDesk sends no data to anyone. There is no telemetry and no update check.
