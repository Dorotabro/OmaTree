//! Recovery (Trash and checkpoints), the F1 reference and the Search additions of ticket E7b.
//! Behaviour follows the Qt app (`RecoveryDialog.qml`, `tests/qml/{recovery,recoverykeys,dialogs,
//! escape}.qml`) except where `docs/EGUI_MIGRATION_PLAN.md` records a deviation. Everything runs
//! through real frames of a `Host` on copies of a notebook.

use super::search_tests::search_nb;
use super::tree_tests::shape;
use super::*;
use crate::recovery_ui::format_utc;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const SHOT: &str = "![shot](assets/12.png)\n\nand ![wide](assets/7.jpg \"2\")\n";

/// X(Y(Z)), Keep, Solo. Z carries Markdown image references to attachments that do not exist
/// (storage for them arrives in E9): they are text and must stay exactly as written.
fn make_recovery(path: &Path) {
    let mut doc = Document::open(path).unwrap();
    let x = doc.create_root("X").unwrap();
    let y = doc.create_child(x, "Y").unwrap();
    let z = doc.create_child(y, "Z").unwrap();
    let keep = doc.create_root("Keep").unwrap();
    let solo = doc.create_root("Solo").unwrap();
    for (id, body) in [
        (x, "x body\n"),
        (y, "y body\n"),
        (z, SHOT),
        (keep, "keep body\n"),
        (solo, "solo body\n"),
    ] {
        doc.set_body(id, body).unwrap();
    }
    doc.save().unwrap();
}

fn rec_nb(name: &str) -> (crate::tests::TestDir, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    make_recovery(&p);
    (d, p)
}

const CTRL_SHIFT: Modifiers = Modifiers::COMMAND.plus(Modifiers::SHIFT);

impl H {
    fn open_rec(name: &str) -> (crate::tests::TestDir, PathBuf, H) {
        let (d, p) = rec_nb(name);
        let mut h = H::open_tree(&p);
        // Wide enough that the centred dialog starts to the right of the tree pane, which the
        // `text_in(.., false)` lookups take as "not in the tree".
        h.screen = Vec2::new(1300.0, 700.0);
        h.settle();
        (d, p, h)
    }

    fn recovery(&mut self) {
        self.key(Key::R, CTRL_SHIFT);
        self.settle();
    }

    fn rec_open(&self) -> bool {
        *self.host.dialog() == Dialog::Recovery
    }

    fn rec_list_focus(&self) -> bool {
        self.ctx.memory(|m| m.has_focus(Host::recovery_list_id()))
    }

    /// From the list: Shift+Tab to the header, Right for Checkpoints, Tab back into the list.
    fn go_to_checkpoints(&mut self) {
        self.key(Key::Tab, Modifiers::SHIFT);
        self.press(Key::ArrowRight);
        self.press(Key::Tab);
    }

    fn rec_tabs_focus(&self) -> bool {
        self.ctx.memory(|m| m.has_focus(Host::recovery_tabs_id()))
    }

    /// Delete `title` (and everything below it) the way the Delete key does after "Move to Trash".
    fn trash(&mut self, title: &str) {
        let ctx = self.ctx.clone();
        let id = self.id(title);
        self.host.select(&ctx, id);
        self.host.confirm_delete(&ctx, id);
        self.settle();
    }

    fn titles(&self) -> Vec<String> {
        self.host.rows().iter().map(|r| r.title.clone()).collect()
    }

    fn doc_shape(&self) -> String {
        shape(self.host.document().unwrap())
    }

    fn type_into(&mut self, title: &str, add: &str) {
        self.select(title);
        let end = self.host.buffer().len();
        self.set_caret(end, end);
        self.type_text(add);
    }

    fn reasons(&self) -> Vec<String> {
        self.host
            .document()
            .unwrap()
            .recovery()
            .checkpoints()
            .iter()
            .map(|c| c.reason().to_owned())
            .collect()
    }
}

// --- The lists ------------------------------------------------------------------------------

#[test]
fn recovery_opens_on_empty_lists_with_the_safe_button_focused() {
    let (_d, _p, mut h) = H::open_rec("rec-empty");
    h.recovery();
    assert!(h.rec_open());
    assert_eq!(h.host.recovery_tab(), RecoveryTab::Trash);
    assert_eq!(h.host.recovery_counts(), (0, 0));
    assert!(h.host.recovery_selected().is_none());
    h.text_in("Trash is empty", false);
    assert!(!h.rec_list_focus(), "no list to focus when it is empty");
    // The header still switches tabs, and the other list says it is empty too.
    h.key(Key::Tab, Modifiers::SHIFT);
    assert!(h.rec_tabs_focus());
    h.press(Key::ArrowRight);
    assert_eq!(h.host.recovery_tab(), RecoveryTab::Checkpoints);
    h.text_in("No checkpoints yet", false);
    h.press(Key::ArrowLeft);
    assert_eq!(h.host.recovery_tab(), RecoveryTab::Trash);
    // Enter on the focused Close button closes it and restores focus.
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.has_focus(Focus::Tree));
}

#[test]
fn trash_and_checkpoints_list_newest_first_with_dates_paths_and_counts() {
    let (_d, _p, mut h) = H::open_rec("rec-order");
    for t in ["Keep", "Solo", "X"] {
        h.trash(t);
    }
    h.recovery();
    assert_eq!(h.host.recovery_counts().0, 3);
    assert_eq!(
        h.host.recovery_titles(),
        ["X", "Solo", "Keep"],
        "newest first"
    );
    h.text_in("X", false);
    // A three-note subtree says so, and where it comes back to.
    let lines = h.painted();
    assert!(lines
        .texts
        .iter()
        .any(|t| t.text.ends_with("\u{b7} 3 notes") && t.text.starts_with("Deleted 20")));
    assert!(lines
        .texts
        .iter()
        .any(|t| t.text.ends_with("\u{b7} 1 note")));
    assert!(lines
        .texts
        .iter()
        .any(|t| t.text == "Returns to the top level"));
    h.key(Key::Tab, Modifiers::SHIFT);
    h.press(Key::ArrowRight);
    let shown = h.host.recovery_titles();
    let mut core: Vec<String> = h.reasons();
    core.reverse();
    assert_eq!(shown, core, "checkpoints too, newest first");
    assert!(shown[0].contains("Before deleting") || !shown.is_empty());
    h.press(Key::Escape);
}

#[test]
fn a_nested_deleted_note_says_where_it_returns() {
    let (_d, _p, mut h) = H::open_rec("rec-where");
    h.trash("Z");
    h.recovery();
    h.text_in("Returns to X \u{203a} Y", false);
    h.press(Key::Escape);
}

#[test]
fn dates_are_written_as_utc() {
    assert_eq!(format_utc(0), "1970-01-01 00:00 UTC");
    assert_eq!(format_utc(1_700_000_000), "2023-11-14 22:13 UTC");
    assert_eq!(format_utc(951_782_400), "2000-02-29 00:00 UTC");
    assert_eq!(format_utc(-1), "1969-12-31 23:59 UTC");
}

// --- Trash --------------------------------------------------------------------------------------

#[test]
fn enter_restores_a_deleted_leaf_selects_and_reveals_it() {
    let (_d, _p, mut h) = H::open_rec("rec-leaf");
    h.trash("Keep");
    assert!(!h.titles().contains(&"Keep".to_owned()));
    h.recovery();
    assert!(h.rec_list_focus(), "the list has the keyboard on open");
    assert_eq!(h.host.recovery_selected(), Some(0));
    h.press(Key::Enter);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.titles().contains(&"Keep".to_owned()));
    assert_eq!(h.sel(), "Keep");
    assert_eq!(h.host.buffer(), "keep body\n");
    assert!(h.has_focus(Focus::Tree));
    assert!(h.host.document().unwrap().recovery().trash().is_empty());
    assert!(
        h.host.status().starts_with("Restored"),
        "{}",
        h.host.status()
    );
    h.text_in("Keep", true);
}

#[test]
fn a_whole_nested_subtree_comes_back_with_its_bodies_and_places() {
    let (_d, _p, mut h) = H::open_rec("rec-subtree");
    let before = h.doc_shape();
    let bodies: Vec<String> = ["X", "Y", "Z"].iter().map(|t| h.body(t)).collect();
    h.trash("X");
    assert_eq!(h.doc_shape(), "Keep|Solo");
    h.recovery();
    h.press(Key::Space);
    assert_eq!(h.doc_shape(), before);
    let now: Vec<String> = ["X", "Y", "Z"].iter().map(|t| h.body(t)).collect();
    assert_eq!(now, bodies);
    assert_eq!(h.sel(), "X");
    // Everything beneath it can be opened and shown.
    h.select("Z");
    assert_eq!(h.host.buffer(), SHOT);
}

#[test]
fn markdown_attachment_references_are_kept_exactly_as_text() {
    let (_d, p, mut h) = H::open_rec("rec-assets");
    h.trash("X");
    h.recovery();
    h.press(Key::Enter);
    assert_eq!(h.body("Z"), SHOT, "Trash restore");
    h.type_into("Keep", " edit");
    h.trash("X");
    h.recovery();
    h.go_to_checkpoints();
    h.press(Key::Enter);
    assert!(h.host.recovery_confirming());
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(h.body("Z"), SHOT, "checkpoint restore");
    h.key(Key::S, CTRL);
    assert!(snapshot(&p).iter().any(|r| r.4 == SHOT), "and on disk");
}

#[test]
fn a_restore_the_notebook_refuses_stays_in_trash_and_changes_nothing() {
    let (_d, _p, mut h) = H::open_rec("rec-conflict");
    h.trash("Keep");
    // A different note now has the name the deleted one would return to.
    let ctx = h.ctx.clone();
    h.host.create_root(&ctx);
    h.type_text("Keep");
    h.press(Key::Enter);
    h.settle();
    assert!(h.titles().contains(&"Keep".to_owned()));
    let shape_before = h.doc_shape();
    let dirty_before = h.host.is_dirty();
    h.host.focus(Focus::Tree);
    h.settle();
    h.recovery();
    h.text_in("A note with the same name already exists where this would be restored. Rename or move it first.", false);
    h.press(Key::Enter);
    assert!(h.rec_open(), "the dialog stays open");
    assert!(h
        .host
        .recovery_error()
        .is_some_and(|e| e.contains("same name")));
    assert_eq!(h.host.recovery_counts().0, 1, "the entry is still in Trash");
    assert_eq!(h.doc_shape(), shape_before);
    assert_eq!(h.host.is_dirty(), dirty_before);
    assert_eq!(h.host.document().unwrap().recovery().trash().len(), 1);
    h.press(Key::Escape);
    // After renaming the other note it can be restored.
    h.go("Keep");
    h.press(Key::F2);
    h.type_text("Kept");
    h.press(Key::Enter);
    h.settle();
    h.host.focus(Focus::Tree);
    h.settle();
    h.recovery();
    h.press(Key::Enter);
    assert!(h.titles().contains(&"Keep".to_owned()) && h.titles().contains(&"Kept".to_owned()));
}

#[test]
fn restoring_twice_or_a_stale_entry_is_refused_without_harm() {
    let (_d, _p, mut h) = H::open_rec("rec-twice");
    h.trash("Solo");
    let ctx = h.ctx.clone();
    h.recovery();
    assert!(h.host.restore_trash(&ctx, 0));
    let shape_after = h.doc_shape();
    h.recovery();
    assert!(!h.host.restore_trash(&ctx, 0), "nothing left to restore");
    assert!(h
        .host
        .recovery_error()
        .is_some_and(|e| e.contains("no longer available")));
    assert!(!h.host.restore_checkpoint(&ctx, 99));
    assert_eq!(h.doc_shape(), shape_after);
    h.press(Key::Escape);
}

#[test]
fn restoring_leaves_other_notes_and_their_editor_state_alone() {
    let (_d, _p, mut h) = H::open_rec("rec-others");
    h.type_into("Solo", "!1");
    let caret = h.caret("Solo");
    let before: Vec<String> = ["Keep", "Solo"].iter().map(|t| h.body(t)).collect();
    h.trash("X");
    h.go("Solo");
    h.host.focus(Focus::Editor);
    h.settle();
    h.recovery();
    h.press(Key::Enter);
    let after: Vec<String> = ["Keep", "Solo"].iter().map(|t| h.body(t)).collect();
    assert_eq!(after, before);
    h.select("Solo");
    assert_eq!(h.caret("Solo"), caret, "caret kept");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Solo"), "solo body\n", "its undo history is intact");
}

// --- Checkpoints --------------------------------------------------------------------------------

/// Keep edited, Solo deleted (a checkpoint of that moment), then more changes.
fn with_checkpoint(name: &str) -> (crate::tests::TestDir, PathBuf, H) {
    let (d, p, mut h) = H::open_rec(name);
    h.type_into("Keep", " one");
    h.trash("Solo");
    h.type_into("Keep", " two");
    let ctx = h.ctx.clone();
    h.host.create_root(&ctx);
    h.type_text("Extra");
    h.press(Key::Enter);
    h.settle();
    h.host.focus(Focus::Tree);
    h.settle();
    (d, p, h)
}

fn checkpoint_tab(h: &mut H) {
    h.recovery();
    h.go_to_checkpoints();
}

#[test]
fn a_checkpoint_asks_first_and_cancelling_changes_nothing() {
    let (_d, _p, mut h) = with_checkpoint("rec-confirm");
    let shape_before = h.doc_shape();
    let reasons = h.reasons();
    checkpoint_tab(&mut h);
    assert_eq!(h.host.recovery_tab(), RecoveryTab::Checkpoints);
    h.text_in("Restoring replaces the whole notebook with how it was at that time. You will be asked to confirm.", false);
    h.press(Key::Enter);
    assert!(h.host.recovery_confirming());
    h.texts_starting("Replace the notebook?");
    // The safe answer has the keyboard: Enter cancels.
    h.press(Key::Enter);
    assert!(!h.host.recovery_confirming() && h.rec_open());
    assert_eq!(h.doc_shape(), shape_before);
    // Escape also steps back, and a second Escape leaves.
    h.press(Key::Enter);
    assert!(h.host.recovery_confirming());
    h.press(Key::Escape);
    assert!(!h.host.recovery_confirming() && h.rec_open());
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(h.doc_shape(), shape_before);
    assert_eq!(
        h.reasons(),
        reasons,
        "no checkpoint was taken by cancelling"
    );
}

#[test]
fn restoring_a_checkpoint_replaces_the_notebook_and_checkpoints_what_it_replaced() {
    let (_d, _p, mut h) = with_checkpoint("rec-replace");
    assert!(h.titles().contains(&"Extra".to_owned()));
    let count = h.reasons().len();
    checkpoint_tab(&mut h);
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert_eq!(*h.host.dialog(), Dialog::None);
    // The state at the delete: Solo present, Extra not yet created, Keep " one".
    assert_eq!(h.doc_shape(), "X(Y(Z))|Keep|Solo");
    assert_eq!(h.body("Keep"), "keep body\n one");
    assert_eq!(
        h.reasons().len(),
        count + 1,
        "what was replaced was checkpointed"
    );
    assert!(h
        .reasons()
        .last()
        .unwrap()
        .starts_with("Before restoring checkpoint"));
    assert!(h.host.is_dirty());
    assert!(h.has_focus(Focus::Tree));
    // And it can be undone from Recovery: the newest checkpoint is the state just replaced.
    h.recovery();
    h.go_to_checkpoints();
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert!(h.titles().contains(&"Extra".to_owned()));
    assert_eq!(h.body("Keep"), "keep body\n one two");
}

#[test]
fn unsaved_edits_and_the_pending_buffer_are_part_of_what_is_checkpointed() {
    let (_d, _p, mut h) = H::open_rec("rec-unsaved");
    h.type_into("Keep", " precious");
    assert!(h.host.is_dirty());
    h.trash("Solo");
    h.type_into("Keep", " typed after");
    h.host.focus(Focus::Editor);
    h.settle();
    h.recovery(); // flushes the editor first
    h.press(Key::Enter);
    assert_eq!(
        h.body("Keep"),
        "keep body\n precious typed after",
        "nothing was lost"
    );
    let newest = h.reasons().last().cloned().unwrap();
    assert!(newest.starts_with("Before restoring"), "{newest}");
    // That checkpoint holds the unsaved text.
    h.recovery();
    h.go_to_checkpoints();
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert_eq!(h.body("Keep"), "keep body\n precious typed after");
}

#[test]
fn an_invalid_pending_title_blocks_recovery_and_is_not_discarded() {
    let (_d, _p, mut h) = H::open_rec("rec-title");
    h.go("Keep");
    h.press(Key::F2);
    h.press(Key::Backspace);
    assert_eq!(h.host.title_text(), "");
    h.key(Key::R, CTRL_SHIFT);
    h.settle();
    assert!(
        matches!(h.host.dialog(), Dialog::Error(_)),
        "the refusal is explained"
    );
    assert!(!h.rec_open());
    assert_eq!(h.host.title_text(), "", "the typed text is still there");
    h.press(Key::Enter);
    h.type_text("Keep");
    h.press(Key::Enter);
    h.settle();
    h.host.focus(Focus::Tree);
    h.settle();
    h.recovery();
    assert!(h.rec_open(), "valid again: Recovery opens");
    h.press(Key::Escape);
}

#[test]
fn checkpoint_restore_forgets_undo_of_changed_notes_and_keeps_it_for_unchanged_ones() {
    let (_d, _p, mut h) = H::open_rec("rec-undo");
    h.type_into("Solo", " s1");
    h.trash("X"); // checkpoint: Keep and Solo as they are now
    h.type_into("Keep", " k1"); // changed after the checkpoint, with undo history
    let keep_now = h.body("Keep");
    h.go("Solo");
    checkpoint_tab(&mut h);
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert_ne!(
        h.body("Keep"),
        keep_now,
        "Keep went back to its checkpointed text"
    );
    let restored = h.body("Keep");
    // Ctrl+Z in Keep cannot bring ` k1` back: its history was forgotten.
    h.select("Keep");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Keep"), restored);
    // Solo is the same text before and after: its history is still its own.
    h.select("Solo");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Solo"), "solo body\n");
}

#[test]
fn restoring_the_selected_note_and_a_checkpoint_that_lacks_it_both_leave_a_valid_state() {
    let (_d, _p, mut h) = H::open_rec("rec-selected");
    h.go("Y");
    h.trash("X");
    assert_eq!(
        h.host
            .selected()
            .map(|id| h.host.tree().row(id).unwrap().title.clone()),
        Some("Keep".into())
    );
    h.recovery();
    h.press(Key::Enter);
    assert_eq!(h.sel(), "X", "the restored root is selected");
    h.select("Z");
    assert_eq!(h.host.buffer(), SHOT);
    // A checkpoint from before Z existed would drop the selected note: build one.
    let ctx = h.ctx.clone();
    h.host.select(&ctx, h.id("Keep"));
    h.host.create_child(&ctx);
    h.type_text("Child");
    h.press(Key::Enter);
    h.settle();
    h.host.focus(Focus::Tree);
    h.settle();
    assert_eq!(h.sel(), "Child");
    // The newest checkpoint predates "Child".
    checkpoint_tab(&mut h);
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert!(!h.titles().contains(&"Child".to_owned()));
    let sel = h.host.selected().expect("a note is selected");
    assert!(
        h.host.tree().contains(sel),
        "the selection names a note that exists"
    );
    assert_eq!(
        h.host.buffer(),
        h.host
            .document()
            .unwrap()
            .notebook()
            .get(sel)
            .unwrap()
            .body(),
        "and the editor shows its text"
    );
    assert!(h.host.title_text() == h.host.tree().row(sel).unwrap().title);
}

// --- Search, keyboard, safety ----------------------------------------------------------------------

#[test]
fn search_open_during_a_restore_follows_the_restored_notes() {
    let (_d, _p, mut h) = H::open_rec("rec-search");
    h.trash("X");
    h.ctrl_f();
    h.type_text("x body");
    assert!(h.found().is_empty(), "X is in Trash, not searchable");
    h.recovery(); // from the search field
    assert!(h.rec_open());
    h.press(Key::Enter);
    h.advance(0.3);
    assert!(h.host.search_open(), "Search stays open");
    assert_eq!(h.found(), ["X"], "and finds the restored note");
    // A checkpoint restore refreshes it too.
    h.ctrl_f();
    h.type_text("keep body");
    assert_eq!(h.found(), ["Keep"]);
    h.type_into("Keep", " zzz");
    h.recovery();
    h.go_to_checkpoints();
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.advance(0.3);
    assert!(h.host.search_scans() > 0);
    assert_eq!(h.found().len(), 1);
}

#[test]
fn the_dialog_is_modal_and_its_keys_stay_inside_it() {
    let (_d, _p, mut h) = H::open_rec("rec-modal");
    h.trash("Keep");
    h.trash("Solo");
    h.trash("Z");
    h.host.focus(Focus::Editor);
    h.settle();
    let body_before = h.host.buffer().to_owned();
    h.recovery();
    assert!(h.rec_open() && h.rec_list_focus());
    // Up and Down move a bounded selection and move nothing beneath.
    assert_eq!(h.host.recovery_selected(), Some(0));
    h.press(Key::ArrowDown);
    h.press(Key::ArrowDown);
    h.press(Key::ArrowDown);
    assert_eq!(h.host.recovery_selected(), Some(2));
    h.press(Key::ArrowUp);
    h.press(Key::Home);
    assert_eq!(h.host.recovery_selected(), Some(0));
    h.press(Key::End);
    assert_eq!(h.host.recovery_selected(), Some(2));
    h.press(Key::PageUp);
    assert_eq!(h.host.recovery_selected(), Some(0));
    // Left and Right in the list do not switch tabs; Delete, typing and Ctrl+F go nowhere.
    h.press(Key::ArrowRight);
    assert_eq!(h.host.recovery_tab(), RecoveryTab::Trash);
    h.press(Key::Delete);
    h.type_text("typed behind the dialog");
    h.ctrl_f();
    h.key(Key::N, CTRL);
    assert!(h.rec_open());
    assert!(!h.host.search_open());
    assert_eq!(h.host.recovery_counts().0, 3);
    assert_eq!(h.host.buffer(), body_before);
    assert!(!h.host.is_dirty() || h.reasons().len() >= 3);
    // Tab goes header -> list -> Close and Shift+Tab goes back; Close closes.
    h.key(Key::Tab, Modifiers::SHIFT);
    assert!(h.rec_tabs_focus(), "header");
    h.press(Key::Tab);
    assert!(h.rec_list_focus());
    h.press(Key::Tab);
    assert!(!h.rec_list_focus() && !h.rec_tabs_focus(), "Close");
    h.press(Key::Enter);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.has_focus(Focus::Editor), "focus is back where it was");
    assert_eq!(h.host.recovery_counts().0, 3, "closing restored nothing");
}

#[test]
fn escape_and_close_never_restore_and_enter_restores_exactly_the_marked_row() {
    let (_d, _p, mut h) = H::open_rec("rec-escape");
    for t in ["Keep", "Solo", "X"] {
        h.trash(t);
    }
    h.recovery();
    h.press(Key::ArrowDown); // Solo
    h.press(Key::Enter);
    assert_eq!(h.sel(), "Solo");
    assert_eq!(h.host.document().unwrap().recovery().trash().len(), 2);
    h.recovery();
    assert_eq!(h.host.recovery_titles(), ["X", "Keep"]);
    assert_eq!(
        h.host.recovery_selected(),
        Some(0),
        "reopens on the first row"
    );
    let shape_before = h.doc_shape();
    h.press(Key::Escape);
    assert_eq!(h.doc_shape(), shape_before);
    assert!(h.has_focus(Focus::Tree));
}

#[test]
fn a_mouse_click_on_restore_restores_that_row() {
    let (_d, _p, mut h) = H::open_rec("rec-click");
    h.trash("Keep");
    h.trash("Solo");
    h.recovery();
    h.advance(0.5);
    // The second row ("Keep"): its Restore command sits at the right of its title line.
    let title = h.text_in("Keep", false);
    let restore: Vec<_> = h
        .painted()
        .texts
        .into_iter()
        .filter(|t| t.text == "Restore")
        .collect();
    assert_eq!(restore.len(), 2);
    let mine = restore
        .iter()
        .min_by(|a, b| {
            (a.centre_y() - title.centre_y())
                .abs()
                .total_cmp(&(b.centre_y() - title.centre_y()).abs())
        })
        .unwrap();
    let at = Pos2::new(mine.pos.x + 4.0, mine.centre_y());
    h.press_at(at);
    h.release_at(at);
    assert_eq!(h.sel(), "Keep");
    assert_eq!(h.host.document().unwrap().recovery().trash().len(), 1);
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn recovery_marks_the_notebook_for_autosave_and_a_save_survives_a_reopen() {
    let (_d, p, mut h) = H::open_rec("rec-autosave");
    h.trash("X");
    h.key(Key::S, CTRL);
    assert!(!h.host.is_dirty());
    h.recovery();
    h.press(Key::Enter);
    assert!(h.host.is_dirty(), "a restore is a change");
    h.advance(AUTOSAVE_DELAY + 0.5);
    assert!(!h.host.is_dirty(), "and autosave writes it");
    let raw = fs::read(&p).unwrap();
    assert_eq!(&raw[60..64], &[0, 0, 0, 3], "still a schema v3 file");
    let doc = Document::open_existing(&p).unwrap();
    assert_eq!(shape(&doc), "X(Y(Z))|Keep|Solo");
    assert!(
        doc.recovery().trash().is_empty(),
        "the entry left Trash on disk too"
    );
    assert!(doc
        .recovery()
        .checkpoints()
        .iter()
        .any(|c| c.reason().starts_with("Before restoring")));
    assert_eq!(
        doc.notebook().get(h.id("Z")).unwrap().body(),
        SHOT,
        "references intact on disk"
    );
}

#[test]
fn an_externally_modified_notebook_keeps_the_restore_in_memory_and_reports_the_conflict() {
    let (_d, p, mut h) = H::open_rec("rec-conflict-file");
    h.trash("Solo");
    h.key(Key::S, CTRL);
    // Someone else writes the file.
    {
        let mut other = Document::open_existing(&p).unwrap();
        other.create_root("Elsewhere").unwrap();
        other.save().unwrap();
    }
    h.recovery();
    h.press(Key::Enter);
    assert!(h.titles().contains(&"Solo".to_owned()));
    h.advance(AUTOSAVE_DELAY + 0.5);
    assert!(h.host.is_dirty(), "not written over the other change");
    assert!(h.host.autosave_blocked());
    assert!(h.host.autosave_problem().is_some());
    assert!(
        h.titles().contains(&"Solo".to_owned()),
        "the restore is not lost"
    );
    assert!(
        !snapshot(&p).iter().any(|r| r.3 == "Solo"),
        "the file still has the other version"
    );
}

#[cfg(unix)]
#[test]
fn a_save_that_fails_after_a_restore_keeps_the_restored_notes() {
    let (_d, p, mut h) = H::open_rec("rec-savefail");
    h.trash("Solo");
    h.key(Key::S, CTRL);
    h.recovery();
    h.press(Key::Enter);
    // SQLite needs to create a journal next to the file.
    let nb = p.parent().unwrap().to_path_buf();
    fs::set_permissions(&nb, fs::Permissions::from_mode(0o555)).unwrap();
    h.key(Key::S, CTRL);
    fs::set_permissions(&nb, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(h.host.is_dirty());
    assert!(h.titles().contains(&"Solo".to_owned()));
    h.key(Key::S, CTRL);
    assert!(!h.host.is_dirty());
}

#[test]
fn a_long_title_and_a_narrow_window_still_fit_the_dialog() {
    let d = dir("rec-narrow");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let long = doc
        .create_root("An extraordinarily long note title that cannot fit on one line of a narrow dialog \u{17e}lu\u{165}ou\u{10d}k\u{fd}")
        .unwrap();
    doc.set_body(long, "x").unwrap();
    doc.create_root("Other").unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_sized(&p, Vec2::new(420.0, 360.0));
    h.host.focus(Focus::Tree);
    h.settle();
    h.trash("An extraordinarily long note title that cannot fit on one line of a narrow dialog \u{17e}lu\u{165}ou\u{10d}k\u{fd}");
    h.recovery();
    h.advance(0.5);
    let screen = h.screen;
    for t in h
        .painted()
        .texts
        .into_iter()
        .filter(|t| !t.text.starts_with('\u{25cf}'))
    {
        // (A right-aligned label's galley is anchored at its right edge: the status bar's.)
        assert!(
            t.pos.x >= 0.0 && t.right() <= screen.x,
            "{:?} inside the window: {} .. {}",
            t.text,
            t.pos.x,
            t.right()
        );
    }
    assert!(h
        .painted()
        .texts
        .iter()
        .any(|t| t.elided && t.text.starts_with("An extraordinarily")));
    h.press(Key::Escape);
}

// --- F1 -----------------------------------------------------------------------------------------

#[test]
fn f1_lists_the_real_shortcuts_and_not_the_retired_ones() {
    let (_d, _p, mut h) = H::open_rec("rec-f1");
    h.press(Key::F1);
    assert_eq!(*h.host.dialog(), Dialog::Shortcuts);
    h.advance(0.5);
    let ctx = h.ctx.clone();
    // What is printed is what the key map says, section by section.
    let sections = h.host.shortcut_sections(&ctx);
    let mut texts: Vec<String> = sections
        .iter()
        .flat_map(|(h, _, e)| {
            std::iter::once(h.to_string())
                .chain(e.iter().flat_map(|(k, w)| [k.clone(), (*w).to_owned()]))
        })
        .collect();
    let has = |texts: &[String], s: &str| texts.iter().any(|t| t == s);
    for (s, what) in [
        (keymap::SAVE, "Save (an untitled notebook asks where)"),
        (keymap::OPEN, "Open a notebook"),
        (
            keymap::NEW_NOTEBOOK,
            "New notebook (untitled, in memory until saved)",
        ),
        (keymap::SAVE_AS, "Save As"),
        (keymap::NEW_NOTE, "New note"),
        (keymap::NEW_CHILD, "New child note"),
        (keymap::FIND, "Go to the search field"),
        (keymap::RECOVERY, "Recovery: Trash and checkpoints"),
        (keymap::HELP, "This reference"),
        (keymap::MOVE_UP, "Move the note up among its siblings"),
    ] {
        let label = ctx.format_shortcut(&s);
        assert!(has(&texts, &label), "{what}: {label}");
        assert!(has(&texts, what), "{what}");
    }
    // The editor's own bindings, read from its key map.
    let keys = h.host.keys;
    for s in [
        keys.bold,
        keys.italic,
        keys.code,
        keys.bullets,
        keys.numbers,
        keys.toggle_task,
    ]
    .into_iter()
    .flatten()
    {
        assert!(has(&texts, &ctx.format_shortcut(&s)), "{s:?}");
    }
    assert!(has(&texts, "Bold") && has(&texts, "Inline code"));
    assert!(
        has(&texts, "/") && has(&texts, "/strike"),
        "slash menu and strikethrough command"
    );
    assert!(
        !texts.iter().any(|t| t.to_lowercase().contains("preview")),
        "the retired Edit/Preview toggle is not listed"
    );
    assert!(!texts.iter().any(|t| t.contains("N / Y")));
    // And the dialog paints the top of it.
    let painted: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    texts.clear();
    assert!(has(&painted, "Keyboard shortcuts") && has(&painted, "NOTES"));
    assert!(has(&painted, &ctx.format_shortcut(&keymap::NEW_NOTE)));
    // Escape closes it and the keyboard returns to where it was.
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.has_focus(Focus::Tree));
}

#[test]
fn f1_toggles_scrolls_and_does_nothing_behind_another_question() {
    let (_d, _p, mut h) = H::open_rec("rec-f1b");
    h.host.focus(Focus::Editor);
    h.settle();
    h.press(Key::F1);
    assert_eq!(*h.host.dialog(), Dialog::Shortcuts);
    h.press(Key::PageDown);
    h.press(Key::End);
    h.press(Key::Home);
    assert!(!h.host.is_dirty(), "keys in the reference change nothing");
    h.press(Key::F1);
    assert_eq!(*h.host.dialog(), Dialog::None, "F1 again closes it");
    assert!(h.has_focus(Focus::Editor));
    h.recovery();
    h.press(Key::F1);
    assert!(h.rec_open(), "F1 is ignored while another dialog is open");
    h.press(Key::Escape);
}

// --- Search additions -----------------------------------------------------------------------------

#[test]
fn page_keys_move_through_results_and_matches_are_highlighted() {
    let d = dir("rec-search-page");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    for i in 0..60 {
        let id = doc
            .create_root(&format!("Item {i:02} \u{17d}lu\u{165}ou\u{10d}k\u{fd}"))
            .unwrap();
        doc.set_body(id, &format!("body number {i} about a Needle here"))
            .unwrap();
    }
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_tree(&p);
    h.search_for("item");
    assert_eq!(h.found().len(), 60);
    h.press(Key::PageDown);
    let after_page = h.host.search_current().unwrap();
    assert!(after_page >= 2, "a page is more than one row: {after_page}");
    h.press(Key::PageDown);
    assert!(h.host.search_current().unwrap() > after_page);
    h.press(Key::PageUp);
    assert_eq!(h.host.search_current(), Some(after_page));
    for _ in 0..30 {
        h.press(Key::PageDown);
    }
    assert_eq!(
        h.host.search_current(),
        Some(59),
        "stops at the last result"
    );
    // The query is marked in titles and snippets, case-insensitively and across Czech letters.
    h.search_for("ŽLUŤ");
    let marked: Vec<String> = h
        .painted()
        .texts
        .into_iter()
        .filter(|t| t.pos.x < h.tree_right())
        .flat_map(|t| t.marked)
        .collect();
    assert!(
        marked.iter().any(|m| m.to_lowercase() == "žluť"),
        "{marked:?}"
    );
    h.search_for("needle");
    let marked: Vec<String> = h
        .painted()
        .texts
        .into_iter()
        .flat_map(|t| t.marked)
        .collect();
    assert!(
        marked.iter().any(|m| m == "Needle"),
        "the snippet's own spelling is kept: {marked:?}"
    );
    // The number of results is shown, and named for assistive technology.
    h.text_in("60", true);
}

#[test]
fn match_ranges_follow_unicode_lowercasing() {
    use crate::search::match_ranges;
    assert_eq!(match_ranges("Příliš žluťoučký", "žluť"), vec![10..16]);
    assert_eq!(
        match_ranges("aAaA", "aa"),
        vec![0..4],
        "overlapping hits are merged"
    );
    assert_eq!(match_ranges("crab 🦀 and 🦀", "🦀"), vec![5..9, 14..18]);
    assert!(match_ranges("abc", "").is_empty());
    // A character whose lower case is longer (İ -> i̇) still maps to whole characters.
    let r = match_ranges("İstanbul", "i̇s");
    assert_eq!(r, vec![0..3]);
}

// Qt-compatible search fixture shared with the E7a tests.
#[test]
fn the_e7a_search_fixture_still_works_after_recovery_was_added() {
    let (_d, _p) = search_nb("rec-search-fixture");
}

/// Release-mode numbers for the migration plan (6,300 notes, 20 Trash entries, checkpoints):
/// `cargo test --release -p omatree-egui -- --ignored --nocapture recovery_timings`
#[test]
#[ignore]
fn recovery_timings() {
    let d = dir("rec-timings");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let body =
        "Příliš žluťoučký kůň úpěl ďábelské ódy. A line of ordinary text for the body.\n".repeat(4);
    for i in 0..300 {
        let r = doc.create_root(&format!("Root {i:03}")).unwrap();
        doc.set_body(r, &body).unwrap();
        for j in 0..20 {
            let c = doc
                .create_child(r, &format!("Child {i:03}-{j:02}"))
                .unwrap();
            doc.set_body(c, &body).unwrap();
        }
    }
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_tree(&p);
    let ms = |h: &mut H, f: &mut dyn FnMut(&mut H)| {
        let t = std::time::Instant::now();
        f(h);
        t.elapsed().as_secs_f64() * 1000.0
    };
    let ctx = h.ctx.clone();
    let mut del = vec![];
    for i in 0..20 {
        let id = h.id(&format!("Root {:03}", i * 3));
        h.host.select(&ctx, id);
        del.push(ms(&mut h, &mut |h| h.host.confirm_delete(&ctx, id)));
    }
    println!(
        "delete a 21-note subtree (incl. checkpoint of 6,300 notes): median {:.1} ms",
        {
            del.sort_by(|a, b| a.total_cmp(b));
            del[10]
        }
    );
    let open = ms(&mut h, &mut |h| {
        h.recovery();
    });
    println!(
        "open Recovery with 20 Trash entries and {} checkpoints (3 frames): {open:.1} ms",
        h.host.recovery_counts().1
    );
    let mut idle = vec![];
    for _ in 0..30 {
        idle.push(ms(&mut h, &mut |h| h.frame(vec![], Modifiers::NONE)));
    }
    idle.sort_by(|a, b| a.total_cmp(b));
    println!("idle frame with the dialog open: median {:.2} ms", idle[15]);
    let restore = ms(&mut h, &mut |h| {
        h.press(Key::Enter);
    });
    println!("restore a Trash entry (2 frames): {restore:.1} ms");
    h.recovery();
    h.go_to_checkpoints();
    h.press(Key::Enter);
    let confirm = ms(&mut h, &mut |h| {
        h.press(Key::Tab);
        h.press(Key::Enter);
    });
    println!("restore a checkpoint of 6,300 notes (hashes of every body, incl. 4 frames): {confirm:.1} ms; dialog {:?}", h.host.dialog());
}
