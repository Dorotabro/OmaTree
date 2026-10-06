#!/bin/bash
# Builds OmaTree.app and OmaTree-VERSION-arm64.dmg for Apple Silicon.
#
#   packaging/macos/build-dmg.sh
#
# Writes dist/OmaTree-VERSION-arm64.dmg (and leaves dist/macos/OmaTree.app).
# Needs macOS on Apple Silicon, Rust, and Qt 6 with macdeployqt (Homebrew's
# `qt` is what this was built with; set QT_PREFIX or put its bin/ on PATH to
# use another). Nothing is published, signed with a real identity, or sent
# for notarization: see docs/PACKAGING.md.
#
# Optional environment:
#   QT_PREFIX      Qt install prefix (default: found from macdeployqt / qmake)
#   SIGN_IDENTITY  a codesigning identity to pass to macdeployqt instead of the
#                  default ad-hoc signature (UNTESTED: this Mac has none)
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)
cd "$root"

die() { echo "build-dmg.sh: $*" >&2; exit 1; }
need() { command -v "$1" >/dev/null 2>&1 || die "missing required tool: $1 ($2)"; }

[ "$(uname -s)" = Darwin ] || die "this builds the macOS package; run it on a Mac"
[ "$(uname -m)" = arm64 ] || die "Apple Silicon only (arm64); this Mac is $(uname -m)"

# ---- tools -----------------------------------------------------------------
if [ -n "${QT_PREFIX:-}" ]; then
    PATH="$QT_PREFIX/bin:$QT_PREFIX/libexec:$PATH"
fi
need cargo "install Rust"
need macdeployqt "install Qt 6, e.g. brew install qt, or set QT_PREFIX"
need qmake6 "part of Qt 6"
need iconutil "Xcode command line tools"
need sips "part of macOS"
need hdiutil "part of macOS"
need codesign "Xcode command line tools"
need otool "Xcode command line tools"
need vtool "Xcode command line tools"
need plutil "part of macOS"

version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n1)
[ -n "$version" ] || die "could not read the version from Cargo.toml"
qt_version=$(qmake6 -query QT_VERSION)
qt_libs=$(qmake6 -query QT_INSTALL_LIBS)
qt_qml=$(qmake6 -query QT_INSTALL_QML)
qt_plugins=$(qmake6 -query QT_INSTALL_PLUGINS)
case "$qt_version" in 6.*) ;; *) die "Qt 6 is required, found $qt_version" ;; esac
# The oldest macOS the bundled Qt itself runs on.
min_macos=$(vtool -show-build "$qt_libs/QtCore.framework/Versions/A/QtCore" | sed -n 's/^ *minos //p' | head -n1)
[ -n "$min_macos" ] || die "could not read Qt's minimum macOS version"

out=$root/dist/macos
dmg=$root/dist/OmaTree-$version-arm64.dmg
rm -rf "$out"
mkdir -p "$out" "$root/dist"
# The bundle is assembled, signed and packed in a temporary directory, not in
# the repository: a checkout inside iCloud Drive (~/Documents) gets Finder and
# file-provider attributes on every directory, which codesign refuses.
work=$(mktemp -d "${TMPDIR:-/tmp}/omatree-macos.XXXXXX")
trap 'rm -rf "$work"' EXIT
app=$work/OmaTree.app
echo "OmaTree $version, Qt $qt_version ($qt_libs), macOS $min_macos or newer, arm64"

# ---- the executable --------------------------------------------------------
# Build paths (source tree, Cargo home, $HOME) are removed from the binary,
# and a separate target directory keeps development builds undisturbed.
cargo_home=${CARGO_HOME:-$HOME/.cargo}
qt_real=$(cd "$qt_libs" && pwd -P)
export CARGO_TARGET_DIR=$root/target/macos
# (rustc uses the last matching rule, so the most specific come last.)
export RUSTFLAGS="${RUSTFLAGS:-} --remap-path-prefix=$HOME=/home --remap-path-prefix=$root=/omatree --remap-path-prefix=$cargo_home=/cargo"
export CXXFLAGS="${CXXFLAGS:-} -ffile-prefix-map=$HOME=/home -ffile-prefix-map=$root=/omatree -ffile-prefix-map=$qt_real=/qt/lib -ffile-prefix-map=$qt_libs=/qt/lib"
cargo build --release --locked
binary=$CARGO_TARGET_DIR/release/omatree
[ -x "$binary" ] || die "the release build produced no executable"
[ "$(lipo -archs "$binary")" = arm64 ] || die "the executable is not arm64-only"

# ---- the bundle ------------------------------------------------------------
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$app/Contents/Frameworks" "$app/Contents/PlugIns"
install -m755 "$binary" "$app/Contents/MacOS/omatree"
sed -e "s/@VERSION@/$version/g" -e "s/@MINIMUM_MACOS@/$min_macos/g" \
    "$here/Info.plist.in" > "$app/Contents/Info.plist"
plutil -lint "$app/Contents/Info.plist" >/dev/null
printf 'APPL????' > "$app/Contents/PkgInfo"

# The icon: the existing artwork, unaltered, at the sizes macOS wants. Only
# the 16-pixel image is derived (scaled down from the 32-pixel one).
iconset=$work/OmaTree.iconset
mkdir -p "$iconset"
logo=$root/assets/logo
cp "$logo/omatree-icon-32.png"   "$iconset/icon_16x16@2x.png"
cp "$logo/omatree-icon-32.png"   "$iconset/icon_32x32.png"
cp "$logo/omatree-icon-64.png"   "$iconset/icon_32x32@2x.png"
cp "$logo/omatree-icon-128.png"  "$iconset/icon_128x128.png"
cp "$logo/omatree-icon-256.png"  "$iconset/icon_128x128@2x.png"
cp "$logo/omatree-icon-256.png"  "$iconset/icon_256x256.png"
cp "$logo/omatree-icon-512.png"  "$iconset/icon_256x256@2x.png"
cp "$logo/omatree-icon-512.png"  "$iconset/icon_512x512.png"
cp "$logo/omatree-icon-1024.png" "$iconset/icon_512x512@2x.png"
sips -z 16 16 "$logo/omatree-icon-32.png" --out "$iconset/icon_16x16.png" >/dev/null
iconutil -c icns "$iconset" -o "$app/Contents/Resources/OmaTree.icns"
rm -rf "$iconset"

# OmaTree's own licences and the third-party inventory. (The bundled Qt and
# its libraries need their own notices before a public release: see
# docs/PACKAGING.md.)
mkdir -p "$app/Contents/Resources/licenses"
cp "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/THIRD_PARTY.md" "$app/Contents/Resources/licenses/"

# ---- Qt --------------------------------------------------------------------
# macdeployqt copies the frameworks the executable links, the plugins they
# need and the QML modules the application's QML imports (it scans qml/), and
# signs ad hoc (or with SIGN_IDENTITY). Then everything OmaTree does not use is
# removed again; see prune_qt below and docs/PACKAGING.md.
deploy=(macdeployqt "$app" -qmldir="$root/qml" -verbose=1)
if [ -n "${SIGN_IDENTITY:-}" ]; then
    deploy+=(-codesign="$SIGN_IDENTITY" -hardened-runtime -timestamp)
fi
# It complains about plug-ins whose own libraries are not installed (virtual
# keyboard, PDF, multimedia, ...), exactly the ones removed again below, so its
# output goes to a log rather than the console.
"${deploy[@]}" >"$out/macdeployqt.log" 2>&1 || { tail -20 "$out/macdeployqt.log" >&2; die "macdeployqt failed"; }
echo "macdeployqt done ($(grep -c '^ERROR' "$out/macdeployqt.log" || true) messages about plug-ins that are removed again; see dist/macos/macdeployqt.log)"

# shellcheck source=packaging/macos/prune.sh
. "$here/prune.sh"
prune_qt "$app"
make_relocatable "$app"

# The pruned bundle is signed again as a whole: removing files invalidates the
# signatures macdeployqt made. Ad hoc unless SIGN_IDENTITY is given.
if [ -n "${SIGN_IDENTITY:-}" ]; then
    codesign --force --deep --options runtime --timestamp --sign "$SIGN_IDENTITY" "$app"
else
    codesign --force --deep --sign - "$app"
fi

"$here/verify-app.sh" "$app"

# ---- the disk image --------------------------------------------------------
# Deliberately plain: the app and a shortcut to /Applications.
stage=$work/dmg
rm -rf "$stage"
mkdir -p "$stage"
cp -R "$app" "$stage/OmaTree.app"
ln -s /Applications "$stage/Applications"
rm -f "$dmg"
hdiutil create -quiet -volname OmaTree -srcfolder "$stage" -fs HFS+ -format UDZO -ov "$work/OmaTree.dmg"
hdiutil verify -quiet "$work/OmaTree.dmg"
cp "$work/OmaTree.dmg" "$dmg"
# A copy of the app for inspection. (Do not sign or re-sign this one in place
# if the repository is in iCloud Drive; see above.)
ditto --noextattr --noqtn "$app" "$out/OmaTree.app"
xattr -cr "$out/OmaTree.app" 2>/dev/null || true

echo
echo "built $out/OmaTree.app"
echo "built $dmg ($(du -h "$dmg" | cut -f1))"
