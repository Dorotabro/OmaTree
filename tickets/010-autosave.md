# Ticket 010 — Debounced autosave

## Goal

Make saving effectively invisible during normal OmaTree use.

For file-backed notebooks, every real change should automatically save after a
short period of inactivity.

The user should normally be able to:

- type
- create notes
- rename notes
- delete or restore notes
- close OmaTree

without thinking about Ctrl+S.

Existing manual Save, Save As, recovery, Trash and safe document-switching
behaviour must remain available.

## Autosave interval

Use a fixed debounce interval of:

    1000 ms

The interval is deliberately not configurable.

When another successful mutation occurs before the timer expires:

- restart the timer
- do not save yet

This means rapid typing should result in one save approximately one second
after typing stops.

Do not implement periodic timer-based saving.

Autosave is mutation-driven and debounced.

## Mutation notification

Dirty state alone is NOT sufficient to trigger autosave.

Once a document is dirty, additional edits may not change the `dirty`
property, but they must still restart the debounce timer.

Expose one Qt signal from NotebookModel representing:

    a real successful document mutation occurred

The exact signal name is flexible, for example:

    documentMutated()
    contentChanged()
    mutationOccurred()

It must fire after every successful mutation that actually changes document
state.

It must cover:

- create root
- create child
- rename
- body edit
- delete / move to Trash
- restore from Trash
- restore checkpoint

It must NOT fire for:

- unchanged title
- unchanged body
- failed operation
- opening dialogs
- selection changes
- merely viewing recovery history

Do not expose NodeId or mutation details to QML solely for autosave.

## Debounce implementation

Use one QML Timer associated with the main document.

Preferred behaviour:

    mutation signal
        ↓
    restart 1000 ms timer
        ↓
    timer expires
        ↓
    attempt autosave

Do not create one timer per editor or operation.

Do not add async Rust, threads, Tokio or another runtime.

The existing synchronous save path is sufficient.

## Save scope

Autosave must call the existing optimized Document save path.

Therefore:

Ordinary text/title/create edits:

    active-only save

Delete / Trash restore / checkpoint restore:

    full save

Schema-v1 first save:

    full transactional v1 → v2 migration

Do not duplicate save-scope decisions in QML.

Document::save remains responsible for deciding what kind of persistence is
required.

## File-backed notebooks

Autosave applies only when the current Document has a filesystem path.

When a file-backed document changes:

- mark dirty as before
- restart the debounce timer
- save when the timer expires

On successful save:

- clear the appropriate dirty state
- update the existing public dirty property
- the `*` disappears from the title

Do not show a "Saved" toast or status message.

Successful autosave should be silent.

## Untitled notebooks

Untitled notebooks have no save destination.

For Untitled notebooks:

- mutations still mark dirty
- mutation notifications still occur
- do NOT start an autosave timer
- do NOT automatically open Save As
- do NOT create any file
- Ctrl+S continues to invoke Save As
- close/open workflows retain Save As / Discard / Cancel

After a successful Save As gives the Document a path:

- subsequent mutations autosave normally

## Manual Save

Ctrl+S remains an immediate save.

For a file-backed notebook:

- stop/cancel any pending debounce timer
- save immediately
- on success clear dirty as normal
- do not perform another redundant timer save afterward

For an Untitled notebook:

- keep the existing Save As behaviour

Manual Save must remain available as an explicit retry after autosave failure.

## Save As

Before starting Save As:

- cancel any pending debounce timer

If Save As succeeds:

- the Document becomes clean
- reset autosave failure suppression
- subsequent mutations use normal autosave

If Save As is cancelled:

- do not modify document state
- if the current document is file-backed and still dirty, schedule autosave
  again where appropriate

If Save As fails:

- keep current path and dirty state unchanged
- report the failure through the existing Save As error flow
- do not accidentally trigger a second autosave error dialog

## Open another notebook

Before switching away from a dirty file-backed notebook:

1. cancel the debounce timer
2. attempt one immediate save

If save succeeds:

- continue directly to Open
- do not show the unsaved-changes confirmation

If save fails:

- preserve the existing Save / Discard / Cancel safety workflow

For dirty Untitled notebooks:

- preserve the existing Save As / Discard / Cancel workflow

If Open is cancelled and the original file-backed notebook remains dirty,
autosave may be rescheduled.

A successful Open resets autosave failure suppression.

## Close behaviour

When closing a dirty file-backed notebook:

1. cancel any pending debounce timer
2. attempt one immediate save

If the save succeeds:

- close immediately
- do not show the unsaved-changes dialog

If the save fails:

- keep the application open
- preserve the existing Save / Discard / Cancel safety workflow
- never discard automatically

Dirty Untitled notebooks retain:

- Save As
- Discard
- Cancel

If closing is cancelled and the document remains open, normal autosave
behaviour should remain usable.

## Autosave failure handling

Autosave can fail.

Examples include:

- filesystem permissions changed
- storage disappeared
- SQLite error
- disk full
- structural full save failed

On autosave failure:

- keep the Document dirty
- preserve all in-memory content
- show one short human-readable error message
- explain that changes remain in memory
- allow Ctrl+S or Save As to retry

Do NOT repeatedly show the same error dialog after every autosave attempt.

Maintain one failure-suppression state for the current Document.

After the first autosave failure:

- later mutations may still schedule autosave attempts
- autosave may retry silently
- repeated failures do not produce repeated dialogs

If a later automatic retry succeeds:

- clear dirty state
- clear failure suppression
- resume normal behaviour

Also clear failure suppression after:

- successful manual Save
- successful Save As
- successful Open of another notebook

A manual Ctrl+S failure should still show its normal explicit save error,
because the user deliberately requested a save.

## No overlapping saves

There must never be two simultaneous saves.

The current persistence path is synchronous, so keep this simple.

Do not introduce concurrency merely for autosave.

A timer save, Ctrl+S, Save As, Open-flush or Close-flush must not create
duplicate persistence operations.

## Recovery guarantees

Autosave must preserve everything implemented in Tickets 008 and 009.

In particular:

- delete still creates a checkpoint before moving to Trash
- Trash persists
- checkpoint history persists
- active-only saves do not touch recovery tables
- recovery-dirty saves remain full atomic saves
- Save As includes all recovery state
- autosave must never weaken transactional guarantees

Autosave is not a replacement for recovery.

Recovery is what makes autosave safe.

## Window title

Keep the existing title convention:

    OmaTree — notes.omatree
    OmaTree — notes.omatree *

The `*` may briefly appear while autosave is waiting.

After successful autosave it disappears.

Do not add:

- save spinner
- status bar
- "Saved" toast
- autosave icon

Quiet is the goal.

## Tests

Preserve all existing tests.

Add tests/runtime verification covering at least:

1. a real mutation emits the autosave/mutation signal
2. unchanged body does not emit it
3. unchanged title does not emit it
4. failed mutation does not emit it
5. ordinary edit marks the document dirty
6. autosave occurs after the debounce interval
7. multiple rapid body edits result in one final save
8. rapid rename/body edits restart the same debounce timer
9. successful autosave clears dirty
10. body text survives close/reopen after autosave
11. create root triggers autosave
12. create child triggers autosave
13. rename triggers autosave
14. delete triggers a full autosave including recovery state
15. Trash restore triggers a full autosave
16. checkpoint restore triggers a full autosave
17. active-only autosave does not write recovery tables
18. Untitled mutation creates no file and performs no save
19. successful Save As enables autosave for later edits
20. Ctrl+S saves immediately without waiting for the timer
21. Ctrl+S cancels a pending redundant timer save
22. Open flushes pending changes before switching documents
23. successful Open resets autosave failure suppression
24. Close flushes pending changes and exits after successful save
25. failed close-flush does not lose data
26. failed autosave leaves dirty true
27. failed autosave preserves in-memory content
28. first autosave failure displays an error
29. repeated autosave failures do not repeatedly display errors
30. later successful autosave clears failure suppression
31. successful manual Save clears failure suppression
32. successful Save As clears failure suppression
33. schema-v1 notebook autosave performs the existing transactional migration
34. recovery history remains intact after ordinary autosaves
35. no QML warnings or binding loops occur

For debounce tests, use a temporary QML/QtTest harness where appropriate.

Use temporary notebook files only.

Never modify real user notebook files during tests.

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Perform offscreen runtime verification of:

- rapid typing debounce
- active-only autosave
- structural/full autosave
- Untitled behaviour
- Save As → later autosave
- Ctrl+S with pending autosave
- Open with pending autosave
- Close with pending autosave
- autosave failure suppression
- recovery data surviving autosave

Verify there are no QML warnings, binding loops or repeated error dialogs.

## Out of scope

Do not:

- implement incremental per-node SQL updates
- change SQLite schema version
- add database tables
- implement undo/redo
- implement drag and drop
- implement move/reparent/reorder
- add configurable autosave timing
- add async Rust
- add threads or an async runtime
- add a status bar
- add save notifications
- implement backups outside the notebook file
- implement recent notebooks
- implement search
- implement Markdown rendering
- implement Omarchy theming
- redesign the UI
- perform unrelated cleanup