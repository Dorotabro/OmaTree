# Checks shared by the Arch, Debian and AppImage verifiers. Source it; do not
# run it. The caller sets $id and $tmp (a scratch directory), then calls
# the functions below. All three formats stage the same files with
# packaging/linux/stage.sh, so they share one layout under their root.
#
# Every check prints "ok", "FAIL" or "skip"; the caller exits with $fails.

id=io.github.Dorotabro.OmaTree
fails=0
ok()   { echo "ok    $*"; }
bad()  { echo "FAIL  $*"; fails=$((fails + 1)); }
skip() { echo "skip  $*"; }
have() { command -v "$1" >/dev/null 2>&1; }
check() { local what=$1; shift; if "$@"; then ok "$what"; else bad "$what"; fi; }

# verify_files ROOT: the files every format must contain.
verify_files() {
    local root=$1 f
    for f in usr/bin/omatree usr/share/applications/$id.desktop \
        usr/share/mime/packages/$id.xml usr/share/metainfo/$id.metainfo.xml \
        usr/share/icons/hicolor/scalable/apps/$id.svg; do
        check "has $f" test -f "$root/$f"
    done
    check "binary is executable" test -x "$root/usr/bin/omatree"
    check "nothing from the source tree or build" bash -c "! find '$root' \( -name '*.rs' -o -name main.qml -o -name TreePane.qml -o -name Cargo.toml -o -name '*.omatree' -o -name '*.db' -o -path '*/.git*' -o -name target \) | grep -q ."
}

# verify_binary BIN: Qt is linked dynamically, SQLite is bundled, no test hook,
# and no build-machine path or user name is baked into the executable.
verify_binary() {
    local bin=$1
    echo "== dynamic links"
    readelf -d "$bin" | grep NEEDED | sed 's/.*\[\(.*\)\]/need  \1/'
    check "Qt Core, Gui and Qml are linked dynamically" bash -c "readelf -d '$bin' | grep -q libQt6Core && readelf -d '$bin' | grep -q libQt6Gui && readelf -d '$bin' | grep -q libQt6Qml"
    check "SQLite is bundled, not linked" bash -c "! readelf -d '$bin' | grep -qi sqlite"
    check "test hook is absent from the release binary" bash -c "! strings -a '$bin' | grep -q OMATREE_TEST_QML"
    echo "== build paths"
    check "no /home path in the binary" bash -c "! strings -a '$bin' | grep -q '/home/'"
    check "no /tmp or repository path in the binary" bash -c "! strings -a '$bin' | grep -Eq '/tmp/|Antigravity|/dist/'"
    check "no user name in the binary" bash -c "! strings -a '$bin' | grep -qF '$USER'"
}

# verify_metadata ROOT: desktop entry, AppStream, MIME and icons.
verify_metadata() {
    local root=$1 size f
    echo "== metadata"
    if have desktop-file-validate; then check "desktop-file-validate" desktop-file-validate "$root/usr/share/applications/$id.desktop"; else skip "desktop-file-validate not installed"; fi
    check "Name=OmaTree" grep -qx 'Name=OmaTree' "$root/usr/share/applications/$id.desktop"
    check "Exec uses %f" grep -qx 'Exec=omatree %f' "$root/usr/share/applications/$id.desktop"
    check "Icon is the application id" grep -qx "Icon=$id" "$root/usr/share/applications/$id.desktop"
    check "MimeType is application/x-omatree" grep -qx 'MimeType=application/x-omatree;' "$root/usr/share/applications/$id.desktop"
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
    check "the icon is the canonical logo" cmp -s "$root/usr/share/icons/hicolor/scalable/apps/$id.svg" "$(dirname "${BASH_SOURCE[0]}")/../../assets/logo/omatree-logo.svg"
    if have rsvg-convert; then check "the SVG renders" bash -c "rsvg-convert -w 64 '$root/usr/share/icons/hicolor/scalable/apps/$id.svg' >/dev/null"; else skip "rsvg-convert not installed"; fi
}

# verify_launch COMMAND...: runs COMMAND with one notebook path, headless, and
# requires it to stay up until the timeout, create the notebook and be silent.
verify_launch() {
    local home=$tmp/home log=$tmp/launch.log status
    rm -rf "$home"; mkdir -p "$home"
    HOME=$home XDG_STATE_HOME=$home/state XDG_CONFIG_HOME=$home/config XDG_DATA_HOME=$home/data \
        QT_QPA_PLATFORM=${LAUNCH_PLATFORM:-offscreen} QT_FORCE_STDERR_LOGGING=1 LC_ALL=C.UTF-8 \
        timeout 8 "$@" "$home/launch-test.omatree" >"$log" 2>&1
    status=$?
    check "it ran until stopped (exit $status, 124 = timeout)" test "$status" -eq 124
    check "it created the notebook it was given with %f" test -s "$home/launch-test.omatree"
    sed 's/^/log   /' "$log"
    check "no QML warnings, binding loops, plugin errors or errors" bash -c "! grep -Eiq 'qml|binding loop|warning|error|TypeError|plugin|could not load' '$log'"
}

verify_done() {
    echo
    if [ "$fails" -eq 0 ]; then echo "all checks passed"; else echo "$fails check(s) FAILED"; fi
    exit "$fails"
}
