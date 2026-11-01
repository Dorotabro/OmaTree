#!/bin/bash
# Accessibility-tree probe on a PRIVATE session bus (ticket E12c): the user's own session bus and its
# accessibility flag are never touched.
#
#   packaging/egui/tests/atspi-probe.sh NOTEBOOK.omatree [KEYS_TO_SEND_FIRST]
#
# Needs a Wayland or X11 display, at-spi-bus-launcher, at-spi2-registryd, dbus-run-session, gdbus and
# python3 with PyGObject/Atspi. Starts a throwaway bus, turns IsEnabled on there, runs the release
# binary in an isolated HOME, optionally presses Down once (`down`) to select the next note, then
# prints the tree. Operation with a real screen reader (Orca) is NOT tested by this.
set -uo pipefail
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../../.." && pwd)
export NB=$(realpath "${1:?usage: atspi-probe.sh NOTEBOOK [down]}") KEYS=${2:-} ROOT=$root HERE=$here
exec dbus-run-session -- bash -c '
/usr/lib/at-spi-bus-launcher --launch-immediately >/dev/null 2>&1 &
sleep 2
/usr/lib/at-spi2-registryd >/dev/null 2>&1 &
sleep 1
gdbus call --session --dest org.a11y.Bus --object-path /org/a11y/bus --method org.freedesktop.DBus.Properties.Set org.a11y.Status IsEnabled "<true>" >/dev/null
h=$(mktemp -d); export HOME=$h XDG_CONFIG_HOME=$h/c XDG_STATE_HOME=$h/s
cp "$NB" "$h/nb.omatree"
"$ROOT/target/release/omatree-egui" "$h/nb.omatree" & app=$!
sleep 4
[ "$KEYS" = down ] && { ydotool key 108:1 108:0; sleep 1.5; }
python3 -I "$HERE/atspi-probe.py"
kill $app; rm -rf "$h"
' 2>&1 | grep -v "dbus-daemon\|^\[session\|fusermount\|^$\|can.t be made"
