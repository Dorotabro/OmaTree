#!/bin/bash
# Builds the egui Debian/Ubuntu amd64 package (omatree-egui) into dist/, inside a disposable
# Debian 13 container (see Dockerfile). Needs Docker or Podman.
#
#   packaging/egui/debian/build-package.sh
#
# The working tree is snapshotted (tracked and new, non-ignored files), the Debian packaging
# directory is placed at the root of a temporary copy and dpkg-buildpackage runs there. Nothing is
# installed on the host and nothing is uploaded. The Cargo download cache lives outside the source
# tree in ${OMATREE_CACHE:-~/.cache/omatree-packaging}. The editor library is fetched at the commit
# pinned in Cargo.lock; no sibling checkout is mounted.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$root"
engine=$(command -v docker || command -v podman || { echo "build-package.sh: neither docker nor podman is installed" >&2; exit 1; })

ver=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/egui-app/Cargo.toml | head -1)
cache=${OMATREE_CACHE:-$HOME/.cache/omatree-packaging}
work=$root/dist/egui-deb-build
image=omatree-egui-build-debian13

rm -rf "$work"
mkdir -p "$work" "$cache/cargo" "$root/dist"
git ls-files -co --exclude-standard | while read -r f; do [ -e "$f" ] && echo "$f"; done \
    | LC_ALL=C sort > "$work/files"
tar --create --no-recursion --files-from="$work/files" --owner=0 --group=0 --numeric-owner \
    --file="$work/src.tar"

packaging/egui/seed-cargo-git.sh "$cache/cargo"

"$engine" build -q -t "$image" packaging/egui/debian

"$engine" run --rm --user "$(id -u):$(id -g)" \
    -v "$work:/work" -v "$cache/cargo:/cargo" \
    -e CARGO_HOME=/cargo -e HOME=/tmp/home -e CARGO_NET_OFFLINE="${CARGO_NET_OFFLINE:-false}" \
    -e SOURCE_DATE_EPOCH="$(git log -1 --format=%ct)" \
    "$image" bash -euc '
        mkdir -p /tmp/home /tmp/build/omatree-egui && cd /tmp/build/omatree-egui
        tar xf /work/src.tar
        cp -r packaging/egui/debian/debian debian
        cargo fetch --locked
        dpkg-buildpackage -b -uc -us
        cp /tmp/build/omatree-egui_'"$ver"'_amd64.deb /tmp/build/omatree-egui_'"$ver"'_amd64.changes /work/
        lintian --info --display-info /tmp/build/omatree-egui_'"$ver"'_amd64.changes > /work/lintian.txt 2>&1 || true
        { echo "rustc: $(rustc -V)"; echo "cargo: $(cargo -V)"; dpkg-query -W debhelper dpkg-dev gcc; } > /work/toolchain.txt
    '

cp "$work/omatree-egui_${ver}_amd64.deb" "$root/dist/"
echo "built: dist/omatree-egui_${ver}_amd64.deb (lintian output: dist/egui-deb-build/lintian.txt)"
