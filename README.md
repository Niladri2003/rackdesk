# RackDesk

A desktop for your Linux server, in the browser. Install one small program, open it in a browser, sign in, and manage the server with windows and apps instead of SSH. The first apps are a file browser and a terminal. Later: services, logs, a system monitor, containers, an app store that adapts to what the server can run, and stock GUI apps streamed into windows.

- Light: one binary, ~10-30 MB RAM idle.
- Secure by default: random first password, localhost unless you choose otherwise, all file access confined (see `docs/security/SECURITY.md`).
- Access it however you like: LAN, reverse proxy, SSH forward, VPN or a tunnel.

**Status:** M0 (skeleton). See `docs/ROADMAP.md`.

| Read | For |
|---|---|
| `docs/ROADMAP.md` | Versions and milestones |
| `docs/ARCHITECTURE.md` | How the pieces fit |
| `docs/security/SECURITY.md` | Threats and controls |
| `docs/design/CLAUDE-DESIGN-PROMPT.md` | The UI design brief |
| `docs/DEV-SETUP.md` | Running it locally |

## Licence
Apache-2.0. See `LICENSE`.
