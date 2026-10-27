#!/bin/bash
# Installs the already-built egui .deb into a clean container of the given image, with the desktop
# libraries the package declares and nothing else from a desktop, then runs it against them and
# purges it. This is the real runtime test of the Debian package (verify-deb.sh only inspects it).
#
#   packaging/egui/debian/test-install.sh dist/omatree-egui_0.1.2_amd64.deb debian:13
#   packaging/egui/debian/test-install.sh dist/omatree-egui_0.1.2_amd64.deb ubuntu:24.04
#
# Installs with --no-install-recommends, so Depends alone has to be enough to resolve every library
# the executable opens at run time. A display is provided inside the container only: Xvfb (X11) and,
# if it installs, weston's headless backend (Wayland), both on Mesa's software renderer. Each
# is reported separately; a display that cannot be started is reported as skipped, not as passed.
# Nothing touches the host's desktop.
set -euo pipefail

deb=$(realpath "${1:?usage: test-install.sh PACKAGE.deb IMAGE}")
image=${2:?usage: test-install.sh PACKAGE.deb IMAGE}
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
engine=$(command -v docker || command -v podman || { echo "no docker or podman" >&2; exit 1; })

"$engine" run --rm -v "$deb:/pkg/$(basename "$deb"):ro" "$image" bash -c '
set -uo pipefail
export DEBIAN_FRONTEND=noninteractive
fails=0
ok()   { echo "ok    $*"; }
bad()  { echo "FAIL  $*"; fails=$((fails + 1)); }
skip() { echo "skip  $*"; }
. /etc/os-release; echo "== $PRETTY_NAME, $(ldd --version | head -1)"
apt-get update -qq >/dev/null
apt-get install -y -qq --no-install-recommends /pkg/*.deb > /tmp/apt.log 2>&1 && ok "apt install ./omatree-egui_*.deb (no recommends)" || { bad "apt install"; tail -15 /tmp/apt.log; exit 1; }
echo "-- apt set up $(grep -c "^Setting up" /tmp/apt.log) packages in total"
[ "$(dpkg-query -W -f="\${Status}" omatree-egui)" = "install ok installed" ] && ok "package status: installed" || bad "package status"
echo "-- dpkg -L omatree-egui"; dpkg -L omatree-egui | grep -v "/$" | sed "s/^/   /"
test -x /usr/bin/omatree-egui && ok "/usr/bin/omatree-egui" || bad "/usr/bin/omatree-egui"
! ldd /usr/bin/omatree-egui | grep -q "not found" && ok "ldd: all linked libraries resolve" || { bad "ldd"; ldd /usr/bin/omatree-egui | grep "not found"; }
ldd /usr/bin/omatree-egui | grep -qi qt && bad "Qt is linked" || ok "no Qt linked"

# Without a display: a clear failure, not a hang or a crash.
mkdir -p /tmp/t && cd /tmp/t; export HOME=/tmp/t XDG_RUNTIME_DIR=/tmp/xdg; mkdir -m 700 -p $XDG_RUNTIME_DIR
env -u WAYLAND_DISPLAY -u DISPLAY timeout 8 /usr/bin/omatree-egui > nodisplay.txt 2>&1; rc=$?
[ $rc -eq 1 ] && grep -q "neither WAYLAND_DISPLAY" nodisplay.txt && ok "no display: exits 1 with the reason (rc=$rc)" || bad "no display: rc=$rc: $(head -c 200 nodisplay.txt)"

# Run-time libraries the executable opens: with the display up, list what really got loaded.
apt-get install -y -qq --no-install-recommends xvfb xauth libgl1-mesa-dri libegl-mesa0 libglx-mesa0 mesa-vulkan-drivers weston >/tmp/apt2.log 2>&1 || apt-get install -y -qq --no-install-recommends xvfb xauth libgl1-mesa-dri libegl-mesa0 libglx-mesa0 >/tmp/apt2.log 2>&1 || { skip "could not install a software GL stack: no run-time test"; fails=$((fails)); }

run_with_display() {   # name, then env arguments (the display is already running); sets RC
    local name=$1; shift
    env -u WAYLAND_DISPLAY -u DISPLAY "$@" LD_DEBUG=libs LD_DEBUG_OUTPUT=/tmp/t/ld-$name HOME=/tmp/t timeout 8 /usr/bin/omatree-egui > /tmp/t/out-$name.txt 2>&1; RC=$?
    echo "-- $name: exit $RC (124 = still running when the timeout stopped it)"; sed "s/^/   /" /tmp/t/out-$name.txt | head -8
    echo "-- $name libraries loaded: $(cat /tmp/t/ld-$name.* 2>/dev/null | sed -n "s/.*calling init: //p" | xargs -rn1 basename | sort -u | grep -E "wayland|xkb|X11|xcb|Xcursor|Xi|EGL|GL|fontconfig|dbus" | tr "\n" " ")"
}
if command -v Xvfb >/dev/null; then
    Xvfb :77 -screen 0 1280x800x24 >/tmp/t/xvfb.log 2>&1 &
    xvfb=$!; sleep 2
    run_with_display x11 DISPLAY=:77
    [ "$RC" = 124 ] && [ -z "$(grep -Ev "^\s*\$|libEGL warning|MESA" /tmp/t/out-x11.txt)" ] && ok "X11 (Xvfb + Mesa llvmpipe): the window opened, ran until the timeout stopped it, with no output beyond the Mesa container warnings" || bad "X11 run exited $RC"
    kill $xvfb 2>/dev/null
else skip "Xvfb unavailable"; fi
if command -v weston >/dev/null; then
    weston --backend=headless --socket=wl-test --width=1280 --height=800 >/tmp/t/weston.log 2>&1 &
    west=$!; sleep 3
    run_with_display wayland WAYLAND_DISPLAY=wl-test XDG_SESSION_TYPE=wayland
    [ "$RC" = 124 ] && [ -z "$(grep -Ev "^\s*\$|libEGL warning|MESA" /tmp/t/out-wayland.txt)" ] && ok "Wayland (weston headless + Mesa software): the window opened, ran until the timeout stopped it, with no output beyond the Mesa container warnings" || bad "Wayland run exited $RC (weston log: $(tail -2 /tmp/t/weston.log | tr "\n" " "))"
    kill $west 2>/dev/null
else skip "weston unavailable: no Wayland run in this image"; fi

apt-get purge -y -qq omatree-egui >/dev/null 2>&1
! dpkg -s omatree-egui >/dev/null 2>&1 && ok "purged" || bad "purge"
left=$(dpkg -L omatree-egui 2>/dev/null | while read -r p; do [ -f "$p" ] && echo "$p"; done)
[ -z "$left" ] && ok "no package-owned files remain" || { bad "files remain:"; echo "$left"; }
echo "== failures: $fails"; exit $fails
'
