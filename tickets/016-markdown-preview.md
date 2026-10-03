# Ticket 016 — Markdown edit and preview

## Goal

Add deliberately simple Markdown preview to note bodies.

The note body remains exactly what it is today:

    plain UTF-8 text stored in Node.body

Markdown is presentation only.

There must never be:

- rendered HTML stored in Notebook
- rendered Markdown stored in SQLite
- a second rich-text body
- Markdown-specific persistence
- a schema change

Conceptually:

                    Node.body
                       │
                 raw UTF-8 text
                       │
                ┌──────┴──────┐
                │             │
             Edit mode    Preview mode
             raw text      rendered
                              Markdown

The raw source is always authoritative.

## Product philosophy

This is not a rich-text editor.

The user writes Markdown directly.

OmaTree merely provides a pleasant read-only rendering of it.

Do not add:

- formatting toolbar
- WYSIWYG editing
- slash commands
- Markdown syntax menus
- Markdown document properties
- plugins
- wiki syntax
- backlinks
- frontmatter UI

Keep the feature almost embarrassingly simple.

## Supported Markdown

Preview must correctly render the ordinary Markdown constructs users are
likely to type manually:

- headings
- paragraphs
- bold
- italic
- bold + italic combinations
- unordered lists
- ordered lists
- nested lists where Qt supports them
- blockquotes
- inline code
- fenced code blocks
- horizontal rules
- links

It is acceptable if Qt naturally supports additional Markdown syntax.

Do not build special OmaTree product features around those extensions in this
ticket.

The minimum promised OmaTree Markdown vocabulary is the list above.

## Qt implementation

Prefer Qt's existing Markdown / QTextDocument support.

Do not add a JavaScript Markdown renderer.

Do not embed a web browser.

Do not introduce WebEngine.

Do not introduce Chromium.

Do not implement a Markdown parser from scratch.

Do not add a third-party Markdown dependency unless Qt's existing facilities
prove insufficient for a required safety property.

Use the smallest clean Qt/CXX-Qt-compatible implementation.

## Raw HTML

Raw HTML embedded in Markdown is not part of OmaTree's Markdown feature.

Where the chosen Qt API permits it, parse Markdown with HTML disabled.

HTML such as:

    <b>hello</b>
    <img src="...">

must not become an escape hatch into arbitrary rich content or resource
loading.

It may appear as literal text or otherwise be safely ignored according to the
chosen Qt Markdown API.

Document the exact behaviour in the completion report.

## External resources and images

Images and embedded resources are OUT OF SCOPE.

Markdown such as:

    ![example](https://example.com/image.png)

must NOT cause OmaTree Preview to automatically fetch that URL.

Likewise, Markdown must not automatically load arbitrary local files.

Previewing a note must not generate network traffic merely because Markdown
contains an image or resource reference.

Use the smallest reliable Qt-native mechanism to prevent resource loading.

Possible mechanisms may involve configuring the backing QTextDocument or a
small bridge/helper.

Do not modify the stored Markdown merely to accomplish this.

If direct QML MarkdownText cannot satisfy this requirement safely, do not
accept silent remote loading just for implementation convenience.

## Edit / Preview UI

Add one small body-mode control to EditorPane.

It should communicate:

    Preview

while editing, and:

    Edit

while previewing.

Keep it visually restrained.

Do not add a toolbar.

Place it naturally near the existing title/Delete area or immediately above
the body.

The exact placement may be chosen based on the current layout.

Use existing semantic Theme roles only.

Do not add literal colors.

## Keyboard shortcut

Add:

    Ctrl+E

to toggle body Edit / Preview mode.

This shortcut only affects the Markdown body presentation.

It must not modify the note.

If there is no selected note, Ctrl+E should do nothing.

Do not steal focus or produce an error dialog when no note is selected.

## Edit mode

Edit mode is the existing plain-text editor.

It must remain:

- plain text
- directly editable
- bound to the raw Node.body
- subject to the existing dirty/autosave behaviour

Do not change the persistence semantics of typing.

Markdown characters remain visible normally:

    ## Heading

    **bold**

    `code`

There is no inline WYSIWYG formatting while editing.

## Preview mode

Preview mode is:

- read-only
- rendered Markdown
- derived from the current raw body
- never persisted separately

Switching to Preview must immediately render the latest body text, including
edits that have not yet been autosaved to disk.

Preview must not require Save.

Preview must not mark the Document dirty.

Preview must not emit documentMutated.

Preview must not start or restart autosave merely because the mode changed.

## Preview appearance

Use the active semantic OmaTree theme.

At minimum:

- normal text -> Theme.foreground
- links -> Theme.accent
- background -> editor/background semantic role
- muted or secondary rendering, where needed -> Theme.mutedForeground
- selection -> Theme.selection / selectionForeground

Headings should inherit the application's typography and remain restrained.

Code should use an appropriate monospace font via Qt/platform defaults.

Do not bundle a font.

Do not hard-code an Omarchy theme.

When the Omarchy/system theme changes while Preview is visible, the preview
must update visually with the rest of OmaTree.

## Visual scope

Do not attempt the full visual redesign in this ticket.

Ticket 017 will perform the dedicated Omarchy-native / TUI-inspired visual
polish pass.

For Ticket 016:

- make Preview clean
- make it theme-correct
- make spacing readable
- avoid obviously ugly Qt defaults where a small adjustment suffices

Do not turn this ticket into the larger design pass.

## Links

Markdown links should be visually identifiable using the active theme.

Example:

    [Qt](https://www.qt.io)

A link should open only after explicit user activation.

Do not fetch the target merely to render the preview.

Support external opening for:

- http
- https
- mailto

Use the platform's normal external URL handling.

Unsupported or malformed schemes must fail safely.

Do not execute:

- javascript:
- data:
- arbitrary commands

Do not add an embedded browser.

## Search interaction

Search continues to search the RAW Markdown source exactly as it does today.

For example, a body containing:

    **elephant**

is searched as the literal stored text containing the Markdown markers.

Do not teach Search about rendered text in this ticket.

Search result snippets remain plain text.

Activating a Search result while Preview mode is active should:

- select/reveal the note normally
- show the selected note in Preview mode

Do not automatically force Edit mode merely because Search selected another
note.

## Mode lifetime

Edit / Preview is application UI state, not notebook state.

Do not store it in:

- Node
- Notebook
- Document
- SQLite
- .omatree files

Default on application startup:

    Edit

When selecting a different note:

- preserve the current Edit/Preview mode

This makes Preview useful for browsing several notes.

When replacing the entire document through:

- New Notebook
- Open

reset body mode to:

    Edit

This avoids carrying transient presentation state across documents.

## Selection changes

If Preview mode is active and another note is selected:

- immediately render that note's body
- do not modify either note
- do not briefly display stale content from the previous note

If selection becomes empty:

- show the existing empty/no-selection editor state
- do not leave the previous note's preview visible

## Cursor and edit state

Toggling to Preview and back to Edit should not unnecessarily destroy the
user's editing position.

Where practical, preserve:

- text cursor position
- selection
- editor scroll position

Do not introduce substantial state machinery solely for this.

A clean implementation where the existing editor remains instantiated but is
hidden during Preview is acceptable if it avoids side effects.

Preview scroll position may be independent.

When a different note is selected, Preview scroll may reset to the top.

When the whole document is replaced, all transient body-view state must reset
safely.

## Editor body loading

Be careful with the existing body loading/writeback guard.

Displaying Markdown must never accidentally write rendered or transformed
text back through setBody().

Only edits made in the raw plain-text editor may modify Node.body.

The renderer must have a one-way relationship:

    raw body -> Preview

never:

    Preview -> raw body

## Empty body

A selected note with an empty body in Preview mode should show a restrained
empty state.

Possible wording:

    Nothing to preview

Use muted theme text.

Do not display an error.

## Invalid Markdown

Malformed or incomplete Markdown must never cause an error dialog.

Markdown is intentionally tolerant.

Render whatever Qt can interpret.

The source must always remain available unchanged by switching back to Edit.

## Task lists / tables / other Qt extensions

If the chosen Qt Markdown renderer naturally renders additional constructs
such as:

- task lists
- tables
- strikethrough

that is acceptable.

However:

- do not add custom UI for them
- do not advertise them as core OmaTree features yet
- do not make persistence depend on them

Interactive task-list checkboxes in Preview must NOT mutate the raw note body.

Preview is read-only.

If Qt's default Markdown widget makes task-list boxes interactive, configure
or structure Preview so interaction cannot mutate the underlying text.

## Recovery

Recovery/Trash behaviour is unchanged.

A restored note's body is raw text as before.

If Preview is active after a restore/selection change, render the restored raw
body normally.

Preview mode itself must never create:

- checkpoint
- Trash entry
- recovery mutation

## Dirty/autosave guarantees

Toggling Preview:

    dirty state unchanged

Toggling Edit:

    dirty state unchanged

Theme changing while Preview is visible:

    dirty state unchanged

Following a link:

    dirty state unchanged

Only modifying raw body text keeps the existing mutation behaviour.

## Performance

Do not implement:

- Markdown render cache
- background rendering threads
- incremental Markdown AST
- persistence cache
- worker pool

Render on demand using Qt.

For normal OmaTree note sizes this should be trivial.

If large Markdown notes later expose measurable performance problems, address
them from evidence.

## Tests — state and integration

Preserve every existing test.

Add tests or a runtime harness covering at least:

1. application starts in Edit mode
2. Ctrl+E enters Preview for a selected note
3. Ctrl+E returns to Edit
4. Ctrl+E with no selection is harmless
5. Preview does not mutate raw body
6. Preview does not mark Document dirty
7. Preview does not trigger documentMutated
8. Preview does not create a checkpoint
9. Preview does not alter Trash
10. Preview does not trigger autosave
11. latest unsaved-in-memory body text appears in Preview
12. switching back reveals byte-identical raw Markdown
13. selecting another note while Preview is active stays in Preview
14. selected note's correct body replaces the previous preview
15. empty selection does not leave stale preview visible
16. New Notebook resets mode to Edit
17. Open resets mode to Edit
18. Search result activation preserves Preview mode
19. Recovery selection/restoration behaves safely with Preview
20. theme change updates Preview without document mutation

## Tests — Markdown rendering

Exercise at least:

21. heading
22. paragraph
23. bold
24. italic
25. unordered list
26. ordered list
27. blockquote
28. inline code
29. fenced code block
30. horizontal rule
31. link
32. malformed/incomplete Markdown
33. Unicode text
34. empty body

Verify enough rendered/document structure to demonstrate that Markdown mode is
actually active rather than merely showing raw source.

Do not create brittle pixel-perfect rendering tests.

## Tests — safety

Explicitly verify:

35. remote Markdown image does not cause a network fetch
36. local-file image/resource is not automatically loaded
37. raw HTML does not enable arbitrary embedded content
38. unsupported URL schemes are not externally opened
39. http/https links require explicit activation
40. Preview cannot edit the raw body through rendered task-list controls or
    other rich-text interactions

If fully automated network detection is disproportionately difficult, use a
local temporary HTTP listener/server in the test harness and prove it receives
no request.

Do not contact the public Internet during tests.

## Runtime verification

Exercise at least:

- write Markdown in Edit
- toggle Preview with the UI control
- toggle with Ctrl+E
- heading / bold / italic / lists
- blockquote
- inline and fenced code
- external link
- malformed Markdown
- Unicode
- empty note
- rapidly switch notes while Preview is active
- Search -> activate result while Preview active
- New Notebook while Preview active
- Open another notebook while Preview active
- restore a note from Trash
- switch Omarchy themes while Preview is visible
- confirm no remote image/resource fetch occurs
- confirm raw Markdown remains unchanged

Use temporary notebook files only.

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo build

Perform a clean offscreen launch.

Perform the Markdown runtime scenarios above.

No real-desktop screenshot is required for this ticket.

## Out of scope

Do not:

- add a Markdown formatting toolbar
- add Ctrl+B / Ctrl+I editing helpers yet
- add WYSIWYG editing
- add syntax highlighting to Edit mode
- add attachments
- add embedded images
- add local image management
- add Mermaid
- add MathJax/LaTeX
- add wiki links
- add backlinks
- add tags
- add frontmatter UI
- add plugins
- add WebEngine
- add JavaScript rendering
- change Search semantics
- change SQLite schema
- perform the Ticket 017 visual redesign
- perform unrelated cleanup