#!/usr/bin/env bash
# Makes the key that signs RackDesk releases. Run it once, on your own
# machine.
#
#   scripts/make-release-key.sh
#
# It writes:
#   release-key.private.pem     the private key. NOT in the repository (see
#                               .gitignore). Store it as the GitHub Actions
#                               secret RELEASE_SIGNING_KEY, keep a copy in a
#                               password manager, then delete the file.
#   packaging/release-key.pem   the public key. Commit it.
# and puts the public key into packaging/install.sh. Commit that too.
#
# The installer trusts a download only if this key signed it. If the private
# key is lost, no new release can be installed by an older installer. If it
# leaks, someone else can sign a release: make a new key and publish a new
# installer at once.
set -euo pipefail
cd "$(dirname "$0")/.."

PRIVATE=release-key.private.pem
PUBLIC=packaging/release-key.pem

if [ -e "$PRIVATE" ] || [ -e "$PUBLIC" ]; then
  echo "A key exists already ($PRIVATE or $PUBLIC). Remove it first if you mean to replace it." >&2
  exit 1
fi

umask 077
openssl ecparam -name prime256v1 -genkey -noout -out "$PRIVATE"
umask 022
openssl ec -in "$PRIVATE" -pubout -out "$PUBLIC" 2>/dev/null

# Put the public key between the 2 marker lines of install.sh.
python3 - "$PUBLIC" packaging/install.sh <<'PY'
import sys
key = open(sys.argv[1]).read().strip()
path = sys.argv[2]
text = open(path).read()
start = text.index("# BEGIN RELEASE KEY\n") + len("# BEGIN RELEASE KEY\n")
end = text.index("# END RELEASE KEY")
open(path, "w").write(text[:start] + "RELEASE_KEY='" + key + "'\n" + text[end:])
PY

cat <<TEXT

Done.

1. Store the private key as a GitHub Actions secret:
     gh secret set RELEASE_SIGNING_KEY < $PRIVATE
2. Save a copy of $PRIVATE in a password manager, then delete the file:
     rm $PRIVATE
3. Commit the public key:
     git add $PUBLIC packaging/install.sh
TEXT
