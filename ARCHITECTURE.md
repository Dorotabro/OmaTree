# OmaTree Architecture

This document describes the intended technical direction.

It is deliberately small and may evolve as implementation teaches us more.

## Technology stack

### Core

Rust

Responsibilities will eventually include:

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

The intended conceptual model is:

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

SQLite is intended for notebook storage.

The initial design should aim for one notebook corresponding to one file.

The exact file extension and schema will be defined in a later ticket.

SQLite should remain an internal implementation detail.

## Note contents

For the initial application, treat note contents as UTF-8 text.

Do not introduce a Markdown rendering engine, rich-text document model, or HTML editor unless a future ticket explicitly requires it.

Formatting capabilities can be evaluated later.

## Themes

The application must have a generic Qt theme implementation.

Omarchy support should later be provided through an optional theme integration layer.

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

## Current priority

Do not attempt to implement the complete architecture immediately.

Development should proceed in small independently testable tickets.