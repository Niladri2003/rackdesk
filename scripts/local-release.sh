#!/usr/bin/env bash
# Builds a release on this machine and signs it with a throwaway key. The
# result is a folder that install.sh accepts, for a test on a real server
# before a real release exists. Needs Docker, Node and openssl.
#
#   scripts/local-release.sh                 for the processor of this machine
#   scripts/local-release.sh x86_64          for x86_64
#   scripts/local-release.sh aarch64         for aarch64
#   scripts/local-release.sh --no-build x86_64   sign the files that are in dist/
#
# The folder is rd-release/ in the top folder of the repository. Set
# RELEASE_DIR to use another one. Then, for example:
#
#   scp -r rd-release user@server:
#   ssh user@server sudo sh rd-release/install.sh --from rd-release
#
# How the key is used: a new key pair is made, its private half signs the
# list of checksums, and its public half is written into the copy of
# install.sh in the folder. Then both halves are deleted. The real release
# key is never used, and nothing else can be signed for this installer.
#
# A build for the other processor type runs under emulation. It works, and
# it can take 20 to 40 minutes the first time.
set -euo pipefail
cd "$(dirname "$0")/.."

BUILD=1
ARCH=""
for arg in "$@"; do
  case "$arg" in
    --no-build) BUILD=0 ;;
    x86_64 | aarch64) ARCH="$arg" ;;
    *) echo "usage: local-release.sh [--no-build] [x86_64|aarch64]" >&2; exit 1 ;;
  esac
done
if [ -z "$ARCH" ]; then
  case "$(uname -m)" in
    arm64 | aarch64) ARCH=aarch64 ;;
    x86_64 | amd64) ARCH=x86_64 ;;
    *) echo "unknown processor type: $(uname -m)" >&2; exit 1 ;;
  esac
fi
case "$ARCH" in
  aarch64) PLATFORM=linux/arm64; NFPM_FILE=Linux_arm64; NFPM_SUM=d4eee71839e92c68cbd3b101640bad32d2856fc551acae0635be9118e48c9d9c ;;
  x86_64) PLATFORM=linux/amd64; NFPM_FILE=Linux_x86_64; NFPM_SUM=2afaa5ffb5e3493402e31e7c67a5a1fb42133e3f66fc315ca0051934125561c7 ;;
esac
NFPM_VERSION=2.49.0
REL="${RELEASE_DIR:-rd-release}"
ARCHIVE="rackdesk-linux-$ARCH.tar.gz"

# ---------- Build ----------
if [ "$BUILD" -eq 1 ]; then
  (cd web && npm run build >/dev/null)
  mkdir -p dist
  # Build output is kept in Docker volumes, 1 pair per processor type, so a
  # second build is fast.
  docker run --rm --platform "$PLATFORM" \
    -v "$PWD":/src:ro -v "$PWD/dist":/out \
    -v "rackdesk-musl-$ARCH-target":/target -v "rackdesk-musl-$ARCH-cargo":/usr/local/cargo/registry \
    -e CARGO_TARGET_DIR=/target -e OUT_DIR=/out -w /src \
    rust:1-alpine sh -c "
      set -e
      apk add --no-cache bash build-base file tar curl >/dev/null
      curl -fsSL -o /tmp/nfpm.tgz https://github.com/goreleaser/nfpm/releases/download/v$NFPM_VERSION/nfpm_${NFPM_VERSION}_$NFPM_FILE.tar.gz
      echo '$NFPM_SUM  /tmp/nfpm.tgz' | sha256sum -c - >/dev/null
      tar -xzf /tmp/nfpm.tgz -C /usr/local/bin nfpm
      bash scripts/build-release.sh $ARCH
      bash scripts/measure.sh /target/$ARCH-unknown-linux-musl/release/rackdeskd | tee /out/measured-$ARCH.txt
    "
fi
[ -f "dist/$ARCHIVE" ] || { echo "dist/$ARCHIVE is missing. Run without --no-build." >&2; exit 1; }

# ---------- The folder, signed with a throwaway key ----------
KEYS="$(mktemp -d)"
trap 'rm -rf "$KEYS"' EXIT

rm -rf "$REL"
mkdir -p "$REL"
cp "dist/$ARCHIVE" "$REL/"
# The packages of this processor type, if they were built.
case "$ARCH" in
  aarch64) PACKAGES="dist/*_arm64.deb dist/*.aarch64.rpm" ;;
  x86_64) PACKAGES="dist/*_amd64.deb dist/*.x86_64.rpm" ;;
esac
for package in $PACKAGES; do
  if [ -f "$package" ]; then cp "$package" "$REL/"; fi
done

openssl ecparam -name prime256v1 -genkey -noout -out "$KEYS/private.pem" 2>/dev/null
openssl ec -in "$KEYS/private.pem" -pubout -out "$KEYS/public.pem" 2>/dev/null
python3 - "$KEYS/public.pem" packaging/install.sh "$REL/install.sh" <<'PY'
import sys
key = open(sys.argv[1]).read().strip()
text = open(sys.argv[2]).read()
start = text.index("# BEGIN RELEASE KEY\n") + len("# BEGIN RELEASE KEY\n")
end = text.index("# END RELEASE KEY")
open(sys.argv[3], "w").write(text[:start] + "RELEASE_KEY='" + key + "'\n" + text[end:])
PY
# macOS has shasum, Linux has sha256sum. The output is the same.
if command -v sha256sum >/dev/null 2>&1; then
  (cd "$REL" && sha256sum -- * >"$KEYS/SHA256SUMS")
else
  (cd "$REL" && shasum -a 256 -- * >"$KEYS/SHA256SUMS")
fi
mv "$KEYS/SHA256SUMS" "$REL/SHA256SUMS"
openssl dgst -sha256 -sign "$KEYS/private.pem" -out "$REL/SHA256SUMS.sig" "$REL/SHA256SUMS"

echo
echo "Test release for $ARCH in $REL/:"
ls -1 "$REL"
