
# OmaTree

OmaTree is a simple tree-structured note-taking application.

Each node in the tree is a note, and every note can have child notes.

The goal is to keep the interface deliberately minimal: a tree on the left,
text on the right, and very little else.

OmaTree is being built with Omarchy in mind, but is intended to remain
cross-platform.

## Status

OmaTree is currently in early development.

It already supports:

- hierarchical notes
- plain-text note editing
- drag-and-drop reordering and reparenting
- SQLite notebook files
- Open, Save and Save As
- debounced autosave
- persistent Trash
- recovery checkpoints
- recovery after destructive tree operations

The UI and file format are still evolving.

Do not use the current development version as the only copy of important data.

## Install

Binary package, for **Omarchy / Arch Linux x86_64** only (it uses the system
Qt 6):

    sudo pacman -U ./omatree-0.1.0-1-x86_64.pkg.tar.zst

Remove it with:

    sudo pacman -R omatree

`.omatree` files become associated with OmaTree after the usual desktop and MIME
database processing that pacman's hooks perform.

To build from source, see [Technology](#technology) and [Testing](#testing):
`cargo build --release` needs Qt 6 (base and declarative) and a Rust toolchain.
The package itself is built with `packaging/arch/build-package.sh`; see
[`docs/PACKAGING.md`](docs/PACKAGING.md). There are no packages for other
platforms yet.

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
- `tickets/`

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

Development currently happens primarily on Linux / Omarchy.

Windows and macOS support are intended, but are not yet a development priority.

## License

OmaTree is licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.

Third-party components are listed in [THIRD_PARTY.md](THIRD_PARTY.md).
