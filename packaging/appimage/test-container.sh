#!/bin/bash
# Runs the already-built AppImage in a clean container of the given image. The
# AppImage is not rebuilt per distribution, and nothing is installed from it.
#
#   packaging/appimage/test-container.sh dist/OmaTree-0.1.1-x86_64.AppImage ubuntu:20.04
#
# Containers have no FUSE, so the AppImage runs with its supported
# extract-and-run mode (APPIMAGE_EXTRACT_AND_RUN=1). Qt runs offscreen.
#
# Step 1 runs it in the image as it comes, which shows exactly which system
# libraries the AppImage expects the host to have (it deliberately does not
# bundle the low-level desktop stack). Step 2 installs the usual desktop
# libraries of that distribution (apt) and requires a clean run.
set -euo pipefail

app=$(realpath "${1:?usage: test-container.sh APPIMAGE IMAGE}")
image=${2:?usage: test-container.sh APPIMAGE IMAGE}
engine=$(command -v docker || command -v podman || { echo "no docker or podman" >&2; exit 1; })

"$engine" run --rm -v "$app:/in/$(basename "$app"):ro" "$image" bash -c '
set -uo pipefail
export DEBIAN_FRONTEND=noninteractive
fails=0
ok()  { echo "ok    $*"; }
bad() { echo "FAIL  $*"; fails=$((fails + 1)); }
. /etc/os-release; echo "== $PRETTY_NAME (glibc $(ldd --version | head -1 | grep -o "[0-9.]*$"))"
mkdir -p /tmp/t /tmp/xdg && chmod 700 /tmp/xdg && cp /in/*.AppImage /tmp/t/OmaTree.AppImage && chmod +x /tmp/t/OmaTree.AppImage && cd /tmp/t
export XDG_RUNTIME_DIR=/tmp/xdg HOME=/tmp/t LC_ALL=C.UTF-8 QT_QPA_PLATFORM=offscreen APPIMAGE_EXTRACT_AND_RUN=1

echo "-- step 1: the image as it comes"
./OmaTree.AppImage --appimage-extract >/dev/null 2>&1
missing=$(for f in squashfs-root/usr/bin/omatree $(find squashfs-root/usr/plugins/platforms/libqoffscreen.so squashfs-root/usr/lib -name "*.so*" -type f); do LD_LIBRARY_PATH=squashfs-root/usr/lib ldd $f 2>/dev/null | grep "not found"; done | sort -u)
if [ -z "$missing" ]; then ok "no missing system libraries"; else echo "info  system libraries this image lacks:"; echo "$missing" | sed "s/^/      /"; fi
timeout 8 ./OmaTree.AppImage /tmp/t/bare.omatree > bare.txt 2>&1; rc=$?
echo "info  bare run: exit $rc"; sed "s/^/      /" bare.txt | head -6

if [ -n "$missing" ] || [ $rc -ne 124 ]; then
    echo "-- step 2: after installing the distribution desktop libraries"
    # Debian 11 is archived: its mirrors have moved.
    [ "${VERSION_CODENAME:-}" = bullseye ] && sed -i "s|deb.debian.org|archive.debian.org|; /security/d" /etc/apt/sources.list
    apt-get update -qq >/dev/null 2>&1
    apt-get install -y -qq --no-install-recommends libfontconfig1 libfreetype6 libxkbcommon0 libxkbcommon-x11-0 libegl1 libgl1 libwayland-client0 libx11-6 libx11-xcb1 libxcb1 libcom-err2 libgpg-error0 fonts-dejavu-core >/tmp/apt.log 2>&1 || { bad "apt install of desktop libraries"; tail -5 /tmp/apt.log; }
fi
rm -f /tmp/t/example.omatree
timeout 8 ./OmaTree.AppImage /tmp/t/example.omatree > out.txt 2>&1; rc=$?
[ $rc -eq 124 ] && ok "ran headless until timeout killed it (extract-and-run)" || bad "exited early (rc=$rc)"
[ -s /tmp/t/example.omatree ] && ok "notebook created: $(stat -c %s /tmp/t/example.omatree) bytes" || bad "notebook file not created"
[ -s out.txt ] && { echo "-- application output:"; sed "s/^/   /" out.txt; }
grep -Eqi "warning|error|not installed|could not|binding loop|plugin|module" out.txt && bad "output contains warnings" || ok "no QML/plugin warnings"
echo "== failures: $fails"; exit $fails
'
