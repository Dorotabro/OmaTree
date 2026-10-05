# Third-party software

This is an inventory prepared for binary distribution (Ticket 022). It records
what OmaTree uses and what a later packaging step must do. It is **not** a
statement that OmaTree's distribution is already compliant, and it is not
legal advice. Facts below come from the installed Qt files' SPDX headers and
license texts, `Cargo.lock`/`cargo metadata`, and the crates' own sources, as
of OmaTree 0.1.0 on the development machine (Arch Linux).

OmaTree itself is `MIT OR Apache-2.0` (see `LICENSE-MIT`, `LICENSE-APACHE`).

## Qt

- **Version:** Qt 6.11.2 (Arch packages `qt6-base 6.11.2-3`, `qt6-declarative 6.11.2-1`).
- **Linkage:** dynamic. The release executable (`target/release/omatree`) has
  `NEEDED` entries for `libQt6Core`, `libQt6Gui`, `libQt6Qml` (plus
  `libstdc++`, `libgcc_s`, libm, libc). Qt Quick and the QML modules are
  loaded at run time through the QML engine. Nothing from Qt is statically
  linked or copied into the executable.
- **Modules OmaTree uses.** Directly linked: Core, Gui, Qml. Loaded at run
  time by the QML it imports (`QtQuick`, `QtQuick.Controls` with the Basic
  style, `QtQuick.Layouts`, `QtQuick.Dialogs`): Qt Quick, Quick Templates 2,
  Quick Controls 2 (+ Basic style and Impl), Quick Layouts, Quick Dialogs 2,
  QmlModels, QmlMeta, QmlWorkerScript. These pull in Network, DBus and
  OpenGL. `QtTest` is used only by the test suite and is not distributed.
- **Other libraries seen loaded on this machine** (Widgets, Svg,
  WaylandClient, the GTK3 platform theme, an input-method plugin) come from
  the desktop's Qt platform plugins, not from OmaTree's own dependencies; a
  package must decide separately which Qt platform plugins it ships or
  requires.
- **License of every module above:** the headers of each carry
  `LicenseRef-Qt-Commercial OR LGPL-3.0-only OR GPL-2.0-only OR GPL-3.0-only`,
  so each is available under **LGPL-3.0-only** (the license OmaTree would use;
  its text is in the Qt packages: `/usr/share/licenses/qt6-base/LGPL-3.0-only.txt`).
  Minor exceptions inside these modules are a few files under other permissive
  or LGPL-2.1+ terms (for example Unicode-3.0, BSD-3-Clause), and one QtQml
  *header* file licensed `GPL-3.0-only WITH Qt-GPL-exception-1.0`, which is a
  build-time development header, not part of the shipped libraries.
- **No GPL-only Qt module is used.** (Checked: none of the modules above is
  GPL-only; GPL-only Qt add-ons such as Charts or Virtual Keyboard are not used.)

### What a later packaging step must satisfy (Ticket 023)

These follow from the LGPL-3.0 text and Qt's open-source licensing; verify
them against the official Qt licensing documentation for the exact release
when packaging. Writing them down here does not satisfy them.

- Provide the LGPL-3.0 license text (and the GPL-3.0 text it builds on) with
  the distribution, and say that OmaTree uses Qt, with a pointer to the
  license, in a place users can find (for example an About/credits text or
  the package documentation).
- Do not restrict recipients' rights under the LGPL: keep OmaTree's own
  license terms compatible, and do not prohibit modification of the Qt
  libraries for the recipient's own use, or reverse engineering to debug such
  modifications.
- Keep Qt replaceable: link dynamically (as now), ship or depend on separate
  Qt shared libraries, and do not use a packaging format that prevents a user
  from running OmaTree against their own build of Qt where the license
  requires it. A distro package depending on the distro's Qt does this
  naturally; a self-contained bundle, AppImage or Flatpak needs checking.
- Provide the corresponding source for any Qt libraries OmaTree itself
  distributes (written offer or a link to the exact source version), and keep
  any local patches to Qt available. Not needed if Qt is only a dependency
  installed from the distro.
- Do not use Qt's trademarks beyond acknowledging Qt.

### Qt in the AppImage (Ticket 032)

The Arch and Debian/Ubuntu packages use the system's Qt and contain none. The
**AppImage is the one format that redistributes Qt**, so for it the obligations
above apply, and are met as follows. This restates facts; it adds no legal
conclusion beyond the audit above.

- **Qt version and origin:** Qt 6.8.3, the official unmodified binary release
  from <https://download.qt.io/> (installed with a pinned `aqtinstall`), bundled
  with ICU 73.2 (Unicode license), which that Qt build links. Qt has no local
  patches.
- **Modules and plugins bundled:** listed in the AppImage's
  `usr/share/doc/omatree/QT-NOTICE.md` (source: `packaging/appimage/QT-NOTICE.md`).
  The AppImage build checks that every bundled `libQt6*` module is named there.
- **Dynamic linking:** unchanged. `omatree` links `libQt6Core`, `libQt6Gui` and
  `libQt6Qml` as shared libraries; nothing is static.
- **Licence texts:** `Qt-LGPL-3.0.txt`, `Qt-GPL-3.0.txt`, `ICU-73.2-LICENSE.txt`,
  and the copyright files of the other libraries copied from the build system
  (`third-party/`, with `BUNDLED-LIBRARIES.txt`), in `usr/share/licenses/omatree/`.
- **Replaceability and source:** the AppImage can be unpacked
  (`--appimage-extract`) and its Qt libraries replaced; the exact Qt source is at
  `https://download.qt.io/archive/qt/6.8/6.8.3/`. Both are explained in
  `QT-NOTICE.md`.

## Rust dependencies

Direct dependencies (all have machine-readable license metadata):

| Crate | Version | License | Notes |
|---|---|---|---|
| cxx | 1.0.202 | MIT OR Apache-2.0 | C++/Rust bridge |
| cxx-qt | 0.10.0 | MIT OR Apache-2.0 | |
| cxx-qt-lib | 0.10.0 | MIT OR Apache-2.0 | |
| cxx-qt-build (build only) | 0.10.0 | MIT OR Apache-2.0 | |
| rusqlite | 0.40.2 | MIT | brings `libsqlite3-sys` |
| toml | 1.1.6 | MIT OR Apache-2.0 | |

All 82 crates in `Cargo.lock` (excluding OmaTree) carry license metadata; none
is missing one, none is GPL/AGPL-only, and none is copyleft-only. Transitive
licenses present: MIT OR Apache-2.0 (61), MIT (8), `MIT/Apache-2.0` (3, the
old spelling), Unlicense OR MIT (3), Apache-2.0 OR MIT (2), Apache-2.0 (2),
Zlib (1), `MIT OR Apache-2.0 OR LGPL-2.1-or-later` (1), and
`(MIT OR Apache-2.0) AND Unicode-3.0` (1). Items worth a note:

- `codespan-reporting` (Apache-2.0 only; two versions): used by the C++
  bridge code generator at build time, not part of the executable.
- `foldhash` (Zlib): linked into the executable (through `hashbrown`, from
  `rusqlite`). The Zlib license asks for no attribution in binaries.
- `r-efi` (MIT OR Apache-2.0 OR LGPL-2.1-or-later): an alternative, the
  permissive options apply; it is not used on Linux.
- `unicode-ident` (`... AND Unicode-3.0`): a compile-time (proc-macro)
  dependency, not in the executable.
- The old `MIT/Apache-2.0` spelling in `vcpkg` (a build helper) is only a
  metadata spelling.

No crate in the dependency set ships a `NOTICE` file.

## SQLite

`rusqlite` is built with the `bundled` feature, so SQLite 3.53.2 (the
amalgamation inside `libsqlite3-sys 0.38.2`) is compiled into the OmaTree
executable. SQLite is in the public domain: its source begins with "The author
disclaims copyright to this source code" and a blessing in place of a legal
notice. No attribution or license text is required by SQLite, and none is
invented here. (`libsqlite3-sys` the Rust wrapper is MIT; the optional
SQLCipher and OpenSSL paths of that crate are not enabled.)

## NOTICE file

**Not required, none created.** OmaTree is Apache-2.0 *or* MIT at the
recipient's option, which does not in itself require a NOTICE, and no
dependency that would be distributed provides one.

## Distribution checklist

- [x] LGPL-3.0 text and Qt acknowledgement included (AppImage: `QT-NOTICE.md`
      and the licence texts; Arch and Debian ship no Qt).
- [x] The Qt libraries stay replaceable in the chosen package format (system Qt
      for Arch and Debian; an unpackable AppImage).
- [x] Source/offer for any Qt libraries that the package itself ships (AppImage:
      the `download.qt.io` link in `QT-NOTICE.md`).
- [x] Include `LICENSE-MIT`, `LICENSE-APACHE` and this file (Debian: in
      `/usr/share/doc/omatree/`, with `copyright`).
- [ ] Re-run the inventory (`cargo metadata`, `readelf -d`) for the release
      commit and the Qt version actually packaged.
