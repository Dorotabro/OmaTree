# OmaTree

**OmaTree is a local-first tree-structured note-taking app, inspired by CherryTree and designed with Omarchy in mind.**

Notes live in a simple hierarchy: every node is a note, and every note can have child notes nested as deeply as needed. The interface stays deliberately minimal, with the tree on the left and the selected note on the right. A notebook is stored locally as a single `.omatree` file.

## What it does

- **Notes in a tree.** Create, rename, nest, reorder and move notes with the mouse or keyboard.
- **Markdown.** Notes are plain text; `Ctrl+E` switches between editing and a read-only Markdown Preview. Preview never loads images or other resources, local or remote.
- **Local search.** `Ctrl+F` searches titles and note text in the open notebook.
- **Keyboard-first workflow.** The whole application can be driven from the keyboard; `F1` lists the shortcuts.
- **Autosave and recovery.** Deleted notes go to a persistent Trash, and structural changes create recovery checkpoints.
- **One notebook, one file.** Each notebook is a single local `.omatree` SQLite file. No account, cloud service, telemetry or network access is required.
- **Theme integration.** On Omarchy, OmaTree follows the current theme live. Elsewhere it follows the system light/dark preference.

## What it deliberately is not

OmaTree is small on purpose. It has no sync, collaboration, plugins, graph view, AI features or rich-text editing, and it is not intended to grow into a general knowledge-management platform.

See [`VISION.md`](VISION.md) for the design philosophy.

## Status

This is the first public release, **v0.1.0**.

The notebook format is versioned but may still change between releases, so keep backups of notebooks that matter to you.

## Install

Release downloads are available from the [GitHub Releases page](https://github.com/Dorotabro/OmaTree/releases). Each release also includes `SHA256SUMS`.

| System | Package | Qt |
|---|---|---|
| Omarchy / Arch Linux | native package | system |
| Debian 13+, recent Ubuntu | native `.deb` | system Qt 6.8+ |
| Other x86_64 Linux | AppImage | bundled Qt 6.8.3 |
| macOS | planned | |
| Windows | planned | |

There are no macOS or Windows builds yet, and no release dates are promised.

### Omarchy / Arch Linux

The native package uses the system Qt 6:

```bash
sudo pacman -U ./omatree-0.1.0-1-x86_64.pkg.tar.zst
```

Remove it with:

```bash
sudo pacman -R omatree
```

### Debian / Ubuntu

The native amd64 package uses the system Qt 6.

OmaTree needs a newer Qt than Debian 12 and Ubuntu 22.04/24.04 provide, so the `.deb` is currently supported where it has been tested:

- Debian 13
- Ubuntu 25.10
- Ubuntu 26.04

Install with `apt` so dependencies are resolved automatically:

```bash
sudo apt install ./omatree_0.1.0_amd64.deb
```

Remove it with:

```bash
sudo apt purge omatree
```

On Debian 12, Ubuntu 22.04 or Ubuntu 24.04, use the AppImage instead.

### Other x86_64 Linux: AppImage

The AppImage bundles its own Qt runtime, so it does not depend on your system Qt version:

```bash
chmod +x OmaTree-0.1.0-x86_64.AppImage
./OmaTree-0.1.0-x86_64.AppImage
```

You can also pass a `.omatree` file path to open it directly.

The AppImage has a measured **glibc 2.30+** requirement on x86_64 and needs the usual Linux desktop libraries such as OpenGL/EGL, X11 or Wayland client libraries, fontconfig, FreeType and libxkbcommon.

It has been tested on:

- Ubuntu 20.04
- Debian 11, 12 and 13
- Ubuntu 24.04 and 26.04
- Arch Linux on real Wayland and X11/XWayland sessions

Without FUSE, run it with:

```bash
APPIMAGE_EXTRACT_AND_RUN=1 ./OmaTree-0.1.0-x86_64.AppImage
```

The AppImage does not install itself. Desktop-menu and `.omatree` file association integration depends on your desktop or AppImage integration tool. The AppImage contains the desktop, icon and MIME metadata those tools can use.

## Build from source

You need:

- a Rust toolchain
- Qt 6 Base
- Qt 6 Declarative
- Qt Quick Controls, Layouts and Dialogs

Qt **6.8 and 6.11 are tested**. Qt 6.4 is too old for the current UI.

On Arch Linux:

```bash
sudo pacman -S qt6-base qt6-declarative
```

Build and run:

```bash
cargo build --release
./target/release/omatree
```

You can optionally pass a `.omatree` file path to open an existing notebook directly.

For development:

```bash
cargo run
```

How the release packages are built and verified is documented in [`docs/PACKAGING.md`](docs/PACKAGING.md).

## Notebook files

An OmaTree notebook is stored as a single SQLite file with the `.omatree` extension, for example:

```text
notes.omatree
```

Notebook files contain the note tree, note contents, Trash and recovery history.

Running OmaTree without a path starts a new untitled notebook.

## Technology

OmaTree is built with:

- Rust
- Qt 6
- Qt Quick / QML
- CXX-Qt
- SQLite

Rust owns the application logic and data model. QML handles presentation and interaction.

## Development

Before submitting changes, run:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
cargo build
```

Project design and development rules are documented in:

- [`VISION.md`](VISION.md)
- [`ARCHITECTURE.md`](ARCHITECTURE.md)
- [`AGENTS.md`](AGENTS.md)
- [`CONTRIBUTING.md`](CONTRIBUTING.md)

## Testing

Run the full suite:

```bash
cargo test
```

Run only the permanent integration scenarios:

```bash
cargo test integration_tests
```

The integration tests run the real application headlessly in separate processes using Qt's `offscreen` platform. They cover workflows across Rust, Qt models and QML, including saving and reopening, autosave, New/Open, Search, expansion state, Trash and Recovery, Markdown Preview resource blocking, theme switching, keyboard navigation and dialogs.

The tests use temporary directories, contact only `127.0.0.1`, and fail on QML warnings or hung scenarios.

See `src/integration_tests.rs` and `tests/qml/` for the harness and scenarios.

## Platform support

```text
Linux
  Arch / Omarchy              native package
  Debian 13+, Ubuntu 25.10+   native .deb
  other x86_64 Linux          AppImage (glibc 2.30+)

macOS                         planned
Windows                       planned
```

Development happens primarily on Arch Linux / Omarchy.

macOS and Windows are intended eventually, and the stack is cross-platform, but no builds are currently produced or tested there.

## Design philosophy

OmaTree is intentionally small.

It is not intended to become a knowledge-management platform, collaboration suite, plugin ecosystem, graph database or productivity operating system.

It is a tree of notes.

That should remain easy to understand.

## License

OmaTree is licensed under either of:

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

Third-party components are listed in [THIRD_PARTY.md](THIRD_PARTY.md).
