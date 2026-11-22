#!/usr/bin/env bash
# Builds the release files for 1 processor type into dist/.
#
#   scripts/build-release.sh x86_64
#   scripts/build-release.sh aarch64
#
# It expects:
#   - web/dist is built (`npm run build` in web/). The UI is embedded in rackdeskd.
#   - the Rust target <arch>-unknown-linux-musl is installed, with a C compiler
#     for it (the `ring` crate has C and assembly code).
#   - nfpm is on the PATH, for the .deb and .rpm files. Without it they are skipped.
#
# Output in dist/:
#   rackdesk-linux-<arch>.tar.gz      the files that install.sh installs
#   rackdesk_<version>_<arch>.deb     if nfpm is found
#   rackdesk-<version>.<arch>.rpm     if nfpm is found
#
# The programs are linked statically against musl, so they run on any Linux
# of that processor type, whatever C library it has.
set -euo pipefail
cd "$(dirname "$0")/.."

ARCH="${1:?usage: build-release.sh x86_64|aarch64}"
case "$ARCH" in
  x86_64) DEB_ARCH=amd64 ;;
  aarch64) DEB_ARCH=arm64 ;;
  *) echo "unknown processor type: $ARCH" >&2; exit 1 ;;
esac
TARGET="$ARCH-unknown-linux-musl"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
OUT="${OUT_DIR:-dist}"

[ -f web/dist/index.html ] || { echo "web/dist is missing. Run: cd web && npm ci && npm run build" >&2; exit 1; }

cargo build --release --locked --target "$TARGET" -p rackdeskd -p rackdesk-root
BUILT="$TARGET_DIR/$TARGET/release"
VERSION="$(sed -n '/^\[workspace.package\]/,/^\[/s/^version = "\(.*\)"/\1/p' Cargo.toml)"

# A program that needs a shared library would not start on a system that
# lacks it. Stop here if the link is not static.
for program in rackdeskd rackdesk-root; do
  if command -v file >/dev/null 2>&1 && ! file "$BUILT/$program" | grep -Eq 'static(ally|-pie) linked'; then
    echo "$program is not statically linked" >&2
    exit 1
  fi
done

units() {
  # units BINDIR FOLDER: writes the 2 unit files for programs in BINDIR.
  mkdir -p "$2"
  for unit in rackdeskd.service rackdesk-root.service; do
    sed "s|@BINDIR@|$1|g" "packaging/systemd/$unit" >"$2/$unit"
  done
}

# ---------- The archive for install.sh ----------
NAME="rackdesk-linux-$ARCH"
STAGE="$OUT/$NAME"
rm -rf "$STAGE"
mkdir -p "$STAGE/bin"
cp "$BUILT/rackdeskd" "$BUILT/rackdesk-root" "$STAGE/bin/"
units /usr/local/bin "$STAGE/systemd"
cp packaging/rackdesk.env packaging/uninstall.sh LICENSE "$STAGE/"
chmod 0755 "$STAGE/bin/"* "$STAGE/uninstall.sh"
# Fixed owner, order and time, so the same source gives the same archive.
tar --sort=name --owner=0 --group=0 --numeric-owner --mtime='2020-01-01 00:00:00Z' \
  -cf - -C "$OUT" "$NAME" | gzip -n >"$OUT/$NAME.tar.gz"
rm -rf "$STAGE"

# ---------- The packages ----------
if command -v nfpm >/dev/null 2>&1; then
  PKG="$OUT/pkg-$ARCH"
  rm -rf "$PKG"
  mkdir -p "$PKG"
  cp "$BUILT/rackdeskd" "$BUILT/rackdesk-root" "$PKG/"
  units /usr/bin "$PKG/systemd"
  # nfpm does not fill in variables in file paths, so they are written here.
  sed -e "s|\${VERSION}|$VERSION|g" -e "s|\${DEB_ARCH}|$DEB_ARCH|g" -e "s|\${PKG}|$PKG|g" \
    packaging/nfpm.yaml >"$PKG/nfpm.yaml"
  nfpm package --config "$PKG/nfpm.yaml" --packager deb --target "$OUT/"
  nfpm package --config "$PKG/nfpm.yaml" --packager rpm --target "$OUT/"
  rm -rf "$PKG"
else
  echo "nfpm not found: no .deb and .rpm files were made" >&2
fi

ls -l "$OUT"
