#!/bin/bash
# Compile-only evidence for other targets (ticket E12c), in a disposable container. This is NOT a
# runtime test: a crate that type-checks for Windows or macOS may still misbehave there.
#
#   packaging/egui/tests/cross-check.sh x86_64-pc-windows-gnu [more targets]
#   WITH_TESTS=1 packaging/egui/tests/cross-check.sh x86_64-pc-windows-gnu   # the test targets instead
#
# Uses the Debian 13 build image of the egui packaging (pinned Rust 1.99.0), adds the target with
# rustup and, for Windows, the MinGW-w64 C compiler that rusqlite's bundled SQLite needs. The MSVC
# targets need Microsoft's headers and libraries and the Apple targets need Apple's SDK; neither can
# be had here, so those are expected to fail at the C step and say so. The editor library comes from the
# host's Cargo cache (packaging/egui/seed-cargo-git.sh), as in the package builds.
set -uo pipefail
root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
cd "$root"
engine=$(command -v docker || command -v podman) || { echo "no docker or podman" >&2; exit 1; }
cache=${OMATREE_CACHE:-$HOME/.cache/omatree-packaging}
mkdir -p "$cache/cargo-cross"
packaging/egui/seed-cargo-git.sh "$cache/cargo-cross"
"$engine" build -q -t omatree-egui-build-debian13 packaging/egui/debian >/dev/null
"$engine" build -q -t omatree-egui-cross -f packaging/egui/tests/cross.Dockerfile packaging/egui/tests >/dev/null
work=$(mktemp -d); trap 'rm -rf "$work"' EXIT
git ls-files -co --exclude-standard | while read -r f; do [ -e "$f" ] && echo "$f"; done | LC_ALL=C sort > "$work/files"
tar --create --no-recursion --files-from="$work/files" --file="$work/src.tar"
for target in "$@"; do
  echo "== cargo check --target $target"
  mkdir -p "$cache/target-$target"
  "$engine" run --rm --user "$(id -u):$(id -g)" -v "$work:/work:ro" -v "$cache/cargo-cross:/cargo" -v "$cache/target-$target:/tmp/t" -e CARGO_HOME=/cargo -e HOME=/tmp/home -e TARGET="$target" -e WITH_TESTS="${WITH_TESTS:-}" \
    omatree-egui-cross bash -c '
      set -u; mkdir -p /tmp/home /tmp/s && cd /tmp/s && tar xf /work/src.tar
      export CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc
      cargo fetch --locked >/dev/null 2>&1
      for p in omatree-core omatree-egui; do
        if [ -n "${WITH_TESTS:-}" ]; then
          if cargo check --locked --release --target "$TARGET" -p $p --tests --target-dir /tmp/t 2>/tmp/err.txt; then echo "ok    $p TESTS type-check for $TARGET"; else echo "FAIL  $p TESTS for $TARGET:"; grep -E "^error" -A6 /tmp/err.txt | head -n 14; fi
          continue
        fi
        if cargo check --locked --release --target "$TARGET" -p $p --target-dir /tmp/t 2>/tmp/err.txt; then echo "ok    $p type-checks for $TARGET"; else echo "FAIL  $p for $TARGET:"; grep -E "^error" -A6 /tmp/err.txt | head -n 14; fi
      done' 2>&1
done
