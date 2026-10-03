# Ticket 015 — New Notebook and document workflow

## Goal

Add a proper New Notebook command.

The user must be able to replace the current document with a fresh empty
Untitled notebook without restarting OmaTree.

New Notebook must respect all existing:

- autosave behaviour
- unsaved-change safety
- Save / Save As workflows
- recovery behaviour
- search state
- QModelIndex safety

Do not create a file automatically.

## User interface

Add:

    File ▸ New Notebook

to the existing compact File menu.

Place it before Open.

Do not add a toolbar.

Do not assign Ctrl+N.

Ctrl+N is already used for "New note" and is more useful during ordinary
OmaTree usage.

Do not add another keyboard shortcut in this ticket.

## Result of New Notebook

After New Notebook succeeds, the application must contain a completely fresh:

- empty Notebook
- empty Trash
- empty checkpoint history
- fresh node-id allocation state
- no current filesystem path
- clean dirty state

The window title becomes:

    OmaTree — Untitled

No file is created.

Autosave does not run because the new document has no path.

The empty-state UI appears exactly as it does when OmaTree is launched without
a notebook path.

## Document architecture

Add the smallest clean pure-Rust operation for creating/resetting to a fresh
Document.

Conceptually:

    Document::new()

already represents the desired state.

If a new helper is needed, keep it small.

NotebookModel should replace the current Document using the proper Qt model
reset sequence.

Do not mutate the old Notebook node-by-node to simulate a new document.

A document replacement is one of the valid uses of:

    beginResetModel
    endResetModel

## Selection safety

Before or during the model reset:

- clear the current selection
- ensure no stale QModelIndex survives
- clear the editor contents

After New Notebook:

- no tree node is selected
- the editor shows the existing empty/no-selection state

Do not attempt to preserve selection between documents.

## Search state

If Search mode is active when New Notebook is requested:

- leave Search mode
- clear the query
- clear search results
- restore the normal tree pane

The new document must not inherit search state from the previous one.

Do not let search-result indexes survive the model reset.

## Recovery state

The new document starts with:

- empty Trash
- zero checkpoints

Do not carry recovery history between notebooks.

Do not create a checkpoint merely because the user created a new document.

Switching documents is not a structural mutation of either notebook.

## Autosave timer

Before starting the New Notebook workflow:

- cancel any pending autosave timer

The current document must be safely handled before it is replaced.

After the new Untitled document is created:

- no autosave timer should be pending
- ordinary edits mark it dirty
- autosave remains disabled until Save As gives it a path

Do not create an implicit file.

## Clean current document

If the current document is clean:

- replace it immediately with the new Untitled document
- show no confirmation dialog

This applies to both:

- file-backed clean documents
- clean Untitled documents

## Dirty file-backed document

If the current document is file-backed and dirty:

1. cancel the pending autosave debounce
2. attempt one immediate save

If the save succeeds:

- create the new Untitled document immediately
- do not show an unsaved-changes dialog

If the save fails:

reuse the existing safe-to-leave workflow:

- Save
- Discard
- Cancel

### Save

Attempt the explicit save again.

If it succeeds:

- create the new Untitled document

If it fails:

- remain in the current document
- report the save failure through the existing explicit-save error path

### Discard

Create the new Untitled document without saving the in-memory changes.

The existing file on disk remains untouched.

### Cancel

Abort New Notebook.

Leave the current document completely unchanged.

If appropriate, restore normal autosave behaviour for the still-open document.

## Dirty Untitled document

If the current document is Untitled and dirty:

reuse the existing safe-to-leave workflow:

- Save As
- Discard
- Cancel

### Save As

Run the existing Save As workflow.

Only if Save As completes successfully:

- create the new Untitled document

If Save As is cancelled or fails:

- abort New Notebook
- preserve the current document

### Discard

Create the new Untitled document.

The unsaved in-memory document is intentionally discarded.

### Cancel

Abort New Notebook and leave everything unchanged.

## Reuse existing safety workflow

Do not create a second independent unsaved-changes dialog.

Reuse the existing safe-to-leave / continuation logic already used by:

- Open
- Close

Extend it cleanly for New Notebook.

Open, Close and New should share the same document-leaving semantics wherever
possible.

Do not duplicate a large state machine in QML.

## Dialog wording

The existing generic unsaved-changes dialog may be reused if the wording still
makes sense.

If small wording changes are necessary so it works naturally for:

- Open
- Close
- New Notebook

keep them generic.

Do not add separate dialogs solely for New.

## File menu order

Use a sensible compact ordering such as:

    New Notebook
    Open…
    Save
    Save As…
    Recovery…

Do not add a traditional desktop menu bar.

Keep the existing small File control.

## Failed New operation

There is no meaningful way for constructing a fresh in-memory Document to
fail under normal conditions.

However, if any model-reset or Qt-facing operation encounters an invalid
state:

- do not leave QML holding stale indexes
- prefer a clean empty selection
- never partially mix nodes from two documents

Do not add elaborate error handling for impossible allocation failures.

## Theme behaviour

The current theme is application state, not document state.

Creating a new notebook must NOT:

- change the theme
- reload Omarchy unnecessarily
- alter semantic palette state

The new document should immediately use the current active theme.

## Search behaviour after New

Ctrl+F on the new empty document should behave normally:

- search opens
- empty query state is shown
- there are no results

No previous search query is retained.

## Dirty state after New

Immediately after creating the new document:

    dirty = false

Creating the first note later:

    dirty = true

Because the document has no path:

- no autosave occurs
- Ctrl+S invokes Save As as before

After successful Save As:

- dirty becomes false
- later edits autosave normally

## Tests — Document/model

Preserve all existing tests.

Add tests covering at least:

1. fresh Document is empty
2. fresh Document has no path
3. fresh Document is clean
4. fresh Document has empty Trash
5. fresh Document has zero checkpoints
6. fresh Document starts with fresh node-id allocation
7. replacing a populated document yields an empty document
8. replacing a document does not mutate the old saved file
9. creating a note in the new Untitled document marks it dirty

## Tests — safe New workflow

Cover at least:

10. clean file-backed document creates New immediately
11. clean Untitled document creates New immediately
12. dirty file-backed document is saved before New
13. successful save-flush proceeds to New without confirmation
14. failed save-flush enters the existing safe-to-leave workflow
15. Save success after failed flush proceeds to New
16. Save failure after failed flush keeps current document
17. Discard after failed flush creates New without altering the old disk file
18. Cancel keeps the current document unchanged
19. dirty Untitled Save As success proceeds to New
20. dirty Untitled Save As cancellation aborts New
21. dirty Untitled Save As failure aborts New
22. dirty Untitled Discard creates New
23. dirty Untitled Cancel keeps current document

## Tests — QML/runtime

Cover where practical:

24. File menu contains New Notebook
25. New clears tree selection
26. New clears editor contents
27. New exits Search mode
28. New clears search query/results
29. New clears Recovery/Trash state
30. New title becomes "OmaTree — Untitled"
31. no autosave timer remains pending after New
32. Ctrl+N still creates a note, not a notebook
33. Ctrl+Shift+N still creates a child
34. theme remains unchanged
35. first note creation after New behaves normally
36. Ctrl+S after editing the new Untitled document invokes Save As
37. after Save As, later edits autosave normally
38. no stale QModelIndex survives reset
39. no QML warnings or binding loops occur

Use temporary files only.

Never modify real user notebooks during automated testing.

## Runtime verification

Exercise at least:

- New from a clean saved notebook
- New from a dirty saved notebook
- failed save before New
- Save / Discard / Cancel paths
- New from dirty Untitled
- Save As / Discard / Cancel paths
- New while Search is open
- New after using Recovery
- create notes in the new document
- Save As the new document
- autosave after Save As
- Omarchy theme remains active
- no QML warnings or binding loops

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Perform a clean offscreen launch.

Perform runtime verification of the New Notebook workflow.

## Out of scope

Do not:

- implement Markdown rendering
- add a New Notebook keyboard shortcut
- implement recent notebooks
- implement templates
- implement multiple documents/windows
- implement tabs
- automatically name/save new notebooks
- add default notebook locations
- add a welcome/start screen
- implement session restore
- change SQLite schema
- redesign the UI
- perform unrelated cleanup