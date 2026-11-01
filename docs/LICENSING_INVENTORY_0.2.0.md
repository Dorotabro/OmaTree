# Licensing inventory for the egui 0.2.0 prerelease

A factual list for the maintainer. It records what is in the source tree and in the Linux packages, which notice covers
it, and what is missing or ambiguous. It makes **no licensing decision and asserts no ownership**; the maintainer's decision
that the repository continues under `MIT OR Apache-2.0` (2026-10-31) is recorded in `THIRD_PARTY.md` and
`docs/EGUI_DECISIONS.md` (decision 10). **Licensing gate: CLOSED on 2026-11-01** (final maintainer decision, section 7); publication still needs a tag, final artifacts and the maintainer's authorization. Earlier steps: (resolved: the logo on 2026-10-31; the fonts and the Debian licence text in R3a; acceptance only: item 4 of section 7).
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

Authorship in `git log --all` (78 commits): `Dorotabro` (66, plus 3 as "Finish Some" with the same account email and GitHub id),
"Martin Rudinec" (8, two local machine names), "Arthur Augusto" (1, `omastore.toml`, 7 lines, PR #1; file removed from the tree 2026-10-31); line counts per identity are in
`THIRD_PARTY.md`. No contributor agreement; `CONTRIBUTING.md`
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
| Liberation Sans ×4, Liberation Mono ×1 (`crates/editor/assets/fonts`, embedded for PDF export), metadata version 2.1.x | yes (in the binary) | SIL OFL-1.1, "Digitized data copyright (c) 2010 Google Corporation; Copyright (c) 2012 Red Hat, Inc.", Reserved Font Name "Liberation"; `LICENSE-Liberation.txt` + `third-party-licenses.txt` | **resolved (R3a)**: notice present; SHA-256 recorded and checked; version 2.1.5, byte-identical to Arch `ttf-liberation 2.1.5-2`; compared with the upstream 2.1.5 archive (outlines, metrics, names, licence text identical except one glyph, U+25D5, in two files, and cmap encoding/timestamp); unmodified by OmaTree; see `THIRD_PARTY.md` |
| egui default fonts via `epaint_default_fonts` (Ubuntu-Light, Hack, Noto Emoji, egui and emoji icon fonts) | yes (in the binary) | OFL-1.1, Ubuntu Font Licence 1.0, Hack/MIT texts in `third-party-licenses.txt` | covered |
| Logo `assets/logo/omatree-logo.svg` (embeds a flat-colour raster), PNG icons 32–1024, `windows/omatree.ico` | PNG 32–512 and the SVG installed as icons (Linux packages); `.ico` only in the Qt Windows installer | **maintainer's original artwork**, `MIT OR Apache-2.0` by his statement of 2026-10-31 (below) | **resolved**: provenance and permission recorded; files verified against the description; SHA-256 recorded and checked |
| `docs/screenshots/*.png` | no | none stated | maintainer's captures of OmaTree (show the Kanagawa and a light palette and Noto Emoji glyphs) |
| SQLite (via `libsqlite3-sys` bundled) | yes (in the binary) | public domain | no notice required |

### Logo and icons: provenance and permission (recorded 2026-10-31)

The maintainer confirms that the original OmaTree logo was **designed and drawn entirely by him in Affinity**; that the artwork
is simple lines and dots on a blue background; that **no AI-generated graphics and no third-party artwork** were used; and that he
**authorizes distribution of the original logo and its exported icon variants under `MIT OR Apache-2.0`**, alongside OmaTree.

Verification of the files against that description (R3, 2026-10-31):

| Check | Result |
|---|---|
| `omatree-logo.svg` contents | 5 circles, 5 paths, 3 rectangles, one `<image>`; the embedded raster is a 2000 × 2000 PNG with **one colour** (`#283845`, the blue background); no other bitmap or text; exported from a vector editor (the `serif` namespace is Affinity's) |
| Rendered artwork | white lines branching into dots on the dark blue square: a tree (checked visually, SVG render and 1024 px PNG side by side) |
| PNG icons 32, 64, 128, 256, 512, 1024 | each matches the SVG rendered at the same size: normalised RMSE 0.0017 (1024), 0.0032 (512), 0.0029 (256), 0.0012 (128), 0.0054 (64), 0.0138 (32): anti-aliasing differences only |
| `windows/omatree.ico` (7 frames) | 32, 64, 128 and 256 px frames are **pixel-identical** to the shipped PNGs; 16, 24 and 48 px frames show the same motif (they exist only in the ICO; 2-4 % from a plain SVG rendering, i.e. small-size resampling) |
| Packaged icons | the Arch package, the `.deb` and the AppImage candidates contain the 32-512 px PNGs and the SVG **byte-identical** to the repository files; the egui window embeds `omatree-icon-256.png`; the `.ico` is used by the Qt Windows installer only |
| Integrity going forward | `verify-sources.sh` checks the SHA-256 of the SVG, the six PNGs and the ICO against `THIRD_PARTY.md`; a changed or new image fails the check until the table and the maintainer's statement are updated |

The files carry no author metadata; this is the maintainer's statement, not an independent finding about authorship, and no legal
conclusion is drawn. The raster inside the SVG, the colour (`#283845`) and the shapes are therefore covered by the same statement.

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
| Debian | `usr/share/doc/omatree-egui/copyright` (DEP-5): MIT text; Apache-2.0 by reference to `/usr/share/common-licenses/Apache-2.0` (lintian-required) **and** to the complete text shipped in the package: `LICENSE-APACHE` and `LICENSE-MIT` in the same directory (R3a) | `usr/share/doc/omatree-egui/THIRD_PARTY.md.gz`, `third-party-licenses.txt.gz` |
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

## 7. Missing or ambiguous permissions: all closed (2026-11-01)

Final maintainer decision (2026-11-01): `MIT OR Apache-2.0` approved for the project code the maintainer is entitled to license; "OmaTree contributors" accepted as the copyright attribution, with ChatGPT and Claude assistance documented separately; logo and icons are the maintainer's own artwork; the OmaStore listing is removed; third-party dependencies, fonts and assets keep their own licences and notices. Final checks found no unresolved obligation. See `THIRD_PARTY.md`.

R3a (2026-10-31) audited the contributor history and the fonts and fixed the Debian licence text. Details and the
contributor table are in `THIRD_PARTY.md` ("Rights and attribution issues: status", items 1 and 3).

1. ~~Logo and icons~~ **resolved 2026-10-31** (section 3).
2. ~~Liberation fonts~~ **resolved (R3a)**: origin, version, licence and notices verified; byte comparison with the upstream archive
   done (not identical: distribution rebuild, one glyph differs); OmaTree never modified them.
3. ~~Debian ships the Apache text by reference~~ **resolved (R3a)**: the complete text is installed as `/usr/share/doc/omatree-egui/LICENSE-APACHE` (with `LICENSE-MIT`); `copyright` points at it and keeps the common-licenses reference that lintian requires (lintian rejects an embedded copy); `verify-deb.sh` and `verify-sources.sh` check this.
4. ~~Editor and generic holder~~ **accepted by the maintainer on 2026-11-01** (`MIT OR Apache-2.0`, "OmaTree contributors", AI assistance from ChatGPT and Claude documented separately). The prototype's 13 commits are all by the repository owner's account; no
   third-party code was found; no licence file existed at `30a95dd`. Rights in AI-assisted code: no legal conclusion is drawn.
   The wording "OmaTree contributors" is accurate for a history with several contributing identities.
5. ~~Identity "Martin Rudinec"~~ **resolved by the maintainer's statement (2026-10-31)**: the 8 commits (about 6,700 lines at HEAD) are
   their own, made from two machines; there is exactly one other human contributor. Recorded as a statement, not inferred from the name.
   "Finish Some" (3 commits) shares the GitHub account id of `Dorotabro`.
6. ~~The outside contribution `omastore.toml`~~ (Arthur Augusto, PR #1, 7 lines of Qt-package listing metadata) **no longer included**: the
   maintainer removed the file on 2026-10-31. It was inspected first (one commit, one file, no copied code, trailers or notice
   requirement) and no obligations were identified. The only outside contribution to the tree is gone; the commit remains in Git
   history and in earlier tags. No contributor question remains.

Legal metadata (`LICENSE-*`, manifests, copyright lines, notices) is unchanged by R3a; only the Debian `copyright` file gained a pointer to the shipped
Apache text and a font paragraph, and the Debian `rules` installs the two licence files.
