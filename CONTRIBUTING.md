# Contributing to RackDesk

Thank you for helping. This page says how to set up the project, which checks a change must pass, and which rules are never broken.

Found a security bug? Do not open an issue. See `SECURITY.md`.

## Set up

You need Rust (stable), Node 24 and, for the Linux-only code, Docker. Development works on Linux and macOS. The product runs on Linux.

```sh
make setup     # install the web UI packages
make run       # the server on 127.0.0.1:7171. It prints the admin password at first start
make web       # in a second terminal: the UI on http://localhost:5173
```

`make help` lists every command. To try RackDesk without touching your files: `cargo run -p rackdeskd -- --demo`.

## Where things are

| Path | What it is |
|---|---|
| `crates/` | The Rust code: the server `rackdeskd`, the root program, and the libraries they use |
| `web/` | The React UI |
| `site/` | The landing page |
| `packaging/` | The installer, the systemd units and the package config |
| `scripts/` | Build, test and release scripts |

`CLAUDE.md` in the root, in `crates/`, in `web/` and in `site/` hold the detailed rules for each part. They are written for coding assistants and are just as useful for people.

## Before you open a pull request

Run the checks that CI runs:

```sh
make check        # format, lint, tests, UI lint and build
make e2e          # browser tests and accessibility checks. After any UI change
make test-linux   # the Linux-only code, in a container. After a change to rd-fs, the root program or its protocol
```

A change to `packaging/` or to the start-up of the server also needs `make test-release`. It builds a release, installs it in a container with systemd, and removes it again.

To try a build on a real server before a release exists:

```sh
make local-release ARCH=x86_64      # or ARCH=aarch64. Default: the processor of this machine
scp -r rd-release user@server:
ssh user@server sudo sh rd-release/install.sh --from rd-release
```

`rd-release/` is signed with a key that is made for that 1 build and deleted. Git ignores the folder.

## Rules that are never broken

- **Server-side checks are the security.** Hiding a button in the UI is never a control.
- **A file path from a request goes through `rd_fs::Root` only.** No `std::fs` or `tokio::fs` with request data anywhere else.
- **Secrets are never logged:** passwords, session ids, tokens.
- **Every new endpoint** has a session check, input size limits, a test for a request with no session, and a CSRF test if it changes state.
- **No `unsafe`.** The workspace denies it.
- **Nothing makes the server more exposed without an explicit flag.** It listens on 127.0.0.1 by default.
- **Nothing runs in the background that nobody is looking at.** No polling, indexing or sampling without an open window.
- **RackDesk works with the server as it is.** It reads the existing units, containers and configs. It never needs its own layout.

## Tests

Write a test where one is needed: for a security control, for a rule that can break without anyone seeing it, and for a bug that was found. Before you add a test, name what would break without it. Do not write a test for every function or every text.

## Style

- Simple English in code comments, docs and UI text. Short sentences. Digits for numbers.
- The hyphen (-) is the only dash.
- A comment says what the code does and why. It never names a milestone or a version.
- Commits: a short subject that starts with the area, for example `files: resume an upload after a network fault`.
- A new dependency needs a reason in the pull request. Its licence must be on the list in `deny.toml`.
- A decision that is hard to reverse gets a short record in `docs/adr/`.

## Making a release

For maintainers.

1. Set `version` under `[workspace.package]` in `Cargo.toml`, and run `cargo check` so `Cargo.lock` follows.
2. In `CHANGELOG.md`, rename `## Unreleased` to the version, for example `## 1.0.0`.
3. Run `make check`, `make e2e`, `make test-linux` and `make test-release`.
4. Commit, then tag and push: `git tag v1.0.0 && git push origin v1.0.0`.
5. The `release` workflow builds, signs and creates a **draft** release. Read it, then publish it.

The first time only: run `scripts/make-release-key.sh` and follow what it prints. Without the key the workflow stops.
