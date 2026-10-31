# R1: release-readiness audit (first public egui prerelease)

Ticket R1, on `egui-integration` at `40ffcec` (T3). It prepares the documentation, plans the release and
lists what stands in the way. **Nothing was committed, tagged, pushed, published or installed; no E13
cut-over; no schema, feature or version change; no real notebook was opened.** Findings come from reading the
scripts and source plus the checks in "Validation"; a script that exists is **not** evidence that it works
unless a record says it was run.

**Update:** R2 (`docs/R2_RELEASE_REPORT.md`) fixed the stale schema text (B5), fixed the elided-title assertion, rebuilt and verified candidate Linux packages (B3, candidate level) and ran a Linux GUI check (B4). B1 and B2 remain open.

Related: `docs/RELEASING.md` (procedure), `docs/screenshots/README.md` (images), `docs/PACKAGING_EGUI.md`,
`docs/EGUI_MIGRATION_PLAN.md` (E12a–E13a, M1, T1–T3 records).

## 1. Summary

| | |
|---|---|
| What can be released | **Linux x86_64 only**: Arch / Omarchy package, Debian/Ubuntu `.deb`, AppImage (egui version). |
| What cannot | Windows and macOS packages of the egui version **do not exist**. |
| Source | The repository is already **public** on GitHub (checked with `gh repo view`: `PUBLIC`), `origin/egui-integration` equals the local branch, the editor is the workspace crate, `Cargo.lock` has no Git source. |
| Packages on disk | `dist/*egui*` were built on 2026-10-28, **before** M1, T1, T2, T3 and still carry version 0.1.2. They are stale and must not be released. |
| Blockers | 5 release blockers (section 6): two are maintainer decisions (B1, B2), three are work (B3–B5). |

## 2. Version strategy

**Existing identifiers.**

| Item | Value |
|---|---|
| Git tags | `v0.1.0` (2026-10-05), `v0.1.1` (2026-10-06), `v0.1.2` (2026-10-11): all **Qt** releases |
| Branches | `main`, `release-0.1.2`, `windows-compat`, `egui-integration` (all on `origin`) |
| Workspace version | `0.1.2` in the root, `crates/core`, `crates/editor`, `crates/egui-app` (**equal to the released Qt version**) |
| Notebook schema | up to 5 (3 → 4 attachments, 5 colour tags); raised only when needed |
| Application identity (egui) | `io.github.Dorotabro.OmaTreeEgui`, binary `omatree-egui`, package `omatree-egui`, desktop `Name=OmaTree (egui)` |
| Application identity (Qt) | `io.github.Dorotabro.OmaTree`, binary `omatree` |

**Problem.** Any egui artifact called 0.1.2 would collide with the Qt `v0.1.2` release (same tag, near-identical file
names). A SemVer suffix such as `-rc.1` cannot be used in `Cargo.toml`: `packaging/egui/arch/build-package.sh` and the
Debian and AppImage scripts copy the version verbatim into the Arch `pkgver` (hyphens are illegal), the Debian version
and the file names, and `verify-sources.sh` demands that the crates, the AppStream release and the Debian changelog
carry the same string.

**Recommendation: `0.2.0`, tag `v0.2.0`, published on GitHub as a pre-release** (`--prerelease`, never "latest").
Reason: a new toolkit and a notebook format that older releases refuse is a minor bump in 0.x, the number is free, and
the "prerelease" status is carried by the GitHub flag, the release notes and the README, not by a string the
packaging cannot represent. If a visible suffix is wanted (`0.2.0-rc.1` in SemVer, `0.2.0~rc1` for Debian,
`0.2.0rc1` for Arch), that is a packaging change and a separate ticket; it is not recommended for the first release.
If the maintainer prefers a lower number for the prerelease (for example `0.2.0` only for the final), the same rule
holds: plain `X.Y.Z`.

**Release artifact names** (as the scripts build them today, egui side by side with Qt):

| File | Platform |
|---|---|
| `omatree-egui-0.2.0-1-x86_64.pkg.tar.zst` | Arch / Omarchy |
| `omatree-egui_0.2.0_amd64.deb` | Debian 13+, Ubuntu 24.04+ |
| `OmaTree-egui-0.2.0-x86_64.AppImage` | other x86_64 Linux, glibc 2.30+ |
| `SHA256SUMS` | the three lines above |

After the E13 cut-over the names lose `-egui` (`omatree-0.2.0-1-x86_64.pkg.tar.zst`, `omatree_0.2.0_amd64.deb`,
`OmaTree-0.2.0-x86_64.AppImage`). That also matches the pattern in `omastore.toml`
(`omatree-{version}-*-x86_64.pkg.tar.zst`, `usr/bin/omatree`), which today names the **Qt** package.

**Every location that must change together** (`0.1.2` → `0.2.0`):

| Location | Notes |
|---|---|
| `Cargo.toml` (root, Qt package `omatree`) | `version`; checked equal to the egui crate by `verify-sources.sh` |
| `crates/core/Cargo.toml` | `version` |
| `crates/editor/Cargo.toml` | `version` |
| `crates/egui-app/Cargo.toml` | `version`; the scripts read it from here |
| `Cargo.lock` | the four workspace packages; refreshed by a normal `cargo build -p omatree-egui` (not `--locked`); commit the result |
| `packaging/egui/linux/io.github.Dorotabro.OmaTreeEgui.metainfo.xml` | new first `<release version="0.2.0" date="…">`; `type="development"` is for development builds, decide it; the description still says "schema version 4", now stale (5) |
| `packaging/egui/debian/debian/changelog` | new top entry `omatree-egui (0.2.0) …` with date and author line |
| `docs/PACKAGING_EGUI.md` | file names and command examples contain `0.1.2` |
| script header comments in `packaging/egui/{verify-package.sh, debian/test-install.sh, appimage/test-container.sh}` | examples only |
| `README.md` | mentions no version for egui today |
| Qt-only (change only if Qt is still shipped): `packaging/linux/…OmaTree.metainfo.xml`, `packaging/debian/debian/changelog` | at the cut-over they are deleted |
| Derived automatically: `packaging/windows/OmaTree.iss` (`/DAppVersion` from `Cargo.toml`), `packaging/macos/Info.plist.in` (`@VERSION@`), PKGBUILD (`@PKGVER@`) | no edit |

The application does not display its version anywhere (no About box, `--version` is refused as an option-like
argument), so a user cannot read the version from the running program: see "Future improvements".

## 3. Platform release inventory

Legend: **egui** = packaging for the egui version, **Qt (legacy)** = packaging that exists in the repository for the Qt
version only. "Record" is what the repository's own records say was *run*; "R1" is what I did.

| Item | Where | State | Evidence |
|---|---|---|---|
| Arch package (egui) | `packaging/egui/arch/{PKGBUILD.in,build-package.sh,verify-arch.sh}` | **Working, verified for 0.1.2 on the E12b/P2 tree** | Record: built with `makepkg`, `verify-package.sh` passed, installed into a private root and run on native Wayland. Not installed on a real system. `namcap` never run. R1: `verify-sources.sh` only; not rebuilt. |
| Debian package (egui) | `packaging/egui/debian/…` (Debian 13 container) | **Working, verified** | Record: built, `lintian` 0 errors, `apt install`/`purge` in `debian:13` and `ubuntu:24.04`, run under Xvfb and Weston. Refused on Debian 12 / Ubuntu 22.04 (glibc ≥ 2.39). R1: not rebuilt. |
| AppImage (egui) | `packaging/egui/appimage/…` (Ubuntu 20.04 container) | **Working, verified** | Record: run on Ubuntu 20.04 and Debian 12 (X11) and on native Wayland on the dev machine. Bundles no libraries (host needs Wayland/X11, GL, fontconfig, dbus). No update information, no `appimaged` test. R1: not rebuilt. |
| Windows installer (egui) | none | **Does not exist** | E12c: only a type-check for `x86_64-pc-windows-gnu` (MinGW). No MSVC build, no run. |
| macOS bundle / DMG (egui) | none | **Does not exist** | M1: the binary was run from the build directory on macOS 27 / arm64. No `.app`, `Info.plist`, identity, signing or notarization; the menu bar shows `omatree-egui`. |
| Windows installer (Qt, legacy) | `packaging/windows/{OmaTree.iss,build-installer.ps1,…}`, `windows/omatree.{ico,rc,manifest}` | Qt only; released as v0.1.2 | Builds `OmaTree-<v>-x64-setup.exe`; per-user install, `.omatree` ProgID association. **Cannot package the egui binary as is.** |
| macOS DMG (Qt, legacy) | `packaging/macos/{build-dmg.sh,Info.plist.in,verify-app.sh,licenses…}` | Qt only; released as v0.1.2 | `macdeployqt` bundle of the Qt build, arm64, with an `.icns` made by `iconutil`. No `CFBundleDocumentTypes` in `Info.plist.in`, i.e. Finder opening there relies on Qt. |
| Qt Linux packages / release flow (legacy) | `packaging/{arch,debian,appimage,linux}`, `release-build.sh`, `checksums.sh`, `release-checksums.sh`, `verify-package.sh` | Qt only | Names `omatree-*`; `release-checksums.sh` lists exactly five Qt files and stops if any is missing. **Must not be used for the egui release.** |
| Application icons | `assets/logo/omatree-icon-{32,64,128,256,512,1024}.png`, `omatree-logo.svg`; egui embeds the 256 px PNG; installed hicolor + symbolic icons named `io.github.Dorotabro.OmaTreeEgui` | **Egui Linux: working** (checked by `verify-sources.sh` and in the E12b run) | The Windows `.ico` (`windows/omatree.ico`) and the macOS `.icns` step are part of the Qt flow only. Wayland ignores window icons; the desktop file supplies them. |
| Desktop entry | `packaging/egui/linux/io.github.Dorotabro.OmaTreeEgui.desktop` | **Working**: `Name=OmaTree (egui)`, `Exec=omatree-egui %f`, `MimeType=application/x-omatree;` | `desktop-file-validate` passes (record, `verify-sources.sh`). Name changes to `OmaTree` at E13. |
| File association | MIME XML `…OmaTreeEgui.xml`, glob `*.omatree`, no magic | **Linux working** (isolated MIME database, hostile file name test); the Qt package defines the same MIME type. | Windows (ProgID) and macOS (`CFBundleDocumentTypes`, UTI) associations: none for egui. |
| Licence and notice inclusion | `THIRD_PARTY.md` (generated table, 300 crates), `third-party-licenses.txt` (bundled in every package), `packaging/egui/licences/` (16 upstream copies), `LICENSE-MIT`, `LICENSE-APACHE`, `debian/copyright` | **Linux packages include them**; `verify-sources.sh` fails if the table is stale | Open points are maintainer decisions (section 6). Windows and macOS notice bundles (`packaging/{windows,macos}/licenses`) are Qt's (LGPL/GPL Qt text) and do not apply to egui. |
| Checksums | none for egui except `dist/SHA256SUMS-egui-arch` | **Manual** (`sha256sum`) | `docs/RELEASING.md` step 9 |
| Release automation / CI | none (no `.github/`) | **None** | all steps manual |

## 4. Review of T1–T3, M1 and E12a–E12b for release

- **T1 colour tags, T2 header picker, T3 table inline Markdown, M1 macOS fixes:** implemented and *verified on the
  Mac only*. The records state "Linux and Windows were not run". The three screenshots in `docs/screenshots/` show the
  tags, picker and table rendering, which is a visual confirmation of the build that captured them but not a test
  record. **A Linux smoke test of the final build is therefore a release prerequisite** (`docs/RELEASING.md` step 8).
- **T1 changes the format:** a notebook that has a colour tag is schema 5; Qt v0.1.2 and the pre-T1 egui build refuse it
  (`tools/pre-e9-compat.sh` demonstrates this, and it passes). The README and release notes must say so. The AppStream
  description and `docs/PACKAGING_EGUI.md` still say "schema version 4" and need updating before the release.
- **E12a/E12b:** the Linux packaging evidence in `docs/PACKAGING_EGUI.md` predates M1/T1–T3. The packages must be rebuilt
  from the tag and re-verified; the old verification does not carry over to a new binary.

## 5. Source availability and licensing audit (facts only)

| Question | Finding |
|---|---|
| Is the editor in the repository? | Yes: `crates/editor` (package `omatree-editor`), a workspace member since P2, committed (`40ffcec` contains it). |
| Is anything private still required? | **No.** `verify-sources.sh` passes its checks that no manifest, lock entry or script names an editor Git repository or sibling checkout and that `Cargo.lock` has no `git+` source; `cargo tree` shows no Qt crate. |
| Is the repository public? | **Yes** (`gh repo view`: visibility `PUBLIC`, default branch `main`). `origin/egui-integration` equals the local branch, so the source of this ticket's base is already public. Visibility was not changed. |
| Clean checkout builds? | See "Validation": a clean clone of the committed `HEAD` with an empty `CARGO_HOME`. |
| Editor licence metadata | `crates/editor/Cargo.toml`: `MIT OR Apache-2.0`, same as the root and every workspace crate. `LICENSE-MIT` reads "Copyright (c) 2026 OmaTree contributors". |
| Editor licence **ownership** | **Open, maintainer's**: `THIRD_PARTY.md` flags (a) the copyright line (no individual holder is named), (b) the status of AI-assisted code, (c) that the imported revision `30a95dd…` of the former private prototype carried no licence file. Nothing found says rights are uncertain; nothing documents that they are certain either. R1 assigns and changes no licence. |
| Fonts | Liberation Sans/Mono (SIL OFL-1.1) embedded for PDF export; notice in `crates/editor/assets/fonts/LICENSE-Liberation.txt` and in `third-party-licenses.txt`. The exact upstream release is not recorded. |
| Other third-party | 300 crates in the egui build, generated table in `THIRD_PARTY.md`. **16 crates** declare a licence but do not ship the file; `packaging/egui/licences/` holds upstream copies with `SOURCES.md`; three sources (krilla, profiling, gl-rs) are branch references, not tags. |
| README claims | The old README described only the Qt version; the new one says the egui version is unreleased. |

## 6. Findings, classified

### Release blockers (must be resolved or decided before publishing the prerelease)

| # | Finding | Why | Resolution |
|---|---|---|---|
| B1 | **Maintainer decisions are open**: cut-over versus side-by-side prerelease; canonical app ID / binary / package names; version; which platforms are in the release. | The scripts, names, README and `omastore.toml` all depend on them. | Section 8. |
| B2 | **Editor ownership and copyright statement not confirmed** (section 5). | Public distribution under `MIT OR Apache-2.0` rests on it; `THIRD_PARTY.md` says the maintainer must confirm. | Maintainer; R1 decides nothing. |
| B3 | **The packages in `dist/` are stale** (2026-10-28, before M1/T1–T3, version 0.1.2) and **no egui package has been built or verified from a tag**. | A release must be built from the tag in a clean worktree, verified, and smoke-tested. | `docs/RELEASING.md` steps 5–8. |
| B4 | **Linux smoke test of T1–T3 on the final build is not recorded** (those tickets were verified on macOS). | "Mark GUI behaviour that was not actually exercised as **not tested**." | `docs/RELEASING.md` step 8, recorded in the release notes. |
| B5 | **Stale user-facing text**: AppStream description and `docs/PACKAGING_EGUI.md` say schema 4, version 0.1.2 filenames, and the old private-editor wording is historical. A Qt user who opens a v5 notebook gets a refusal; the notes must warn. | Misinformation in a public package. | Fix at the version-bump commit; release notes carry the warning. |

If the release is **not** Linux-only (the maintainer wants Windows or macOS artifacts), the following additionally
block, because no packaging exists and nothing was run: Windows installer, file association, taskbar identity
(`AppUserModelID`), an actual Windows run (E12c checklist); macOS `.app` bundle with `CFBundleName`, Finder "Open With"
(not implemented), signing and notarization decisions, the pre-existing `ulimit -v` test failure, Dock Quit and
log-out checks.

### Acceptable documented limitations (for a Linux-first prerelease; state in the notes and README)

- Windows and macOS: no egui packages. README says so; Qt v0.1.2 stays available for them.
- **macOS `ulimit -v` test failure**: `integrity_tests::memory_exhaustion_is_an_error_never_an_abort` fails on macOS
  (`ulimit -v` is unsupported there, "cannot modify limit"), identically on a pristine checkout. It **passes on Linux**
  (R1 run: 374 passed in `omatree-core`). Not weakened. A macOS-aware harness is a separate small task and is a blocker
  only for macOS artifacts.
- **Debug-only assertion when right-clicking an elided tree title** (T1 finding): the elided-title tooltip
  (`tree_ui.rs`, `resp.show_tooltip_text(&r.title)`) and the row's context menu share an id, tripping an egui
  `debug_assert` ("popup/move changed layer_id … Tooltip to Foreground"). Release builds, which the packages are, are
  unaffected. **Not reproduced by R1** (no GUI session was driven); the cause is as recorded in T1. It will crash
  debug builds that developers run: a small fix ticket is recommended before wider contribution.
- Native X11 (only Xvfb and XWayland tested), screen reader, real input method, Hi-DPI, Fedora/openSUSE and other
  distributions: unverified. AppImage bundles no graphics libraries.
- `.deb` needs glibc ≥ 2.39 (not installable on Debian 12 / Ubuntu 22.04); the AppImage is the portable format.
- No single-instance IPC: opening a notebook while the app runs starts a second window.
- Packages install next to the Qt package (two launcher entries) until the cut-over; no `mimeapps.list` is written.
- No `namcap` run; no man page (`lintian` warning).
- Native dialogs: a missing portal shows as a cancel in some cases (typed-path fallback exists).
- Checksums only; no signature (no GPG, no Authenticode, no notarization).
- Notebook compatibility: schema 4/5 notebooks are refused by Qt v0.1.2 and the older egui build.
- Table column widths with one very long cell; links in table cells are styled, not clickable; the exporters do not
  render inline Markdown in cells (T3 known limits). Tag keyboard route into the picker is long (T1).

### Future improvements (not release-gating)

- Show the application version (About or `--version`).
- A tag-driven egui release script (egui analogue of `packaging/release-build.sh`) and a checksum script for the new names.
- CI (formatting, clippy, tests, packaging verification).
- Windows (Inno Setup for the egui binary, `AppUserModelID`) and macOS (`.app`, `Info.plist`, Open Document delegate,
  signing) packaging; replace the macOS `ulimit -v` test harness.
- Replace the branch-reference licence copies with tags; record the Liberation font release; run `namcap`.
- Fix the elided-title popup id clash; documentation of Shift+Tab to the notebook menu in F1.
- Optional visible prerelease suffix support in the packaging scripts.

## 7. E13 prerequisites and remaining work

E13 checklist (from the E13a record, status as of R1):

| # | Prerequisite | Status |
|---|---|---|
| 1 | Canonical application ID | **open, maintainer** |
| 2 | Executable and crate names | **open, maintainer** |
| 3 | Desktop, MIME, icon names | open (follows 1–2) |
| 4 | `default-members` / Qt removal from the workspace | open |
| 5 | `Provides/Conflicts/Replaces` for Qt package `omatree` | open |
| 6 | Remove Qt (crate, `build.rs`, `cpp/`, `qml/`, tests, Qt packaging, Qt licence text) | open; keep `crates/core` untouched; keep tag `v0.1.2` and branch `release-0.1.2` for rollback |
| 7 | Release version (this audit: `0.2.0`) | proposal, **needs approval** |
| 8 | Recent-notebooks migration from Qt | open; the egui app reads `~/.config/omatree/recent.txt`; Qt's store was not compared by R1 |
| 9 | Rollback and compatibility documentation | partly (README notebook note, `RELEASING.md` §15) |
| 10 | Rebuild all three packages from the final commit, verify, install once for real on a scratch system | open (B3) |
| 11 | Document unsupported platforms | done in README and section 6 |
| – | Update `omastore.toml` to the new asset names, then check with the `omastore-check` skill | open; today it points at the Qt asset |
| – | Fresh clone of the **committed** revision | see "Validation" |

## 8. Decisions requiring maintainer approval

1. **Release shape:** side-by-side prerelease named `omatree-egui` (no cut-over, no Qt removal) **or** full E13 cut-over
   first. Recommendation: do not cut over for the first prerelease; ship beside Qt, then cut over at the next release.
   (`omastore.toml` names the Qt asset; only the post-cut-over names fit it.)
2. **Platforms in the first prerelease:** Linux x86_64 only (recommended; Windows and macOS packaging does not exist).
3. **Version:** `0.2.0`, tag `v0.2.0`, GitHub prerelease flag (recommended), versus a SemVer suffix (needs script work).
4. **Editor licence and copyright:** confirm the copyright line, the status of AI-assisted code and the missing licence file
   in the former prototype repository. R1 chose and changed none.
5. **Application identity at the cut-over:** keep `io.github.Dorotabro.OmaTree` (carries over Qt users' registrations) or a
   new ID; binary name; package name.
6. **Hero screenshot:** confirm which supplied image was captured on Omarchy, and whether to retake with a saved
   notebook; decide whether to ship without Windows and macOS screenshots (recommended: yes).
7. **Pre-release compatibility policy:** accept that schema 4/5 notebooks are refused by Qt v0.1.2, or add a note/export
   path before releasing.
8. **Small tickets to schedule:** elided-title popup assertion; macOS `ulimit -v` test; app version display; egui release
   script; AppStream/PACKAGING doc refresh (can ride on the version-bump commit).
9. **Approval of the `docs/RELEASING.md` process**, including that the tag is made only after validation passes on the
   exact merge commit and the release stays a prerelease.

## 9. Validation (R1, Linux x86_64, Omarchy, Rust 1.99.0; private `TMPDIR`; no real notebook used)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean (Qt installed here, so the Qt package is included) |
| `cargo test --locked --workspace --no-fail-fast` | **all passed**: Qt 43; `omatree-core` 374 (+4 ignored); `omatree-editor` 248 (+4 ignored; the macOS-only font test is not compiled on Linux, hence one fewer than the Mac record); `omatree-egui` 383 (+16 ignored). The macOS `ulimit -v` test passes on Linux. |
| `cargo build --locked --release --workspace` | ok |
| `packaging/egui/tests/verify-sources.sh` | all checks passed (identity, desktop/MIME, versions, no Git source, no Qt crate, licence table current, user configuration untouched) |
| `python3 packaging/egui/third-party.py check THIRD_PARTY.md` | current (300 crates) |
| `tools/pre-e9-compat.sh` | ok: pre-E9 core opens v3, refuses v4 and v5 and leaves the files unchanged |
| Clean clone of committed `HEAD` (`40ffcec`, branch `egui-integration`, cloned from the local repository), empty `CARGO_HOME`, `GIT_CONFIG_GLOBAL=/dev/null`, no `SSH_AUTH_SOCK`: `cargo build --locked --release -p omatree-egui` | **passed** (7 min); the new Cargo home holds only `registry/`, no `git/`, no Git fetch in the log. Not a clone from GitHub; the remote branch is identical to the local one (`rev-list` 0/0). |
| Packages rebuilt / installed / run | **not done**: out of scope for R1 (they are built from the tag, `docs/RELEASING.md`). No GUI session was driven: GUI behaviour is **not tested** by R1. |
