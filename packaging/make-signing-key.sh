#!/bin/sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
UID_STR="Dannesk Archive Signing Key <support@dannesk.com>"
KEYRING="$HERE/dannesk-archive-keyring.gpg"

if [ -e "$KEYRING" ]; then
    echo "$KEYRING exists — refusing to overwrite the published key." >&2
    exit 1
fi

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
