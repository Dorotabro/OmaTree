#!/bin/bash
# Checks a built egui OmaTree Linux package without installing it.
#
#   packaging/egui/verify-package.sh dist/omatree-egui-0.2.0-1-x86_64.pkg.tar.zst
#   packaging/egui/verify-package.sh dist/omatree-egui_0.2.0_amd64.deb
#   packaging/egui/verify-package.sh dist/OmaTree-egui-0.2.0-x86_64.AppImage
#
# Picks the verifier by file type; each shares the checks in packaging/egui/linux/verify-common.sh.
# Exit status is non-zero if any check failed. (The Qt packages' verifier is packaging/verify-package.sh.)
set -uo pipefail

pkg=${1:?usage: verify-package.sh PACKAGE}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

case $pkg in
    *omatree-egui*.pkg.tar.zst) exec "$here/arch/verify-arch.sh" "$pkg" ;;
    *omatree-egui*.deb)         exec "$here/debian/verify-deb.sh" "$pkg" ;;
    *OmaTree-egui*.AppImage)    exec "$here/appimage/verify-appimage.sh" "$pkg" ;;
    *) echo "verify-package.sh: not an egui package name: $pkg" >&2; exit 2 ;;
esac
