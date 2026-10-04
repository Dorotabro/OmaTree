# Ticket 025 — New notes always enter Edit mode

## Goal

Fix one small Markdown Preview workflow problem.

Currently Edit / Preview mode is preserved when selection changes.

That is desirable when browsing existing notes, but it causes a bad creation
workflow:

1. user is viewing an existing note in Preview
2. user presses Ctrl+N or Ctrl+Shift+N
3. the new note is created and its title receives focus
4. the editor remains in Preview mode
5. after naming the note, the user cannot immediately type its body

A newly created note must always begin in Edit mode.

## Required behaviour

The following creation flows must force the editor to Edit mode after the note
is successfully created:

- Ctrl+N — New note
- Ctrl+Shift+N — New child
- + NOTE
- + CHILD
- "create first note" empty-state action, if present
- any other UI path that invokes the same note-creation operations

The newly created note should otherwise retain the current creation behaviour:

- it becomes selected
- its title receives focus
- title editing works exactly as before
- normal body editing works after title entry

## Existing-note behaviour

Do NOT change the existing behaviour when merely selecting/browsing notes.

If Preview mode is active:

- selecting another existing note stays in Preview
- activating a Search result stays in Preview
- Recovery/restored-note navigation behaves as before

The special rule applies only to successful creation of a brand-new note.

## Failed creation

If note creation fails for any reason:

- do not change Edit / Preview mode
- preserve current selection
- preserve focus
- show the existing error behaviour

Only a successful creation forces Edit mode.

## Document replacement

Preserve existing behaviour:

- New Notebook resets to Edit
- Open resets to Edit

No change is required there.

## Dirty / persistence semantics

Switching Preview -> Edit as part of note creation is UI state only.

It must not independently:

- mark the Document dirty
- emit another document mutation
- create another checkpoint
- trigger another autosave
- alter Recovery state

The note creation itself retains its existing mutation semantics.

## Focus

Do not move focus directly to the body.

The current creation workflow intentionally focuses the new note's title.

Preserve that.

The important requirement is simply that the body is now in Edit mode and
ready once the user proceeds from the title.

## Permanent integration regression

Add this to the permanent integration suite.

Cover at least:

1. select an existing note
2. enter Preview mode
3. Ctrl+N
4. new note is selected
5. editor is now in Edit mode
6. title has the expected creation focus
7. raw body can be edited normally
8. repeat for Ctrl+Shift+N
9. repeat for mouse/footer note creation where practical
10. selecting an existing note while Previewing still preserves Preview
11. failed creation does not alter mode
12. no extra mutation/checkpoint/autosave is caused by the mode switch itself

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo test integration_tests
- cargo build

Perform a clean offscreen launch.

Manually verify:

Preview existing note -> Ctrl+N -> name note -> type body

and:

Preview existing note -> Ctrl+Shift+N -> name child -> type body

## Out of scope

Do not:

- change Markdown rendering
- change Search
- change creation naming rules
- change title focus behaviour
- add shortcuts
- redesign the editor header
- perform packaging
- perform unrelated cleanup