//! File lifecycle hardening (ticket E8b): external-modification conflicts and Reload from Disk,
//! file-dialog reliability, command-line and outside open requests, quit paths, recent notebooks.
//! Dialog answers are scripted or held back by the test; notebooks are disposable copies.

use super::files_tests::{notebook, recording, stamp, two};
use super::*;
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;
use std::sync::mpsc;

type Held = Rc<RefCell<Vec<mpsc::Sender<PickResult>>>>;

/// A picker whose answers the test sends itself, later (or never).
fn held() -> (Picker, Held, Rc<RefCell<Vec<PickRequest>>>) {
    let senders: Held = Rc::default();
    let log: Rc<RefCell<Vec<PickRequest>>> = Rc::default();
    let (s, l) = (Rc::clone(&senders), Rc::clone(&log));
    let picker: Picker = Box::new(move |req, _| {
        l.borrow_mut().push(req.clone());
        let (tx, rx) = mpsc::channel();
        s.borrow_mut().push(tx);
        rx
    });
    (picker, senders, log)
}

/// "first.omatree" opened and edited, then changed by someone else, then Ctrl+S: the conflict
/// question is open.
fn conflicted(name: &str) -> (PathBuf, PathBuf, H) {
    let (d, a, _) = two(name);
    let mut h = H::open_tree(&a);
    h.make_dirty();
    {
        let mut other = Document::open_existing(&a).unwrap();
        other.create_root("Elsewhere").unwrap();
        other.save().unwrap();
    }
    h.key(Key::S, CTRL);
    h.settle();
    assert_eq!(h.dialog_now(), Dialog::Conflict);
    (d, a, h)
}

fn titles(h: &H) -> Vec<String> {
    h.host.rows().iter().map(|r| r.title.clone()).collect()
}

impl H {
    /// Type `add` at the end of the selected note (leaving the tree focused afterwards).
    fn make_dirty_note(&mut self, add: &str) {
        self.host.focus(Focus::Editor);
        self.settle();
        let end = self.host.buffer().len();
        self.set_caret(end, end);
        self.type_text(add);
        self.host.focus(Focus::Tree);
        self.settle();
    }
}

// --- Conflict: Keep editing, Save As, Reload from Disk -------------------------------------------

#[test]
fn keep_editing_after_a_conflict_changes_nothing_and_the_conflict_stays() {
    let (_d, a, mut h) = conflicted("hard-keep");
    let theirs = stamp(&a);
    let (buffer, id) = (h.host.buffer().to_owned(), h.host.notebook_id());
    h.press(Key::Enter); // "Keep editing" has the keyboard
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.host.is_dirty());
    assert_eq!(h.host.buffer(), buffer);
    assert_eq!(h.host.notebook_id(), id);
    assert!(
        h.host.autosave_blocked(),
        "still in conflict: nothing was resolved"
    );
    assert_eq!(stamp(&a), theirs, "nothing written");
    h.advance(AUTOSAVE_DELAY * 3.0);
    assert_eq!(stamp(&a), theirs, "and autosave does not try again");
}

#[test]
fn save_as_after_a_conflict_keeps_my_version_elsewhere_and_clears_the_conflict() {
    let (d, a, mut h) = conflicted("hard-saveas");
    let theirs = stamp(&a);
    let mine = d.join("mine.omatree");
    h.with_picker(vec![Some(mine.clone())]);
    h.press(Key::Tab); // Save As…
    h.press(Key::Enter);
    h.settle();
    assert_eq!(h.named(), "mine.omatree");
    assert!(!h.host.autosave_blocked() && h.host.autosave_problem().is_none());
    assert!(!h.host.is_dirty());
    assert!(
        snapshot(&mine).iter().any(|r| r.4.contains("edited")),
        "my edit is in my file"
    );
    assert_eq!(
        stamp(&a),
        theirs,
        "their file was left exactly as they wrote it"
    );
    // And a cancelled Save As leaves the conflict in place.
    let (_d, a2, mut h2) = conflicted("hard-saveas-cancel");
    let t2 = stamp(&a2);
    h2.with_picker(vec![None]);
    h2.press(Key::Tab);
    h2.press(Key::Enter);
    h2.settle();
    assert!(h2.host.autosave_blocked() && h2.host.is_dirty());
    assert_eq!(stamp(&a2), t2);
}

#[test]
fn reload_from_disk_replaces_the_notebook_only_after_a_confirmation() {
    let (_d, a, mut h) = conflicted("hard-reload");
    let theirs = stamp(&a);
    let id = h.host.notebook_id();
    h.press(Key::Tab);
    h.press(Key::Tab); // Reload from Disk…
    h.press(Key::Enter);
    assert_eq!(
        h.dialog_now(),
        Dialog::ConfirmReload,
        "edits exist: asks first"
    );
    assert_eq!(
        h.host.notebook_id(),
        id,
        "nothing replaced before the answer"
    );
    h.press(Key::Tab); // from "Keep editing" to the destructive answer
    h.press(Key::Enter);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(
        titles(&h).contains(&"Elsewhere".to_owned()),
        "the disk version"
    );
    assert!(
        !h.host.buffer().contains("edited"),
        "my unsaved edit is gone, as announced"
    );
    assert!(!h.host.is_dirty());
    assert!(!h.host.autosave_blocked() && h.host.autosave_problem().is_none());
    assert!(h.host.notebook_id() > id);
    assert_eq!(stamp(&a), theirs, "reload never writes");
    assert!(h.host.status().starts_with("Reloaded"));
    // Editing and saving work again, with no new conflict.
    h.make_dirty();
    h.advance(AUTOSAVE_DELAY + 0.5);
    assert!(!h.host.is_dirty() && !h.host.autosave_blocked());
    assert!(snapshot(&a).iter().any(|r| r.4.contains("edited")));
    h.make_dirty();
    h.key(Key::S, CTRL);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(&fs::read(&a).unwrap()[60..64], &[0, 0, 0, 3]);
}

#[test]
fn cancelling_the_reload_confirmation_keeps_every_edit_and_all_state() {
    let (_d, a, mut h) = conflicted("hard-reload-cancel");
    let theirs = stamp(&a);
    let (buffer, id, caret) = (
        h.host.buffer().to_owned(),
        h.host.notebook_id(),
        h.caret("Alpha"),
    );
    h.press(Key::Tab);
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.press(Key::Enter); // Keep editing is the default
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(
        (h.host.buffer().to_owned(), h.host.notebook_id()),
        (buffer, id)
    );
    assert_eq!(h.caret("Alpha"), caret);
    assert!(h.host.is_dirty() && h.host.autosave_blocked());
    assert_eq!(stamp(&a), theirs);
}

#[test]
fn a_reload_that_fails_leaves_the_notebook_its_edits_and_the_conflict_alone() {
    let (_d, a, mut h) = conflicted("hard-reload-fails");
    let (buffer, id) = (h.host.buffer().to_owned(), h.host.notebook_id());
    fs::write(&a, "this is no longer a notebook").unwrap();
    h.press(Key::Tab);
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.settle();
    assert!(
        matches!(h.dialog_now(), Dialog::Error(m) if m.contains("could not be reloaded") && m.contains("unchanged"))
    );
    assert_eq!(h.host.notebook_id(), id);
    assert_eq!(h.host.buffer(), buffer);
    assert!(
        h.host.is_dirty() && h.host.autosave_blocked(),
        "the conflict is not resolved"
    );
    assert_eq!(
        fs::read_to_string(&a).unwrap(),
        "this is no longer a notebook",
        "file untouched"
    );
    h.press(Key::Enter);
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn reload_resets_undo_search_recovery_and_tree_state_but_keeps_the_selected_note() {
    let d = dir("hard-reload-state");
    let a = d.join("n.omatree");
    {
        let mut doc = Document::open(&a).unwrap();
        let p = doc.create_root("Parent").unwrap();
        doc.create_child(p, "Kid").unwrap();
        let j = doc.create_root("Journal").unwrap();
        doc.set_body(j, "disk text\n").unwrap();
        doc.create_root("Doomed").unwrap();
        doc.set_expanded(p, true);
        doc.save().unwrap();
    }
    let mut h = H::open_tree(&a);
    h.trash_note("Doomed");
    h.select("Journal");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text(" mine");
    h.ctrl_f();
    h.type_text("kid");
    assert_eq!(h.found(), ["Kid"]);
    h.recovery_open_close();
    // Someone else rewrites the file; the user reloads from the menu.
    {
        let mut other = Document::open_existing(&a).unwrap();
        other.create_root("Fresh").unwrap();
        other.save().unwrap();
    }
    h.host.focus(Focus::Tree);
    h.settle();
    // (Autosave may already have written " mine"; what counts is what is on disk now.)
    let on_disk = snapshot(&a)
        .into_iter()
        .find(|r| r.3 == "Journal")
        .unwrap()
        .4;
    h.make_dirty_note(" unsaved");
    let ctx = h.ctx.clone();
    h.host.request_reload(&ctx);
    h.settle();
    assert_eq!(h.dialog_now(), Dialog::ConfirmReload);
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.settle();
    assert_eq!(h.sel(), "Journal", "the same note stays selected");
    assert_eq!(h.host.buffer(), on_disk);
    assert!(!h.host.search_open() && h.host.search_results().is_empty());
    assert!(h.expanded("Parent"), "expansion comes from the file");
    h.recovery_open_close();
    assert!(titles(&h).contains(&"Fresh".to_owned()));
    // Ctrl+Z in the reloaded note cannot bring back " mine".
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::Z, CTRL);
    assert_eq!(
        h.host.buffer(),
        on_disk,
        "no undo step of the old notebook survives"
    );
}

#[test]
fn reload_of_a_clean_notebook_needs_no_confirmation_and_an_untitled_one_has_nothing_to_reload() {
    let (_d, a, _) = two("hard-reload-clean");
    let mut h = H::open_tree(&a);
    {
        let mut other = Document::open_existing(&a).unwrap();
        other.create_root("Later").unwrap();
        other.save().unwrap();
    }
    let ctx = h.ctx.clone();
    h.host.request_reload(&ctx);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(titles(&h).contains(&"Later".to_owned()));
    let mut u = H::blank();
    let ctx = u.ctx.clone();
    u.host.request_reload(&ctx);
    assert!(u.host.status().contains("no file"));
    assert_eq!(*u.host.dialog(), Dialog::None);
}

#[test]
fn a_conflict_during_a_replacement_offers_the_same_choices_and_reload_drops_the_pending_action() {
    let (_d, a, b) = two("hard-leave-conflict");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b)]);
    h.make_dirty();
    {
        let mut other = Document::open_existing(&a).unwrap();
        other.create_root("Elsewhere").unwrap();
        other.save().unwrap();
    }
    h.ctrl_o();
    h.answer_save();
    assert_eq!(h.dialog_now(), Dialog::Conflict);
    assert_eq!(h.host.pending_leave(), Some(&Leave::Open), "still waiting");
    // Discard is offered only while something is waiting; Reload drops the waiting action.
    h.press(Key::Tab);
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.settle();
    assert!(h.host.pending_leave().is_none());
    assert!(
        log.borrow().is_empty(),
        "no file dialog was started by the reload"
    );
    assert_eq!(h.named(), "first.omatree");
    assert!(titles(&h).contains(&"Elsewhere".to_owned()));
}

// --- File dialogs -------------------------------------------------------------------------------------

#[test]
fn a_second_request_while_a_file_dialog_is_open_is_ignored() {
    let (_d, a, b) = two("hard-dup-picker");
    let mut h = H::open_tree(&a);
    let (picker, senders, log) = held();
    h.host.set_picker(Some(picker));
    h.ctrl_o();
    assert_eq!(h.dialog_now(), Dialog::Picking);
    h.ctrl_o();
    h.ctrl_alt_n();
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    assert_eq!(log.borrow().len(), 1, "one dialog");
    assert_eq!(h.named(), "first.omatree");
    senders.borrow()[0].send(Some(b)).unwrap();
    h.settle();
    assert_eq!(h.named(), "second.omatree");
}

#[test]
fn a_late_answer_from_an_abandoned_dialog_goes_nowhere() {
    let (d, a, b) = two("hard-late");
    let mut h = H::open_tree(&a);
    let (picker, senders, _) = held();
    h.host.set_picker(Some(picker));
    h.ctrl_o();
    assert_eq!(h.dialog_now(), Dialog::Picking);
    // "Type a path instead…": the second button.
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert!(
        matches!(h.dialog_now(), Dialog::Open { .. }),
        "{:?}",
        h.dialog_now()
    );
    // The abandoned dialog answers anyway.
    let _ = senders.borrow()[0].send(Some(b.clone()));
    h.settle();
    assert_eq!(h.named(), "first.omatree", "its answer was not used");
    assert!(matches!(h.dialog_now(), Dialog::Open { .. }));
    // The typed path still works.
    h.type_text(&b.display().to_string());
    h.press(Key::Enter);
    assert_eq!(h.named(), "second.omatree");
    let _ = d;
}

#[test]
fn cancel_in_the_waiting_dialog_drops_the_pending_action_and_ignores_the_late_answer() {
    let (_d, a, b) = two("hard-picker-cancel");
    let mut h = H::open_tree(&a);
    let (picker, senders, _) = held();
    h.host.set_picker(Some(picker));
    h.make_dirty();
    h.ctrl_o();
    h.answer_discard(); // the question is answered; the file dialog opens
    assert_eq!(h.dialog_now(), Dialog::Picking);
    let before = stamp(&a);
    h.press(Key::Enter); // Cancel has the keyboard
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.host.pending_leave().is_none());
    let _ = senders.borrow()[0].send(Some(b));
    h.settle();
    assert_eq!(h.named(), "first.omatree");
    assert!(
        h.host.is_dirty(),
        "the edits the user chose to keep are still here"
    );
    assert_eq!(stamp(&a), before);
}

#[test]
fn a_dialog_that_dies_is_a_cancel_and_changes_nothing() {
    let (_d, a, _) = two("hard-picker-dead");
    let mut h = H::open_tree(&a);
    h.host.set_picker(Some(Box::new(|_, _| mpsc::channel().1))); // the sender is already gone
    let before = stamp(&a);
    h.ctrl_o();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(h.named(), "first.omatree");
    assert_eq!(stamp(&a), before);
}

#[test]
fn typed_paths_are_always_available_from_the_file_menu_even_with_a_native_dialog() {
    let (_d, a, b) = two("hard-typed-menu");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![]);
    let ctx = h.ctx.clone();
    h.host.request_leave(&ctx, Leave::OpenTyped);
    h.settle();
    assert!(matches!(h.dialog_now(), Dialog::Open { .. }));
    h.type_text(&b.display().to_string());
    h.press(Key::Enter);
    assert_eq!(h.named(), "second.omatree");
    h.host.start_save_as_typed(&ctx);
    h.settle();
    assert!(matches!(h.dialog_now(), Dialog::SaveAs { .. }));
    assert!(log.borrow().is_empty(), "no native dialog was involved");
    h.press(Key::Escape);
    // The menu lists them.
    h.host.focus(Focus::Tree);
    h.settle();
    h.key(Key::Tab, Modifiers::SHIFT);
    h.press(Key::Enter);
    h.advance(0.3);
    let texts: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    for want in [
        "Open by path\u{2026}",
        "Save As by path\u{2026}",
        "Reload from disk\u{2026}",
        "Quit",
    ] {
        assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
    }
    h.press(Key::Escape);
}

#[test]
fn repeated_overwrite_answers_and_stale_paths_never_write_twice() {
    let (_d, a, b) = two("hard-overwrite");
    let mut h = H::open_tree(&a);
    h.with_picker(vec![Some(b.clone())]);
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    h.settle();
    assert!(matches!(h.dialog_now(), Dialog::Overwrite { .. }));
    let ctx = h.ctx.clone();
    h.host.confirm_overwrite(&ctx);
    h.host.confirm_overwrite(&ctx);
    h.host.confirm_overwrite(&ctx);
    h.settle();
    assert_eq!(h.named(), "second.omatree");
    let after = stamp(&b);
    h.host.confirm_overwrite(&ctx); // nothing is being asked any more
    assert_eq!(stamp(&b), after);
    // The application's question is its own: a refusal on cancel leaves the target alone.
    let (_d2, a2, b2) = two("hard-overwrite2");
    let mut h = H::open_tree(&a2);
    h.with_picker(vec![Some(b2.clone())]);
    let before = stamp(&b2);
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    h.settle();
    h.press(Key::Enter); // "Cancel" has the keyboard
    assert_eq!(stamp(&b2), before);
    assert_eq!(h.named(), "first.omatree");
}

// --- Command line and outside requests --------------------------------------------------------------------

#[test]
fn the_command_line_opens_exactly_one_notebook_and_refuses_anything_else() {
    let (d, a, _) = two("hard-args");
    let ctx_of = || H::bare(Vec2::new(1000.0, 700.0));
    // One valid path, with spaces and Czech letters.
    let odd = d.join("Složka s mezerami");
    fs::create_dir_all(&odd).unwrap();
    let file = odd.join("Poznámky č. 2.omatree");
    notebook(&file, &[("Příliš", "x")]);
    for path in [&a, &file] {
        let mut h = ctx_of();
        let ctx = h.ctx.clone();
        h.host.start_with_args(&ctx, std::slice::from_ref(path));
        h.settle();
        assert!(h.has_path());
        assert_eq!(h.named(), path.file_name().unwrap().to_string_lossy());
        assert_eq!(*h.host.dialog(), Dialog::None);
    }
    // No arguments: untitled. An invalid file: untitled and the reason. Several, or an
    // option: nothing opened, and the reason.
    let text = d.join("plain.omatree");
    fs::write(&text, "no").unwrap();
    for (args, expect_error) in [
        (vec![], false),
        (vec![text.clone()], true),
        (vec![d.join("missing.omatree")], true),
        (vec![a.clone(), file.clone()], true),
        (vec![PathBuf::from("--frobnicate")], true),
    ] {
        let mut h = ctx_of();
        let ctx = h.ctx.clone();
        h.host.start_with_args(&ctx, &args);
        h.settle();
        assert!(!h.has_path(), "{args:?}");
        assert_eq!(
            matches!(h.dialog_now(), Dialog::Error(_)),
            expect_error,
            "{args:?}"
        );
    }
    assert_eq!(fs::read_to_string(&text).unwrap(), "no");
    assert!(!d.join("missing.omatree").exists());
    assert!(check_args(&[a.clone(), a.clone()]).is_err());
    assert_eq!(check_args(&[]).unwrap(), None);
}

#[test]
fn an_outside_open_request_meets_the_same_guard_as_file_open() {
    let (d, a, b) = two("hard-outside");
    let mut h = H::open_tree(&a);
    h.make_dirty();
    let ctx = h.ctx.clone();
    h.host.request_open_path(&ctx, b.clone());
    assert!(
        matches!(h.dialog_now(), Dialog::Unsaved { .. }),
        "unsaved edits are asked about"
    );
    h.host.request_open_path(&ctx, b.clone()); // a second request does not stack
    assert_eq!(h.host.pending_leave(), Some(&Leave::OpenPath(b.clone())));
    h.host.cancel_dialog();
    h.settle();
    assert_eq!(h.named(), "first.omatree");
    assert!(h.host.is_dirty());
    // Discarded, it opens; a malformed request changes nothing.
    h.host.request_open_path(&ctx, b.clone());
    h.answer_discard();
    assert_eq!(h.named(), "second.omatree");
    let text = d.join("plain.omatree");
    fs::write(&text, "no").unwrap();
    h.host.request_open_path(&ctx, text.clone());
    h.settle();
    assert!(matches!(h.dialog_now(), Dialog::Error(_)));
    assert_eq!(h.named(), "second.omatree");
    assert_eq!(fs::read_to_string(&text).unwrap(), "no");
}

// --- Quit paths -----------------------------------------------------------------------------------------------

#[test]
fn repeated_close_requests_keep_one_question_and_never_close_a_dirty_notebook() {
    let (_d, a, _) = two("hard-close-repeat");
    let mut h = H::open_tree(&a);
    h.make_dirty();
    for _ in 0..4 {
        assert!(!h.request_close());
    }
    assert!(!h.closed);
    assert!(matches!(h.dialog_now(), Dialog::Unsaved { .. }));
    assert_eq!(h.host.pending_leave(), Some(&Leave::Close));
    h.host.cancel_dialog();
    h.settle();
    assert!(h.host.pending_leave().is_none());
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(!h.closed);
}

#[test]
fn closing_while_another_question_is_open_is_handled_by_what_that_question_is() {
    // A view or an unrelated question gives way to the unsaved-changes question.
    let (_d, a, _) = two("hard-close-modal");
    let mut h = H::open_tree(&a);
    h.make_dirty();
    h.recovery_open_close_keep();
    assert_eq!(h.dialog_now(), Dialog::Recovery);
    assert!(!h.request_close());
    assert!(matches!(h.dialog_now(), Dialog::Unsaved { .. }));
    h.host.cancel_dialog();
    h.settle();
    // A Save As in progress keeps the keyboard; the window stays, with a word of explanation.
    h.with_picker(vec![]);
    h.host.set_picker(None);
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    assert!(matches!(h.dialog_now(), Dialog::SaveAs { .. }));
    assert!(!h.request_close());
    assert!(matches!(h.dialog_now(), Dialog::SaveAs { .. }));
    assert!(h.host.status().contains("open question"));
    h.press(Key::Escape);
    // A file dialog in flight: dirty keeps the window, clean lets it go.
    let (picker, _senders, _) = held();
    h.host.set_picker(Some(picker));
    h.make_dirty_note(" more");
    h.ctrl_o();
    h.answer_discard(); // the notebook is still dirty; the file dialog opens
    assert_eq!(h.dialog_now(), Dialog::Picking);
    assert!(!h.request_close());
    assert_eq!(h.dialog_now(), Dialog::Picking);
    let (_d2, a2, _) = two("hard-close-picking");
    let mut c = H::open_tree(&a2);
    let (picker, _s, _) = held();
    c.host.set_picker(Some(picker));
    c.ctrl_o();
    assert!(c.request_close(), "nothing to lose");
}

#[test]
fn a_failing_save_keeps_the_window_for_close_new_and_open_alike() {
    for what in ["close", "new", "open"] {
        let (d, a, b) = two(&format!("hard-save-fails-{what}"));
        let mut h = H::open_tree(&a);
        let log = h.with_picker(vec![Some(b)]);
        h.make_dirty();
        match what {
            "close" => assert!(!h.request_close()),
            "new" => h.ctrl_alt_n(),
            _ => h.ctrl_o(),
        }
        fs::set_permissions(&d, fs::Permissions::from_mode(0o555)).unwrap();
        h.answer_save();
        fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(
            matches!(h.dialog_now(), Dialog::Unsaved { error: Some(_) }),
            "{what}"
        );
        assert!(
            !h.closed && h.named() == "first.omatree" && h.host.is_dirty(),
            "{what}"
        );
        assert!(log.borrow().is_empty(), "{what}");
    }
}

// --- Recent notebooks -----------------------------------------------------------------------------------------------

#[test]
fn recent_notebooks_keep_paths_only_and_follow_successful_opens() {
    let (d, a, b) = two("hard-recent");
    let store = d.join("config").join("omatree").join("recent.txt");
    let mut h = H::blank();
    h.host.set_recent_file(Some(store.clone()));
    let log = h.with_picker(vec![
        Some(a.clone()),
        None,
        Some(d.join("missing.omatree")),
        Some(b.clone()),
    ]);
    h.ctrl_o(); // opens a
    h.ctrl_o(); // cancelled
    assert_eq!(h.host.recent_notebooks(), [fs::canonicalize(&a).unwrap()]);
    h.ctrl_o(); // a failing open: not recorded
    h.press(Key::Enter);
    assert_eq!(h.host.recent_notebooks().len(), 1);
    h.ctrl_o(); // opens b
    assert_eq!(
        h.host.recent_notebooks(),
        [fs::canonicalize(&b).unwrap(), fs::canonicalize(&a).unwrap()]
    );
    assert_eq!(log.borrow().len(), 4);
    let text = fs::read_to_string(&store).unwrap();
    assert!(
        text.contains("second.omatree") && !text.contains("body"),
        "paths only: {text}"
    );
    // Persisted for the next start; opening an entry goes through the same guard.
    let mut h2 = H::blank();
    h2.host.set_recent_file(Some(store.clone()));
    assert_eq!(h2.host.recent_notebooks().len(), 2);
    let first = h2.host.recent_notebooks()[1].clone();
    let ctx = h2.ctx.clone();
    h2.host.request_open_path(&ctx, first);
    assert_eq!(h2.named(), "first.omatree");
    // A recent file that has gone is refused, forgotten, and the notebook stays.
    fs::remove_file(&b).unwrap();
    let gone = fs::canonicalize(&a)
        .unwrap()
        .with_file_name("second.omatree");
    h2.host.request_open_path(&ctx, gone);
    assert!(matches!(h2.dialog_now(), Dialog::Error(_)));
    assert_eq!(h2.named(), "first.omatree");
    assert_eq!(
        h2.host.recent_notebooks().len(),
        1,
        "the dead entry is dropped"
    );
    // Clearing.
    h2.host.clear_recent();
    assert!(h2.host.recent_notebooks().is_empty());
    assert_eq!(fs::read_to_string(&store).unwrap(), "");
}

#[test]
fn a_missing_malformed_or_unwritable_recent_file_is_harmless() {
    let d = dir("hard-recent-bad");
    let mut h = H::blank();
    h.host
        .set_recent_file(Some(d.join("nope").join("recent.txt")));
    assert!(h.host.recent_notebooks().is_empty());
    let junk = d.join("junk.txt");
    fs::write(&junk, "relative/path\n\n\u{0}\nnot absolute\n").unwrap();
    h.host.set_recent_file(Some(junk));
    assert!(
        h.host.recent_notebooks().is_empty(),
        "only absolute paths are believed"
    );
    // A read-only configuration folder: opening still works, nothing is kept, nothing breaks.
    let cfg = d.join("ro");
    fs::create_dir_all(&cfg).unwrap();
    fs::set_permissions(&cfg, fs::Permissions::from_mode(0o555)).unwrap();
    let (_d, a, _) = two("hard-recent-ro");
    h.host.set_recent_file(Some(cfg.join("recent.txt")));
    let ctx = h.ctx.clone();
    h.host.request_open_path(&ctx, a);
    fs::set_permissions(&cfg, fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(h.named(), "first.omatree");
    assert!(!cfg.join("recent.txt").exists());
}

#[test]
fn the_file_menu_lists_recent_notebooks_and_clearing_them() {
    let (d, a, _) = two("hard-recent-menu");
    let store = d.join("recent.txt");
    let mut h = H::open_tree(&a);
    h.host.set_recent_file(Some(store));
    let ctx = h.ctx.clone();
    h.host.request_open_path(&ctx, a.clone());
    h.host.focus(Focus::Tree);
    h.settle();
    h.key(Key::Tab, Modifiers::SHIFT);
    h.press(Key::Enter);
    h.advance(0.3);
    let texts: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    assert!(texts.iter().any(|t| t == "Open recent"), "{texts:?}");
}

#[test]
fn nothing_is_written_by_cancelling_anything() {
    let (_d, a, b) = two("hard-no-writes");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![None, None]);
    let before = (stamp(&a), stamp(&b));
    h.ctrl_o();
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    h.settle();
    let ctx = h.ctx.clone();
    h.host.request_reload(&ctx); // clean: reloads, which reads only
    h.recovery_open_close();
    assert_eq!(log.borrow().len(), 2);
    assert_eq!((stamp(&a), stamp(&b)), before);
    let _ = recording(vec![]);
}
