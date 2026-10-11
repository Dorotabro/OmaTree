#!/bin/bash
# Writes the release-level dist/SHA256SUMS: exactly the five release files, in
# this order, and nothing else.
#
#   packaging/release-checksums.sh [DIST_DIR]        (DIST_DIR defaults to dist/)
#
# The release is built on three machines (see "Release procedure" in
# docs/PACKAGING.md). Copy the Mac's OmaTree-<version>-arm64.dmg and the
# Windows machine's OmaTree-<version>-x64-setup.exe into the Linux machine's
# dist/ next to the three Linux packages, then run this there. It stops if any
# of the five is missing. The .sha256 files that the Mac and Windows scripts
# write are not part of the release and are not listed; if one is next to its
# file, it is only used to check that the file was not damaged in the copy.
# It signs nothing and uploads nothing.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ver=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
[ -n "$ver" ] || { echo "release-checksums.sh: no version in Cargo.toml" >&2; exit 1; }
dist=${1:-$root/dist}
cd "$dist"

files=(
    "omatree-$ver-1-x86_64.pkg.tar.zst"
    "omatree_${ver}_amd64.deb"
    "OmaTree-$ver-x86_64.AppImage"
    "OmaTree-$ver-arm64.dmg"
    "OmaTree-$ver-x64-setup.exe"
)
for f in "${files[@]}"; do
    [ -f "$f" ] || { echo "release-checksums.sh: missing $dist/$f" >&2; exit 1; }
done

# A copy that was damaged on its way here must not be listed as good.
for f in "${files[@]}"; do
    [ -f "$f.sha256" ] || continue
    want=$(awk '{print tolower($1)}' "$f.sha256")
    have=$(sha256sum "$f" | cut -d' ' -f1)
    [ "$want" = "$have" ] || { echo "release-checksums.sh: $f does not match $f.sha256 (damaged copy?)" >&2; exit 1; }
done

sha256sum "${files[@]}" > SHA256SUMS
sha256sum --check SHA256SUMS
[ "$(wc -l < SHA256SUMS)" -eq 5 ] || { echo "release-checksums.sh: SHA256SUMS does not have exactly five lines" >&2; exit 1; }
echo "dist/SHA256SUMS lists the five v$ver release files"
