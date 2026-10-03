# Ticket 019 — Keyboard and interaction polish

## Goal

Perform one focused paper-cut pass over OmaTree's existing interaction model.

Do not add product features.

The purpose of this ticket is to remove small inconsistencies that make an
otherwise keyboard-friendly application unexpectedly require the mouse or
leave stale visual state behind.

Primary targets:

1. keyboard-operable confirmation dialogs
2. keyboard access to the tree context menu
3. stale hover-state cleanup after menus/popups
4. keyboard/focus regression audit
5. ordinary text-editor shortcut verification

Keep this ticket small and surgical.

## Product principle

OmaTree is keyboard-first but not keyboard-exclusive.

Anything that can reasonably be reached with the mouse should have a sensible
keyboard path.

Do not create Vim-style modes or invent a new shortcut vocabulary.

Prefer established desktop conventions.

## Confirmation dialogs

OmaTree-owned confirmation dialogs must be fully keyboard operable.

At minimum:

    Enter / Return
        activate the default affirmative action

    Space
        activate the currently focused button

    Escape
        activate Cancel / dismiss safely

This applies to dialogs such as:

- Move to Trash
- unsaved changes
- overwrite / replace file
- other OmaTree-owned confirmation dialogs

## Default action semantics

Each confirmation dialog should have one explicit default action where that is
safe and conventional.

Examples:

### Move to Trash

Default:

    Cancel

Destructive confirmation should NOT become easier to trigger accidentally just
because Enter was pressed.

If the destructive button is explicitly focused, Space may activate it.

Enter should follow the safe/default action unless the existing dialog
semantics already define otherwise.

### Save / Discard / Cancel

Choose the safest sensible default.

Prefer:

    Save

where the user's intent is to preserve data.

Do not make Discard the default.

### Replace existing file

Prefer:

    Cancel

unless there is a strong existing application convention to the contrary.

The goal is keyboard operability, not dangerous one-key confirmation.

## Focus on dialog open

When an OmaTree dialog opens:

- keyboard focus must land on a meaningful control
- focus must be visibly indicated
- Tab / Shift+Tab must move through available controls
- Escape must work regardless of which control is focused

Do not leave focus trapped behind the dialog.

## Dialog button activation

Verify every Command-style dialog button works by:

- mouse click
- keyboard focus + Space
- keyboard focus + Enter/Return where appropriate

Do not implement separate activation logic for every individual dialog if a
shared Command or ThemedDialog component can solve this once.

Prefer fixing shared primitives.

## Tree context menu keyboard access

Add conventional keyboard access to the tree context menu.

Support:

    Shift+F10

and, where Qt exposes it reliably:

    Menu / Context Menu key

The menu must open for the currently selected tree node.

If there is no selected node:

- open the tree-level context menu at a sensible position
- node-specific actions are disabled or omitted
- Expand All / Collapse All remain available

Do not create a second keyboard-only menu implementation.

Reuse the existing tree context menu.

## Context menu placement

When opened from the keyboard:

- position the menu near the selected row if practical
- otherwise use a stable sensible fallback inside the tree pane

Do not require pointer coordinates to exist.

The menu must remain fully keyboard navigable once open.

## Context menu keyboard behaviour

Verify:

    Up / Down
        move through enabled items

    Enter / Return
        activate selected item

    Space
        activate selected item where supported

    Escape
        close menu without action

Disabled items must be skipped or harmless.

Do not require tooltips for disabled actions in this ticket.

## File menu keyboard behaviour

Audit the existing File command menu.

Verify it can be operated without the mouse:

- open through its existing keyboard path if one exists
- otherwise make the File command reachable through normal focus traversal
- Up / Down navigate
- Enter activates
- Escape closes
- disabled items behave safely

Do not add a traditional menu bar.

Do not add Alt+F-style application menus unless one already follows naturally
from Qt and the existing UI.

## Footer command strip focus

Review:

- + NOTE
- + CHILD
- SEARCH
- FILE

They should:

- accept keyboard focus
- show visible focus
- activate with Enter / Space
- preserve existing mouse behaviour

Do not change their visual language.

Do not add icons.

## Stale hover state

Fix the known issue where a tree row may retain hover tint after:

- opening a context menu
- activating a context menu item
- closing a context menu
- pointer leaving during popup interaction

The hover state must reflect actual pointer position.

Do not solve this with arbitrary timers.

Prefer correct pointer/hover-state ownership or explicit reset when the popup
opens/closes.

Check the same class of issue in:

- command-strip items
- File menu
- dialogs
- Search results

Fix only genuine stale-state cases found.

## Tree keyboard audit

Preserve and verify all existing tree keyboard behaviour.

At minimum:

    Up / Down
        navigate visible rows

    Left / Right
        ordinary tree expand/collapse/navigation

    Ctrl+Left / Ctrl+Right
        recursive collapse/expand

    Ctrl+N
        new root note

    Ctrl+Shift+N
        new child note

    F2
        rename

    Delete
        Move to Trash flow

    Alt+Up / Alt+Down
        sibling movement

    Shift+F10 / Menu
        context menu

Do not change existing shortcuts unless there is an actual conflict.

## Search keyboard audit

Verify existing Search behaviour still works:

    Ctrl+F
    Up
    Down
    Enter
    Escape

Search result activation must still:

- reveal the note
- select it
- expand ancestors
- close Search

No visual/keyboard polish in this ticket should change Search semantics.

## Markdown keyboard audit

Verify:

    Ctrl+E
        Edit / Preview toggle

When returning from Preview to Edit:

- body focus returns sensibly
- cursor/selection preservation remains intact

Do not add Ctrl+B or Ctrl+I in this ticket.

## Text editor shortcut audit

Verify the raw body editor behaves like a normal desktop text editor.

At minimum:

    Ctrl+Z
        undo

    Ctrl+Shift+Z
        redo where Qt supports it

    Ctrl+X
        cut

    Ctrl+C
        copy

    Ctrl+V
        paste

    Ctrl+A
        select all

Also verify:

- Shift+Arrow selection
- Home / End
- Ctrl+Home / Ctrl+End
- Page Up / Page Down where applicable

Do not replace Qt's normal text-editing machinery.

If the current TextArea/TextEdit already provides these correctly, add no
custom implementation.

The purpose is to catch accidental shortcut interception by application-level
Shortcuts.

## Title editor shortcut audit

Perform the same sanity check for the title field:

- Ctrl+A
- Ctrl+C
- Ctrl+X
- Ctrl+V
- Ctrl+Z
- selection with Shift

Application shortcuts must not steal ordinary editing behaviour while a text
field is focused.

## Shortcut conflict policy

Application-wide shortcuts must not fire when the user is performing a
standard text-editing action that uses the same key combination.

If a conflict exists:

- prefer conventional text editing while the text control is focused
- preserve the application shortcut elsewhere

Do not invent unusual replacement shortcuts unless necessary.

## Focus traversal

Audit Tab / Shift+Tab through the main interface.

It should be possible to move sensibly among interactive controls without
getting trapped.

Do not require a perfect cyclical order through every tree row.

Prioritise:

- tree
- footer command strip
- title
- Edit/Preview
- Delete
- body editor

Search mode and dialogs should have their own sensible focus path.

## Focus visual consistency

Every keyboard-focusable command should have a visible focus state using the
existing semantic visual language.

Do not add new colours.

Use existing:

- Theme.accent
- Theme.danger
- Theme.border
- current Command styling

Do not make focus outlines visually loud.

## Mouse regression

Keyboard fixes must not break:

- ordinary clicks
- double clicks where used
- drag/drop
- disclosure clicks
- Ctrl+click recursive expansion
- context-menu right click
- Search result click
- Markdown link click

## View-state persistence regression

Ticket 018 expansion persistence must remain unchanged.

Verify:

- expanding/collapsing alone does not set content dirty
- Ctrl+S persists view state
- reopening restores expansion
- context-menu expand/collapse updates live expansion state
- keyboard recursive expansion also updates view state

## Theme regression

Perform interaction checks in:

- current dark Omarchy theme
- one light theme

Verify:

- focus remains visible
- danger/default dialog actions remain understandable
- disabled menu items remain legible
- hover cleanup works in both themes

Do not add theme-specific exceptions.

## Tests

Preserve all existing tests.

Add permanent unit/runtime coverage where practical for at least:

1. Enter activates a safe/default dialog action
2. Escape cancels/dismisses a dialog
3. Space activates the focused dialog command
4. destructive dialog defaults remain safe
5. Shift+F10 opens the tree context menu
6. context menu uses current selection
7. context menu can open with no selected node
8. keyboard menu navigation activates an item
9. Escape closes context menu without action
10. File menu keyboard navigation works
11. footer Commands activate through keyboard
12. stale tree hover clears after popup interaction
13. ordinary tree shortcuts remain connected
14. Search keyboard flow remains connected
15. Ctrl+E remains connected
16. body editor Ctrl+Z works
17. body editor Ctrl+C/X/V/A are not intercepted
18. title editor standard editing shortcuts are not intercepted
19. expansion persistence still works after keyboard expand/collapse
20. no document mutation occurs merely from menu/dialog navigation

Where QML runtime behaviour cannot be meaningfully unit-tested, use a temporary
QtTest harness for this ticket.

The permanent integration suite is still Ticket 020.

## Runtime verification

On a real desktop, exercise:

### Dialogs

- Move to Trash:
  - keyboard only
  - Escape
  - Tab
  - Shift+Tab
  - Space
  - Enter

- unsaved changes dialog
- overwrite/replace dialog if practical with temporary files

Verify safe defaults.

### Tree

- Shift+F10
- Menu key if available
- context-menu navigation
- right-click context menu
- Ctrl+click expansion
- Ctrl+Left / Ctrl+Right
- Delete
- F2
- Alt+Up / Alt+Down

### Editor

- title copy/paste/undo/select-all
- body copy/paste/undo/redo/select-all
- cursor navigation
- Ctrl+E

### Search

- Ctrl+F
- keyboard-only result selection and activation

### Hover

Specifically reproduce the old sequence:

1. hover a tree row
2. open context menu
3. move pointer away
4. close or activate menu

The old row must not remain highlighted.

Check File/command hover state similarly.

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo build

Perform:

- clean offscreen launch
- real-desktop keyboard interaction pass
- dark/light theme focus verification

No QML warnings or binding loops.

## Completion criteria

Ticket 019 succeeds when OmaTree can be operated through its normal workflows
without surprising transitions from keyboard to mouse.

The application should feel consistent rather than feature-rich.

Known awkward interactions should be removed without growing another layer of
UX complexity.

## Out of scope

Do not:

- add features
- add icons
- add tooltips for every disabled action
- add recent files
- remember window geometry
- remember splitter width
- add settings
- change Markdown features
- change Search semantics
- change schema
- add new persistence state
- implement permanent integration-test infrastructure
- perform packaging
- redesign the visual style
- perform unrelated cleanup