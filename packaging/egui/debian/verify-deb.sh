#!/bin/bash
# Checks a built egui OmaTree .deb without installing it. Run through
# packaging/egui/verify-package.sh. The real installation test (apt, a clean Debian/Ubuntu system,
# a display, purge) is packaging/egui/debian/test-install.sh.
set -uo pipefail

pkg=${1:?usage: verify-deb.sh PACKAGE.deb}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
licdir=usr/share/doc/omatree-egui
want_license_texts=0
. "$here/../linux/verify-common.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/pkg" "$tmp/ctl" "$tmp/root"
bsdtar -xf "$pkg" -C "$tmp/pkg"
bsdtar -xf "$tmp"/pkg/control.tar.* -C "$tmp/ctl"
bsdtar -xf "$tmp"/pkg/data.tar.* -C "$tmp/root"
root=$tmp/root

echo "== package"
ls -l "$pkg" | awk '{print "size  compressed " $5 " bytes"}'
sed 's/^/info  /' "$tmp/ctl/control"
(cd "$root" && find . -type f | sort) | sed 's/^/file  /'

echo "== control"
ver=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/crates/egui-app/Cargo.toml" | head -1)
for f in 'Package: omatree-egui' "Version: $ver" 'Architecture: amd64' 'Section: utils' 'Priority: optional' \
    'Homepage: https://github.com/Dorotabro/OmaTree'; do
    check "$f" grep -qx "$f" "$tmp/ctl/control"
done
deps=$(grep '^Depends:' "$tmp/ctl/control")
for d in libwayland-client0 libwayland-egl1 libxkbcommon0 libxkbcommon-x11-0 libegl1 libgl1 libx11-6 libxcb1 \
    libxcursor1 libxi6 libfontconfig1 libdbus-1-3 libc6; do
    check "Depends has $d" bash -c "echo '$deps' | grep -q '$d'"
done
check "Depends has no Qt, and no legacy omatree package" bash -c "! echo '$deps' | grep -Eqi 'qt|qml|omatree( |,|\$|\()'"
check "no Conflicts/Replaces/Provides touching the Qt package" bash -c "! grep -Eq '^(Conflicts|Replaces|Provides|Breaks):.*omatree([^-]|\$)' '$tmp/ctl/control'"
check "no maintainer scripts (triggers handle caches)" bash -c "! ls '$tmp'/ctl | grep -Eq '^(preinst|postinst|prerm|postrm)$'"

echo "== contents"
# Debian compresses the larger documents; unpack them to check their content.
find "$root/usr/share/doc" -name '*.gz' -exec gunzip {} +
verify_files "$root"
check "has /usr/share/doc/omatree-egui/copyright" test -f "$root/usr/share/doc/omatree-egui/copyright"
check "copyright is DEP-5 and names both licences" bash -c "grep -q '^Format: https://www.debian.org/doc/packaging-manuals/copyright-format/1.0/' '$root/usr/share/doc/omatree-egui/copyright' && grep -q '^License: MIT' '$root/usr/share/doc/omatree-egui/copyright' && grep -q '^License: Apache-2.0' '$root/usr/share/doc/omatree-egui/copyright'"
check "changelog is installed" bash -c "ls '$root'/usr/share/doc/omatree-egui/changelog* >/dev/null 2>&1"
check "no /usr/share/licenses (Debian uses doc/NAME)" test ! -e "$root/usr/share/licenses"
check "no shared libraries at all" bash -c "! find '$root' -name '*.so*' | grep -q ."
verify_binary "$root/usr/bin/$bin"
verify_metadata "$root"
verify_licences "$root"

echo "== packaged binary"
verify_launch "$root/usr/bin/$bin"
verify_done
