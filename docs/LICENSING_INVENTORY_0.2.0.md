# Licensing inventory for the egui 0.2.0 prerelease

A factual list for the maintainer. It records what is in the source tree and in the Linux packages, which notice covers
it, and what is missing or ambiguous. It makes **no licensing decision and asserts no ownership**; the maintainer's decision
that the repository continues under `MIT OR Apache-2.0` (2026-10-31) is recorded in `THIRD_PARTY.md` and
`docs/EGUI_DECISIONS.md` (decision 10). **Public release status: BLOCKED** until the items in section 7 are answered.
Counts are from `git ls-files` at the R3 working tree.

## 1. Project-owned source (declared `MIT OR Apache-2.0`)

| Part | Files | Lines | In the Linux packages? | Notes |
|---|---|---|---|---|
| `crates/core` (`omatree-core`) | 14 `.rs` | 15,456 | compiled in | manifest `MIT OR Apache-2.0` |
| `crates/egui-app` (`omatree-egui`) | 46 `.rs` | 24,283 | compiled in | manifest `MIT OR Apache-2.0` |
| `crates/editor` (`omatree-editor`) | 22 `.rs` | 16,645 | compiled in | see section 2 |
| Qt application (`src/`, `build.rs`, `qml/`, `tests/qml/`, `cpp/`) | 7 + 39 + 6 | 2,645 + 5,123 + 677 | **no** (legacy, Qt packages only) | root manifest `MIT OR Apache-2.0` |
| Packaging scripts and metadata (`packaging/`, `tools/`) | 107 + 1 | 8,652 + 52 | scripts: no; metadata: yes | |
| Documentation (`docs/`, root `.md`) | 8 + root files | 3,646 + … | README and `THIRD_PARTY.md` shipped | |

Authorship in `git log`: `Dorotabro` (65 commits, plus 3 as "Finish Some" with the same account email), "Martin Rudinec"
(8, two local machine names), "Arthur Augusto" (1, `omastore.toml`, 7 lines, PR #1). No contributor agreement; `CONTRIBUTING.md`
says contributions are accepted under the project licence. The maintainer states that the application was developed primarily
with AI coding assistance (provenance only; see `THIRD_PARTY.md`).

## 2. Editor source included in the workspace

`crates/editor` is a snapshot, without Git history, of revision `30a95dd43064232dc1ac04315c8020e2307a223c` of the former
private prototype repository (ticket P2), plus later edits made in this repository (`git log -- crates/editor`: 4 commits by
two author names; first commit 2026-10-28). **That revision carried no licence file.** The crate declares `MIT OR Apache-2.0` on
the maintainer's decision; its 13-commit prototype history had a single author account (the one owning this repository);
the imported source contains no copyright, "adapted from" or SPDX marker. Ownership facts are the maintainer's to confirm
(section 7).

## 3. Bundled fonts and assets

| Asset | In packages | Licence / notice | Status |
|---|---|---|---|
| Liberation Sans ×4, Liberation Mono ×1 (`crates/editor/assets/fonts`, embedded for PDF export), metadata version 2.1.x | yes (in the binary) | SIL OFL-1.1, "Digitized data copyright (c) 2010 Google Corporation; Copyright (c) 2012 Red Hat, Inc.", Reserved Font Name "Liberation"; `LICENSE-Liberation.txt` + `third-party-licenses.txt` | notice present; SHA-256 recorded and checked; **not compared with an upstream release** |
| egui default fonts via `epaint_default_fonts` (Ubuntu-Light, Hack, Noto Emoji, egui and emoji icon fonts) | yes (in the binary) | OFL-1.1, Ubuntu Font Licence 1.0, Hack/MIT texts in `third-party-licenses.txt` | covered |
| Logo `assets/logo/omatree-logo.svg` (embeds a raster image), PNG icons 32–1024, `windows/omatree.ico` | PNG 32–512 and the SVG installed as icons (Linux packages); `.ico` only in the Qt Windows installer | none stated | **origin, author, tool and terms undocumented** (commit `8ea3bfa` by the maintainer's account, 2026-10-04) |
| `docs/screenshots/*.png` | no | none stated | maintainer's captures of OmaTree (show the Kanagawa and a light palette and Noto Emoji glyphs) |
| SQLite (via `libsqlite3-sys` bundled) | yes (in the binary) | public domain | no notice required |

## 4. Third-party dependencies of the egui binary

300 compiled crates, listed with versions and licence expressions in the generated table of `THIRD_PARTY.md` (kept current
by `packaging/egui/third-party.py check`). Licence expressions are all permissive; the only expression naming a copyleft licence is
`Apache-2.0 OR GPL-2.0-only`, taken under Apache-2.0. Run-time system libraries (Wayland, X11, EGL/GL, xkbcommon, fontconfig,
libdbus) are opened from the host and are not distributed. No Qt component is in the egui packages.

## 5. Required notices and where they are

| Package | MIT + Apache-2.0 texts | Third-party notices |
|---|---|---|
| Arch | `usr/share/licenses/omatree-egui/LICENSE-MIT`, `LICENSE-APACHE` | same directory: `THIRD_PARTY.md`, `third-party-licenses.txt` (every crate's licence and notice files, each distinct text once, with the crates it covers; fonts' OFL/UFL included) |
| AppImage | same paths inside the image | same |
| Debian | `usr/share/doc/omatree-egui/copyright` (DEP-5): MIT text; Apache-2.0 by reference to `/usr/share/common-licenses/Apache-2.0` | `usr/share/doc/omatree-egui/THIRD_PARTY.md.gz`, `third-party-licenses.txt.gz` |
| Source archives (git archive / tag) | `LICENSE-MIT`, `LICENSE-APACHE` at the root (tracked) | `THIRD_PARTY.md`, `packaging/egui/licences/`, fonts' `LICENSE-Liberation.txt` |

`verify-package.sh` checks these in each package; `verify-sources.sh` checks the declarations and the licence texts.

## 6. The upstream notice copies (R2 flagged three; sixteen crates in all)

Sixteen crates declare a licence but ship no licence file; upstream copies are in `packaging/egui/licences/`.
For the three taken from branch heads, the exact locked version and the commit recorded in each published crate's
`.cargo_vcs_info.json` were used to fetch the authoritative text and compare it:

| Crate | Version | Commit the crate was published from | Result |
|---|---|---|---|
| krilla | 0.8.2 | `3ffdf0588cf98050aad6edba51ca70162e1fb5b5` | MIT and Apache texts identical to the copies |
| profiling | 1.0.18 | `8271551172eb6fa4cba47369aedd93790c623df9` | MIT and Apache texts identical |
| gl_generator / khronos_api | 0.14.0 / 3.1.0 | `ea503e8d…` / `f150967b…` | `LICENSE` (Apache-2.0) identical |

Nothing was replaced. egui and accesskit copies were taken from release tags in E12c. The copies are still copies (the crates
do not carry the files), so their copyright lines are those of the upstream repositories at those commits.

## 7. Missing or ambiguous permissions (maintainer's, open)

1. **Logo and icons**: author, tool and terms unrecorded; shipped in every package. Needs a statement that they may be
   distributed under `MIT OR Apache-2.0`, or another stated licence.
2. **Editor**: copyright holder wording ("OmaTree contributors") and distribution rights for code from a repository that had
   no licence file and was developed primarily with AI assistance; no legal conclusion is drawn here.
3. **Contributors**: confirm the "Martin Rudinec" commits are the maintainer's; one outside contribution rests on the
   `CONTRIBUTING.md` statement only.
4. Liberation fonts: unmodified status and exact release unverified.
5. Debian ships the Apache text by reference only (Debian convention).

Legal metadata (`LICENSE-*`, manifests, copyright lines, notices) was **not changed** in R3.
