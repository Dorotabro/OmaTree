#!/bin/bash
# Runs INSIDE the build container (started by build-package.sh), from the root
# of the unpacked source snapshot. Builds OmaTree against the official Qt in
# $QT_ROOT, assembles the AppDir, bundles Qt with linuxdeploy and writes the
# AppImage and the AppDir listing to the directory given as $1.
set -euo pipefail

outdir=${1:?usage: build-in-container.sh OUTDIR}
src=$PWD
id=io.github.Dorotabro.OmaTree
appdir=/tmp/build/OmaTree.AppDir
qt=$QT_ROOT

# --- build -----------------------------------------------------------------
# Keep the build machine's paths out of the binary (Rust and C++).
cargohome=${CARGO_HOME:-$HOME/.cargo}
remap="$HOME=/build/home $cargohome=/build/cargo $src=/build/omatree"
export RUSTFLAGS=""; export CFLAGS=""; export CXXFLAGS=""
for m in $remap; do
    RUSTFLAGS+=" --remap-path-prefix=$m"
    CFLAGS+=" -ffile-prefix-map=$m"
    CXXFLAGS+=" -ffile-prefix-map=$m"
done
export QMAKE=$qt/bin/qmake
cargo fetch --locked
cargo build --release --locked

# --- AppDir: OmaTree's own files, from the canonical sources -----------------
rm -rf "$appdir"
packaging/linux/stage.sh "$appdir" target/release/omatree

# Qt is redistributed in this format: its notice and the licence texts it needs.
install -Dm644 packaging/appimage/QT-NOTICE.md "$appdir/usr/share/doc/omatree/QT-NOTICE.md"
install -Dm644 /usr/share/common-licenses/LGPL-3 "$appdir/usr/share/licenses/omatree/Qt-LGPL-3.0.txt"
install -Dm644 /usr/share/common-licenses/GPL-3 "$appdir/usr/share/licenses/omatree/Qt-GPL-3.0.txt"
install -Dm644 /opt/licenses/ICU-73.2-LICENSE "$appdir/usr/share/licenses/omatree/ICU-73.2-LICENSE.txt"

# --- bundle Qt -----------------------------------------------------------------
# linuxdeploy copies the libraries the executable needs and, through its Qt
# plugin, the platform plugins and the QML modules that the QML imports.
export LD_LIBRARY_PATH=$qt/lib
export QML_SOURCES_PATHS=$src/qml
# Platforms: xcb (default), Wayland (two client plugins) and offscreen (the
# automated test platform). Qt Wayland also needs its shell, decoration and
# graphics-integration plugins, which linuxdeploy does not pick up by itself.
export EXTRA_PLATFORM_PLUGINS="libqwayland-generic.so;libqwayland-egl.so;libqoffscreen.so"
linuxdeploy --appdir "$appdir" --plugin qt \
    --executable "$appdir/usr/bin/omatree" \
    --desktop-file "$appdir/usr/share/applications/$id.desktop" \
    --icon-file assets/logo/omatree-logo.svg --icon-filename "$id"

# --- prune ---------------------------------------------------------------------
# What linuxdeploy copies is generous. These are not used: OmaTree forces the
# Basic Controls style, uses no effects/particles/shapes/SQL/XML models, no
# network (so no TLS backends), and its images are built-in PNG resources.
# Checked with QML_IMPORT_TRACE and QT_DEBUG_PLUGINS (see docs/PACKAGING.md).
cd "$appdir/usr"
for s in Fusion Imagine Material Universal FluentWinUI3; do
    rm -rf "qml/QtQuick/Controls/$s" lib/libQt6QuickControls2${s}*.so.6
done
rm -rf qml/QtQuick/{Effects,Particles,Shapes,LocalStorage,VectorImage,NativeStyle} qml/QtQml/XmlListModel
rm -f lib/libQt6{QuickEffects,QuickParticles,QuickShapes,Sql,QmlXmlListModel,QmlLocalStorage}.so.6
rm -rf plugins/tls plugins/imageformats
# libxkbcommon and its -x11 helper share private structures and must be the same
# version, and they read the host's keyboard data: use the host's pair. (Keeping
# one of them bundled crashes Qt's xcb start-up in xkb_x11_keymap_new_from_device.)
rm -f lib/libxkbcommon.so.0 lib/libxkbcommon-x11.so.0
cd "$src"
# linuxdeploy-plugin-qt's hook forces the gtk2 platform theme on GNOME/XFCE, which
# is not bundled. Qt platform themes of the host (gtk3, kde, qt6ct) cannot be
# loaded from inside the AppImage either. The bundled XDG desktop portal theme
# can: it gives the system's own file dialogs through xdg-desktop-portal, and
# Qt falls back to its built-in dialog if there is no portal. So that theme is
# selected when none, or gtk2/gtk3, was asked for; any other choice is respected.
cat > "$appdir/apprun-hooks/linuxdeploy-plugin-qt-hook.sh" <<'HOOK'
# OmaTree AppImage: see packaging/appimage/build-in-container.sh
case "${QT_QPA_PLATFORMTHEME:-}" in
    ""|gtk2|gtk3) export QT_QPA_PLATFORMTHEME=xdgdesktopportal ;;
esac
HOOK

# Qt Wayland needs these in addition to the platform plugins themselves.
for d in wayland-shell-integration wayland-decoration-client wayland-graphics-integration-client; do
    cp -r "$qt/plugins/$d" "$appdir/usr/plugins/$d"
done

# --- licences of the non-Qt libraries that were copied in --------------------------
lic=$appdir/usr/share/licenses/omatree/third-party
mkdir -p "$lic"
{
    echo "Libraries copied into this AppImage from the Ubuntu 20.04 build system."
    echo "Package, version, library file. Copyright files are in this directory."
} > "$lic/BUNDLED-LIBRARIES.txt"
for f in "$appdir"/usr/lib/*.so*; do
    n=$(basename "$f")
    case $n in libQt6*|libicu*) continue ;; esac
    pkg=$(dpkg -S "/usr/lib/x86_64-linux-gnu/$n" "/lib/x86_64-linux-gnu/$n" 2>/dev/null | head -1 | cut -d: -f1 || true)
    [ -n "$pkg" ] || { echo "no package owns $n" >&2; exit 1; }
    echo "$pkg $(dpkg-query -W -f='${Version}' "$pkg") $n" >> "$lic/BUNDLED-LIBRARIES.txt"
    install -Dm644 "/usr/share/doc/$pkg/copyright" "$lic/$pkg.copyright"
done

# --- AppImage ----------------------------------------------------------------
cd /tmp/build
ARCH=x86_64 appimagetool --runtime-file /opt/tools/runtime-x86_64 \
    "$appdir" "$outdir/OmaTree-$VERSION-x86_64.AppImage"
(cd "$appdir" && find . | LC_ALL=C sort) > "$outdir/appdir-listing.txt"
