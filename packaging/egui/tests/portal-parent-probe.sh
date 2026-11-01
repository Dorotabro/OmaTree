#!/bin/bash
# Shows the parent a native file dialog is requested with (ticket E12c), by watching the real
# session bus: the `parent_window` argument of the xdg-desktop-portal FileChooser.OpenFile call.
#
#   packaging/egui/tests/portal-parent-probe.sh BINARY NOTEBOOK.omatree [x11]
#
# Needs a graphical session with xdg-desktop-portal and a file-chooser backend, dbus-monitor and
# ydotool (to press Ctrl+O and Escape). It opens a dialog and cancels it; no file is chosen.
# "" means no parent; "wayland:<handle>" an exported Wayland surface; "x11:<xid>" an X11 window.
# Under X11 pass `x11` (WAYLAND_DISPLAY is unset for the run, so it uses Xwayland on a Wayland desktop).
set -uo pipefail
bin=${1:?usage: portal-parent-probe.sh BINARY NOTEBOOK [x11]}; nb=$(realpath "${2:?notebook}")
h=$(mktemp -d); trap 'rm -rf "$h"' EXIT
dbus-monitor --session "interface='org.freedesktop.portal.FileChooser'" > "$h/mon.txt" 2>&1 & mon=$!
sleep 1
HOME=$h XDG_CONFIG_HOME=$h/c env ${3:+-u WAYLAND_DISPLAY} "$bin" "$nb" >/dev/null 2>&1 & app=$!
sleep 3
ydotool key 29:1 24:1 24:0 29:0; sleep 2.5; ydotool key 1:1 1:0; sleep 1
kill $app $mon 2>/dev/null
grep -A1 "member=OpenFile" "$h/mon.txt" | sed -n 2p | sed 's/^ *string/parent_window =/'
