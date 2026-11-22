<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/png/lockup-dark.png">
    <img src="assets/logo/png/lockup-light.png" alt="RackDesk" width="330">
  </picture>
</p>

# RackDesk

A desktop for your Linux server, in the browser. Install one small program, open it in a browser, sign in, and manage the server with windows and apps instead of SSH. The first apps are a file browser and a terminal. Later: services, logs, a system monitor, containers, an app store that adapts to what the server can run, and stock GUI apps streamed into windows.

- Light: one program with the UI inside it. Measured numbers are below.
- Secure by default: random first password, this machine only unless you choose another address, all file access confined (see `docs/security/SECURITY.md`).
- Access it however you like: SSH forward, VPN, reverse proxy, a tunnel or the local network.
- Open source, Apache-2.0. No account, no telemetry, no update check.

**Status:** version 1 is feature complete and not released yet: sign-in, sessions, HTTPS, the desktop, a full file browser with a text editor, zip archives, a permissions editor, an audit log with a view in Settings, a small root program, a terminal, an installer and packages. See `docs/ROADMAP.md` and `CHANGELOG.md`.

## Measured
Release build for aarch64, linked statically against musl. Measured with `scripts/measure.sh`.

| What | Value |
|---|---|
| Program size (the server with the UI inside it) | 8.1 MB |
| Download size of the release archive (`rackdeskd` and `rackdesk-root`) | 4.1 MB |
| Memory after start, nothing connected | 3.8 MB |
| Memory after a sign-in, a password change and 3 file listings, then 10 s with no request | 4.1 MB |

Memory is the resident set size. A sign-in needs about 19 MB more for under 1 second, for the password hash. Each open terminal adds its shell process. The release workflow measures both processor types and writes the numbers into the release notes.

## Try it without installing
Demo mode shows sample files in a temporary folder. It can't read or change the real files of the machine, and it has no terminal.

```sh
rackdeskd --demo          # from a release
make demo                 # from the source code
```
Open http://127.0.0.1:7171 and sign in with the printed password.

## Install
On a Linux server with systemd, x86_64 or aarch64:

```sh
curl -fsSL https://github.com/Niladri2003/rackdesk/releases/latest/download/install.sh | sudo sh
```

The installer checks the signature of the release before it installs anything. It creates the admin account, prints the password once, and starts the server on `127.0.0.1:7171`. To open it from your computer:

```sh
ssh -L 7171:127.0.0.1:7171 you@your-server
```
then open http://127.0.0.1:7171.

| Installer option | What it does |
|---|---|
| `--listen ADDRESS` | Listen on another address. Off 127.0.0.1 the server uses HTTPS |
| `--user NAME` | Run the server as this existing Linux user, so the file browser has that user's rights. Default: a new system user `rackdesk`, which can read what every user can read |
| `--with-root` | Enable `rackdesk-root` for a fixed list of root actions: file details, permissions, owner, reading a protected file |
| `--shell-user NAME` | Turn the terminal on. Its shells run as this Linux user |
| `--version VERSION` | Install a certain release, for example `v1.0.0` |

With options: `curl -fsSL .../install.sh | sudo sh -s -- --shell-user deploy`

**Update:** run the same command again. The settings and the data are kept.
**Remove:** `sudo rackdesk-uninstall`. It removes everything the installer added. `rackdesk-uninstall --list` shows the list first.
**Packages:** each release also has a `.deb` and a `.rpm` file. After installing one, run the 2 commands it prints.

Settings are in `/etc/rackdesk/rackdesk.env`. After a change: `sudo systemctl restart rackdeskd rackdesk-root`.

| Read | For |
|---|---|
| `docs/ROADMAP.md` | Versions and milestones |
| `docs/ARCHITECTURE.md` | How the pieces fit |
| `docs/security/SECURITY.md` | Threats and controls |
| `docs/DEV-SETUP.md` | Running it locally |
| `docs/design/CLAUDE-DESIGN-PROMPT.md` | The UI design brief |
| `SECURITY.md` | How to report a vulnerability |
| `CONTRIBUTING.md` | Setup, checks and rules for a change |
| `CHANGELOG.md` | What changed in each release |

## Settings
Each setting is a command flag or an environment variable. All have defaults.

| Flag | Environment variable | Default | Meaning |
|---|---|---|---|
| `--listen` | `RACKDESK_LISTEN` | `127.0.0.1:7171` | Address and port to listen on |
| `--data-dir` | `RACKDESK_DATA_DIR` | `./dev-data`. The installer sets `/var/lib/rackdesk` | Folder for the account file, the audit log and the certificate |
| `--tls` | `RACKDESK_TLS` | `auto` | `auto`: HTTPS unless the listen address is loopback. `on`: HTTPS always. `off`: plain HTTP always |
| `--tls-cert`, `--tls-key` | `RACKDESK_TLS_CERT`, `RACKDESK_TLS_KEY` | not set | Your own certificate and key (PEM). Without them the server makes a self-signed certificate |
| `--max-upload-bytes` | `RACKDESK_MAX_UPLOAD_BYTES` | 20 GiB | Largest file that can be uploaded |
| `--terminal` | `RACKDESK_TERMINAL` | `off` | Who starts the shell of the terminal. `off`: no terminal. `as-user`: `rackdesk-root` starts it, as the user it was configured with. `direct`: the server, as its own user |
| `--root-socket` | `RACKDESK_ROOT_SOCKET` | not set | The socket of `rackdesk-root`. Without it, nothing is done as root |
| `--behind-proxy` | `RACKDESK_BEHIND_PROXY` | off | Set it when a reverse proxy with HTTPS forwards requests to RackDesk |
| `--files-root` | `RACKDESK_FILES_ROOT` | `/` | The folder the file browser shows as `/`. Nothing above it can be read |

| `--demo` | - | off | Sample files in a temporary folder. No terminal, this machine only |

| Command | What it does |
|---|---|
| `rackdeskd init` | Creates the data folder and the admin account, and prints the password. Does nothing if the account exists |
| `rackdeskd reset-password` | Prints a new random admin password and ends every session |

Lost the admin password? On an installed server run:

```sh
sudo -u rackdesk rackdeskd --data-dir /var/lib/rackdesk reset-password
```

## Licence
Apache-2.0. See `LICENSE`.
