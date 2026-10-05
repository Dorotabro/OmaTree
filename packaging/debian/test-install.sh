#!/bin/bash
# Installs the already-built .deb into a clean container of the given image,
# runs OmaTree headless, then purges it.
#
#   packaging/debian/test-install.sh dist/omatree_0.1.0_amd64.deb debian:12
#   packaging/debian/test-install.sh dist/omatree_0.1.0_amd64.deb ubuntu:24.04
#
# Installs with --no-install-recommends, so the Depends field alone has to be
# enough. The package is never rebuilt per distribution.
set -euo pipefail

deb=$(realpath "${1:?usage: test-install.sh PACKAGE.deb IMAGE}")
image=${2:?usage: test-install.sh PACKAGE.deb IMAGE}
engine=$(command -v docker || command -v podman || { echo "no docker or podman" >&2; exit 1; })

"$engine" run --rm -v "$deb:/pkg/$(basename "$deb"):ro" "$image" bash -c '
set -uo pipefail
export DEBIAN_FRONTEND=noninteractive
fails=0
ok()  { echo "ok    $*"; }
bad() { echo "FAIL  $*"; fails=$((fails + 1)); }
. /etc/os-release; echo "== $PRETTY_NAME"
apt-get update -qq >/dev/null
apt-get install -y -qq --no-install-recommends /pkg/*.deb > /tmp/apt.log 2>&1 && ok "apt install ./omatree_*.deb" || { bad "apt install"; tail -15 /tmp/apt.log; }
echo "-- apt installed $(grep -c "^Setting up" /tmp/apt.log) packages in total"
dpkg -s omatree | grep -E "^(Status|Version|Architecture)"
[ "$(dpkg-query -W -f="\${Status}" omatree)" = "install ok installed" ] && ok "package status: installed" || bad "package status"
echo "-- dpkg -L omatree"; dpkg -L omatree | grep -v "/$" | sed "s/^/   /"
test -x /usr/bin/omatree && ok "/usr/bin/omatree" || bad "/usr/bin/omatree"
! ldd /usr/bin/omatree | grep -q "not found" && ok "ldd: all libraries resolve" || { bad "ldd"; ldd /usr/bin/omatree | grep "not found"; }
mkdir -p /tmp/t && cd /tmp/t
export XDG_RUNTIME_DIR=/tmp/xdg; mkdir -m 700 -p $XDG_RUNTIME_DIR
QT_QPA_PLATFORM=offscreen HOME=/tmp/t timeout 6 /usr/bin/omatree /tmp/t/example.omatree > out.txt 2>&1; rc=$?
[ $rc -eq 124 ] && ok "ran headless for 6 s until timeout killed it" || { bad "headless run exited early (rc=$rc)"; }
[ -s /tmp/t/example.omatree ] && ok "notebook created: $(stat -c %s /tmp/t/example.omatree) bytes" || bad "notebook file not created"
if [ -s out.txt ]; then echo "-- application output:"; sed "s/^/   /" out.txt; fi
grep -Eqi "warning|error|not installed|could not|binding loop|plugin" out.txt && bad "output contains warnings" || ok "no QML/plugin warnings"
apt-get purge -y -qq omatree >/dev/null 2>&1
! dpkg -s omatree >/dev/null 2>&1 && ok "purged" || bad "purge"
left=$(dpkg -L omatree 2>/dev/null | while read -r p; do [ -f "$p" ] && echo "$p"; done)
[ -z "$left" ] && ok "no package-owned files remain" || { bad "files remain:"; echo "$left"; }
echo "== failures: $fails"; exit $fails
'
