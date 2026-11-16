# Rust guide

Read the root `CLAUDE.md` first. This file covers how the Rust crates are written.

## Which crate does what
Dependencies point one way. `rackdeskd` depends on `rd-fs` and `rd-auth`. Those depend on `rd-api`. Library crates never depend on `rackdeskd`, axum or tokio.

| Crate | Put here | Never put here |
|---|---|---|
| `rd-api` | Request and response types, enums, `ApiError` | I/O, logic, anything not sent over the wire |
| `rd-fs` | Every operation on user files, as a method on `Root` | HTTP types, auth |
| `rd-auth` | Passwords, the credential store, sessions (M1), rate limiting (M1) | HTTP types, file browsing |
| `rackdeskd` | Routes, extractors, middleware, start-up, config | Path handling, password handling |

A new area of work (terminal, services, helper protocol) gets its own `rd-*` crate, so it can be tested without the server.

## Adding or changing an API type
1. Define it in `crates/rd-api/src/lib.rs` with `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]` and `#[ts(export)]`.
2. Enums use `#[serde(rename_all = "snake_case")]`.
3. `u64` and `i64` fields need `#[ts(type = "number")]` (or `"number | null"`). Without it, ts-rs emits `bigint`, which doesn't match JSON.
4. Run `cargo test -p rd-api` and commit the new files in `web/src/api/generated/`. CI fails if they're stale.
5. Give every field whose meaning isn't obvious a doc comment with its unit (bytes, Unix seconds).

## Errors
- Library crates define their own error enum with `thiserror`. `rackdeskd` uses `anyhow` for start-up only.
- Every error that reaches the client is an `ApiError { code, message }`. `code` is snake_case and stable (`not_found`, `permission_denied`, `unauthorized`); the UI matches on it. Add a `code()` method to the crate's error enum, as `FsError` does.
- Messages sent to the client never contain absolute server paths, hashes or internal detail.
- No `unwrap()` or `expect()` outside tests, except where a comment explains why it can't fail.

## File access
- A new file operation is a new method on `rd_fs::Root`. It takes the UI path as `&str` and calls `clean_path` first.
- Use the `cap_std` handle (`self.dir`), never `std::fs` with a joined path. A string check like `starts_with(root)` is not a control.
- Describe symlinks, don't follow them, unless the operation is meant to and stays beneath the root.
- Each new method gets an escape test: `..`, an absolute path, a symlink pointing outside, and a symlink to `/`. See the tests at the bottom of `crates/rd-fs/src/lib.rs`.
- Writes go to a temp file on the same filesystem, then `fsync`, then an atomic rename.

## Endpoints
- Routes are `/api/v1/...`. Unknown `/api/*` paths return 404, never the UI.
- Handlers are thin: extract, check the session, call a library crate, map the error to `ApiError`.
- Every state-changing route checks `Origin` or `Sec-Fetch-Site` (M1). WebSocket upgrades check them too.
- Put a body size limit on every route that accepts a body.
- Blocking work (file listing, argon2) runs in `tokio::task::spawn_blocking`, not on the async runtime.
- Security headers are set once in `router()` in `crates/rackdeskd/src/main.rs`. Loosening the CSP needs a line in the security plan saying why.

## Secrets
- Compare secrets with a constant-time function. Password checks go through `rd_auth::verify_password`.
- Random values come from `getrandom`. Session ids are 256 bits.
- Files holding secrets are created with mode `0600` at open time, not chmod-ed afterwards.
- Don't derive `Debug` on a type that holds a plaintext secret, or write a `Debug` that redacts it.

## Tests
- Unit tests live in a `#[cfg(test)] mod tests` at the bottom of the file. Integration tests for `rackdeskd` go in `crates/rackdeskd/tests/` and drive the router in-process.
- Use `tempfile::tempdir()` for anything on disk. Never write under the repo or `/tmp` by hand.
- Unix-only tests get `#[cfg(unix)]`.
- A security control isn't done until a test fails without it.

## Style
- Each file starts with a `//!` comment saying what it's for and which doc or ADR explains the choice.
- Comments explain why, and mention the milestone when something is deliberately left for later (`M3 note: ...`).
- Run `cargo fmt --all` and fix every clippy warning; CI uses `-D warnings`.
