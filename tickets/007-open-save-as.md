# Ticket 007 — Open and Save As

## Goal

Complete OmaTree's basic document workflow.

The user must be able to:

- open an existing notebook through a native Qt file dialog
- save an Untitled notebook using Save As
- save the current notebook under a different name/path
- safely switch between notebooks without losing unsaved work

Keep the UI deliberately minimal.

Do not implement autosave or recent files yet.

## File dialogs

Use Qt Quick's supported native-capable file dialog APIs.

Do not implement a custom file browser.

Use `.omatree` as the preferred notebook extension.

Open dialogs should primarily show:

    *.omatree

but provide access to all files if appropriate.

When saving, if the chosen filename has no extension, append:

    .omatree

Do not rewrite an explicitly supplied different extension.

## Open

Add:

    Ctrl+O

and an unobtrusive visible Open action.

Opening a notebook must use the existing Document / Storage architecture.

On successful open:

- replace the current Notebook
- clear the previous selection safely
- update the tree
- update the document name
- update the current path
- clear dirty state

Opening must remain transactional:

- build/load the new document first
- only replace the current document after successful validation
- if opening fails, leave the existing document completely unchanged

A failed open must never overwrite the target file.

Display a short human-readable error.

## Save As

Add:

    Ctrl+Shift+S

and an unobtrusive visible Save As action.

Save As must work for both:

- Untitled notebooks
- already file-backed notebooks

On successful Save As:

- save the current Notebook to the selected path
- associate the Document with the new path
- update the window title
- clear dirty state

The previous notebook file must remain unchanged when saving under a new name.

## Saving to a new path

If the selected target does not exist:

- create a new OmaTree SQLite database there
- save the current Notebook into it
- switch the Document association only after the save succeeds

If creation or save fails:

- keep the current path/document association unchanged
- keep dirty state unchanged
- do not leave the application believing the failed target is current

## Saving over an existing file

Never silently destroy an existing file.

If the Save As target already exists:

- require explicit user confirmation before replacing its notebook contents

If the target is an existing valid OmaTree notebook:

- after confirmation, open/validate it as an OmaTree database
- transactionally save the current Notebook into it
- switch the current Document association only after success

If the existing target is not a valid supported OmaTree notebook:

- refuse to overwrite it
- show a human-readable error
- leave the target byte-for-byte untouched

Do not overwrite arbitrary non-OmaTree files.

If the selected Save As path is already the current notebook path, treat it as
a normal Save rather than an overwrite operation.

## Unsaved changes before Open

Opening another notebook while the current notebook is dirty must prompt the
user first.

For a file-backed dirty notebook, offer:

- Save
- Discard
- Cancel

Save:
- save the current notebook
- continue with Open only if the save succeeds

Discard:
- continue to the Open dialog without saving

Cancel:
- abort the Open operation

For a dirty Untitled notebook, offer:

- Save As
- Discard
- Cancel

Save As:
- run the Save As workflow
- continue with Open only if Save As succeeds

Discard:
- continue to Open without saving

Cancel:
- abort

Do not clear or mutate the current document merely because the user chose
Discard. The existing document should only actually disappear after another
notebook has successfully opened.

## Close behaviour

Improve the existing dirty-close workflow.

File-backed dirty notebook:

- Save
- Discard
- Cancel

Untitled dirty notebook:

- Save As
- Discard
- Cancel

Save / Save As must close the application only after successful persistence.

A cancelled or failed save must leave OmaTree open.

## Save action

Keep the existing:

    Ctrl+S

behaviour.

For a file-backed notebook:

- save normally

For an Untitled notebook:

- Ctrl+S should now invoke Save As rather than returning an error

This makes Save work naturally without requiring the user to know the
difference.

## Minimal UI

Do not add a traditional desktop menu bar.

Provide one compact, unobtrusive way to discover document actions.

For example, a small flat menu/button containing:

- Open
- Save
- Save As

Choose whatever fits the current UI with the least visual clutter.

Do not redesign the rest of OmaTree.

## Architecture

Keep:

    Document
        owns Notebook
        owns optional Storage
        owns optional path
        owns dirty state

and:

    NotebookModel
        Qt/QML adapter
        coordinates Document operations

SQLite logic must remain in `storage.rs`.

Do not move SQL into QML or NotebookModel.

Add the minimum Document APIs necessary for Save As.

Do not introduce a generic document-manager/service abstraction.

## State safety

Pay particular attention to state transitions.

After any failed:

- open
- save
- save as
- overwrite attempt

the existing in-memory Notebook, current path, document name and dirty state
must remain correct.

Do not partially switch documents.

## Tests

Preserve all existing tests.

Add tests covering at least:

1. Save As from Untitled to a new path
2. saved content survives reopening
3. Save As changes the current path
4. Save As clears dirty only after success
5. Save As failure preserves dirty state and current path
6. Save As from one file to another leaves the original unchanged
7. Save As to the current path behaves like Save
8. explicit overwrite of an existing valid OmaTree notebook works
9. overwrite failure leaves the existing target valid and unchanged
10. attempting to overwrite a non-OmaTree file is refused and leaves it unchanged
11. open failure leaves the current Document unchanged
12. opening another valid notebook replaces the Document only after success
13. Ctrl+S on Untitled routes through Save As
14. dirty Open workflow correctly handles Save / Discard / Cancel
15. dirty Untitled Open workflow correctly handles Save As / Discard / Cancel
16. dirty Untitled close supports Save As / Discard / Cancel

Use temporary files and directories.

Never modify real user notebook files during tests.

Qt/QML workflows that cannot reasonably run in cargo test may be exercised
through a temporary QML/QtTest harness.

Do not weaken application architecture solely for test convenience.

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Also perform offscreen runtime verification of:

- Open
- Save
- Save As
- overwrite confirmation
- failed open
- failed overwrite
- dirty switching workflow
- dirty close workflow

Verify there are no QML runtime warnings or binding loops.

## Out of scope

Do not:

- implement autosave
- implement recent files
- implement New Notebook
- implement multiple open windows
- implement tabs
- implement backups
- implement incremental SQLite persistence
- change the SQLite schema
- implement import/export
- implement drag and drop
- implement reparenting/reordering
- implement search
- implement Markdown rendering
- implement Omarchy theming
- implement preferences/settings
- redesign the main interface
- perform unrelated cleanup
- add unrelated dependencies