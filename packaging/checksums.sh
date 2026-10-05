#!/bin/bash
# Writes dist/SHA256SUMS for the three Linux packages and checks the result.
#
#   packaging/checksums.sh
#
# Run it after the three build scripts. It does not sign anything and uploads
# nothing; the release (Ticket 033) regenerates the sums from the tagged source.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
ver=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
cd "$root/dist"

files=("omatree-$ver-1-x86_64.pkg.tar.zst" "OmaTree-$ver-x86_64.AppImage" "omatree_${ver}_amd64.deb")
for f in "${files[@]}"; do
    [ -f "$f" ] || { echo "checksums.sh: missing dist/$f" >&2; exit 1; }
done
sha256sum "${files[@]}" > SHA256SUMS
sha256sum --check SHA256SUMS
