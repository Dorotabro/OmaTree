#!/bin/bash
# Builds the Arch / Omarchy package into dist/.
#
#   packaging/arch/build-package.sh
#
# Makes a source snapshot of the current working tree (tracked and new,
# non-ignored files), fills in PKGBUILD.in with that tarball and its SHA-256,
# runs makepkg on it, and writes SHA-256 sums of the artefacts. Nothing is
# installed. CARGO_NET_OFFLINE=true in the environment makes the dependency
# fetch use the local Cargo cache only.
#
# This is the validation path. A release package is built from the tagged
# release archive instead; see the note at the top of PKGBUILD.in.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"

ver=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
dist=$root/dist
work=$dist/arch-build
tarball=omatree-$ver.tar.gz

rm -rf "$work"
mkdir -p "$work"

# The snapshot: a fixed order, owner and timestamp, so the same tree gives the
# same archive.
git ls-files -co --exclude-standard | while read -r f; do [ -e "$f" ] && echo "$f"; done \
    | LC_ALL=C sort > "$work/files"
tar --create --gzip --no-recursion --files-from="$work/files" \
    --transform "s,^,omatree-$ver/," --owner=0 --group=0 --numeric-owner \
    --mtime="@$(git log -1 --format=%ct)" --file="$dist/$tarball"
sha=$(sha256sum "$dist/$tarball" | cut -d' ' -f1)
cp "$dist/$tarball" "$work/$tarball"

sed -e "s,@PKGVER@,$ver," -e "s,@SOURCE@,$tarball," -e "s,@SHA256@,$sha," \
    packaging/arch/PKGBUILD.in > "$work/PKGBUILD"

cd "$work"
PKGDEST=$dist SRCDEST=$work BUILDDIR=$work/build makepkg --force --noconfirm --cleanbuild

cd "$dist"
sha256sum "$tarball" omatree-"$ver"-1-x86_64.pkg.tar.zst > SHA256SUMS
echo "built: $dist/omatree-$ver-1-x86_64.pkg.tar.zst"
