# OmaTree Agent Instructions

OmaTree is a deliberately simple hierarchical note-taking application.

The core interaction is:

- a tree of nodes on the left
- the selected node's note on the right
- every node is itself a note
- nodes may contain child nodes

The application must remain simple, fast, local-first, and pleasant to use.

## Approved technology

- Rust
- Qt 6
- Qt Quick / QML
- CXX-Qt for Rust/Qt integration
- SQLite for notebook persistence

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

Do not replace the approved technology stack without explicit instruction.

Do not add functionality merely because it might be useful.

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

The default screen should ultimately be little more than:

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

Do not create Git commits unless explicitly asked.
