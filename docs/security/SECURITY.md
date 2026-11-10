# Security plan

Webdesk can read and write files on a server, and later run programs. Treat it as **remote root access with a web UI**: every bug here is a server compromise. The rule is "secure defaults, convenience later", not the other way round.

## What we protect
1. The server: files, credentials, and the processes it runs.
2. The admin's session: no one else can act as them.
3. Other people's data on the server (v1 has one account; later, multiple users).

## Who attacks
| Attacker | Example |
|---|---|
| Internet scanners | Find the port, try default passwords and known panel exploits within minutes |
| A malicious website the admin visits | CSRF, or trying to frame the UI (clickjacking) |
| A malicious file on the server | An uploaded `.html` or `.svg` that runs script in Webdesk's origin; an archive that writes outside its folder |
| Someone on the same network | Sniffing plain HTTP |
| A compromised app (v2+) | Trying to reach the agent's API or the helper |

## Controls
| # | Threat | Control | Milestone | Test |
|---|---|---|---|---|
| A1 | Guessable first password | **No default password.** A random 20-character password (~116 bits) is generated on first start, printed once to the terminal (never logged), and stored as argon2id in a 0600 file | M0 ✅ | `wd-auth` tests |
| A2 | Password guessing | Rate limit per IP and account, exponential backoff, lockout. argon2id makes offline cracking slow | M1 | integration |
| A3 | Initial password left in place | Forced change before any other API call works. Minimum 12 characters, checked against a common-password list | M1 | integration |
| A4 | Session theft | 256-bit random session id, stored server-side; `HttpOnly; Secure; SameSite=Strict`; idle and absolute timeouts; rotated at sign-in; revoked on password change | M1 | integration |
| A5 | CSRF | `SameSite=Strict`, plus rejection of state-changing requests whose `Origin` or `Sec-Fetch-Site` isn't our own | M1 | integration |
| A6 | Locked out admin | `webdeskd reset-password`, run as root on the server. No email reset and no backdoor | M1 | manual |
| T1 | Sniffing | HTTPS by default (a self-signed certificate is generated, or the user's own). Plain HTTP only on loopback; the server warns otherwise | M0 warning ✅ / M1 TLS | — |
| T2 | Exposed by accident | Listens on `127.0.0.1` by default. Binding to a public address is an explicit flag | M0 ✅ | — |
| F1 | Path traversal (`../../etc/shadow`) and symlink escapes | All access goes through `wd-fs::Root` (cap-std, `openat2 RESOLVE_BENEATH` on Linux). `clean_path` rejects `..`, NUL and backslashes | M0 ✅ | `wd-fs` escape tests, fuzzing in M4 |
| F2 | Stored XSS from user files | User files are served with `Content-Disposition: attachment` by default. Previews use a sandboxed separate origin or render as text, never inline HTML/SVG in our origin. `X-Content-Type-Options: nosniff` | M3 | integration |
| F3 | Corrupt or partial writes | Upload to a temp file, check size and hash, `fsync`, then rename atomically | M4 | integration |
| F4 | Zip Slip, zip bombs | Check every entry path through `Root`; limits on total size, entry count and compression ratio | M5 | fuzz |
| F5 | Disk filled by uploads | Per-upload and total size limits, free-space check, cleanup of abandoned uploads | M4 | integration |
| P1 | Agent compromise = root | The agent runs as unprivileged `webdesk`. Root actions go to `webdesk-helper` over a unix socket (`SO_PEERCRED`) with a fixed allowlist and audit log | M5 | fuzz the helper protocol |
| P2 | Unaccountable changes | An append-only audit log of writes and privileged actions | M5 | — |
| H1 | Clickjacking and script injection | `frame-ancestors 'none'`, `X-Frame-Options: DENY`, a strict CSP (`default-src 'self'`, no inline scripts), `Referrer-Policy: no-referrer` | M0 ✅ (tightened in M2) | header test |
| H2 | Request floods | Body size limits, upload chunk limits, a concurrency limit per session | M1/M4 | — |
| S1 | Malicious dependency | `cargo deny` and `npm audit` in CI, a reviewed lockfile, few dependencies, no install scripts in production builds | M0 ✅ | CI |
| S2 | Tampered releases | Signed releases and checksums; the installer verifies them | M6 | — |
| W1 | Apps attacking the agent (v2+) | Each app runs as its own user with cgroup limits. Proxied web apps get their own origin (subdomain or port), never ours. The agent's API rejects requests coming from app origins | v2 | integration |

## Rules for contributors
- Never build a filesystem path from request data outside `wd-fs`.
- Never log secrets, passwords or session ids. The initial password is printed with `println!`, not through `tracing`.
- Every new endpoint: an auth check, input limits, a test for unauthenticated access, and a CSRF test if it changes state.
- `unsafe` is denied workspace-wide. If it's ever needed, it gets an ADR and a `// SAFETY:` comment.

## Reporting
See `SECURITY.md` at the repo root when the project is published (M6): a private contact, a 90-day disclosure window, and an advisory for every fix.
