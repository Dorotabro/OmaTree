#!/bin/bash
# Checks a built egui OmaTree AppImage. Run through packaging/egui/verify-package.sh.
#
# Extracts it with its own --appimage-extract (which needs no FUSE) and checks the AppDir; then, only
# if FUSE is usable here, runs the image itself and says so, so "extracted" and "mounted and
# executed" are never confused. Exit status is non-zero if any check failed.
set -uo pipefail

img=$(realpath "${1:?usage: verify-appimage.sh OmaTree-egui-VERSION-x86_64.AppImage}")
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../linux/verify-common.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "== image"
ls -l "$img" | awk '{print "size  " $5 " bytes"}'
check "ELF with the type 2 AppImage magic (AI\\002)" bash -c "[ \"\$(dd if='$img' bs=1 skip=8 count=3 2>/dev/null | od -An -c | tr -d ' ')\" = 'AI002' ] || [ \"\$(dd if='$img' bs=1 skip=8 count=3 2>/dev/null | xxd -p)\" = '414902' ]"
check "named OmaTree-egui-VERSION-x86_64.AppImage" bash -c "basename '$img' | grep -Eq '^OmaTree-egui-[0-9]+\.[0-9]+\.[0-9]+-x86_64\.AppImage\$'"
check "the version in the name is the egui crate version" bash -c "basename '$img' | grep -q -- '-$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/crates/egui-app/Cargo.toml" | head -1)-'"
mkdir -p "$tmp/x"
(cd "$tmp/x" && "$img" --appimage-extract >/dev/null 2>&1)
root=$tmp/x/squashfs-root
check "extracted without FUSE (--appimage-extract)" test -d "$root"

echo "== AppDir"
(cd "$root" && find . -not -type d | LC_ALL=C sort) | sed 's/^/file  /'
check "AppRun is executable" test -x "$root/AppRun"
check "AppRun runs the egui binary, quoting its arguments" bash -c "grep -q 'usr/bin/omatree-egui\" \"\$@\"' '$root/AppRun'"
check "AppRun refers only to omatree-egui, never the Qt binary" bash -c "! grep -Eq 'usr/bin/omatree([^-]|\$)' '$root/AppRun'"
check "top-level desktop entry is the egui one" cmp -s <(grep -v '^X-AppImage' "$root/$id.desktop") "$root/usr/share/applications/$id.desktop"
check "top-level icon $id.png and .DirIcon are 256x256 PNGs" bash -c "file '$root/$id.png' | grep -q '256 x 256' && file -L '$root/.DirIcon' | grep -q '256 x 256'"
check "X-AppImage-Version is the crate version" grep -qx "X-AppImage-Version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/crates/egui-app/Cargo.toml" | head -1)" "$root/$id.desktop"
verify_files "$root"
check "nothing but the one executable, in usr/bin" bash -c "[ \"\$(ls '$root/usr/bin')\" = '$bin' ]"
check "no shared libraries bundled (display, GL and font libraries are the host's)" bash -c "! find '$root' -name '*.so*' | grep -q ."
verify_binary "$root/usr/bin/$bin"
verify_metadata "$root"
verify_licences "$root"

echo "== packaged binary (from the extracted AppDir, through AppRun)"
verify_launch "$root/AppRun"

echo "== the image itself"
if [ -c /dev/fuse ] && have fusermount; then
    out=$("$img" --appimage-version 2>&1 | head -1)
    check "the image mounts and answers --appimage-version ($out)" test -n "$out"
    if [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then verify_launch "$img"; else skip "mounted launch: no display"; fi
else
    skip "FUSE is not usable here: the image was extracted, not mounted and executed"
fi
verify_done
