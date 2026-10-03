# Ticket 011 — Move, reorder and reparent nodes

## Goal

Allow users to rearrange the OmaTree hierarchy safely.

Users must be able to:

- reorder nodes among siblings
- move a node under another node
- move a child back to the root level
- move an entire subtree as one unit

The operation must preserve node IDs, note contents and descendants.

Every successful structural move must create a recovery checkpoint first.

The existing autosave system should then persist the resulting structure.

## Core model operation

Add one pure-Rust Notebook operation representing structural movement.

Conceptually:

    move_node(node_id, new_parent_id, new_position)

The exact Rust API is flexible.

Where:

- `node_id` is the root of the subtree being moved
- `new_parent_id` is optional; no parent means root level
- `new_position` is the desired zero-based sibling position under the new parent

The operation moves the node and its complete descendant subtree.

Do not recreate nodes.

Do not assign new IDs.

## Required invariants

The operation must guarantee:

- the moved node still exists exactly once
- all descendants remain attached to it
- node IDs remain unchanged
- sibling positions remain contiguous from 0
- sibling ordering is deterministic
- no node references a nonexistent parent
- cycles cannot be created

The following moves must fail cleanly:

- moving a nonexistent node
- moving beneath a nonexistent parent
- moving a node beneath itself
- moving a node beneath any of its descendants
- any move that would create a cycle

Failures must not partially alter the tree.

## No-op moves

A move that would produce the exact same structure/order is a no-op.

Examples:

- moving row 2 to row 2 under the same parent
- moving a node to its existing parent and effective position

A no-op must:

- return success or an explicit no-change result
- not create a checkpoint
- not set dirty state
- not emit documentMutated
- not trigger autosave

Choose the smallest clean API for expressing whether the tree changed.

## Sibling position semantics

Define the move operation clearly using the final desired position after the move.

Example:

Current:

    A
    B
    C
    D

Move B to final position 3:

    A
    C
    D
    B

Do not expose Qt's internal destination-row quirks to Notebook.

Notebook should use simple final logical positions.

Any adjustment required by Qt beginMoveRows semantics belongs in the Qt adapter.

## Moving within one parent

Support reordering among siblings.

Examples:

    A B C D
    ↓
    B A C D

and:

    A B C D
    ↓
    A C D B

Positions must remain contiguous.

## Moving between parents

Support:

    Projects
      OmaTree
    Archive

moving OmaTree to:

    Projects
    Archive
      OmaTree

The whole subtree moves with it.

## Moving to root

A child may be moved back to root level.

Its parent becomes None and it is inserted at the requested root position.

## Recovery checkpoint

Before every successful structural move:

- create a checkpoint containing the current active Notebook

Use a human-readable reason such as:

    Before moving "OmaTree"

Do not create the checkpoint until the move has been validated as possible.

Do not create a checkpoint for failed or no-op moves.

The move then marks:

    active_dirty = true
    recovery_dirty = true

This ensures autosave performs the existing full recovery-aware save.

Do not manually invoke persistence from the move implementation.

The existing mutation signal + autosave mechanism should handle it.

## Qt model integration

Expose move/reparent support through NotebookModel.

Use Qt's proper model-move notifications.

Prefer:

    beginMoveRows
    endMoveRows

for moves that can be represented as a Qt row move.

Do not normally implement movement by resetting the entire model.

Do not fake a move as delete + insert if proper move notifications can safely
represent it.

Handle correctly:

- source parent QModelIndex
- destination parent QModelIndex
- root parent
- source row
- destination row
- same-parent moves
- cross-parent moves

Pay particular attention to Qt's destination-row semantics when source and
destination share a parent.

The pure Rust Notebook API should remain independent of those Qt-specific
semantics.

## QModelIndex safety

Do not store raw Rust pointers in QModelIndex.

Continue using the existing stable NodeId-based mapping.

After a move:

- stale indexes must fail safely
- the moved node must be addressable at its new position
- parent() and index() must return the new hierarchy correctly

Do not expose NodeId directly to QML.

## Selection

If the moved node is currently selected:

- keep that same logical node selected after the move where practical

Because QModelIndex row/parent data may change, do not rely on the old
selection index remaining valid.

It is acceptable for the move API to return a fresh QModelIndex for the moved
node after the operation.

If preserving selection becomes unsafe in a specific case, clearing selection
is preferable to leaving a stale index.

## Drag and drop UI

Add direct drag-and-drop rearrangement to the tree.

The interaction should feel simple:

- drag a tree row
- drop between siblings to reorder
- drop onto another node to make it a child
- drop at root level to move it to root

Do not add a separate "move node" dialog.

Use Qt Quick/QML mechanisms appropriate for the existing TreeView.

If Qt Quick TreeView does not provide a complete built-in drag/drop workflow,
implement the smallest clean delegate-level drag interaction necessary.

Do not create a second hierarchy in QML.

NotebookModel remains the authority.

## Drop affordance

While dragging, provide a restrained visual indication of the target.

At minimum distinguish:

- drop before/after a row
- drop as child of a row
- drop at root level

Do not build elaborate animation.

A simple line/highlight is enough.

## Expanding during drag

Automatic hover-to-expand is NOT required for this ticket.

Existing expanded nodes should remain usable as drop targets.

Do not add timers solely for drag expansion.

## Delete and Recovery behaviour

Existing Delete → Trash behaviour must remain unchanged.

Moving a node is not deletion and must not create Trash entries.

Recovery checkpoint restore must correctly undo a move because the pre-move
checkpoint contains the old tree.

Trash restore behaviour must remain correct after nodes have been moved.

## Autosave

A successful move must:

- mark recovery dirty
- emit the existing document mutation signal exactly once
- restart the existing autosave debounce timer

The resulting autosave should use the existing full-save path because recovery
state changed.

Do not call save directly from the move operation.

A failed or no-op move must not schedule autosave.

## Persistence

Do not change the SQLite schema.

Positions and parent IDs already contain everything necessary to persist the
new tree.

Do not add move-history tables.

Do not add any new persistence concept.

## Keyboard/accessibility fallback

Add a minimal non-drag fallback if it can be done cleanly.

Preferred commands:

- Alt+Up: move selected node one sibling position up
- Alt+Down: move selected node one sibling position down

These should use the same move operation and recovery behaviour.

Do not add keyboard shortcuts for every possible reparent operation in this
ticket.

If these shortcuts conflict with Qt/desktop behaviour, document the conflict
and choose another simple pair.

## Tests — Notebook model

Add pure Rust tests covering at least:

1. reorder root upward
2. reorder root downward
3. reorder child upward
4. reorder child downward
5. move root beneath another root
6. move child between parents
7. move child to root
8. move parent with nested descendants
9. descendant hierarchy remains intact after move
10. IDs remain unchanged
11. positions remain contiguous after same-parent move
12. positions remain contiguous after cross-parent move
13. moving beneath self fails
14. moving beneath descendant fails
15. missing source fails
16. missing destination parent fails
17. failed move leaves tree byte/logically unchanged
18. exact no-op move does not count as a change
19. position below range clamps or fails according to one clearly documented rule
20. position above range clamps or fails according to one clearly documented rule

Choose either clamping or explicit range errors consistently.

## Tests — Document / recovery

Cover at least:

21. successful move creates exactly one checkpoint
22. checkpoint reason identifies the moved node
23. no-op move creates no checkpoint
24. failed move creates no checkpoint
25. successful move sets active dirty
26. successful move sets recovery dirty
27. successful move emits/schedules one mutation
28. checkpoint restore restores the exact pre-move tree
29. moved structure survives save/reopen
30. moved structure survives Save As

## Tests — Qt/model

Cover where practical:

31. correct source and destination QModelIndex relationships after move
32. same-parent row movement
33. cross-parent movement
34. root ↔ child movement
35. stale pre-move QModelIndex is handled safely
36. fresh moved-node index resolves correctly
37. selected moved node can be reselected safely

Use runtime QML/QtTest verification where CXX-Qt prevents normal Rust-side
QObject construction.

## Runtime verification

Exercise at least:

- drag reorder among roots
- drag reorder among children
- drag root onto another root
- drag child from one parent to another
- drag child back to root
- move a multi-level subtree
- reject dragging a node onto itself
- reject dragging a node onto its descendant
- selected node remains safe after move
- recovery checkpoint restores the old structure
- autosave persists the new structure
- restart preserves the new structure
- Recovery dialog remains correct
- no QML warnings or binding loops

Use temporary notebook files.

Do not modify real user notebooks during automated/runtime testing.

## Visual scope

Keep the existing OmaTree look.

Only add visuals required for understandable drag/drop.

Do not redesign the tree.

Do not implement Omarchy theming yet.

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Perform offscreen/runtime verification of drag/drop and keyboard movement.

Launch on a real graphical desktop if needed to verify pointer drag behaviour,
but do not steal focus unnecessarily.

## Out of scope

Do not:

- change SQLite schema
- implement undo/redo stack
- implement cut/copy/paste
- implement duplicate node
- implement multi-selection
- implement hover auto-expand
- implement arbitrary drag animations
- implement search
- implement Markdown rendering
- implement Omarchy theming
- implement settings
- implement recent files
- implement multiple windows
- add async Rust
- add threads
- perform unrelated cleanup