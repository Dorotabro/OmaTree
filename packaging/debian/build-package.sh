#!/bin/bash
# Builds the Debian/Ubuntu amd64 package into dist/, inside a disposable
# Debian 13 container (see Dockerfile). Needs Docker or Podman.
#
#   packaging/debian/build-package.sh
#
# The working tree is snapshotted (tracked and new, non-ignored files), the
# Debian packaging directory is placed at the root of a temporary copy, and
# dpkg-buildpackage runs there. Nothing is installed on the host and nothing
# is uploaded. The Cargo download cache lives outside the source tree in
# ${OMATREE_CACHE:-~/.cache/omatree-packaging}.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
cd "$root"
engine=$(command -v docker || command -v podman || { echo "build-package.sh: neither docker nor podman is installed" >&2; exit 1; })

ver=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cache=${OMATREE_CACHE:-$HOME/.cache/omatree-packaging}
work=$root/dist/deb-build
image=omatree-build-debian13

rm -rf "$work"
mkdir -p "$work" "$cache/cargo" "$root/dist"
git ls-files -co --exclude-standard | while read -r f; do [ -e "$f" ] && echo "$f"; done \
    | LC_ALL=C sort > "$work/files"
tar --create --no-recursion --files-from="$work/files" --owner=0 --group=0 --numeric-owner \
    --file="$work/src.tar"

"$engine" build -q -t "$image" packaging/debian

"$engine" run --rm --user "$(id -u):$(id -g)" \
    -v "$work:/work" -v "$cache/cargo:/cargo" \
    -e CARGO_HOME=/cargo -e HOME=/tmp/home -e CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-false}" \
    "$image" bash -euc '
        mkdir -p /tmp/home /tmp/build/omatree && cd /tmp/build/omatree
        tar xf /work/src.tar
        cp -r packaging/debian/debian debian
        cargo fetch --locked
        dpkg-buildpackage -b -uc -us
        cp /tmp/build/omatree_'"$ver"'_amd64.deb /tmp/build/omatree_'"$ver"'_amd64.changes /work/
        lintian --info --display-info /tmp/build/omatree_'"$ver"'_amd64.changes > /work/lintian.txt 2>&1 || true
    '

cp "$work/omatree_${ver}_amd64.deb" "$root/dist/"
echo "built: dist/omatree_${ver}_amd64.deb (lintian output: dist/deb-build/lintian.txt)"
