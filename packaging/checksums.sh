#!/bin/bash
# Writes dist/SHA256SUMS for the three Linux packages (and, if it has been
# copied into dist/ from the Mac that built it, the macOS disk image) and checks
# the result.
#
#   packaging/checksums.sh
#
# Run it after the three build scripts. It does not sign anything and uploads
# nothing. For a release, run it after building all three from the tagged source.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ver=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
cd "$root/dist"

files=("omatree-$ver-1-x86_64.pkg.tar.zst" "OmaTree-$ver-x86_64.AppImage" "omatree_${ver}_amd64.deb")
# The disk image is built on a Mac (packaging/macos/build-dmg.sh), not here.
[ -f "OmaTree-$ver-arm64.dmg" ] && files+=("OmaTree-$ver-arm64.dmg")
for f in "${files[@]}"; do
    [ -f "$f" ] || { echo "checksums.sh: missing dist/$f" >&2; exit 1; }
done
sha256sum "${files[@]}" > SHA256SUMS
sha256sum --check SHA256SUMS
