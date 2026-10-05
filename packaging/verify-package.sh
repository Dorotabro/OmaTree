#!/bin/bash
# Checks a built OmaTree Linux package without installing it.
#
#   packaging/verify-package.sh dist/omatree-0.1.0-1-x86_64.pkg.tar.zst
#   packaging/verify-package.sh dist/omatree_0.1.0_amd64.deb
#   packaging/verify-package.sh dist/OmaTree-0.1.0-x86_64.AppImage
#
# Picks the verifier by file type; each one shares the checks in
# packaging/linux/verify-common.sh. Exit status is non-zero if any check failed.
set -uo pipefail

pkg=${1:?usage: verify-package.sh PACKAGE}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)

case $pkg in
    *.pkg.tar.zst) exec "$here/arch/verify-arch.sh" "$pkg" ;;
    *.deb)         exec "$here/debian/verify-deb.sh" "$pkg" ;;
    *.AppImage)    exec "$here/appimage/verify-appimage.sh" "$pkg" ;;
    *) echo "verify-package.sh: don't know how to verify $pkg" >&2; exit 2 ;;
esac
