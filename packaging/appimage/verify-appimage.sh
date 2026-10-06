#!/bin/bash
# Checks a built OmaTree AppImage without installing it. Run through
# packaging/verify-package.sh. Works on a copy in a clean temporary directory.
#
# Checks the AppDir inside (files, bundled Qt, plugins, QML modules, library
# policy, licences), measures the ABI the binaries require, and launches the
# AppImage headless: directly, with the extract-and-run fallback, and with a
# notebook path.
set -uo pipefail

src=$(realpath "${1:?usage: verify-appimage.sh OmaTree-VERSION-x86_64.AppImage}")
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
. "$here/../linux/verify-common.sh"

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
cp "$src" "$tmp/OmaTree.AppImage"
chmod +x "$tmp/OmaTree.AppImage"
app=$tmp/OmaTree.AppImage
(cd "$tmp" && ./OmaTree.AppImage --appimage-extract >/dev/null)
root=$tmp/squashfs-root
bin=$root/usr/bin/omatree
lib=$root/usr/lib

echo "== package"
ls -l "$src" | awk '{print "size  compressed " $5 " bytes"}'
echo "size  extracted $(du -sk "$root" | cut -f1) KiB"
sha256sum "$src" | sed 's/^/sha   /'

echo "== AppDir"
for f in AppRun .DirIcon $id.desktop $id.svg usr/share/doc/omatree/QT-NOTICE.md \
    usr/share/licenses/omatree/{LICENSE-MIT,LICENSE-APACHE,THIRD_PARTY.md,Qt-LGPL-3.0.txt,Qt-GPL-3.0.txt,ICU-73.2-LICENSE.txt} \
    usr/share/licenses/omatree/third-party/BUNDLED-LIBRARIES.txt; do
    check "has $f" test -e "$root/$f"
done
verify_files "$root"
# appimagetool/linuxdeploy add exactly one line, X-AppImage-Version.
check "root desktop entry is the canonical one (plus X-AppImage-Version)" bash -c "diff <(grep -v '^X-AppImage-Version=' '$root/$id.desktop') '$here/../linux/$id.desktop'"
check "root icon is the canonical logo" cmp -s "$root/$id.svg" "$here/../../assets/logo/omatree-logo.svg"
ver=$(sed -n 's/^version = "\(.*\)"/\1/p' "$here/../../Cargo.toml" | head -1)
check "version $ver in the file name" bash -c "[[ '$(basename "$src")' == OmaTree-$ver-x86_64.AppImage ]]"

echo "== Qt"
qtver=$(strings -a "$lib/libQt6Core.so.6" | grep -Eo 'Qt 6\.[0-9]+\.[0-9]+' | head -1)
echo "info  bundled $qtver"
check "bundled Qt is 6.8.3" test "$qtver" = "Qt 6.8.3"
echo "info  Qt modules: $(ls "$lib" | grep '^libQt6' | sed 's/^libQt6//; s/\.so.*//' | tr '\n' ' ')"
echo "info  platform plugins: $(ls "$root/usr/plugins/platforms" | sed 's/^libq//; s/\.so//' | tr '\n' ' ')"
echo "info  QML modules: $(cd "$root/usr/qml" && find . -name qmldir | sed 's#^\./##; s#/qmldir$##' | tr '\n' ' ')"
for m in Core Gui Qml Quick QuickControls2 QuickControls2Basic QuickTemplates2 QuickLayouts QuickDialogs2; do
    check "libQt6$m is bundled" test -e "$lib/libQt6$m.so.6"
done
for p in xcb wayland-generic wayland-egl offscreen; do
    check "platform plugin $p" test -e "$root/usr/plugins/platforms/libq$p.so"
done
for d in wayland-shell-integration wayland-decoration-client wayland-graphics-integration-client; do
    check "Wayland plugin directory $d" bash -c "ls '$root/usr/plugins/$d'/*.so >/dev/null"
done
for m in QtQuick QtQuick/Controls QtQuick/Controls/Basic QtQuick/Templates QtQuick/Layouts QtQuick/Dialogs QtQuick/Window; do
    check "QML module $m (qmldir and plugin)" bash -c "test -f '$root/usr/qml/$m/qmldir'"
done
check "no style other than Basic is bundled" bash -c "! ls '$root/usr/qml/QtQuick/Controls' | grep -Eq '^(Material|Universal|Fusion|Imagine|FluentWinUI3)$'"
check "Qt is dynamically linked: the executable has no Qt code inside" bash -c "! strings -a '$bin' | grep -q 'QQmlApplicationEngine::QQmlApplicationEngine' && readelf -d '$bin' | grep -q 'libQt6Core.so.6'"
check "every bundled Qt module is named in QT-NOTICE.md" bash -c "for l in '$lib'/libQt6*.so.6; do n=\$(basename \$l .so.6); n=\${n#libQt6}; grep -q \"\$n\" '$root/usr/share/doc/omatree/QT-NOTICE.md' || { echo \"missing \$n\"; exit 1; }; done"
check "QT-NOTICE.md names the source location" grep -q 'download.qt.io/archive/qt/6.8/6.8.3' "$root/usr/share/doc/omatree/QT-NOTICE.md"

echo "== library policy (nothing from the host system's low-level stack)"
for l in libc.so libm.so libpthread.so libdl.so librt.so ld-linux libstdc++ libgcc_s libGL libEGL libGLX libOpenGL libgbm libdrm libvulkan libX11 libxcb.so libwayland-client libwayland-server libxkbcommon libfontconfig libfreetype libasound libudev; do
    check "no $l bundled" bash -c "! ls '$lib' | grep -q '^${l//./\\.}'"
done
check "no GPU driver / Mesa files" bash -c "! find '$root' -iname '*mesa*' -o -iname '*_dri.so' -o -iname 'nvidia*' | grep -q ."

echo "== ABI requirements (maximum symbol version needed)"
maxver() { # PREFIX over all ELF files: highest PREFIX_x.y.z found
    find "$root" -type f \( -name '*.so*' -o -path '*/usr/bin/*' \) -print0 | xargs -0 objdump -T 2>/dev/null \
        | grep -o "$1_[0-9.]*" | sed "s/^$1_//" | sort -uV | tail -1
}
glibc=$(maxver GLIBC); glibcxx=$(maxver GLIBCXX); cxxabi=$(maxver CXXABI)
echo "abi   GLIBC   $glibc"
echo "abi   GLIBCXX $glibcxx"
echo "abi   CXXABI  $cxxabi"
echo "abi   OmaTree executable alone: GLIBC $(objdump -T "$bin" | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -uV | tail -1), GLIBCXX $(objdump -T "$bin" | grep -o 'GLIBCXX_[0-9.]*' | sed 's/GLIBCXX_//' | sort -uV | tail -1)"
check "GLIBC requirement is at most 2.35 (the build baseline)" bash -c "[ \"\$(printf '%s\n' 2.35 '$glibc' | sort -V | tail -1)\" = 2.35 ]"
check "no unresolved libraries beyond the system ones" bash -c "! LD_LIBRARY_PATH='$lib' ldd '$bin' | grep 'not found'"

verify_binary "$bin"

echo "== no developer or build paths anywhere in the AppImage"
check "no home directory, user name or repository path in any file" bash -c "! grep -rIl -e '/home/$USER' -e 'Antigravity' -e '/tmp/build' -e '/build/home' '$root' | grep -q ."
check "no such path in the binaries either" bash -c "! find '$root' -type f \( -name '*.so*' -o -path '*/bin/*' \) -print0 | xargs -0 strings -a | grep -Eq '/home/$USER|Antigravity|/tmp/build'"

verify_metadata "$root"

echo "== platform theme hook"
check "the hook selects the bundled portal theme" grep -q 'QT_QPA_PLATFORMTHEME=xdgdesktopportal' "$root/apprun-hooks/linuxdeploy-plugin-qt-hook.sh"
check "the portal theme plugin is bundled" test -e "$root/usr/plugins/platformthemes/libqxdgdesktopportal.so"
check "no gtk theme plugin is bundled (it would load the host's GTK against bundled glib)" bash -c "! ls '$root/usr/plugins/platformthemes' | grep -q gtk"

echo "== launch (offscreen)"
echo "info  AppRun via the extracted tree:"
verify_launch "$root/AppRun"
echo "info  AppRun with a host that asks for a platform theme the bundle lacks (QT_QPA_PLATFORMTHEME=gtk3):"
QT_QPA_PLATFORMTHEME=gtk3 verify_launch "$root/AppRun"
echo "info  the AppImage itself (needs FUSE here):"
if "$app" --appimage-version >/dev/null 2>&1 && [ -e /dev/fuse ]; then verify_launch "$app"; else skip "no FUSE available: direct run not tested"; fi
if [ -n "${DISPLAY:-}" ]; then
    echo "info  the xcb platform on this machine's X server / XWayland (DISPLAY=$DISPLAY):"
    LAUNCH_PLATFORM=xcb verify_launch "$root/AppRun"
else
    skip "no DISPLAY: xcb platform not launched"
fi
echo "info  with APPIMAGE_EXTRACT_AND_RUN=1:"
APPIMAGE_EXTRACT_AND_RUN=1 verify_launch "$app"
verify_done
