# Ticket 006 — Open and save notebook files

## Goal

Connect the existing SQLite persistence layer to the running OmaTree
application.

OmaTree must be able to:

- start with an existing notebook file
- load its tree and note contents
- edit the notebook normally
- save changes
- close and reopen with the same contents

Keep the workflow deliberately minimal.

Do not implement a file picker or Save As yet.

## Notebook file format

Use `.omatree` as the conventional notebook file extension.

The file itself remains the existing SQLite database.

Do not add a wrapper format or change the schema.

Do not reject other file extensions yet; `.omatree` is a convention,
not an enforced restriction in this ticket.

## Command-line startup

Support:

    omatree <path>

with either zero or one notebook path.

### Existing file

If the supplied path exists:

- open it using the existing `Storage`
- load its `Notebook`
- replace the Notebook currently owned by `NotebookModel`
- clear the current QML selection
- display the loaded tree

### Missing file

If the supplied path does not exist:

- create a new SQLite notebook at that exact path
- begin with an empty Notebook
- associate that path with the current document

### No path

If no path is supplied:

- start with an empty in-memory notebook
- do not create any file automatically
- treat it as an untitled notebook with no save destination

Do not implement a recent-files system.

## Loading into NotebookModel

Add the minimum supported mechanism for replacing the Notebook inside
`NotebookModel`.

Replacing the whole Notebook is one of the cases where a full Qt model reset
is correct.

Use the proper model reset notifications.

Do not use model reset for ordinary node mutations.

Any QML selection from the previous Notebook must be cleared before or as part
of replacing the model so stale QModelIndex values cannot survive a load.

## Persistence architecture

Reuse the existing `Storage` implementation.

Do not duplicate SQLite logic in the Qt/QML layer.

Keep `storage.rs` responsible for SQLite behaviour.

Use the simplest architecture that fits the existing CXX-Qt ownership model.

It is acceptable for `NotebookModel` to coordinate opening/saving a notebook
while delegating the actual persistence work to `Storage`.

Do not introduce a large controller/service hierarchy solely for this ticket.

## Current notebook path

Track the optional filesystem path associated with the currently loaded
Notebook.

Expose enough information to QML to display a useful window title.

Examples:

    OmaTree — Untitled
    OmaTree — notes.omatree
    OmaTree — notes.omatree *

The `*` indicates unsaved changes.

Do not expose unnecessary filesystem internals to QML.

## Dirty state

Track whether the in-memory Notebook differs from the last successful
load/save.

Dirty becomes true after a successful mutation that changes the Notebook,
including:

- create root
- create child
- rename
- body edit
- delete

Dirty becomes false after:

- successful load
- successful save
- creation of a new empty notebook file

A failed mutation must not mark the Notebook dirty.

A failed save must leave dirty true.

Expose dirty state to QML using the appropriate Qt property/signal mechanism.

## Save

Add a save operation for the current Notebook.

When a notebook path exists:

- save through the existing transactional `Storage::save`
- report failure cleanly
- clear dirty only after successful save

The existing full-notebook transactional rewrite is acceptable here.

Do not implement incremental persistence yet.

When no notebook path exists:

- save must fail cleanly
- do not invent a path
- do not create a file in the working directory

## Save UI

Add:

    Ctrl+S

to save the current notebook.

Keep the visual UI minimal.

A traditional File menu is not required.

If save fails, display a small understandable error dialog or equivalent
minimal error UI.

Do not expose raw SQLite errors directly to the user if a simple human-readable
message can be provided.

## Startup errors

Opening an invalid, corrupt, or unsupported notebook must never panic.

If an existing supplied file cannot be loaded:

- report the error visibly
- do not overwrite or replace that file
- do not associate the failed path with an empty notebook
- leave the application in a safe state

The user must not be able to accidentally destroy an unreadable notebook just
because loading failed.

## Closing with unsaved changes

Prevent accidental data loss.

If a file-backed notebook is dirty when the window is closed:

offer:

- Save
- Discard
- Cancel

Save:
- attempt to save
- close only if save succeeds

Discard:
- close without saving

Cancel:
- keep OmaTree open

For an untitled notebook with no path:

offer:

- Discard
- Cancel

Do not implement Save As in this ticket.

## Tests

Preserve all existing tests.

Add pure-Rust tests where practical for any new non-Qt state or path logic.

Runtime verification must cover at least:

1. launch with no path produces an empty Untitled notebook
2. launch with a nonexistent path creates a valid empty `.omatree` database
3. create notes, edit bodies, save, restart, and recover the same tree/content
4. nested nodes survive restart
5. sibling ordering survives restart
6. node titles survive restart
7. Ctrl+S clears dirty state after successful save
8. editing after save makes the notebook dirty again
9. failed save leaves dirty state set
10. loading a notebook clears the previous selection
11. corrupt/non-SQLite input fails without overwriting the file
12. unsupported schema version fails cleanly
13. closing a dirty file-backed notebook supports Save / Discard / Cancel
14. closing a dirty Untitled notebook supports Discard / Cancel
15. no QML runtime warnings or errors occur during normal use

Use temporary notebook files for automated/runtime checks.

Do not modify the user's real notebooks during testing.

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Also perform offscreen/runtime verification of the notebook open/save workflow.

All verification must pass.

## Out of scope

Do not:

- implement Save As
- implement a file picker
- implement recent notebooks
- implement autosave
- implement incremental SQLite updates
- change the database schema
- implement backups
- implement import/export
- implement drag and drop
- implement move/reparent/reorder
- implement search
- implement Markdown rendering
- implement Omarchy theming
- implement settings/preferences
- add unrelated dependencies
- perform unrelated cleanup