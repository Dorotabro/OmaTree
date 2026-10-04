# Ticket 028 — Pre-release audit fixes

## Goal

Fix the three release-relevant findings accepted from the Ticket 027
pre-release code audit.

Accepted findings:

1. H1 — recovery checkpoints can inflate notebook size dramatically
2. M1 — an edited title can be silently lost before commit
3. M2 — two OmaTree instances can silently overwrite one another's changes

Do not address unrelated audit findings in this ticket.

Do not refactor the application generally.

The application is feature-frozen for v0.1.

---

# Part 1 — Bound recovery checkpoint storage

## Problem

Recovery checkpoints currently store complete Notebook snapshots.

Up to 100 checkpoints may therefore duplicate all note bodies.

Measured audit result:

- roughly 2 MB active notebook
- 100 structural move/checkpoint/save operations
- resulting file approximately 204 MB

Later full recovery saves also rewrite the complete retained checkpoint set.

This creates both:

- excessive file growth
- increasingly slow structural saves

This blocks v0.1.

## Required solution

Retain the existing checkpoint count cap:

    MAX_CHECKPOINTS = 100

Add a second checkpoint-history budget:

    MAX_CHECKPOINT_BYTES = 16 MiB

or a similarly named constant.

Use:

    16 * 1024 * 1024

bytes.

This budget applies to retained checkpoint snapshot data, not to:

- the active Notebook
- Trash
- SQLite bookkeeping
- unrelated database pages

## Retention policy

After adding a new checkpoint:

1. retain newest checkpoints preferentially
2. prune oldest checkpoints until the retained history is within the byte
   budget
3. still respect the existing count cap

### Oversized newest checkpoint

If the newest checkpoint by itself exceeds the byte budget:

- retain that newest checkpoint
- remove all older checkpoints

Do not discard the only fresh recovery point merely because the active
notebook itself is large.

Therefore:

    checkpoint history <= budget

normally, but:

    one oversized newest checkpoint

is an explicitly allowed exception.

This bounds large notebooks to approximately one extra active-notebook-sized
snapshot rather than up to 100.

## Size accounting

Use deterministic in-memory accounting.

Count enough stored content to meaningfully approximate checkpoint size.

At minimum include the byte lengths of:

- titles
- bodies

plus reasonable fixed overhead for nodes if useful.

Do not serialize the entire checkpoint merely to learn its size.

Do not query SQLite file size to make retention decisions.

The retention rule belongs to Recovery/Document semantics and must remain
deterministic before persistence.

## When pruning happens

Prune immediately when a checkpoint is created.

The in-memory checkpoint collection should already satisfy the retention
policy before the next save.

Loading an existing v3 notebook created by an older OmaTree version may contain
more history than the new byte budget.

On load:

- accept the existing file
- prune the in-memory checkpoint collection to the new retention policy
- do not rewrite the file merely because it was opened
- persist the pruned state on the next legitimate recovery/full save

Do not reject old files because their recovery history exceeds the new budget.

## Checkpoint behaviour

Preserve:

- checkpoint reasons
- timestamps
- full snapshot semantics
- checkpoint restore
- checkpoint-before-destructive-operation behaviour
- newest-first Recovery UI ordering

Do not introduce delta checkpoints in this ticket.

Do not redesign Recovery storage.

## Persistence performance

The purpose of this fix is to bound the pathological amount of recovery data
rewritten by structural saves.

Do not add asynchronous SQLite or threads.

Keep the current transactional persistence model.

## Tests — checkpoint budget

Add tests covering at least:

1. checkpoints below byte budget remain
2. count cap still applies
3. byte budget prunes oldest first
4. newest checkpoint is retained
5. one oversized newest checkpoint is retained alone
6. adding a small checkpoint after an oversized one behaves correctly
7. loaded legacy/current database with excessive checkpoint history opens
   successfully
8. opening such a file performs no write
9. next appropriate save persists the pruned history
10. checkpoint restore still works after pruning
11. Trash is unaffected

Add one non-timing regression representing the audit scenario:

- create a notebook with substantial bodies
- create many structural checkpoints
- verify retained snapshot bytes remain bounded according to the defined rule

Do not use a strict wall-clock assertion in normal tests.

Wall-clock tests are too environment-dependent.

A manual/release benchmark may be reported separately.

---

# Part 2 — Never silently lose an edited title

## Problem

The title editor currently commits primarily through editingFinished.

Some navigation/action paths load another note before the previous title is
committed.

Observed cases include:

- click another note
- Ctrl+N
- Ctrl+Shift+N
- close the application immediately after editing the title

The user's typed title can therefore disappear silently.

This must be fixed before release.

## Core rule

A pending title edit must be resolved before abandoning the currently loaded
note.

The title editor must know which note it is editing independently of whatever
note becomes selected next.

Do not rely solely on focus-loss signal ordering.

## Preferred architecture

Centralize pending-title commit behaviour.

The exact implementation may differ, but avoid sprinkling unrelated
`commitTitle()` calls through every possible UI action if the editor can
reliably own this lifecycle.

A good design may involve:

- EditorPane tracking the QModelIndex/note currently loaded for title editing
- commitPendingTitle() operating on that stored note
- load(newIndex) resolving the previous pending title before replacing its
  editor state

Choose the smallest robust approach compatible with the current architecture.

## Commit failure

Title commit can fail because of:

- empty title
- sibling-title collision

In that case:

- do not silently discard the typed value
- do not navigate/create/close as though the commit succeeded

The action that would abandon the edit must be stopped or safely resolved.

Show the existing title-validation message.

The user must remain able to correct the title.

## Selection change

If the user clicks another tree note while a modified title is pending:

1. attempt to commit the old title
2. if successful, continue selection change
3. if rejected, remain on/currently restore the original note and title edit

Do not leave the UI showing one note while the editor contains another note's
pending title.

## New note / child

Before successful creation changes selection:

1. commit current pending title
2. if commit succeeds, continue creation
3. if commit fails, abort creation

Preserve the Ticket 025 rule:

    successful newly created note -> Edit mode

## Close

Before safe-to-leave/close considers the document state:

- commit the pending title first

A successfully committed rename then participates in ordinary dirty/save/
autosave semantics.

If the rename is invalid:

- do not close
- allow the user to correct it

## New / Open

Apply the same rule before abandoning the existing Document.

Pending valid title changes must not disappear merely because:

- New Notebook
- Open

was requested.

Invalid title must prevent the transition until resolved or explicitly
discarded through an appropriate existing workflow.

Do not create another independent unsaved-title dialog.

## Other actions

Audit existing actions that can replace selection/document state.

Ensure the central solution covers relevant paths such as:

- Search-result activation
- Recovery navigation
- Delete
- move where selection changes
- tree keyboard navigation

Do not add redundant handlers if central editor/selection lifecycle handling
already covers them.

## Tests — pending title

Permanent integration coverage must include:

1. type title -> click/select another note -> rename persists
2. type title -> keyboard navigate to another note -> rename persists
3. type title -> Ctrl+N -> rename persists, new note created
4. type title -> Ctrl+Shift+N -> rename persists
5. type title -> New Notebook -> pending rename is saved/handled correctly
6. type title -> Open -> pending rename is handled correctly
7. type title -> Close -> pending rename is committed before save/leave logic
8. conflicting pending title -> selection change is refused safely
9. empty pending title -> selection change is refused safely
10. failed commit never silently restores the old title without explanation

Use temporary files only.

---

# Part 3 — Detect concurrent modification by another OmaTree instance

## Problem

Two OmaTree processes can open the same notebook.

Each holds its own in-memory Document.

Current behaviour permits:

1. instance A saves change A
2. instance B, which loaded the older state, saves change B
3. B rewrites the notebook from its stale snapshot
4. A's already-saved change disappears

SQLite serialization does not prevent this because the writes occur
sequentially.

Silent overwrite is unacceptable once .omatree files can be opened normally
from the desktop.

## Required behaviour

Before writing an already-open file, detect whether another connection/process
has committed changes since this Storage instance observed the file.

If external modification is detected:

- refuse the write
- preserve all in-memory OmaTree changes
- do not overwrite the external changes
- report a clear error
- allow the user to recover by reopening or Save As

Do not attempt automatic merging.

## Preferred SQLite mechanism

Investigate:

    PRAGMA data_version

for the current persistent SQLite connection.

It is acceptable if it reliably satisfies this requirement with the current
Storage connection lifetime.

Important:

- `data_version` is meaningful when comparing observations from the SAME
  connection over time
- do not compare numeric values between separate connections
- establish a baseline after opening/loading
- before each write, read it again
- another connection's commit should cause a mismatch
- OmaTree's own commits through the same connection must not be mistaken for
  external modification

Verify these semantics with tests against the actual Storage architecture.

## If PRAGMA data_version is unsuitable

If the current Storage connection lifetime or write architecture makes
data_version unreliable:

STOP and choose the smallest reliable optimistic-concurrency mechanism.

Acceptable alternatives may include:

- a persisted notebook revision/generation marker
- a robust file-change fingerprint

Do not introduce an advisory lock-file design without strong justification.

Do not silently implement a fragile heuristic based only on modification
timestamps.

Explain any departure from data_version in the completion report.

## Scope

Protect existing file-backed notebook writes:

- Save
- autosave
- structural/recovery save
- expansion/view-state Save
- other writes to the currently open notebook

Save As to a different target is not a conflict with the old file.

Existing overwrite validation for the new target remains authoritative.

## Error behaviour

Introduce a specific conflict error rather than presenting this as a generic
SQLite failure.

Suggested user-facing meaning:

    This notebook changed on disk after OmaTree opened it.

    Your changes are still in memory. Reopen the notebook to use the version
    on disk, or use Save As to preserve your current version.

Exact wording may be refined to fit existing dialogs.

Do not automatically reload because that would discard in-memory changes.

## Autosave

If autosave encounters an external-modification conflict:

- do not overwrite the file
- retain the document dirty state
- retain in-memory changes
- surface the existing one-time autosave failure mechanism with a clear
  conflict message
- repeated autosave attempts may remain suppressed according to the existing
  failure policy

Explicit Save should continue to report the conflict until the user resolves
it through:

- Open/reopen
- Save As
- another deliberate supported action

## New / Close safe-to-leave

If a dirty document cannot be saved because of an external modification:

- do not silently discard it
- existing Save / Discard / Cancel safe-to-leave behaviour should still work

Discard remains an explicit user decision.

Save As must remain available as the safe preservation path.

## External deletion

Do not regress existing deleted-file handling.

If data_version cannot distinguish file deletion/replacement, preserve the
existing explicit save failure.

This ticket's primary requirement is concurrent OmaTree/SQLite writers.

## Tests — concurrent instances

Add Rust/storage tests with two independent Storage/Document instances against
one temporary notebook.

Cover at least:

1. both open the same starting file
2. A saves a change
3. B attempts to save stale state
4. B receives external-modification conflict
5. A's saved change remains intact
6. B's in-memory change remains intact
7. B can Save As to another file successfully
8. after B reopens the original, it can save normally
9. autosave-style write detects the same conflict
10. expansion-only save cannot overwrite external content
11. structural/recovery save cannot overwrite external content
12. two read-only/no-change instances cause no false conflict
13. own successful save does not cause a false conflict on next own save

Add a permanent integration scenario if the user-facing autosave/error
boundary requires QML verification.

Do not spawn real desktop windows if two independent Storage instances provide
the required storage-level proof.

---

# Part 4 — Audit finding disposition

Do NOT address these Ticket 027 findings now:

## M3 — sibling/model scale

Defer.

Current performance is acceptable for normal v0.1 notebooks.

Record it in an appropriate post-v0.1 TODO/backlog if one exists.

Do not introduce a sibling index/cache immediately before release.

## L1 — build path leakage

Defer to packaging.

Ticket 029 should use release-path remapping/stripping where appropriate.

## L2 — pathological Markdown performance

Defer.

Do not redesign Markdown rendering for deliberately pathological input before
v0.1.

## L3 — .gitignore

Defer to final public-repository/release cleanup.

## L4 — Cargo metadata / cargo publish

Defer to packaging/public-release cleanup.

## L5 — deleted-file wording

Optional later polish.

No behavioural fix required for v0.1.

---

# Permanent regression suite

All accepted audit findings must acquire permanent regression coverage.

Do not use disposable QML harnesses.

The test count should increase only where meaningful.

---

# Verification

Run:

    cargo fmt --check
    cargo clippy --all-targets
    cargo test
    cargo test integration_tests
    cargo build
    cargo build --release

Run the permanent integration suite twice if QML behaviour changed
substantially.

Perform a local release-mode checkpoint benchmark after H1:

Use a notebook approximately comparable to the audit probe:

- around 1000 notes
- around 2 KB body each
- many structural moves/checkpoints

Report:

- initial file size
- file size after 100 checkpoints/moves
- approximate final save time

This benchmark is informative, not a brittle automated pass/fail timing test.

The resulting file must no longer exhibit the approximately 100x checkpoint
growth found by Ticket 027.

---

# Completion criteria

Ticket 028 succeeds when:

1. recovery history is bounded by both count and byte budget
2. repeated structural changes cannot multiply a notebook toward 100x through
   retained checkpoints
3. pending title edits cannot silently disappear on navigation/create/close
4. one stale OmaTree instance cannot overwrite another instance's saved work
5. all existing functionality/tests remain intact

---

# Completion report

Report:

- files modified
- H1 implementation
- checkpoint byte accounting
- exact retention policy
- benchmark before/after numbers
- M1 title-commit root cause and architecture
- every abandonment/navigation path covered
- invalid-title behaviour
- M2 concurrency mechanism selected and why
- conflict error/user-facing behaviour
- autosave/safe-to-leave interaction
- tests added
- total test count
- integration-test count
- fmt/clippy/test/build/release-build results
- disposition of every other Ticket 027 finding
- whether anything still blocks packaging

Do not package OmaTree.

Do not begin another ticket.