# Qt in the OmaTree AppImage

Unlike the Arch and Debian packages of OmaTree, which use the Qt of your
system, **this AppImage contains Qt**. This file says what, under which
licence, and how to get the source. (It restates the facts in
`THIRD_PARTY.md`; it is not legal advice.)

## What is bundled

- **Qt 6.8.3**, the official, unmodified binary release published by the Qt
  Project and The Qt Company (<https://www.qt.io/>, <https://download.qt.io/>),
  built for Linux x86_64 (gcc).
- **Modules** (the files `usr/lib/libQt6*.so.6`): Core, DBus, Gui, Network,
  OpenGL, Qml, QmlMeta, QmlModels, QmlWorkerScript, Quick, QuickControls2,
  QuickControls2Basic, QuickControls2BasicStyleImpl, QuickControls2Impl,
  QuickDialogs2, QuickDialogs2QuickImpl, QuickDialogs2Utils, QuickLayouts,
  QuickTemplates2, LabsFolderListModel, WaylandClient,
  WaylandEglClientHwIntegration, XcbQpa.
- **Plugins** (`usr/plugins/`): platforms `xcb`, `wayland-generic`,
  `wayland-egl` and `offscreen`; the Wayland shell, decoration and graphics
  integration plugins; input-method plugins (`compose`, `ibus`); the XDG
  desktop portal platform theme; the XCB GL integrations.
- **QML modules** (`usr/qml/`): QtQuick, QtQuick.Controls (Basic style only),
  QtQuick.Templates, QtQuick.Layouts, QtQuick.Dialogs, QtQuick.Window,
  QtQml, QtQml.Models, QtQml.WorkerScript, Qt.labs.folderlistmodel.
- **ICU 73.2** (`libicu*.so.73`), which that Qt build is linked against.

## Licence

These Qt modules are available under the **GNU Lesser General Public License,
version 3 (LGPL-3.0-only)**, and OmaTree uses them under it. Copyright (C) The
Qt Company Ltd. and other contributors. Qt and the Qt logo are trademarks of
The Qt Company Ltd.

The licence texts are in `usr/share/licenses/omatree/`:

- `Qt-LGPL-3.0.txt` and `Qt-GPL-3.0.txt` (the LGPL is an addition to the GPL)
- `ICU-73.2-LICENSE.txt`
- `third-party/`: the copyright files of the other libraries that were copied
  into the AppImage from the build system, and `BUNDLED-LIBRARIES.txt`, which
  lists them with their versions.

OmaTree itself is `MIT OR Apache-2.0` (`LICENSE-MIT`, `LICENSE-APACHE` in the
same directory).

## Dynamic linking

OmaTree is **dynamically linked** to Qt. The `omatree` executable only
references the shared libraries `libQt6Core`, `libQt6Gui` and `libQt6Qml`; Qt
is not compiled into it and is not modified. The AppImage is a plain read-only
filesystem image, so you can replace Qt with your own build:

    ./OmaTree-0.1.0-x86_64.AppImage --appimage-extract    # writes ./squashfs-root
    # replace the libQt6*.so.6 files in squashfs-root/usr/lib (and plugins/QML
    # modules if needed) with your own build of a compatible Qt 6.8, then run
    ./squashfs-root/AppRun

## Corresponding Qt source

The Qt source code for this exact version is available from the Qt Project:

- <https://download.qt.io/archive/qt/6.8/6.8.3/single/qt-everywhere-src-6.8.3.tar.xz>
  (everything), or per module under
  <https://download.qt.io/archive/qt/6.8/6.8.3/submodules/>.
- ICU 73.2: <https://github.com/unicode-org/icu/releases/tag/release-73-2>

The other bundled libraries are packages of Ubuntu 20.04 (versions in
`BUNDLED-LIBRARIES.txt`); their source is available from
<https://archive.ubuntu.com/ubuntu/> and <https://launchpad.net/ubuntu>.
