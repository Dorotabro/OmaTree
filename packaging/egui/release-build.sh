#!/bin/bash
# Builds the three Linux x86_64 packages of the egui version from ONE Git revision, in a clean worktree,
# verifies them and writes the provenance record and the checksums. Nothing is tagged, pushed, uploaded or
# published, no tracked file is modified, and the Qt release scripts are not used.
#
#   packaging/egui/release-build.sh --version 0.2.0                  # candidate from HEAD
#   packaging/egui/release-build.sh --version 0.2.0 --ref v0.2.0     # from a tag (or any commit)
#   options: --out DIR        where the artifacts go (default dist/egui-release-VERSION)
#            --allow-branch B one more branch accepted for a HEAD build (default: egui-integration and main)
#            --skip-source-checks   do not run packaging/egui/tests/verify-sources.sh in the worktree
#
# HEAD build (no --ref): the current branch must be egui-integration or main and the working tree must be
# clean (no modified, staged or untracked files), so the artifacts come from the committed revision and from
# nothing else. --ref build: REF must name an existing commit or tag; a tag called v* must be annotated; the
# working tree is not used and may be dirty. The revision is checked out into a temporary detached worktree
# that is removed afterwards.
#
# Checksums: SHA256SUMS is written only when REF is the tag vVERSION (a final build); every other build writes
# SHA256SUMS.candidate. Final checksums must in the end describe the exact files attached to the release
# (docs/RELEASING.md step 9: run sha256sum again on those files).
#
# Needs: git, python3, makepkg, bsdtar, Docker (the Debian and AppImage builds run in pinned containers),
# network access to crates.io. The output of the three package scripts goes to OUT/logs/.
set -euo pipefail

die() { echo "release-build.sh: $*" >&2; exit 1; }
step() { echo; echo "== $*"; }

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)

version= ref= out= extra_branch= source_checks=1
while [ $# -gt 0 ]; do
    case $1 in
        --version) version=${2:?--version needs a value}; shift 2 ;;
        --ref) ref=${2:?--ref needs a value}; shift 2 ;;
        --out) out=${2:?--out needs a value}; shift 2 ;;
        --allow-branch) extra_branch=${2:?--allow-branch needs a value}; shift 2 ;;
        --skip-source-checks) source_checks=0; shift ;;
        -h|--help) sed -n '2,25p' "$0"; exit 0 ;;
        *) die "unknown argument: $1 (see --help)" ;;
    esac
done
[ -n "$version" ] || die "--version X.Y.Z is required (the version this release is meant to carry)"
case $version in
    [0-9]*.[0-9]*.[0-9]*) ;; *) die "--version must be plain X.Y.Z (no prerelease suffix: the Arch and Debian versions cannot carry one)" ;;
esac
case $version in *[!0-9.]*) die "--version must be plain X.Y.Z" ;; esac

for t in git python3 makepkg bsdtar docker sha256sum; do
    command -v "$t" >/dev/null || die "required tool not found: $t"
done
cd "$root"
git rev-parse --git-dir >/dev/null 2>&1 || die "not inside a Git checkout"

# --- 1. which revision -----------------------------------------------------------------------------------
if [ -z "$ref" ]; then
    branch=$(git branch --show-current)
    case $branch in
        egui-integration|main) ;;
        *) [ -n "$extra_branch" ] && [ "$branch" = "$extra_branch" ] \
               || die "HEAD is on '${branch:-a detached HEAD}', not egui-integration or main (use --ref, or --allow-branch)" ;;
    esac
    [ -z "$(git status --porcelain)" ] \
        || die "the working tree is not clean (modified, staged or untracked files); commit or stash first, or build a committed revision with --ref"
    ref=HEAD
    mode="HEAD of $branch"
else
    mode="--ref $ref"
fi
commit=$(git rev-parse --verify "$ref^{commit}" 2>/dev/null) || die "no such commit or tag: $ref"
tree=$(git rev-parse "$commit^{tree}")
final=0
if git rev-parse -q --verify "refs/tags/$ref" >/dev/null; then
    [ "$(git cat-file -t "refs/tags/$ref")" = tag ] || die "tag $ref is lightweight; release tags must be annotated (git tag -a)"
    [ "$ref" = "v$version" ] && final=1
    [ "$ref" = "v$version" ] || echo "note: tag $ref is not v$version; this is a candidate build"
fi
short=$(git rev-parse --short=12 "$commit")
out=${out:-$root/dist/egui-release-$version}
case $out in /*) ;; *) out=$PWD/$out ;; esac
[ ! -e "$out" ] || [ -z "$(ls -A "$out" 2>/dev/null)" ] || die "$out exists and is not empty; choose another --out or remove it"
mkdir -p "$out/logs"

step "revision: $commit ($mode), tree $tree"

# --- 2. a clean worktree of exactly that commit ----------------------------------------------------------
work=$(mktemp -d "${TMPDIR:-/tmp}/omatree-egui-release.XXXXXX")
cleanup() { git -C "$root" worktree remove --force "$work/src" >/dev/null 2>&1 || true; rm -rf "$work"; }
trap cleanup EXIT
git worktree add --detach -q "$work/src" "$commit"
cd "$work/src"
[ "$(git rev-parse HEAD)" = "$commit" ] || die "worktree is not at $commit"
[ -z "$(git status --porcelain --ignored)" ] || die "the checkout of $commit is not clean"

# --- 3. the version is the intended one, everywhere ------------------------------------------------------
step "version $version"
ver_of() { sed -n 's/^version = "\(.*\)"/\1/p' "$1" | head -1; }
for f in Cargo.toml crates/core/Cargo.toml crates/editor/Cargo.toml crates/egui-app/Cargo.toml; do
    [ "$(ver_of "$f")" = "$version" ] || die "$f says $(ver_of "$f"), not $version"
done
id=io.github.Dorotabro.OmaTreeEgui
meta=packaging/egui/linux/$id.metainfo.xml
meta_ver=$(python3 - "$meta" <<'PY'
import sys, xml.etree.ElementTree as ET
print(ET.parse(sys.argv[1]).getroot().find("releases/release").get("version"))
PY
)
[ "$meta_ver" = "$version" ] || die "AppStream's newest release is $meta_ver, not $version"
deb_ver=$(sed -n '1s/^omatree-egui (\(.*\)) .*/\1/p' packaging/egui/debian/debian/changelog)
[ "$deb_ver" = "$version" ] || die "the Debian changelog's newest entry is $deb_ver, not $version"
cargo metadata --locked --offline --format-version 1 >/dev/null 2>&1 \
    || cargo metadata --locked --format-version 1 >/dev/null 2>&1 \
    || die "Cargo.lock is not consistent with the manifests (cargo metadata --locked failed)"
[ -z "$(git status --porcelain)" ] || die "reading the metadata changed a tracked file (Cargo.lock out of date?)"
echo "ok: manifests, Cargo.lock, AppStream and the Debian changelog all say $version"

if [ "$source_checks" = 1 ]; then
    step "source verification (packaging/egui/tests/verify-sources.sh)"
    packaging/egui/tests/verify-sources.sh > "$out/logs/verify-sources.log" 2>&1 \
        || { tail -n 20 "$out/logs/verify-sources.log" >&2; die "source verification failed (log: $out/logs/verify-sources.log)"; }
    tail -n 1 "$out/logs/verify-sources.log"
fi

# --- 4. build with the egui packaging scripts (all use --locked) -----------------------------------------
export CARGO_NET_OFFLINE=${CARGO_NET_OFFLINE:-false}
arch_pkg=omatree-egui-$version-1-x86_64.pkg.tar.zst
deb_pkg=omatree-egui_${version}_amd64.deb
img_pkg=OmaTree-egui-$version-x86_64.AppImage
build() {   # build NAME SCRIPT FILE
    step "build $1"
    "$2" > "$out/logs/build-$1.log" 2>&1 \
        || { tail -n 25 "$out/logs/build-$1.log" >&2; die "$1 build failed (log: $out/logs/build-$1.log)"; }
    [ -f "dist/$3" ] || die "$1 build finished but dist/$3 is missing"
    echo "built dist/$3"
}
build debian packaging/egui/debian/build-package.sh "$deb_pkg"
build appimage packaging/egui/appimage/build-package.sh "$img_pkg"
build arch packaging/egui/arch/build-package.sh "$arch_pkg"

# --- 5. verify what was built ----------------------------------------------------------------------------
for p in "$arch_pkg" "$deb_pkg" "$img_pkg"; do
    step "verify $p"
    packaging/egui/verify-package.sh "dist/$p" > "$out/logs/verify-$p.log" 2>&1 \
        || { tail -n 20 "$out/logs/verify-$p.log" >&2; die "verification of $p failed (log: $out/logs/verify-$p.log)"; }
    tail -n 1 "$out/logs/verify-$p.log"
done

# --- 6. collect, checksum, record provenance -------------------------------------------------------------
step "collect"
for p in "$arch_pkg" "$deb_pkg" "$img_pkg"; do cp -p "dist/$p" "$out/$p"; done
sums=SHA256SUMS; [ "$final" = 1 ] || sums=SHA256SUMS.candidate
( cd "$out" && sha256sum "$arch_pkg" "$deb_pkg" "$img_pkg" > "$sums" && sha256sum --check "$sums" )

deb_control=$(bsdtar -xOf "$out/$deb_pkg" 'control.tar*' 2>/dev/null | bsdtar -xOf - ./control 2>/dev/null | sed -n 's/^\(Package\|Version\|Architecture\): /  deb \1: /p' || true)
arch_info=$(bsdtar -xOf "$out/$arch_pkg" .PKGINFO | sed -n 's/^\(pkgname\|pkgver\|arch\) = /  arch \1: /p')
{
    echo "OmaTree egui $version: build provenance"
    echo "kind:            $([ "$final" = 1 ] && echo "FINAL build from tag $ref" || echo "CANDIDATE build (not a release artifact)")"
    echo "source:          $mode"
    echo "commit:          $commit"
    echo "tree:            $tree"
    echo "commit date:     $(git log -1 --format=%cI "$commit")"
    echo "built (UTC):     $(date -u +%Y-%m-%dT%H:%M:%SZ)"
    echo "host:            $(uname -sr), glibc $(getconf GNU_LIBC_VERSION | cut -d' ' -f2)"
    echo "host rust:       $(rustc --version 2>/dev/null || echo none) / $(cargo --version 2>/dev/null || echo none)"
    echo "container rust:  $(sed -n 's/.*--default-toolchain \([0-9.]*\).*/\1/p' packaging/egui/debian/Dockerfile | head -1) (Debian image), $(sed -n 's/.*--default-toolchain \([0-9.]*\).*/\1/p' packaging/egui/appimage/Dockerfile | head -1) (AppImage image)"
    echo "docker:          $(docker version --format '{{.Server.Version}}' 2>/dev/null || echo unknown)"
    echo "build images:    $(docker images --no-trunc --format '{{.Repository}} {{.ID}}' 2>/dev/null | grep '^omatree-egui-build' | tr '\n' ';')"
    echo "makepkg:         $(makepkg --version 2>/dev/null | head -n 1)"
    echo "dependencies:    cargo build --locked from Cargo.lock ($(sha256sum Cargo.lock | cut -c1-16)…), crates.io only"
    echo "packages:"
    echo "$arch_info"; echo "$deb_control"; echo "  appimage version: $version (from the file name)"
    echo "files:"
    ( cd "$out" && for p in "$arch_pkg" "$deb_pkg" "$img_pkg"; do printf '  %s  %s  %s bytes\n' "$(sha256sum "$p" | cut -d' ' -f1)" "$p" "$(stat -c %s "$p")"; done )
    [ "$final" = 1 ] || echo "note: hashes of a candidate are NOT release hashes; a final build comes from the tag and is checksummed again from the files attached to the release."
} > "$out/PROVENANCE.txt"

step "done"
cat "$out/PROVENANCE.txt"
echo
echo "artifacts and records: $out"
echo "Nothing was tagged, pushed, uploaded or published. Next steps are in docs/RELEASING.md."
