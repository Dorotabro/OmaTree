#!/bin/bash
# Installs OmaTree's files into a staging root, in the standard Linux layout.
#
#   packaging/linux/stage.sh DESTDIR [PATH-TO-omatree-BINARY]
#
# DESTDIR is created if needed and is the only thing written to: nothing of
# the running system is touched, and no MIME or icon cache is updated (the
# package manager's hooks do that after a real installation). PREFIX defaults
# to /usr. The binary defaults to target/release/omatree. Any missing input is
# an error, and the run stops there.
set -euo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
root=$(cd "$here/../.." && pwd)

destdir=${1:?usage: stage.sh DESTDIR [BINARY]}
binary=${2:-$root/target/release/omatree}
prefix=${PREFIX:-/usr}
id=io.github.Dorotabro.OmaTree

need() {
    [ -f "$1" ] || { echo "stage.sh: missing required file: $1" >&2; exit 1; }
}

for f in "$binary" \
    "$here/$id.desktop" "$here/$id.xml" "$here/$id.metainfo.xml" \
    "$root/assets/logo/omatree-logo.svg" \
    "$root/LICENSE-MIT" "$root/LICENSE-APACHE" "$root/THIRD_PARTY.md" "$root/README.md"; do
    need "$f"
done
for size in 32 64 128 256 512; do
    need "$root/assets/logo/omatree-icon-$size.png"
done
[ -x "$binary" ] || { echo "stage.sh: not executable: $binary" >&2; exit 1; }

d=$destdir$prefix
install -Dm755 "$binary" "$d/bin/omatree"
install -Dm644 "$here/$id.desktop" "$d/share/applications/$id.desktop"
install -Dm644 "$here/$id.xml" "$d/share/mime/packages/$id.xml"
install -Dm644 "$here/$id.metainfo.xml" "$d/share/metainfo/$id.metainfo.xml"

install -Dm644 "$root/assets/logo/omatree-logo.svg" \
    "$d/share/icons/hicolor/scalable/apps/$id.svg"
for size in 32 64 128 256 512; do
    install -Dm644 "$root/assets/logo/omatree-icon-$size.png" \
        "$d/share/icons/hicolor/${size}x${size}/apps/$id.png"
done

install -Dm644 "$root/LICENSE-MIT" "$d/share/licenses/omatree/LICENSE-MIT"
install -Dm644 "$root/LICENSE-APACHE" "$d/share/licenses/omatree/LICENSE-APACHE"
install -Dm644 "$root/THIRD_PARTY.md" "$d/share/licenses/omatree/THIRD_PARTY.md"
install -Dm644 "$root/README.md" "$d/share/doc/omatree/README.md"
