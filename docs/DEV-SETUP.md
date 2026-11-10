# Development setup

Works on macOS and Linux. The file layer uses cap-std, so no Linux VM is needed until system features (M5).

## Needs
Rust stable (`rust-toolchain.toml`), Node 20+.

## Run
```sh
# terminal 1: the server (prints the admin password on first start)
cargo run -p webdeskd -- --data-dir ./dev-data

# terminal 2: the UI with hot reload, proxying /api to the server
cd web && npm install && npm run dev      # http://localhost:5173
```
To test the single binary: `cd web && npm run build`, then open http://127.0.0.1:7171.
Lost the dev password? Delete `./dev-data` and restart.

## Checks (same as CI)
```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # also regenerates web/src/api/generated/*.ts
cd web && npm run lint && npm run build
```

## Adding an API type
Add it in `crates/wd-api` with `#[derive(TS)] #[ts(export)]`, run `cargo test -p wd-api`, and commit the generated `.ts` file. CI fails if the generated files are stale.
