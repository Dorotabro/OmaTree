#!/bin/bash
# Hi-DPI scaling evidence on X11 (ticket E12c), in a disposable Arch Linux container (same glibc as the Arch-built binary) with Xvfb: runs the
# AppImage-free release binary at several WINIT_X11_SCALE_FACTOR values and reports the window's
# physical size and saves screenshots. It exercises egui's pixels-per-point handling on X11 only; a real
# Hi-DPI monitor, fractional Wayland scaling and macOS Retina are NOT covered. Nothing touches the host's displays.
#
#   packaging/egui/tests/hidpi-probe.sh OUTDIR [BINARY]
set -uo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
out=$(realpath -m "${1:?usage: hidpi-probe.sh OUTDIR [BINARY]}"); bin=$(realpath "${2:-$root/target/release/omatree-egui}")
mkdir -p "$out"
docker run --rm -v "$bin:/omatree-egui:ro" -v "$out:/out" archlinux:latest bash -c '
pacman -Sy --noconfirm --needed xorg-server-xvfb xorg-xauth imagemagick xorg-xdpyinfo xdotool libx11 libxcursor libxi libxkbcommon-x11 mesa libglvnd fontconfig ttf-dejavu >/dev/null 2>&1 || { echo "pacman failed"; exit 1; }
Xvfb :55 -screen 0 2560x1600x24 >/dev/null 2>&1 &
sleep 2
export DISPLAY=:55 HOME=/tmp/h; mkdir -p $HOME
for s in 1 1.5 2; do
  WINIT_X11_SCALE_FACTOR=$s timeout 12 /omatree-egui >/dev/null 2>&1 & app=$!
  sleep 5
  id=$(xdotool search --onlyvisible --name OmaTree | head -n1)
  g=$(xdotool getwindowgeometry --shell $id 2>/dev/null | tr "\n" " ")
  echo "scale $s: window $g"
  import -window root -crop 1400x900+0+0 /out/hidpi-$s.png 2>/dev/null
  kill $app 2>/dev/null; wait $app 2>/dev/null
done'
