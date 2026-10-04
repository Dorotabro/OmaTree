# Packaging

How OmaTree is packaged for Linux, and what a release still needs. The first
binary target is **Omarchy / Arch Linux x86_64**; the desktop metadata and the
install layout are plain freedesktop files and are reusable by other formats.

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

The Arch package uses the **system Qt** and does **not** contain Qt: no Qt
shared library is copied into it, so it redistributes no Qt binaries. See
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

**For the public release (Ticket 030):** the template's `source` must become the
tagged GitHub archive URL and `sha256sums` its real checksum (never `SKIP`).
The metainfo file also needs its first `<release>` (version and date).

## Verifying a package

    packaging/verify-package.sh dist/omatree-0.1.0-1-x86_64.pkg.tar.zst

extracts the package to a temporary directory (nothing is installed) and checks
the file list, that no Qt library is inside, the dynamic links, the absence of
build paths and of the test hook, `desktop-file-validate`, `appstreamcli
validate`, the MIME XML and a temporary MIME database (the glob is present and
no SQLite magic), every icon's dimensions, SVG self-containedness, and a headless
launch of the packaged binary with `%f`, requiring no QML warnings.

Known validator output: `desktop-file-validate` hints that `Office;Utility;` has
two main categories; AppStream reports pedantic notes for the mixed-case
component id (it is the fixed application id), missing release information
(Ticket 030) and missing developer information.

## Release-readiness commands

    cargo fmt --check
    cargo clippy --all-targets
    cargo test                        # unit and integration tests
    cargo test integration_tests      # integration tests only
    cargo build --release

## Future targets

Not done: AUR, Flatpak, AppImage, Snap, Debian/RPM, macOS, Windows. Each can
reuse `packaging/linux/` and `stage.sh`.
