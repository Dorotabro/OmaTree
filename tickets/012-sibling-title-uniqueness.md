# Ticket 012 — Sibling title uniqueness and collision safety

## Goal

Prevent ambiguous duplicate note titles among siblings.

OmaTree must enforce this rule:

    Within one parent, sibling note titles must be unique.

Root notes are siblings of one another and follow the same rule.

The same title MAY appear in different branches.

Example — valid:

    Work
      Ideas

    Personal
      Ideas

Example — invalid:

    Work
      Ideas
      Ideas

This rule must be enforced in the Rust model, not only in QML.

## Title equivalence

Sibling-title comparison is:

- case-insensitive
- insensitive to leading/trailing whitespace

Examples that conflict:

    Ideas
    ideas
    IDEAS
    " Ideas "
    "ideas   "

Leading and trailing whitespace must be trimmed before storing a newly
committed title.

Internal whitespace is preserved.

For example:

    "Project  Ideas"

is not automatically rewritten to:

    "Project Ideas"

Do not add Unicode normalization libraries.

Use a simple documented normalization strategy based on trimmed Rust strings
and Unicode-aware lowercase conversion available in the standard library.

## Empty titles

A title that becomes empty after trimming is invalid.

Examples:

    ""
    "     "

Creation through the UI should never produce an empty title.

Rename to an empty title must fail cleanly and preserve the previous title.

## Model authority

Add small reusable Notebook helpers for sibling-title validation.

Conceptually, the model needs to answer:

    Does this parent already contain an equivalent sibling title?

with support for excluding one NodeId during rename or same-node checks.

The exact API is flexible.

Do not duplicate title-collision rules in QML, Document and NotebookModel.

Notebook must remain the authority.

## Creating notes

The strict Notebook create operations must reject duplicate sibling titles.

However, the UI's normal "New note" workflow should remain frictionless.

When the user creates a note, automatically choose the first available title:

    New note
    New note 2
    New note 3
    New note 4
    ...

Comparison uses the same case-insensitive/trimmed rules.

Example:

Existing siblings:

    New note
    NEW NOTE 2
    New note 4

The next automatically generated title should be:

    New note 3

Generate the unique default name through Rust/model logic, not by maintaining
a separate list in QML.

After creation, retain the current behaviour of selecting the new note and
focusing its title for immediate editing.

## Rename

Renaming a node must reject a title that conflicts with another sibling.

Example:

    Branch 1
    Branch 2

Renaming Branch 2 to:

    Branch 1

must fail.

The original title must remain unchanged.

Show a short user-facing message such as:

    A note named "Branch 1" already exists here.

Do not silently add a suffix during an explicit rename.

Do not merge notes.

Do not overwrite another note.

A case-only change of the same node is allowed when it does not create a new
collision.

Example:

    ideas

may be renamed to:

    Ideas

if there is no other equivalent sibling.

## Reorder within the same parent

Pure sibling reordering does not change titles or parent membership.

It must remain allowed.

Do not reject an otherwise valid same-parent reorder merely because the file
contains legacy duplicate siblings from an older OmaTree version.

## Moving / reparenting

When moving a node to a DIFFERENT parent, validate the node's title against
the destination parent's existing children before creating a checkpoint or
changing anything.

Example:

    Projects
      Ideas

    Archive
      Ideas

Moving Projects / Ideas under Archive must fail cleanly.

The tree must remain unchanged.

The failed move must:

- create no recovery checkpoint
- change no dirty state
- emit no documentMutated signal
- trigger no autosave

Moving to root follows the same rule against existing root titles.

Moving within the same parent is only a reorder and does not require a new
title-collision check.

## Drag-and-drop feedback

Update the existing drag/drop validation so a destination that would cause a
title conflict is visibly invalid before drop.

Reuse the existing invalid-drop visual treatment.

The user should not need to release the mouse before learning that the move
is impossible.

Do not add a new drag UI.

## Trash restoration

Restoring a Trash entry must check the restored subtree root against the
destination parent.

If the original parent still exists but already contains an equivalent title:

- refuse the restore
- leave the Trash entry intact
- create no recovery checkpoint
- leave dirty state unchanged

If the original parent is gone and restore would fall back to root:

- apply the same collision check against root notes

Show a short human-readable error.

Do not auto-rename restored notes.

### Legacy Trash contents

Trash created by older OmaTree versions may itself contain internal sibling
duplicates.

Do not make such recovery data permanently unusable.

An old subtree may be restored exactly as stored if its root does not collide
with the active destination.

After restore, legacy internal conflicts may remain and should be reported by
the legacy-conflict mechanism described below.

Recovery fidelity is more important than silently rewriting historical data.

## Checkpoint restoration

Checkpoint restore must remain an exact recovery operation.

A checkpoint may originate from an older version of OmaTree and may contain
duplicate sibling titles.

Do NOT reject or rewrite such a checkpoint.

Restore it exactly.

After restoration, detect and report any legacy title conflicts.

Recovery must never become unusable because newer validation rules are
stricter than the historical document.

## Existing notebooks and backward compatibility

Existing notebooks may already contain duplicate sibling titles.

They MUST continue to open.

Do not:

- refuse to open them
- silently rename notes
- delete notes
- change the database merely by opening it

Loading persisted data remains permissive with respect to sibling-title
conflicts.

After opening a notebook containing conflicts, show one concise warning such
as:

    This notebook contains duplicate note names under the same parent.
    Nothing was changed. Rename the duplicates to resolve them.
    OmaTree will prevent new duplicates.

Do not repeatedly display the warning while editing.

Provide a pure-Rust way to count/detect current sibling-title conflicts.

Expose only the minimum needed to QML.

Do not expose NodeIds merely for this warning.

## Working with legacy conflicts

A legacy-conflicted notebook must remain repairable.

The user must be able to:

- rename conflicting notes to unique titles
- delete conflicting notes
- move a conflicting note to a different parent where its title is unique

Operations must not create NEW collisions.

Same-parent reorder remains allowed because it does not create or worsen the
title set.

Explicit rename to a different conflicting title remains forbidden.

Once conflicts are manually resolved, normal uniqueness guarantees keep them
resolved.

## Persistence

Do not change the SQLite schema.

Do not bump schema version.

Titles remain stored exactly as normal TEXT values.

Uniqueness is a domain-model rule, not a database UNIQUE constraint.

This is intentional because:

- comparison is normalized in Rust
- root parent_id is NULL
- legacy duplicate files must remain loadable

Existing v1/v2 migration behaviour must remain unchanged.

## Errors

Add the smallest appropriate model error for sibling-title conflicts.

The exact Rust type is flexible.

It should allow the Qt/UI layer to produce a human-readable message without
parsing arbitrary error strings.

Do not build a large validation framework.

## Recovery checkpoint reasons

Continue using human-readable checkpoint reasons such as:

    Before moving "Ideas"

Duplicate historical titles may make old reasons ambiguous, which is
acceptable.

Do not redesign checkpoint metadata in this ticket.

## Tests — title normalization

Add tests covering at least:

1. exact duplicate root title rejected
2. root duplicate differing only by case rejected
3. root duplicate differing only by surrounding whitespace rejected
4. duplicate child under the same parent rejected
5. same title under different parents allowed
6. empty title rejected
7. whitespace-only title rejected
8. stored committed title is trimmed
9. internal whitespace is preserved
10. case-only rename of the same node succeeds when otherwise unique

## Tests — automatic names

Cover at least:

11. first automatic root is "New note"
12. second is "New note 2"
13. missing suffix numbers are reused appropriately
14. case-insensitive existing defaults are respected
15. children generate names independently per parent
16. roots and child groups do not interfere with one another

## Tests — rename

Cover at least:

17. rename to sibling collision fails
18. failed rename preserves original title
19. failed rename does not dirty the Document
20. failed rename creates no mutation signal/autosave
21. rename resolving a legacy duplicate succeeds

## Tests — move

Cover at least:

22. cross-parent move into title collision fails
23. move to root into title collision fails
24. failed collision move leaves tree unchanged
25. failed collision move creates no checkpoint
26. failed collision move leaves dirty state unchanged
27. failed collision move produces no mutation
28. same-parent reorder remains allowed
29. moving a legacy duplicate to a conflict-free parent succeeds
30. drag canDrop/canReparent rejects a conflicting destination

## Tests — recovery

Cover at least:

31. Trash restore into destination collision fails
32. failed Trash restore leaves Trash entry intact
33. failed Trash restore creates no checkpoint
34. failed Trash restore leaves dirty state unchanged
35. legacy Trash subtree with internal duplicates remains recoverable
36. checkpoint restore containing legacy duplicates succeeds exactly
37. conflicts are detected after restoring a legacy checkpoint

## Tests — backward compatibility

Cover at least:

38. persisted notebook containing duplicate siblings still opens
39. opening legacy duplicates does not modify the file
40. duplicate-conflict count/detection is correct
41. conflict-free notebook reports no conflicts
42. saving/reopening legacy duplicate data does not silently rename it
43. manually resolving conflicts produces a conflict-free notebook
44. new duplicate creation remains blocked after legacy file load

Use temporary notebook files only.

## Runtime verification

Exercise at least:

- create several default sibling notes and observe unique names
- rename into a collision and see clear rejection
- rename with case-only change
- drag onto a conflicting parent and see invalid target feedback
- successful drag to a non-conflicting parent
- Trash restore collision rejection
- legacy duplicate notebook opens with one warning and unchanged data
- legacy conflicts can be manually resolved
- checkpoint containing duplicates can still be restored
- autosave behaviour remains correct
- no QML warnings or binding loops

Do not modify real user notebooks during automated testing.

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Perform a clean offscreen launch.

## Out of scope

Do not:

- require globally unique titles
- auto-rename explicit user renames
- auto-rename moved nodes
- auto-rename Trash restores
- merge notes
- change SQLite schema
- bump schema version
- implement search
- implement paths/breadcrumbs
- implement hover auto-expand
- implement undo/redo
- implement Omarchy theming
- redesign the UI
- add Unicode-normalization dependencies
- perform unrelated cleanup