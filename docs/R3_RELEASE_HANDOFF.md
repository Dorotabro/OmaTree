# R3 release handoff: OmaTree egui 0.2.0 (Linux x86_64 prerelease)

Prepared on `egui-integration` (HEAD `3ddde99`, R2) with **uncommitted** changes in the working tree. No commit, tag, push,
GitHub release, upload or publication was made; no real notebook or system installation was touched.

## 1. Where things stand: four separate states

| State | Status | Why |
|---|---|---|
| **1. Ready to commit** | **YES** | Source, metadata, documentation and the release script are consistent; the full validation passes on this exact tree (section 5). |
| **2. Ready to tag** | **NO, not yet** | Needs the commit, the merge to `main` and the validation re-run on that exact commit (`docs/RELEASING.md` §4–§5). Recommended to hold until the licence items in section 6 are answered, because the tagged source carries the legal notices. |
| **3. Ready to build final artifacts** | **NO** | Final artifacts are built from the tag (`--ref v0.2.0`). The script and the process are ready and were run on a scratch commit (candidate only). |
| **4. Ready to publish** | **NO: BLOCKED** | The licensing gate is open (section 6), final artifacts do not exist, and the maintainer has not authorized tagging or publication. |

## 2. Decisions and proposed values

| Item | Value |
|---|---|
| Final source version | `0.2.0` (root package, `omatree-core`, `omatree-editor`, `omatree-egui`, `Cargo.lock`, AppStream, Debian changelog) |
| Proposed tag | `v0.2.0`, annotated, on the merge commit, created only by the maintainer |
| Release type | GitHub **prerelease** (never "latest"); Qt `v0.1.2` stays the latest |
| Platform | Linux x86_64 only |
| Identity | unchanged: `io.github.Dorotabro.OmaTreeEgui`, binary `omatree-egui`, packages `omatree-egui` (no E13) |
| Artifacts | `omatree-egui-0.2.0-1-x86_64.pkg.tar.zst`, `omatree-egui_0.2.0_amd64.deb`, `OmaTree-egui-0.2.0-x86_64.AppImage`, `SHA256SUMS` |

## 3. What changed in R3 (the commit)

- **Version 0.1.2 → 0.2.0** in the four `Cargo.toml` and four `Cargo.lock` entries; AppStream: new first
  `<release version="0.2.0" date="2026-10-31" type="development">` (the older 0.1.2 entry is kept as history; adjust the date to
  the publication date if you prefer); Debian changelog: new top entry; egui file-name examples in `docs/PACKAGING_EGUI.md` and in the
  three verifier script headers. Historical records, the Qt tags and the Qt metadata were **not** changed.
- **Unavoidable effects on the Qt development build** (it shares the workspace version and the root `Cargo.toml`): the Qt binary
  now reports 0.2.0 (`set_application_version` uses `CARGO_PKG_VERSION`); the Windows resource version derives from it; and the legacy Qt
  packaging scripts (`packaging/{arch,debian,appimage,macos,windows}`, `release-build.sh`), if run from this tree, would build
  "0.2.0" Qt packages whose AppStream and Debian changelog still say 0.1.2, so their own verifiers would fail ("newest AppStream
  release is the Cargo.toml version"). Qt is not being released from this tree and Qt v0.1.2 is built from its tag; the Qt unit tests
  (43) pass. Nothing was changed in the Qt packaging.
- **New: `packaging/egui/release-build.sh`** (section 7), `docs/RELEASE_NOTES_0.2.0.md` (draft), `docs/LICENSING_INVENTORY_0.2.0.md`,
  `docs/R3_LICENSING_REPORT.md`, this handoff.
- **Licensing checks and notices** (from the R3 licensing step): 10 new checks in `verify-sources.sh`; `THIRD_PARTY.md` records the
  decision, AI-assistance provenance, copied-code review, font hashes and open items; the four upstream notice copies (krilla,
  profiling, gl_generator, khronos_api) were verified identical at the commits their published crates record
  (`packaging/egui/licences/SOURCES.md`); nothing was replaced; legal metadata unchanged.
- README: status and Downloads section state that egui 0.2.0 is an unpublished prerelease, Qt v0.1.2 remains available, Linux x86_64 only,
  schema 4 and 5 are refused by Qt v0.1.2, data lives in local `.omatree` notebooks; the only release link is v0.1.2.

## 4. Release identity and coexistence (Arch and Debian 13 containers, real Qt v0.1.2 packages)

The released Qt v0.1.2 packages were downloaded from GitHub (SHA-256 checked against its `SHA256SUMS`) into a scratch directory and installed
in disposable containers together with the candidates. Nothing was installed on this machine.

| Check | Arch (`archlinux:latest`) | Debian (`debian:13`) |
|---|---|---|
| Qt v0.1.2 installed first | ok | ok |
| Previous egui 0.1.2 candidate installed, then **upgraded** to 0.2.0 | ok (`omatree-egui 0.2.0-1`) | ok (`Unpacking … (0.2.0) over (0.1.2)`) |
| Files shared between the Qt and egui packages | **0** | **0** |
| Qt files byte-identical after installing and upgrading egui | yes | yes |
| `/usr/bin/omatree` (Qt) untouched, `/usr/bin/omatree-egui` added | yes | yes |
| Desktop entries | `OmaTree` (`omatree %f`) and `OmaTree (egui)` (`omatree-egui %f`), own icons | same |
| MIME | two XML files under their own names (identical `application/x-omatree` definition); **no `mimeapps.list` written** by either package, so no preferred application is replaced | same |
| Metadata version | AppStream first release `0.2.0`, `pacman -Q` `0.2.0-1` | `dpkg` `0.2.0` |
| Removal | `pacman -R`: Qt intact (`pacman -Qk`: 39 files, 0 missing) | `apt purge`: 0 egui files left, Qt intact |

Not tested: which application a desktop environment offers by default when both are registered (no desktop was installed in the
containers). The application shows no version in its UI; the version is visible in package metadata only. The E12b identity is unchanged
and no E13 renaming was done.

## 5. Validation results (working tree, Linux x86_64, Rust 1.99.0)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean |
| `cargo test --locked --workspace --no-fail-fast` | all passed: Qt 43; core 374 (+4 ignored); editor 248 (+4); egui 387 (+16) |
| `cargo build --locked --release --workspace` | ok |
| `packaging/egui/tests/verify-sources.sh` | all checks passed (including version consistency at 0.2.0 and the licensing section) |
| `python3 packaging/egui/third-party.py check THIRD_PARTY.md` | current (300 crates) |
| `tools/pre-e9-compat.sh` | ok: v3 opens; v4, v4-pruned, v5-tagged refused by the pre-E9 core, files unchanged |
| Package verifiers (`verify-package.sh`) on the three candidates | all passed, 0 failures, 0 skips |
| `debian/test-install.sh` (`debian:13`, `ubuntu:24.04`); `appimage/test-container.sh` (`ubuntu:20.04`, `debian:12`) | failures 0 |

No test was modified to force a pass. The macOS-only `ulimit -v` test was not run on a Mac (passes on Linux).

## 6. Licensing gate: BLOCKED for public release

Decided by the maintainer: `MIT OR Apache-2.0` continues. Recorded and checked. **Not resolved** (and not declared resolved; full list in
`docs/LICENSING_INVENTORY_0.2.0.md`, `docs/R3_LICENSING_REPORT.md`, `THIRD_PARTY.md`):

1. **Logo and icon provenance** (SVG logo with an embedded raster, derived PNG icons, `windows/omatree.ico`): author, tool and terms are
   documented nowhere; they ship in every package. **The maintainer must state their origin and distribution terms.**
2. Editor crate: maintainer's confirmation that the copyright wording "OmaTree contributors" and distribution under `MIT OR Apache-2.0`
   are intended for code imported from a repository that had no licence file and was developed primarily with AI assistance (recorded
   as provenance; no legal conclusion drawn).
3. Contributor identity ("Martin Rudinec" commits; one outside contribution of 7 lines with no contributor agreement).
4. Liberation fonts: exact release and "unmodified" unverified against upstream.
5. Debian ships the Apache text by reference (Debian convention).

Closed in R3: the three notice copies highlighted in R2 are identical to the repositories at the exact commits recorded in the locked
crates (krilla 0.8.2, profiling 1.0.18, gl_generator 0.14.0 / khronos_api 3.1.0).

## 7. Release build procedure: `packaging/egui/release-build.sh`

```bash
packaging/egui/release-build.sh --version 0.2.0 --ref v0.2.0   # final build from the tag
packaging/egui/release-build.sh --version 0.2.0                # candidate from a clean HEAD on egui-integration or main
```

It: refuses a HEAD build on another branch or with any modified, staged or untracked file; accepts `--ref` as a commit or an **annotated** tag
(a lightweight `v*` tag is refused); checks out that commit in a temporary detached worktree (so no tracked file is touched and everything
traces to one commit); requires the version to be plain `X.Y.Z` and identical in the four manifests, AppStream's newest release and the Debian
changelog, and `cargo metadata --locked` to succeed without changing a file; runs `verify-sources.sh`; builds with the three egui scripts
(`--locked`, pinned containers; Arch with `makepkg`), reports a failing step with the tail of its log; runs `verify-package.sh` on each
artifact; copies exactly the three deterministically named artifacts to `dist/egui-release-X.Y.Z/`; writes `SHA256SUMS` only for a build from
the tag `vX.Y.Z` (otherwise `SHA256SUMS.candidate`) and `PROVENANCE.txt` (commit, tree, toolchains, image IDs, hashes). It never creates or pushes a
tag, never uploads, never publishes, and never calls the Qt release scripts.

Tested: the argument and state refusals (no version, bad version, unknown ref, dirty tree) in this repository, and a full successful run on a
scratch copy (below). **Not tested:** a build with `--ref` of a real tag (none exists), and the lightweight-tag refusal path.

## 8. Final Linux candidate testing (packaged binaries, native Omarchy Wayland, isolated `HOME`, disposable notebooks)

Binaries were extracted from the candidate Arch package and `.deb`, and the AppImage was run with `APPIMAGE_EXTRACT_AND_RUN=1`; no
system installation. Screens were read; files were read back with `sqlite3`. File dialogs were set to typed paths.

| Path | Arch binary | `.deb` binary | AppImage |
|---|---|---|---|
| Launch (class `io.github.Dorotabro.OmaTreeEgui`, title) | pass | pass | pass |
| Create a notebook and a note, **save** (typed path), schema 5, integrity ok | pass | n/a (fixture) | n/a (fixture) |
| Open and **reopen** (tag, table, image, expansion state intact) | pass | pass (fixture) | pass (fixture) |
| **Image** paste from the clipboard, stored as an attachment | pass | pass | pass |
| **Colour tag** from the header picker, tree dot and collapsed cue | pass | pass | pass |
| **Styled table** (bold, italic, code, strikethrough, link) | pass | pass | pass |
| **Export**: ZIP (image notes default to ZIP; ZIP holds the note and its image), PDF (1 page, creator OmaTree), Markdown | ZIP, PDF pass | Markdown pass | ZIP pass |
| **Recovery**: delete then restore from Trash | pass | pass | not run |
| **Close protection**: unsaved edit, Save/Discard/Cancel, Discard confirmation, nothing saved afterwards | pass (also: a close with an export question open is refused with "Finish or cancel the open question") | pass | pass |

Notes: (1) one run showed a stray second pasted image; it was traced to my test input (a shortcut modifier left pressed by the input tool; plain typing of
the same text pasted nothing), not to the application, but it was not reproduced in isolation. (2) The first typed line after the title Enter was
lost once when typed within milliseconds of the key; not reproduced at human speed and not investigated further. (3) Not exercised in this
step: the native portal dialogs, drag and drop, Compact Notebook, X11, Hi-DPI.

## 9. Candidate provenance and checksums

Built with the new script from a **scratch Git repository** that holds a copy of the working tree (a real commit does not exist yet), so the
commit below cannot be fetched from GitHub. The packaged content equals the current working tree except three files that are not shipped
(`release-build.sh`, `docs/PACKAGING_EGUI.md`, `docs/RELEASING.md`).

| Field | Value |
|---|---|
| Kind | CANDIDATE: not a release artifact |
| Source | scratch commit `664c5fabf60859990d9b2f96b1a01c9c53f78431` (tree `59e560b170906fd9c7329d7858e7c0e900771f9d`), branch `egui-integration` of the scratch repo |
| Rust | host `rustc 1.99.0 (b940084d7 2026-09-28)`; containers rustup 1.99.0 (Debian 13 and Ubuntu 20.04 images, pinned by digest) |
| Environment | Linux 7.2.8 (Omarchy), glibc 2.44, Docker 29.9.2, makepkg 7.1.0; `cargo build --locked`, crates.io only; this candidate reused the Cargo cache that R2 filled from a fresh start (a final build should use a fresh `OMATREE_CACHE` and `CARGO_HOME`) |
| Built (UTC) | 2026-10-31 |

| File | Package version | SHA-256 |
|---|---|---|
| `omatree-egui-0.2.0-1-x86_64.pkg.tar.zst` | pkgver `0.2.0-1` | `442dab04a0446ffd0aa54369b5db26d5d618083b63a68d229764289a698459c3` |
| `omatree-egui_0.2.0_amd64.deb` | `0.2.0` | `ac5b64ac48ad0942b1156590043657f57b846b263f9f4f0fa2086b353eeeb90e` |
| `OmaTree-egui-0.2.0-x86_64.AppImage` | `0.2.0` (file name) | `bdd56d97e6b683fd72eb6a89207c6d946114c39fa9283a68d301d6abb1e77a39` |

**These hashes are not release hashes.** The source will change at commit, and the final packages come from the tag. **Final checksums
process:** `release-build.sh --ref v0.2.0` writes `SHA256SUMS`; then download the files **attached to the draft release** into an empty
directory and run `sha256sum --check SHA256SUMS`; publish only if it passes.

## 10. Remaining blockers before publication

1. Licensing gate (section 6), above all the logo/icon provenance statement.
2. Maintainer authorization to commit, tag and upload (none given).
3. Merge to `main` and the validation re-run on the exact commit; tag; final build from the tag; smoke test of the final packages;
   checksum check of the attached files.
4. Optional, recommended: a real-system install of the final package by the maintainer (not done by me), a portal-dialog check, and
   deciding whether the README or release notes should state the AI-assisted development.

## 11. Commands the maintainer runs (nothing below has been run)

```bash
# 1. commit (review `git status` and `git diff` first)
git add -A
git commit -m "R3: Prepare OmaTree egui 0.2.0 Linux prerelease"
git push origin egui-integration

# 2. merge to main (docs/RELEASING.md section 4), then on main:
git switch main && git pull --ff-only
cargo fmt --all -- --check && cargo clippy --locked --workspace --all-targets -- -D warnings \
  && cargo test --locked --workspace --no-fail-fast && cargo build --locked --release --workspace \
  && packaging/egui/tests/verify-sources.sh && python3 packaging/egui/third-party.py check THIRD_PARTY.md \
  && tools/pre-e9-compat.sh

# 3. only after the licensing gate is cleared and you decide to tag
git tag -a v0.2.0 -m "OmaTree egui 0.2.0 (prerelease)"
git push origin v0.2.0

# 4. build the final artifacts from the tag
packaging/egui/release-build.sh --version 0.2.0 --ref v0.2.0      # -> dist/egui-release-0.2.0/

# 5. smoke-test the final packages (docs/RELEASING.md section 8), then create a DRAFT prerelease
gh release create v0.2.0 --draft --prerelease --verify-tag \
  --title "OmaTree egui 0.2.0 (prerelease)" --notes-file docs/RELEASE_NOTES_0.2.0.md
gh release upload v0.2.0 \
  dist/egui-release-0.2.0/omatree-egui-0.2.0-1-x86_64.pkg.tar.zst \
  dist/egui-release-0.2.0/omatree-egui_0.2.0_amd64.deb \
  dist/egui-release-0.2.0/OmaTree-egui-0.2.0-x86_64.AppImage \
  dist/egui-release-0.2.0/SHA256SUMS

# 6. verify the attached files, then (only then) publish as a prerelease; keep it off "latest"
D=$(mktemp -d) && gh release download v0.2.0 -D "$D" && (cd "$D" && sha256sum --check SHA256SUMS)
gh release edit v0.2.0 --draft=false --prerelease
```

Draft-release notes: `docs/RELEASE_NOTES_0.2.0.md` is a draft with an HTML comment at the top (it is invisible when rendered; delete it if
you prefer). Its links point at the `v0.2.0` tag, which must exist first. Check its file list against the attached files.

## 12. Recommendation

- **Commit: GO.**
- **Tag: HOLD** until the licensing items in section 6 (at least logo/icon provenance) are answered; technically ready after merge and re-validation.
- **Final build: GO once the tag exists** (script and process verified on a candidate).
- **Publish: NO-GO (BLOCKED)** until licensing is resolved and final artifacts are built from the tag, smoke-tested and checksum-verified.
