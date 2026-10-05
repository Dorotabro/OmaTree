#!/bin/bash
# Builds the portable x86_64 AppImage into dist/, inside a disposable Ubuntu
# 20.04 container (see Dockerfile). Needs Docker or Podman.
#
#   packaging/appimage/build-package.sh
#
# Snapshots the working tree (tracked and new, non-ignored files), builds
# OmaTree there against the container's official Qt, and bundles that Qt with
# linuxdeploy. Nothing is installed on the host and nothing is uploaded. The
# Cargo download cache lives outside the source tree in
# ${OMATREE_CACHE:-~/.cache/omatree-packaging}.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"
engine=$(command -v docker || command -v podman || { echo "build-package.sh: neither docker nor podman is installed" >&2; exit 1; })

ver=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cache=${OMATREE_CACHE:-$HOME/.cache/omatree-packaging}
work=$root/dist/appimage-build
image=omatree-build-appimage
out=OmaTree-$ver-x86_64.AppImage

rm -rf "$work"
mkdir -p "$work" "$cache/cargo" "$root/dist"
git ls-files -co --exclude-standard | while read -r f; do [ -e "$f" ] && echo "$f"; done \
    | LC_ALL=C sort > "$work/files"
tar --create --no-recursion --files-from="$work/files" --owner=0 --group=0 --numeric-owner \
    --file="$work/src.tar"

"$engine" build -q -t "$image" packaging/appimage

"$engine" run --rm --user "$(id -u):$(id -g)" \
    -v "$work:/work" -v "$cache/cargo:/cargo" \
    -e CARGO_HOME=/cargo -e HOME=/tmp/home -e CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-false}" \
    -e VERSION="$ver" -e SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)" \
    "$image" bash -euc '
        mkdir -p /tmp/home /tmp/build/omatree && cd /tmp/build/omatree
        tar xf /work/src.tar
        packaging/appimage/build-in-container.sh /work
    '

cp "$work/$out" "$root/dist/$out"
chmod +x "$root/dist/$out"
echo "built: dist/$out"
