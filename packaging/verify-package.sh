#!/bin/bash
# Checks a built OmaTree pacman package without installing it.
#
#   packaging/verify-package.sh dist/omatree-0.1.0-1-x86_64.pkg.tar.zst
#
# Extracts it to a temporary directory and checks the file list, the
# dependencies, the dynamic links, the absence of build paths, the desktop /
# AppStream / MIME metadata, the icons, and a headless launch of the packaged
# binary. A validator that is not installed is reported as skipped. Exit
# status is non-zero if any check failed.
set -uo pipefail

pkg=${1:?usage: verify-package.sh PACKAGE.pkg.tar.zst}
id=io.github.Dorotabro.OmaTree
fails=0
ok()   { echo "ok    $*"; }
bad()  { echo "FAIL  $*"; fails=$((fails + 1)); }
skip() { echo "skip  $*"; }
have() { command -v "$1" >/dev/null 2>&1; }
check() { local what=$1; shift; if "$@"; then ok "$what"; else bad "$what"; fi; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
root=$tmp/root
mkdir -p "$root"
bsdtar -xf "$pkg" -C "$root"

echo "== package"
ls -l "$pkg" | awk '{print "size  compressed " $5 " bytes"}'
grep -E '^(pkgname|pkgver|arch|size|license|depend|optdepend) ' "$root/.PKGINFO" | sed 's/^/info  /'

echo "== contents"
(cd "$root" && find . -type f ! -name '.*' | sort) | sed 's/^/file  /'
for f in usr/bin/omatree usr/share/applications/$id.desktop \
    usr/share/mime/packages/$id.xml usr/share/metainfo/$id.metainfo.xml \
    usr/share/icons/hicolor/scalable/apps/$id.svg \
    usr/share/licenses/omatree/LICENSE-MIT usr/share/licenses/omatree/LICENSE-APACHE \
    usr/share/licenses/omatree/THIRD_PARTY.md; do
    check "has $f" test -f "$root/$f"
done
check "binary is executable" test -x "$root/usr/bin/omatree"
check "no Qt libraries bundled" bash -c "! find '$root' -name 'libQt*' | grep -q ."
check "no shared libraries at all" bash -c "! find '$root' -name '*.so*' | grep -q ."
check "nothing from the source tree or build" bash -c "! find '$root' \( -name '*.rs' -o -name '*.qml' -o -name Cargo.toml -o -name '*.omatree' -o -name '*.db' -o -path '*/.git*' -o -name target \) | grep -q ."

echo "== dynamic links"
bin=$root/usr/bin/omatree
readelf -d "$bin" | grep NEEDED | sed 's/.*\[\(.*\)\]/need  \1/'
check "Qt Core, Gui and Qml are linked dynamically" bash -c "readelf -d '$bin' | grep -q libQt6Core && readelf -d '$bin' | grep -q libQt6Gui && readelf -d '$bin' | grep -q libQt6Qml"
check "SQLite is bundled, not linked" bash -c "! readelf -d '$bin' | grep -qi sqlite"
check "no unresolved libraries (ldd)" bash -c "! ldd '$bin' | grep -q 'not found'"
check "test hook is absent from the release binary" bash -c "! strings -a '$bin' | grep -q OMATREE_TEST_QML"

echo "== build paths"
check "no /home path in the binary" bash -c "! strings -a '$bin' | grep -q '/home/'"
check "no /tmp or repository path in the binary" bash -c "! strings -a '$bin' | grep -Eq '/tmp/|Antigravity|/dist/arch-build'"
check "no user name in the binary" bash -c "! strings -a '$bin' | grep -qF '$USER'"

echo "== metadata"
if have desktop-file-validate; then check "desktop-file-validate" desktop-file-validate "$root/usr/share/applications/$id.desktop"; else skip "desktop-file-validate not installed"; fi
check "Exec uses %f" grep -qx 'Exec=omatree %f' "$root/usr/share/applications/$id.desktop"
if have appstreamcli; then check "appstreamcli validate" appstreamcli validate --no-net "$root/usr/share/metainfo/$id.metainfo.xml"; else skip "appstreamcli not installed"; fi
check "MIME XML is well formed" xmllint --noout "$root/usr/share/mime/packages/$id.xml"
check "MIME has no magic matching" bash -c "! grep -q '<magic' '$root/usr/share/mime/packages/$id.xml'"
if have update-mime-database; then
    mkdir -p "$tmp/mime/packages"
    cp "$root/usr/share/mime/packages/$id.xml" "$tmp/mime/packages/"
    update-mime-database "$tmp/mime" >/dev/null 2>&1
    check "the temporary MIME database has *.omatree" grep -q 'application/x-omatree:\*.omatree' "$tmp/mime/globs2"
    check "no SQLite magic claimed" bash -c "! grep -q omatree '$tmp/mime/magic'"
else
    skip "update-mime-database not installed"
fi

echo "== icons"
check "the SVG has no external references" bash -c "! grep -Eq 'href=\"(https?:|file:|[^#d])' '$root/usr/share/icons/hicolor/scalable/apps/$id.svg'"
check "the SVG needs no fonts" bash -c "! grep -Eq '<text|font-family' '$root/usr/share/icons/hicolor/scalable/apps/$id.svg'"
for size in 32 64 128 256 512; do
    f=$root/usr/share/icons/hicolor/${size}x${size}/apps/$id.png
    if have identify; then
        check "icon ${size}x${size} decodes at ${size}x${size}" bash -c "[ \"\$(identify -format '%wx%h' '$f')\" = '${size}x${size}' ]"
    else
        check "icon ${size}x${size} is a PNG" bash -c "file '$f' | grep -q 'PNG image data, $size x $size'"
    fi
done
if have rsvg-convert; then check "the SVG renders" bash -c "rsvg-convert -w 64 '$root/usr/share/icons/hicolor/scalable/apps/$id.svg' >/dev/null"; else skip "rsvg-convert not installed"; fi

echo "== packaged binary, offscreen"
home=$tmp/home
mkdir -p "$home"
log=$tmp/launch.log
HOME=$home XDG_STATE_HOME=$home/state XDG_CONFIG_HOME=$home/config XDG_DATA_HOME=$home/data \
    QT_QPA_PLATFORM=offscreen QT_FORCE_STDERR_LOGGING=1 \
    timeout 6 "$bin" "$home/launch-test.omatree" >"$log" 2>&1
status=$?
check "it ran until stopped (exit $status, 124 = timeout)" test "$status" -eq 124
check "it created the notebook it was given with %f" test -s "$home/launch-test.omatree"
cat "$log" | sed 's/^/log   /'
check "no QML warnings, binding loops or errors" bash -c "! grep -Eiq 'qml|binding loop|warning|error|TypeError' '$log'"

echo
if [ "$fails" -eq 0 ]; then echo "all checks passed"; else echo "$fails check(s) FAILED"; fi
exit "$fails"
