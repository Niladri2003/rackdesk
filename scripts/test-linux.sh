#!/usr/bin/env bash
# Runs the Linux-only checks in a container, as root.
#
# The root program asks the kernel who is on the other end of its socket
# (SO_PEERCRED). That exists on Linux only, so this code is not compiled on
# macOS. This script lints and tests it on Linux from any machine with
# Docker. CI runs the same commands on its Linux machine.
#
# The source folder is mounted read-only. Build output goes to Docker
# volumes, so a second run is fast.
set -euo pipefail
cd "$(dirname "$0")/.."

CRATES=(-p rd-fs -p rd-root-proto -p rackdesk-root -p rackdeskd)

docker run --rm \
  -v "$PWD":/src:ro \
  -v rackdesk-linux-target:/target \
  -v rackdesk-linux-cargo:/usr/local/cargo/registry \
  -e CARGO_TARGET_DIR=/target \
  -w /src \
  rust:1-slim \
  sh -c "rustup component add clippy >/dev/null 2>&1; \
         cargo clippy --locked ${CRATES[*]} --all-targets -- -D warnings && \
         cargo test --locked ${CRATES[*]}"
