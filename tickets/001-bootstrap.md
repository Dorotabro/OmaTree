# Ticket 001: Bootstrap OmaTree

## Goal

Create the smallest possible working OmaTree application using:

- Rust
- Qt 6
- Qt Quick / QML
- CXX-Qt

The purpose of this ticket is only to prove that the selected technology stack builds and runs successfully.

## Required result

Running the application should display a native Qt window.

The window should:

- have the title "OmaTree"
- have a reasonable default size
- display the text "OmaTree"

Nothing else is required.

## Important constraints

Do NOT implement:

- note nodes
- tree view
- text editor
- SQLite
- persistence
- Omarchy theming
- menus
- settings
- search
- file handling

Do not prematurely implement future architecture.

This ticket is only a technology bootstrap.

## Build system

Use a conventional CXX-Qt project structure suitable for continued development.

Prefer Cargo as the primary developer-facing build command.

Avoid unnecessary dependencies.

## Verification

The project must successfully run:

    cargo fmt --check
    cargo clippy
    cargo test
    cargo build

The application must then launch successfully on the current Arch Linux / Omarchy environment using Qt 6.

## Completion report

When finished, report:

1. files created
2. dependencies introduced
3. build/run command
4. verification results
5. any warnings or problems encountered

Do not begin ticket 002.