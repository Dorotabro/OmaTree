# Ticket 003 — SQLite notebook persistence

## Goal

Add SQLite persistence for OmaTree notebooks.

A notebook is stored as a single SQLite database file.

This ticket implements only the persistence layer and round-trip loading.
Do not connect persistence to QML or the application UI yet.

The existing pure-Rust `Notebook` model must remain independent of SQLite.

## Dependency

Use `rusqlite`.

Prefer its bundled SQLite feature for predictable cross-platform builds unless
there is a strong technical reason not to.

Do not introduce an ORM or another database abstraction library.

## Storage module

Create a Rust storage module responsible for:

- creating a new notebook database
- opening an existing notebook database
- creating/validating schema version 1
- saving a `Notebook`
- loading a `Notebook`

Keep SQLite-specific types and logic out of `notebook.rs` wherever practical.

## Schema

Schema version 1 must persist every node with:

- `id`
- `parent_id`
- `position`
- `title`
- `body`

Use SQLite's schema version mechanism (`PRAGMA user_version`) with version `1`.

The nodes table should use an integer primary key for node IDs.

Parent relationships should use a foreign key where practical.

Enable SQLite foreign-key enforcement for opened connections.

No timestamps or other metadata are required yet.

## Save behaviour

For this ticket, saving the complete notebook in one transaction is acceptable.

The save operation must be atomic:

- either the complete notebook is saved
- or the previous valid database state remains intact

Do not implement incremental per-keystroke persistence yet.

Do not implement autosave.

## Load behaviour

Loading must reconstruct the same logical notebook:

- node IDs preserved
- parent relationships preserved
- sibling positions preserved
- titles preserved
- bodies preserved
- root ordering preserved
- child ordering preserved

After loading, creating another node must generate an ID greater than all
existing IDs. IDs must never be accidentally reused because a notebook was
loaded from disk.

If reconstruction requires a new validated constructor or crate-private
hydration API in `Notebook`, add the smallest API necessary.

Do not expose model internals publicly merely to make SQLite loading easier.

## NodeId and SQLite integers

The current `NodeId` uses `u64`, while SQLite INTEGER values are signed 64-bit.

Handle this conversion explicitly and safely.

Do not silently truncate or wrap IDs.

Do not redesign the NodeId type unless there is a compelling reason.

## Error handling

Persistence failures must return Rust errors rather than panic.

Distinguish at least:

- database/SQLite errors
- unsupported schema version
- invalid persisted notebook data

Do not build an elaborate error hierarchy beyond what this ticket needs.

## Tests

Use temporary database files.

Add tests covering at least:

1. creating a new empty notebook database
2. saving and reopening an empty notebook
3. round-trip of multiple root nodes
4. round-trip of nested child nodes
5. preservation of root ordering
6. preservation of child ordering
7. preservation of titles and bodies
8. preservation of node IDs
9. creation of a new node after reload does not reuse an existing ID
10. reopening an existing valid database
11. unsupported schema version fails cleanly
12. invalid persisted parent relationship fails cleanly

Tests must use real SQLite files, not mocked persistence.

## Verification

Before completion run:

- `cargo fmt --check`
- `cargo clippy`
- `cargo test`
- `cargo build`

All must pass.

## Out of scope

Do not:

- modify QML
- change the visual UI
- expose persistence through CXX-Qt
- implement autosave
- implement incremental database updates
- implement search
- implement migrations beyond schema version 1
- implement backups
- implement import/export
- implement move/reorder/reparent
- add notebook file-extension handling
- perform unrelated dependency cleanup