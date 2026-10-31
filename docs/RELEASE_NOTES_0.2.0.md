<!--
DRAFT release notes for the GitHub release v0.2.0 (prerelease). Nothing here says the release exists: it is not
published, and the tag does not exist yet. Links use the v0.2.0 tag, which must exist before the release is published.
Before publishing: replace nothing but check the file list and the SHA-256 note against the files actually attached.
-->

# OmaTree egui 0.2.0 (prerelease)

**This is a prerelease of the Rust + egui version of OmaTree, for Linux x86_64.** It is a separate release next to
the Qt version: **Qt [v0.1.2](https://github.com/Dorotabro/OmaTree/releases/tag/v0.1.2) is still available** and remains the
release for Windows and macOS. Installing the egui version does not remove or replace it.

## What it is

OmaTree is a deliberately simple, local-first notebook: a tree of notes on the left, the selected note on the right. Every
node is a note and can have child notes. A notebook is one `.omatree` file (a SQLite database) on your disk. No account, no cloud
service, no telemetry, no network access.

This version is written in Rust with [egui](https://github.com/emilk/egui). There is no Electron, web view or Qt.

## Differences from Qt v0.1.2

- **Live Markdown instead of Preview.** You edit the Markdown source and it is rendered as you type; the raw syntax shows where
  the caret is. There is no Edit/Preview toggle, and `Ctrl+E` is inline code.
- **Styled tables**, with bold, italic, code, strikethrough and links inside cells.
- **Images stored inside the notebook** (paste from the clipboard); nothing is fetched or read from outside the notebook.
- **Colour tags** (six colours) from the tree menu or a picker in the note header, with a hint on collapsed branches.
- **Export** of a note to Markdown, a folder with images, a ZIP or a PDF; **Compact Notebook** to remove unused images.
- The tree, search (always visible at the top of the tree), Trash and checkpoint recovery, autosave and Omarchy theme
  following carry over.
- Notebook format: **schema versions 4 and 5** (below).

## Highlights

Tree-structured notes with keyboard and drag-and-drop; source-backed live Markdown; styled tables; clipboard images in the
notebook; colour tagging; search; Trash and recovery; Markdown, ZIP and PDF export; automatic Omarchy theme integration
(system light/dark elsewhere); keyboard-first (`F1` lists every shortcut).

## Platforms

**Linux x86_64 only** in this release:

| File | For |
|---|---|
| `omatree-egui-0.2.0-1-x86_64.pkg.tar.zst` | Arch Linux / Omarchy |
| `omatree-egui_0.2.0_amd64.deb` | Debian 13+, Ubuntu 24.04+ (needs glibc 2.39 or newer) |
| `OmaTree-egui-0.2.0-x86_64.AppImage` | other x86_64 Linux with glibc 2.30+ and a Wayland or X11 desktop |
| `SHA256SUMS` | SHA-256 of the three files |

**There are no Windows or macOS packages of the egui version.** Use Qt v0.1.2 on those systems.

## Installing

Check the download first: `sha256sum --check --ignore-missing SHA256SUMS`.

```bash
# Arch / Omarchy
sudo pacman -U omatree-egui-0.2.0-1-x86_64.pkg.tar.zst
# Debian 13+ / Ubuntu 24.04+
sudo apt install ./omatree-egui_0.2.0_amd64.deb
# Any x86_64 Linux (glibc 2.30+)
chmod +x OmaTree-egui-0.2.0-x86_64.AppImage && ./OmaTree-egui-0.2.0-x86_64.AppImage [notebook.omatree]
```

The packages are named `omatree-egui` and share no file with the Qt package `omatree`; both can be installed. The launcher then
shows "OmaTree" (Qt) and "OmaTree (egui)". Remove with `sudo pacman -R omatree-egui` or `sudo apt remove omatree-egui`.
The AppImage does not install itself. Packages are **unsigned**; use the checksums.

## Notebook compatibility: read before opening a notebook

OmaTree writes the oldest format a notebook can do with and raises it only when a save needs it:

| Format | Written when | Qt v0.1.2 |
|---|---|---|
| schema 3 | no image and no colour tag | opens it |
| schema 4 | an image is saved | **refuses it** (file unchanged) |
| schema 5 | a colour tag is saved | **refuses it** (file unchanged) |

A notebook is never lowered again. **Keep a copy of any notebook you still need to open with Qt v0.1.2.**

## Known limitations

- Linux x86_64 only; native X11, screen readers, input methods and Hi-DPI are not verified; only Omarchy/Hyprland (Wayland)
  and containers were tested.
- The `.deb` does not install on Debian 12 or Ubuntu 22.04 (glibc); use the AppImage there. The AppImage bundles no graphics
  libraries, so the host must provide Wayland or X11, GL, fontconfig and D-Bus.
- No single-instance handling: opening a notebook from a file manager starts a new window. The application does not display its
  version.
- Without a desktop file-dialog portal, Open and Save show a "type a path" field instead.
- Table columns are sized by their widest cell, links inside table cells are styled but not clickable, and the PDF and Markdown
  exports do not render inline Markdown in cells.
- Packages are unsigned and there is no automatic update.

## Building from source

```bash
git clone https://github.com/Dorotabro/OmaTree.git && cd OmaTree && git checkout v0.2.0
cargo build --locked --release -p omatree-egui     # no Qt needed
./target/release/omatree-egui
```

## Documentation and licence

[README](https://github.com/Dorotabro/OmaTree/blob/v0.2.0/README.md) ·
[VISION](https://github.com/Dorotabro/OmaTree/blob/v0.2.0/VISION.md) ·
[ARCHITECTURE](https://github.com/Dorotabro/OmaTree/blob/v0.2.0/ARCHITECTURE.md) ·
[Linux packaging](https://github.com/Dorotabro/OmaTree/blob/v0.2.0/docs/PACKAGING_EGUI.md) ·
[third-party notices](https://github.com/Dorotabro/OmaTree/blob/v0.2.0/THIRD_PARTY.md).
Licensed under either the Apache License 2.0 or the MIT licence, at your option. Copyright (c) 2026 OmaTree contributors.
