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

Status: `omatree-core` **exists** (ticket E1, `crates/core`); the other three are
planned.

    omatree-core     [implemented] Qt-free, egui-free: notebook, document, storage (SQLite),
                     recovery, search, theme palette, link safety
    omatree-editor   egui Markdown editor library (from the standalone editor):
                     live-rendered source-backed editing, attachments trait, export, PDF
    omatree (Qt)     [implemented] the current application at the repository root;
                     depends on omatree-core; behaviour unchanged
    omatree-egui     the new application: egui host over core + editor

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