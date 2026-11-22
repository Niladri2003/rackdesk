# Architecture

## Components
| Part | Language | Role |
|---|---|---|
| `crates/rd-api` | Rust | The API contract. Types are exported to `web/src/api/generated/*.ts` by ts-rs |
| `crates/rd-fs` | Rust | All file access, confined beneath one root (cap-std) |
| `crates/rd-auth` | Rust | Accounts: initial password, argon2id, credential store, sessions in memory, limit on password guessing |
| `crates/rackdeskd` | Rust (tokio + axum) | The server: HTTP API, WebSockets (terminal from M6), embedded UI |
| `crates/rd-helper-proto` | Rust | The messages between the server and the helper, and their framing |
| `crates/rackdesk-helper` | Rust (no async) | Small root program for a fixed list of actions: stat, chmod, chown, read. From M6 it also starts the terminal's shell as a configured Linux user |
| `web/` | React + TypeScript + Vite | The desktop shell and the built-in apps |

## Request flow
```text
Browser: React desktop shell (windows, taskbar) + apps (Files, Settings…)
   │  HTTPS: REST (JSON) for actions; WebSocket for live data (terminal in M6, logs and metrics in v1.5)
   ▼
rackdeskd (user "rackdesk", unprivileged from M5)
   ├── CSRF check on every state-changing request
   ├── auth: session cookie -> account. Every API route outside `/session` needs a session
   ├── rd-fs::Root ──► files beneath the configured root
   ├── app runner (v2+) ──► terminal programs, local web UIs, GUI apps
   │  unix socket, SO_PEERCRED (M5)
   ▼
rackdesk-helper (root): chmod/chown, privileged reads, services. An allowlist, nothing else
```

## UI: windows are generic
Every window has the same chrome and holds one type of content:

| Content | Holds | Version |
|---|---|---|
| `builtin` | A React component from the bundle (Files, Settings, Terminal) | v1 |
| `web` | An iframe showing a local web app, reverse-proxied by rackdeskd on its own subdomain or port | v2 |
| `stream` | A remote GUI window streamed over WebRTC (video plus input) | v4 |

So stock apps later need a new content type and a runner, not a redesign.

## UI: where state lives
| State | Kept in | Why |
|---|---|---|
| Session | An `HttpOnly` cookie and the server's memory | JavaScript never sees it |
| Favourite folders | `localStorage`, 1 entry per account | Small, and not secret |
| Window layout | `localStorage`, 1 entry per account | Window positions depend on the screen, so they belong to the browser |
| Theme, accent, wallpaper | `localStorage` | The sign-in page uses them before anyone is signed in |
| Everything else | The server, read when a window opens | Nothing polls in the background |

Data read from `localStorage` is treated as untrusted: every field is checked and unknown values are dropped.

## Apps are generic too
`rd_api::AppManifest` = id, name, icon, `AppKind` (`builtin` | `tui` | `web` | `gui`) and `Requirements` (RAM, CPUs, GPU). In v3, the server reports its capabilities, and the store compares them with each app's requirements.

The app runner (v2) is one supervisor. It starts an app when its window opens and stops it when the window closes, with cgroup limits per app. It's built so it can later use the namespace and cgroup work from `~/projects/sandbox`.

## Data layout (production)
```text
/usr/bin/rackdeskd                    the binary, with the UI embedded
/var/lib/rackdesk/credentials.json    0600, argon2id hashes only
/var/lib/rackdesk/tls/                the self-signed certificate (cert.pem) and its key (key.pem, 0600)
<target folder>/.rd-upload-*.part     an upload in progress. In the target folder, so the final rename is on 1 disk
~/.local/share/Trash/                 the trash (freedesktop layout). `<files root>/.Trash-<uid>` if the home folder is outside the files root
/var/lib/rackdesk/audit.log           0600, 1 JSON line per change
/usr/bin/rackdesk-helper              the root helper
/run/rackdesk/helper.sock             0600, owned by the rackdesk user. The helper listens here
/etc/rackdesk/config.toml             M7
```

## Users and rights (production)
| Program | Runs as | Started with |
|---|---|---|
| `rackdeskd` | `rackdesk`, a user with no special rights | `--helper-socket /run/rackdesk/helper.sock` |
| `rackdesk-helper` | `root` | `--socket /run/rackdesk/helper.sock --allow-uid <id of rackdesk> --files-root <folder> --shell-user <user>` |
| A terminal shell | The user from `--shell-user` | Started by the helper with `runuser -l`, when the server runs with `--terminal helper` |

- The helper is optional. Without `--helper-socket`, the server does nothing as root, and a change that the system refuses stays refused.
- The 2 programs can have different files roots. The helper's root should be the smallest folder that the admin needs.
- The systemd units, the user account and the install script are M7.

## Choices (see docs/adr)
- 0001: Rust (axum) for the server, React + TypeScript for the UI, and one binary.
- 0002: cap-std for all file access.
- 0003: random initial password, never a default one.
- 0004: sessions are kept in memory, not on disk.
- 0005: plain CSS with design tokens, no CSS framework.

## Keeping it light
- One process at idle. The UI is static files served from memory.
- Metrics are sampled only while a window is subscribed to them.
- Apps run only while their window is open (v2+).
- The frontend's size affects only the first download (assets are cached immutably by content hash), not server RAM.
