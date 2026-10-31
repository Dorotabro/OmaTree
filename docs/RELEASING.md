# Releasing OmaTree (egui)

The manual procedure for the first public Rust + egui prerelease and for releases after it. Nothing here is
automated: no script in this repository tags, signs, publishes or uploads, and there is no CI. Each step names
the existing script where one exists and says **manual** where none does. Do not invent a command that is not
listed here.

Read first: `docs/R1_RELEASE_AUDIT.md` (what is and is not ready), `docs/PACKAGING_EGUI.md` (what each
package verification covers), `docs/EGUI_MIGRATION_PLAN.md` (E13a cut-over checklist).

**Scope of the egui build scripts.** `packaging/egui/{arch,debian,appimage}/build-package.sh` build Linux x86_64
packages only. There is **no Windows or macOS packaging for the egui version**. `packaging/release-build.sh`,
`packaging/checksums.sh`, `packaging/release-checksums.sh`, `packaging/verify-package.sh` and the
`packaging/{arch,debian,appimage,macos,windows}` directories belong to the **Qt** version; they build and
checksum Qt files under the names `omatree-*`, and must not be used to build or list egui packages.

Version placeholders: `X.Y.Z` is the version in `Cargo.toml`, `vX.Y.Z` the tag. See the audit for the proposal.

## 0. Preconditions

- All **release blockers** in `docs/R1_RELEASE_AUDIT.md` are closed, and the maintainer has approved the version,
  the licence and copyright statement, and the artifact list.
- **Licensing (R3):** the repository is `MIT OR Apache-2.0` by the maintainer's decision of 2026-10-31
  (`THIRD_PARTY.md`, "Licensing decision and provenance"; `docs/EGUI_DECISIONS.md` decision 10). The items listed
  there under "Unresolved rights and attribution issues" (notably the **logo and icon provenance**, and the AI-assistance
  provenance) are either resolved by the maintainer in writing, with `THIRD_PARTY.md` updated, or accepted explicitly
  for a prerelease. Do not publish with them silently open.
- A Linux x86_64 machine with Rust (the pinned toolchain, see `docs/PACKAGING_EGUI.md` "Reproducibility"), `git`,
  `python3`, `makepkg`, Docker (or Podman) with network access, `bsdtar`, `readelf`, `objdump`, `xmllint`,
  `desktop-file-validate`, `appstreamcli`, and `gh` logged in.
- Windows and macOS: no egui packages exist. Do not list them in the release. Say so in the notes.

## 1. Final validation (on the release branch, clean tree)

```bash
git status --short                         # must print nothing
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
TMPDIR=$(mktemp -d) cargo test --locked --workspace --no-fail-fast
cargo build --locked --release --workspace
tools/pre-e9-compat.sh
packaging/egui/tests/verify-sources.sh
python3 packaging/egui/third-party.py check THIRD_PARTY.md
```

`verify-sources.sh` includes the licensing checks: every manifest declares `MIT OR Apache-2.0`; `LICENSE-MIT` and the
unmodified `LICENSE-APACHE` are present and tracked; AppStream, Debian `copyright` and the PKGBUILD agree with them and with
the notice "Copyright (c) 2026 OmaTree contributors"; no AI provider is named as holder; the embedded fonts match the
hashes in `THIRD_PARTY.md`.

All must pass. Known environment-specific failures must be listed in the release notes, not hidden. Today the
only one is `integrity_tests::memory_exhaustion_is_an_error_never_an_abort` on **macOS** (it uses `ulimit -v`);
it passes on Linux. The Qt tests need Qt 6 installed.

Then, from a **clean clone of the commit** (not the working tree) with an empty Cargo home and no Git credentials,
prove the source is public and self-contained:

```bash
D=$(mktemp -d)
git clone --branch <release-branch> https://github.com/Dorotabro/OmaTree.git "$D/src"
cd "$D/src"
CARGO_HOME="$D/cargo" GIT_CONFIG_GLOBAL=/dev/null env -u SSH_AUTH_SOCK \
    cargo build --locked --release -p omatree-egui
```

(Do this against the public repository, or an unauthenticated clone, once the maintainer has decided the
repository's visibility. This procedure does not change it.)

## 2. E13 cut-over prerequisites

The release must not be built until the cut-over decision is taken. The checklist is "E13 cut-over checklist"
in `docs/EGUI_MIGRATION_PLAN.md` (E13a record); the maintainer-facing summary is in the audit. In short, one
reviewable, revertible change that:

1. fixes the canonical application ID, binary name and package name, and renames the desktop, AppStream, MIME and
   icon files together (`verify-sources.sh` is the list of places);
2. makes `omatree-egui` the workspace default and removes Qt (`default-members`, `build.rs`, `cpp/`, `qml/`, the
   `cxx`/`cxx-qt` dependencies, the QML tests, Qt packaging, Qt sections of `THIRD_PARTY.md`), keeping `crates/core`;
3. adds `Provides/Conflicts/Replaces` (Arch, Debian) for the Qt package `omatree`;
4. updates `omastore.toml` (it names the Qt asset `omatree-{version}-*-x86_64.pkg.tar.zst` and `usr/bin/omatree`);
5. updates `AGENTS.md`, `ARCHITECTURE.md`, `README.md`, `docs/PACKAGING_EGUI.md`.

If the first egui prerelease ships **beside** Qt (no cut-over), skip 2 to 4 and publish under the `omatree-egui`
names exactly as the scripts build them. That is a maintainer decision; see the audit.

## 3. Version update

Change every location in the audit's "Version locations" table in one commit (manifests, `Cargo.lock`
workspace entries, AppStream release entry, Debian changelog; the Qt files are derived or deleted at the cut-over). Then:

```bash
cargo build --locked -p omatree-egui      # refreshes the version in Cargo.lock; commit the result
packaging/egui/tests/verify-sources.sh    # fails if the crate, AppStream and Debian versions disagree
```

Do not put a SemVer prerelease suffix (`-rc.1`) in `Cargo.toml`: the scripts copy the version verbatim into the
Arch `pkgver` (a hyphen is illegal there) and the Debian version. Mark the release a **prerelease on GitHub**
instead.

## 4. Merge into main

**Manual, reviewed by the maintainer.**

```bash
git switch egui-integration
git push origin egui-integration          # only if not yet pushed
gh pr create --base main --head egui-integration --title "Release vX.Y.Z" --body-file <notes>
# review, then merge with the merge strategy the maintainer chooses
git switch main && git pull --ff-only
```

Keep the Qt state recoverable: the tag `v0.1.2` and the branch `release-0.1.2` already exist and stay.

## 5. Tag

Annotated tag on the merge commit, **after** the validation in step 1 passed on exactly that commit.

```bash
git tag -a vX.Y.Z -m "OmaTree X.Y.Z (prerelease)"
git push origin vX.Y.Z
git rev-parse vX.Y.Z^{commit}              # record this hash in the release notes
```

## 6. Build every package from the same tag

Use the egui release script. It builds the three Linux packages from **one** revision in a temporary clean worktree,
checks the version everywhere, runs `verify-sources.sh` and `verify-package.sh`, and writes `PROVENANCE.txt` and the
checksums. It never tags, pushes, uploads or publishes, never modifies a tracked file and does not use the Qt scripts.

```bash
packaging/egui/release-build.sh --version 0.2.0 --ref v0.2.0      # final build from the tag
packaging/egui/release-build.sh --version 0.2.0                   # candidate from a clean HEAD of egui-integration or main
packaging/egui/release-build.sh --version 0.2.0 --ref <commit>    # candidate from any commit
```

Output goes to `dist/egui-release-0.2.0/` (`--out` changes it): the three packages, `PROVENANCE.txt`, `logs/`, and
`SHA256SUMS` (only for a build from the tag `v0.2.0`; every other build writes `SHA256SUMS.candidate`). It refuses: a HEAD build
on another branch or with a dirty tree; a lightweight `v*` tag; a version that is not plain `X.Y.Z`; manifests, `Cargo.lock`,
AppStream or the Debian changelog that do not all say the requested version. A Docker daemon, `makepkg`, `bsdtar`
and network access to crates.io are needed. Use a fresh `OMATREE_CACHE` and `CARGO_HOME` to prove no stale cache supplies anything.

The three artifacts are traceable to one commit through `PROVENANCE.txt` (commit, tree, toolchain, images, hashes).
No Windows or macOS package exists; do not build the Qt scripts and publish their output as the egui release.

## 7. Package inspection

For each file:

```bash
packaging/egui/verify-package.sh dist/omatree-egui-X.Y.Z-1-x86_64.pkg.tar.zst
packaging/egui/verify-package.sh dist/omatree-egui_X.Y.Z_amd64.deb
packaging/egui/verify-package.sh dist/OmaTree-egui-X.Y.Z-x86_64.AppImage
```

Then read the listings once by eye: no `/home/...` path, no `.rs` file, no fixture notebook, `THIRD_PARTY.md`,
`third-party-licenses.txt`, `LICENSE-MIT` and `LICENSE-APACHE` present, version string equal to `X.Y.Z`.

```bash
packaging/egui/debian/test-install.sh dist/omatree-egui_X.Y.Z_amd64.deb debian:13
packaging/egui/debian/test-install.sh dist/omatree-egui_X.Y.Z_amd64.deb ubuntu:24.04
packaging/egui/appimage/test-container.sh dist/OmaTree-egui-X.Y.Z-x86_64.AppImage ubuntu:20.04
packaging/egui/appimage/test-container.sh dist/OmaTree-egui-X.Y.Z-x86_64.AppImage debian:12
```

## 8. Smoke tests

On a machine or VM that is **not** the development machine, with a throw-away notebook (never a real one), an
isolated `HOME` if testing on the same account (`HOME=$(mktemp -d)`), install each package the normal way
(`sudo pacman -U`, `sudo apt install ./…deb`, `chmod +x` and run the AppImage) and check:

1. starts; window title, icon, launcher entry `OmaTree`;
2. new note, child note, rename, drag, Colour tag from header and tree menu;
3. live Markdown, a table, a pasted clipboard image;
4. Save As (native dialog), close, reopen, content and image intact;
5. search, delete, Recovery restore;
6. export to Markdown, ZIP and PDF; open the PDF;
7. Open from the file manager (double-click a `.omatree`);
8. switch the Omarchy theme live (Omarchy only);
9. remove the package; no package-owned file remains; notebooks untouched.

Record pass, fail or **not tested** for each, per platform. Anything not run is **not tested**: never infer a pass.

## 9. SHA-256 checksums

The release script writes `SHA256SUMS` for a build from the tag. The checksum scripts in `packaging/` (not `packaging/egui/`) name Qt files and do not apply.
Final checksums must describe the files that are **attached to the release**: recompute them from those files and compare.

```bash
cd dist
sha256sum omatree-egui-X.Y.Z-1-x86_64.pkg.tar.zst \
          omatree-egui_X.Y.Z_amd64.deb \
          OmaTree-egui-X.Y.Z-x86_64.AppImage > SHA256SUMS
sha256sum --check SHA256SUMS
```

`SHA256SUMS` lists exactly the uploaded packages, nothing else (not the source tarball, not `lintian.txt`).
Nothing is signed.

## 10. GitHub draft release

```bash
gh release create vX.Y.Z --draft --prerelease --verify-tag \
    --title "OmaTree X.Y.Z (prerelease)" --notes-file RELEASE_NOTES.md
```

The release is created as a **draft** and a **prerelease**. Do not publish yet.

## 11. Upload packages

```bash
gh release upload vX.Y.Z \
    dist/omatree-egui-X.Y.Z-1-x86_64.pkg.tar.zst \
    dist/omatree-egui_X.Y.Z_amd64.deb \
    dist/OmaTree-egui-X.Y.Z-x86_64.AppImage \
    dist/SHA256SUMS
gh release view vX.Y.Z --json assets --jq '.assets[].name'
```

Download the uploaded files into a new empty directory and check them against `SHA256SUMS` before publishing.

## 12. Release notes

`RELEASE_NOTES.md` states, truthfully:

- what the release is (first Rust + egui prerelease) and the exact tag and commit;
- the file list, and that **no Windows or macOS package is provided**;
- the notebook compatibility warning: notebooks written by this version may be schema 4 or 5 and are refused by
  Qt v0.1.2 and earlier; keep copies;
- known limitations (audit "Acceptable documented limitations"), unverified platforms, and known test failures;
- the licence (`MIT OR Apache-2.0`, notice "OmaTree contributors") and where the third-party notices are installed
  (`/usr/share/licenses/omatree-egui/` on Arch and in the AppImage, `/usr/share/doc/omatree-egui/` on Debian);
- how to verify checksums;
- that the Qt version remains available at `v0.1.2`.

## 13. Publish the prerelease

Only after steps 7 to 11 pass and the maintainer approves:

```bash
gh release edit vX.Y.Z --draft=false --prerelease
```

Keep `--prerelease` set. Do not mark it "latest".

## 14. Post-release verification

In a new empty directory, as a user would:

```bash
gh release download vX.Y.Z --pattern '*'
sha256sum --check SHA256SUMS
```

Install one package on a clean system, start it, open and save a throw-away notebook. Check the release page
(assets, prerelease flag, notes), the README at the tag (it must not describe unreleased things as available),
and that `omastore.toml` (if kept) points at assets that exist. Open an issue for each defect found.

## 15. Rollback and corrective release

Published assets and tags may have been downloaded or cached. Prefer correcting forward.

1. **Stop new downloads:** `gh release edit vX.Y.Z --draft=true` (or delete only the faulty asset with
   `gh release delete-asset vX.Y.Z <name>`), and say in the release notes what was wrong.
2. **Do not move or delete a published tag.** Fix the problem on a branch and publish **vX.Y.(Z+1)** from steps 1 to 14.
3. A notebook-corrupting or data-loss defect: unpublish immediately (step 1), warn in the notes and the README, then
   release the corrected version. Users keep their notebooks as ordinary files; the pre-release compatibility
   note tells them to keep copies of notebooks they still need in an older version.
4. A bad **merge** into `main` (before or after the tag): the cut-over is one revertible change, so
   `git revert -m 1 <merge-commit>` on `main` restores the Qt tree; the tag `v0.1.2` and branch `release-0.1.2`
   are untouched.
5. Record what happened in `docs/EGUI_MIGRATION_PLAN.md` or the issue tracker.

Never force-push `main` or a tag.
