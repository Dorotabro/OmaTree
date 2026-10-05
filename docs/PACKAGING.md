# Packaging

How OmaTree is packaged for Linux, and what a release still needs. There are
three formats, built from the same sources and the same desktop metadata:

| Format | File | Qt | Built in |
|---|---|---|---|
| Arch / Omarchy | `omatree-0.1.0-1-x86_64.pkg.tar.zst` | system | the host (`makepkg`) |
| Debian / Ubuntu | `omatree_0.1.0_amd64.deb` | system | Debian 13 container |
| AppImage | `OmaTree-0.1.0-x86_64.AppImage` | **bundled** (6.8.3) | Ubuntu 20.04 container |

All three are written to `dist/` and listed in `dist/SHA256SUMS`. Nothing is
published, tagged or uploaded by any of these scripts.

## Identity

| Item | Value |
|---|---|
| Display name | OmaTree |
| Executable | `omatree` |
| Version | 0.1.0 |
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

writes `dist/omatree-0.1.0-1-x86_64.pkg.tar.zst`, a source snapshot
`dist/omatree-0.1.0.tar.gz` and `dist/SHA256SUMS` (`dist/` is git-ignored).
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

builds `dist/omatree_0.1.0_amd64.deb` with the normal Debian tooling
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

    packaging/debian/test-install.sh dist/omatree_0.1.0_amd64.deb debian:13
    packaging/debian/test-install.sh dist/omatree_0.1.0_amd64.deb ubuntu:26.04

`apt install` with `--no-install-recommends` (the Depends field alone must
suffice), `dpkg -s`/`dpkg -L`, a headless run on a new notebook with no QML or
plugin output, then `apt purge` and a check that no package file remains.
Passing: Debian 13, Ubuntu 25.10, Ubuntu 26.04.

## AppImage

    packaging/appimage/build-package.sh

builds `dist/OmaTree-0.1.0-x86_64.AppImage` in a disposable container
(`packaging/appimage/Dockerfile`, digest-pinned). It builds OmaTree against an
official **Qt 6.8.3** and bundles that Qt with linuxdeploy; `Exec=omatree %f`
means `./OmaTree-0.1.0-x86_64.AppImage notebook.omatree` opens the notebook.

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

    packaging/appimage/test-container.sh dist/OmaTree-0.1.0-x86_64.AppImage ubuntu:20.04

Step one runs it in the bare image (which prints the host libraries it needs),
step two installs the distribution's usual desktop libraries and requires a clean
headless run with extract-and-run. Passing: Ubuntu 20.04, Debian 11, 12 and 13,
Ubuntu 24.04 and 26.04.

## Release builds

    packaging/release-build.sh v0.1.0

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

    packaging/verify-package.sh dist/omatree-0.1.0-1-x86_64.pkg.tar.zst
    packaging/verify-package.sh dist/omatree_0.1.0_amd64.deb
    packaging/verify-package.sh dist/OmaTree-0.1.0-x86_64.AppImage

picks the verifier by file type (`arch/verify-arch.sh`, `debian/verify-deb.sh`,
`appimage/verify-appimage.sh`); all of them share `packaging/linux/verify-common.sh`
for the desktop entry, AppStream, MIME, icon, build-path and headless-launch
checks. The AppImage verifier also checks the bundled Qt, plugins and QML modules,
the library policy, the licence files, and measures the ABI.

Known validator output: `desktop-file-validate` is clean. `appstreamcli validate`
reports one pedantic note, that developer information is missing (none is
invented to silence it). The AppStream file carries the `0.1.0` release entry.

## Release-readiness commands

    cargo fmt --check
    cargo clippy --all-targets
    cargo test                        # unit and integration tests
    cargo test integration_tests      # integration tests only
    cargo build --release

## Not done

AUR, Flatpak, Snap, RPM, ARM, macOS and Windows packages, an apt repository,
signing, and publishing: none exists. Each can reuse `packaging/linux/`.
