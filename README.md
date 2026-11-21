<p>
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/logo/png/lockup-dark.png">
    <img src="assets/logo/png/lockup-light.png" alt="RackDesk" width="330">
  </picture>
</p>

# RackDesk

A desktop for your Linux server, in the browser. Install one small program, open it in a browser, sign in, and manage the server with windows and apps instead of SSH. The first apps are a file browser and a terminal. Later: services, logs, a system monitor, containers, an app store that adapts to what the server can run, and stock GUI apps streamed into windows.

- Light: one binary, ~10-30 MB RAM idle.
- Secure by default: random first password, localhost unless you choose otherwise, all file access confined (see `docs/security/SECURITY.md`).
- Access it however you like: LAN, reverse proxy, SSH forward, VPN or a tunnel.

**Status:** M6 is done: sign-in, sessions, HTTPS, the desktop, a full file browser with a text editor, zip archives, a permissions editor, an audit log with a view in Settings, a small root helper, and a terminal. The release work (M7) is next. See `docs/ROADMAP.md`.

| Read | For |
|---|---|
| `docs/ROADMAP.md` | Versions and milestones |
| `docs/ARCHITECTURE.md` | How the pieces fit |
| `docs/security/SECURITY.md` | Threats and controls |
| `docs/design/CLAUDE-DESIGN-PROMPT.md` | The UI design brief |
| `docs/DEV-SETUP.md` | Running it locally |

## Settings
Each setting is a command flag or an environment variable. All have defaults.

| Flag | Environment variable | Default | Meaning |
|---|---|---|---|
| `--listen` | `RACKDESK_LISTEN` | `127.0.0.1:7171` | Address and port to listen on |
| `--data-dir` | `RACKDESK_DATA_DIR` | `./dev-data` | Folder for the account file and the certificate |
| `--tls` | `RACKDESK_TLS` | `auto` | `auto`: HTTPS unless the listen address is loopback. `on`: HTTPS always. `off`: plain HTTP always |
| `--tls-cert`, `--tls-key` | `RACKDESK_TLS_CERT`, `RACKDESK_TLS_KEY` | not set | Your own certificate and key (PEM). Without them the server makes a self-signed certificate |
| `--max-upload-bytes` | `RACKDESK_MAX_UPLOAD_BYTES` | 20 GiB | Largest file that can be uploaded |
| `--terminal` | `RACKDESK_TERMINAL` | `off` | Who starts the shell of the terminal. `off`: no terminal. `helper`: `rackdesk-helper`, as the user it was started with. `direct`: the server, as its own user |
| `--helper-socket` | `RACKDESK_HELPER_SOCKET` | not set | The socket of `rackdesk-helper`. Without it, nothing is done as root |
| `--behind-proxy` | `RACKDESK_BEHIND_PROXY` | off | Set it when a reverse proxy with HTTPS forwards requests to RackDesk |
| `--files-root` | `RACKDESK_FILES_ROOT` | `/` | The folder the file browser shows as `/`. Nothing above it can be read |

Lost the admin password? Run `rackdeskd reset-password` on the server. It prints a new random password.

## Licence
Apache-2.0. See `LICENSE`.
