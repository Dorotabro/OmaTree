# Ticket 029 — Linux / Omarchy packaging and desktop integration

## Goal

Turn the current release-ready OmaTree source tree into a properly integrated
Linux desktop application, with Omarchy / Arch Linux as the first binary
distribution target.

This ticket must produce:

- stable application identity
- application/window icon integration
- .desktop launcher
- .omatree MIME association
- AppStream metadata
- standard Linux install layout
- an installable Arch/Omarchy pacman package
- release-oriented packaging verification
- packaging documentation

Do not publish anything.

Do not create a GitHub release.

Do not tag v0.1.0.

The public-release pass is Ticket 030.

---

# Scope

Primary target:

    Omarchy / Arch Linux x86_64

Secondary goal:

    keep Linux metadata/install layout standards-compliant and reusable by
    future packaging formats

Out of scope for this ticket:

- AUR publication
- Flatpak
- AppImage
- Snap
- Debian packages
- RPM packages
- macOS packaging
- Windows packaging
- Microsoft Store
- auto-update
- installer GUI

Do not let cross-platform packaging inflate this ticket.

---

# Application identity

Adopt the following canonical identity:

    Display name:    OmaTree
    Executable:      omatree
    Version:         0.1.0
    Application ID:  io.github.Dorotabro.OmaTree
    File extension:  .omatree
    MIME type:       application/x-omatree
    Icon name:       io.github.Dorotabro.OmaTree

The QML module URI `org.omatree` is internal implementation detail and does not
need to match the desktop application ID.

Document this distinction where useful.

---

# Existing logo assets

Use the committed assets under:

    assets/logo/

Expected files:

    omatree-logo.svg
    omatree-icon-32.png
    omatree-icon-64.png
    omatree-icon-128.png
    omatree-icon-256.png
    omatree-icon-512.png
    omatree-icon-1024.png

Do not redesign, recolour or regenerate the logo.

Validate that:

- SVG is self-contained
- SVG has no external file references
- installed raster files have correct dimensions
- no font dependency is required to render the icon

The SVG is the scalable master.

---

# Part 1 — Qt application identity

Set the Qt application metadata early during startup.

At minimum:

    application name/display name = OmaTree

On Linux set the desktop-file identity to:

    io.github.Dorotabro.OmaTree

Use the proper Qt API rather than depending on window-title heuristics.

The purpose is correct desktop-shell association, especially under Wayland.

Do not change:

- executable name
- QML URI
- notebook file format
- database schema

## Window/application icon

Make the application icon available to Qt from application resources.

The running OmaTree window should have the OmaTree icon when launched directly,
not only when launched from the desktop file.

Use the existing Qt/QML resource mechanism.

Do not load the icon from an absolute filesystem development path.

The packaged binary must remain relocatable with respect to its compiled
resources.

---

# Part 2 — Desktop entry

Add:

    packaging/linux/io.github.Dorotabro.OmaTree.desktop

It should describe a normal graphical desktop application.

Required concepts:

    Type=Application
    Name=OmaTree
    Exec=omatree %f
    Icon=io.github.Dorotabro.OmaTree
    Terminal=false
    MimeType=application/x-omatree;

Use suitable standard Categories, preferably a restrained combination such as:

    Office;Utility;

Add a concise Comment describing OmaTree.

Suggested wording:

    A local-first notebook for people who think in trees

or, if better suited to desktop metadata:

    A simple tree-structured note-taking application

Do not invent marketing paragraphs in the desktop file.

Use `%f`, not `%F`.

OmaTree opens one notebook per process.

Do not add a shell command or quote the Exec value incorrectly.

Validate with `desktop-file-validate` when available.

---

# Part 3 — MIME type

Add Shared MIME Info metadata:

    packaging/linux/io.github.Dorotabro.OmaTree.xml

Register:

    application/x-omatree

with:

    *.omatree

Description:

    OmaTree notebook

Use extension/glob identification.

Do NOT add SQLite magic matching, because an OmaTree file is SQLite internally
and generic SQLite databases must not be claimed as OmaTree notebooks.

Do not change the database format merely to create MIME magic.

Verify that the generated MIME database recognises the glob when the necessary
tools are available.

---

# Part 4 — AppStream metadata

Add:

    packaging/linux/io.github.Dorotabro.OmaTree.metainfo.xml

Use:

    component ID: io.github.Dorotabro.OmaTree
    name: OmaTree
    project license: MIT OR Apache-2.0

Describe OmaTree accurately and briefly.

The description should reflect the actual product:

- local-first
- tree-structured notes
- Markdown edit/preview
- local .omatree notebook files
- no account/cloud requirement

Do not promise:

- sync
- collaboration
- mobile support
- plugins
- backlinks
- graph views

Add appropriate categories/keywords only if valid AppStream fields support
them.

Repository/homepage URLs may point to:

    https://github.com/Dorotabro/OmaTree

Do not fabricate a release URL that does not yet exist.

Ticket 030 may add final v0.1 release metadata/date.

Validate with `appstreamcli validate` when available.

---

# Part 5 — Linux install layout

Create one reusable staging mechanism for Linux packaging rather than copying
install rules into multiple package formats.

For example:

    packaging/linux/stage.sh

Exact filename may differ if a clearer design fits the repository.

It must support a DESTDIR-style staging root.

The standard system-prefix layout should be:

    /usr/bin/omatree

    /usr/share/applications/
        io.github.Dorotabro.OmaTree.desktop

    /usr/share/mime/packages/
        io.github.Dorotabro.OmaTree.xml

    /usr/share/metainfo/
        io.github.Dorotabro.OmaTree.metainfo.xml

    /usr/share/icons/hicolor/scalable/apps/
        io.github.Dorotabro.OmaTree.svg

    /usr/share/icons/hicolor/32x32/apps/
        io.github.Dorotabro.OmaTree.png

    /usr/share/icons/hicolor/64x64/apps/
        io.github.Dorotabro.OmaTree.png

    /usr/share/icons/hicolor/128x128/apps/
        io.github.Dorotabro.OmaTree.png

    /usr/share/icons/hicolor/256x256/apps/
        io.github.Dorotabro.OmaTree.png

    /usr/share/icons/hicolor/512x512/apps/
        io.github.Dorotabro.OmaTree.png

1024 may also be installed if appropriate, but do not invent nonexistent
hicolor conventions merely to use every source image.

Install project licensing/documentation appropriately, including:

    LICENSE-MIT
    LICENSE-APACHE
    THIRD_PARTY.md

Use normal Arch/Linux conventions for license/document locations.

The staging operation must:

- fail on missing required files
- preserve executable permissions
- not mutate the host's /usr
- not update host MIME/icon caches itself

Package-manager/system hooks can handle caches after a real installation.

---

# Part 6 — Qt LGPL packaging strategy

For the Arch/Omarchy package:

DO NOT bundle Qt libraries.

Link dynamically against the system Qt installation.

Use native package dependencies for the required Qt modules.

Derive the exact dependencies from:

- actual linked libraries
- QML imports
- current runtime requirements

Likely dependencies include Qt 6 Base and Declarative, and Omarchy's Wayland
environment may require the Qt Wayland platform plugin.

Do not blindly copy this guess into the package without verifying it.

The installed pacman package must contain no copied Qt .so files.

This keeps the initial distribution model simple and consistent with the
Ticket 022 LGPL audit.

Update `docs/PACKAGING.md` to explain that the native Arch package relies on
system Qt and therefore does not redistribute Qt binaries.

---

# Part 7 — Cargo/release metadata

Update Cargo metadata where appropriate for public release.

At minimum consider:

    description
    repository
    publish = false

Recommended description:

    A local-first notebook for people who think in trees.

Repository:

    https://github.com/Dorotabro/OmaTree

Prevent accidental `cargo publish` if OmaTree is not intended to be a crates.io
package.

Do not change crate/package naming unnecessarily.

Do not update dependency versions.

---

# Part 8 — Build-path leakage

Ticket 027 found that a normal release binary can contain absolute paths from
the build machine.

The distributable package build must avoid exposing the user's home/repository
path where reasonably practical.

Use Rust path remapping during the package build.

Cover at least:

- project source/build path
- packaging Cargo home/dependency source path where practical

Use `--remap-path-prefix` or the appropriate Rust packaging mechanism.

Do not globally make developer builds awkward merely to solve this.

The requirement applies to the distributable packaging build.

After building the package, inspect the release binary with `strings` or an
equivalent local tool.

It must not contain:

    /home/<developer>/

or the absolute OmaTree repository path.

Dependency/library names and virtual remapped paths are fine.

---

# Part 9 — Arch / Omarchy package

Produce an installable pacman package:

    omatree-0.1.0-1-x86_64.pkg.tar.zst

Use standard Arch packaging mechanisms.

Keep the package recipe/build tooling in:

    packaging/arch/

The exact implementation may use:

- a PKGBUILD template
- a small package-build script
- a temporary source archive

Prefer simplicity and reproducibility over cleverness.

The repository should retain enough packaging source that the package can be
rebuilt for future releases without reconstructing the process from memory.

## Package metadata

Use:

    pkgname=omatree
    pkgver=0.1.0
    pkgrel=1
    arch=x86_64

License metadata must reflect:

    MIT
    Apache-2.0

Use the verified runtime dependencies from Part 6.

Use `cargo build --release --locked`.

Do not run Cargo against an unlocked dependency graph.

Do not package `/target` or development files.

## Package contents

The package must contain only the intended installed application files.

No:

- source checkout
- Cargo registry
- build directory
- tests
- screenshots
- temporary databases
- `.git`
- Qt shared libraries
- compiler artefacts

Use the shared Linux staging mechanism rather than maintaining a second
independent install manifest if practical.

---

# Part 10 — Development vs release package builds

It must be possible to validate packaging before Ticket 030 creates a public
tag.

If the final public-source checksum/tag is not yet available, use a local
temporary source snapshot for Ticket 029 validation.

Do not weaken the eventual release integrity just to make local testing easy.

If a template requires a final release source checksum or tag, document that
clearly for Ticket 030.

For validation, a throwaway copy under `/tmp` is acceptable.

Do not alter Git history merely to test packaging.

---

# Part 11 — Dist output

Package-build tooling may write generated release artefacts beneath:

    dist/

At minimum produce during validation:

    dist/omatree-0.1.0-1-x86_64.pkg.tar.zst

If a source archive is generated as part of the clean package process, keep it
under `dist/` as well.

Generate SHA-256 checksums for distributable artefacts where practical.

Do not commit generated package binaries.

Ensure `dist/` cannot accidentally be committed.

Adding `/dist/` to `.gitignore` is appropriate as part of this ticket.

Do not perform the broader `.gitignore` cleanup from Ticket 027 yet unless
directly required by packaging.

---

# Part 12 — Package verification

Validate the generated package without permanently installing it on the
developer machine.

Inspect package contents.

Use appropriate available tools, for example:

    pacman -Qlp
    bsdtar
    namcap

`namcap` is optional if not installed.

Do not install new tooling merely for this ticket.

Check:

- expected binary path
- desktop file
- MIME XML
- AppStream file
- icon files
- license files
- no unexpected build files

Extract the package to a temporary directory and inspect the packaged binary.

Run:

    ldd

or equivalent and verify:

- Qt is dynamically linked from the system
- SQLite behaviour matches the current bundled-rusqlite architecture
- there are no bundled Qt libraries inside the package

Run the staged/packaged executable with the Qt offscreen platform and verify a
clean launch.

No QML warnings or binding loops.

---

# Part 13 — Desktop integration validation

Where tools are available:

- validate desktop entry
- validate AppStream metadata
- parse MIME XML
- build a temporary MIME database
- verify `.omatree` appears in its glob table
- verify all installed icon files decode correctly

On the actual Omarchy desktop, perform a non-destructive integration smoke test
if practical using temporary XDG/PATH overrides.

Do not permanently copy files into `/usr` as part of automated validation.

At minimum confirm:

- packaged app icon appears correctly
- desktop-file application identity matches the running Qt application
- launcher starts OmaTree
- `%f` passes one notebook path correctly

If this cannot be tested without installing the package, report the exact
manual test required after installation rather than mutating the system.

---

# Part 14 — README / packaging documentation

Update README installation information carefully.

For v0.1 the supported binary package target is:

    Omarchy / Arch Linux x86_64

Document source build separately.

Do not imply that macOS or Windows packages exist yet.

Document the planned package install command once the generated filename is
known, for example:

    sudo pacman -U ./omatree-0.1.0-1-x86_64.pkg.tar.zst

Document removal:

    sudo pacman -R omatree

Mention that `.omatree` files become associated with OmaTree after normal
desktop/MIME database processing.

Update `docs/PACKAGING.md` with:

- application identity
- install paths
- Qt system-library strategy
- package-build process
- validation process
- future packaging targets

---

# Part 15 — Existing verification suite

Run the complete existing baseline:

    cargo fmt --check
    cargo clippy --all-targets
    cargo test
    cargo test integration_tests
    cargo build
    cargo build --release

Expected baseline before packaging work:

    288 Rust tests
    1 ignored benchmark
    19 integration scenarios

Do not weaken tests merely to make packaging succeed.

---

# Part 16 — Package-specific checks

Also run the new packaging verification.

Report:

- package filename
- package compressed size
- installed size
- package file list summary
- exact dependencies
- binary linked Qt modules
- whether Qt libraries were bundled
- whether absolute build paths remain
- metadata validation results
- MIME validation result
- icon validation result
- offscreen packaged-binary result

---

# Manual Omarchy installation test

Do NOT automatically install the package with sudo.

At completion, provide the user with the exact command for manually installing
the generated package.

After the user installs it, the intended manual checklist is:

1. launch OmaTree from the desktop/application launcher
2. confirm correct icon
3. create and save a `.omatree` notebook
4. close OmaTree
5. double-click the `.omatree` file
6. confirm OmaTree opens it
7. verify launcher/task-switcher grouping/icon
8. uninstall/reinstall if desired with pacman

The user decides whether to perform this system-changing test.

---

# Out of scope

Do not:

- create GitHub release
- make repository public
- push tags
- publish AUR package
- add Flatpak
- add AppImage
- add Snap
- add Debian/RPM packages
- package macOS
- package Windows
- touch Microsoft Store
- redesign icon
- add product features
- refactor persistence/model architecture
- address M3 scale optimisation
- rewrite Markdown
- update dependencies

This ticket ends with a locally generated and verified Omarchy/Arch package,
plus reusable Linux desktop-integration metadata.