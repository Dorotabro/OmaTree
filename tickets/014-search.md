# Ticket 014 — Note search

## Goal

Add simple, fast search across the active OmaTree notebook.

Search must cover:

- note titles
- note bodies

Search is deliberately simple.

Do not add:

- tags
- fuzzy search
- regex
- search operators
- SQLite FTS
- indexing services
- semantic/vector search

The active in-memory Notebook remains the source of truth.

## User experience

Add:

    Ctrl+F

to enter Search mode.

Also provide one small discoverable Search action in the existing tree UI.

Keep the control visually unobtrusive.

When Search mode opens:

- focus the search field immediately
- select its existing contents if appropriate
- show search results in the left pane
- keep the note editor on the right

The normal hierarchy should temporarily give way to the results list.

Do not permanently add another application panel.

Pressing Escape:

- exits Search mode
- clears the search query/results
- restores the normal tree view
- does not change the currently selected note

## Search behaviour

Search is:

- case-insensitive
- substring based
- performed against title and body
- live while the user types

Trim leading/trailing whitespace from the query.

An empty or whitespace-only query produces no results.

Examples:

    omatree

matches:

    OmaTree
    Notes about omatree
    "I was working on OMaTrEe yesterday"

Do not interpret punctuation specially.

Do not implement word stemming or fuzzy matching.

## Matching

A note appears at most once in the result list.

A match occurs when the normalized query is found in either:

- title
- body

If both title and body match, still return one result.

Use a simple Unicode-aware lowercase comparison using the Rust standard
library, consistent with the project's existing preference for small,
understandable normalization rules.

Do not introduce a search dependency.

## Search implementation

Implement search in pure Rust against the active Notebook.

Do not search SQLite directly.

This ensures search immediately sees:

- unsaved edits
- newly created notes
- renames
- moves
- restored notes

The exact API is flexible.

Conceptually return lightweight search results containing enough information
for presentation:

- stable internal node identity
- title
- breadcrumb/path
- short preview/snippet
- whether the strongest match was in title or body, if useful

Do not expose NodeId directly to QML.

The Qt adapter may keep ephemeral search result data containing NodeIds
internally.

That is acceptable because search results are derived presentation state,
not a second mutable hierarchy.

## Result ordering

Use deterministic ordering.

Preferred order:

1. title matches
2. body-only matches

Within each group, preserve tree order.

Do not add relevance scoring.

Do not alphabetically reorder the user's notebook.

## Breadcrumb/path

Search results must make identically named notes in different branches easy
to distinguish.

Display a breadcrumb using the note's ancestors.

Example:

    TODO
    OmaTree › Release

and:

    TODO
    Threatwright › v0.2

Do not require globally unique note titles.

For a root note, the breadcrumb may show:

    Root

or simply omit the parent path.

Choose the visually cleaner option.

## Result snippet

Show one short secondary line beneath or beside the title.

For a body match:

- show text around the first matching occurrence
- collapse line breaks into spaces
- keep the snippet short enough for the sidebar

For a title-only match:

- use the first useful non-empty portion of the body if available
- otherwise the breadcrumb alone is sufficient

Do not render Markdown in search results.

Search sees the body as plain text.

Do not build rich match highlighting if it meaningfully complicates the
implementation.

A clean unhighlighted snippet is acceptable.

## Unicode safety

Snippet generation must not slice UTF-8 strings at invalid byte boundaries.

Accented text and non-ASCII content must not panic.

Use character-safe logic.

## Search model / Qt architecture

Use the smallest clean CXX-Qt-compatible design.

Acceptable approaches include:

- a small dedicated read-only QAbstractListModel for search results
- result state owned by NotebookModel and exposed through a minimal Qt API

Choose the cleaner implementation for the current CXX-Qt 0.10 architecture.

Do NOT:

- build search results by traversing the tree in QML
- copy note bodies into a QML JavaScript database
- expose NodeId as a JavaScript number
- create another mutable Notebook

The Rust Notebook remains authoritative.

## Search debounce

Do not run a complete search on every individual key event if avoidable.

Use a short fixed debounce around:

    100–150 ms

One QML Timer is sufficient.

Typing quickly should result in one search after the user briefly pauses.

Do not expose this as a setting.

Search must still feel immediate.

## Results UI

While Search mode is active, the left pane should contain:

- search field
- optional compact result count
- scrollable result list

Each result should display:

- note title
- breadcrumb/path in muted text
- short snippet where useful

Use the existing semantic theme roles.

Selected/hovered results should visually follow the same restrained OmaTree
style as the normal tree.

Do not introduce new hard-coded colors.

## Activating a result

Clicking a result, or pressing Enter on the selected result, must:

1. resolve the result to a fresh valid QModelIndex
2. leave Search mode
3. restore the normal tree
4. expand the note's ancestors
5. select the matching note
6. show that note in the editor

The note should therefore become visible in its real hierarchical context.

Do not retain or trust a QModelIndex captured when the search was originally
performed.

Use stable internal identity and resolve a fresh index when activating.

## Stale search results

The Notebook may change while Search mode is open.

Examples:

- autosave occurs
- note body changes
- note title changes
- node moves
- node is deleted
- node is restored

Search must fail safely.

When a real document mutation occurs while Search mode is open:

- debounce and rerun the current search

This should naturally remove deleted results and update renamed/edited ones.

If a result somehow refers to a node that no longer exists:

- activation must fail safely
- do not panic
- do not select an unrelated note

## Selection behaviour

Entering Search mode must not clear the current tree selection.

Merely typing/searching does not modify document state.

Activating a result changes selection normally.

Closing Search with Escape returns to the existing selection.

Search must never:

- mark the Document dirty
- trigger autosave
- create recovery checkpoints
- modify Trash
- modify recovery state

## Empty states

For an empty query, show a quiet instruction such as:

    Type to search notes

For a query with zero matches:

    No matching notes

Use muted semantic text.

Do not show an error dialog for zero results.

## Keyboard behaviour

Support at least:

    Ctrl+F      enter Search
    Escape      leave Search
    Down        move through results
    Up          move through results
    Enter       activate selected result

Typing in the search field must behave normally.

Do not interfere with editor text shortcuts when Search mode is closed.

## Search button

Add one restrained Search action.

It may live in the tree footer alongside the existing actions.

Do not create a toolbar or menu bar.

The current compact structure should remain intact.

## Performance

A straightforward O(n) scan of the in-memory Notebook is expected and
acceptable.

Do not prematurely introduce:

- indexes
- caches requiring complex invalidation
- SQLite FTS
- background threads

If performance becomes measurable with genuinely large notebooks, optimize
later based on evidence.

## Tests — pure Rust search

Cover at least:

1. empty notebook returns no results
2. empty query returns no results
3. whitespace-only query returns no results
4. exact title match
5. case-insensitive title match
6. partial title match
7. body match
8. case-insensitive body match
9. note matching title and body appears once
10. title matches sort before body-only matches
11. tree order is retained inside match groups
12. same title under different parents produces separate results
13. breadcrumb distinguishes those results
14. deeply nested breadcrumb is correct
15. root breadcrumb is sensible
16. body snippet includes matching context
17. multiline body snippet collapses line breaks safely
18. Unicode text search does not panic
19. Unicode snippet generation does not split invalid UTF-8
20. deleted nodes disappear from later searches
21. renamed notes reflect the new title
22. changed body reflects the new content
23. moved node receives an updated breadcrumb

## Tests — Qt / interaction

Cover where practical:

24. Ctrl+F enters Search mode
25. search field receives focus
26. search debounce produces one result refresh for rapid typing
27. result list displays title/path/snippet
28. Up/Down navigate results
29. Enter activates a result
30. clicking a result activates it
31. activation exits Search mode
32. ancestors are expanded
33. activated note becomes the selected tree note
34. Escape exits without changing selection
35. document mutation while searching refreshes results
36. stale/deleted result fails safely
37. Search never marks Document dirty
38. Search never triggers autosave
39. Search never creates a checkpoint
40. semantic theme switching still updates Search UI

Use a temporary QML/QtTest harness where normal Rust-side QObject construction
is impractical.

## Runtime verification

Exercise at least:

- search by title
- search by body
- case-insensitive search
- repeated title in different branches
- deeply nested result
- result activation and tree reveal
- keyboard-only search workflow
- zero-result state
- edit a note while Search is open and observe result refresh
- rename/move a matched note and observe updated path
- delete a matched note and observe removal
- switch Omarchy theme while Search is open
- no QML warnings or binding loops

Use temporary notebook files only.

## Verification

Run:

- `cargo fmt --check`
- `cargo clippy --all-targets`
- `cargo test`
- `cargo build`

Perform a clean offscreen launch.

Perform runtime search verification.

## Out of scope

Do not:

- implement Markdown rendering
- implement Markdown search semantics
- implement fuzzy search
- implement regex
- implement tags
- implement filters
- implement search history
- implement replace
- implement SQLite FTS
- add a search index
- add background threads
- add async Rust
- implement backlinks
- implement wiki links
- redesign the application
- change SQLite schema
- perform unrelated cleanup