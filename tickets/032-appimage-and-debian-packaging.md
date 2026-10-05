# Ticket 032 — AppImage and Debian/Ubuntu packaging

## Goal

Add two additional Linux distribution formats for OmaTree v0.1.0:

1. a portable x86_64 AppImage
2. a native Debian/Ubuntu amd64 .deb package

The existing Arch/Omarchy package from Ticket 029 remains unchanged in
principle.

This ticket must produce and verify:

    dist/OmaTree-0.1.0-x86_64.AppImage
    dist/omatree_0.1.0_amd64.deb

alongside the existing:

    dist/omatree-0.1.0-1-x86_64.pkg.tar.zst

Do not publish anything.

Do not make the GitHub repository public.

Do not create a Git tag or GitHub release.

Do not upload packages anywhere.

Ticket 033 will perform the public v0.1.0 release.

---

# Distribution strategy

The three Linux formats intentionally use different runtime strategies.

## Arch / Omarchy

Existing strategy:

    native package
    system Qt
    pacman dependencies

Do not redesign it.

## Debian / Ubuntu

Use:

    native .deb package
    system Qt
    APT/dpkg dependencies

Do not bundle Qt inside the .deb unless a concrete compatibility problem makes
that unavoidable and is reported before implementation.

## AppImage

Use:

    self-contained portable application
    bundled Qt runtime and required plugins/QML modules

The AppImage must not depend on the target distribution having OmaTree's Qt
version installed.

---

# Architectures

This ticket targets:

    AppImage: x86_64
    Debian:   amd64

Do not add ARM packages.

Do not cross-compile.

Future architectures can be added after v0.1.

---

# Part 1 — Controlled build environments

Do not build the portable artifacts directly against the Arch/Omarchy host
userspace.

Use clean, reproducible Linux build environments.

Prefer an available container runtime:

- Podman
- Docker

Use whichever is already installed and suitable.

Do not install a new host container runtime automatically.

If neither is available, report the blocker before falling back to a host build.

All container work should use throwaway build environments and mounted/copied
source.

Do not modify the host's /usr.

Do not require sudo outside the disposable container environment.

---

# Part 2 — AppImage compatibility baseline

The AppImage's portability is constrained by the oldest glibc and related
system ABI against which its executable and bundled libraries were built.

Therefore:

- select the oldest practical build userspace in which OmaTree and its required
  Qt version can be built reliably
- prefer compatibility over using the newest distro

A likely starting candidate is Ubuntu 22.04 x86_64 or another userspace with a
similarly conservative glibc baseline.

However:

DO NOT declare Ubuntu 22.04 compatibility merely because the container starts.

First verify that OmaTree and the required Qt/CXX-Qt stack can actually build
there.

If the distribution's system Qt is too old for OmaTree:

- investigate using an appropriate Qt build within that older userspace
- ensure that Qt itself does not raise the resulting runtime ABI baseline

Do not silently move the AppImage build back to Arch.

If Ubuntu 22.04 proves impractical, choose the next-oldest reasonable baseline
and report the resulting minimum compatibility.

---

# Part 3 — Measure binary ABI requirements

For the AppImage executable and important bundled native libraries, inspect the
required versions of at least:

    GLIBC
    GLIBCXX
    CXXABI

Use suitable tools such as:

    readelf
    objdump
    strings

Report the maximum required versions.

The completion report must state the resulting practical Linux compatibility
floor.

Do not claim compatibility with distributions older than the measured ABI
requirements.

---

# Part 4 — AppImage tooling

Add AppImage packaging sources under:

    packaging/appimage/

Prefer maintained AppImage tooling such as:

- linuxdeploy
- Qt deployment support/plugin
- appimagetool

or an equivalent current workflow.

Do not commit downloaded packaging-tool binaries into the repository.

If external packaging tools are downloaded:

- pin the exact release/version
- verify a published SHA-256 or another trustworthy checksum where practical
- cache only outside the tracked source tree
- document the versions used

Do not depend on "latest" URLs in a release build script.

The packaging process must be reproducible enough that Ticket 033 can rebuild
the same release intentionally.

---

# Part 5 — AppDir layout

Create a standards-compliant AppDir.

Conceptually it should contain:

    OmaTree.AppDir/
        AppRun
        io.github.Dorotabro.OmaTree.desktop
        io.github.Dorotabro.OmaTree.svg
        usr/
            bin/
                omatree
            lib/
                ...
            plugins/
                ...
            qml/
                ...
            share/
                applications/
                mime/
                metainfo/
                icons/
                licenses/
                doc/

Exact Qt deployment layout may be chosen by the deployment tooling.

Reuse the Linux metadata already created in Ticket 029.

Do not maintain subtly different copies of:

- desktop entry
- MIME XML
- AppStream metadata
- logo

Use the canonical files from `packaging/linux/` and `assets/logo/`.

---

# Part 6 — AppImage Qt deployment

Bundle the Qt runtime required by OmaTree.

Determine the actual modules/plugins from:

- linked shared libraries
- QML imports
- runtime plugin loading

The AppImage must contain all Qt components necessary to run on a normal
machine without system Qt.

At minimum inspect requirements around:

- Qt Core
- Qt Gui
- Qt Qml
- Qt Quick
- Qt Quick Controls Basic
- Qt Quick Templates
- Qt Quick Layouts
- Qt Quick Dialogs
- platform plugins
- image format plugins where needed

The deployment must include the QML modules, not merely the linked `.so`
libraries.

A package that launches the binary but shows missing-QML-module errors does not
pass.

---

# Part 7 — Display platform plugins

The AppImage should support normal modern Linux desktop environments.

Where practical include working support for:

    Wayland
    X11/XCB
    offscreen

Do not bundle arbitrary platform plugins simply because they exist.

Verify what Qt actually needs.

On the Omarchy host, perform a real Wayland launch test.

If XWayland/X11 is available, also verify the XCB platform where practical.

The offscreen platform remains the automated smoke-test baseline.

---

# Part 8 — AppImage library policy

Do not indiscriminately copy every library returned by `ldd`.

Avoid bundling low-level system libraries that AppImage conventionally expects
from the host where bundling them would reduce portability or create ABI
problems.

In particular inspect the treatment of:

- glibc
- dynamic loader
- libpthread where relevant
- librt where relevant
- libdl where relevant
- graphics stack libraries
- Mesa/DRM
- Wayland client system libraries
- X11 system libraries

Let maintained deployment tooling drive this where sensible, then audit its
output.

Do not bundle host GPU drivers.

---

# Part 9 — AppImage Qt licensing

Unlike the Arch and Debian native packages, the AppImage redistributes Qt
shared libraries.

Therefore implement the Qt LGPL obligations documented by Ticket 022.

At minimum the AppImage must contain accessible documentation stating:

- Qt version redistributed
- Qt modules redistributed
- applicable LGPL licence
- Qt copyright/attribution
- where the corresponding Qt source code can be obtained
- that OmaTree uses dynamically linked Qt libraries

Include the appropriate LGPL licence text.

Reuse/extend THIRD_PARTY.md where sensible.

Do not claim legal conclusions beyond the existing Ticket 022 audit.

Do not convert Qt to static linking.

The AppImage binary must remain dynamically linked to the bundled Qt shared
libraries.

---

# Part 10 — AppImage resource safety

Packaging must not weaken OmaTree's Markdown/resource protections.

Run the existing integration suite before packaging.

Then verify the packaged application still:

- launches without network activity merely because a note is previewed
- blocks remote Markdown image loads
- blocks local-file Markdown resource loads

Do not build a second renderer configuration for AppImage.

---

# Part 11 — AppImage application metadata

The AppImage must embed/use:

    Name=OmaTree
    Exec=omatree %f
    Icon=io.github.Dorotabro.OmaTree
    MIME=application/x-omatree

Use the existing canonical desktop entry where possible.

Passing one `.omatree` path to the AppImage must open that notebook.

Example:

    ./OmaTree-0.1.0-x86_64.AppImage ~/Notes/example.omatree

must work.

---

# Part 12 — AppImage desktop integration expectations

Do not pretend that merely downloading an AppImage automatically registers MIME
types and launchers system-wide.

Document accurately:

- direct execution works without installation
- desktop/menu integration depends on the user's desktop/AppImage integration
  tooling
- the AppImage contains the correct desktop/icon/MIME metadata for such tools

Do not write an automatic installer in this ticket.

---

# Part 13 — AppImage output

Produce:

    dist/OmaTree-0.1.0-x86_64.AppImage

Mark it executable.

Generate its SHA-256.

Report:

- compressed AppImage size
- extracted AppDir size
- maximum GLIBC/GLIBCXX/CXXABI requirements
- bundled Qt version
- bundled Qt modules
- platform plugins
- QML modules
- Qt licence files included

---

# Part 14 — AppImage verification

Verify outside the source tree.

At minimum:

1. copy the AppImage to a clean temporary directory
2. ensure it is executable
3. launch with:
       QT_QPA_PLATFORM=offscreen
4. create/open a temporary `.omatree` notebook
5. confirm no QML warnings or binding loops
6. confirm no missing plugins/QML modules
7. inspect bundled libraries
8. inspect application metadata
9. inspect icon
10. verify no absolute developer/repository paths are present

Also test:

    APPIMAGE_EXTRACT_AND_RUN=1

or the equivalent supported extract-and-run mechanism, so environments without
working FUSE have a documented fallback if applicable.

Do not assert that FUSE is unnecessary unless verified.

---

# Part 15 — AppImage clean-environment testing

Run the AppImage in clean disposable Linux environments where technically
possible.

Test at least two of:

- selected build-baseline distribution
- Debian 12
- Ubuntu 24.04

Prefer three if inexpensive.

Use offscreen execution inside containers.

The test must use the produced AppImage artifact, not rebuild OmaTree inside
each test distribution.

If AppImage runtime/FUSE cannot operate inside a container, use its supported
extract-and-run mode and clearly report that limitation.

The important question is:

    Does the already-built AppImage executable and bundled runtime run there?

Report each tested environment separately.

---

# Part 16 — Debian/Ubuntu package baseline

Create a native `.deb` for amd64.

Prefer building on a conservative Debian-family baseline rather than Arch.

Start with:

    Debian 12 amd64

unless actual Qt/build requirements make that impossible.

The goal is to maximize compatibility with modern Debian and Ubuntu-family
systems while still using system Qt.

If Debian 12 cannot build OmaTree because its Qt or compiler environment is too
old:

- determine the concrete reason
- choose the next reasonable baseline
- report it

Do not silently build the `.deb` on Arch and wrap the Arch binary.

---

# Part 17 — Debian packaging sources

Add packaging files under:

    packaging/debian/

Prefer a normal Debian packaging workflow.

A suitable structure may contain:

    packaging/debian/debian/
        control
        rules
        changelog
        copyright
        install
        source/format
        ...

and a wrapper such as:

    packaging/debian/build-package.sh

The wrapper may create a temporary source tree and place the Debian packaging
directory at the expected root.

Do not clutter the OmaTree repository root with generated Debian build files.

Use standard Debian tools where practical:

    dpkg-buildpackage
    debhelper

Do not manually assemble an ad-hoc ar archive if normal tooling is available.

---

# Part 18 — Debian package metadata

Produce package:

    Package: omatree
    Version: 0.1.0
    Architecture: amd64
    Section: utils
    Priority: optional

Description should accurately describe OmaTree.

Suggested short description:

    local-first tree-structured note-taking application

Use:

    Homepage: https://github.com/Dorotabro/OmaTree

Use the project's actual dual licence in Debian copyright metadata.

Do not claim the GitHub release exists yet.

---

# Part 19 — Debian dependencies

The `.deb` uses system Qt.

Derive native ELF dependencies using standard Debian tooling such as:

    dpkg-shlibdeps
    dh_shlibdeps

Do not hardcode version constraints merely by copying Arch dependencies.

Also inspect QML imports because QML module packages are runtime dependencies
that ELF dependency scanning may not detect.

Add the required QML packages explicitly.

Audit at least the packages providing:

- Qt Quick
- Qt Quick Controls
- Qt Quick Templates
- Qt Quick Layouts
- Qt Quick Dialogs
- any other imported QML module

The resulting package must install on a clean test system using:

    apt install ./omatree_0.1.0_amd64.deb

without the user manually hunting down missing QML modules afterward.

---

# Part 20 — Debian system integration

Reuse the existing Linux installation metadata.

The `.deb` should install essentially the same logical files as the Arch
package:

    /usr/bin/omatree
    /usr/share/applications/io.github.Dorotabro.OmaTree.desktop
    /usr/share/mime/packages/io.github.Dorotabro.OmaTree.xml
    /usr/share/metainfo/io.github.Dorotabro.OmaTree.metainfo.xml
    /usr/share/icons/hicolor/...
    /usr/share/licenses or Debian-appropriate copyright/documentation paths
    /usr/share/doc/omatree/...

Follow Debian policy for licence/copyright locations rather than blindly
duplicating Arch paths if conventions differ.

Do not ship Qt libraries inside the `.deb`.

---

# Part 21 — Debian maintainer scripts

Avoid maintainer scripts unless actually needed.

Modern desktop/MIME/icon cache updates are generally handled by package-manager
triggers.

Do not manually run cache-update commands in postinst if standard package
triggers already provide the correct behaviour.

If maintainer scripts are genuinely required, explain why.

---

# Part 22 — Build path remapping

Apply the same release privacy principle as the Arch package.

The `.deb` and AppImage executables must not embed:

- developer home path
- absolute repository path
- temporary build-root path

Use the existing Rust/C++ remapping approach where appropriate.

Inspect each final binary.

Normal package metadata that records a generic build environment is not the
same as leaking the developer's personal filesystem path.

---

# Part 23 — Debian package output

Produce:

    dist/omatree_0.1.0_amd64.deb

Generate its SHA-256.

Report:

- compressed package size
- installed size
- exact Depends field
- recommended/suggested dependencies, if any
- file manifest summary

---

# Part 24 — Debian validation tools

Where available run:

    dpkg-deb --info
    dpkg-deb --contents
    lintian

`lintian` is optional if not installed in the build/test environment.

Do not install it on the host merely for this ticket.

Inside a disposable Debian build container it may be installed as part of the
packaging toolchain if convenient.

Report all warnings.

Do not suppress warnings blindly.

---

# Part 25 — Debian clean installation test

Test the generated `.deb` in clean disposable systems.

At minimum test:

    Debian 12
    Ubuntu 24.04

if dependency resolution permits.

Optionally test Ubuntu 22.04 as an additional compatibility probe.

For each environment:

1. start clean container
2. copy in the already-built `.deb`
3. run:
       apt update
       apt install ./omatree_0.1.0_amd64.deb
4. verify package status
5. inspect:
       dpkg -L omatree
6. run OmaTree using:
       QT_QPA_PLATFORM=offscreen
7. create/open a temporary notebook
8. confirm no QML/plugin warnings
9. uninstall/purge OmaTree
10. verify package-owned files are removed

Do not rebuild the package separately per test distribution.

We are testing one distributable artifact.

If one distro cannot satisfy dependencies:

- do not weaken dependencies
- report the real support floor

---

# Part 26 — Debian/Ubuntu support claim

The README must only claim environments actually tested.

For example, if verification succeeds on Debian 12 and Ubuntu 24.04:

    Debian 12+
    Ubuntu 24.04+
    compatible derivatives with equivalent Qt 6 runtime packages

If Ubuntu 22.04 also passes, the documented floor may be lowered accordingly.

Do not claim:

    all Debian-based Linux distributions

because that cannot be meaningfully guaranteed.

---

# Part 27 — Shared packaging verification

Extend the packaging verification tooling rather than creating three completely
unrelated verification philosophies.

A reasonable structure is:

    packaging/
        verify-package.sh
        linux/
        arch/
        appimage/
        debian/

It is acceptable to add format-specific verifier scripts invoked by a shared
top-level script.

Keep each script understandable.

Do not build a general-purpose packaging framework.

---

# Part 28 — Checksums

Generate or update:

    dist/SHA256SUMS

to contain the current pre-release artifacts:

    omatree-0.1.0-1-x86_64.pkg.tar.zst
    OmaTree-0.1.0-x86_64.AppImage
    omatree_0.1.0_amd64.deb

The checksums will be regenerated from the final tagged source in Ticket 033.

Do not sign packages yet unless signing infrastructure already exists and is
explicitly configured.

---

# Part 29 — Documentation

Update:

    README.md
    docs/PACKAGING.md

Document the three Linux distribution options.

Suggested user-facing structure:

## Omarchy / Arch Linux

    sudo pacman -U ./omatree-0.1.0-1-x86_64.pkg.tar.zst

## Debian / Ubuntu

    sudo apt install ./omatree_0.1.0_amd64.deb

Use `apt install ./...`, not raw `dpkg -i`, because APT resolves dependencies.

## Other x86_64 Linux

    chmod +x OmaTree-0.1.0-x86_64.AppImage
    ./OmaTree-0.1.0-x86_64.AppImage

Document the actual tested compatibility floor after verification.

Do not imply Windows/macOS binaries exist.

---

# Part 30 — Source builds

Preserve source-build documentation.

Do not make packaged binaries the only documented path.

Source users should still be able to build with the existing Cargo/Qt
instructions.

---

# Part 31 — Existing application verification

Before and after packaging changes run:

    cargo fmt --check
    cargo clippy --all-targets
    cargo test
    cargo test integration_tests
    cargo build
    cargo build --release

Expected current baseline:

    290 Rust tests
    1 ignored benchmark
    21 integration scenarios

Do not change application behaviour unless packaging exposes a real build bug.

If a source change becomes necessary for Debian/AppImage compatibility:

- make the smallest compatible change
- add appropriate regression coverage
- clearly report it

Do not opportunistically refactor.

---

# Part 32 — Real host AppImage smoke test

On the current Omarchy/Hyprland system, run the final AppImage outside the
repository.

Verify:

- correct application icon
- correct Wayland app_id
- real Wayland launch
- Search/tree/editor work
- open an existing `.omatree` file argument
- Markdown Preview works
- native/system file dialogs work from the bundled Qt environment

Do not change the installed Arch package to perform this test.

The AppImage must coexist with the installed native package.

---

# Part 33 — Final artifact inspection

Before completing this ticket, inspect all three Linux packages side by side.

Confirm:

## Arch

- system Qt
- no bundled Qt libraries

## Debian

- system Qt
- no bundled Qt libraries
- correct APT dependencies

## AppImage

- bundled Qt
- no dependence on system Qt
- Qt licence/source information included

All three must:

- contain the canonical icon
- use application ID io.github.Dorotabro.OmaTree
- open one `.omatree` path correctly
- contain no developer absolute paths
- use version 0.1.0

---

# Out of scope

Do not:

- publish packages
- upload to GitHub
- make the repository public
- tag v0.1.0
- create a GitHub release
- publish to AUR
- create an apt repository
- add PPA infrastructure
- add Flatpak
- add Snap
- add RPM
- add ARM builds
- add macOS packaging
- add Windows packaging
- add auto-update
- sign artifacts unless already configured
- add application features
- redesign UI
- update Rust dependencies merely because newer versions exist

This ticket ends with three locally verified Linux artifacts ready for the
final v0.1.0 release pass.