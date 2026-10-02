# Ticket 004 — QML-compatible hierarchical tree model

## Goal

Expose OmaTree's existing Rust `Notebook` hierarchy to QML through a proper
Qt hierarchical item model.

This ticket is about the Rust ↔ Qt/QML boundary only.

Do not build the real OmaTree visual interface yet.

The existing pure-Rust `Notebook` remains the source of truth.

## Architecture

Implement a QML-compatible hierarchical model using Qt's standard
`QAbstractItemModel` model/view architecture.

Use the supported CXX-Qt 0.10 mechanisms and existing Qt/CXX-Qt types.

Before implementing, inspect the CXX-Qt 0.10 APIs/examples available locally
if necessary.

Do NOT implement a parallel tree structure in QML or JavaScript.

Do NOT copy the Notebook hierarchy into a separate long-lived data structure
solely for presentation.

The Qt model should adapt the existing `Notebook`.

## Model behaviour

The model represents the Notebook hierarchy:

- root nodes appear as root rows
- child nodes appear beneath their parent
- sibling order matches `Node::position`
- hierarchy may be arbitrarily deep
- the model has one visible column

The visible display value is the node title.

Use standard Qt model semantics for:

- index
- parent
- row count
- column count
- data
- roles where appropriate

Do not expose raw Rust pointers or references through QModelIndex.

Use stable node identity internally.

## Node identity

`NodeId` is currently `u64`.

Avoid exposing NodeId to QML as a JavaScript number if doing so could lose
integer precision.

Prefer QModelIndex-based interaction or another safe opaque representation.

Do not redesign NodeId solely for QML.

## Mutations

Expose the minimum Qt/QML-callable operations necessary for later UI work:

- create a root node
- create a child beneath an existing node
- rename a node
- delete a node and its subtree

These operations must modify the existing Rust `Notebook`.

The Qt model must emit the correct model notifications for structural and
data changes so QML views update without resetting the whole model.

Use the appropriate Qt begin/end insert/remove calls and data-change
notifications.

Do not use full model reset as the normal implementation for every mutation.

## Note body access

Expose a minimal Qt-facing way to:

- read the body of a node
- update the body of a node

The body does not need to be a visible tree-model role.

This is only bridge functionality for a later editor ticket.

Do not implement autosave or persistence integration here.

## Ownership

There must be exactly one authoritative Notebook instance for the Qt-facing
model.

Do not maintain separate independently mutable Rust and QML copies.

Choose the simplest CXX-Qt-compatible ownership design.

If this requires restructuring the current empty `OmaTreeApp` bridge, keep
the change minimal and explain the chosen ownership structure in the
completion report.

## Persistence

Do not connect `Storage` to the Qt model yet.

The model may begin with an empty in-memory Notebook.

Ticket 003 persistence code must remain unchanged except for strictly
necessary compatibility changes.

## QML

Keep `qml/main.qml` essentially unchanged.

It may instantiate or reference the new model if required to verify QML
registration, but:

- do not build the sidebar
- do not build the editor
- do not add menus
- do not style the application
- do not implement the final layout

The real visual UI belongs to the next ticket.

## Error handling

Invalid model indexes or invalid operations must fail cleanly.

Do not panic because QML supplied an invalid or stale QModelIndex.

Normal invalid UI operations should return a failure value or otherwise be
handled safely.

## Tests

Add meaningful tests where practical.

Cover at least:

1. root row count
2. child row count
3. nested hierarchy
4. correct parent/index relationships
5. sibling ordering
6. title data returned for nodes
7. creating a root updates the model
8. creating a child updates the model
9. renaming changes the returned title
10. deleting a node removes its whole subtree
11. body read/write reaches the underlying Notebook
12. invalid/stale indexes are handled without panic

Prefer tests against the actual Qt-facing model where feasible.

Pure helper logic may be tested separately if Qt runtime requirements make
some model tests impractical.

Do not weaken the architecture merely to make testing easier.

## Verification

Before completion run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Also launch the application with:

`QT_QPA_PLATFORM=offscreen`

and verify that the QML module loads without runtime errors.

All verification must pass.

## Out of scope

Do not:

- implement SQLite loading/saving through the UI
- implement autosave
- implement the real two-pane visual interface
- implement drag and drop
- implement move/reparent/reorder
- implement search
- implement menus
- implement Omarchy theming
- add unrelated dependencies
- perform unrelated cleanup
- change the database schema