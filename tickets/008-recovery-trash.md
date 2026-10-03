# Ticket 008 — Trash and recovery checkpoints

## Goal

Add persistent recovery mechanisms before OmaTree gains autosave.

OmaTree must protect the user from accidental destructive changes.

This ticket introduces:

- persistent Trash for deleted subtrees
- persistent recovery checkpoints
- restoration from Trash
- restoration of a previous checkpoint
- database schema version 2
- safe migration/compatibility with existing schema version 1 notebooks

Do NOT implement autosave yet.

The central principle is:

    Saving must not make accidental destructive actions irreversible.

## Delete behaviour

Deleting a node must no longer permanently destroy it.

When the user deletes a node:

1. create a recovery checkpoint of the current active Notebook
2. copy the node and its complete descendant subtree into Trash
3. remove the subtree from the active Notebook
4. mark the Document dirty

Preserve in Trash:

- node IDs
- titles
- bodies
- complete subtree hierarchy
- sibling ordering inside the subtree
- the deleted root's original parent ID
- the deleted root's original sibling position
- deletion timestamp

The existing delete confirmation should now describe the operation as moving
the node/subtree to Trash.

Example:

    Move "Projects" and everything beneath it to Trash?

Do not describe normal Delete as permanent deletion.

## Trash lifetime

Trash is persistent notebook data.

It must:

- survive Save
- survive close/reopen
- survive Save As
- remain associated with that notebook

Do not automatically purge Trash.

Do not implement age-based cleanup.

Do not implement Empty Trash in this ticket.

Text is cheap; safety is more important than aggressive cleanup.

## Restoring from Trash

Provide a recovery UI that lists trashed subtree roots.

For each entry display enough information to identify it, at minimum:

- root title
- deletion date/time

The user must be able to restore a Trash entry.

When restoring:

- preserve the original node IDs
- preserve the complete subtree
- preserve titles and bodies
- preserve ordering inside the subtree
- remove the entry from Trash after successful restoration

Placement:

If the original parent still exists:

- restore beneath that parent
- attempt to restore the original sibling position

If the original position is no longer available:

- clamp it to a valid sibling position

If the original parent no longer exists:

- restore the subtree as a root node
- place it at the end of the root list

Restoration must never silently overwrite or replace an existing active node.

If any restored node ID conflicts with an active node ID, fail cleanly.

Create a recovery checkpoint immediately before restoring from Trash so the
restore itself can later be undone through checkpoint recovery.

## Recovery checkpoints

Add persistent full-Notebook checkpoints.

A checkpoint stores:

- the complete active Notebook hierarchy
- all node IDs
- parent relationships
- sibling positions
- titles
- bodies
- creation timestamp
- human-readable reason

Examples of checkpoint reasons:

    Before deleting "Projects"
    Before restoring "Projects" from Trash
    Before restoring checkpoint "Before deleting Projects"

Checkpoints are not created for ordinary text typing or rename operations.

For this ticket, automatically create checkpoints only before operations that
replace or destructively modify tree structure:

- delete subtree
- restore Trash entry
- restore previous checkpoint

The API should make it straightforward for future move/reparent/reorder
operations to create a checkpoint before structural changes.

## Checkpoint limit

Keep at most the newest 100 checkpoints.

When adding checkpoint 101:

- discard the oldest checkpoint

This limit is fixed.

Do not add a preference for it.

Trash entries are NOT subject to this limit.

## Restoring a checkpoint

Provide a recovery UI listing available checkpoints.

At minimum display:

- reason
- creation date/time

The user may choose a checkpoint and restore it.

Before replacing the current active Notebook:

1. create a checkpoint of the current active Notebook
2. replace the active Notebook with the selected checkpoint state
3. mark the Document dirty
4. clear the current tree selection

Restoring a checkpoint does NOT immediately save to disk.

The user may still close and choose Discard if the restore was accidental.

Trash should remain intact when restoring a checkpoint.

Existing checkpoint history should also remain intact, subject to the
100-checkpoint limit.

A model reset is appropriate when restoring an entire checkpoint.

## Recovery UI

Add one compact, unobtrusive recovery entry point.

For example:

    Recovery…

It may live inside the existing File menu or beside it.

Do not add a permanent sidebar or large recovery panel.

The Recovery UI should provide access to:

- Trash
- Checkpoints

A simple dialog with two sections/tabs is sufficient.

Do not redesign the main OmaTree interface.

Do not expose NodeId to QML.

## Persistent state architecture

Keep the existing architecture:

    Document
        owns active Notebook
        owns optional Storage
        owns optional path
        owns dirty state

Extend Document with recovery state.

Prefer a small pure-Rust recovery module, for example:

    recovery.rs

containing the relevant recovery data structures and logic.

The exact names are flexible.

The active Notebook must remain the normal tree presented by NotebookModel.

Trash and checkpoint data must NOT appear as fake nodes inside the active
Notebook tree.

Do not implement Trash by inserting a special "Trash" note into Notebook.

## Stable IDs

Recovery makes NodeId lifetime more important.

Existing node IDs must never be reused merely because a node was deleted and
moved to Trash.

Persist the Notebook's next node ID / allocation counter as notebook metadata.

After save/reopen:

- newly created nodes must continue using IDs above the previously allocated
  range
- IDs present only in Trash must not be reused
- restoring Trash must therefore not normally encounter ID collisions

Handle overflow safely.

Do not change NodeId away from its current u64 representation merely for this
ticket.

## SQLite schema version 2

Introduce schema version 2.

Version 2 must persist:

- active nodes
- next node ID metadata
- Trash entries
- nodes belonging to each Trash entry
- recovery checkpoint metadata
- nodes belonging to each checkpoint

Keep SQLite implementation inside storage.rs.

A reasonable relational design is expected.

Do not serialize whole recovery objects into opaque JSON/BLOB columns unless
there is a compelling technical reason.

Human-readable TEXT fields and normal SQLite rows are preferred.

## Schema version 1 compatibility

Existing version 1 OmaTree notebooks must remain readable.

Opening a version 1 notebook must:

- load the active Notebook normally
- treat Trash as empty
- treat checkpoint history as empty
- derive a safe next node ID from the existing node IDs

Do not mutate a version 1 file merely because it was opened.

When a version 1 notebook is next successfully saved:

- migrate it transactionally to schema version 2
- persist the current active Notebook
- persist recovery state
- persist the next node ID
- set PRAGMA user_version = 2 only as part of the successful transaction

If migration/save fails:

- the original valid version 1 database must remain valid
- do not leave a half-migrated database

New notebook files must be created directly as schema version 2.

Schema versions greater than 2 remain unsupported and must fail cleanly.

## Atomic persistence

Saving a version 2 document must transactionally persist:

- active Notebook
- next node ID
- Trash
- checkpoints

These pieces represent one logical document state.

A failed save must not result in:

- new active nodes with old recovery data
- new Trash with old active nodes
- partial checkpoint state
- a changed schema version without the corresponding data

Either the entire document save succeeds or the previous valid database state
remains intact.

The current full-state rewrite approach is acceptable.

Do not implement incremental persistence yet.

## Dirty state

The following recovery operations make the Document dirty:

- delete / move to Trash
- restore from Trash
- restore checkpoint

Opening or successfully saving still clears dirty.

Simply opening the Recovery dialog does not make the Document dirty.

## Selection safety

Deleting the selected node must preserve the current safe selection behaviour.

Restoring a checkpoint replaces the entire active model, so selection must be
cleared.

Trash restoration may select the restored root if practical, but this is not
required.

No stale QModelIndex must survive a model reset.

## Tests

Preserve all existing tests.

Add tests covering at least:

1. deleting a leaf moves it to Trash instead of losing it
2. deleting a parent stores its entire subtree
3. Trash preserves titles, bodies, IDs and hierarchy
4. Trash survives save and reopen
5. restoring Trash reproduces the subtree
6. restore uses the original parent when it still exists
7. restore falls back to root if the original parent is gone
8. restore handles an unavailable original sibling position safely
9. restoring removes the entry from Trash
10. ID collisions during restore fail cleanly
11. node IDs are not reused after delete/save/reopen
12. a checkpoint is created before delete
13. checkpoint contains the exact pre-delete tree
14. restoring a checkpoint restores the previous active tree
15. restoring a checkpoint creates a checkpoint of the state being replaced
16. ordinary body edits do not create checkpoints
17. ordinary rename does not create checkpoints
18. only the newest 100 checkpoints are retained
19. version 1 notebook loads with empty recovery state
20. saving version 1 migrates transactionally to version 2
21. failed v1-to-v2 migration leaves the version 1 file valid
22. version 2 active Notebook round-trips correctly
23. version 2 Trash round-trips correctly
24. version 2 checkpoints round-trip correctly
25. version 2 next-node-ID metadata round-trips correctly
26. unsupported schema versions still fail cleanly
27. Save As preserves Trash and checkpoints
28. recovery operations correctly set dirty state

Use temporary databases/files only.

Add runtime/QML verification for:

- Delete → Trash
- Recovery dialog displays the Trash entry
- Restore from Trash
- checkpoint list display
- checkpoint restore
- selection safety
- no QML warnings or binding loops

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Also perform offscreen runtime verification of the Recovery UI.

Do not modify real user notebook files during testing.

## Out of scope

Do not:

- implement autosave
- implement undo/redo keyboard history
- implement drag and drop
- implement move/reparent/reorder
- implement Empty Trash
- automatically purge Trash
- implement configurable history limits
- implement diff views
- implement revision previews
- implement backups outside the notebook database
- implement recent notebooks
- implement search
- implement Markdown rendering
- implement Omarchy theming
- redesign the main interface
- add async Rust
- add threads or an async runtime
- implement incremental SQLite persistence
- perform unrelated cleanup