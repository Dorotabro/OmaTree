# Packaging

How OmaTree is packaged for Linux, macOS and Windows, and what a release still needs.
The three Linux formats are built from the same sources and the same desktop metadata
(macOS and Windows are below):

| Format | File | Qt | Built in |
|---|---|---|---|
| Arch / Omarchy | `omatree-0.1.1-1-x86_64.pkg.tar.zst` | system | the host (`makepkg`) |
| Debian / Ubuntu | `omatree_0.1.1_amd64.deb` | system | Debian 13 container |
| AppImage | `OmaTree-0.1.1-x86_64.AppImage` | **bundled** (6.8.3) | Ubuntu 20.04 container |

All three are written to `dist/` and listed in `dist/SHA256SUMS`. Nothing is
published, tagged or uploaded by any of these scripts.

## Identity

| Item | Value |
|---|---|
| Display name | OmaTree |
| Executable | `omatree` |
| Version | 0.1.1 |
| Application ID | `io.github.Dorotabro.OmaTree` |
| Notebook extension | `.omatree` (a SQLite database inside) |
| MIME type | `application/x-omatree` |
| Icon name | `io.github.Dorotabro.OmaTree` |

The application ID is the desktop-file id (the `.desktop` file name without its
suffix), the AppStream component id and the icon name, and it is what the
program tells Qt (`QGuiApplication::setDesktopFileName`) so that the launcher,
task switcher and Wayland compositor match a running window to its launcher.
The QML module URI `org.omatree` is an internal implementation detail and is
unrelated: it is never shown to the desktop.

The window icon is compiled into the executable from `assets/logo/` (see
`build.rs` and `cpp/app_identity.cpp`), so the window has it however OmaTree is
started, not only when launched from the desktop file.

## Files

Installed by `packaging/linux/stage.sh DESTDIR` (default prefix `/usr`):

    /usr/bin/omatree
    /usr/share/applications/io.github.Dorotabro.OmaTree.desktop
    /usr/share/mime/packages/io.github.Dorotabro.OmaTree.xml
    /usr/share/metainfo/io.github.Dorotabro.OmaTree.metainfo.xml
    /usr/share/icons/hicolor/scalable/apps/io.github.Dorotabro.OmaTree.svg
    /usr/share/icons/hicolor/{32x32,64x64,128x128,256x256,512x512}/apps/io.github.Dorotabro.OmaTree.png
    /usr/share/licenses/omatree/{LICENSE-MIT,LICENSE-APACHE,THIRD_PARTY.md}
    /usr/share/doc/omatree/README.md

- The desktop entry opens one notebook per process: `Exec=omatree %f`.
- The MIME type is matched by the `*.omatree` glob only. There is deliberately
  no SQLite magic: generic SQLite databases must never be claimed.
- The 1024-pixel PNG is not installed (it is not a hicolor size); the SVG is the
  scalable icon. The SVG is self-contained (its image is embedded) and needs no
  fonts.
- `stage.sh` fails on any missing input, writes only inside DESTDIR, and never
  touches the host or updates MIME/icon caches. The package manager's hooks do
  that after a real installation.

## Qt and licensing

The Arch package (and the Debian one) uses the **system Qt** and does **not**
contain Qt: no Qt shared library is copied into it, so it redistributes no Qt
binaries. The AppImage is the exception: it bundles Qt, see below. See
[`THIRD_PARTY.md`](../THIRD_PARTY.md) for the Qt (LGPL-3.0, linked dynamically)
and Rust dependency inventory and the obligations a package that did bundle Qt
would have. OmaTree's own license files and `THIRD_PARTY.md` are installed under
`/usr/share/licenses/omatree/`. SQLite is compiled into the executable
(public domain); nothing needs a NOTICE file.

### Arch dependencies

Derived from the built binary, not guessed:

- `readelf -d` shows direct links to `libQt6Core`, `libQt6Gui`, `libQt6Qml`
  (`qt6-base`, `qt6-declarative`), `libstdc++`, `libgcc_s`, libm/libc.
- The QML imports are `QtQuick`, `QtQuick.Controls` (Basic style),
  `QtQuick.Layouts`, `QtQuick.Dialogs` and QtQuick Templates: all in
  `qt6-declarative`.
- The Wayland platform plugin (`libqwayland.so`) and the offscreen plugin are
  in `qt6-base`.

So `depends=(qt6-base qt6-declarative glibc libgcc libstdc++)`. Optional:
`qt6-wayland` (extra Wayland integration plugins) and `xdg-desktop-portal`
(native file dialogs). Other libraries seen loaded on a desktop (GTK platform
theme, input methods, SVG) come from that desktop's Qt platform plugins, not
from OmaTree.

## Building the Arch package

    packaging/arch/build-package.sh

writes `dist/omatree-0.1.1-1-x86_64.pkg.tar.zst`, a source snapshot
`dist/omatree-0.1.1.tar.gz` and `dist/SHA256SUMS` (`dist/` is git-ignored).
It snapshots the working tree, fills `packaging/arch/PKGBUILD.in` with the
tarball and its SHA-256, and runs `makepkg`. The build uses `cargo fetch
--locked` and `cargo build --release --locked --frozen`, so only what
`Cargo.lock` names is built. Set `CARGO_NET_OFFLINE=true` to use only the local
Cargo cache.

**Build paths** are removed from the distributable binary with
`--remap-path-prefix` (Rust) and `-ffile-prefix-map` (C++) for the source tree,
the Cargo home and `$HOME`. Ordinary development builds are unchanged.

**For a public release:** the template's `source` must become the
tagged GitHub archive URL and `sha256sums` its real checksum (never `SKIP`).
The metainfo file also needs its first `<release>` (version and date).

## Debian / Ubuntu package

    packaging/debian/build-package.sh

builds `dist/omatree_0.1.1_amd64.deb` with the normal Debian tooling
(`dpkg-buildpackage`, debhelper 13) in a disposable **Debian 13** container (needs
Docker or Podman; `packaging/debian/Dockerfile` pins the image by digest). The
packaging is `packaging/debian/debian/`; the script copies it to the root of a
temporary copy of the source, so the repository root stays clean. Rust is
installed in the container from a pinned, checksummed `rustup-init` because
Debian's own Rust is too old; the Cargo download cache is kept outside the tree
(`~/.cache/omatree-packaging`).

- **Depends** is derived by `dh_shlibdeps` for the executable's libraries
  (`libc6`, `libgcc-s1`, `libstdc++6`, `libqt6core6t64`, `libqt6gui6`,
  `libqt6qml6`) plus the QML modules, which ELF scanning cannot see, listed by
  hand from the QML imports: `qml6-module-qtquick`, `-qtquick-controls`,
  `-qtquick-dialogs`, `-qtquick-layouts`, `-qtquick-templates`,
  `-qtquick-window`, `-qtqml-workerscript`. **Recommends:** `qt6-wayland`,
  `xdg-desktop-portal`.
- **Files:** the same as the Arch package, except that licences follow Debian
  policy: `/usr/share/doc/omatree/copyright` (MIT or Apache-2.0),
  `THIRD_PARTY.md`, `README.md`, and no `/usr/share/licenses`. No Qt library is
  in the package. There are **no maintainer scripts**: package-manager triggers
  update the MIME, icon and desktop caches.
- **Qt version.** OmaTree's QML needs a Qt newer than 6.4. On Qt 6.4.2 (Debian 12
  and Ubuntu 24.04) the `.deb` installs, but the application prints a stream of
  QML errors (`Qt.styleHints.colorScheme` and the Tree delegate's properties are
  `undefined`), and on Qt 6.2 (Ubuntu 22.04) the `TreeViewDelegate` type does not
  exist at all. So the baseline is Debian 13 (Qt 6.8.2), and Debian 12 and
  Ubuntu 22.04/24.04 are **not supported**. The dependency is not weakened to hide
  this.
- **Lintian** (run in the build container, nothing suppressed): `W: no-manual-page`
  and `I: hardening-no-fortify-functions` (the executable is Rust). The
  spelling hint `ment`/`meant` in the binary comes from a dependency's text.

Test the finished file on clean systems (it is never rebuilt per distribution):

    packaging/debian/test-install.sh dist/omatree_0.1.1_amd64.deb debian:13
    packaging/debian/test-install.sh dist/omatree_0.1.1_amd64.deb ubuntu:26.04

`apt install` with `--no-install-recommends` (the Depends field alone must
suffice), `dpkg -s`/`dpkg -L`, a headless run on a new notebook with no QML or
plugin output, then `apt purge` and a check that no package file remains.
Passing (with the 0.1.0 package; rerun for each release): Debian 13, Ubuntu 25.10, Ubuntu 26.04.

## AppImage

    packaging/appimage/build-package.sh

builds `dist/OmaTree-0.1.1-x86_64.AppImage` in a disposable container
(`packaging/appimage/Dockerfile`, digest-pinned). It builds OmaTree against an
official **Qt 6.8.3** and bundles that Qt with linuxdeploy; `Exec=omatree %f`
means `./OmaTree-0.1.1-x86_64.AppImage notebook.omatree` opens the notebook.

- **Build userspace: Ubuntu 20.04** (glibc 2.31). An AppImage cannot run on an
  older glibc than the one it was built against, so the oldest userspace that
  builds reliably sets the floor. Ubuntu 22.04 also builds, but its binaries need
  glibc 2.35 (`hypotf@GLIBC_2.35`) and 2.34 from the copied system libraries;
  20.04 gives **GLIBC 2.30**. Ubuntu's own Qt (6.2) cannot run OmaTree, so Qt
  comes from the official binary release, installed in a first Docker stage with
  a pinned `aqtinstall` (3.3.0) and copied in; those Qt libraries are built for an
  older glibc and do not raise the floor.
- **Tools**, exact releases, SHA-256 verified in the Dockerfile, never "latest":
  linuxdeploy `1-alpha-20250213-2`, linuxdeploy-plugin-qt `1-alpha-20250213-1`,
  appimagetool `1.9.1`, type2-runtime `20251108`. appimagetool and the runtime
  match the digests GitHub publishes; linuxdeploy and its Qt plugin publish none,
  so their hashes are those of the files that were tested. The binaries are not
  in the repository.
- **Measured ABI** (`objdump -T` over the executable and every bundled library):
  maximum **GLIBC 2.30, GLIBCXX 3.4.21, CXXABI 1.3.11**. Practical floor: a Linux
  with glibc 2.30 or newer and libstdc++ from GCC 5 or newer, and the desktop
  libraries below. That covers Ubuntu 20.04+, Debian 11+ and anything of that age
  or newer; nothing older is claimed.
- **Bundled:** the Qt modules Core, DBus, Gui, Network, OpenGL, Qml, QmlMeta,
  QmlModels, QmlWorkerScript, Quick, QuickControls2 (Basic style only),
  QuickTemplates2, QuickLayouts, QuickDialogs2, Labs.FolderListModel,
  WaylandClient, XcbQpa; the QML modules QtQuick, Controls (Basic), Templates,
  Layouts, Dialogs, Window, QtQml, Models, WorkerScript; the platform plugins
  `xcb`, `wayland-generic`, `wayland-egl` and `offscreen`, with the Wayland shell,
  decoration and graphics plugins; the compose and IBus input contexts and the
  XDG desktop portal theme; and ICU 73.2. This list was checked with
  `QML_IMPORT_TRACE` and `QT_DEBUG_PLUGINS` (what Qt really loads), then
  everything else linuxdeploy copies was removed: the other four Controls
  styles, Effects, Particles, Shapes, SQL, XML models, TLS backends and image
  plugins (the app icon is a built-in PNG; Markdown images are never loaded).
- **Not bundled**, by design (the host's low-level stack): glibc, libstdc++,
  libgcc, libGL/EGL/GLX, libdrm/Mesa and GPU drivers, libX11/libxcb (the core
  ones), libwayland-client, libxkbcommon and libxkbcommon-x11, fontconfig, FreeType. The host must
  provide them, as any desktop does; `libcom_err` and `libgpg-error` too. Bundled
  from the build system and so listed with their licences: glib, D-Bus, systemd,
  krb5, libpng, brotli and similar (`BUNDLED-LIBRARIES.txt`).
- **Platform theme.** linuxdeploy-plugin-qt's `gtk2` theme hook is replaced:
  the host's Qt themes (`gtk3`, `kde`, `qt6ct`) cannot be loaded from inside an
  AppImage, so when none or gtk2/gtk3 is requested the bundled `xdgdesktopportal`
  theme is selected, which gives the system's own file dialogs through
  xdg-desktop-portal (tested with xdg-desktop-portal-gtk on Hyprland). Qt falls
  back to its built-in dialog when there is no portal.
- **Licences.** The AppImage redistributes Qt, so it carries
  `usr/share/doc/omatree/QT-NOTICE.md` (version, modules, LGPL-3.0, copyright,
  dynamic linking, how to replace Qt, where the exact source is),
  `Qt-LGPL-3.0.txt`, `Qt-GPL-3.0.txt`, `ICU-73.2-LICENSE.txt` and the copyright
  files of the other bundled libraries. See `THIRD_PARTY.md`.
- **Desktop integration.** An AppImage does not register itself. Running it
  directly works; the menu entry and the `.omatree` association depend on the
  user's AppImage integration tool, which reads the desktop file, icon and MIME
  data inside it. There is no installer. Without FUSE, run it with
  `APPIMAGE_EXTRACT_AND_RUN=1` (tested: it works without FUSE in a container).

Test the finished file on clean systems (it is not rebuilt per distribution):

    packaging/appimage/test-container.sh dist/OmaTree-0.1.1-x86_64.AppImage ubuntu:20.04

Step one runs it in the bare image (which prints the host libraries it needs),
step two installs the distribution's usual desktop libraries and requires a clean
headless run with extract-and-run. Passing (with the 0.1.0 AppImage; rerun for each release): Ubuntu 20.04, Debian 11, 12 and 13,
Ubuntu 24.04 and 26.04.

## macOS (Apple Silicon)

    packaging/macos/build-dmg.sh

writes `dist/OmaTree-0.1.1-arm64.dmg` (about 36 MB) and `dist/OmaTree-0.1.1-arm64.dmg.sha256` and leaves an inspection copy
at `dist/macos/OmaTree.app` (about 86 MB). **arm64 only**: there is no universal
or x86_64 build, and the script refuses to run on another architecture. The app
needs **macOS 14 or newer**, which is what Qt 6.11.2 itself requires; the script
reads that from the Qt it bundles and writes it to `LSMinimumSystemVersion`.

**Status: published as unsigned and not notarized (v0.1.1).** The app is ad-hoc
signed only (arm64 will not run unsigned), so Gatekeeper blocks a downloaded copy
until the user approves it in System Settings ▸ Privacy & Security ▸ Open Anyway;
the README says so. Developer ID signing and notarization are still to do (below).

### Tools

macOS on Apple Silicon, Rust, **Qt 6** with `macdeployqt` (built with Homebrew's
`qt` 6.11.2; set `QT_PREFIX` for another), and the Xcode command line tools
(`iconutil`, `sips`, `codesign`, `otool`, `vtool`, `hdiutil`, `plutil`). Full Xcode
is not needed. The script stops with a clear message if any is missing.

### What the script does

1. `cargo build --release --locked` into `target/macos` (so development builds are
   undisturbed), with `--remap-path-prefix` / `-ffile-prefix-map` so that no
   source, Cargo, home or Qt-install path is left in the executable.
2. Assembles `OmaTree.app` **in a temporary directory**, not in the checkout: a
   checkout inside iCloud Drive (`~/Documents`) gets Finder / file-provider
   attributes on every directory, and `codesign` refuses them.
3. Writes `Info.plist` from `packaging/macos/Info.plist.in` and the icon.
4. Runs `macdeployqt -qmldir=qml`, then **prunes** (`packaging/macos/prune.sh`).
5. Makes the bundle self-contained (`make_relocatable`), writes its third-party
   notices (`licenses.py`), signs it ad hoc, and runs
   `packaging/macos/verify-app.sh` on it.
6. Packs a plain HFS+ disk image: `OmaTree.app` and an `Applications` shortcut.

### Bundle and identity

    OmaTree.app/Contents/{Info.plist, PkgInfo, MacOS/omatree, Frameworks/, PlugIns/,
                          Resources/{OmaTree.icns, qml/, qt.conf, licenses/}, _CodeSignature/}

| Key | Value |
|---|---|
| `CFBundleName`, `CFBundleDisplayName` | OmaTree |
| `CFBundleIdentifier` | `io.github.Dorotabro.OmaTree` |
| `CFBundleExecutable` / `CFBundlePackageType` | `omatree` / `APPL` |
| `CFBundleShortVersionString`, `CFBundleVersion` | 0.1.1 (a valid three-part version; no separate build number) |
| `LSMinimumSystemVersion` | 14.0 (from Qt) |
| `CFBundleIconFile` | `OmaTree` (`OmaTree.icns`) |

The executable is the ordinary release binary, with no wrapper. The window title
is `OmaTree — <notebook>`; the application name shown in the menu bar and Dock
comes from `CFBundleName`.

**Icon.** `OmaTree.icns` is made by `iconutil` from the existing PNGs
(`assets/logo/omatree-icon-{32,64,128,256,512,1024}.png`), unaltered. Only the
16-pixel image is derived, by scaling the 32-pixel one down with `sips`. The
artwork is a full-bleed square, so macOS 26 draws it inside its own rounded tile.

**Document type.** Only `.omatree` is registered, as the exported type
`io.github.Dorotabro.OmaTree.notebook` (conforms to `public.data` only, MIME
`application/x-omatree`), with OmaTree as its Owner/Editor. It does not claim
SQLite or any generic type, matching the Linux MIME definition. A Finder
double-click or Open With is not a command-line argument on macOS: the system
sends the application a file-open event, which `cpp/file_open.cpp` (macOS only)
passes to `openFromSystem` in `main.qml`. That asks about unsaved changes exactly
like File ▸ Open. The command-line path still works as on Linux.

### Qt that is bundled

`macdeployqt` copies every module OmaTree's QML imports whole, including all six
Quick Controls styles, the virtual keyboard, PDF, multimedia and every image
format (133 MB). OmaTree uses the Basic style only, draws its own UI, makes no
network connection and loads no images, so `prune.sh` removes what is not used.
The lists in it are what Qt really loads, measured with `DYLD_PRINT_LIBRARIES`
over the integration suite on the real macOS platform plugin, plus the QML modules
the application imports:

- Frameworks: Core, DBus, Gui, Network, OpenGL, Qml, QmlMeta, QmlModels,
  QmlWorkerScript, Quick, QuickControls2 (+Basic, BasicStyleImpl, Impl),
  QuickDialogs2 (+QuickImpl, Utils), QuickLayouts, QuickTemplates2,
  Labs.FolderListModel; and the libraries they need (ICU, glib, OpenSSL because
  QtNetwork links it, freetype, harfbuzz, libpng, zstd, brotli, pcre2, dbus,
  md4c, double-conversion, libb2, graphite2, gettext).
- QML modules: QtQuick, Controls (Basic and impl only), Templates, Layouts,
  Dialogs, Window, QtQml (Models, WorkerScript), Qt.labs.folderlistmodel.
- Plug-ins: `platforms/libqcocoa`, `tls/` Secure Transport and certificates-only.
  No image-format, icon-engine, SQL, multimedia or input-context plug-ins.

`QtNetwork` is linked by the executable (through the Rust Qt bindings), which is
why the TLS plug-ins and OpenSSL come along although OmaTree never connects out.

### Dependency audit

`verify-app.sh` checks every Mach-O file in the bundle: arm64 only; every run path
is relative and ends inside the bundle; every dependency is a system library
(`/usr/lib`, `/System`) or resolves, the way dyld resolves it (`@rpath` through the
file's own run paths), to a file in the bundle; bundled libraries have `@rpath`
install names; no `/Users`, `/opt/homebrew` or the builder's home in the
executable, QML or configuration; `codesign --verify --deep --strict`; and that
the app starts with `env -i` and prints nothing. Run it on any copy:

    packaging/macos/verify-app.sh /Applications/OmaTree.app

`macdeployqt` leaves Homebrew's run paths (some pointing outside the bundle) and
some absolute install names; `make_relocatable` replaces them. Strings inside
Qt's own binaries still contain the Homebrew prefix as compiled-in defaults: those
are not dependencies, and `qt.conf` points Qt at the bundle.

### Licences of what the app contains

The app **contains** Qt and fifteen other libraries, so it carries their notices
(`Contents/Resources/licenses/`). They are written by
`packaging/macos/licenses.py` from the *pruned* bundle, not from a fixed list:
every file in `Contents/Frameworks` is traced back to the Homebrew keg it was
copied from, and its formula's version, licence, home page and source archive go
into `THIRD_PARTY_NOTICES.md`, with the keg's licence files under
`third-party/<library>/`. The script fails if a bundled file cannot be traced or
a library has no licence file, and `verify-app.sh` checks the result again (every
bundled library listed, nothing listed that is not bundled, every licence text
present). It needs Homebrew (`brew`) and `python3`.

Qt is used under the LGPL-3.0, dynamically linked: the notice says so, says how to
replace the frameworks (and to re-sign the app afterwards), and links the
corresponding source. The LGPL and GPL texts (`packaging/macos/licenses/`) are
the unmodified FSF texts taken from Homebrew kegs. The Homebrew kegs of Qt carry
no licence files of their own, so Qt's third-party code is covered by the formula
licence expressions and a pointer to Qt's own documentation, not by copies of
each text. The Rust crates and SQLite built into the executable are in
`THIRD_PARTY.md` (included). The libraries' licences are as Homebrew's formulae
declare them, reproduced as given; this is an inventory, not legal advice.

### Signing, Gatekeeper, notarization

- **State: ad-hoc signed** (`codesign -dv`: `Signature=adhoc`, no team
  identifier). Not Developer ID signed; **not notarized**; the DMG itself is not
  signed. `spctl --assess --type execute` says *rejected*, as expected.
- On this Mac `security find-identity -v -p codesigning` finds **no identities**,
  so no `Developer ID Application` certificate exists here.
- `xcrun notarytool` and `stapler` are present (Command Line Tools, notarytool
  1.1.2). Nothing has been submitted. To notarize later you need: an Apple
  Developer Program membership, a *Developer ID Application* certificate in the
  keychain, and notarytool credentials stored with
  `xcrun notarytool store-credentials` (an app-specific password or an API key).
  Then: build with `SIGN_IDENTITY="Developer ID Application: ..."` (hardened
  runtime and a secure timestamp; **untested**, nothing here could try it), sign
  the DMG, `notarytool submit --wait`, then `stapler staple` the DMG.
- Until then a copy downloaded from the internet carries the quarantine
  attribute, and Gatekeeper will not open an ad-hoc-signed app without the user
  approving it in System Settings ▸ Privacy & Security. A copy built and run on
  the same Mac is not quarantined.

### Testing a bundle

    packaging/macos/verify-app.sh dist/macos/OmaTree.app   # as above
    hdiutil attach dist/OmaTree-0.1.1-arm64.dmg            # then copy to /Applications
    open /Applications/OmaTree.app                         # LaunchServices, no shell environment
    open notebook.omatree                                  # the Finder double-click path

Install from the DMG (or the temporary bundle), not from a copy inside iCloud
Drive: the file provider adds attributes to its directories and the copy then
fails `codesign --verify`. (`dist/macos/OmaTree.app` is stripped of them by the
script, but a synced folder may add them back.)

## Windows (x64, Inno Setup)

    packaging\windows\build-installer.ps1

writes `dist\OmaTree-0.1.1-x64-setup.exe` (about 13.4 MB; 47 MB installed) and
`dist\OmaTree-0.1.1-x64-setup.exe.sha256` (`<hash>  <file>`, ready to be merged into a
release-level `SHA256SUMS`). **Unsigned.** The version in both names is read from
`Cargo.toml`; a version bump needs no change in `packaging\windows\`.

**Status: built and tested on one Windows 11 machine (see "Clean-machine verification"
for what that does and does not prove). Not published; the README does not yet claim
Windows support.**

### Tools

| | Tested with |
|---|---|
| Windows | 10 1809 or later at run time (Qt 6.8's own floor); built on Windows 11 (10.0.26300) |
| Rust | 1.99.0, `x86_64-pc-windows-msvc` |
| C++ | MSVC 19.44 (Visual Studio 2022 Build Tools 14.44, C++ x64 workload) and Windows SDK 10.0.26100 (`rc.exe`) |
| Qt | 6.8.3 `msvc2022_64`, the official binaries, installed with `aqtinstall` 3.3.0 (`aqt install-qt windows desktop 6.8.3 win64_msvc2022_64 --archives qtbase qtdeclarative qtshadertools qtsvg`); the kit must have `bin\windeployqt.exe` and its `sbom\` folder |
| Inno Setup | **6.7.3** (`winget install JRSoftware.InnoSetup`); 6.3 or later is needed (`x64compatible`) |

The script finds each tool (parameters `-QtDir`, `-InnoSetup`, `-VcRedistDir`, or
`QT_DIR`/`QMAKE`/`ISCC`, `PATH`, the usual folders), loads the Visual Studio developer
environment itself, and stops with a message naming what is missing. It sets `QMAKE`
for the build from the Qt kit it found, so the packaged Qt is the one it built
against. `-SkipBuild` restages the last build; `-OutDir` changes `dist\`.

### What the script does

1. Reads the version from `Cargo.toml`.
2. `cargo build --release --locked` into `target\windows` (so development builds are
   untouched), with `--remap-path-prefix` for the checkout, Cargo and rustup,
   `/d1trimfile` for C++, and `/PDBALTPATH:%_PDB%`: **no build-machine path is left in
   the executable**.
3. Stages `target\windows-package\app`: the executable, then `windeployqt --release
   --qmldir qml` (with `--no-translations`, `--no-system-d3d-compiler`,
   `--no-system-dxc-compiler`, `--no-opengl-sw`, `--no-compiler-runtime` and
   `--skip-plugin-types generic,iconengines,imageformats,networkinformation,qmltooling,tls`),
   checks that every Qt file is byte-identical to the kit's, applies `prune.txt`, adds
   the C++ runtime, licences and notices.
4. `verify-package.ps1 -Stage`, then compiles `OmaTree.iss`, then
   `verify-package.ps1 -Installer`, then writes the checksum.

`make-icon.ps1` regenerates `windows\omatree.ico` (committed) from `assets\logo`; run it
only if the artwork changes. `PackagingTools.ps1` (a PE import reader and resource
inspector, so no `dumpbin` is needed), `write-licenses.ps1` and `prune.txt` are the
rest of `packaging\windows\`.

### Identity

| Item | Value |
|---|---|
| Installer | `OmaTree-<version>-x64-setup.exe` (Inno's setup program is itself a 32-bit executable; it installs the 64-bit application and refuses non-x64 Windows) |
| Inno **AppId** | `{60910A77-7C66-4B5D-ABCB-3D8A4D819908}`. **Never change it**: a different AppId installs side by side instead of upgrading |
| Windows **AUMID** | `io.github.Dorotabro.OmaTree`: the same string as the Linux desktop-file id and macOS bundle identifier (`DESKTOP_FILE_NAME` in `src/main.rs`) |
| **ProgID** | `OmaTree.Notebook` ("OmaTree Notebook") |
| Publisher | OmaTree contributors (the copyright holder named in `LICENSE-MIT`; no company is invented) |
| Install location | `%LOCALAPPDATA%\Programs\OmaTree`, per user, **no administrator rights or UAC prompt** (`PrivilegesRequired=lowest`) |
| Start menu | `OmaTree` (no folder), carrying the AUMID. A desktop shortcut is offered, **unchecked** |
| Add/Remove Programs | "OmaTree", version, publisher, the OmaTree icon, size (Settings, Apps shows 0.1.1, 47.3 MB) |

**AUMID.** `cpp/app_identity.cpp` calls `SetCurrentProcessExplicitAppUserModelID` first
thing (Windows only), and the installer gives the Start-menu shortcut and the ProgID the
same id. Verified on the real taskbar: one button, "OmaTree - 1 running window", whose
automation id is `Appid: io.github.Dorotabro.OmaTree`, and `Get-StartApps` lists
OmaTree with that AppID.

### The executable's resources

One path: `windows\omatree.rc`, compiled by `build.rs` with the SDK's `rc.exe` and
linked into the executable only (not the test programs). It holds the **icon** (seven
PNG-compressed images: 16, 24, 32, 48, 64, 128, 256 pixels; the 32 to 256 are the
existing `assets\logo` PNGs byte for byte, the others are scaled from the 1024 one), the
**version information** (OmaTree; description OmaTree; product and file version from
`Cargo.toml`; copyright; no company name) and the **manifest**
`windows\omatree.manifest` (UTF-8 active code page, `asInvoker`) as the one and only
manifest resource: a manifest is not also given to the linker. `verify-package.ps1`
fails on zero or two manifests, a missing icon or version resource, a console
subsystem, or a version that differs from `Cargo.toml`. The runtime window icon
(`cpp/app_identity.cpp`) is unchanged: Windows needs it for the title bar, taskbar and
Alt+Tab as before.

A future change that adds a resource must go into `omatree.rc`; adding another
manifest by any route makes the link fail with a duplicate-resource error.

### Qt that is bundled, and the MSVC runtime

`windeployqt` copies far more than OmaTree uses (103 MB). What stays (175 files, 43 MB):
the libraries Core, Gui, Network, OpenGL, Qml, QmlMeta, QmlModels, QmlWorkerScript,
Quick, QuickControls2 (+Basic, BasicStyleImpl, Impl), QuickDialogs2 (+QuickImpl, Utils),
QuickLayouts, QuickTemplates2, Labs.FolderListModel; `platforms\qwindows.dll`; the QML
modules QtQuick (+Window, Controls with the Basic style and `impl`, Templates, Layouts,
Dialogs with its QML fallback), QtQml (+Models, WorkerScript), Qt.labs.folderlistmodel
(14 QML plug-ins, 123 QML files).

What goes, and why (`prune.txt` and the `windeployqt` options; **every item is backed by
evidence**, nothing is removed because it merely looks unused):

- a traced run (`QT_DEBUG_PLUGINS=1`, `QML_IMPORT_TRACE=1`, the list of modules the
  process really loaded) over new note, Preview, Search, F1, Delete dialog, FILE menu,
  Recovery, native Open and Save As, close, with a scrubbed environment: only the Basic
  style, Templates, Controls, Layouts and Dialogs were loaded; no other style, no Effects
  or Shapes, no TLS, network-information, touch, icon-engine or debugger plug-in.
  Image-format plug-ins are asked about by Qt (the icon PNGs) but are not needed: PNG is
  built into Qt Gui, and the app icon is PNG; Markdown images are never loaded.
- the application forces the Basic style (`cpp/controls_style.cpp`);
- the dependency audit (nothing that stays imports anything removed) and a second traced
  run on the pruned tree: no QML or plug-in message, every module from the application
  folder;
- `d3dcompiler_47.dll`, `dxcompiler.dll`, `dxil.dll`: Qt Quick ships precompiled shaders
  and Windows has its own D3D runtime; `opengl32sw.dll` and the 25 MB `vc_redist.x64.exe`
  that `windeployqt` copies are not shipped (see below).

`QtNetwork` is linked (through the Rust Qt bindings) although OmaTree makes no network
connection, so `Qt6Network.dll` stays and the TLS plug-ins do not.

**MSVC runtime: app-local deployment.** The files import `MSVCP140`, `MSVCP140_1`,
`MSVCP140_2`, `VCRUNTIME140` and `VCRUNTIME140_1`. The script copies exactly those
DLLs (found by following the imports of every staged file, no more), unmodified, from
the Visual Studio Build Tools' `VC\Redist\MSVC\<version>\x64\Microsoft.VC143.CRT`
(14.44.35211.0), next to `omatree.exe`, where Windows finds them first (loaded and
confirmed from the install folder, not System32). So a machine without the
Visual C++ Redistributable works and **no prerequisite installer or download** is
involved; the Universal CRT (`api-ms-win-crt-*`) is part of Windows 10 and later. The
other route, running `vc_redist.x64.exe`, needs administrator rights and would break the
per-user install, so it is not used. The licence notice says where the files came from
and links Microsoft's redistribution documentation; it makes no further legal claim.

### Dependency audit (self-containment)

`verify-package.ps1 -Stage` reads the PE import table of every `.exe` and `.dll` in the
tree: each import must be a DLL in the application folder (where Windows looks, also
for plug-ins) or a Windows system DLL (an API-set name, or a file in `System32`).
It also fails if `omatree.exe` contains `C:\Users`, `.cargo`, `.rustup`, `BuildTools`,
`Visual Studio`, the checkout path or the user name, if any Qt DLL contains `C:\Qt`, or
if the tree holds import libraries, `.pdb`s, debug Qt, headers or a redistributable
installer. Measured on the installed app: with the Qt install folder renamed so that it
did not exist, `PATH` reduced to `System32`, and no `QMAKE`, the app ran with all 30
loaded modules from its own folder and none from `C:\Qt`.

### Licences of what the installer contains

`write-licenses.ps1` writes `licenses\THIRD_PARTY_NOTICES.md` from the **staged files**:
every `Qt6*.dll` is looked up in the SPDX SBOMs that ship with the Qt kit
(`sbom\qtbase-*.spdx.json`, `qtdeclarative-*.spdx.json`) for its licence and for the
third-party code Qt bundles in that module (zlib, PCRE2, libpng, FreeType, HarfBuzz,
md4c, double-conversion, tinycbor, the Unicode data, libpsl and its public-suffix list,
wintab, masm and others: 42 entries; some apply to other platforms or are header-only,
and the list errs towards including them), the Microsoft runtime files are listed with
their versions, and 22 licence texts (the texts of Qt's own `LICENSES\` directories at
tag v6.8.3, kept in `packaging\windows\licenses\`) are copied to `licenses\texts\`.
The script stops if a Qt module is not in the SBOMs or a text is missing, and the
verifier fails if a bundled file is not named in the notice or a listed licence has no
text, so a new Qt module cannot slip in unlisted. Qt is used under the LGPL-3.0,
dynamically linked, **unmodified** (the Qt files are checked to be byte-identical to the
kit), and the notice says how to replace it and where the source is. The installed
folder also holds `LICENSE-MIT`, `LICENSE-APACHE` and `THIRD_PARTY.md`. This is an
inventory, not legal advice.

### Installing, upgrading, uninstalling

**What the installer writes**, all per user: the files in `{app}`; the Start-menu
shortcut (and the optional desktop one); the uninstall entry under
`HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall`; and the `.omatree` type under
`HKCU\Software\Classes`: `.omatree` (default `OmaTree.Notebook`, `Content Type`
`application/x-omatree`, `OpenWithProgids`) and the ProgID `OmaTree.Notebook` with its
friendly name, `AppUserModelID`, `DefaultIcon` (`"{app}\omatree.exe",0`) and
`shell\open\command` = `"{app}\omatree.exe" "%1"` (executable and notebook both quoted).
**Only `.omatree`** is registered: no SQLite or generic database extension. It does not
write a `UserChoice`, so a program the user chose for `.omatree` themselves is never
overridden or removed. Opening is the existing argv behaviour: one notebook per
process, as on Linux; double-clicking a second notebook opens a second window. No
single-instance mechanism exists or was added. Tested with real Explorer double-clicks
on notebooks with spaces and with Czech letters in their paths; Explorer shows the type
"OmaTree Notebook" and the OmaTree icon.

**Upgrade.** Running a newer `OmaTree-<version>-x64-setup.exe` over an installation (the
same AppId) replaces the installer-owned files in place, rewrites the same registry
entries and the shortcut, and updates the uninstall entry; it never touches a notebook.
A running OmaTree is asked to close through Windows' restart manager, and its own
unsaved-changes question applies. Tested: the same version installed over itself (one
uninstall entry, identical files, association and shortcut intact, notebooks untouched);
a newer version follows the same path.

**Uninstall** removes the installed files, the shortcuts, the uninstall entry, the
`.omatree` keys the installer created and `%LOCALAPPDATA%\OmaTree` (below). It deletes
nothing else: **notebooks are never touched**; they are ordinary files wherever the user
saved them, and nothing is kept inside the installation folder. Tested on the real
system: install, reinstall over it, uninstall, install again, uninstall; three notebooks
(with spaces and Czech characters in their paths) were byte-identical throughout, and an
unrelated user association (`.iss`) was left alone.

**What OmaTree writes outside a notebook** (audited by watching `%APPDATA%`,
`%LOCALAPPDATA%`, `%TEMP%` and `ProgramData` during a session that used every feature):
only Qt's caches in `%LOCALAPPDATA%\OmaTree\cache` (`qmlcache\*.qmlc` and the Qt Quick
D3D11 pipeline cache; a few KB, rebuilt on demand, removed by the uninstaller) and
transient SQLite temporary files in `%TEMP%` that are deleted again. No settings file,
registry value or `%APPDATA%` data.

### Defender, SmartScreen, signing

- **Not signed.** `Get-AuthenticodeSignature` says `NotSigned` for the installer and
  `omatree.exe`. The Windows certificate stores of the build machine hold no usable
  code-signing certificate: only two self-signed, GUID-named test certificates that
  Windows does not trust, which are not used and must not be (they would only fake a
  signature). Signing needs a real Authenticode certificate. The installer is ready for
  it: `ISCC /DSignToolName=<name> /S<name>="<signtool command>"` signs the installer and
  the uninstaller (`SignTool=`/`SignedUninstaller=` in `OmaTree.iss`); signing the
  executable and DLLs would be an extra step before staging. Nothing else changes.
- **Microsoft Defender** scans the installer and the installed executable normally and
  finds nothing (`MpCmdRun -Scan -ScanType 3`, engine 1.1.26080.3, signatures
  1.459.574.0). It was not disabled.
- **SmartScreen.** A file built locally has no download mark and starts without a
  warning. A *downloaded* copy (tested on a copy marked `ZoneId=3`, as a browser
  does) is stopped: **"Windows protected your PC. Microsoft Defender SmartScreen
  prevented an unrecognized app from starting. Running this app might put your PC at
  risk."** with a **More info** link; More info shows *App:
  OmaTree-0.1.1-x64-setup-downloaded.exe*, *Publisher: Unknown publisher*, and the
  buttons **Run anyway** and **Don't run**. So a user can proceed with More info, then
  Run anyway. The warning will repeat for every new unsigned build; a signing
  certificate (and, for the reputation-based check, time and downloads) is what removes
  it. None of this was changed or disabled. For the release notes: the installer is
  unsigned and Windows will warn.

### Clean-machine verification

**A truly clean machine was not available, and this is not claimed.** This Windows 11
*Home* has no Windows Sandbox or Hyper-V, the machine holds the development tools, and
creating a second user account is not something the build may do. What was done instead:

- the app was run from the staged folder and from the installed folder with a **scrubbed
  environment** (`PATH` limited to `System32`, no `QMAKE`/`QTDIR`/Cargo/Visual Studio
  variables) and with the **Qt install folder renamed away**; every loaded module came
  from the application folder or Windows; the C++ runtime DLLs loaded were the app-local
  ones; the dependency audit passed;
- the install, reinstall, uninstall and file association were exercised in the real user
  session.

Not shown: that a machine that has *never* had Qt, Visual Studio or a Visual C++
Redistributable behaves the same. (The app-local runtime and the audit are what argue it
should.) Repeat on a fresh VM or second PC before a release:

    OmaTree-<version>-x64-setup.exe /VERYSILENT /LOG=setup.log   # or the wizard
    # then: launch from Start; double-click a .omatree file (spaces, Czech letters);
    # create, save, reopen; Alt+F4 with an unsaved note; uninstall; check the notebook.

### Testing from an agent or other packaged-app environment

A process started from a *packaged* desktop app (an assistant or terminal that is
itself an MSIX app) can have its writes to `HKCU` redirected to a private copy: the
installer then "succeeds" and the keys are readable from that process, but **Explorer
and Settings never see them**, so the `.omatree` association and the Add/Remove Programs
entry cannot be tested that way. Start the installer from the real shell (the Run
dialog, Explorer, or a normal terminal) and read the registry from a process the shell
started. This is a property of the test environment, not of the installer.

### Linux and macOS

Nothing under `packaging\windows\` or `windows\` is used by the other platforms. Shared
files changed for Windows packaging: `build.rs` (Windows MSVC only, early return
elsewhere), `cpp/app_identity.cpp` (a new function, `Q_OS_WIN` only, a no-op elsewhere)
and `src/main.rs` (one extern declaration and one call that does nothing off Windows).
These need a Linux and macOS build and test run.

## Release builds

    packaging/release-build.sh v0.1.1

checks the tag out into a clean temporary worktree, builds all three packages from
it with the scripts above, and writes `dist/SHA256SUMS` for exactly those three
files. Use it, not the day-to-day builds, for anything that is published. (The
Arch build makes its source tarball from that checkout, with its real SHA-256.
A PKGBUILD for a public source archive, as an AUR package would use, takes the
archive URL and its checksum instead; see `PKGBUILD.in`.)

## Checksums

    packaging/checksums.sh

writes `dist/SHA256SUMS` for exactly the three packages above and verifies it.
(`arch/build-package.sh` writes its own sums file too, with the source tarball;
run `checksums.sh` last.) Nothing is signed yet.

## Verifying a package

    packaging/verify-package.sh dist/omatree-0.1.1-1-x86_64.pkg.tar.zst
    packaging/verify-package.sh dist/omatree_0.1.1_amd64.deb
    packaging/verify-package.sh dist/OmaTree-0.1.1-x86_64.AppImage

picks the verifier by file type (`arch/verify-arch.sh`, `debian/verify-deb.sh`,
`appimage/verify-appimage.sh`); all of them share `packaging/linux/verify-common.sh`
for the desktop entry, AppStream, MIME, icon, build-path and headless-launch
checks. The AppImage verifier also checks the bundled Qt, plugins and QML modules,
the library policy, the licence files, and measures the ABI.

Known validator output: `desktop-file-validate` is clean. `appstreamcli validate`
reports one pedantic note, that developer information is missing (none is
invented to silence it). The AppStream file carries the `0.1.1` release entry (and the `0.1.0` one).

## Release-readiness commands

    cargo fmt --check
    cargo clippy --all-targets
    cargo test                        # unit and integration tests
    cargo test integration_tests      # integration tests only
    cargo build --release

## Not done

AUR, Flatpak, Snap, RPM, ARM Linux, an apt repository, Developer ID signing,
notarization, Authenticode signing, ARM Windows, an MSI/MSIX package, and publishing:
none exists. (The Linux ones
can reuse `packaging/linux/`. macOS is below.)
