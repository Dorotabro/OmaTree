# OmaTree Architecture

This document describes the technical direction and how the application is built.

It is deliberately small and may evolve.

> **Migration in progress.** Qt/QML is being replaced by egui/eframe (see
> `docs/EGUI_DECISIONS.md`, `docs/EGUI_MIGRATION_PLAN.md`). Everything from
> "Technology stack" to "Platform-specific code" below describes the **current
> implementation** (Qt) and stays accurate until the cut-over. The section
> "Target architecture (egui)" at the end is **proposed and mostly not implemented**.

## Current implementation (Qt)

## Technology stack

### Core

Rust. Since ticket E1 the Qt-free domain code lives in the library crate
`omatree-core` (`crates/core`): `notebook`, `document`, `storage`, `recovery`,
`search`, `theme` (palette) and `link`. The Qt application at the repository
root depends on it. `cargo test -p omatree-core` needs no Qt.

Responsibilities include:

- notebook model
- node manipulation
- persistence
- SQLite access
- search
- file handling
- platform-independent application logic

### UI

Qt 6 with Qt Quick / QML.

Responsibilities:

- main window
- tree display
- note editor
- keyboard interaction
- menus and dialogs
- visual styling

### Rust / Qt bridge

Use CXX-Qt.

Rust owns application/domain logic.

QML should primarily describe presentation and interaction rather than contain application business logic.

## Conceptual layers

    ┌──────────────────────────────┐
    │            QML UI            │
    │                              │
    │ TreeView        Note Editor  │
    └──────────────┬───────────────┘
                   │
                CXX-Qt
                   │
    ┌──────────────▼───────────────┐
    │          Rust Core           │
    │                              │
    │ Node model                   │
    │ Notebook operations          │
    │ Persistence                  │
    │ Search                       │
    └──────────────┬───────────────┘
                   │
                SQLite
                   │
    ┌──────────────▼───────────────┐
    │        Notebook file         │
    └──────────────────────────────┘

## Node model

The conceptual model is:

    Node
    ├── id
    ├── parent
    ├── position
    ├── title
    ├── body
    ├── created_at
    └── updated_at

Every node contains a note.

A node may also have child nodes.

There are no separate folder objects.

## Persistence

SQLite is used for notebook storage: one notebook is one file, conventionally
named `*.omatree` (MIME type `application/x-omatree`). The schema is versioned,
and may change between releases.

SQLite should remain an internal implementation detail.

## Note contents

Note contents are UTF-8 text. The editor is a plain-text editor.

A note can be shown in a read-only Markdown Preview, rendered with Qt's built-in
Markdown support. Preview never loads local or remote resources.

Do not introduce a rich-text document model or HTML editor.

## Themes

The application must have a generic Qt theme implementation.

Omarchy support is provided through an optional theme integration layer.

Conceptually:

    ThemeProvider
        ├── Default/System
        └── Omarchy

OmaTree must continue functioning if no Omarchy files exist.

## Platform-specific code

Platform-specific functionality should be isolated.

Avoid embedding Linux-specific assumptions into the core application.

## Dependencies

Prefer the smallest practical dependency set.

A dependency should solve a concrete problem rather than merely provide convenience.

## Scope

Keep changes small and independently testable, and preserve OmaTree's deliberately
small scope (see `VISION.md`).

---

# Target architecture (egui): proposed, not yet implemented

The decisions are recorded in `docs/EGUI_DECISIONS.md`; the sequence, APIs and risks are
in `docs/EGUI_MIGRATION_PLAN.md`. This section only states the intended shape.

## Crates

Status: `omatree-core` **exists** (ticket E1, `crates/core`); `omatree-egui` **exists as a
working notebook application** (tickets E4 to E8a, `crates/egui-app`: tree with drag and drop,
Search, Recovery, F1, themes, New/Open/Save As through native dialogs; attachments, export and
the cut-over are still to come); the editor library lives in the
`omatree-egui-editor-spike` repository (branch `editor-library`) and is consumed as a Git
dependency pinned to an exact commit (see the E4.1 record in
`docs/EGUI_MIGRATION_PLAN.md`, which also says how to update the pin).

    omatree-core     [implemented] Qt-free, egui-free: notebook, document, storage (SQLite),
                     recovery, search, theme palette, link safety
    omatree-editor   egui Markdown editor library (from the standalone editor):
                     live-rendered source-backed editing, attachments trait, export, PDF
    omatree (Qt)     [implemented] the current application at the repository root;
                     depends on omatree-core; behaviour unchanged
    omatree-egui     [E8a: working] the new application: egui host over core + editor;
                     starts untitled or with a path, hierarchical tree, Search, Recovery,
                     F1, New/Open/Save As, autosave, guarded close; no attachments yet

The standalone editor keeps a runnable demo binary built on the same library.

## Layers

    ┌──────────────────────────────┐
    │   egui host (tree, dialogs)  │
    │   EditorView (note editor)   │
    └──────────────┬───────────────┘
                   │  plain Rust calls, no bridge
    ┌──────────────▼───────────────┐
    │        omatree-core          │
    │ Document · Notebook · Search │
    │ Recovery · Attachments       │
    └──────────────┬───────────────┘
                SQLite (one notebook, one file)

## Principles

- `Document` is the only owner of note content. The editor works on a working copy of the
  active note's text and writes every real change back to `Document`.
- Markdown source is the single authoritative text. The editor never serialises a rich
  document back to Markdown.
- Live Markdown rendering replaces Edit/Preview; raw source of the span under the caret
  stays visible and editable.
- Caret, selection and undo history are kept per note.
- Images come only from notebook-owned attachments (random 128-bit ids, stored in
  SQLite, referenced as `assets/<32-hex-id>.<ext>`). No remote loading, no arbitrary
  filesystem images, no networking.
- Attachment cleanup is reference-aware (active notes, Trash, retained checkpoints) and
  never automatic for recoverable attachments. **Compact Notebook** is a deliberate,
  confirmed operation; pruning and SQLite space reclamation are separate steps.
- Shortcuts: Ctrl+E inline code; Ctrl+Shift+S Save As.
- Theme: the existing semantic palette (Omarchy, else system light/dark) drives the
  egui visuals.
- Export (Markdown folder, ZIP, PDF) reads only the attachment store.
## Implemented in E4 (egui shell, since extended: see the sections below)

    omatree-egui (crates/egui-app)  Host: Document, rows, selected node, one text buffer,
                                    status, UnavailableStore, EditorKeys::omatree()
        ├── omatree-core            Document is the owner of every body; write-through via
        │                           Document::set_body on changed frames, before switching
        │                           notes and before saving
        └── omatree_editor          EditorView keyed by NoteKey { notebook counter, node id }

- The host lends one `String` (the selected note) to the editor; caret, selection and undo
  live in egui memory per `NoteKey`. Opening a notebook bumps the counter and forgets the
  old notebook's state.
- Attachments are unavailable until E9: `UnavailableStore` refuses every put and get, so
  no reference is created and no file or URL is ever loaded.
- Ctrl+S saves; Ctrl+Shift+S is reserved for Save As and is consumed by the host so it
  never saves or formats; Ctrl+E is inline code.
- Not yet: autosave, Save As, close guard, tree interaction, Search/Trash/Recovery, theme.

## Lifecycle added in E4.1

- `Host::logic` (called from `eframe::App::logic`, so it also runs while minimised) answers
  every close request: a clean notebook closes; a dirty one gets `CancelClose` and a modal
  Save / Discard (confirmed twice) / Cancel. A Save that fails keeps the window open and
  dirty and offers Save As. Repeated requests never bypass the question.
- Save is `Document::save`; Save As (Ctrl+Shift+S) is an in-app path dialog over
  `Document::save_as` with overwrite confirmation; a native file dialog is left to E8.
- Autosave follows the Qt timer (1 s after the last real edit, existing notebooks only,
  first failure reported once) and additionally stops after an external-modification
  conflict until the user resolves it.
- The editor library is pinned by commit in `crates/egui-app/Cargo.toml` and `Cargo.lock`;
  no sibling checkout is required.

## Tree and note operations added in E5a

- `Tree` (derived from `Document`, rebuilt only when structure or a title changes) holds every
  note in order and the list of visible rows; `Document` stays the owner of hierarchy and of
  which notes are open. Rows are painted by hand and only the viewport is touched.
- The tree is one focusable widget; its keys (arrows, Ctrl+arrows, Enter, Delete, Page keys,
  Shift+F10) act only while it has the keyboard. Ctrl+N, Ctrl+Shift+N, F2 and Alt+Up/Down are
  application-wide.
- The note title is an editable field committed against its own note before that note is left;
  a refused title blocks the move, creation, save and close.
- Create, rename, move and delete use `Document` methods; deleting flushes the editor first and
  forgets editor state (caret, selection, undo) of every deleted note.
- Expansion is view state (never dirties); content changes start the autosave countdown, and
  `Host::logic` re-requests a repaint on every frame while it runs.

## Theme pipeline (E6a)

`omatree_core::theme` is the only authority on colours (Omarchy discovery, parsing, confirmation,
fallbacks). The egui host's `theme.rs` translates the resulting `Palette` into egui visuals and
the editor's palette, once per change; the editor never reads files or the environment, and the
host never watches a second source. See `docs/EGUI_MIGRATION_PLAN.md`, "E6a implementation record".

## Chrome spacing and derived colours (E6b)

The egui host's measurements live in one file, `crates/egui-app/src/layout.rs` (the shared 16-point
axis that equals the editor's own document padding, bar and row heights, pane limits). Presentation
colours that are not a palette role as such (selected/hover fills, guides, a label that reads on the
danger fill, text pulled to a legibility floor) are derived once per palette change by
`theme::Chrome` from the E6a palette; nothing in the host holds a theme-specific colour.

## Search (E7a)

`crates/egui-app/src/search.rs` holds the query, the derived result list and its drawing; the searching
itself is `omatree_core::search::search` over the in-memory `Document`, so there is no second engine or
index. Results are throwaway: invalidated by query, structure or body changes (a debounce for bodies),
never trusted on activation (the note is looked up again by `NodeId`), and chosen through
`Host::select`, so pending-title validation and hierarchy reveal stay in one place.

## Recovery, shortcuts and F1 (E7b)

`recovery_ui.rs` lists what `Document::recovery()` retains and calls `restore_trash` /
`restore_checkpoint`; it then repairs the host (tree, selection, editor state, Search) in the same
call. `keymap.rs` holds the application-wide shortcuts once: `Host::handle_shortcuts` consumes them
and `help.rs` (F1) prints them through egui's formatter, together with the editor's `EditorKeys`.
`Dialog` has two new variants (`Recovery`, `Shortcuts`); both go through `open_dialog`, `focus_back`
and `cancel_dialog` like every other question.

## The egui application today (E8a)

`crates/egui-app` is one `Host` over exactly one `Document` (`omatree-core`) and one `EditorView`.
It starts with an untitled in-memory notebook or the path given; there is no second store of
notebook content. Modules: `tree`/`tree_ui`/`drag` (virtualised tree, keys, drag and drop),
`search`, `recovery_ui`, `help` + `keymap` (F1 and the one shortcut table), `theme` + `layout`
(palette-derived colours, spacing), `lifecycle` (Save, autosave, conflicts, the modal questions)
and `files` (New, Open, and the native file dialogs).

Leaving a notebook (closing the window, New, Open) has one guard, `Host::request_leave`: commit
the title, flush the editor, ask Save / Discard / Cancel when dirty, and carry on only after a
successful save or a confirmed discard; the waiting action is a single `Option<Leave>` consumed
once. A notebook is only ever replaced by `Host::install`, which resets editor state, Search,
Recovery and autosave state together. The system file dialogs (`rfd`, desktop portal) run on a
thread and are polled once per frame; without them a typed-path dialog asks the same questions.

### Conflicts, reload and open requests (E8b)

`reload.rs` is the way out of an external-modification conflict: Save As, or Reload from Disk through
`Document::open_existing` and `Host::install` (nothing is reset unless the load worked), never an
overwrite. `files.rs` also owns the command-line check and `request_open_path`, the single entry for
every other incoming open request, and `recent.rs` keeps the recent-notebooks paths in the user's
configuration folder. There is still no single-instance channel: a second launch is a second process.

## Attachments (E9a)

Images belong to the notebook, not to any note: each is a BLOB row in the `attachments` table of the
same SQLite file (schema version 4), named by a random 128-bit id and referenced from Markdown as
`assets/<id>.png|jpg`. There is no foreign key to notes, Trash or checkpoints, so what an attachment
"belongs to" is derived from the bodies, and nothing deletes one except the future, explicit Compact
Notebook operation. A new attachment is first held in memory by the `Document` (`put_attachment`,
readable at once, also for an untitled notebook) and is written by the next save in the *same
checked transaction* as the notes; a file stays at schema version 3 until that first save, which
creates the table and sets version 4 atomically. Save As copies everything stored, plus what is
pending. The Qt build shares this core; it never writes attachments and older binaries refuse a
version 4 file without touching it (`tools/pre-e9-compat.sh`).
