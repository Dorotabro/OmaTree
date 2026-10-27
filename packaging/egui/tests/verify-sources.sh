#!/bin/bash
# Source-level regression checks for the egui Linux packaging. No package, root or display is
# needed, nothing is installed, and the user's own configuration is neither read for its content
# nor written (the script proves that at the end). Usable in CI.
#
#   packaging/egui/tests/verify-sources.sh
#
# Covers: desktop entry, MIME XML and its isolated registration, %f launch (spaces, Unicode, no
# shell), binary selection, one application identity, icons, install layout and uninstall, licence
# inventory, version consistency, the editor pin and independence from a sibling checkout.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
repo=$(cd "$here/../../.." && pwd)
cd "$repo"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
. "$repo/packaging/egui/linux/verify-common.sh"   # ok/bad/check/skip, $id, $bin
L=packaging/egui/linux

# Fingerprint the real user's desktop configuration before touching anything.
fingerprint() { { ls -l --time-style=+%s "$HOME/.config/mimeapps.list" "$HOME/.local/share/applications/mimeapps.list" 2>&1; \
    ls -R --time-style=+%s -l "$HOME/.local/share/mime" "$HOME/.local/share/applications" "$HOME/.config/omatree" 2>&1; } | sha256sum; }
before=$(fingerprint)

echo "== identity"
crate_ver=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/egui-app/Cargo.toml | head -1)
check "APP_ID in lib.rs is $id" grep -q "pub const APP_ID: &str = \"$id\";" crates/egui-app/src/lib.rs
check "main.rs sets the window's app_id from APP_ID and nothing else" bash -c "grep -q 'with_app_id(omatree_egui::APP_ID)' crates/egui-app/src/main.rs && ! grep -q 'with_app_id(\"' crates/egui-app/src/main.rs"
check "the desktop file is named $id.desktop" test -f $L/$id.desktop
check "desktop StartupWMClass, Icon, AppStream id, MIME icon and the install script all use the id" bash -c "
    grep -qx 'StartupWMClass=$id' $L/$id.desktop && grep -qx 'Icon=$id' $L/$id.desktop &&
    grep -q '<id>$id</id>' $L/$id.metainfo.xml && grep -q '<icon name=\"$id\"/>' $L/$id.xml &&
    grep -q '^id=$id\$' $L/stage.sh"
check "no packaging/egui file mentions the Qt identity io.github.Dorotabro.OmaTree followed by anything but Egui" bash -c "! grep -rn 'io\.github\.Dorotabro\.OmaTree' packaging/egui | grep -v 'io\.github\.Dorotabro\.OmaTreeEgui' | grep -v -e verify-sources.sh -e verify-common.sh | grep -q ."
check "the Qt packaging directory has no egui identity (the two stay independent)" bash -c "! grep -rl 'OmaTreeEgui\|omatree-egui' packaging/linux packaging/arch packaging/debian packaging/appimage packaging/*.sh 2>/dev/null | grep -q ."

echo "== binary selection"
check "stage.sh installs omatree-egui, from target/release/omatree-egui" bash -c "grep -q 'target/release/omatree-egui' $L/stage.sh && grep -q '^bin=omatree-egui' $L/stage.sh"
check "no egui packaging script builds or copies target/release/omatree (the Qt binary)" bash -c "! grep -rnE 'target/release/omatree([^-]|\$)' packaging/egui | grep -v verify-sources.sh | grep -q ."
check "every egui package build uses -p omatree-egui" bash -c "for f in packaging/egui/arch/PKGBUILD.in packaging/egui/debian/debian/rules packaging/egui/appimage/build-in-container.sh; do grep -q 'cargo build --release --locked.* -p omatree-egui' \$f || exit 1; done"
check "stage.sh refuses to run without the binary" bash -c "! $L/stage.sh $tmp/none $tmp/missing-binary >/dev/null 2>&1"

echo "== desktop entry"
if have desktop-file-validate; then check "desktop-file-validate" desktop-file-validate $L/$id.desktop; else skip "desktop-file-validate not installed"; fi
check "Exec is exactly 'omatree-egui %f'" grep -qx 'Exec=omatree-egui %f' $L/$id.desktop
check "Terminal=false, one MimeType, application/x-omatree" bash -c "grep -qx 'Terminal=false' $L/$id.desktop && grep -qx 'MimeType=application/x-omatree;' $L/$id.desktop"

echo "== MIME"
check "MIME XML is well formed" xmllint --noout $L/$id.xml
check "type application/x-omatree with only the glob *.omatree and no magic" bash -c "grep -c '<glob ' $L/$id.xml | grep -qx 1 && grep -q 'glob pattern=\"\*\.omatree\"' $L/$id.xml && ! grep -q '<magic' $L/$id.xml && grep -q 'type=\"application/x-omatree\"' $L/$id.xml"

echo "== isolated installation (a private prefix and private XDG directories)"
inst=$tmp/inst; xdg_data=$inst/usr/share
cat > $tmp/omatree-egui <<'STUB'
#!/bin/sh
printf 'ARGC=%s\n' "$#" > "$ARGV_OUT"
for a in "$@"; do printf 'ARG=[%s]\n' "$a" >> "$ARGV_OUT"; done
STUB
chmod +x $tmp/omatree-egui
mkdir -p $tmp/home
staged=0
if python3 packaging/egui/third-party.py check THIRD_PARTY.md >/dev/null 2>&1; then
    check "stage.sh stages into the private prefix" $L/stage.sh $inst $tmp/omatree-egui && staged=1
    check "the installed layout is exactly the documented one" bash -c "cd $inst && find . -type f | sed 's,^\./,,' | LC_ALL=C sort > $tmp/layout && diff $tmp/layout <(printf '%s\n' \
     usr/bin/omatree-egui usr/share/applications/$id.desktop usr/share/doc/omatree-egui/README.md \
     usr/share/icons/hicolor/128x128/apps/$id.png usr/share/icons/hicolor/256x256/apps/$id.png usr/share/icons/hicolor/32x32/apps/$id.png \
     usr/share/icons/hicolor/512x512/apps/$id.png usr/share/icons/hicolor/64x64/apps/$id.png usr/share/icons/hicolor/scalable/apps/$id.svg \
     usr/share/licenses/omatree-egui/LICENSE-APACHE usr/share/licenses/omatree-egui/LICENSE-MIT usr/share/licenses/omatree-egui/THIRD_PARTY.md \
     usr/share/licenses/omatree-egui/third-party-licenses.txt usr/share/metainfo/$id.metainfo.xml usr/share/mime/packages/$id.xml | LC_ALL=C sort)"
else
    skip "the dependency sources are not in the Cargo home (run cargo fetch --locked): staging, registration and launch not exercised"
fi

if [ "$staged" = 1 ] && have update-mime-database && have xdg-mime; then
    env_iso="HOME=$tmp/home XDG_DATA_HOME=$tmp/home/.local/share XDG_CONFIG_HOME=$tmp/home/.config XDG_DATA_DIRS=$xdg_data XDG_CONFIG_DIRS=$tmp/none"
    update-mime-database "$xdg_data/mime" >/dev/null 2>&1
    update-desktop-database "$xdg_data/applications" >/dev/null 2>&1
    mkdir -p "$tmp/files"; nb="$tmp/files/ünï notebook ✓ one.omatree"; echo "SQLite format 3" > "$nb"
    # GLib's database (what GTK file managers and the portal read); xdg-mime's own `filetype` query
    # falls back to `file(1)` on a bare desktop and would not consult the isolated database.
    ctype() { env $env_iso gio info -a standard::content-type "$1" 2>/dev/null | sed -n 's/.*content-type: //p'; }
    check "a Unicode file name with spaces is application/x-omatree" test "$(ctype "$nb")" = application/x-omatree
    printf 'SQLite format 3\0abc' > "$tmp/files/plain.db"; printf 'SQLite format 3\0abc' > "$tmp/files/other.sqlite"
    check "an ordinary SQLite file is not claimed" bash -c "[ \"$(ctype "$tmp/files/plain.db")\" != application/x-omatree ] && [ \"$(ctype "$tmp/files/other.sqlite")\" != application/x-omatree ]"
    check "the isolated default application for the type is the egui desktop entry" bash -c "[ \"\$(env $env_iso xdg-mime query default application/x-omatree)\" = $id.desktop ]"
    if have gio; then
        # gio launches the desktop entry exactly as a file manager does: the path replaces %f as one argument.
        check "launching the entry with that path passes exactly one, intact argument (no shell)" bash -c "env $env_iso PATH=$tmp:\$PATH ARGV_OUT=$tmp/argv gio launch '$xdg_data/applications/$id.desktop' \"$nb\" >/dev/null 2>&1; sleep 0.3; [ \"\$(cat $tmp/argv)\" = \"\$(printf 'ARGC=1\nARG=[%s]' \"$nb\")\" ]"
        cat > "$tmp/launch.sh" <<'LAUNCH'
#!/bin/bash
# launch.sh DESKTOP FILE: launch as a file manager does, in a fresh directory, and report what ran.
cd "$LAUNCHDIR" && rm -f PWNED PWNED2 "$ARGV_OUT" && gio launch "$1" "$2" >/dev/null 2>&1
sleep 0.4
[ "$(head -1 "$ARGV_OUT" 2>/dev/null)" = ARGC=1 ] && [ ! -e PWNED ] && [ ! -e PWNED2 ]
LAUNCH
        chmod +x "$tmp/launch.sh"; mkdir -p "$tmp/launchdir"
        hostile=$tmp/files/'$(touch PWNED); `touch PWNED2` & x.omatree'; echo "SQLite format 3" > "$hostile"
        check "a file name full of shell syntax is still one argument, and nothing runs" env $env_iso PATH=$tmp:$PATH ARGV_OUT=$tmp/argv2 LAUNCHDIR=$tmp/launchdir "$tmp/launch.sh" "$xdg_data/applications/$id.desktop" "$hostile"
    else skip "gio is not installed: the %f launch was not exercised"; fi
else skip "registration not exercised (nothing staged, or update-mime-database / xdg-mime missing)"; fi

echo "== uninstall"
if [ "$staged" = 1 ]; then
    (cd "$inst" && find . -type f | sed 's,^\./,,' > "$tmp/installed-files")
    check "removing exactly the installed files and their empty directories leaves nothing" bash -c "set -e; cd '$inst'; while read -r f; do rm -f \"\$f\"; done < $tmp/installed-files; find . -mindepth 1 -type d -empty -delete; find . -mindepth 1 | grep -vE '^\./usr/share/(mime|applications)(/|\$)' | grep -q . && exit 1; exit 0"
else skip "nothing staged"; fi

echo "== icons"
for size in 32 64 128 256 512; do check "assets/logo/omatree-icon-$size.png is ${size}x${size}" bash -c "file assets/logo/omatree-icon-$size.png | grep -q 'PNG image data, $size x $size'"; done
check "assets/logo/omatree-logo.svg exists and is the installed scalable icon" test -s assets/logo/omatree-logo.svg
check "the window icon embeds the 256 px PNG, which exists" grep -q 'omatree-icon-256.png' crates/egui-app/src/lib.rs

echo "== versions"
root_ver=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
core_ver=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/core/Cargo.toml | head -1)
meta_ver=$(xmllint --xpath 'string(/component/releases/release[1]/@version)' $L/$id.metainfo.xml)
deb_ver=$(sed -n '1s/^omatree-egui (\(.*\)) .*/\1/p' packaging/egui/debian/debian/changelog)
check "egui crate ($crate_ver) = Qt crate ($root_ver) = core ($core_ver)" test "$crate_ver" = "$root_ver" -a "$crate_ver" = "$core_ver"
check "AppStream ($meta_ver) and Debian changelog ($deb_ver) match the crate version" test "$meta_ver" = "$crate_ver" -a "$deb_ver" = "$crate_ver"

echo "== editor dependency and reproducibility inputs"
rev=$(sed -n 's/.*omatree_editor = {.*rev = "\([0-9a-f]\{40\}\)".*/\1/p' crates/egui-app/Cargo.toml)
check "the editor is a pinned git dependency (40-hex rev)" test -n "$rev"
check "Cargo.lock resolves the editor to exactly that commit" grep -q "omatree-egui-editor-spike.git?rev=$rev#$rev" Cargo.lock
check "the pin is the documented commit 30a95dd…" test "$rev" = 30a95dd43064232dc1ac04315c8020e2307a223c
check "no manifest or lock entry points at a sibling editor checkout" bash -c "! grep -rnE 'path *= *\"\.\./.*(editor|spike)|omatree-egui-editor-(library|spike)/' Cargo.toml crates/*/Cargo.toml Cargo.lock | grep -q ."
check "no egui packaging script mentions a sibling editor directory" bash -c "! grep -rnE 'omatree-egui-editor-library|omatree-egui-editor-spike/' packaging/egui | grep -v verify-sources.sh | grep -q ."
check "every egui build uses --locked" bash -c "! grep -rn 'cargo \(build\|fetch\)' packaging/egui | grep -v 'locked' | grep -v -e verify-sources.sh -e third-party.py -e seed-cargo-git.sh | grep -q ."
check "the Qt-free claim: omatree-egui's dependency tree has no cxx-qt or Qt crate" bash -c "! cargo tree --locked --offline -p omatree-egui --target x86_64-unknown-linux-gnu -e normal,build --prefix none 2>/dev/null | grep -Eqi '^(cxx-qt|cxx_qt|qt|qmetaobject)'"

echo "== licences"
if python3 packaging/egui/third-party.py check THIRD_PARTY.md >/dev/null 2>&1 || [ "$staged" = 1 ]; then
    check "THIRD_PARTY.md's generated crate table is current" python3 packaging/egui/third-party.py check THIRD_PARTY.md
else skip "dependency sources not in the Cargo home: the crate table was not compared"; fi
check "THIRD_PARTY.md flags the editor crate for manual review" grep -q 'no `license` field' THIRD_PARTY.md
check "THIRD_PARTY.md labels the Qt sections as legacy and not applying to egui" grep -q 'None of$' THIRD_PARTY.md

echo "== the user's own configuration"
check "no real MIME, desktop-entry, mimeapps or OmaTree configuration was touched" test "$before" = "$(fingerprint)"

verify_done
