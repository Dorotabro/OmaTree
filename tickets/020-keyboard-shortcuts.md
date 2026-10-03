# Ticket 020 — Keyboard shortcuts reference

## Goal

Add a compact, themed Keyboard Shortcuts reference for OmaTree.

OmaTree now has enough keyboard functionality that discoverability matters.

The reference should fit the existing visual language:

- compact
- text-first
- keyboard-friendly
- no traditional Help menu
- no new footer button
- no browser or external documentation

## Entry points

Add:

    F1

to open the Keyboard Shortcuts reference.

Also add an entry at the bottom of the existing FILE menu:

    Keyboard Shortcuts    F1

Keep the existing footer unchanged.

Do not add a permanent HELP command.

## Dialog / overlay

Use the existing OmaTree-owned themed dialog/overlay components.

The reference should:

- use the current semantic theme
- work in dark and light themes
- use normal UI typography for descriptions
- use the system fixed-width font for shortcut labels
- be compact enough to scan quickly
- avoid looking like a giant settings page

Escape closes it.

F1 may also close it if that is easy and natural, but this is optional.

Opening or closing the reference must not:

- mark the document dirty
- trigger autosave
- create a checkpoint
- modify Trash
- change note selection
- modify Search state
- modify tree expansion state

## Sections

Group shortcuts into these sections:

### Notes

Include current bindings such as:

    Ctrl+N
        New note

    Ctrl+Shift+N
        New child

    F2
        Rename selected note

    Delete
        Move selected note to Trash

    Alt+Up
        Move note up

    Alt+Down
        Move note down

### Tree

Include current bindings such as:

    Up / Down
        Navigate visible notes

    Left / Right
        Collapse / expand normally

    Ctrl+Left
        Collapse subtree

    Ctrl+Right
        Expand subtree

    Ctrl+Click disclosure
        Collapse / expand subtree recursively

    Shift+F10
        Open tree context menu

    Menu key
        Open tree context menu

Only include the Menu key if Ticket 019 actually implements it reliably.

### Editor

Include:

    Ctrl+E
        Edit / Preview

Also list ordinary editing shortcuts only if they are actually supported by
the current editor and Ticket 019 confirms them:

    Ctrl+Z
        Undo

    Ctrl+Shift+Z
        Redo

    Ctrl+X
        Cut

    Ctrl+C
        Copy

    Ctrl+V
        Paste

    Ctrl+A
        Select all

Do not advertise shortcuts that do not actually work.

### Search

Include:

    Ctrl+F
        Search

    Up / Down
        Select search result

    Enter
        Open selected result

    Escape
        Close Search

### Files

Include the current implemented file shortcuts:

    Ctrl+O
        Open notebook

    Ctrl+S
        Save

    Ctrl+Shift+S
        Save As

Do not invent a New Notebook shortcut if none exists.

### General

Include:

    F1
        Keyboard Shortcuts

    Escape
        Close current dialog/menu where applicable

Only list behaviours that are consistently true.

## Accuracy requirement

Before writing the final displayed list, inspect the actual current shortcut
bindings in QML and the Ticket 019 implementation.

The dialog must reflect reality.

Do not blindly copy the examples above if the source code differs.

If a shortcut is ambiguous, platform-specific or not implemented, omit it or
describe it accurately.

The reference must not become stale documentation on the day it is created.

## Visual design

Follow the current OmaTree visual language.

Suggested structure:

    NOTES
    Ctrl+N              New note
    Ctrl+Shift+N        New child
    F2                  Rename

    TREE
    Ctrl+Right          Expand subtree
    Ctrl+Left           Collapse subtree

Use:

- Theme.foreground for descriptions
- Theme.mutedForeground for secondary structure
- Theme.accent or accentSecondary sparingly for section headings
- system fixed-width font for shortcut labels

Do not add hard-coded colours.

Do not use icons.

Do not create card-style sections.

Do not add excessive separators.

## Scrolling

If the content does not fit in a small window:

- make the shortcut list vertically scrollable

Do not allow the dialog to grow absurdly tall.

It should remain usable on smaller screens.

## Keyboard behaviour

When opened:

- focus should land sensibly inside the dialog
- Tab / Shift+Tab should not escape behind it
- Escape closes it
- mouse operation remains normal

No shortcut inside the list needs to be clickable.

This is a reference, not a command palette.

## FILE menu

Add the shortcut reference at the bottom of the existing FILE menu.

Use a separator before it if appropriate.

Example:

    New Notebook
    Open…
    Save
    Save As…
    ─────────────
    Recovery…
    ─────────────
    Keyboard Shortcuts    F1

Reuse the existing command-menu styling.

Do not add a traditional Help menu or menu bar.

## Tests

Preserve all existing tests.

Add/runtime-check at least:

1. F1 opens the shortcut reference
2. FILE menu entry opens the same reference
3. Escape closes it
4. opening it does not modify document dirty state
5. opening it does not trigger autosave
6. opening it does not create a checkpoint
7. current note selection remains unchanged
8. tree expansion state remains unchanged
9. Search state remains unchanged
10. shortcut labels shown match actual implemented bindings
11. light-theme rendering remains readable
12. dark-theme rendering remains readable

Do not build a complicated documentation framework.

A small source-level or runtime consistency check is sufficient where practical.

## Runtime verification

Exercise:

- F1 from tree focus
- F1 from editor focus
- F1 from Search
- FILE menu entry
- Escape close
- Tab / Shift+Tab
- dark theme
- light theme

Verify no QML warnings or binding loops.

## Verification

Run:

- cargo fmt --check
- cargo clippy --all-targets
- cargo test
- cargo build

Perform a clean offscreen launch.

## Out of scope

Do not:

- add a Help menu
- add a footer HELP command
- add a command palette
- add searchable documentation
- open a browser
- generate HTML docs
- add icons
- change existing shortcuts
- add new application features
- implement permanent integration-test infrastructure
- perform packaging
- perform unrelated cleanup