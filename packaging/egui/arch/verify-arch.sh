#!/bin/bash
# Checks a built egui OmaTree pacman package without installing it. Run through
# packaging/egui/verify-package.sh.
#
# Extracts it to a temporary directory and checks the file list, the dependencies, the dynamic
# links, the absence of build paths, the desktop / AppStream / MIME metadata, the icons, the
# licence files and (with a display) a launch of the packaged binary. A validator that is not
# installed is reported as skipped. Exit status is non-zero if any check failed.
set -uo pipefail

pkg=${1:?usage: verify-arch.sh PACKAGE.pkg.tar.zst}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../linux/verify-common.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
root=$tmp/root
mkdir -p "$root"
bsdtar -xf "$pkg" -C "$root"

echo "== package"
ls -l "$pkg" | awk '{print "size  compressed " $5 " bytes"}'
grep -E '^(pkgname|pkgver|arch|size|license|depend|optdepend) ' "$root/.PKGINFO" | sed 's/^/info  /'
check "pkgname is omatree-egui" grep -qx 'pkgname = omatree-egui' "$root/.PKGINFO"
check "arch is x86_64" grep -qx 'arch = x86_64' "$root/.PKGINFO"
check "pkgver matches the egui crate version" grep -qx "pkgver = $(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/crates/egui-app/Cargo.toml" | head -1)-1" "$root/.PKGINFO"
check "no dependency on Qt" bash -c "! grep -E '^depend = ' '$root/.PKGINFO' | grep -qi qt"
check "no dependency on the legacy omatree package, and no conflict with it" bash -c "! grep -Eq '^(depend|conflict|replaces|provides) = omatree($|[<>=])' '$root/.PKGINFO'"

echo "== contents"
(cd "$root" && find . -type f ! -name '.*' | sort) | sed 's/^/file  /'
verify_files "$root"
check "no shared libraries at all" bash -c "! find '$root' -name '*.so*' | grep -q ."

verify_binary "$root/usr/bin/$bin"
check "no unresolved libraries (ldd)" bash -c "! ldd '$root/usr/bin/$bin' | grep -q 'not found'"
verify_metadata "$root"
verify_licences "$root"

echo "== packaged binary"
verify_launch "$root/usr/bin/$bin"
verify_done
