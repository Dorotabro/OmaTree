# Ticket 023 — Markdown spacing and tree alignment

## Goal

Fix two small but noticeable visual issues discovered during real use:

1. Markdown paragraphs currently render with effectively no visible vertical
   separation, so blank lines in the Markdown source appear to do nothing.
2. The first root note in the tree sits too close to the top edge and is
   visually misaligned with the note title in the editor pane.

This is a very small polish ticket.

Do not add features.

Do not redesign the UI.

---

# Part 1 — Markdown paragraph and line-break rendering

## Problem

The raw Markdown editor correctly preserves blank lines.

For example:

    one

    two

is stored exactly as typed.

However, Preview currently renders the two paragraphs with essentially no
visible spacing between them.

This makes the source:

    one

    two

look almost identical to a simple hard line break.

The problem is presentation/rendering, not persistence.

Do not modify or normalise the raw Markdown source.

## Required Markdown behaviour

Preserve ordinary Markdown semantics.

### Paragraph separation

This:

    one

    two

must render as two paragraphs with clearly visible vertical separation.

The separation should feel approximately like a conventional paragraph gap,
not an enormous blank region.

Aim for roughly one-half to one line of additional vertical breathing room.

The exact value should be typography-driven and visually balanced.

### Hard line break — two trailing spaces

This source:

    one··
    two

where `··` represents two literal trailing spaces, must render as:

    one
    two

with a hard line break but WITHOUT paragraph spacing between the two lines.

### Hard line break — backslash

Where supported by Qt's CommonMark implementation:

    one\
    two

must behave like the hard-break example above.

If Qt genuinely does not support this construct, document that rather than
implementing a custom Markdown parser.

### Soft newline

Do not convert every single source newline into a hard break.

Normal Markdown semantics remain authoritative.

For example:

    one
    two

may remain one paragraph according to Qt/CommonMark behaviour.

Do not introduce a "GitHub soft-break" mode merely to make Markdown behave
like a plain-text editor.

### Multiple blank lines

Markdown normally collapses multiple blank source lines into paragraph
separation.

Do not attempt to make:

    one



    two

produce proportionally larger gaps than:

    one

    two

unless Qt naturally does so.

We need readable paragraphs, not preservation of arbitrary whitespace.

---

# Renderer implementation

Inspect the current Ticket 016 Markdown pipeline and determine why paragraph
spacing is lost.

Current architecture must remain:

    raw Node.body
        ↓
    QTextDocument Markdown parsing
        ↓
    safe formatting/resource handling
        ↓
    read-only rich-text Preview

Do not replace this architecture.

Prefer fixing QTextDocument/QTextBlock formatting before rich-text
serialization.

Where practical, derive paragraph spacing from the active/default font's line
metrics rather than hard-coding theme-specific pixel values.

The fix must survive serialization into the rich-text representation currently
displayed by QML.

Do not solve this by altering the Markdown source with:

- extra `<br>` tags
- `&nbsp;`
- injected spaces
- raw HTML
- fake Markdown characters

Raw source remains byte-identical.

---

# Existing Markdown styling

Preserve Ticket 017 styling:

- H1 accent
- H2 secondary accent
- lower heading hierarchy
- blockquote treatment
- inline code
- fenced code panels
- list marker colours
- links
- rules
- tables/task lists where Qt supports them

Add sensible vertical spacing between block types where needed.

At minimum inspect:

- paragraph → paragraph
- heading → paragraph
- paragraph → heading
- paragraph → list
- list → paragraph
- paragraph → blockquote
- blockquote → paragraph
- fenced code block → paragraph
- paragraph → fenced code block
- horizontal rule surroundings

Do not perform another Markdown visual redesign.

The objective is simply that blocks no longer visually collide.

---

# Code block followed by text

The real-world case that exposed the issue was:

    ```rust
    fn main() {
        println!("Praise the Omnissiah");
    }
    ```

    text

`text` must not appear glued directly to the code panel.

There should be a clear but restrained gap between the fenced block and the
following paragraph.

---

# Markdown invariants

The fix must not change:

- Node.body contents
- dirty semantics
- autosave
- Recovery
- Search raw-source semantics
- Markdown resource blocking
- raw HTML handling
- external-link filtering
- Preview read-only behaviour

Do not weaken the Ticket 016 network/resource safety implementation.

---

# Permanent Markdown regression tests

Add this behaviour to the permanent integration suite rather than a temporary
harness.

Cover at least:

1. `one\n\ntwo` renders as two visually separated paragraphs
2. paragraph rendering is measurably taller than an equivalent hard-break
   rendering
3. two-space hard break produces two lines without paragraph-sized spacing
4. backslash hard break works if supported by Qt
5. raw source remains byte-identical after Preview
6. multiple blank lines do not corrupt or duplicate source
7. fenced code followed by a paragraph has visible separation
8. paragraph followed by fenced code has visible separation
9. existing Markdown resource-safety test still passes

Avoid pixel-perfect screenshots.

A same-run geometry/content-height comparison is acceptable if it is stable
under the offscreen test environment.

Do not encode one particular font's absolute pixel metrics into tests.

---

# Part 2 — Tree top alignment

## Problem

In normal tree mode, the first root note begins almost against the top of the
window.

The editor pane has deliberate top breathing room around its title.

The result makes the panes look vertically unrelated.

Current visual impression:

    Another note        [higher]
                       Sub Branch

Desired impression:

    Another note       Sub Branch

The first root-note text should visually align with the editor title row.

## Required behaviour

Add a small top inset to the normal tree area.

The baseline/visual centre of the first root note should approximately align
with the note title in the editor header.

This is a visual alignment requirement, not a pixel-perfect mathematical one.

Use the existing shared `Ui.qml` metrics.

Prefer an existing spacing token.

If a dedicated tree-top metric is genuinely clearer, add one small semantic
metric rather than scattering a literal number into TreePane.qml.

Do not hard-code a value purely for the current screenshot resolution.

---

# Tree behaviour that must remain unchanged

The top inset must not break:

- scrolling
- tree guide geometry
- disclosure markers
- selection background
- accent bar
- hover
- keyboard navigation
- drag/drop
- recursive expansion
- context menu positioning
- expansion restoration
- Search mode
- footer position

The footer remains pinned where it currently belongs.

Only the normal tree content needs the additional top breathing room.

---

# Search

Do not blindly add the same top inset to Search mode if Search already has
correct intentional spacing.

Inspect it visually.

Normal Tree and Search may have different top layouts because Search contains
its own field/header.

Do not disturb Search merely for symmetry.

---

# Tree integration test

Do not add a brittle pixel-perfect test for visual alignment.

A lightweight structural test may confirm that the TreeView has the intended
top inset/metric if useful.

The real acceptance test is visual inspection on the real desktop.

---

# Real-desktop verification

Run OmaTree on the real graphical desktop.

Inspect:

## Markdown

Use exactly these examples:

    one

    two

and:

    one··
    two

and:

    one\
    two

and a fenced code block followed by a normal paragraph.

Confirm:

- paragraphs have an obvious gap
- hard line breaks remain ordinary consecutive lines
- code and following prose no longer collide visually
- raw source is unchanged after Edit → Preview → Edit

Check one dark Omarchy theme and one light theme.

## Tree

Inspect a notebook containing several root notes and nested branches.

Confirm:

- first root row no longer hugs the top edge
- first root row visually aligns with the editor title/header
- tree guides still begin correctly
- scrolling back to the top produces the same clean alignment
- Search still looks correct
- footer is unchanged

No full-desktop screenshot is required.

If screenshots are taken, capture OmaTree only.

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
- permanent Markdown-spacing integration regression
- real-desktop dark-theme check
- real-desktop light-theme check

No QML warnings or binding loops.

---

# Completion criteria

Ticket 023 is complete when:

1. blank lines in Markdown produce clearly visible paragraph separation
2. hard Markdown line breaks remain simple line breaks rather than paragraph
   gaps
3. code/paragraph transitions have sensible breathing room
4. raw Markdown is untouched
5. the top root note visually aligns with the editor title
6. no existing tree or Markdown behaviour regresses

---

# Out of scope

Do not:

- add Markdown features
- add Ctrl+B / Ctrl+I
- add syntax highlighting
- allow images/resources
- change Markdown safety rules
- change Search semantics
- add icons
- redesign the tree
- redesign Markdown colours
- change persistence
- change schema
- remember window geometry
- perform packaging
- generate an application icon
- perform unrelated cleanup