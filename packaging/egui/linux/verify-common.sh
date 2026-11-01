# Checks shared by the egui Arch, Debian and AppImage verifiers. Source it; do not run it. The
# caller sets $tmp (a scratch directory), then calls the functions below. All three formats stage
# the same files with packaging/egui/linux/stage.sh, so they share one layout under their root.
#
# Every check prints "ok", "FAIL" or "skip"; the caller exits with $fails.

id=io.github.Dorotabro.OmaTreeEgui
bin=omatree-egui
# Where the licence files are: Arch and AppImage use usr/share/licenses/NAME, Debian usr/share/doc/NAME
# (and keeps the two licence texts in debian/copyright, so it sets want_license_texts=0).
licdir=${licdir:-usr/share/licenses/$bin}
want_license_texts=${want_license_texts:-1}
fails=0
ok()   { echo "ok    $*"; }
bad()  { echo "FAIL  $*"; fails=$((fails + 1)); }
skip() { echo "skip  $*"; }
have() { command -v "$1" >/dev/null 2>&1; }
check() { local what=$1; shift; if "$@"; then ok "$what"; else bad "$what"; fi; }
repo=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)

# verify_files ROOT: the files every format must contain, and nothing that belongs to the source,
# to the Qt application or to its identity.
verify_files() {
    local root=$1 f
    for f in usr/bin/$bin usr/share/applications/$id.desktop usr/share/mime/packages/$id.xml \
        usr/share/metainfo/$id.metainfo.xml usr/share/icons/hicolor/scalable/apps/$id.svg \
        $licdir/THIRD_PARTY.md $licdir/third-party-licenses.txt; do
        check "has $f" test -f "$root/$f"
    done
    if [ "$want_license_texts" = 1 ]; then
        check "has $licdir/LICENSE-MIT and LICENSE-APACHE" bash -c "test -f '$root/$licdir/LICENSE-MIT' && test -f '$root/$licdir/LICENSE-APACHE'"
    fi
    check "binary is executable" test -x "$root/usr/bin/$bin"
    check "no Qt application files (binary, desktop entry, icon, MIME) from the legacy package" bash -c "! find '$root' \( -path '*/bin/omatree' -o -name 'io.github.Dorotabro.OmaTree.*' -o -name 'io.github.Dorotabro.OmaTree.png' \) | grep -q ."
    check "nothing from the source tree or build" bash -c "! find '$root' \( -name '*.rs' -o -name '*.qml' -o -name Cargo.toml -o -name Cargo.lock -o -name '*.omatree' -o -name '*.db' -o -path '*/.git*' -o -name target \) | grep -q ."
    check "no Qt libraries bundled" bash -c "! find '$root' -name 'libQt*' | grep -q ."
}

# verify_binary BIN: only glibc-family libraries are linked, SQLite is bundled, no Qt anywhere,
# and no build-machine path or user name is baked into the executable.
verify_binary() {
    local b=$1 needed
    echo "== dynamic links"
    needed=$(readelf -d "$b" | sed -n 's/.*NEEDED.*\[\(.*\)\]/\1/p')
    echo "$needed" | sed 's/^/need  /'
    check "only glibc, libm, libgcc_s and the loader are linked" bash -c "! echo '$needed' | grep -Ev '^(libc\.so\.6|libm\.so\.6|libgcc_s\.so\.1|ld-linux-x86-64\.so\.2|libdl\.so\.2|libpthread\.so\.0|librt\.so\.1)$' | grep -q ."
    check "no Qt in the dynamic section" bash -c "! readelf -d '$b' | grep -qi 'libQt'"
    check "no Qt or CXX-Qt symbols or QML strings" bash -c "! strings -a '$b' | grep -Eq 'libQt6|QtQuick|cxx_qt|QQmlApplicationEngine'"
    check "SQLite is bundled, not linked" bash -c "! readelf -d '$b' | grep -qi sqlite"
    echo "glibc  newest symbol version needed: $(objdump -T "$b" | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1)"
    echo "== build paths"
    check "no /home path in the binary" bash -c "! strings -a '$b' | grep -q '/home/'"
    check "no /tmp or repository path in the binary" bash -c "! strings -a '$b' | grep -v '^/tmp/\.X11-unix' | grep -Eq '/tmp/|Antigravity|/dist/'"
    check "no user name in the binary" bash -c "[ -z '$USER' ] || ! strings -a '$b' | grep -qF '$USER'"
}

# verify_metadata ROOT: desktop entry, AppStream, MIME and icons.
verify_metadata() {
    local root=$1 size f d=$1/usr/share/applications/$id.desktop
    echo "== metadata"
    if have desktop-file-validate; then check "desktop-file-validate" desktop-file-validate "$d"; else skip "desktop-file-validate not installed"; fi
    check "Name contains OmaTree" grep -q '^Name=OmaTree' "$d"
    check "Exec is the egui binary with %f only" grep -qx "Exec=$bin %f" "$d"
    check "Exec has no shell and no other field code" bash -c "! grep '^Exec=' '$d' | grep -Eq 'sh -c|bash|%[FuUdDnNickvm]|[;|&\$\`]'"
    check "Terminal=false" grep -qx 'Terminal=false' "$d"
    check "Icon is the application id" grep -qx "Icon=$id" "$d"
    check "StartupWMClass is the application id" grep -qx "StartupWMClass=$id" "$d"
    check "MimeType is exactly application/x-omatree" grep -qx 'MimeType=application/x-omatree;' "$d"
    check "the desktop entry is named after the application id" test "$(basename "$d")" = "$id.desktop"
    if have appstreamcli; then check "appstreamcli validate" appstreamcli validate --no-net "$root/usr/share/metainfo/$id.metainfo.xml"; else skip "appstreamcli not installed"; fi
    check "AppStream id is the application id" bash -c "[ \"\$(xmllint --xpath 'string(/component/id)' '$root/usr/share/metainfo/$id.metainfo.xml')\" = '$id' ]"
    check "AppStream provides the egui binary" bash -c "[ \"\$(xmllint --xpath 'string(/component/provides/binary)' '$root/usr/share/metainfo/$id.metainfo.xml')\" = '$bin' ]"
    local cargo_ver meta_ver
    cargo_ver=$(sed -n 's/^version = "\(.*\)"/\1/p' "$repo/crates/egui-app/Cargo.toml" | head -1)
    meta_ver=$(xmllint --xpath 'string(/component/releases/release[1]/@version)' "$root/usr/share/metainfo/$id.metainfo.xml" 2>/dev/null)
    check "newest AppStream release ($meta_ver) is the egui crate version ($cargo_ver)" test -n "$cargo_ver" -a "$meta_ver" = "$cargo_ver"
    check "MIME XML is well formed" xmllint --noout "$root/usr/share/mime/packages/$id.xml"
    check "MIME has no magic matching" bash -c "! grep -q '<magic' '$root/usr/share/mime/packages/$id.xml'"
    if have update-mime-database; then
        mkdir -p "$tmp/mime/packages"
        cp "$root/usr/share/mime/packages/$id.xml" "$tmp/mime/packages/"
        update-mime-database "$tmp/mime" >/dev/null 2>&1
        check "a temporary MIME database has exactly *.omatree for application/x-omatree" bash -c "grep -q 'application/x-omatree:\*.omatree' '$tmp/mime/globs2' && [ \"\$(grep -c omatree '$tmp/mime/globs2')\" = 1 ]"
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
    check "the icon is the canonical logo" cmp -s "$root/usr/share/icons/hicolor/scalable/apps/$id.svg" "$repo/assets/logo/omatree-logo.svg"
    if have rsvg-convert; then check "the SVG renders" bash -c "rsvg-convert -w 64 '$root/usr/share/icons/hicolor/scalable/apps/$id.svg' >/dev/null"; else skip "rsvg-convert not installed"; fi
}

# verify_licences ROOT: the licence files that the distributed binary needs.
verify_licences() {
    local l=$1/$licdir/third-party-licenses.txt
    echo "== licences"
    check "the licence bundle names SQLite's status and the fonts' licences" bash -c "grep -q 'OFL' '$l' && grep -qi 'ubuntu font licen' '$l'"
    check "the licence bundle lists every crate with a licence (no empty expression)" bash -c "! grep -E '^[A-Za-z0-9_.-]+ [0-9]+\.[0-9]+[^ :]*: *\$' '$l' | grep -q ."
    check "the bundle lists as many crates as THIRD_PARTY.md" bash -c "n=\$(sed -n 's/^\([0-9]*\) crates are compiled.*/\1/p' '$1/$licdir/THIRD_PARTY.md'); m=\$(sed -n '/^PART 1: crates\$/,/^PART 2: licence/p' '$l' | grep -Ec '^[A-Za-z0-9_.-]+ [0-9]+\.[0-9]+[^ :]*: '); [ -n \"\$n\" ] && [ \"\$m\" = \"\$n\" ]"
}

# make_fixture FILE: a valid schema-version-4 notebook, written by the current core's own
# (ignored) fixture test, to FILE. Needs cargo and the repository; OMATREE_FIXTURE=/path/x.omatree
# supplies one instead. Returns non-zero if neither is possible.
make_fixture() {
    local out=$1 dir=$tmp/fixture
    if [ -n "${OMATREE_FIXTURE:-}" ] && [ -f "$OMATREE_FIXTURE" ]; then cp "$OMATREE_FIXTURE" "$out"; return; fi
    have cargo && [ -f "$repo/Cargo.toml" ] || return 1
    mkdir -p "$dir"
    (cd "$repo" && OMATREE_COMPAT_DIR=$dir cargo test --locked -p omatree-core --quiet -- --ignored write_compat_fixtures >/dev/null 2>&1) || return 1
    cp "$dir/v4.omatree" "$out"
}

# verify_launch COMMAND...: with a display available, runs COMMAND with one existing notebook (a
# name with spaces and non-ASCII), in an isolated HOME, and requires it to stay up until the
# timeout, to leave the notebook's bytes untouched, and to write nothing to stderr. Without a display
# or a fixture it is skipped, never faked. (omatree-egui refuses a path that does not exist; it
# does not create it.)
verify_launch() {
    local home=$tmp/home log=$tmp/launch.log status nb before after
    if [ -z "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; then skip "launch: no Wayland or X11 display here"; return; fi
    rm -rf "$home"; mkdir -p "$home"
    nb="$home/launch test ünï.omatree"
    if ! make_fixture "$nb"; then skip "launch: no notebook fixture (needs cargo, or OMATREE_FIXTURE)"; return; fi
    before=$(sha256sum "$nb" | cut -d' ' -f1)
    HOME=$home XDG_STATE_HOME=$home/state XDG_CONFIG_HOME=$home/config XDG_DATA_HOME=$home/data LC_ALL=C.UTF-8 \
        timeout 8 "$@" "$nb" >"$log" 2>&1
    status=$?
    after=$(sha256sum "$nb" | cut -d' ' -f1)
    check "it ran until stopped (exit $status, 124 = timeout)" test "$status" -eq 124
    check "opening the notebook and stopping left its bytes unchanged" test "$before" = "$after"
    check "no -wal/-journal file is left next to the notebook" bash -c "! ls '$home' | grep -Eq -- '-(wal|shm|journal)\$'"
    check "the recent-notebooks list is in the isolated config, not elsewhere" bash -c "[ ! -e '$home/.config' ] && grep -qF 'launch test' '$home/config/omatree/recent.txt'"
    sed 's/^/log   /' "$log"
    check "no errors, warnings or panics on stderr" bash -c "! grep -Eiq 'error|panic|warn|could not|failed' '$log'"
}

verify_done() {
    echo
    if [ "$fails" -eq 0 ]; then echo "all checks passed"; else echo "$fails check(s) FAILED"; fi
    exit "$fails"
}
