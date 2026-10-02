# Ticket 005 — Basic two-pane OmaTree UI

## Goal

Build the first functional OmaTree user interface.

The application should now visually behave like a simple tree-structured
notebook:

- notes/tree on the left
- selected note editor on the right

Use the existing `NotebookModel`.

This ticket is UI and interaction only.

Do not connect SQLite persistence yet.

## Overall layout

Replace the bootstrap placeholder UI with a simple two-pane layout.

Left pane:

- hierarchical `TreeView`
- displays note titles
- visually indicates the selected note
- supports arbitrary nesting
- scrolls when necessary

Right pane:

- displays the selected note title
- editable plain-text note body
- fills the available space
- scrolls when necessary

Use Qt Quick / QML only.

Do not introduce Qt Widgets.

## Initial state

A newly launched application begins with an empty notebook.

The empty state should be understandable rather than looking broken.

Provide an obvious way to create the first root note.

Do not insert fake/sample notes automatically.

## Selection

Maintain one selected note in QML using the QModelIndex supplied by the
TreeView / NotebookModel.

When selection changes:

- display the selected node's title
- load its body using `NotebookModel.body(index)`

When there is no selection:

- the editor is disabled or otherwise clearly empty
- no stale note contents remain visible

Deletion of the selected node must leave the UI in a valid state.

Do not expose NodeId to QML.

## Editing note bodies

The right-hand editor edits plain text only.

When the user changes the body, update the in-memory Notebook through
`NotebookModel.setBody(index, text)`.

Avoid accidental write-back loops when changing selection and loading a new
body into the editor.

No Markdown rendering is required.

No autosave or SQLite persistence is required.

## Node creation

Provide UI actions for:

- create root note
- create child of selected note

New notes may use a simple default title such as:

`New note`

After creation:

- select the newly created note where practical
- make it easy to rename immediately

Do not implement complex title-generation logic.

## Rename

Provide a simple way to rename the selected node.

Choose the simplest clean interaction supported by QML.

Acceptable approaches include:

- inline tree editing
- a small rename action/input
- a lightweight dialog

Do not create an elaborate modal workflow.

## Delete

Provide a way to delete the selected node.

Deleting a node deletes its subtree using the existing model operation.

For this ticket, a simple confirmation dialog is appropriate because subtree
deletion is destructive.

After deletion:

- selection must not reference the deleted QModelIndex
- the editor must not display deleted note content
- the TreeView must remain valid

## Controls

Keep controls minimal.

The interface should not become a toolbar-heavy traditional desktop editor.

At minimum provide access to:

- new root
- new child
- rename
- delete

These may be small buttons, contextual actions, or a simple compact menu.

Choose the least visually intrusive implementation.

## Visual direction

This is not the final Omarchy theme ticket.

However, the UI should already be deliberately minimal and clean.

Prefer:

- restrained spacing
- clear hierarchy
- minimal borders
- no unnecessary toolbars
- no decorative gradients
- no excessive icons
- no complex panels

Use standard Qt/system colours for now.

Do not hard-code an Omarchy-specific palette yet.

The design should make later theme substitution straightforward.

## Window

Use an application-appropriate Qt Quick root component.

If changing from `Window` to `ApplicationWindow` is useful for actions,
shortcuts or dialogs, it is allowed in this ticket.

Retain a sensible default window size.

The application must resize correctly.

## Model ownership

There must still be exactly one `NotebookModel` instance shared by:

- the tree
- selection handling
- editor body access
- node actions

Do not create duplicate models.

## Testing / runtime verification

Existing Rust tests must continue to pass.

In addition, manually/runtime verify at least:

1. empty application launches correctly
2. root note creation appears immediately
3. child creation appears beneath its parent
4. several nesting levels display correctly
5. selecting notes switches editor contents
6. body edits are retained when switching between notes
7. rename updates the tree immediately
8. deleting a leaf works
9. deleting a node with children removes the subtree
10. deleting the selected note clears or safely changes selection
11. resizing the window behaves correctly
12. no QML runtime warnings/errors are produced

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Launch normally on the graphical desktop and verify the actual interface.

Also perform an offscreen launch if practical to catch QML load errors.

## Out of scope

Do not:

- connect SQLite storage
- implement open/save notebook workflows
- implement autosave
- implement drag and drop
- implement reparenting/reordering
- implement search
- implement Markdown rendering
- implement Omarchy theme loading
- implement custom fonts
- implement import/export
- implement settings/preferences
- add unrelated dependencies
- perform unrelated cleanup