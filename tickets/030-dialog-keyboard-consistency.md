# Ticket 030 — Dialog keyboard consistency

## Goal

Audit all OmaTree dialogs and make their keyboard interaction predictable,
visible and consistent.

The immediate bug is the Move to Trash confirmation:

- arrow keys change something internally
- the visual focus indication does not follow correctly
- NO remains underlined even when another action appears selected

This makes it unclear what Enter will activate.

Fix the underlying shared dialog/button behaviour where practical rather than
patching only the Trash dialog.

Do not redesign the dialog visuals.

---

# Interaction principle

A modal dialog must always have exactly one clearly visible keyboard target
when one of its actions owns focus.

The visual focus state must match the action that Enter or Space will trigger.

There must never be:

- one button looking selected
- another button actually owning focus/default activation

---

# Part 1 — Inventory all dialogs

Inspect all production QML for:

- Dialog
- ThemedDialog
- MessageDialog
- custom modal/popup confirmation surfaces
- dialog-specific action controls

Produce a concise inventory in the completion report.

Classify each dialog as:

1. binary confirmation
2. multi-choice destructive/save workflow
3. OK/close-only informational dialog
4. non-button modal surface such as Keyboard Shortcuts

Do not change behaviour until the full inventory is understood.

---

# Part 2 — Binary confirmation behaviour

For destructive Yes/No confirmation dialogs such as Move to Trash:

## Initial state

The safe/non-destructive action must receive initial keyboard focus.

For Move to Trash:

    NO

must be initially focused.

YES must never be the initial action.

## Arrow keys

Left / Right must move focus between the available actions.

For:

    NO    YES

the behaviour is:

    Right from NO -> YES
    Left from YES -> NO

Cycling/wrapping is acceptable if it is consistent:

    Right from YES -> NO
    Left from NO -> YES

If existing application conventions already prefer non-wrapping, retain that
instead.

The important requirement is that keyboard focus visibly moves.

## Enter

Enter / Return activates the currently focused action.

Examples:

    focused NO + Enter  -> cancel dialog
    focused YES + Enter -> perform destructive action

## Space

Space activates the currently focused action, consistent with normal button
behaviour.

## Escape

Escape always chooses the safe cancellation path.

For Move to Trash:

    Escape -> NO

Close the dialog and do nothing destructive.

## Direct letter shortcuts

For simple Yes/No dialogs:

    N -> No
    Y -> Yes

These shortcuts act immediately and close the dialog.

They must be case-insensitive.

They must not require Alt.

They must only be active while that modal dialog is active.

Do not allow the keystroke to leak to controls beneath the dialog.

---

# Part 3 — Visual focus state

Fix the current contradictory focus styling.

The action with actual keyboard focus must have one clear focus indication.

The exact existing OmaTree visual language may be retained, for example:

- accent colour
- underline
- both, if they unambiguously belong to the same focused item

But:

- an unfocused default/safe action must not retain the same focus underline
- colour and underline must not indicate different actions
- hover must not masquerade as keyboard focus
- stale hover state must not remain after keyboard navigation

The safe default may have separate semantics internally, but do not represent
"default" using the same decoration used for actual focus.

Use ActiveFocus/focus state as the source of truth where possible.

Do not maintain a second independent "selected button" state unless Qt's focus
model cannot provide the required behaviour.

---

# Part 4 — Multi-choice dialogs

Audit dialogs such as:

    Save / Discard / Cancel

or equivalent safe-to-leave workflows.

They should support keyboard-only operation.

Required baseline:

- one safe initial focus target
- Left / Right moves focus between action buttons
- Tab / Shift+Tab still work
- Enter/Return activates focused action
- Space activates focused action
- Escape maps to Cancel
- focus styling follows actual focus

Do NOT invent ambiguous single-letter shortcuts for every action.

For example:

    Save / Discard / Cancel

does not automatically require S/D/C unless existing OmaTree conventions make
them useful and unambiguous.

The purpose of this ticket is predictable keyboard behaviour, not creation of
a mnemonic system.

---

# Part 5 — Save As overwrite confirmation

If OmaTree has an overwrite Yes/No confirmation:

Treat overwriting as destructive.

Initial focus must be the safe choice.

Required:

    N -> do not overwrite
    Y -> overwrite
    Escape -> do not overwrite
    arrows -> move visible focus
    Enter -> activate focused action

Preserve all existing overwrite safety semantics.

---

# Part 6 — Recovery dialogs

Inspect Recovery-related dialogs.

For confirmation actions that would replace/restore state:

- safe action initially focused
- arrows navigate
- Enter activates focused action
- Escape cancels

Do not alter Recovery semantics, checkpoint state or persistence.

If Recovery is a list-selection dialog rather than a simple confirmation,
preserve list navigation and only normalize its action-button behaviour.

---

# Part 7 — Informational dialogs

For one-action dialogs such as an error or information dialog:

- Escape closes where currently safe
- Enter/Return activates the sole acknowledgement action
- Space activates it when focused

Do not add Y/N shortcuts to informational dialogs.

External-modification conflict dialogs must continue preserving dirty
in-memory state exactly as Ticket 028 specifies.

---

# Part 8 — Keyboard Shortcuts dialog

Preserve:

    F1 / Escape -> close

Do not force arrow-button semantics onto it if it has no action row.

Only adjust shared dialog code if necessary to prevent regressions.

---

# Part 9 — Shared implementation

Prefer fixing this in shared components such as:

- ThemedDialog
- Command-style dialog buttons/items
- shared action-row helpers

rather than duplicating Keys handlers across every dialog.

However, do not build a large dialog framework immediately before v0.1.

Use the smallest shared abstraction that gives correct behaviour.

Possible implementation pattern:

- explicit initialFocusItem
- normal Qt ActiveFocus on each action
- KeyNavigation.left/right between actions
- Enter/Return/Space activation
- dialog-level Escape
- dialog-specific N/Y shortcuts only for binary confirmations

Follow the actual current architecture rather than forcing this exact shape.

---

# Part 10 — Focus priority

Modal dialogs own keyboard interaction while open.

While a modal dialog is visible:

- tree shortcuts must not fire
- editor Escape-to-tree must not fire
- Search shortcuts must not fire beneath the dialog
- Delete must not cause a second Trash dialog
- N/Y must not type into hidden/underlying controls

After the dialog closes:

- focus should return sensibly to the control/context that opened it
- for Trash cancellation, the original selected tree note should remain
  selected and tree navigation should continue normally

---

# Part 11 — Move to Trash exact regression

Add permanent integration coverage reproducing the user's reported workflow.

Starting with tree focus and a selected note:

1. press Delete
2. Move to Trash dialog opens
3. NO is visibly/factually focused
4. press Right
5. YES now owns focus
6. NO no longer has focus styling
7. press Left
8. NO owns focus again
9. press Right
10. press Enter
11. note moves to Trash

Also test:

    Delete -> N
        dialog closes
        note remains

    Delete -> Escape
        dialog closes
        note remains

    Delete -> Y
        note moves to Trash

And:

    Delete -> Right -> Enter
        note moves to Trash

    Delete -> Enter
        initial NO is activated
        note remains

After cancellation, Up/Down should work in the tree immediately.

---

# Part 12 — Other-dialog integration coverage

Add permanent coverage for the important dialog classes found during the
inventory.

At minimum verify:

- Trash Yes/No
- dirty-document Save/Discard/Cancel
- overwrite confirmation, if testable without a native dialog
- Recovery confirmation/action row, if present
- an informational/error dialog
- F1 still closes normally

Do not duplicate native file-dialog testing that the current headless suite
cannot safely exercise.

Test shared focus/action mechanics once where sufficient, plus critical
destructive workflows individually.

---

# Part 13 — F1 shortcut reference

Check whether the existing Keyboard Shortcuts reference should mention dialog
behaviour.

If useful, General may say something concise such as:

    Arrow keys    Choose dialog action
    Enter         Activate dialog action
    Escape        Cancel / close dialog

Do not clutter F1 with N/Y unless they are consistently implemented for all
Yes/No confirmations and genuinely useful to document.

---

# Part 14 — Verification

Run:

    cargo fmt --check
    cargo clippy --all-targets
    cargo test
    cargo test integration_tests
    cargo build
    cargo build --release

Because this bug was discovered in the installed package, perform a real
desktop check.

Verify manually:

    tree -> Delete
    Right / Left
    Enter
    N
    Y
    Escape

Confirm that the focus decoration always matches the action Enter will invoke.

Check one other multi-choice dialog with keyboard only.

Run a clean offscreen launch.

No QML warnings or binding loops.

---

# Packaging

Ticket 029's installed package now contains the pre-fix dialog behaviour.

After Ticket 030 passes:

- rebuild the Arch package using the existing Ticket 029 tooling
- verify the rebuilt package
- do not bump the application version
- do not publish it
- do not create a Git tag

This is still v0.1.0 pre-release development.

Report the rebuilt package filename and verification result.

---

# Completion report

Report:

- inventory of dialogs found
- root cause of the Trash visual/focus mismatch
- shared implementation changed
- exact initial focus rules
- arrow-key behaviour
- Enter/Space behaviour
- Escape behaviour
- N/Y behaviour
- stale hover/focus handling
- behaviour of each other dialog class
- F1 changes, if any
- permanent tests added/updated
- total Rust test count
- integration scenario count
- real-desktop verification
- rebuilt package verification
- whether anything now blocks Ticket 031

Do not start Ticket 031.