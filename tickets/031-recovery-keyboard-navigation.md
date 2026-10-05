# Ticket 031 — Recovery keyboard navigation

## Goal

Complete OmaTree's keyboard-only workflow by making the Recovery dialog fully
navigable without a mouse.

Current behaviour is incomplete:

- Tab can reach some controls
- only the top recovery entry is practically usable
- Up/Down do not select recovery entries
- there is no obvious keyboard-selected recovery item

The Recovery dialog should behave like a normal keyboard-navigable list.

Do not redesign Recovery or change its persistence semantics.

---

# Required list behaviour

When the active Recovery tab contains entries:

- one recovery row has a current selection
- that selection is visually obvious
- Up moves to the previous row
- Down moves to the next row
- Enter restores the selected row
- Space restores the selected row

Do not wrap at the top/bottom unless the existing ListView convention already
does so.

Prefer normal bounded list behaviour:

    Up on first row     -> stays on first row
    Down on last row    -> stays on last row

---

# Initial focus

When Recovery opens:

- preserve the existing active tab
- if that tab contains entries:
    - select the first row
    - give the recovery list keyboard focus
- if the active tab is empty:
    - give focus to a sensible dialog control, preferably the tab/header or
      Close

Do not automatically restore anything.

---

# Row selection

Use the list's real currentIndex/currentItem state as the source of truth.

Do not invent a second unrelated "selected recovery id" unless required by the
current architecture.

The selected row should have a restrained OmaTree-style visual treatment.

For example:

- subtle surface highlight
- accent edge/underline
- slightly stronger foreground

Do not add icons.

Do not use danger colour merely because Recovery can change state.

---

# Restore action

When the recovery list owns focus:

    Enter / Return -> restore current row
    Space          -> restore current row

This should invoke the exact same existing restore operation as clicking that
row's RESTORE command.

Do not duplicate Recovery logic.

After a successful restore:

- preserve existing Recovery semantics
- update the list as it currently does
- keep keyboard focus in a sensible place
- if the restored item disappears from the list, select the next valid item
  where possible
- if no entries remain, move focus somewhere safe such as Close or the tab bar

Do not leave currentIndex pointing at an invalid delegate.

---

# Trash and Checkpoints tabs

Preserve the two existing tabs:

    TRASH
    CHECKPOINTS

Tab / Shift+Tab must continue participating in normal focus traversal.

When the tab header owns focus:

    Left / Right -> switch between Trash and Checkpoints

If switching tabs:

- update the visible list
- if the destination contains entries, select its first row
- if empty, leave no invalid row selection
- do not trigger Restore

Do not make Left/Right switch tabs while the recovery list itself owns focus.

---

# Up / Down priority

When the recovery list owns focus:

- Up / Down must navigate Recovery entries
- they must not leak through to the tree behind the modal dialog
- they must not move dialog buttons
- they must not switch tabs

The modal Recovery surface owns those keys.

---

# Mouse behaviour

Preserve current mouse behaviour.

The existing RESTORE command remains clickable.

Clicking a row may also make it the current selection if that fits the current
delegate structure.

Do not require users to click the row before clicking RESTORE.

---

# Escape

Escape closes Recovery exactly as it does now.

It must never restore anything.

Focus should return to the previously appropriate application context.

---

# Close

The CLOSE command remains keyboard-accessible through normal Tab traversal.

Enter / Space on focused CLOSE closes Recovery.

Do not change the shared dialog behaviour fixed in Ticket 030.

---

# Empty states

For an empty active tab:

- Up / Down do nothing
- Enter / Space do nothing
- no phantom row is selected
- Tab / Shift+Tab still work
- Left / Right on tab focus may switch to the other tab
- Escape closes normally

Do not synthesize fake rows.

---

# Visual consistency

The keyboard-selected Recovery row must make it clear which entry Enter will
restore.

There must not be conflicting visual signals between:

- row selection
- RESTORE command hover
- RESTORE command focus
- tab focus

Keep hover and keyboard selection distinct but compatible.

Avoid the Ticket 030 problem where two different states looked "selected".

---

# Permanent integration test

Extend the permanent suite with a Recovery keyboard scenario.

Cover at least:

1. open Recovery with multiple checkpoint entries
2. first row is selected/focused
3. Down selects second row
4. Up returns to first row
5. Enter restores the currently selected row
6. Space restores the selected row
7. Up on first row stays valid
8. Down on last row stays valid
9. Escape closes without restoring
10. tree/editor shortcuts do not fire behind the modal dialog
11. switch Trash / Checkpoints using keyboard focus on the tab header
12. destination tab gets a valid first-row selection if non-empty
13. empty tab has no invalid currentIndex
14. Tab can reach Close
15. Enter on Close closes the dialog
16. mouse RESTORE behaviour still works
17. selection remains valid after an item disappears following restore

If restoring a checkpoint mutates the document as expected, preserve the
existing checkpoint/Recovery semantics exactly.

Do not add extra persistence behaviour merely for keyboard navigation.

---

# Real desktop verification

Perform a real keyboard-only pass:

    open Recovery
    Down
    Down
    Up
    Enter

and:

    open Recovery
    Tab / Shift+Tab
    Left / Right on tabs
    Down through entries
    Space to restore
    Escape to close

Confirm the visibly selected row always matches the row that Enter/Space will
restore.

Check both Trash and Checkpoints with at least two entries where practical.

---

# Verification

Run:

    cargo fmt --check
    cargo clippy --all-targets
    cargo test
    cargo test integration_tests
    cargo build
    cargo build --release

Run a clean offscreen launch.

No QML warnings or binding loops.

---

# Packaging

After this fix:

- rebuild the existing Arch package
- keep version 0.1.0
- run packaging/verify-package.sh
- do not publish/tag/release

Report the rebuilt package verification.

---

# Completion report

Report:

- Recovery focus/list architecture
- initial focus behaviour
- Up/Down implementation
- Enter/Space restore implementation
- tab keyboard behaviour
- empty-list behaviour
- visual selection implementation
- focus after restore
- permanent tests added/updated
- total test count
- integration scenario count
- real-desktop result
- rebuilt package result
- whether anything still blocks Ticket 032

Do not start Ticket 032.