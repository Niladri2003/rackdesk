# Roadmap

Webdesk is one small program you install on a Linux server. You open it in a browser and get a desktop: windows, a taskbar, an app launcher and apps. The first app is a file browser. It starts light (it runs on a 512 MB VPS), and the app store later adapts to what the server can handle.

How users reach the port is their choice (LAN, their own reverse proxy, SSH forward, VPN or a tunnel). Webdesk doesn't depend on any of these.

## Versions at a glance
| Version | What ships | Extra resources |
|---|---|---|
| **v1** | Desktop shell, sign-in, file browser | ~10–30 MB RAM |
| v2 | Terminal, system monitor, terminal apps (`btop`, `lazygit`…), local web UIs proxied into windows | ~0 (programs start when opened) |
| v3 | Adaptive app store: detect CPU/RAM/GPU/Docker, show only apps that fit, install web apps | Per app |
| v4 | Stock GUI apps (Flatpak/apt) streamed into windows. Xpra prototype first, then our own headless Wayland compositor (Smithay) with WebRTC and GPU encoding | Per app, on demand |
| Later | Multiple users and roles, several servers in one desktop, sandboxed apps (reusing `~/projects/sandbox`) | — |

## v1 milestones
Each milestone ends with tests passing, the docs updated and a journal entry in `docs/journal/`.

### M0: Skeleton ✅
- Rust workspace (`wd-api`, `wd-fs`, `wd-auth`, `webdeskd`) and React + TypeScript + Vite in `web/`.
- Rust types are exported to TypeScript with ts-rs.
- **Random initial admin password** (no default password), stored as an argon2id hash in a 0600 file.
- `wd-fs::Root`, confined to a root folder with cap-std. Tests cover `..` and symlink escapes.
- `/healthz`, `/api/v1/server`, the UI embedded in the binary, baseline security headers.
- CI, docs, ADRs.
- **Done when:** `cargo test` and `npm run build` pass, and the server serves the UI. ✅

### M1: Sign-in and sessions
- `POST /api/v1/session` (sign in), `DELETE` (sign out), `GET` (who am I).
- Session cookie: random 256-bit id, stored server-side, `HttpOnly`, `Secure`, `SameSite=Strict`. Idle timeout of 12 h, absolute timeout of 7 days.
- Forced password change while `must_change_password` is set. Every API call except change-password returns `403 password_change_required`.
- CSRF protection: SameSite plus a check of `Origin`/`Sec-Fetch-Site` on every state-changing request.
- Rate limit and lockout: per IP and per account, with exponential backoff.
- HTTPS: a self-signed certificate is generated on first start (rustls). Optional ACME later. Plain HTTP only on loopback.
- `webdeskd reset-password`: a CLI command run as root on the server, for recovery.
- **Done when:** integration tests cover sign-in, sign-out, the forced change, lockout and CSRF rejection.

### M2: Desktop shell (from the Claude Design handoff)
- Design tokens and the component library in `web/`.
- Window manager: open, focus, move, resize, minimise, maximise, snap, z-order, keyboard switcher. Window state is kept per user.
- Generic window content: `builtin` now, with `web` and `stream` stubs (see ARCHITECTURE).
- Taskbar, launcher with search, notifications, settings (theme, accent, wallpaper, change password).
- Responsive: windows on desktop, maximised on tablet, one app at a time on phone.
- Reconnecting banner.
- **Done when:** Playwright tests open, move and close windows. axe accessibility checks pass in both themes.

### M3: File browser (read)
- `GET /api/v1/fs/list`: paged and sorted. Virtualised list and grid in the UI, so 100k entries scroll smoothly.
- Sidebar (home, root, favourites, disks with usage), breadcrumbs, search in the current folder.
- Download with HTTP Range (resume, video seeking), and folder download as a streamed zip.
- Preview of images, video, audio, PDF, and text with syntax highlighting.
- **Done when:** a 100k-file folder lists in under 1 s on a small VPS, and the escape tests pass.

### M4: File browser (write)
- Upload: chunked and resumable (tus-style protocol), with progress, pause, retry and cancel. Chunks are written to a temp file, checked, then renamed atomically.
- New folder, rename, move, copy (with progress), duplicate, conflict handling (replace, keep both, skip).
- Trash, following the freedesktop spec, plus restore and permanent delete.
- Text editor with save, and a check that the file hasn't changed on disk since it was opened.
- **Done when:** fuzzed path inputs and upload edge cases (disk full, cancel, duplicate chunk) are covered.

### M5: Privilege split and system features
- `webdeskd` runs as the unprivileged `webdesk` user.
- A small `webdesk-helper` (root, unix socket, `SO_PEERCRED`) does only a fixed list of actions: chmod/chown, reading files the `webdesk` user can't read when the admin asks to, and later services.
- Permissions editor, zip and unzip (Zip Slip checks, size and entry-count limits).
- Audit log of every write and every privileged action.
- **Done when:** the helper's protocol is fuzzed, and the agent can't act outside the helper's allowlist.

### M6: Release
- Install script and a systemd unit with hardening (`ProtectSystem`, `NoNewPrivileges` for the agent).
- `.deb`/`.rpm` and a static musl binary. Signed releases with checksums.
- Upgrade path (data dir migrations).
- Measured resource use published: RAM when idle and binary size.
- Security review pass against `docs/security/SECURITY.md`.

## Working rules
- **Server-side checks are the security.** The UI hiding a button is never a control.
- Every file operation goes through `wd-fs::Root`. No `std::fs` with paths from requests anywhere else.
- A new endpoint needs an auth test, plus a CSRF test if it changes state.
- Keep the agent light: nothing runs in the background that nobody is looking at.
