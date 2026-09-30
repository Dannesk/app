#!/bin/sh
# Stages and builds dannesk_<version>_<arch>.deb — no cargo-deb, plain dpkg-deb.
# Runs anywhere rust + dpkg-deb exist (dev box, CI container); output in packaging/out/.
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
ROOT=$(dirname -- "$HERE")

VERSION=$(grep -m1 '^version' "$ROOT/Cargo.toml" | cut -d'"' -f2)
ARCH=$(dpkg --print-architecture)

cargo build --release --manifest-path "$ROOT/Cargo.toml"

TARGET_DIR=$(cargo metadata --format-version 1 --no-deps --manifest-path "$ROOT/Cargo.toml" \
    | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
BIN="$TARGET_DIR/release/Dannesk"

STAGE=$(mktemp -d)
trap 'rm -rf "$STAGE"' EXIT
chmod 755 "$STAGE"   # mktemp gives 0700, which would become the package's root-dir mode

# Payload: binary (lowercase per Debian convention), menu entry, the one icon.
install -Dm755 "$BIN"                  "$STAGE/usr/bin/dannesk"
install -Dm644 "$HERE/dannesk.desktop" "$STAGE/usr/share/applications/dannesk.desktop"
install -Dm644 "$ROOT/src/icon.svg"    "$STAGE/usr/share/icons/hicolor/scalable/apps/dannesk.svg"

# Signing keyring for the update repo — optional until the release key exists.
# Without it, postinst skips the apt hookup and the .deb is a plain install.
if [ -f "$HERE/dannesk-archive-keyring.gpg" ]; then
    install -Dm644 "$HERE/dannesk-archive-keyring.gpg" \
        "$STAGE/usr/share/keyrings/dannesk-archive-keyring.gpg"
else
    echo "NOTE: packaging/dannesk-archive-keyring.gpg missing — building without the update-repo hookup." >&2
fi

# Control metadata. The glibc floor is read off the binary we actually built,
# so Depends is honest wherever this runs (dev box today, 22.04 container in CI).
GLIBC_MIN=$(objdump -T "$BIN" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/GLIBC_//' | sort -V | tail -1)
SIZE_KB=$(du -sk "$STAGE" | cut -f1)
mkdir -p "$STAGE/DEBIAN"
sed -e "s/@VERSION@/$VERSION/" \
    -e "s/@ARCH@/$ARCH/" \
    -e "s/@GLIBC@/$GLIBC_MIN/" \
    -e "s/@SIZE@/$SIZE_KB/" \
    "$HERE/control.in" > "$STAGE/DEBIAN/control"
install -m755 "$HERE/postinst" "$STAGE/DEBIAN/postinst"
install -m755 "$HERE/postrm"   "$STAGE/DEBIAN/postrm"

mkdir -p "$HERE/out"
dpkg-deb --build --root-owner-group "$STAGE" "$HERE/out/dannesk_${VERSION}_${ARCH}.deb"
