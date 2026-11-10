# Architecture

## Components
| Part | Language | Role |
|---|---|---|
| `crates/wd-api` | Rust | The API contract. Types are exported to `web/src/api/generated/*.ts` by ts-rs |
| `crates/wd-fs` | Rust | All file access, confined beneath one root (cap-std) |
| `crates/wd-auth` | Rust | Accounts: initial password, argon2id, credential store (sessions from M1) |
| `crates/webdeskd` | Rust (tokio + axum) | The server: HTTP API, WebSockets (v2), embedded UI |
| `webdesk-helper` (M5) | Rust | Small root helper for a fixed list of privileged actions |
| `web/` | React + TypeScript + Vite | The desktop shell and the built-in apps |

## Request flow
```text
Browser: React desktop shell (windows, taskbar) + apps (Files, Settings…)
   │  HTTPS: REST (JSON) for actions; WebSocket for live data (v2: terminal, metrics)
   ▼
webdeskd (user "webdesk", unprivileged from M5)
   ├── auth: session cookie → account (M1)
   ├── wd-fs::Root ──► files beneath the configured root
   ├── app runner (v2+) ──► terminal programs, local web UIs, GUI apps
   │  unix socket, SO_PEERCRED (M5)
   ▼
webdesk-helper (root): chmod/chown, privileged reads, services. An allowlist, nothing else
```

## UI: windows are generic
Every window has the same chrome and holds one type of content:

| Content | Holds | Version |
|---|---|---|
| `builtin` | A React component from the bundle (Files, Settings, Terminal) | v1 |
| `web` | An iframe showing a local web app, reverse-proxied by webdeskd on its own subdomain or port | v2 |
| `stream` | A remote GUI window streamed over WebRTC (video plus input) | v4 |

So stock apps later need a new content type and a runner, not a redesign.

## Apps are generic too
`wd_api::AppManifest` = id, name, icon, `AppKind` (`builtin` | `tui` | `web` | `gui`) and `Requirements` (RAM, CPUs, GPU). In v3, the server reports its capabilities, and the store compares them with each app's requirements.

The app runner (v2) is one supervisor. It starts an app when its window opens and stops it when the window closes, with cgroup limits per app. It's built so it can later use the namespace and cgroup work from `~/projects/sandbox`.

## Data layout (production)
```text
/usr/bin/webdeskd                    the binary, with the UI embedded
/var/lib/webdesk/credentials.json    0600, argon2id hashes only
/var/lib/webdesk/sessions/           M1
/var/lib/webdesk/uploads/            M4: temp files for in-progress uploads, same filesystem as targets where possible
/var/lib/webdesk/audit.log           M5
/etc/webdesk/config.toml             M6
```

## Choices (see docs/adr)
- 0001: Rust (axum) for the server, React + TypeScript for the UI, and one binary.
- 0002: cap-std for all file access.
- 0003: random initial password, never a default one.

## Keeping it light
- One process at idle. The UI is static files served from memory.
- Metrics are sampled only while a window is subscribed to them.
- Apps run only while their window is open (v2+).
- The frontend's size affects only the first download (assets are cached immutably by content hash), not server RAM.
