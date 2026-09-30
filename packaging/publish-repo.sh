#!/bin/sh
# Builds and signs the apt repository from released .debs: a static directory
# tree that Cloudflare R2 serves at https://apt.dannesk.com.
#
#   publish-repo.sh <version> <deb>...
#   publish-repo.sh 0.1.0 ~/Downloads/dannesk_0.1.0_amd64.deb ~/Downloads/dannesk_0.1.0_arm64.deb
#
# The .debs are the ones release.yml built and attached to the DRAFT release:
# download them from the draft's page. This machine holds the signing key; CI
# never does (ROADMAP.md).
#
# The tree lives in $REPO_DIR (default ~/apt.dannesk.com) and is the source of
# truth for the bucket: it keeps every version published so far and is synced
# UP, never rebuilt from the bucket. Layout:
#
#   dannesk-archive-keyring.gpg              what the README's curl line fetches
#   SHA256SUMS, SHA256SUMS.asc               this version's .debs — the landing's Verify row
#   dists/stable/InRelease                   the signed index apt trusts
#   dists/stable/Release, Release.gpg        the same, as the older detached pair
#   dists/stable/main/binary-<arch>/Packages{,.gz}
#   pool/main/d/dannesk/dannesk_<v>_<arch>.deb
#
# Matches postinst's sources entry exactly: URIs https://apt.dannesk.com,
# Suites stable, Components main, Architectures amd64 arm64.
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

# Every .deb must be ours, this version, one of our architectures, and must
# carry THIS keyring — a package that enrols the repository with a different
# key, or none, would leave every install unable to update.
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
done
install -m644 "$KEYRING" "$REPO_DIR/dannesk-archive-keyring.gpg"

cd "$REPO_DIR"

# From the repository root, so each Filename: is pool/main/d/dannesk/… —
# relative to the URIs line, which is how apt resolves it.
for arch in $ARCHES; do
    d="dists/stable/main/binary-$arch"
    mkdir -p "$d"
    dpkg-scanpackages --arch "$arch" pool/ > "$d/Packages"
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

# Checksums of this version's packages, with a detached signature.
(cd "$POOL" && sha256sum dannesk_"$VERSION"_*.deb) > SHA256SUMS
gpg --batch --yes --local-user "$SIGNING_KEY" --digest-algo SHA256 --detach-sign --armor \
    -o SHA256SUMS.asc SHA256SUMS

gpg --verify dists/stable/InRelease >/dev/null 2>&1 || { echo "InRelease does not verify" >&2; exit 1; }

echo
echo "Repository built in $REPO_DIR for $VERSION. In this order:"
echo "  1. Upload the whole tree to the R2 bucket behind apt.dannesk.com"
echo "     (rclone sync $REPO_DIR <remote>:<bucket>, or the Cloudflare dashboard)."
echo "  2. Check:  curl -4 -fsS https://apt.dannesk.com/dists/stable/InRelease | head -3"
echo "  3. On a clean Ubuntu 22.04 or Debian 12: the README's apt block, then apt install dannesk."
echo "  4. Publish the draft release on GitHub — not before step 2 passes; the .deb enrols this repository."
echo "  5. Landing: SHA256SUMS, SHA256SUMS.asc (here) and packaging/out/dannesk.pub into landing/public,"
echo "     then drop the hrefs into Hero.astro's DOWNLOADS, APT and VERIFY."
