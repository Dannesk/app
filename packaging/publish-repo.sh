#!/bin/sh
set -eu

HERE=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_DIR=${REPO_DIR:-$HOME/apt.dannesk.com}
KEYRING="$HERE/dannesk-archive-keyring.gpg"
SIGNING_KEY=${SIGNING_KEY:-Dannesk Archive Signing Key}
ARCHES="amd64 arm64"

[ $# -ge 2 ] || { echo "usage: $0 <version> <deb>..." >&2; exit 2; }
VERSION=$1
shift

[ -f "$KEYRING" ] || { echo "$KEYRING missing — run make-signing-key.sh first" >&2; exit 1; }
for t in dpkg-deb dpkg-scanpackages apt-ftparchive gpg; do
    command -v "$t" >/dev/null || { echo "$t missing (apt install dpkg-dev apt-utils gnupg)" >&2; exit 1; }
done

for deb in "$@"; do
    p=$(dpkg-deb -f "$deb" Package)
    v=$(dpkg-deb -f "$deb" Version)
    a=$(dpkg-deb -f "$deb" Architecture)
    [ "$p" = dannesk ]    || { echo "$deb: package is $p" >&2; exit 1; }
    [ "$v" = "$VERSION" ] || { echo "$deb: version $v, expected $VERSION" >&2; exit 1; }
    case " $ARCHES " in
        *" $a "*) ;;
        *) echo "$deb: architecture $a, expected one of: $ARCHES" >&2; exit 1 ;;
    esac
    tmp=$(mktemp -d)
    dpkg-deb -x "$deb" "$tmp"
    shipped="$tmp/usr/share/keyrings/dannesk-archive-keyring.gpg"
    if [ ! -f "$shipped" ]; then
        rm -rf "$tmp"
        echo "$deb: carries no keyring, so it will not enrol the repository. Commit packaging/dannesk-archive-keyring.gpg and rebuild." >&2
        exit 1
    fi
    if ! cmp -s "$shipped" "$KEYRING"; then
        rm -rf "$tmp"
        echo "$deb: the keyring it ships is not $KEYRING" >&2
        exit 1
    fi
    rm -rf "$tmp"
done

POOL="$REPO_DIR/pool/main/d/dannesk"
mkdir -p "$POOL"
for deb in "$@"; do
    install -m644 "$deb" "$POOL/"
    install -m644 "$deb" "$REPO_DIR/"
done
install -m644 "$KEYRING" "$REPO_DIR/dannesk-archive-keyring.gpg"

cd "$REPO_DIR"

for arch in $ARCHES; do
    d="dists/stable/main/binary-$arch"
    mkdir -p "$d"
    dpkg-scanpackages --multiversion --arch "$arch" pool/ > "$d/Packages"
    gzip -9nkf "$d/Packages"
done

apt-ftparchive \
    -o APT::FTPArchive::Release::Origin=Dannesk \
    -o APT::FTPArchive::Release::Label=Dannesk \
    -o APT::FTPArchive::Release::Suite=stable \
    -o APT::FTPArchive::Release::Codename=stable \
    -o APT::FTPArchive::Release::Architectures="$ARCHES" \
    -o APT::FTPArchive::Release::Components=main \
    -o APT::FTPArchive::Release::Description="Dannesk — DeFi wallet for XRPL and Bitcoin" \
    release dists/stable > dists/stable/Release

rm -f dists/stable/InRelease dists/stable/Release.gpg
gpg --batch --yes --local-user "$SIGNING_KEY" --digest-algo SHA256 --clearsign \
    -o dists/stable/InRelease dists/stable/Release
gpg --batch --yes --local-user "$SIGNING_KEY" --digest-algo SHA256 --detach-sign --armor \
    -o dists/stable/Release.gpg dists/stable/Release

(cd "$POOL" && sha256sum dannesk_"$VERSION"_*.deb) > SHA256SUMS
gpg --batch --yes --local-user "$SIGNING_KEY" --digest-algo SHA256 --detach-sign --armor \
    -o SHA256SUMS.asc SHA256SUMS

gpg --verify dists/stable/InRelease >/dev/null 2>&1 || { echo "InRelease does not verify" >&2; exit 1; }

echo
echo "Repository built in $REPO_DIR for $VERSION. Next:"
echo "  1. Upload to the bucket, the .debs first and the index last: the new .debs (pool/ and"
echo "     the top level), SHA256SUMS + SHA256SUMS.asc, each Packages pair, then dists/stable/."
echo "     Then delete from the bucket the versions removed from the pool, if any."
echo "  2. On an install of the previous version: apt update && apt install --only-upgrade dannesk"
echo "  3. Publish the draft release on GitHub as a pre-release, not before step 2 passes."
echo "  4. Landing: VERSION in src/consts.ts, and SHA256SUMS + SHA256SUMS.asc from here into public/."
