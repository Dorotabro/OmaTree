#!/bin/bash
# Builds all three Linux packages from a Git tag, in a clean checkout, and
# writes dist/SHA256SUMS for exactly those files.
#
#   packaging/release-build.sh v0.1.0
#
# The tag is checked out into a temporary worktree (never your working tree),
# which must be clean; the three build scripts run there, so nothing from a
# dirty tree or an earlier build can get in. Needs Docker or Podman, makepkg
# and the other prerequisites of the three build scripts. Nothing is published.
set -euo pipefail

tag=${1:?usage: release-build.sh TAG}
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$root"
git rev-parse -q --verify "refs/tags/$tag" >/dev/null || { echo "release-build.sh: no tag $tag" >&2; exit 1; }

work=$(mktemp -d "${TMPDIR:-/tmp}/omatree-release.XXXXXX")
trap 'git worktree remove --force "$work/src" 2>/dev/null; rm -rf "$work"' EXIT
git worktree add --detach -q "$work/src" "$tag"
cd "$work/src"
[ -z "$(git status --porcelain --ignored)" ] || { echo "release-build.sh: checkout of $tag is not clean" >&2; exit 1; }
echo "building $tag = $(git rev-parse HEAD)"

packaging/arch/build-package.sh
packaging/debian/build-package.sh
packaging/appimage/build-package.sh
packaging/checksums.sh

mkdir -p "$root/dist"
for f in dist/*.pkg.tar.zst dist/*.deb dist/*.AppImage dist/SHA256SUMS; do cp -f "$f" "$root/dist/"; done
echo "release artefacts for $tag are in $root/dist"
