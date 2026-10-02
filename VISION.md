# OmaTree

OmaTree is a tiny tree-structured note editor.

It exists because hierarchical notes are useful, but applications providing them often become visually cluttered and unnecessarily complicated.

OmaTree deliberately does less.

## Core concept

Every item in the tree is a note.

For example:

    Projects
    ├── OmaTree
    │   ├── Ideas
    │   └── Bugs
    └── Threatwright
        ├── Architecture
        └── Roadmap

"Projects", "OmaTree", "Ideas", and every other node may contain text.

There is no distinction between a folder and a note.

## Primary interface

The intended interface is approximately:

    ┌────────────────────┬─────────────────────────────┐
    │                    │                             │
    │ ▾ Projects         │ OmaTree                     │
    │   ├ OmaTree        │                             │
    │   │ ├ Ideas        │ A stupidly simple tree      │
    │   │ └ Bugs         │ of notes.                   │
    │   └ Threatwright   │                             │
    │                    │                             │
    │                    │                             │
    └────────────────────┴─────────────────────────────┘

The tree is the navigation.

The right side is the editor.

That is the application.

## Principles

### Simplicity

A new user should understand OmaTree within seconds of opening it.

### Local first

Notes live on the user's computer.

OmaTree does not require:

- an account
- a network connection
- a cloud service

### One notebook, one file

The intended notebook format is a single SQLite database file.

SQLite is an implementation detail.

The user should experience it simply as an OmaTree notebook.

### User ownership

OmaTree should eventually provide straightforward export to common text formats so that notes are never trapped inside the application.

### Native desktop application

OmaTree is not a web application wrapped in a desktop shell.

It should start quickly, use little memory, and behave like a normal desktop application.

### Omarchy

OmaTree was conceived on Omarchy.

On Omarchy it should eventually integrate tightly with the current desktop theme, including colours and typography.

This is enhancement, not dependency.

Without Omarchy, OmaTree must remain a normal cross-platform Qt application.

## Non-goals

OmaTree is not intended to become:

- Obsidian
- Notion
- an IDE
- a knowledge graph
- a project-management suite
- a collaboration platform
- a document publishing system

Features such as backlinks, graph views, online collaboration, AI integration and plugin marketplaces are outside the core vision.

## First milestone

The first useful OmaTree should provide only:

- hierarchical nodes
- editable note text
- create node
- create child node
- rename node
- delete node
- rearrange nodes
- persistent storage
- autosave
- basic search
- pleasant keyboard navigation

Everything beyond that must justify its existence.