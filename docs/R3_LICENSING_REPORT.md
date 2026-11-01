# R3: licensing decision and source/package licence readiness

Following the maintainer's decision of 2026-10-31: continue the existing dual licence, **`MIT OR Apache-2.0`**.
Working tree uncommitted; no commit, tag, push or publication; no Rust code, schema or version changed.

## What was inspected first

- `LICENSE-MIT` (standard MIT text, "Copyright (c) 2026 OmaTree contributors") and `LICENSE-APACHE` (standard Apache-2.0 text,
  with the stock appendix template). Both were added in commit `77383b6` (2026-10-03). **Neither was modified.**
  `LICENSE-APACHE` is byte-identical (ignoring line endings) to the unmodified upstream copy in `packaging/egui/licences/`.
- Cargo metadata: root package, `omatree-core`, `omatree-editor`, `omatree-egui` all `license = "MIT OR Apache-2.0"`,
  `repository` the project's; `publish = false`; no `authors` fields.
- Notices already in use: the same "Copyright (c) 2026 OmaTree contributors" in `LICENSE-MIT`, Debian `copyright`, the Windows
  installer resources and the macOS `Info.plist`. No SPDX or copyright headers exist in source files (none are required).
- History: 65 commits by `Dorotabro`, 3 by "Finish Some" (same account email), 8 by "Martin Rudinec" (two local hostnames),
  1 by an outside contributor (`omastore.toml`, PR #1; the file was removed from the tree on 2026-10-31, see item 3 below).

## Requirements, one by one

| # | Requirement | Result |
|---|---|---|
| 1 | Preserve the valid MIT and Apache-2.0 texts | **Done**: unchanged; now guarded by a check against the upstream copy. |
| 2 | Consistent `MIT OR Apache-2.0` | **Done**: the four manifests were already consistent; AppStream `project_license`, Debian `copyright` ("MIT or Apache-2.0", DEP-5 spelling) and the PKGBUILD (`MIT`, `Apache-2.0`) agree. Checked by `verify-sources.sh`. |
| 3 | Both licence files in the source distribution; notices in binary packages | **Verified**: both files and the fonts' notice are tracked (so in every `git archive` and in the source tarball built for the Arch package); Arch package and AppImage install `LICENSE-MIT`, `LICENSE-APACHE`, `THIRD_PARTY.md`, `third-party-licenses.txt` under `usr/share/licenses/omatree-egui/`; the `.deb` follows Debian policy: `copyright` (DEP-5) holds the MIT text and names Apache-2.0, whose text is the one in `/usr/share/common-licenses/Apache-2.0` on every Debian system, plus `THIRD_PARTY.md` and `third-party-licenses.txt` in `usr/share/doc/omatree-egui/`. The `.deb` therefore contains no copy of the Apache text itself; that is the Debian convention, but if a self-contained copy is wanted it is a packaging change for the maintainer to approve. |
| 4 | Existing copyright attribution; no invented holders; no AI-provider ownership | **Done**: the project's existing generic line is kept everywhere; no person, company or AI provider was added. A check fails if an AI provider appears in a copyright line of the licence, Debian copyright, AppStream or manifests. |
| 5 | Separately licensed dependencies, fonts, assets | **Documented and checked**: 300 crates (no copyleft-only; table current), egui default fonts (OFL, Ubuntu Font Licence, Hack), SQLite (public domain), `emojis` data (Unicode-3.0, `LICENSE-UNICODE` in the bundle), the five Liberation fonts (OFL-1.1, notice beside them and in the bundle; **SHA-256 now recorded in `THIRD_PARTY.md` and checked**). The logo and icons are the maintainer's original artwork under `MIT OR Apache-2.0` (recorded 2026-10-31, checked by hash). |
| 6 | Third-party code copied into the project | **Reviewed, none found** by search (headers, "adapted/copied/based on/ported from", licence text, source URLs, vendored directories, data tables). A search cannot prove absence and no similarity analysis was done; stated as such in `THIRD_PARTY.md`. |
| 7 | Flag unresolved issues | **Done**, below and in `THIRD_PARTY.md`, "Rights and attribution issues: status". |
| 8 | Update `THIRD_PARTY.md`, release docs, checks | **Done**: `THIRD_PARTY.md` (decision, AI provenance, copied-code review, separately licensed material, font hashes, open issues); `docs/EGUI_DECISIONS.md` decision 10; `docs/RELEASING.md` (licensing precondition, what `verify-sources.sh` now checks, release-note content); README licence section carries the notice; `verify-sources.sh` has 10 new licensing checks (negative controls: wrong manifest licence, appended text in `LICENSE-APACHE`, changed copyright holder in Debian `copyright`, one changed font byte each fail as intended). |

**AI assistance.** Recorded in `THIRD_PARTY.md` as a provenance fact on the maintainer's confirmation: the application,
including the editor, was developed primarily with AI coding assistance. It is not treated as an assignment of copyright or as
a prohibition, and **no legal conclusion is drawn** about whether or to whom rights subsist in AI-assisted output; the
maintainer carries that decision. Whether to say so in the public README or release notes is left to the maintainer (the
README currently states only the licence and the generic notice).

## Status of the flagged items (all closed 2026-11-01)

**Final decision (2026-11-01):** the maintainer approves `MIT OR Apache-2.0` for the project code they are entitled to license, accepts "OmaTree contributors" as the copyright attribution (ChatGPT and Claude assistance documented separately in `THIRD_PARTY.md`), confirms the logo and icons as their own, notes the OmaStore listing is removed, and states that third-party dependencies, fonts and assets keep their licences and notices. Final checks found no unresolved obligation; **the licensing gate is closed.**


1. ~~Logo and icon provenance~~ **Resolved 2026-10-31.** The maintainer confirmed that he designed and drew the logo entirely himself in Affinity (simple lines and dots on a blue background), that no AI-generated or third-party artwork was used, and authorized the logo and its exported icon variants under `MIT OR Apache-2.0`. Recorded in `THIRD_PARTY.md` and `docs/LICENSING_INVENTORY_0.2.0.md`; the files were verified against that description (flat-colour embedded raster, PNGs and ICO frames matching the SVG, packaged icons byte-identical) and their SHA-256 are now checked by `verify-sources.sh`.
2. ~~AI-assisted provenance and the generic holder "OmaTree contributors"~~ **accepted by the maintainer (2026-11-01)**; the notice
   is unchanged. R3a found the wording accurate for the history (several contributing identities); acceptance only. If it
   changes, `LICENSE-MIT`, Debian `copyright`, `OmaTree.iss` and `Info.plist.in` change together.
3. ~~Contributor identity and the outside contribution~~ **Resolved.** The maintainer states that "Martin Rudinec" is their own identity; "Finish Some" is the same GitHub account id as `Dorotabro`. The only outside contribution, `omastore.toml` (7 lines of Qt-package listing metadata, PR #1), was inspected (no obligations identified) and then **removed from the tree by the maintainer's decision (2026-10-31)**; the tree and the 0.2.0 source archive no longer include it. No contributor question remains. (The commit stays in Git history and in the earlier tags.)
4. ~~Fonts~~ **Resolved in R3a**: Liberation 2.1.5, byte-identical to Arch `ttf-liberation 2.1.5-2`, compared with the upstream archive
   (differences: one glyph in two files, cmap encoding, timestamp), unmodified by OmaTree, notices preserved (`THIRD_PARTY.md`). 16 crates'
   notices are upstream copies (three from branch heads, verified).
5. ~~Debian Apache text by reference~~ **Resolved in R3a**: `LICENSE-MIT` and the complete `LICENSE-APACHE` are installed in `/usr/share/doc/omatree-egui/` and `copyright` points at them; checked by `verify-deb.sh` and `verify-sources.sh`.

## Verification

- Candidate packages rebuilt with the updated notices (scratch copy at version 0.2.0, fresh caches, supported egui scripts
  only): `verify-package.sh` passes for all three (Arch 60, Debian 79, AppImage 71 `ok`; 0 failures, 0 skips). Arch and
  AppImage contents contain both licence texts (identical to the repository's) and the updated `THIRD_PARTY.md`; the `.deb` as
  described above. Candidate SHA-256 (not release artifacts): Arch `2e9be534b4d6e19396f85b14dd5bb0958b62d2dcfe0971fa7de5863ee2fc2058`, Debian `fc60af60cf12d84ba448e79fa450ddb17ce75aa77ac44941a92245fc5a1799ff`, AppImage `fbccf6cb0176a536bd99f523d8e6ade18a5dbfb600939f7109d364d264bdf99b`. The R2 isolated install/removal and launch tests
  were not repeated for this rebuild: only notice files changed.
- Working tree: `verify-sources.sh` all checks passed (including the new licensing section), `third-party.py check` current
  (300 crates), `cargo fmt --check` clean. No Rust source changed in R3, so clippy and the test suites were not re-run
  (R2's results stand).

## Readiness for the proposed 0.2.0 prerelease

- **Source licensing checks: ready.** The licence is decided and consistently declared and enforced by checks; the licence
  texts are intact and shipped in the source; `THIRD_PARTY.md` is current.
- **Package licensing checks: ready** for Arch and AppImage; ready for Debian by Debian convention.
- **Not fully closed:** items 2–5 are recorded and need the maintainer's explicit acceptance (or answers) for a prerelease; the logo/icon item was closed on 2026-10-31. Nothing is silently declared resolved.
