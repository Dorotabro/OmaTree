#!/bin/bash
# Runs INSIDE the build container (started by build-package.sh), from the root of the unpacked
# source snapshot. Builds omatree-egui, assembles the AppDir, and writes the AppImage and the AppDir
# listing to the directory given as $1.
set -euo pipefail

outdir=${1:?usage: build-in-container.sh OUTDIR}
src=$PWD
id=io.github.Dorotabro.OmaTreeEgui
appdir=/tmp/build/OmaTree-egui.AppDir

cargohome=${CARGO_HOME:-$HOME/.cargo}
export RUSTFLAGS=""
for m in "$HOME=/build/home" "$cargohome=/build/cargo" "$src=/build/omatree-egui"; do
    RUSTFLAGS+=" --remap-path-prefix=$m"
done
cargo fetch --locked
cargo build --release --locked -p omatree-egui

rm -rf "$appdir"
packaging/egui/linux/stage.sh "$appdir" target/release/omatree-egui
install -m755 packaging/egui/appimage/AppRun "$appdir/AppRun"
# appimagetool wants the desktop entry and the icon at the top of the AppDir.
cp "$appdir/usr/share/applications/$id.desktop" "$appdir/$id.desktop"
cp "$appdir/usr/share/icons/hicolor/256x256/apps/$id.png" "$appdir/$id.png"
cp "$appdir/$id.png" "$appdir/.DirIcon"
echo "X-AppImage-Version=$VERSION" >> "$appdir/$id.desktop"
# The licence texts are the AppImage's only "bundled libraries": there are none besides the Rust crates.
{ echo "rustc: $(rustc -V)"; echo "cargo: $(cargo -V)"; echo "build userspace: $(. /etc/os-release; echo "$PRETTY_NAME"), $(ldd --version | head -1)"; } > "$outdir/toolchain.txt"
echo "glibc symbol floor of the executable: $(objdump -T target/release/omatree-egui | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1)" >> "$outdir/toolchain.txt"

cd /tmp/build
ARCH=x86_64 appimagetool --runtime-file /opt/tools/runtime-x86_64 \
    "$appdir" "$outdir/OmaTree-egui-$VERSION-x86_64.AppImage"
(cd "$appdir" && find . | LC_ALL=C sort) > "$outdir/appdir-listing.txt"
