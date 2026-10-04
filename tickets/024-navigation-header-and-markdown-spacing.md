# Ticket 024 — Persistent Search header and real Markdown paragraph spacing

## Goal

Fix one remaining Markdown rendering defect and improve the left-pane structure.

There are two parts:

1. Make paragraph spacing visibly correct in the ACTUAL rendered Preview.
2. Give the left pane a permanent Search header aligned with the editor header.

This is still pre-packaging UI polish.

Do not add unrelated features.

---

# Part 1 — Fix Markdown spacing in the final displayed Preview

## Problem

Ticket 023 added paragraph spacing logic and an integration test.

However, real-desktop testing still shows this source:

    one

    two

rendered visually as:

    one
    two

with effectively zero paragraph separation.

This is not acceptable.

The previous integration test clearly did not prove the behaviour of the final
displayed Preview accurately enough.

## Required behaviour

### Paragraphs

This:

    one

    two

must visibly render as two separate paragraphs.

There must be unmistakable vertical breathing room between them.

Target:

- roughly 0.5–0.8 line height of extra separation
- enough to visually distinguish a paragraph break from a hard line break
- not so large that ordinary prose looks double-spaced

### Hard break

This:

    one··
    two

where `··` means two literal trailing spaces, must render as:

    one
    two

with no paragraph-sized gap.

Likewise:

    one\
    two

must render as a hard break if Qt supports it.

### Single soft newline

This:

    one
    two

may remain one paragraph according to Qt/CommonMark semantics.

Do not force every source newline into a visible break.

## Test the final rendering layer

The permanent regression must inspect the actual QML Preview/TextEdit that the
user sees.

Do not consider the fix proven merely because:

- QTextDocument block formats contain margins
- an intermediate QTextDocument has greater height
- serialized rich text contains margin-related HTML/CSS

The test must verify the final displayed Preview geometry after the rendered
rich text has passed through the complete production pipeline.

Conceptually:

    Markdown source
        ↓
    C++ Markdown renderer
        ↓
    serialized rich text
        ↓
    QML TextEdit / Preview
        ↓
    MEASURE HERE

The final TextEdit must visibly distinguish:

- paragraph separation
- hard line break

## Investigate serialization loss

Inspect whether QTextDocument paragraph margins are being lost or normalised
during:

- rich-text serialization
- assignment to QML TextEdit
- QML rich-text parsing/layout

Do not assume the Ticket 023 QTextBlockFormat solution survives that boundary.

Fix the real cause.

## Acceptable implementation strategies

Prefer the smallest robust approach.

Possible directions include:

- producing rich text whose paragraph spacing survives QML TextEdit
- explicit paragraph styling in the serialized HTML
- using QTextDocument output formatting that QML preserves
- another Qt-native representation that keeps paragraph margins intact

Do not:

- modify raw Markdown source
- inject fake Markdown characters
- enable arbitrary raw HTML in source
- use WebEngine
- add a JavaScript renderer
- replace the Markdown pipeline wholesale

Generated/sanitized HTML produced internally by OmaTree is acceptable if it is
the safest practical way to preserve layout.

Raw note HTML must remain disabled as before.

## Security invariants

Preserve every Ticket 016 guarantee:

- remote images/resources are not fetched
- local file resources are not loaded
- raw HTML remains non-active
- Preview is read-only
- external URL filtering remains unchanged
- Markdown source remains byte-identical

Re-run the permanent resource-safety scenario.

## Block spacing

Once paragraph spacing works in the final Preview, verify the same final layer
for:

- paragraph → paragraph
- code block → paragraph
- paragraph → code block
- list → paragraph
- paragraph → list
- quote → paragraph
- paragraph → quote
- heading → paragraph
- paragraph → heading
- horizontal rule surroundings

Do not redesign the visual language.

Keep current colours and styles.

## Permanent integration test

Update/add a permanent QML integration scenario that measures the final
Preview item itself.

Cover at least:

1. blank-line paragraph gap is visibly non-zero
2. paragraph gap is significantly larger than hard-break spacing
3. two-space hard break remains consecutive lines
4. backslash hard break remains consecutive lines if supported
5. single soft newline is not treated as paragraph separation
6. code block followed by paragraph has visible separation
7. raw source is unchanged
8. resource-safety scenario still passes

Avoid absolute pixel expectations.

Compare values relative to the Preview's own line height.

The test must fail against the currently broken real behaviour.

---

# Part 2 — Permanent Search header

## Goal

Give the left navigation pane a permanent top header that visually mirrors the
editor header.

Current layout is asymmetric:

    tree starts at top
    editor has title/header + separator

New layout:

    Search field             Note title + EDIT/PREVIEW/DELETE
    ─────────────            ───────────────────────────────
    tree / search results    note body

The separator lines on the left and right should align visually.

## Search header

Add a permanent compact Search field at the top of the left pane.

Visual direction:

    / search notes

or similarly restrained.

The exact placeholder may be:

    Search notes

Keep the existing TUI-inspired visual language.

Use:

- no conventional boxed form styling if avoidable
- subtle focus treatment
- semantic colours only
- current platform/system fonts
- compact height

No icon.

## Header height/alignment

The left Search header should occupy the same effective vertical header band as
the editor's title/header area.

The separator below Search should align with the separator under the editor
header.

The normal tree begins below that line.

This replaces the previous tree top-margin workaround.

Remove or adjust the Ticket 023 topMargin if it is no longer needed.

Do not stack both solutions and create excess space.

## Search behaviour

When the Search field is empty:

- show the normal tree below it

When the user types a query:

- show Search results below the same permanent Search header
- temporarily replace the tree body only
- keep the Search field in place

This means Search is no longer a separate full-pane mode with its own header.

The navigation pane always has the same top structure.

## Ctrl+F

Ctrl+F:

- focuses the permanent Search field
- selects the current query text if appropriate

It does not need to "enter Search mode" structurally anymore.

## Escape

While Search has a query:

- Escape clears the query
- Search results disappear
- normal tree returns
- current note selection remains unchanged where practical

If the Search field is empty:

- Escape should return focus sensibly, preferably to the tree or previous
  context

Do not close the application or alter the document.

## Result activation

Existing Search semantics remain:

- Up / Down navigate results
- Enter activates selected result
- clicking result activates it
- note is revealed in the tree
- ancestors expand
- correct note becomes selected

After activation:

- clear Search query
- normal tree returns
- selected note remains visible

## Search result UI

Reuse the existing Search result styling and hierarchy:

- title
- breadcrumb
- snippet
- selection accent

Do not redesign Search from scratch.

Refactor SearchPane if needed so:

- header field remains persistent
- result list can occupy only the navigation-body area

## Footer

Remove:

    SEARCH

from the bottom command strip.

The footer becomes:

    + NOTE    + CHILD                         FILE

Exact spacing may adapt naturally.

Do not replace Search with an icon.

Do not add HELP.

F1 remains the keyboard-shortcuts reference.

## Empty search state

Do not show:

    Type to search notes

as a large body empty state anymore, because the permanent field itself already
communicates this.

If a non-empty query has zero results, show:

    No matching notes

quietly in the navigation body.

## Tree context menu

Preserve all existing tree context-menu behaviour.

Search header changes must not break:

- right-click
- Shift+F10
- Expand All / Collapse All
- subtree actions
- Move to Trash

## Tree guides

Tree branch guides must begin below the new header separator and remain
unchanged.

Do not alter guide geometry unless required by the layout refactor.

## Drag/drop

Preserve:

- drag/drop
- drop indicators
- recursive expansion
- selection
- scrolling

Do not let the permanent Search header become a drop target.

## Expansion persistence

No change.

Search reveal may continue recording newly revealed ancestors as expanded.

No schema change.

---

# Keyboard shortcuts reference

Update the F1 Keyboard Shortcuts reference if its wording still says Search
"opens Search mode".

Prefer wording such as:

    Ctrl+F    Focus Search

or:

    Ctrl+F    Search notes

Only change wording if needed to match the new behaviour.

No shortcut changes.

---

# Tests — Search header

Use the permanent integration suite.

Cover at least:

1. Search field exists at all times
2. empty query shows normal tree
3. Ctrl+F focuses the field
4. typing shows result list in navigation body
5. Escape clears query and restores tree
6. result activation clears Search and reveals note
7. current note selection remains correct
8. Search does not affect dirty state
9. Search does not trigger autosave
10. Search does not create checkpoints
11. footer no longer contains SEARCH
12. + NOTE / + CHILD / FILE still work
13. F1 shortcut reference still opens
14. Open/New document replacement clears Search query
15. light/dark theme switching while Search is active still works

## Structural alignment test

Do not create brittle pixel-perfect assertions.

A lightweight runtime check may verify that:

- Search header bottom separator and editor header separator are approximately
  aligned

Use a small tolerance if measuring geometry.

Real-desktop visual inspection remains authoritative.

---

# Real-desktop verification

Inspect on the real desktop.

## Markdown

Use exactly:

    one

    two

and:

    one··
    two

Confirm that the first visibly has paragraph spacing and the second does not.

Do not accept intermediate-document evidence.

Look at the final Preview.

## Navigation

Confirm:

- Search header aligns with editor header
- separator lines line up
- tree begins below separator
- footer is cleaner without SEARCH
- Ctrl+F feels natural
- Escape feels natural
- result activation returns to tree cleanly
- deep tree guides remain correct
- context menu still works

Check one dark theme and one light theme.

---

# Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo test integration_tests
- cargo build

Perform:

- clean offscreen launch
- permanent Markdown final-render regression
- Search-header integration scenarios
- real-desktop dark-theme check
- real-desktop light-theme check

No QML warnings or binding loops.

---

# Out of scope

Do not:

- add icons
- add new Search features
- add fuzzy search
- change Search matching semantics
- add Search history
- add Markdown features
- add syntax highlighting
- change persistence
- change schema
- package OmaTree
- generate an icon
- redesign the rest of the UI
- perform unrelated cleanup