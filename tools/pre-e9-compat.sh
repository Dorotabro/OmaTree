#!/usr/bin/env bash
# Independent compatibility check for E9a/E9b: does a *pre-E9* build of omatree-core (taken from git,
# not from this working tree) open the files this build writes, and does it leave a version 4
# file untouched when it refuses it?
#
#   tools/pre-e9-compat.sh [PRE_E9_REV]        (default: the commit before schema version 4)
#
# It builds crates/core at PRE_E9_REV in a scratch workspace, writes the fixtures with the current
# build, and runs the old code's `Storage::open` + `load_document` on them, comparing SHA-256
# before and after. Needs cargo and git; nothing in the repository is modified.
set -euo pipefail
cd "$(dirname "$0")/.."
REV="${1:-8709fd2}"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/old"
git archive "$REV" crates/core Cargo.lock | tar -x -C "$WORK/old"
mkdir -p "$WORK/old/checker/src"
cat > "$WORK/old/Cargo.toml" <<'TOML'
[workspace]
members = ["crates/core", "checker"]
resolver = "2"
TOML
cat > "$WORK/old/checker/Cargo.toml" <<'TOML'
[package]
name = "checker"
version = "0.0.0"
edition = "2021"
[dependencies]
omatree-core = { path = "../crates/core" }
TOML
cat > "$WORK/old/checker/src/main.rs" <<'RS'
use omatree_core::document::Document;
fn main() {
    let path = std::env::args().nth(1).expect("path");
    match Document::open_existing(std::path::Path::new(&path)) {
        Ok(d) => println!("opened: {} top-level notes, trash {}", d.notebook().roots().len(), d.recovery().trash().len()),
        Err(e) => println!("refused: {e}"),
    }
}
RS
echo "== building pre-E9 core ($REV)"
(cd "$WORK/old" && cargo build --release -p checker --quiet)
echo "== writing fixtures with the current build"
OMATREE_COMPAT_DIR="$WORK/fixtures" cargo test -p omatree-core --quiet -- --ignored write_compat_fixtures >/dev/null
for f in v3 v4 v4-pruned v5-tagged; do
  before="$(sha256sum "$WORK/fixtures/$f.omatree" | cut -d' ' -f1)"
  out="$("$WORK/old/target/release/checker" "$WORK/fixtures/$f.omatree")"
  after="$(sha256sum "$WORK/fixtures/$f.omatree" | cut -d' ' -f1)"
  same=$([ "$before" = "$after" ] && echo "file unchanged" || echo "FILE CHANGED")
  echo "$f.omatree (user_version $(od -An -tu4 --endian=big -j60 -N4 "$WORK/fixtures/$f.omatree" | tr -d ' ')): pre-E9 core -> $out; $same"
done
