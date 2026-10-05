
# OmaTree

**OmaTree is a local-first notebook for people who think in trees.**

A tree of notes on the left, the selected note on the right, and very little
else. Every node in the tree is itself a note, and any note can have child notes,
nested as deeply as you like. There are no folders or note types to think about:
you write, press Ctrl+Shift+N, and the idea you are following gets its own note
right under the one that spawned it.

## What it does

- **Notes in a tree.** Create, rename, nest, reorder and move notes, with the
  mouse (drag and drop) or the keyboard (Alt+Up / Alt+Down).
- **Markdown.** Notes are plain text; Ctrl+E switches between editing and a
  read-only Markdown Preview. Preview never loads images or other resources,
  local or remote.
- **Local search.** Ctrl+F searches titles and text of the open notebook.
- **Keyboard first.** The whole application can be driven from the keyboard;
  F1 lists the shortcuts.
- **Safe by default.** Autosave is silent. Deleted notes go to a persistent
  Trash, and larger changes take recovery checkpoints, so a bad drag or an
  accidental delete can be undone.
- **One notebook, one file.** A notebook is a single `.omatree` file on your disk
  (a SQLite database). No account, no cloud service, no network access, no
  telemetry.
- **Fits in.** On Omarchy, OmaTree follows the current theme live; elsewhere it
  follows your system's light or dark setting.

## What it deliberately is not

OmaTree is small on purpose. It has no sync, collaboration, plugins, graph view,
AI features or rich-text editing, and it is not meant to grow into a
knowledge-management platform. See [`VISION.md`](VISION.md).

## Status

This is the first public release, **v0.1.0**. It is early software: the
notebook format is versioned but may still change between releases, and you
should keep backups of notebooks that matter to you.

## Install

Release downloads are on the
[GitHub Releases page](https://github.com/Dorotabro/OmaTree/releases). Every file
is listed in `SHA256SUMS`.

| System | Package | Qt |
|---|---|---|
| Omarchy / Arch Linux | native package | system |
| Debian 13+, recent Ubuntu | native `.deb` | system (Qt 6.8 or newer) |
| Other x86_64 Linux | AppImage | bundled (6.8.3) |
| macOS | planned | |
| Windows | planned | |

There are no macOS or Windows builds yet, and no dates are promised.

### Omarchy / Arch Linux

A native package; it uses the system Qt 6:

    sudo pacman -U ./omatree-0.1.0-1-x86_64.pkg.tar.zst

Remove it with `sudo pacman -R omatree`.

### Debian / Ubuntu

A native package for amd64 that uses the system Qt 6. OmaTree needs a newer Qt
than Debian 12 and Ubuntu 22.04/24.04 provide (Qt 6.2 and 6.4), so the `.deb` is
supported only where it has been tested: **Debian 13**, **Ubuntu 25.10** and
**Ubuntu 26.04**. Use `apt`, not `dpkg -i`, so that dependencies are installed:

    sudo apt install ./omatree_0.1.0_amd64.deb

Remove it with `sudo apt purge omatree`.

On Debian 12, Ubuntu 22.04 or 24.04, and other systems the `.deb` does not
support, use the AppImage below.

### Other x86_64 Linux: AppImage

A self-contained build that brings its own Qt, so it does not depend on your
system's Qt version:

    chmod +x OmaTree-0.1.0-x86_64.AppImage
    ./OmaTree-0.1.0-x86_64.AppImage [notebook.omatree]

- The measured binary requirement is **glibc 2.30 or newer** (and libstdc++ from
  GCC 5 or newer), on x86_64.
- It also needs the usual libraries of a Linux desktop: OpenGL/EGL, the X11 or
  Wayland client libraries, fontconfig, FreeType and libxkbcommon.
- Tested on Ubuntu 20.04, Debian 11, 12 and 13, and Ubuntu 24.04 and 26.04
  (headless), and on Arch Linux with real Wayland and X11 (XWayland) sessions.
- Without FUSE, run it with `APPIMAGE_EXTRACT_AND_RUN=1`.

Running the file works as it is; the AppImage does not install itself. A menu
entry and the `.omatree` file association need your desktop's AppImage
integration tool (the AppImage contains the desktop, icon and MIME files such a
tool uses). It can be used alongside a native package.

## Build from source

You need a Rust toolchain and Qt 6 (base and declarative modules, with Qt Quick
Controls, Layouts and Dialogs) newer than 6.4; Qt 6.8 and 6.11 are tested.
On Arch: `qt6-base` and `qt6-declarative`.

    cargo build --release
    ./target/release/omatree [notebook.omatree]

How the three packages are built and verified is in
[`docs/PACKAGING.md`](docs/PACKAGING.md).

## Technology

OmaTree is built with:

- Rust
- Qt 6
- Qt Quick / QML
- CXX-Qt
- SQLite

Rust owns the application logic and data model. QML handles presentation and
interaction.

## Notebook files

An OmaTree notebook is stored as a single SQLite file, conventionally using:

```text
.omatree
```

For example:

```text
notes.omatree
```

Notebook files contain the note tree, note contents, Trash and recovery
history.

## Running

Build and run:

```bash
cargo run
```

Open an existing notebook:

```bash
cargo run -- ~/Documents/notes.omatree
```

Or run the compiled binary directly:

```bash
./target/debug/omatree ~/Documents/notes.omatree
```

Running OmaTree without a path starts a new untitled notebook.

## Development

Before submitting changes, run:

```bash
cargo fmt --check
cargo clippy --all-targets
cargo test
cargo build
```


Project design and development rules are documented in:

- `VISION.md`
- `ARCHITECTURE.md`
- `AGENTS.md`
- `CONTRIBUTING.md`

## Design philosophy

OmaTree is intentionally small.

It is not intended to become a knowledge-management platform, collaboration
suite, plugin ecosystem, graph database, or productivity operating system.

It is a tree of notes.

That should remain easy to understand.

## Testing

    cargo test                       # everything: unit tests and integration tests
    cargo test integration_tests     # only the integration tests (about 10 s)

The integration tests run the real application, headless, in separate
processes on Qt's `offscreen` platform (no desktop session needed, only the
Qt 6 Quick and QtTest QML modules and the usual Qt platform plugins). Each
scenario in `tests/qml/` drives the production `main.qml` with QtTest and
checks workflows that cross Rust, the Qt models and QML: saving and
reopening, autosave, New/Open, Search, expansion state, Trash and Recovery,
Markdown Preview (including that it loads no resources), live theme
switching and a keyboard/dialog smoke test. They use only temporary
directories (their own HOME and XDG state, never yours), contact only
`127.0.0.1`, and fail on any QML warning or a scenario that hangs. See
`src/integration_tests.rs` for how they are run and `tests/qml/Base.qml` for
the helpers; a new scenario is one new QML file plus a short test function.

## Platform support

    Linux
      Arch / Omarchy              native package
      Debian 13+, Ubuntu 25.10+   native .deb (tested: Debian 13, Ubuntu 25.10, 26.04)
      other x86_64 Linux          AppImage (glibc 2.30+)

    macOS                         planned
    Windows                       planned

Development happens primarily on Arch Linux / Omarchy. macOS and Windows are
intended eventually, and the stack (Rust, Qt 6) is cross-platform, but nothing is
built or tested there yet, so they are not supported.

## License

OmaTree is licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

Third-party components are listed in [THIRD_PARTY.md](THIRD_PARTY.md).
