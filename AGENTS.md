# OmaTree Agent Instructions

OmaTree is a deliberately simple hierarchical note-taking application.

The core interaction is:

- a tree of nodes on the left
- the selected node's note on the right
- every node is itself a note
- nodes may contain child nodes

The application must remain simple, fast, local-first, and pleasant to use.

## Migration status (read first)

The maintainer has approved replacing the Qt/QML frontend with **Rust + egui/eframe**
(see `docs/EGUI_DECISIONS.md` and `docs/EGUI_MIGRATION_PLAN.md`). The migration is in
progress on the `egui-integration` branch and **is mostly not implemented yet**.

- **Current implementation:** the Qt 6 / QML / CXX-Qt application in this repository.
  It must stay buildable, tested and functional until a separate, explicit cut-over
  decision (ticket E13). Do not remove or break it.
- **Target:** an egui/eframe application sharing the Qt-free Rust core and an egui
  Markdown editor library.

When a rule below is marked *Qt (legacy)* it applies to the existing Qt code only.
Rules marked *Target* apply to new egui work. Unmarked rules apply to both.

## Approved technology

Current (legacy, kept working):

- Rust
- Qt 6, Qt Quick / QML, CXX-Qt
- SQLite for notebook persistence

Target (new work):

- Rust
- egui / eframe
- SQLite for notebook persistence (unchanged)

Do not use Qt, QML or CXX-Qt in the new egui crates. Do not use egui in the Qt-free
core crate.

## Platform goals

OmaTree is developed on Arch Linux / Omarchy first.

It must not depend on Omarchy to function.

The architecture should remain suitable for later builds on:

- other Linux distributions
- Windows
- macOS

Omarchy-specific integration must remain optional and isolated.

## Rules

Do not introduce:

- Electron
- web frontends
- React
- GTK
- KDE Frameworks
- cloud services
- accounts
- telemetry
- networking
- plugin systems

unless the maintainer explicitly decides otherwise.

The Qt-to-egui replacement is explicitly approved. Any *other* change to the
technology stack still needs explicit instruction.

Do not add functionality merely because it might be useful.

### Migration rules

- Do the migration work on the integration branch/worktree, not on `main`.
- Keep the Qt application building and its tests passing at every ticket.
- Preserve the Qt-free core: notebook model, SQLite persistence, recovery (Trash and
  checkpoints), search and document lifecycle. Do not rewrite it.
- The egui application has **no Edit/Preview toggle**. Live Markdown rendering replaces
  it, and the raw Markdown stays accessible while editing.
- Shortcuts: **Ctrl+E** is inline code in the egui editor; **Ctrl+Shift+S** is Save As in
  OmaTree (strikethrough needs another shortcut or a slash command). The standalone
  editor's key map may differ and stays configurable.
- Images in the egui application come only from attachments stored in the notebook.
  Never fetch remote resources and never load arbitrary files from disk.
- Attachment ids are random 128-bit values written as `assets/<32-hex-id>.<ext>`.
- Attachment cleanup and Compact Notebook are required features: never delete an
  attachment that an active note, a Trash entry or a retained checkpoint references,
  never prune from "what is visible", and never delete recoverable attachments
  automatically. Pruning and `VACUUM` are separate steps and both need confirmation
  and clear reporting.
- Do not modify the standalone editor repository from here; consume it as a library.

Prefer simple code over clever abstractions.

Keep dependencies minimal.

## UI philosophy

OmaTree should feel calm and obvious.

Avoid:

- permanent toolbars full of buttons
- ribbons
- complex menus
- panels that are not essential
- configuration-heavy interfaces

The default screen should ultimately be little more than (in both the Qt and the egui application):

- tree
- note editor

Keyboard navigation should be treated as a first-class interaction method.

## Development

Work only on the change that was asked for, and do not add functionality
merely because it might be useful.

Before making substantial changes:

1. inspect the repository
2. read the relevant documentation (`VISION.md`, `ARCHITECTURE.md`)
3. make a short plan

Before declaring a change complete, run the appropriate checks.

For Rust code, normally:

    cargo fmt --check
    cargo clippy --all-targets
    cargo test
    cargo build

If something cannot be completed, report the problem rather than silently changing the architecture.

For the Qt application the checks above are unchanged and still required while it
exists. For egui crates run the same four commands for the affected crates (and
`cargo test` for the Qt-free core must not need Qt installed once it is extracted).

Mark GUI behaviour that was not actually exercised as **not tested**; never infer a pass
from compilation.

Do not create Git commits unless explicitly asked.
