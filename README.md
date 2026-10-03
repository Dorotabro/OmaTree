
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

## Platform support

Development currently happens primarily on Linux / Omarchy.

Windows and macOS support are intended, but are not yet a development priority.

## License

License not yet decided.
