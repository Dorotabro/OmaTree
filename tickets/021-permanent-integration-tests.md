# Ticket 021 — Permanent integration test suite

## Goal

Replace the repeated disposable Qt/QML runtime harnesses used during development
with a small permanent integration-test suite.

OmaTree already has extensive Rust unit tests.

This ticket is specifically for behaviours that cross boundaries between:

- Rust
- CXX-Qt
- QML
- Qt models
- timers
- document replacement
- selection
- Search
- Recovery
- Markdown Preview
- Theme state

The suite should make future refactoring and packaging safer.

This is test infrastructure, not a product-feature ticket.

## Core principle

Do not attempt to preserve every assertion from every temporary harness.

Test the important contracts.

Prefer a small stable suite of high-value scenarios over hundreds of brittle
UI assertions.

## Permanent test location

Create a dedicated permanent integration-test area.

A suitable structure might be:

    tests/
      qml/
      integration/

or another clean equivalent.

Do not inject temporary test code into:

    qml/main.qml

and restore it afterward.

Production QML must remain untouched by test execution.

## Runner

Provide one documented command that runs the permanent integration suite.

Strong preference:

    cargo test

if the suite can be integrated cleanly into Cargo without making normal Rust
tests fragile.

If that is disproportionately awkward with CXX-Qt / Qt Quick Test, provide one
additional obvious command such as:

    ./scripts/test-integration

or:

    cargo run --bin omatree-integration-tests

Choose the smallest reliable architecture after inspecting the current build.

Do not create an elaborate testing framework merely to force everything through
Cargo.

The completion report must explain the chosen approach.

## Headless execution

The permanent suite must run without a graphical desktop.

Use an offscreen Qt platform where appropriate.

It must not:

- capture screenshots
- interact with the real desktop
- touch the user's clipboard
- modify the user's Omarchy configuration
- open native file dialogs
- modify real user notebooks

All filesystem work must use temporary directories/files.

## Production code

Tests should exercise the real production models/components where practical.

Do not copy application logic into a fake test implementation.

Mock or bypass only boundaries that cannot sensibly operate headlessly, such as
native file-picker interaction.

The purpose is to catch integration regressions, not merely retest duplicated
logic.

## Isolation

Each test/scenario must begin from known state.

Do not depend on:

- test execution order
- previous notebook files
- user's HOME contents
- user's current Omarchy theme
- previous selection state

Use temporary directories and explicit environment state.

Clean up automatically.

## Timing

Avoid arbitrary long sleeps.

For:

- autosave debounce
- Search debounce
- Theme polling

use bounded QtTest-style waits or event-based checks where practical.

Tests must fail rather than hang indefinitely.

Keep total runtime reasonable.

Target well under one minute on the current development machine.

## QML warnings

Unexpected QML warnings, binding loops or uncaught runtime errors should fail
the relevant integration run where practical.

A clean suite should produce no QML warnings.

## Scenario 1 — Basic note lifecycle

Permanently verify:

1. start with an empty Untitled document
2. create a root note
3. create a child
4. rename a note
5. edit the body
6. selection displays the correct note
7. dirty state changes appropriately
8. save to a temporary `.omatree`
9. reload/open it
10. hierarchy/title/body survive

Do not test every Rust Notebook rule again.

This verifies the QML/model/document bridge.

## Scenario 2 — Autosave

Verify:

1. open/save a file-backed temporary notebook
2. edit a body
3. autosave becomes pending
4. debounce completes
5. file on disk contains the change
6. dirty state becomes clean
7. no Recovery checkpoint is created for ordinary body editing

Use bounded waiting.

## Scenario 3 — New / Open document replacement

Verify:

1. select a note
2. populate Search results
3. create New Notebook
4. old selection becomes invalid safely
5. Search state/results clear
6. editor clears
7. new document is Untitled and clean

Then Open another temporary notebook and verify the replacement path again.

The suite should catch stale QModelIndex regressions.

## Scenario 4 — Search and reveal

Verify:

1. create a nested tree
2. place query text in a deep note
3. search by title or body
4. result appears
5. activate result
6. Search closes
7. ancestors expand
8. correct note becomes selected
9. editor displays the correct body

Also verify a deleted/stale search result fails safely if practical.

## Scenario 5 — Move and expansion state

Verify:

1. construct a nested tree
2. expand selected branches
3. recursively expand/collapse where testable programmatically
4. move/reparent an expanded subtree
5. NodeId-based expansion state survives the move
6. save
7. reopen
8. stored expansion state is restored

No pixel testing of guide lines is required.

Guide metadata is already covered by Rust tests.

## Scenario 6 — Trash and Recovery

Verify:

1. create a subtree
2. delete / Move to Trash
3. Trash entry appears
4. restore it
5. hierarchy/body return
6. no duplicate or stale QModelIndex behaviour occurs

Also exercise one checkpoint restore path.

Do not exhaustively duplicate every Recovery unit test.

## Scenario 7 — Markdown Preview invariants

Verify:

1. enter raw Markdown body
2. enter Preview
3. rendered output is actually Markdown rather than raw source
4. return to Edit
5. raw body is byte-identical
6. Preview toggle does not alter dirty state
7. Preview toggle creates no checkpoint
8. selection change while previewing renders the new note
9. New/Open resets mode to Edit as specified

Preserve the current raw-source search semantics.

## Scenario 8 — Markdown resource safety

Make the Ticket 016 resource-loading safety check permanent.

Use a temporary localhost listener only.

Create Markdown containing:

- remote-style image URL pointing at localhost
- file:// image/resource
- raw HTML image

Open Preview.

Verify:

- localhost receives no image/resource request
- local resource is not automatically loaded
- raw HTML remains non-executable/non-embedded
- note source remains unchanged

Do not contact the public Internet.

This is a security/privacy regression test and should remain permanent.

## Scenario 9 — Theme switching

Use an isolated temporary Omarchy-like state directory.

Do not modify:

    ~/.local/state/omarchy

Verify:

1. app begins with a known test theme
2. semantic Theme values reflect it
3. replace/change the temporary theme palette
4. polling detects it
5. semantic colours update
6. document dirty state does not change
7. current selection remains unchanged
8. Markdown Preview remains functional

No screenshot/pixel comparison is required.

## Scenario 10 — Keyboard/help smoke test

Keep this small.

Verify programmatically where reliable:

- F1 opens Keyboard Shortcuts
- Escape closes it
- Ctrl+F opens Search
- Ctrl+E toggles Preview for a selected note
- Shift+F10 opens the tree context menu if QtTest can reliably synthesize it
- shortcut overlay leaves document state unchanged

Do not duplicate the complete Ticket 019 temporary harness.

Mouse modifier combinations that QtTest cannot synthesize reliably may remain
covered by unit logic plus manual testing.

## Dialog safety smoke test

Verify at least one OmaTree-owned confirmation dialog:

- opens with a meaningful safe default
- Escape cancels
- document remains unchanged

Do not depend on native OS file dialogs.

## Test fixtures

Prefer programmatically constructed temporary notebooks over committed binary
`.omatree` fixtures.

Committed fixture files are acceptable only where a historical schema case
truly requires them.

Keep fixtures minimal.

## Test helpers

Factor repeated setup into small test helpers.

Useful helpers may include:

- create temporary notebook
- create nested note tree
- wait for condition with timeout
- select note
- query visible model state
- install temporary Omarchy palette

Do not create a second application architecture inside the tests.

## CI friendliness

Design the runner so it can later run in CI without a desktop session.

Do not implement CI configuration in this ticket unless trivially necessary.

Document required Qt/offscreen environment assumptions.

## Existing tests

All existing Rust tests must remain unchanged or improve.

Do not delete good unit tests because an integration test now covers similar
behaviour.

Integration tests complement them.

## Documentation

Add a small section to README or development documentation describing:

- normal Rust/full test command
- permanent integration-test command, if separate
- requirement for Qt/offscreen support
- approximate purpose of the integration suite

Keep it short.

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo build

Run the new permanent integration suite from a clean checkout/build state as
far as practical.

Run it twice consecutively to prove it does not depend on stale state.

Verify:

- no user files touched
- no user Omarchy theme modified
- no public network traffic required
- no QML warnings or binding loops

## Completion criteria

Ticket 021 succeeds when the important runtime behaviours that previously
required disposable harnesses can be rerun later without editing production
QML.

The suite should be boring, deterministic and maintainable.

## Out of scope

Do not:

- add product features
- redesign the UI
- change keyboard shortcuts
- change persistence semantics
- change SQLite schema
- create screenshot/golden-image tests
- add pixel-perfect UI tests
- add CI pipelines unless required to make the runner work
- package OmaTree
- add installers
- perform unrelated cleanup