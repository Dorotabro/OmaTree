#!/bin/bash
# Checks a built OmaTree .deb without installing it. Run through
# packaging/verify-package.sh. The real installation test (apt, a clean
# Debian/Ubuntu system, purge) is packaging/debian/test-install.sh.
set -uo pipefail

pkg=${1:?usage: verify-deb.sh PACKAGE.deb}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../linux/verify-common.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/pkg" "$tmp/ctl" "$tmp/root"
bsdtar -xf "$pkg" -C "$tmp/pkg"
bsdtar -xf "$tmp"/pkg/control.tar.* -C "$tmp/ctl"
bsdtar -xf "$tmp"/pkg/data.tar.* -C "$tmp/root"
root=$tmp/root
bin=$root/usr/bin/omatree

echo "== package"
ls -l "$pkg" | awk '{print "size  compressed " $5 " bytes"}'
sed 's/^/info  /' "$tmp/ctl/control"
(cd "$root" && find . -type f | sort) | sed 's/^/file  /'

echo "== control"
for f in 'Package: omatree' 'Version: 0.1.0' 'Architecture: amd64' 'Section: utils' 'Priority: optional' \
    'Homepage: https://github.com/Dorotabro/OmaTree'; do
    check "$f" grep -qx "$f" "$tmp/ctl/control"
done
deps=$(grep '^Depends:' "$tmp/ctl/control")
for d in libqt6core6 libqt6gui6 libqt6qml6 qml6-module-qtquick qml6-module-qtquick-controls \
    qml6-module-qtquick-templates qml6-module-qtquick-layouts qml6-module-qtquick-dialogs; do
    check "Depends has $d" bash -c "echo '$deps' | grep -q '$d'"
done
check "no maintainer scripts (triggers handle caches)" bash -c "! ls '$tmp'/ctl | grep -Eq '^(preinst|postinst|prerm|postrm)$'"

echo "== contents"
verify_files "$root"
check "has /usr/share/doc/omatree/copyright" test -f "$root/usr/share/doc/omatree/copyright"
check "has /usr/share/doc/omatree/THIRD_PARTY.md.gz" test -f "$root/usr/share/doc/omatree/THIRD_PARTY.md.gz"
check "no /usr/share/licenses (Debian uses doc/copyright)" test ! -e "$root/usr/share/licenses"
check "no Qt libraries bundled" bash -c "! find '$root' -name 'libQt*' | grep -q ."
check "no shared libraries at all" bash -c "! find '$root' -name '*.so*' | grep -q ."
verify_binary "$bin"
verify_metadata "$root"

echo "== packaged binary, offscreen (on THIS machine's Qt; test-install.sh uses the real one)"
verify_launch "$bin"
verify_done
