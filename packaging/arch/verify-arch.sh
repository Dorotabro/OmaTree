#!/bin/bash
# Checks a built OmaTree pacman package without installing it. Run through
# packaging/verify-package.sh.
#
# Extracts it to a temporary directory and checks the file list, the
# dependencies, the dynamic links, the absence of build paths, the desktop /
# AppStream / MIME metadata, the icons, and a headless launch of the packaged
# binary. A validator that is not installed is reported as skipped. Exit
# status is non-zero if any check failed.
set -uo pipefail

pkg=${1:?usage: verify-arch.sh PACKAGE.pkg.tar.zst}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../linux/verify-common.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
root=$tmp/root
mkdir -p "$root"
bsdtar -xf "$pkg" -C "$root"
bin=$root/usr/bin/omatree

echo "== package"
ls -l "$pkg" | awk '{print "size  compressed " $5 " bytes"}'
grep -E '^(pkgname|pkgver|arch|size|license|depend|optdepend) ' "$root/.PKGINFO" | sed 's/^/info  /'

echo "== contents"
(cd "$root" && find . -type f ! -name '.*' | sort) | sed 's/^/file  /'
verify_files "$root"
for f in usr/share/licenses/omatree/LICENSE-MIT usr/share/licenses/omatree/LICENSE-APACHE \
    usr/share/licenses/omatree/THIRD_PARTY.md; do
    check "has $f" test -f "$root/$f"
done
check "no Qt libraries bundled" bash -c "! find '$root' -name 'libQt*' | grep -q ."
check "no shared libraries at all" bash -c "! find '$root' -name '*.so*' | grep -q ."

verify_binary "$bin"
check "no unresolved libraries (ldd)" bash -c "! ldd '$bin' | grep -q 'not found'"
verify_metadata "$root"

echo "== packaged binary, offscreen"
verify_launch "$bin"
verify_done
