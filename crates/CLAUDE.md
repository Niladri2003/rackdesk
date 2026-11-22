# Rust guide

Read the root `CLAUDE.md` first. This file covers how the Rust crates are written.

## Which crate does what
Dependencies point one way. `rackdeskd` depends on `rd-fs` and `rd-auth`. Those depend on `rd-api`. Library crates never depend on `rackdeskd`, axum or tokio.

| Crate | Put here | Never put here |
|---|---|---|
| `rd-api` | Request and response types, enums, `ApiError` | I/O, logic, anything not sent over the wire |
| `rd-fs` | Every operation on user files, as a method on `Root` | HTTP types, auth |
| `rd-auth` | Passwords, the credential store, sessions (`session`), the limit on password guessing (`limiter`) | HTTP types, file browsing |
| `rd-root-proto` | The `Request` and `Response` of the root program, and the frame format | Anything that touches files |
| `rackdesk-root` | The root program: the socket, the check of the caller, and `handle()` | Async code, HTTP, new actions without a security plan row |
| `rackdeskd` | Routes, extractors, middleware, start-up, config | Path handling, password handling |

A new area of work (terminal, services, protocol of the root program) gets its own `rd-*` crate, so it can be tested without the server.

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
- A method that opens file content uses `Root::open_file`. It refuses folders, pipes and devices, and checks the type on the open file.
- File content is sent by `files::download` only. It decides `attachment` or `inline` from a fixed list of types (`inline_type`). Adding a type to that list needs a line in the security plan.
- A route that takes a path gets a row in the `..` test and the symlink test in `crates/rackdeskd/tests/files.rs`.
- Each new method gets an escape test: `..`, an absolute path, a symlink pointing outside, and a symlink to `/`. See the tests at the bottom of `crates/rd-fs/src/lib.rs`.
- Writes go to a temp file in the target folder, then `fsync`, then an atomic rename. See `write_file` and `copy` in `crates/rd-fs/src/write.rs`.
- A write method takes a `Conflict` rule when its target name can be in use. It never replaces an entry silently.
- A new write method is added to the loop in the test `odd_paths_never_write_outside_the_root`, and its route to `write_routes()` in `crates/rackdeskd/tests/files_write.rs`. That table checks the session, the CSRF rule and the forced password change for every route.

## Endpoints
- Routes are `/api/v1/...`. Unknown `/api/*` paths return 404, never the UI.
- Handlers are thin: extract, check the session, call a library crate, map the error to `ApiError`. They return `Result<_, ApiErr>` (`crates/rackdeskd/src/error.rs`).
- A new route goes in the `protected` router in `crates/rackdeskd/src/lib.rs`. That router refuses requests with no session, and requests made before the initial password is changed.
- Code that depends on time takes the time as an argument (`now: Instant`), as `rd_auth::session` does. Tests then need no sleep.
- The CSRF check (`csrf.rs`) runs on every POST, PUT, PATCH and DELETE. A WebSocket upgrade is a GET, so it must call the same check itself.
- Put a body size limit on every route that accepts a body. The API default is 8 KiB. A route that takes file content sets its own, as the save and chunk routes do in `lib.rs`.
- Blocking work (file listing, argon2) runs in `tokio::task::spawn_blocking`, not on the async runtime.
- Security headers are set once in `router()` in `crates/rackdeskd/src/lib.rs`. Loosening the CSP needs a line in the security plan saying why.

## The root program
- `Request` in `rd-root-proto` is the full list of what can be done as root. A new action needs a row in the security plan first, then a case in `handle()`, then a row in the test `every_action_stays_beneath_the_files_root`.
- The root program stays small: no async runtime, no HTTP, no config file. Every path goes through `rd_fs::Root`.
- The server calls the root program only for a request with `elevate` (`perms.rs`, `files.rs`). Never as a silent fallback.
- Every call to the root program is recorded in the audit log with an action name that starts with `root.`.
- Code that needs Linux gets `#[cfg(target_os = "linux")]`. Check it with `make test-linux`.

## The terminal
- `term.rs` owns the shells. Each shell has 1 task (`run_shell`) that reads its output, takes messages on a channel, and ends it.
- A new way for a session to end must call `state.terminals().stop(...)`, as sign-out and the password change do.
- A WebSocket route checks the origin itself with `csrf::same_origin`. The CSRF layer covers POST, PUT, PATCH and DELETE only.
- A test that needs a WebSocket binds a real socket. See `listen()` and `open()` in `tests/common/mod.rs`.

## The data folder
- A change to the layout of the data folder (a new file format, a renamed file) is a new step in `MIGRATIONS` in `crates/rackdeskd/src/datadir.rs`, with a test. A step must be safe to run again after it failed half way.
- A file in the data folder is written under a temporary name made with `create_new`, then renamed. The service user can place a link in that folder, and `create_new` never follows one.
- `rackdeskd` refuses to run as root on a data folder that another user owns. Don't add code that changes the owner of a file there.
- The first password is never printed when the output goes to the journal (`JOURNAL_STREAM` is set). `rackdeskd init` and `reset-password` print it, because a person runs them.
- Demo mode (`demo.rs`) must not reach real files. A new setting that widens what the server can reach gets a line in `demo::check`.

## The audit log
- A handler that changes something takes `Extension(actor): Extension<Actor>` and passes its result through `audited()` (`files_write.rs`), or calls `state.audit().record()`.
- A line holds paths and names. Never a password, a session id or file content. A typed username that is not the account is recorded as "(unknown user)".

## Secrets
- Compare secrets with a constant-time function. Password checks go through `rd_auth::verify_password`.
- Random values come from `getrandom`. Session ids are 256 bits.
- Files holding secrets are created with mode `0600` at open time, not chmod-ed afterwards.
- Don't derive `Debug` on a type that holds a plaintext secret, or write a `Debug` that redacts it.

## Tests
- Unit tests live in a `#[cfg(test)] mod tests` at the bottom of the file. Integration tests for `rackdeskd` go in `crates/rackdeskd/tests/` and drive the router in-process. `tests/common/mod.rs` has the helpers (`server()`, `call()`, `sign_in()`, `active_session()`).
- Use `tempfile::tempdir()` for anything on disk. Never write under the repo or `/tmp` by hand.
- Unix-only tests get `#[cfg(unix)]`.
- A security control isn't done until a test fails without it.

## Style
- Each file starts with a `//!` comment saying what it's for and which doc or ADR explains the choice.
- Comments explain what the code does and why. They never name a milestone or a version (`M3`, `v2`). Write "not built yet" or state the limit. The plan lives in `docs/ROADMAP.md`.
- Run `cargo fmt --all` and fix every clippy warning; CI uses `-D warnings`.
