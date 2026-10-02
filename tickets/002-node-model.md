# Ticket 002 — In-memory notebook and node model

## Goal

Implement OmaTree's core hierarchical note model in pure Rust.

This ticket must not involve Qt, QML, SQLite, persistence, or UI code.

Each node is itself a note and may also contain child nodes.

## Data model

Introduce a stable `NodeId` type and a `Node` type containing:

- `id`
- `parent_id`
- `position`
- `title`
- `body`

Introduce a `Notebook` type that owns the collection of nodes.

A notebook may contain multiple root nodes.

## Required operations

Implement:

- create a root node
- create a child node
- get a node by ID
- rename a node
- update a node body
- list root nodes in sibling order
- list children of a node in sibling order
- delete a node and its entire descendant subtree

Node IDs must remain stable for the lifetime of the notebook.

## Tree invariants

The model must guarantee:

- a node cannot reference a nonexistent parent
- sibling ordering is deterministic
- deleting a parent deletes all descendants
- duplicate node IDs cannot exist
- callers cannot accidentally create invalid parent/child relationships

Do not add move/reparent/reorder operations yet. Those will be implemented in a later ticket.

## Design constraints

- Pure Rust only.
- No Qt or CXX-Qt types in the domain model.
- No SQLite.
- No serialization.
- No filesystem access.
- No timestamps yet.
- No search.
- No UI changes.
- Keep the implementation deliberately small.

Do not introduce abstractions solely for hypothetical future requirements.

Choose data structures with later SQLite persistence in mind, but do not implement persistence now.

## Tests

Add meaningful unit tests covering at least:

1. creating multiple root nodes
2. creating nested child nodes
3. root ordering
4. child ordering
5. renaming a node
6. updating node body text
7. deleting a leaf node
8. deleting a node recursively deletes descendants
9. attempting to create a child for an invalid parent fails cleanly

Remove or replace the current placeholder `test_app_instantiation` if it no longer provides useful coverage.

## Verification

Before completion:

- `cargo fmt --check`
- `cargo clippy`
- `cargo test`
- `cargo build`

All must pass.

## Out of scope

Do not:

- modify QML
- implement SQLite
- expose the model through CXX-Qt
- implement drag and drop
- implement reparenting
- implement node reordering
- implement search
- change the application's visual appearance
- perform unrelated dependency cleanup