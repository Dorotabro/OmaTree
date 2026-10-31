# R2: Linux prerelease stabilisation report

Ticket R2, on `egui-integration` at `1564c59` (R1), working tree **uncommitted**. Goal: a verified Linux x86_64
release candidate of the egui version (proposed `0.2.0`) for a GitHub prerelease beside Qt v0.1.2. **No commit, tag,
push, publication, E13 cut-over, Qt removal, schema change or version change in the working tree**; no real notebook
was opened; nothing was installed into the normal system.

## 1. Release model

- **Identity unchanged:** `io.github.Dorotabro.OmaTreeEgui`, binary `omatree-egui`, packages `omatree-egui`, desktop
  `Name=OmaTree (egui)`. Qt packaging, `omastore.toml` and the published v0.1.0–v0.1.2 tags are untouched.
- **Version 0.2.0 verified by building it**, in a scratch copy of the working tree (`git ls-files -co`, in
  `~/.cache/omatree-r2/rc`, its own throw-away Git repository) with only the version locations changed: the four
  `Cargo.toml`, the four workspace entries of `Cargo.lock`, a new AppStream `<release version="0.2.0">`, a new Debian
  changelog entry. Results: all three build scripts accept it; Arch `pkgver` is `0.2.0-1` (a plain number is legal;
  `-rc.1` would not be); Debian `Version: 0.2.0`; `verify-sources.sh` passes its version-consistency check
  ("egui crate (0.2.0) = Qt crate (0.2.0) = core (0.2.0)", "AppStream and Debian changelog match"); the 0.2.0 tag cannot
  collide with `v0.1.2`.
- **GitHub prerelease flag (standard GitHub behaviour, not exercised here):** a release created with `--prerelease` is
  not offered as "Latest", so `releases/latest` keeps pointing at v0.1.2 (Qt). That is the intended coexistence. It also
  means anything that follows "latest" (including OmaStore through `omastore.toml`, which names the Qt asset) keeps
  seeing v0.1.2. `docs/RELEASING.md` already says never to mark it latest.
- **Exact proposed artifacts (pre-cut-over names):**
  `omatree-egui-0.2.0-1-x86_64.pkg.tar.zst`, `omatree-egui_0.2.0_amd64.deb`, `OmaTree-egui-0.2.0-x86_64.AppImage`,
  `SHA256SUMS`. **Tag:** annotated `v0.2.0` on the merge commit, after validation on exactly that commit
  (`docs/RELEASING.md` §5).

## 2. Defects fixed (working tree, uncommitted)

| Defect | Cause | Fix | Evidence |
|---|---|---|---|
| **Debug-only assertion on right-clicking an elided tree title** (T1 finding) | `Response::show_tooltip_text` builds its popup layer from `row_id.with("popup")`, which is also the id `Popup::context_menu` uses for the same row. egui's `widget_rect.rs:228` asserts when that widget id moves from the `Tooltip` layer to the menu's `Foreground` layer in one frame. | `tree_ui.rs`: the elided-title tooltip is an `egui::Tooltip::always_open` with its own id (`resp.id.with("elided-title")`). | Reproduced three ways: headless test failing with the exact assertion; **real window, pre-fix `HEAD` debug build: the process panicked on the right-click with the same message** (log captured); fixed debug build: right-click opens the menu, process alive, log empty. |
| **Tooltip stayed on top of the row's open menu** (found during the GUI check) | The tooltip is shown whenever the elided row is hovered, including while the menu is open under the pointer. | Not shown while the row's menu is open or opening (`Popup::is_id_open` / `secondary_clicked`). | Real-window screenshot before the fix; test `the_tooltip_gives_way_to_the_menu_of_the_same_row`. |
| Stale compatibility text ("schema version 4") | Written before T1 | AppStream description, Debian long description: lazy rules stated (3 stays 3; image → 4; colour tag → 5; never lowered; 0.1.2 and earlier refuse 4 and 5). `docs/PACKAGING_EGUI.md`: new "Notebook compatibility" table. README: "never lowered". `packaging/egui/licences/SOURCES.md`: removed the obsolete "editor has no licence" sentence. | `verify-sources.sh`, package verifiers. |

**New tests (4, `crates/egui-app/src/tests/elided_title_tests.rs`, run in the debug profile where the assertion is armed):**
right-click on an elided title under the pointer opens the menu without asserting and selects the row; the menu of an elided
row still picks a colour (keyboard route) and Up/Down selection keeps working; the elided-title tooltip still shows when no menu
is open; the tooltip gives way to the menu. Not done: a negative control for the last test (the first test is the
regression test and was seen failing with the assertion before the fix).

## 3. Linux GUI verification (actual windows, separate from automated tests)

Release build `target/release/omatree-egui` (fixed tree) unless noted; native **Omarchy / Hyprland 0.56 Wayland**, 1920×1080;
isolated `HOME`, `XDG_CONFIG_HOME`, `XDG_STATE_HOME` with a *copy* of the Omarchy palette; disposable notebooks made with
`omatree-core`; input by `ydotool`/`wtype` (keystrokes only after the app window had focus); `grim` screenshots read one by
one. The window was floated at 1000×640. The user's real `~/.config/omatree` was not touched (directory timestamp unchanged).

| Area | Result |
|---|---|
| Colour tags, collapsed-descendant cue | **Pass.** Collapsed "Projects" shows the red and blue cues; expanded "Ideas" shows a tiny blue cue, "Bugs" its red dot. |
| Elided title: tooltip, right-click, menu (release **and** debug) | **Pass** after the fix; **pre-fix debug build crashed** (see §2). |
| Header picker, mouse | **Pass.** Assign green, clear ("Colour cleared"), assign again; tree dot followed each time; autosaved; file read back with `sqlite3` (colour column, `user_version` 5). |
| Header picker, keyboard | **Pass.** Click title, Tab into the swatches, Right, Right, Enter picked yellow; autosaved; Escape returned to the tree. |
| Styled table and raw source | **Pass.** Grid showed bold, italic, code, strikethrough and link without delimiters; caret in the table showed the full raw Markdown with dimmed delimiters; typing `ZZ` then Ctrl+Z restored the stored body byte for byte (no `ZZ` after autosave). |
| Light/dark live transition | **Pass.** Omarchy theme file replaced by catppuccin-latte, then by the original dark one: window restyled each time within ~3 s; tags and table legible. |
| Autosave | **Pass.** Typed text reached the file within seconds. |
| Close guard, confirmed Discard | **Pass.** Window close with unsaved edits → Save / Discard / Cancel; Escape cancelled and the app stayed; Discard → "Discard changes?" (explicit confirmation) → exit. After Discard the file held only the earlier autosaved text, **no later autosave** (waited 8 s); autosave also does not fire while a dialog is open. The "Keep editing" button was **not clicked** in this session (my click landed on Discard); it is covered by `lifecycle_tests`. |
| Save/reopen, schema v5 | **Pass.** Reopened a notebook with tags: tags, expansion state and content intact; `user_version` 5, `integrity_check` ok. |
| Export Markdown, ZIP, PDF | **Pass** (dialog set to typed paths with `OMATREE_FILE_DIALOG=typed`): `.md` identical to the note, ZIP contains `Tables.md` (126 bytes), PDF is one page, text extractable, creator `OmaTree`, title `Tables`. The native portal dialog was **not** exercised in R2. |
| Trash restore | **Pass.** Delete with subtree → "Move to Trash" confirmation → Recovery showed "Projects · 4 notes" → Restore brought back all four notes with their tags. |
| Checkpoint restore | **Pass.** "Replace the notebook?" confirmation; restoring "Before restoring Projects from Trash" removed Projects again; notebook still schema 5, integrity ok. |
| Packaged binary (extracted Arch package) | **Pass.** Window class `io.github.Dorotabro.OmaTreeEgui`, title `nb5.omatree - OmaTree`; edit autosaved; closed; reopened, text present. |
| AppImage (`APPIMAGE_EXTRACT_AND_RUN=1`) | **Pass** (native Wayland): window class and title correct, closed cleanly. |

**Not tested in R2:** tree drag and drop, the tree menu's colour picker from the keyboard in the real window (covered by tests),
clipboard image paste, Compact Notebook, native portal dialogs, native X11, Hi-DPI, IME, screen reader, narrow-window layouts,
Windows, macOS. The screenshot window was translucent (Omarchy opacity), which makes the images noisy but not wrong.

## 4. Automated validation (Linux, final working tree)

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | clean (after formatting the new test file) |
| `cargo clippy --locked --workspace --all-targets -- -D warnings` | clean |
| `cargo test --locked --workspace --no-fail-fast` | **all passed**: Qt 43; core 374 (+4 ignored); editor 248 (+4); egui **387** (+16; was 383, +4 new). The macOS `ulimit -v` test passes on Linux. |
| `cargo build --locked --release --workspace` | ok |
| `packaging/egui/tests/verify-sources.sh` | all checks passed (also on the 0.2.0 scratch copy) |
| `python3 packaging/egui/third-party.py check THIRD_PARTY.md` | current (300 crates) |
| `tools/pre-e9-compat.sh` | ok: v3 opens; v4, v4-pruned and v5-tagged refused, files unchanged |

No test was modified to force a pass; the only changes to existing test code are the one-line module registration.
Environmental notes: `/tmp` is a 5.8 GB tmpfs, so the package builds ran from `~/.cache/omatree-r2`; the macOS `ulimit -v` failure
(R1) does not occur on Linux and was not re-run on a Mac.

## 5. Package verification (candidates, not release artifacts)

Built with the supported scripts only (`packaging/egui/{debian,appimage,arch}/build-package.sh`), from the scratch copy of the
current source with the 0.2.0 version applied, **fresh `OMATREE_CACHE` and `CARGO_HOME`** (all crates downloaded from crates.io,
no Git source), network on. No Qt script was used. Built 2026-10-31.

| File | SHA-256 |
|---|---|
| `omatree-egui-0.2.0-1-x86_64.pkg.tar.zst` | `3ec651e6da09929bfccea918939e6542259c89a7db122a8e6c023f055b327767` |
| `omatree-egui_0.2.0_amd64.deb` | `20c9cde711ed45eadff2af2048dbdf85f66db097aa2ecd4d5951064f76cc5104` |
| `OmaTree-egui-0.2.0-x86_64.AppImage` | `94b3f6b1135edd8ce2cf61d2264af654591a5668f9a3b7df9e0d1017222db8ec` |

(These hashes belong to the candidates only. Final artifacts are built from the tag and will differ.)

- `packaging/egui/verify-package.sh` on each: **all checks passed** (Arch 60, Debian 79, AppImage 71 `ok`, no `skip`, no failure):
  binary and identity, desktop entry, MIME, icons, AppStream, licence files, no Qt, no build paths.
- Contents: Arch package has 40 files: `usr/bin/omatree-egui`, desktop file, MIME XML, AppStream, hicolor icons (32–512 + SVG),
  `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY.md`, `third-party-licenses.txt`, README. Packaged binary contains the
  colour-tag UI and the new tooltip id (`strings` check). Version `0.2.0-1`, licences `MIT Apache-2.0`.
- `lintian` (Debian): 0 errors; `W: no-manual-page`, `I: hardening-no-fortify-functions` (as before).
- **Clean install and removal in isolated environments:** `apt install` / `purge` in `debian:13` and `ubuntu:24.04`
  (`test-install.sh`: failures 0, no package-owned file remains); Arch package `pacman -U` / `-R` in an `archlinux:latest`
  container (`pacman -Qk`: 40 files, 0 missing; after removal 0 package files left); AppImage ran on X11 in `ubuntu:20.04` and
  `debian:12` containers (`test-container.sh`: failures 0).
- **Installed application launch and persistence:** see §3 (extracted Arch package and AppImage on the real Wayland session).
- File association and desktop integration: checked by the verifiers and `verify-sources.sh` (MIME glob `*.omatree` only, hostile-file-name
  launch through `gio`, isolated MIME database). Opening from a file manager was **not** exercised on a real installation.
- **Not done:** installation on a real system (no permission asked), `namcap`, a real portal dialog from the packaged build,
  Wayland inside the old-distribution containers, native X11.

## 6. Licensing and source audit

Facts (no ownership, authorship or licence is asserted by R2; nothing in the licensing was changed):

- Root `Cargo.toml` and all three workspace crates: `MIT OR Apache-2.0`. `LICENSE-MIT`: "Copyright (c) 2026 OmaTree contributors".
  `LICENSE-APACHE`: the standard text. Debian `copyright`: "2026 OmaTree contributors", "MIT or Apache-2.0".
  The editor crate (`crates/editor`) is part of the repository and the repository is public.
- Compiled dependencies: 300 crates, table current; no copyleft-only licence (the single `Apache-2.0 OR GPL-2.0-only` is taken
  under Apache-2.0); licence texts and notices (including OFL/UFL for fonts and `subsetter`'s NOTICE) are in
  `third-party-licenses.txt`, shipped by all three packages with `LICENSE-MIT`, `LICENSE-APACHE` and `THIRD_PARTY.md`.

**Still requiring maintainer decisions (release blockers for publication):**

1. **Editor ownership and licence basis.** `THIRD_PARTY.md` states that the maintainer's *intention* is to distribute the editor
   under OmaTree's licence, that the former prototype repository carried no licence file at the imported revision `30a95dd`,
   that no individual copyright holder is named, and that the status of AI-assisted code is not addressed. The maintainer
   must confirm the copyright holder wording (is "OmaTree contributors" right for the editor?) and that the project may
   distribute the editor under `MIT OR Apache-2.0`. **Blocker until confirmed.**
2. Whether `LICENSE-MIT`'s "OmaTree contributors" line and the Debian `copyright` line are the intended notice for a public release.
3. Non-blocking, maintainer to accept or schedule: 16 crates' notices are *upstream copies*, three of them from branch heads
   (krilla, profiling, gl-rs) rather than the locked version's tag; the Liberation fonts' exact upstream release is not recorded.

**Update (R3):** the maintainer authorized continuing `MIT OR Apache-2.0`; the decision, the AI-assistance provenance and the remaining unresolved items are recorded in `THIRD_PARTY.md` and `docs/R3_LICENSING_REPORT.md`. Blocker 1 above is therefore decided; the items named in that report remain open.

## 7. Version and identity status

Working tree still `0.1.2` everywhere (as released Qt). Identity unchanged. Candidate artifacts prove `0.2.0` works with every
script and verifier. No code reads the version at run time (no About box, `--version` refused), so a user can check the version
only in package metadata (a future improvement).

## 8. Exact steps remaining before tagging

1. Maintainer approves: version `0.2.0`, Linux-only prerelease side by side with Qt, and the editor licence/copyright basis (§6).
2. Commit the R2 changes (`tree_ui.rs`, `elided_title_tests.rs`, `tests.rs`, metainfo, Debian control, docs). Not done by R2.
3. **Version-bump commit**, changing exactly: `Cargo.toml`, `crates/{core,editor,egui-app}/Cargo.toml` (`0.1.2` → `0.2.0`);
   the four workspace entries in `Cargo.lock`; a new first `<release version="0.2.0" date="…">` in
   `packaging/egui/linux/io.github.Dorotabro.OmaTreeEgui.metainfo.xml` (decide `type="development"`); a new top entry in
   `packaging/egui/debian/debian/changelog`; filename examples in `docs/PACKAGING_EGUI.md` and the script header comments.
   Do **not** touch the Qt metainfo or Qt changelog (they are the released 0.1.2). The recipe was exercised in the scratch copy.
4. Re-run §4 on that commit; push the branch; open the PR to `main`; merge (`docs/RELEASING.md` §4).
5. From a clean clone of the merge commit with an empty Cargo home: `cargo build --locked --release -p omatree-egui`
   (R1 did this for `40ffcec`; repeat for the final commit).
6. Tag `v0.2.0`; build the three packages from a clean worktree of the tag; verify; isolated install tests; checksums; smoke
   test on Linux (`docs/RELEASING.md` §5–§9). A smoke test of the *installed* package on a real system needs the maintainer's permission.
7. Draft prerelease, upload, release notes (compatibility warning, Linux only, known limitations), publish only with approval.
8. Optional before release: `namcap` on the Arch package; a real portal-dialog check from the packaged build; the
   remaining "not tested" items in §3.

## 9. Go / no-go for R3

**GO for R3 (tag preparation and tagged build) on technical grounds:** the only known debug defect is understood, fixed and
regression-tested; stale compatibility text is corrected; the Linux GUI check of M1/T1–T3 features passed on native Wayland; the
automated suite, source verification, licence-table check and compatibility script pass; all three candidate packages build from
a clean cache, verify, install and remove cleanly in isolated environments, and run.

**NO-GO for publication** until blocker 1 of §6 (editor ownership and licence basis) is confirmed by the maintainer, and the
maintainer approves the version and the Linux-only, side-by-side shape. Windows and macOS remain out of scope.
