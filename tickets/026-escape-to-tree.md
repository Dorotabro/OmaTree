# Ticket 026 — Escape returns focus to the tree

## Goal

Improve keyboard-only navigation between the note editor and tree.

When the user is editing or reading the currently selected note, pressing
Escape should return keyboard focus to that note in the tree.

No mouse interaction should be required to resume tree navigation.

## Required behaviour

When a note is selected:

    Escape from body editor
        -> focus selected tree row

    Escape from title editor
        -> focus selected tree row

    Escape from Markdown Preview
        -> focus selected tree row

Afterward:

- Up / Down navigate the tree normally
- Left / Right operate the tree normally
- the same note remains selected
- Edit / Preview mode is preserved

## Title/body contents

Escape is a focus/navigation command only.

It must NOT:

- revert title text
- revert body text
- undo an edit
- save explicitly
- modify document contents

Existing live-edit/autosave semantics remain unchanged.

## Priority

Existing Escape behaviour in higher-priority transient UI remains authoritative.

Examples:

- open dialog -> Escape closes/cancels the dialog
- open menu -> Escape closes the menu
- Keyboard Shortcuts -> Escape closes it
- Search with a query -> Escape clears Search/restores tree as currently specified

Do not let the editor-level Escape handler steal Escape from those surfaces.

## No selection

If no note is selected, Escape from the editor area is harmless.

Do not invent a selection.

## Tree focus

If the tree already has focus:

    Escape -> no-op

Do not collapse nodes or clear selection.

## Document invariants

The focus transfer itself must not:

- mark the document dirty
- emit documentMutated
- create a checkpoint
- affect Trash
- affect expansion state
- start autosave
- modify Search state

## Keyboard Shortcuts reference

Update the F1 reference so the General or Editor section accurately mentions:

    Escape    Return to tree

Only describe it in the context where it applies.

Do not obscure the existing Escape behaviours for Search/dialogs.

## Permanent integration test

Add permanent coverage for:

1. select note and focus body
2. Escape focuses the selected tree row
3. Up/Down work immediately afterward
4. selected note is unchanged by the focus transfer
5. title editor -> Escape -> tree
6. Preview -> Escape -> tree
7. Edit/Preview mode is preserved
8. body/title content is unchanged
9. no additional mutation/checkpoint/autosave occurs
10. Search Escape behaviour still wins while Search is focused
11. dialog Escape behaviour still wins while a dialog is open

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo test integration_tests
- cargo build

Perform a clean offscreen launch.

Manually verify:

    edit note -> Escape -> Up/Down through tree

and:

    Preview -> Escape -> tree

## Out of scope

Do not:

- add new navigation shortcuts
- change Enter behaviour
- change Search Escape semantics
- change dialog/menu Escape semantics
- change persistence
- perform packaging
- perform unrelated cleanup