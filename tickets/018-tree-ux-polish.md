# Ticket 018 — Tree UX, remembered expansion and command polish

## Goal

Improve the part of OmaTree the user interacts with most often:

    the note tree

This ticket should make hierarchy easier to read, make deep trees dramatically
less tedious to navigate, and push the existing text-first Omarchy visual style
a little further.

Primary changes:

1. recursive expand/collapse
2. remembered expansion state
3. visible parent/child tree guides
4. restrained additional semantic colour in commands
5. tree-specific context actions

Do not add an icon pack.

OmaTree remains predominantly text-driven.

## Design direction

The tree should visually borrow from excellent terminal tree views:

    Parent
    ├─ Child
    │  ├─ Grandchild
    │  └─ Grandchild
    └─ Child

Do NOT literally replace the QML tree with text glyph output.

Instead, reproduce the same structural clarity using subtle tree guide lines.

The result should communicate:

- which node owns which children
- where nesting continues
- where a branch terminates

without making the tree visually busy.

## Recursive expand/collapse

Normal disclosure interaction remains unchanged:

    click disclosure marker
        -> expand/collapse only that node

Add:

    Ctrl + click disclosure marker
        -> recursively expand/collapse that entire subtree

Behaviour:

- if the clicked node is currently collapsed:
  - recursively expand the node and all descendants

- if the clicked node is currently expanded:
  - recursively collapse the node and all descendants

Use Qt Quick TreeView's existing recursive expansion APIs.

Do not manually walk the model in QML merely to expand rows if TreeView already
provides the operation.

## Keyboard recursive expansion

For the currently selected tree node add:

    Ctrl+Right
        recursively expand selected subtree

    Ctrl+Left
        recursively collapse selected subtree

Plain Left / Right retain their existing normal tree-navigation behaviour.

Do not change existing shortcuts.

If the selected node has no children, these commands are harmless.

## Expand All / Collapse All

Provide discoverable tree-level commands:

    Expand All
    Collapse All

Do not add another permanent footer button.

Add them to a small tree context menu opened by right-clicking the tree.

Suggested ordering:

    New note
    New child
    ─────────────
    Expand subtree
    Collapse subtree
    Expand all
    Collapse all
    ─────────────
    Move to Trash

Only show/enable actions where they make sense.

The exact ordering may be adjusted for clarity.

Do not duplicate application-wide File actions here.

## Recursive context actions

For the clicked node provide:

    Expand subtree
    Collapse subtree

These operate recursively.

If the context menu is opened over empty tree space:

- node-specific actions are disabled or omitted
- Expand all / Collapse all remain available

## Tree expansion persistence

Remember which nodes were expanded when the notebook was last successfully
saved.

When the notebook is reopened, restore that expansion state.

Example:

    Projects              expanded
      OmaTree             expanded
        Tickets           collapsed
      Threatwright        expanded

After reopening the same saved notebook, restore that layout.

Do not simply expand everything on open.

## Persistence location

Store expansion state inside the .omatree notebook.

This makes the notebook reopen consistently even if it is moved or opened on
another machine.

Expansion state is notebook UI metadata, not note content.

Introduce schema version 3 with a small table dedicated to view state.

Preferred shape:

    expanded_nodes
        node_id INTEGER PRIMARY KEY

A different equally small representation is acceptable if cleaner.

Do not store:

- QModelIndex
- row numbers
- title paths
- QML object identities

Persist stable NodeId values.

## Schema migration

Existing v1/v2 notebooks must continue to open without modification.

A legacy notebook with no stored expansion state opens with the current
collapsed behaviour.

Do not mutate a legacy notebook merely by opening it.

The first successful save may migrate it transactionally to schema v3.

Preserve all existing migration safety guarantees.

## View-state ownership

Expansion state must not live only in QML.

Rust should own the persisted set of expanded NodeIds.

QML owns the live visual TreeView state, but informs the model/document when
the user changes expansion.

The model provides enough API to:

- record an index as expanded/collapsed
- ask whether an index should be expanded after document load/reset
- clear/reconcile stale expansion ids safely

Do not expose NodeId directly to QML.

## View-state dirty semantics

Expansion state is not document content.

Changing expansion must NOT:

- add the window-title dirty star
- create a recovery checkpoint
- create Trash activity
- emit ordinary document mutation
- trigger the normal content autosave debounce
- produce an unsaved-changes warning

Track view-state dirtiness separately if needed.

## Saving view state

A successful ordinary Save / Save As must persist the current expansion state.

If the document is otherwise clean but expansion state changed:

    Ctrl+S

must still save the new expansion state.

It must not incorrectly no-op simply because note content is clean.

When a normal content autosave occurs, any pending expansion-state changes may
piggyback on that save.

Do not start autosave merely because the user opened or closed a branch.

## Close semantics

If only expansion state changed and note content is otherwise clean:

- do not show an unsaved-content warning

Prefer silently preserving the most recent successfully saved view state.

Do not complicate safe-to-leave behaviour merely for tree expansion.

The last successfully persisted expansion state is the authoritative state on
next open.

## Save As

Save As must carry the current expansion state into the new notebook file.

Opening that new file should reproduce the same expanded branches.

## New Notebook

A new Untitled notebook starts with no stored expansion state.

Nothing is expanded because the tree is empty.

## Open

Opening another notebook:

1. reset the model normally
2. clear visual expansion from the previous notebook
3. load the new notebook's persisted expansion state
4. restore expansion only for valid active nodes

Never leak expansion state between notebooks.

## Deleted nodes

Expansion metadata referring to nodes that no longer exist in the active
Notebook must be handled safely.

On save:

- discard stale expansion ids for nodes no longer present

Do not fail the save merely because view metadata is stale.

## Moves

Because expansion state is keyed by NodeId, moving/reparenting a node must not
cause it to forget whether it was expanded.

If an expanded subtree is moved:

- retain its expansion state where practical

Do not key state to breadcrumb/title/path.

## Trash / restore

Moving a subtree to Trash removes it from the visible active tree.

Its stale expansion metadata may remain transiently in memory but must not
cause errors.

If the exact node IDs are restored before the next save, retaining their
previous expansion state is acceptable and desirable.

If not restored, stale ids are removed during persistence reconciliation.

Expansion state is not checkpointed.

Restoring a recovery checkpoint should preserve expansion state only for node
IDs that remain valid after restoration.

## Search interaction

Search result activation already expands ancestors to reveal the selected note.

Those newly visible ancestors should become part of the current live expansion
state.

If the user later saves the notebook, that state is persisted normally.

Search itself must otherwise remain unchanged.

## Expand All performance

Use TreeView's native recursive expansion support.

Do not build a second hierarchy in QML.

Expanding very large notebooks may naturally require some work.

Do not add threads, async code or progressive expansion.

The current in-memory notebook sizes do not justify it.

## Tree guide lines

Add subtle visual branch guides.

The intended structure resembles:

    │
    ├── child
    │   ├── child
    │   └── child
    └── child

but implemented using QML geometry rather than literal full tree strings.

Use:

    Theme.border

for ordinary guides.

Keep guide contrast restrained.

Guides should be clearly visible when looking for hierarchy but should not
compete with note titles.

## Guide geometry

For each visible tree row, render the appropriate:

- ancestor continuation lines
- branch elbow for the current node
- termination where the node is the final sibling

The visual result must distinguish:

    ├─ node with siblings below

from:

    └─ final sibling

and show continuing ancestor branches correctly.

Do not draw lines through unrelated branches.

## Model support for guides

If QML cannot reliably infer whether each ancestor has a following sibling,
add the smallest read-only model support necessary.

For example, a derived guide description may encode:

- depth
- whether current node is last sibling
- which ancestor levels continue

This is presentation metadata only.

Do not expose NodeId.

Do not place tree-structure business logic into JavaScript.

## Disclosure markers

Keep disclosure affordances.

They are useful and should not be replaced solely by guide lines.

Refine their appearance if useful so they harmonise with the new guides.

They should remain visibly distinct for:

- collapsed node with children
- expanded node with children
- leaf node

Do not use image assets.

Text/vector/QML geometry is sufficient.

## Selected row

Preserve the existing:

- quiet selection background
- accent bar

Tree guides inside the selected row may become slightly stronger or use the
active accent where that improves legibility.

Do not turn the whole branch into a bright colour.

## Colour refinement

Push the current visual language slightly further without adding icons.

### Delete

DELETE should use:

    Theme.danger

even at rest.

It may become brighter/stronger on hover or keyboard focus.

The destructive nature of the action should be immediately legible.

Do not require hover before the user can tell it is destructive.

### New note / New child

Keep the text labels.

Use Theme.positive selectively for the leading `+` glyph if practical while
keeping the command text restrained.

Example:

    + NOTE
    + CHILD

where only the structural `+` carries the stronger positive colour.

If splitting the colour cleanly would substantially complicate Command.qml,
use a restrained positive tint for the command instead.

### Search

Keep the text label.

A restrained Theme.accent treatment is appropriate.

Do not add a magnifying-glass icon in this ticket.

### File

Keep the text label.

Prefer neutral or accentSecondary treatment.

Do not add a folder icon in this ticket.

## Icons

Do not add icons in Ticket 018.

This is deliberate.

The current design language is primarily:

- text
- typography
- line geometry
- semantic colour

The tree guide work should be evaluated first.

After this ticket, icons may be reconsidered only if a real discoverability
problem remains.

Do not introduce:

- Font Awesome
- Material icons
- SVG icon bundles
- platform icon dependencies
- emoji as UI icons

## Context menu visual style

The tree context menu should use the existing OmaTree command-menu language.

It must not look like a stock Qt popup if the existing File menu already has a
styled command menu.

Use:

- compact rows
- semantic hover/focus
- restrained border
- fixed-width shortcut hints where applicable

Destructive actions use Theme.danger.

## Tree spacing

Review indentation after adding branch guides.

The guides should not cause deep trees to consume absurd horizontal space.

Slightly reducing or retuning per-level indentation is allowed.

Keep the tree readable at depths of at least 6–8 levels.

Do not redesign the left-pane width automatically.

## Tests — expansion behaviour

Preserve all existing tests.

Cover at least:

1. ordinary expansion changes only one node
2. recursive expand expands all descendants
3. recursive collapse collapses all descendants
4. Expand All expands every expandable node
5. Collapse All collapses all nodes
6. leaf recursive expansion is harmless
7. Ctrl+click uses recursive behaviour
8. Ctrl+Right recursively expands selected subtree
9. Ctrl+Left recursively collapses selected subtree

## Tests — persisted expansion

Cover at least:

10. v2 notebook loads with empty expansion state
11. opening v2 causes no write
12. first successful save migrates to v3 transactionally
13. expanded NodeIds save correctly
14. collapsed NodeIds are absent after save
15. reopen restores saved expansion state
16. Save As carries expansion state
17. expansion-only changes do not mark document content dirty
18. expansion-only changes create no checkpoint
19. expansion-only changes do not trigger autosave
20. manual Ctrl+S persists expansion-only changes
21. ordinary content save also persists pending expansion state
22. stale/missing NodeIds are ignored safely
23. stale ids are removed/reconciled on save
24. moved expanded node retains state
25. restored same-ID node may regain valid expansion state
26. New/Open do not leak prior notebook expansion state

## Tests — visual guide metadata

Pure-Rust/model tests should verify derived guide information for at least:

27. one root
28. multiple roots
29. first of several siblings
30. middle sibling
31. last sibling
32. nested branch with continuing ancestor
33. nested final branch
34. depth of at least four levels
35. moved nodes produce updated guide information

Do not write pixel-perfect tests.

## Runtime verification

Exercise on a real desktop:

- ordinary disclosure click
- Ctrl+click recursive expansion
- Ctrl+click recursive collapse
- Ctrl+Right / Ctrl+Left
- Expand subtree
- Collapse subtree
- Expand All
- Collapse All
- save notebook
- close/reopen and verify expansion restoration
- Save As and verify restoration in new file
- Search activation revealing a deep node
- move an expanded subtree
- delete and restore an expanded subtree
- deep tree guide rendering
- light Omarchy theme
- dark Omarchy theme

Inspect especially:

- `├`-like continuation visual
- `└`-like final branch visual
- deep indentation
- selected row
- command colours
- destructive DELETE colour
- tree context menu

No full-screen screenshots.

If screenshots are used, capture OmaTree only.

## Regression checks

Verify:

- note creation
- rename
- body editing
- Markdown Preview
- Search
- drag/drop
- Trash
- Recovery
- New/Open/Save/Save As
- theme switching

Expansion changes must never mutate note content.

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo build

Perform:

- clean offscreen launch
- real-desktop tree/guide inspection
- save/reopen expansion verification
- light/dark theme inspection

No QML warnings or binding loops.

## Out of scope

Do not:

- add an icon pack
- replace text commands with icons
- add recent notebooks
- remember selected node
- remember editor cursor between application launches
- remember window geometry
- remember splitter width
- add settings UI
- change Search semantics
- add Markdown features
- perform the permanent integration-test ticket
- perform packaging work
- perform unrelated cleanup