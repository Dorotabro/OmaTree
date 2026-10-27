#!/bin/bash
# Makes the pinned editor library available to a build container that has no credentials.
#
#   packaging/egui/seed-cargo-git.sh CARGO_CACHE_DIR
#
# The editor repository (Dorotabro/omatree-egui-editor-spike) is private, so `cargo fetch` inside a
# clean container fails with "failed to authenticate". Cargo has already fetched the exact commit
# on the machine that can read it, into its git cache. This copies that cache into the container's
# Cargo cache, but only after proving that it contains precisely the commit pinned in
# crates/egui-app/Cargo.toml (and Cargo.lock). Nothing is copied from any sibling checkout, and a
# machine that can reach the repository needs no seeding (the build then fetches it itself).
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)
dest=${1:?usage: seed-cargo-git.sh CARGO_CACHE_DIR}
rev=$(sed -n 's/.*omatree_editor = {.*rev = "\([0-9a-f]\{40\}\)".*/\1/p' "$root/crates/egui-app/Cargo.toml")
[ -n "$rev" ] || { echo "seed-cargo-git.sh: no pinned editor rev in crates/egui-app/Cargo.toml" >&2; exit 1; }
grep -q "omatree-egui-editor-spike.git?rev=$rev#$rev" "$root/Cargo.lock" \
    || { echo "seed-cargo-git.sh: Cargo.lock does not resolve the editor to $rev" >&2; exit 1; }

src=${CARGO_HOME:-$HOME/.cargo}/git
for db in "$src"/db/omatree-egui-editor-spike-*; do
    [ -d "$db" ] || continue
    name=$(basename "$db")
    if [ -d "$dest/git/db/$name" ] && git --git-dir="$dest/git/db/$name" cat-file -e "$rev^{commit}" 2>/dev/null; then
        echo "seed: $dest already has the editor at $rev"; exit 0
    fi
    git --git-dir="$db" cat-file -e "$rev^{commit}" 2>/dev/null \
        || { echo "seed-cargo-git.sh: the local Cargo git cache lacks commit $rev" >&2; exit 1; }
    mkdir -p "$dest/git/db" "$dest/git/checkouts"
    rm -rf "$dest/git/db/$name" "$dest/git/checkouts/$name"
    cp -a "$db" "$dest/git/db/$name"
    [ -d "$src/checkouts/$name" ] && cp -a "$src/checkouts/$name" "$dest/git/checkouts/$name"
    echo "seed: copied the editor library at $rev from $src into $dest"
    exit 0
done
echo "seed: no local Cargo git cache for the editor; the build will have to fetch it from GitHub" >&2
