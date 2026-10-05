# OmaTree Architecture

This document describes the technical direction and how the application is built.

It is deliberately small and may evolve.

## Technology stack

### Core

Rust

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