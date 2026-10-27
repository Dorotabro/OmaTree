#!/bin/bash
# Runs the egui AppImage in a clean container of the given image, to see whether it really runs
# on a distribution other than the one it was built on.
#
#   packaging/egui/appimage/test-container.sh dist/OmaTree-egui-0.1.2-x86_64.AppImage ubuntu:20.04
#   packaging/egui/appimage/test-container.sh dist/OmaTree-egui-0.1.2-x86_64.AppImage debian:12
#
# The image gets only what an AppImage user is expected to have: the host's windowing, GL, font and
# D-Bus libraries (installed by name, the same list as the .deb's Depends) plus Xvfb and Mesa's
# software renderer as the display. There is no FUSE in a container, so the image is run with
# APPIMAGE_EXTRACT_AND_RUN=1: that exercises the AppDir and the executable, not the FUSE mount.
# Nothing touches the host's desktop.
set -euo pipefail

img=$(realpath "${1:?usage: test-container.sh APPIMAGE IMAGE}")
image=${2:?usage: test-container.sh APPIMAGE IMAGE}
engine=$(command -v docker || command -v podman || { echo "no docker or podman" >&2; exit 1; })

"$engine" run --rm -v "$img:/app/$(basename "$img"):ro" "$image" bash -c '
set -uo pipefail
export DEBIAN_FRONTEND=noninteractive
fails=0
ok()   { echo "ok    $*"; }
bad()  { echo "FAIL  $*"; fails=$((fails + 1)); }
. /etc/os-release; echo "== $PRETTY_NAME, $(ldd --version | head -1)"
cp /app/*.AppImage /tmp/app.AppImage; chmod +x /tmp/app.AppImage
export APPIMAGE_EXTRACT_AND_RUN=1 HOME=/tmp/t XDG_RUNTIME_DIR=/tmp/xdg; mkdir -p /tmp/t; mkdir -m 700 -p $XDG_RUNTIME_DIR
env -u WAYLAND_DISPLAY -u DISPLAY timeout 20 /tmp/app.AppImage > /tmp/t/nolibs.txt 2>&1; rc=$?
echo "-- before installing any library: exit $rc: $(head -c 160 /tmp/t/nolibs.txt)"
[ $rc -eq 1 ] && ok "runs far enough on this glibc to report that there is no display" || bad "bare image: rc=$rc"
DISPLAY=:77 timeout 20 /tmp/app.AppImage > /tmp/t/nolibs2.txt 2>&1; rc=$?
echo "-- DISPLAY set but no X11 library installed: exit $rc: $(head -c 200 /tmp/t/nolibs2.txt)"
[ $rc -ne 0 ] && [ $rc -ne 124 ] && ok "a missing windowing library is a clean non-zero exit, not a hang or a crash (rc=$rc)" || bad "missing libs: rc=$rc"
apt-get update -qq >/dev/null
apt-get install -y -qq --no-install-recommends libwayland-client0 libwayland-egl1 libxkbcommon0 libxkbcommon-x11-0 libegl1 libgl1 libx11-6 libxcb1 libxcursor1 libxi6 libfontconfig1 libdbus-1-3 xvfb xauth libgl1-mesa-dri libegl-mesa0 libglx-mesa0 >/tmp/apt.log 2>&1 && ok "installed the host libraries by name" || { bad "apt: $(tail -3 /tmp/apt.log)"; }
Xvfb :77 -screen 0 1280x800x24 >/tmp/t/xvfb.log 2>&1 &
xvfb=$!; sleep 2
env -u WAYLAND_DISPLAY DISPLAY=:77 LD_DEBUG=libs LD_DEBUG_OUTPUT=/tmp/t/ld timeout 10 /tmp/app.AppImage > /tmp/t/run.txt 2>&1; rc=$?
echo "-- X11 on $PRETTY_NAME: exit $rc"; sed "s/^/   /" /tmp/t/run.txt | head -5
[ $rc -eq 124 ] && [ -z "$(grep -Ev "^\s*\$|libEGL warning|MESA" /tmp/t/run.txt)" ] && ok "the AppImage opened its window on X11 (Xvfb + Mesa) and ran until the timeout stopped it" || bad "X11 run exited $rc"
kill $xvfb 2>/dev/null
echo "== failures: $fails"; exit $fails
'
