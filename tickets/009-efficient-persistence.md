# Ticket 009 — Autosave-ready persistence

## Goal

Make OmaTree persistence efficient enough for future debounced autosave.

Currently a save rewrites:

- the active Notebook
- Trash
- up to 100 full-Notebook recovery checkpoints
- notebook metadata

That is safe but unnecessarily expensive for ordinary text edits.

Split persistence into two logical domains:

1. active document state
2. recovery state

Ordinary edits should persist the active Notebook without rewriting unchanged
Trash and checkpoint history.

Do NOT implement autosave in this ticket.

Do NOT implement incremental per-node persistence yet.

## Persistence domains

### Active state

Active state consists of:

- active Notebook nodes
- next-node-ID metadata

Changes to active state include:

- create root
- create child
- rename
- body edit

### Recovery state

Recovery state consists of:

- Trash entries
- Trash nodes
- checkpoints
- checkpoint nodes

Operations that modify recovery state include:

- delete / move subtree to Trash
- restore from Trash
- restore checkpoint
- future structural operations that explicitly create a checkpoint

Recovery operations also modify the active Notebook.

## Dirty-state tracking

Extend Document's internal state so it can distinguish:

- active state dirty
- recovery state dirty

The existing externally visible `dirty` concept remains:

    dirty = active_dirty || recovery_dirty

Do not expose unnecessary persistence internals to QML.

The existing UI dirty marker must continue behaving exactly as before.

## Mutation classification

Successful ordinary mutations:

- create root
- create child
- rename
- body edit

must mark:

    active_dirty = true

but must NOT mark:

    recovery_dirty = true

Successful recovery/structural mutations:

- delete / move to Trash
- restore from Trash
- restore checkpoint

must mark both:

    active_dirty = true
    recovery_dirty = true

A mutation that changes nothing must not alter either dirty flag.

Failed mutations must not alter either dirty flag.

## Storage behaviour

Add a storage operation that persists only active state.

The exact API name is flexible, for example:

    save_active(...)
    save_current(...)
    save_notebook_state(...)

Choose the smallest clean API that fits the existing architecture.

For an existing schema-v2 notebook, an active-only save must transactionally
update only:

- `nodes`
- `notebook_meta`

It must NOT write to:

- `trash_entries`
- `trash_nodes`
- `checkpoints`
- `checkpoint_nodes`

Rewriting the complete active `nodes` table inside one transaction is still
acceptable.

Do NOT implement per-node UPDATE/INSERT/DELETE persistence in this ticket.

## Full save

Keep a full-state save operation that atomically persists:

- active Notebook
- next-node-ID metadata
- Trash
- checkpoints

A full save is required whenever recovery state is dirty.

It is also required for cases where a complete independent document must be
written, including Save As.

## Choosing save scope

For an existing schema-v2 file:

### Nothing dirty

If neither active nor recovery state is dirty:

- perform no database write if practical
- return success

### Active state only dirty

If:

    active_dirty = true
    recovery_dirty = false

perform an active-only save.

After success:

    active_dirty = false

Recovery state remains unchanged.

### Recovery state dirty

If:

    recovery_dirty = true

perform a full-state save.

After success:

    active_dirty = false
    recovery_dirty = false

Recovery dirty implies the active state must be persisted in the same
transaction, even if active_dirty was somehow false.

## Schema-v1 files

Do not perform active-only persistence against schema version 1.

A schema-v1 notebook still requires the existing transactional migration to
schema version 2 on its first successful save.

That first save must persist:

- active Notebook
- next-node-ID metadata
- recovery state
- schema version 2

After migration succeeds, future ordinary saves may use active-only
persistence.

Opening a v1 file must remain read-only with respect to migration until the
first successful save.

## Save As

Save As must always write a complete self-contained schema-v2 document,
including:

- active Notebook
- next-node-ID metadata
- Trash
- checkpoints

This is true even when recovery state is currently not dirty.

A new destination must never depend on historical state remaining in the
source file.

After successful Save As:

    active_dirty = false
    recovery_dirty = false

Existing Save As safety and overwrite guarantees must remain unchanged.

## Explicit Save

Ctrl+S should use the optimized save scope.

Examples:

Ordinary body edit:

    active-only save

Delete followed by Ctrl+S:

    full save

Clean document:

    no-op success where practical

Do not change the visible Ctrl+S behaviour.

## Atomicity

Active-only save must be transactional.

A failure while rewriting active nodes or metadata must leave the previous
active database state valid.

Recovery tables must remain untouched.

Full saves retain the existing all-or-nothing guarantees.

A save failure must not incorrectly clear dirty flags.

## Recovery integrity

Active-only saving must not alter recovery history in any way.

This includes:

- checkpoint IDs
- checkpoint timestamps
- checkpoint reasons
- checkpoint node rows
- Trash entry IDs
- Trash timestamps
- Trash node rows

Do not rebuild, reorder or rewrite recovery tables during active-only save.

Tests should prove that no writes occur to these tables, not merely that their
logical contents compare equal afterward.

A suitable test technique is to install SQLite triggers that abort if a
recovery table receives INSERT, UPDATE or DELETE during an active-only save.

Another equally strong approach is acceptable.

## No-op saves

Where practical, saving a completely clean schema-v2 Document should not
rewrite any database table.

Add a test that detects unintended writes.

Do not complicate the implementation merely to optimize metadata reads.

## Model/UI behaviour

No visual changes are required.

NotebookModel and QML must continue to expose one simple `dirty` state.

Do not add:

- "recovery dirty"
- "active dirty"
- save-mode indicators
- status-bar messages

to the UI.

These are implementation details.

## Tests

Preserve all existing tests.

Add tests covering at least:

1. body edit marks active dirty only
2. rename marks active dirty only
3. root creation marks active dirty only
4. child creation marks active dirty only
5. deleting a node marks active and recovery dirty
6. restoring Trash marks active and recovery dirty
7. restoring a checkpoint marks active and recovery dirty
8. unchanged body/title does not alter dirty flags
9. failed mutation does not alter dirty flags
10. active-only save persists active nodes
11. active-only save persists next-node-ID metadata
12. active-only save does not write any recovery table
13. active-only save preserves Trash exactly
14. active-only save preserves checkpoints exactly
15. successful active-only save clears only active dirty
16. recovery-dirty save performs a full save
17. successful full save clears both dirty domains
18. failed active-only save leaves dirty state unchanged
19. failed full save leaves dirty state unchanged
20. active-only save rollback leaves previous active data valid
21. Save As always writes complete recovery state
22. Save As clears both dirty domains after success
23. first save of schema-v1 notebook still performs full transactional migration
24. subsequent ordinary save after v1 migration may use active-only persistence
25. clean schema-v2 save performs no table writes where practical
26. open/save/reopen still preserves all active content
27. Trash still survives normal active-only saves
28. checkpoints still survive normal active-only saves
29. persistent next-node-ID behaviour remains correct
30. all existing v1/v2 migration and recovery tests continue to pass

Use temporary databases only.

## Runtime verification

No new UI workflow is required.

Verify at minimum:

- clean application launch
- ordinary edit + Ctrl+S
- delete + Ctrl+S
- Recovery dialog still shows Trash/checkpoints after ordinary saves
- Save As preserves recovery history
- no QML warnings or binding loops

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Perform an offscreen launch after implementation.

## Out of scope

Do not:

- implement autosave
- implement per-node incremental SQLite updates
- change schema version
- add new database tables
- change checkpoint behaviour
- change Trash behaviour
- implement undo/redo
- implement drag and drop
- implement move/reparent/reorder
- enable WAL or change SQLite journaling strategy solely for performance
- add async Rust
- add threads or an async runtime
- change the UI design
- implement Omarchy theming
- implement search
- perform unrelated cleanup