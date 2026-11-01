#!/bin/bash
# Dead keys, Compose sequences and layout switching on X11 (ticket E12c), in a disposable Debian 13
# container with its own Xvfb, xdotool and xkb: nothing of the host's keyboard configuration or input
# method is involved. The release binary opens a copy of a fixture notebook, characters are typed
# into the note body, the notebook is saved with Ctrl+S, and the stored body is read back from SQLite.
#
#   packaging/egui/tests/xkb-input-probe.sh NOTEBOOK.omatree [BINARY]
#
# What this does NOT test: a real input method (IBus/fcitx composition with a pre-edit string and a
# commit) - the container has none - nor Wayland's text-input path.
set -uo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
nb=$(realpath "${1:?usage: xkb-input-probe.sh NOTEBOOK [BINARY]}"); bin=$(realpath "${2:-$root/target/release/omatree-egui}")
docker run --rm -v "$bin:/omatree-egui:ro" -v "$nb:/in.omatree:ro" archlinux:latest bash -c '
pacman -Sy --noconfirm --needed xorg-server-xvfb xorg-xauth xdotool xorg-setxkbmap xkeyboard-config xorg-xset sqlite libx11 libxcursor libxi libxkbcommon-x11 mesa libglvnd fontconfig ttf-dejavu >/dev/null 2>&1 || { echo "pacman failed"; exit 1; }
export LANG=C.UTF-8 LC_ALL=C.UTF-8 HOME=/tmp/h DISPLAY=:66; mkdir -p $HOME
Xvfb :66 -screen 0 1400x900x24 >/dev/null 2>&1 & sleep 2
run() { # name layout-args then xdotool steps via function steps
  cp /in.omatree /tmp/nb.omatree
  setxkbmap $2 2>&1
  /omatree-egui /tmp/nb.omatree >/tmp/out.txt 2>&1 & app=$!
  sleep 4
  id=$(xdotool search --onlyvisible --name OmaTree | head -n1); xdotool windowfocus $id; sleep 0.5
  xdotool mousemove 700 300 click 1; sleep 0.5          # into the note body
  xdotool key ctrl+End; sleep 0.2
  $3
  sleep 0.5; xdotool key ctrl+s; sleep 1.5
  kill $app; wait $app 2>/dev/null
  body=$(sqlite3 /tmp/nb.omatree "select body from nodes where parent_id is null order by id limit 1")
  echo "-- $1: tail of the stored body: $(printf "%s" "$body" | tail -c 40 | tr "\n" "|")" >&2
  printf "%s" "$body"
}
dead() { xdotool key dead_acute; xdotool key a; xdotool key dead_circumflex; xdotool key e; xdotool key dead_caron; xdotool key s; xdotool key dead_diaeresis; xdotool key o; }
compose() { xdotool key Multi_key; xdotool key apostrophe; xdotool key e; xdotool key Multi_key; xdotool key c; xdotool key o; xdotool key Multi_key; xdotool key s; xdotool key s; }
direct() { xdotool type --delay 40 "ěščřžýáíé ÅÖ ß"; }
check() { local want=$1 body=$2 label=$3; case "$body" in *"$want"*) echo "ok    $label: found $want";; *) echo "FAIL  $label: expected $want";; esac; }
b=$(run "us(intl) dead keys" "-layout us -variant intl" dead | tail -n 1); check "áêšö" "$b" "dead acute+a, circumflex+e, caron+s (via us-intl: dead_caron may be absent), diaeresis+o"
b=$(run "compose" "-layout us -option compose:ralt" compose | tail -n 1); check "éǒß" "$b" "Compose: apostrophe+e, c+o (caron o), s+s"
b=$(run "direct unicode" "-layout us" direct | tail -n 1); check "ěščřžýáíé ÅÖ ß" "$b" "keysym-typed Czech/Nordic/German text"
b=$(run "cz layout" "-layout cz" "xdotool type --delay 40 ěščřžýáíé" | tail -n 1); check "ěščřžýáíé" "$b" "Czech layout (cz) typed keys"
'
