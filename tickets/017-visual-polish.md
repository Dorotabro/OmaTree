# Ticket 017 — Omarchy-native visual polish

## Goal

Give OmaTree a deliberate visual identity.

The application is functionally mature enough that this ticket should focus
on how it feels to use.

The intended aesthetic is:

    a polished GUI descended spiritually from a beautiful TUI

Think:

- compact
- keyboard-first
- quiet
- technical
- confident
- slightly cinematic
- restrained use of colour
- strong typography and spacing
- very little traditional desktop chrome

OmaTree should look especially at home on Omarchy, while remaining attractive
with the built-in OmaTree Dark and Light palettes.

The result should feel like a serious computer program from a timeline where
GUIs evolved from excellent terminal interfaces rather than websites.

Do not turn it into terminal cosplay.

Do not make everything monospace, neon, green-on-black, or covered in fake
shell prompts.

## Non-goals

This is not a feature ticket.

Do not add:

- new Notebook behaviour
- new persistence behaviour
- tags
- backlinks
- settings
- Markdown features
- Markdown editing helpers
- tabs
- multiple documents
- icons requiring bundled assets
- animations for decoration
- a theme picker
- user-customisable styling
- syntax highlighting

Existing behaviour must remain intact.

## Core visual principles

### 1. Low chrome

Controls should visually recede until needed.

Avoid conventional raised GUI buttons where a quiet command-like control is
sufficient.

The content should dominate:

    tree | note

not the controls around it.

### 2. Density without crowding

Keep OmaTree compact.

Tree rows, command areas and dialogs should feel efficient rather than
spacious SaaS UI.

However:

- text must remain readable
- click targets must remain practical
- controls must not become microscopic

### 3. Typography creates hierarchy

Use:

- size
- weight
- spacing
- muted colour

before adding boxes, backgrounds or decoration.

### 4. Colour has meaning

Use colour sparingly but visibly.

Colour should communicate:

- selection
- focus
- interactive elements
- Markdown structure
- warning/destructive actions
- state

Do not decorate arbitrary elements simply because colours are available.

### 5. Sharp and restrained geometry

Prefer:

- straight edges
- hairline borders
- subtle small-radius corners where needed
- flat surfaces

Avoid:

- large rounded cards
- pill buttons everywhere
- drop shadows
- gradients
- glass effects
- glossy controls

## Semantic colour expansion

The existing semantic Theme layer remains authoritative.

Do not hard-code Omarchy theme names.

Do not add literal UI colours to QML.

Extend the semantic palette with a small additional colour vocabulary suitable
for visual hierarchy and Markdown.

Add:

    accentSecondary
    positive
    warning

Existing roles remain:

    background
    surface
    surfaceRaised
    foreground
    mutedForeground
    accent
    selection
    selectionForeground
    border
    danger
    dangerForeground

Do not add a dozen syntax-specific colour properties.

These three additional roles are enough.

## Built-in palette additions

Use these for OmaTree Dark:

    accentSecondary    #9EA7D8
    positive           #8FBFA3
    warning            #D8B36A

Use these for OmaTree Light:

    accentSecondary    #696FA8
    positive           #5D8F71
    warning            #A4772A

These remain original OmaTree palette values.

## Omarchy mapping

When using an Omarchy palette, prefer:

    accentSecondary:
        magenta
        cyan
        blue
        accent

    positive:
        green
        accent

    warning:
        yellow
        accentSecondary
        accent

Treat these as fallback chains.

Missing optional values must never reject an otherwise valid Omarchy theme.

Do not hard-code behaviour for specific themes.

Custom Omarchy themes must inherit the same logic.

## Theme safety

All new colours must:

- update live with Omarchy theme changes
- update live with system light/dark changes
- preserve existing fallback behaviour
- never affect document state

Add tests for the new mappings and fallbacks.

## Shared UI metrics

Introduce one small shared QML styling/metrics singleton rather than scattering
new numbers across components.

It may contain values such as:

- tiny/small/medium/large spacing
- row height
- editor padding
- hairline width
- small corner radius
- control height
- subtle animation duration if one is genuinely useful

Name it appropriately, for example:

    Ui.qml
    Metrics.qml
    Style.qml

Do not put colours there.

Colours remain owned by Theme.

Keep the metrics system small.

## Fonts

Do not bundle fonts.

The normal application font remains the platform/Omarchy UI font.

Use a system fixed-width font selectively for technical/command-like UI where
it improves the TUI-inspired character.

Appropriate uses may include:

- compact command labels
- keyboard shortcut hints
- code in Markdown preview

Do NOT make:

- ordinary note body text
- note titles
- all tree labels

monospace merely for aesthetic effect.

Use Qt/platform font facilities rather than naming one specific installed
font.

## Main window

Polish the overall visual structure.

The two-pane layout must remain obvious without looking boxed-in.

Prefer:

- one subtle pane separator
- strong background/surface distinction
- very little framing
- restrained focus indication

The split handle should feel intentional rather than like a stock Qt widget.

Do not materially change application layout.

## Tree pane

The tree should feel like the navigational core of the application.

### Rows

Refine:

- indentation
- row spacing
- disclosure controls
- hover
- selection
- keyboard focus

Preferred selected-row treatment:

- quiet surface change
- small accent indicator/bar
- clear selected text

Avoid a giant bright rectangle.

Hover should be clearly weaker than selection.

### Nesting

Hierarchy should remain easy to read at a glance.

You may use:

- subtle indentation rhythm
- extremely restrained guide lines
- improved disclosure spacing

Do not turn the tree into a dense web of connector graphics.

### Focus

Keyboard focus should be visible but not loud.

A thin accent edge or similarly restrained signal is appropriate.

## Tree footer / command strip

The current footer contains actions such as:

- New note
- New child
- Search
- File

Redesign this to feel more like a compact command strip and less like a row of
desktop push buttons.

Possible visual language:

    + NOTE    + CHILD    SEARCH    FILE

or another similarly restrained treatment.

Exact casing is a visual decision.

Do not sacrifice clarity for stylisation.

Commands should still be clearly interactive with the mouse.

Where useful, subtle shortcut hints may be shown.

Do not make the footer taller.

## Search

Search should visually belong to the same application.

Refine:

- search field
- focus state
- result selection
- result title
- breadcrumb
- snippet
- empty state

The search field should not look like a large generic form input.

A quiet underline / terminal-like command-field treatment is appropriate.

Search results should retain excellent information hierarchy:

    title
    breadcrumb
    snippet

Use mutedForeground for secondary information.

Use accent only where useful.

Do not add match-highlighting logic in this ticket.

## Editor header

Rethink the current header containing:

- title
- Edit / Preview
- Delete

The title is the dominant element.

The title editor should visually disappear into the page when not focused.

On focus, communicate editability with a restrained treatment such as:

- accent underline
- thin focus edge

Do not display it as a conventional bordered input box.

### Edit / Preview

Redesign the mode switch so it feels intentional.

It may be:

    EDIT  PREVIEW

with the active mode subtly indicated, or another compact command-like
treatment.

Do not create a giant toggle switch.

The mode must remain discoverable.

Ctrl+E continues to work unchanged.

### Delete

Delete should remain visually available but subordinate.

Use Theme.danger sparingly.

It should not dominate the header.

## Editor body

Improve body margins and line layout.

The editor should feel comfortable for sustained writing without becoming
spacious.

Use sensible horizontal padding.

Do not put the body inside a card.

Do not give ordinary text a visible border.

Focus should be apparent primarily through typography/cursor and perhaps a
subtle accent.

## Markdown Preview

Markdown Preview deserves a deliberate visual treatment.

It must remain based on the safe rendering pipeline introduced in Ticket 016.

Do not weaken:

- resource blocking
- HTML handling
- link scheme filtering
- read-only behaviour

### General

Body text:

    foreground

Secondary text:

    mutedForeground

Links:

    accent

Link hover, if practical:

    accentSecondary

### Headings

Use colour to create hierarchy without producing a rainbow document.

Suggested direction:

    H1 -> accent
    H2 -> accentSecondary
    H3+ -> foreground with weight/spacing hierarchy

This is a direction, not a rigid parser requirement.

Do not assign a different arbitrary colour to every heading level.

Headings should have deliberate spacing above/below.

### Blockquotes

Improve Qt's current plain indented treatment.

Use:

- muted or secondary foreground
- a thin accentSecondary left rule
- modest indentation

Do not create a giant quotation card.

### Inline code

Use:

- system fixed-width font
- surfaceRaised background
- subtle warning or accent-derived foreground where readable
- small horizontal padding if Qt's text format permits it cleanly

Do not make inline code look like a button.

### Fenced code blocks

Use:

- system fixed-width font
- surfaceRaised background
- subtle border
- comfortable internal spacing

Syntax highlighting is NOT required.

No language badge is required.

Do not implement a code editor.

### Lists

List markers may use Theme.accent or accentSecondary.

Text remains normal foreground.

Nested lists must remain visually understandable.

### Horizontal rules

Use a quiet Theme.border rule.

Do not use a giant full-bright line.

### Links

Links should be visibly interactive.

Use:

- Theme.accent
- underline or another conventional link signal

Hover feedback should be subtle if available.

Existing safe scheme filtering remains authoritative.

### Tables

If Qt renders Markdown tables, make them visually coherent.

Use:

- Theme.border
- surface/surfaceRaised distinction for headings if practical
- normal foreground text

Do not build a custom table widget.

### Task lists

If Qt renders task-list markers, theme them where practical.

A checked marker may use Theme.positive.

They remain read-only.

Do not add interactive Markdown editing.

### Colour restraint

Markdown Preview should feel richer than raw text but not like a syntax
highlighting demo.

A typical rendered page should still be predominantly:

    foreground + background

with colour appearing at structural points.

## Dialogs

Polish OmaTree-owned dialogs to fit the same visual language.

They should feel compact and deliberate.

Use:

- strong title hierarchy
- restrained border
- quiet background
- command-like buttons
- obvious default/focused action
- danger only for destructive choices

Avoid giant modal cards and oversized buttons.

Native file dialogs remain native.

Do not replace them.

## Recovery dialog

Refine:

- Trash / Checkpoints tabs
- list selection
- timestamps/reasons
- empty states
- action buttons

The tab treatment should feel consistent with Edit/Preview.

A subtle active underline or accent indicator is appropriate.

Do not alter Recovery behaviour.

## File menu / popup

Style the existing compact File popup consistently.

It should read visually more like a command menu than a collection of push
buttons.

Use:

- compact row height
- clear hover/focus
- restrained border
- shortcut hints where already known/useful

Do not create a traditional desktop menu bar.

## Empty states

Review empty states across:

- empty notebook
- no selected note
- Search
- Recovery
- Markdown Preview

They should be quiet.

Do not use illustrations.

Do not use huge headings.

Muted text and simple guidance are enough.

## Keyboard-first behaviour

All existing keyboard shortcuts must continue to work.

Visual focus should make keyboard navigation understandable.

Exercise at least:

- tree navigation
- Ctrl+N
- Ctrl+Shift+N
- Ctrl+F
- Search Up/Down/Enter/Escape
- Ctrl+E
- Ctrl+S
- Alt+Up / Alt+Down
- F2
- Delete

Do not change shortcuts merely for visual consistency.

## Mouse behaviour

Do not accidentally reduce usability while flattening controls.

All clickable actions must retain reasonable hit areas even if their visible
chrome becomes smaller.

Do not require pixel-perfect clicking on text glyphs.

## Animation

Animation is optional and should be nearly invisible.

If used at all, limit it to very short interaction feedback such as a subtle
hover/focus transition.

Do not add:

- sliding panels
- fades on ordinary navigation
- bouncing
- spring animation
- animated backgrounds

A static interface is completely acceptable.

## Accessibility and contrast

Ensure text remains readable across:

- OmaTree Dark
- OmaTree Light
- several Omarchy themes

Do not place muted text on a surface where it becomes effectively invisible.

Do not use colour as the only signal for critical destructive actions or
selection.

Keyboard focus must remain perceivable.

## No hard-coded QML colours

After this ticket, QML must still contain no arbitrary literal UI hex colours.

Add or retain an automated/source-level check where practical.

All colour decisions flow from Theme semantic roles.

## Regression requirements

Visual work must not alter:

- Notebook data
- SQLite schema
- dirty behaviour
- autosave
- New/Open/Save
- Search semantics
- Recovery
- Trash
- drag/drop
- sibling-title validation
- Markdown source
- Markdown safety
- external URL filtering

## Tests

Preserve all existing tests.

Add tests for at least:

1. new built-in semantic roles exist in Dark
2. new built-in semantic roles exist in Light
3. valid Omarchy green/yellow/magenta or equivalent map correctly
4. missing optional Omarchy colours use fallback chains
5. theme switching updates the new semantic roles
6. no document mutation occurs during theme change
7. Markdown safe-resource behaviour is unchanged
8. Markdown raw body remains unchanged by rendering
9. all existing keyboard actions remain connected
10. Search still functions after the visual changes
11. Recovery still functions after the visual changes
12. document reset still clears appropriate transient state

Do not create brittle pixel-perfect tests.

## Runtime visual verification

This ticket MUST be evaluated on a real graphical desktop.

Inspect:

- normal tree
- deeply nested tree
- selected tree row
- keyboard-focused tree
- footer command strip
- editor title unfocused
- editor title focused
- Edit mode
- Preview mode
- Search
- Search results
- File menu
- Recovery
- confirmation dialog
- empty states

Verify Markdown containing:

- H1 / H2 / H3
- bold / italic
- blockquote
- lists
- inline code
- fenced code
- link
- horizontal rule
- table if Qt supports it
- task list if Qt supports it

Test with at least:

- current Omarchy theme
- one substantially different dark Omarchy theme
- one light theme

Switch themes while OmaTree remains open.

### Screenshot privacy

If screenshots are used for visual inspection:

- capture the OmaTree window only
- never capture the entire desktop
- do not capture unrelated applications
- temporary screenshots do not need to be committed
- remove temporary captures after inspection

A whole-screen screenshot is explicitly unnecessary.

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo build

Perform:

- clean offscreen launch
- real-desktop visual verification
- live theme switching
- keyboard regression checks
- Markdown safety regression checks

No QML warnings or binding loops.

## Completion criteria

Ticket 017 is successful when OmaTree still behaves exactly like OmaTree, but
no longer visually resembles a mostly-default Qt utility.

It should feel:

- coherent
- distinctive
- technical
- quiet
- polished
- fast
- native to Omarchy when running there

The interface should reward looking at it without demanding attention.

## Out of scope

Do not:

- add application features
- add settings
- add a theme picker
- bundle fonts
- bundle icon packs
- add syntax highlighting
- add Markdown editing helpers
- add user CSS
- add animations as decoration
- introduce WebEngine
- change SQLite schema
- change persistence
- perform unrelated architecture cleanup