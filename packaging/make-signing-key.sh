#!/bin/sh
# Generates the apt repository signing key and exports its public half.
#
# Run ONCE, on the machine that will publish the repository. The private key
# never leaves it: CI builds the packages, this machine signs the repository
# (publish-repo.sh), and apt trusts the signed InRelease, not the .deb.
#
# The March key and the 2026-01-18 key are void (ROADMAP.md); this makes the
# fresh one. RSA 4096 rather than Ed25519 because every apt back to Ubuntu
# 22.04 / Debian 12 verifies it without question. No expiry, because an
# expired archive key breaks `apt update` on every install until the keyring
# is replaced by hand. Sign-only.
#
# Writes:
#   packaging/dannesk-archive-keyring.gpg   binary keyring. build-deb.sh ships
#                                           it in the .deb, publish-repo.sh puts
#                                           it at the root of apt.dannesk.com.
#                                           COMMIT IT — it is the public half.
#   packaging/out/dannesk.pub               armored copy, for the landing page.
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
UID_STR="Dannesk Archive Signing Key <support@dannesk.com>"
KEYRING="$HERE/dannesk-archive-keyring.gpg"

if [ -e "$KEYRING" ]; then
    echo "$KEYRING exists — refusing to overwrite the published key." >&2
    exit 1
fi

# gpg asks for a passphrase through pinentry; publish-repo.sh asks for it
# again, through gpg-agent, each time it signs.
gpg --batch --quick-generate-key "$UID_STR" rsa4096 sign never

FPR=$(gpg --list-keys --with-colons "=$UID_STR" | awk -F: '/^fpr/ { print $10; exit }')
[ -n "$FPR" ] || { echo "key not found after generation" >&2; exit 1; }

gpg --export "$FPR" > "$KEYRING"
mkdir -p "$HERE/out"
gpg --export --armor "$FPR" > "$HERE/out/dannesk.pub"

echo
echo "fingerprint: $FPR"
echo "keyring:     $KEYRING   (commit this)"
echo "public key:  $HERE/out/dannesk.pub   (landing/public/dannesk.pub)"
echo "publish-repo.sh finds the key by its uid; SIGNING_KEY=$FPR pins it."
