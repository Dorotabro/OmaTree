//! Starting without a file, New, Open and Save As through file dialogs (ticket E8a). Behaviour
//! follows the Qt app (`main.qml`; `tests/qml/{lifecycle,replacement,paths,autosave,conflict,
//! titleclose}.qml`) except where `docs/EGUI_MIGRATION_PLAN.md` records a difference. The file
//! dialogs are scripted: a picker that answers from a list and records what it was asked.

use super::tree_tests::shape;
use super::*;
use std::cell::RefCell;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;
use std::time::SystemTime;

pub(super) type Log = Rc<RefCell<Vec<PickRequest>>>;

/// A picker that records every request and answers from `answers`, then cancels.
pub(super) fn recording(answers: Vec<PickResult>) -> (Picker, Log) {
    let log: Log = Rc::default();
    let mut inner = scripted_picker(answers);
    let seen = Rc::clone(&log);
    let picker: Picker = Box::new(move |req, ctx| {
        seen.borrow_mut().push(req.clone());
        inner(req, ctx)
    });
    (picker, log)
}

/// A notebook file of top-level notes `(title, body)`; ids start from the beginning, so two of
/// them have overlapping `NodeId`s.
pub(super) fn notebook(path: &Path, notes: &[(&str, &str)]) {
    let mut doc = Document::open(path).unwrap();
    for (t, b) in notes {
        let id = doc.create_root(t).unwrap();
        doc.set_body(id, b).unwrap();
    }
    doc.save().unwrap();
}

pub(super) fn stamp(p: &Path) -> (Vec<u8>, SystemTime) {
    (
        fs::read(p).unwrap(),
        fs::metadata(p).unwrap().modified().unwrap(),
    )
}

impl H {
    /// The application as it starts with no path: an untitled notebook, tree focused.
    pub(super) fn blank() -> Self {
        let mut h = H::bare(Vec2::new(1000.0, 700.0));
        let ctx = h.ctx.clone();
        h.host.start_untitled(&ctx);
        h.host.focus(Focus::Tree);
        h.settle();
        h
    }

    pub(super) fn with_picker(&mut self, answers: Vec<PickResult>) -> Log {
        let (p, log) = recording(answers);
        self.host.set_picker(Some(p));
        log
    }

    pub(super) fn ctrl_o(&mut self) {
        self.key(Key::O, CTRL);
        self.settle();
    }

    pub(super) fn ctrl_alt_n(&mut self) {
        self.key(Key::N, CTRL | Modifiers::ALT);
        self.settle();
    }

    pub(super) fn named(&self) -> String {
        self.host.document().unwrap().display_name()
    }

    pub(super) fn has_path(&self) -> bool {
        self.host.document().unwrap().has_path()
    }

    pub(super) fn dialog_now(&self) -> Dialog {
        self.host.dialog().clone()
    }

    /// Make the notebook dirty by typing into the selected note.
    pub(super) fn make_dirty(&mut self) {
        self.host.focus(Focus::Editor);
        self.settle();
        let end = self.host.buffer().len();
        self.set_caret(end, end);
        self.type_text(" edited");
        assert!(self.host.is_dirty());
        self.host.focus(Focus::Tree);
        self.settle();
    }

    pub(super) fn trash_note(&mut self, title: &str) {
        let ctx = self.ctx.clone();
        let id = self.id(title);
        self.host.select(&ctx, id);
        self.host.confirm_delete(&ctx, id);
        self.settle();
    }

    pub(super) fn recovery_open_close(&mut self) {
        self.key(Key::R, CTRL | Modifiers::SHIFT);
        self.settle();
        assert_eq!(*self.host.dialog(), Dialog::Recovery);
        self.press(Key::Escape);
    }

    /// Open Recovery and leave it open.
    pub(super) fn recovery_open_close_keep(&mut self) {
        self.key(Key::R, CTRL | Modifiers::SHIFT);
        self.settle();
    }

    pub(super) fn answer_save(&mut self) {
        let ctx = self.ctx.clone();
        self.host.unsaved_save(&ctx);
        self.settle();
    }

    pub(super) fn answer_discard(&mut self) {
        let ctx = self.ctx.clone();
        self.host.unsaved_discard();
        self.host.confirm_discard(&ctx);
        self.settle();
    }
}

pub(super) fn two(name: &str) -> (crate::tests::TestDir, PathBuf, PathBuf) {
    let d = dir(name);
    let (a, b) = (d.join("first.omatree"), d.join("second.omatree"));
    notebook(&a, &[("Alpha", "alpha body\n"), ("Beta", "beta body\n")]);
    notebook(&b, &[("Gamma", "gamma body\n")]);
    (d, a, b)
}

// --- Starting -------------------------------------------------------------------------------

#[test]
fn starting_without_a_path_gives_an_untitled_notebook_and_creates_no_file() {
    let before: Vec<_> = fs::read_dir(".")
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    let mut h = H::blank();
    assert!(!h.has_path());
    assert_eq!(h.named(), "Untitled");
    assert!(h.host.rows().is_empty() && !h.host.is_dirty());
    assert_eq!(h.host.title, "Untitled - OmaTree", "the title bar says so");
    h.advance(3.0);
    let after: Vec<_> = fs::read_dir(".")
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(before, after, "nothing was created");
    assert!(h.painted().texts.iter().any(|t| t.text == "No notes yet"));
    // The editor pane says what to do, in words, instead of a bare line; the strip's menu is
    // called "menu" because it holds more than file operations (recovery, compact, export).
    let texts: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    assert!(
        texts.iter().any(|t| t == "This notebook is empty"),
        "{texts:?}"
    );
    // (The key is named the way the running platform names it: Ctrl+N, Cmd+N.)
    let new_note = h.ctx.format_shortcut(&keymap::NEW_NOTE);
    assert!(
        texts
            .iter()
            .any(|t| t.starts_with(&format!("Press {new_note}"))),
        "{texts:?}"
    );
    assert!(texts.iter().any(|t| t == "menu"), "{texts:?}");
    assert!(!texts.iter().any(|t| t == "file"), "{texts:?}");
}

#[test]
fn starting_with_a_valid_path_opens_it_and_with_a_bad_one_keeps_an_untitled_notebook() {
    let (d, a, _) = two("files-start");
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    let ctx = h.ctx.clone();
    h.host.start_with(&ctx, Some(&a));
    h.settle();
    assert_eq!(h.named(), "first.omatree");
    assert_eq!(h.sel(), "Alpha");
    assert!(!h.host.is_dirty());
    // Missing, and not a notebook: an untitled notebook and the reason; nothing is created or
    // modified.
    let missing = d.join("nope.omatree");
    let text = d.join("plain.omatree");
    fs::write(&text, "just text").unwrap();
    for bad in [&missing, &text] {
        let mut h = H::bare(Vec2::new(1000.0, 700.0));
        let ctx = h.ctx.clone();
        h.host.start_with(&ctx, Some(bad));
        h.settle();
        assert!(!h.has_path());
        assert!(
            matches!(h.dialog_now(), Dialog::Error(_)),
            "{:?}",
            h.dialog_now()
        );
    }
    assert!(!missing.exists());
    assert_eq!(fs::read_to_string(&text).unwrap(), "just text");
}

// --- New ------------------------------------------------------------------------------------

#[test]
fn new_notebook_replaces_a_clean_one_without_asking() {
    let (_d, a, _) = two("files-new-clean");
    let mut h = H::open_tree(&a);
    let id = h.host.notebook_id();
    let bytes = stamp(&a);
    h.ctrl_alt_n();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(!h.has_path() && h.host.rows().is_empty());
    assert!(h.host.notebook_id() > id, "a new notebook identity");
    assert_eq!(h.host.buffer(), "");
    assert_eq!(stamp(&a), bytes, "the file it replaced was not touched");
    // Ctrl+N is still a new note, not a new notebook.
    h.key(Key::N, CTRL);
    assert_eq!(h.host.rows().len(), 1);
    assert!(!h.has_path());
}

#[test]
fn new_with_unsaved_changes_asks_and_each_answer_does_what_it_says() {
    // Cancel.
    let (_d, a, _) = two("files-new-dirty");
    let mut h = H::open_tree(&a);
    h.make_dirty();
    let buffer = h.host.buffer().to_owned();
    h.ctrl_alt_n();
    assert!(matches!(h.dialog_now(), Dialog::Unsaved { .. }));
    assert_eq!(h.host.pending_leave(), Some(&Leave::New));
    h.host.cancel_dialog();
    h.settle();
    assert_eq!(h.named(), "first.omatree");
    assert!(h.host.is_dirty() && h.host.buffer() == buffer);
    assert!(h.host.pending_leave().is_none());
    // Discard needs its confirmation, and then the edit is not on disk.
    let before = stamp(&a);
    h.ctrl_alt_n();
    let ctx = h.ctx.clone();
    h.host.unsaved_discard();
    assert_eq!(h.dialog_now(), Dialog::ConfirmDiscard);
    assert_eq!(
        h.named(),
        "first.omatree",
        "nothing replaced before the confirmation"
    );
    h.host.confirm_discard(&ctx);
    h.settle();
    assert!(!h.has_path() && h.host.rows().is_empty());
    assert_eq!(stamp(&a), before);
    // Save: the file-backed notebook is written, then replaced.
    let (_d2, a2, _) = two("files-new-save");
    let mut h = H::open_tree(&a2);
    h.make_dirty();
    h.ctrl_alt_n();
    h.answer_save();
    assert!(!h.has_path() && h.host.rows().is_empty());
    assert!(
        snapshot(&a2).iter().any(|r| r.4.contains("edited")),
        "saved before leaving"
    );
}

#[test]
fn an_untitled_notebook_with_notes_saves_through_save_as_before_new_replaces_it() {
    let d = dir("files-untitled-new");
    let target = d.join("kept.omatree");
    let mut h = H::blank();
    let log = h.with_picker(vec![Some(target.clone())]);
    h.key(Key::N, CTRL); // a note
    h.type_text("Draft");
    h.press(Key::Enter);
    h.settle();
    assert!(h.host.is_dirty());
    h.ctrl_alt_n();
    assert!(matches!(h.dialog_now(), Dialog::Unsaved { .. }));
    h.answer_save(); // untitled: Save means choosing a file
    assert_eq!(log.borrow().len(), 1);
    assert!(matches!(
        log.borrow()[0].kind,
        PickKind::Save { then: true }
    ));
    assert_eq!(log.borrow()[0].name, "Untitled.omatree");
    assert!(target.exists());
    assert!(
        !h.has_path() && h.host.rows().is_empty(),
        "then the new notebook replaced it"
    );
    let kept = Document::open_existing(&target).unwrap();
    assert_eq!(kept.notebook().roots()[0].title(), "Draft");
}

// --- Saving an untitled notebook ------------------------------------------------------------------

#[test]
fn ctrl_s_on_an_untitled_notebook_asks_where_and_adds_the_extension() {
    let d = dir("files-save-untitled");
    let mut h = H::blank();
    let log = h.with_picker(vec![Some(d.join("plan"))]);
    h.key(Key::N, CTRL);
    h.type_text("First");
    h.press(Key::Enter);
    h.settle();
    h.key(Key::S, CTRL);
    h.settle();
    assert_eq!(log.borrow().len(), 1);
    assert!(h.has_path());
    assert_eq!(h.named(), "plan.omatree");
    assert!(d.join("plan.omatree").exists() && !d.join("plan").exists());
    assert!(!h.host.is_dirty());
    assert_eq!(h.host.title, "plan.omatree - OmaTree");
    // An explicit extension is left alone.
    let mut h = H::blank();
    h.with_picker(vec![Some(d.join("notes.txt"))]);
    h.key(Key::S, CTRL);
    h.settle();
    assert!(d.join("notes.txt").exists());
}

#[test]
fn an_untitled_notebook_is_never_autosaved_anywhere() {
    let d = dir("files-no-autosave");
    let mut h = H::blank();
    let log = h.with_picker(vec![]);
    h.key(Key::N, CTRL);
    h.type_text("Note");
    h.press(Key::Enter);
    h.advance(AUTOSAVE_DELAY * 4.0);
    assert!(h.host.is_dirty());
    assert!(log.borrow().is_empty(), "no dialog appeared by itself");
    assert!(
        h.host.autosave_problem().is_none(),
        "and no failure was reported"
    );
    assert!(!h.host.status().contains("Autosave"), "{}", h.host.status());
    assert_eq!(fs::read_dir(&d).unwrap().count(), 0);
}

// --- Open --------------------------------------------------------------------------------------

#[test]
fn open_with_a_clean_notebook_goes_straight_to_the_dialog() {
    let (_d, a, b) = two("files-open-clean");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b.clone())]);
    let before = (stamp(&a), stamp(&b));
    h.ctrl_o();
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(log.borrow()[0].kind, PickKind::Open);
    assert_eq!(
        log.borrow()[0].dir.as_deref(),
        a.parent(),
        "starts where this notebook is"
    );
    assert_eq!(h.named(), "second.omatree");
    assert_eq!(h.sel(), "Gamma");
    assert!(!h.host.is_dirty());
    assert!(h.has_focus(Focus::Tree));
    assert!(h.host.status().starts_with("Opened"));
    assert_eq!((stamp(&a), stamp(&b)), before, "opening wrote nothing");
}

#[test]
fn open_with_unsaved_changes_saves_discards_or_cancels_as_told() {
    // Save.
    let (_d, a, b) = two("files-open-save");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b.clone())]);
    h.make_dirty();
    h.ctrl_o();
    assert!(matches!(h.dialog_now(), Dialog::Unsaved { .. }));
    assert!(
        log.borrow().is_empty(),
        "no dialog before the question is answered"
    );
    h.answer_save();
    assert!(snapshot(&a).iter().any(|r| r.4.contains("edited")));
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(h.named(), "second.omatree");
    // Discard.
    let (_d, a, b) = two("files-open-discard");
    let mut h = H::open_tree(&a);
    h.with_picker(vec![Some(b.clone())]);
    h.make_dirty();
    let before = stamp(&a);
    h.ctrl_o();
    h.answer_discard();
    assert_eq!(h.named(), "second.omatree");
    assert_eq!(stamp(&a), before, "the discarded edit was not written");
    // Cancel.
    let (_d, a, b) = two("files-open-cancel");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b)]);
    h.make_dirty();
    let buffer = h.host.buffer().to_owned();
    h.ctrl_o();
    h.host.cancel_dialog();
    h.settle();
    assert!(log.borrow().is_empty());
    assert_eq!(h.named(), "first.omatree");
    assert!(h.host.is_dirty());
    assert_eq!(h.host.buffer(), buffer);
    assert!(h.has_focus(Focus::Tree), "back where it was");
}

#[test]
fn a_cancelled_file_dialog_changes_nothing_and_writes_nothing() {
    let (_d, a, _) = two("files-dialog-cancel");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![None]);
    let before = stamp(&a);
    h.go("Beta");
    h.ctrl_o();
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(h.named(), "first.omatree");
    assert_eq!(h.sel(), "Beta");
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.has_focus(Focus::Tree));
    assert_eq!(stamp(&a), before);
    // A cancelled Save As during a leave drops the leave: the notebook stays, dirty.
    let mut h = H::blank();
    h.with_picker(vec![None]);
    h.key(Key::N, CTRL);
    h.type_text("Kept");
    h.press(Key::Enter);
    h.settle();
    h.ctrl_alt_n();
    h.answer_save();
    assert!(h.host.pending_leave().is_none());
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(
        h.host.is_dirty() && !h.host.rows().is_empty(),
        "still the same notebook"
    );
}

#[test]
fn opening_an_invalid_file_leaves_the_current_notebook_exactly_as_it_was() {
    let (d, a, _) = two("files-open-invalid");
    let text = d.join("plain.omatree");
    fs::write(&text, "not a notebook").unwrap();
    let mut h = H::open_tree(&a);
    h.go("Beta");
    h.host.focus(Focus::Editor);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("!");
    let (caret, buffer, id) = (
        h.caret("Beta"),
        h.host.buffer().to_owned(),
        h.host.notebook_id(),
    );
    h.host.focus(Focus::Tree);
    h.settle();
    let log = h.with_picker(vec![Some(text.clone()), Some(d.join("missing.omatree"))]);
    // The dirty notebook is asked about first (Cancel is not offered a second time here).
    h.ctrl_o();
    h.answer_discard();
    assert!(
        matches!(h.dialog_now(), Dialog::Error(_)),
        "the reason is shown"
    );
    assert_eq!(h.named(), "first.omatree");
    assert_eq!(h.host.notebook_id(), id);
    assert_eq!(h.host.buffer(), buffer, "edits are still there");
    assert_eq!(h.caret("Beta"), caret);
    assert!(h.host.is_dirty());
    h.press(Key::Enter);
    assert_eq!(fs::read_to_string(&text).unwrap(), "not a notebook");
    h.ctrl_o();
    h.answer_discard();
    assert!(
        matches!(h.dialog_now(), Dialog::Error(_)),
        "missing file too"
    );
    assert!(
        !d.join("missing.omatree").exists(),
        "never created by opening"
    );
    assert_eq!(log.borrow().len(), 2);
}

// --- Failures during replacement -------------------------------------------------------------------

#[cfg(unix)]
#[test]
fn a_failing_save_keeps_the_notebook_and_the_question_until_it_works() {
    let (d, a, b) = two("files-save-fails");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b)]);
    h.make_dirty();
    let buffer = h.host.buffer().to_owned();
    h.ctrl_o();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o555)).unwrap();
    h.answer_save();
    fs::set_permissions(&d, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        matches!(h.dialog_now(), Dialog::Unsaved { error: Some(_) }),
        "{:?}",
        h.dialog_now()
    );
    assert_eq!(h.named(), "first.omatree");
    assert_eq!(h.host.buffer(), buffer);
    assert!(h.host.is_dirty());
    assert!(log.borrow().is_empty(), "no replacement started");
    assert_eq!(h.host.pending_leave(), Some(&Leave::Open));
    // Once it can write, Save finishes the job.
    h.answer_save();
    assert_eq!(log.borrow().len(), 1);
    assert_eq!(h.named(), "second.omatree");
}

#[test]
fn an_externally_modified_notebook_cannot_be_saved_over_by_a_replacement() {
    let (_d, a, b) = two("files-conflict");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b)]);
    h.make_dirty();
    {
        let mut other = Document::open_existing(&a).unwrap();
        other.create_root("Elsewhere").unwrap();
        other.save().unwrap();
    }
    let theirs = stamp(&a);
    h.ctrl_o();
    h.answer_save();
    assert_eq!(h.dialog_now(), Dialog::Conflict);
    assert_eq!(stamp(&a), theirs, "their version was not overwritten");
    assert_eq!(h.named(), "first.omatree");
    assert!(h.host.is_dirty());
    assert!(log.borrow().is_empty());
    // Cancel keeps everything; the user can still Save As elsewhere.
    h.host.cancel_dialog();
    h.settle();
    assert!(h.host.is_dirty() && h.named() == "first.omatree");
}

// --- Save As: overwrite, extensions, names ------------------------------------------------------------

#[test]
fn overwriting_an_existing_notebook_is_confirmed_and_a_non_notebook_is_refused() {
    let (d, a, b) = two("files-overwrite");
    let mut h = H::open_tree(&a);
    h.with_picker(vec![Some(b.clone()), Some(b.clone())]);
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    h.settle();
    assert!(
        matches!(h.dialog_now(), Dialog::Overwrite { .. }),
        "{:?}",
        h.dialog_now()
    );
    let before = stamp(&b);
    h.host.cancel_dialog();
    h.settle();
    assert_eq!(stamp(&b), before, "No: nothing replaced");
    assert_eq!(h.named(), "first.omatree");
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    h.settle();
    let ctx = h.ctx.clone();
    h.host.confirm_overwrite(&ctx);
    h.settle();
    assert_eq!(h.named(), "second.omatree");
    assert_eq!(
        Document::open_existing(&b)
            .unwrap()
            .notebook()
            .roots()
            .len(),
        2,
        "replaced by this notebook"
    );
    // A file that is not a notebook is never overwritten, even when confirmed.
    let plain = d.join("plain.omatree");
    fs::write(&plain, "precious").unwrap();
    h.with_picker(vec![Some(plain.clone())]);
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    h.settle();
    h.host.confirm_overwrite(&ctx);
    h.settle();
    assert_eq!(fs::read_to_string(&plain).unwrap(), "precious");
    assert!(matches!(
        h.dialog_now(),
        Dialog::SaveAs { error: Some(_), .. }
    ));
}

#[test]
fn czech_accents_spaces_and_unicode_survive_save_as_and_open() {
    let d = dir("files-unicode");
    let folder = d.join("Složka s mezerami");
    fs::create_dir_all(&folder).unwrap();
    let file = folder.join("Poznámky č. 1 — žluťoučký kůň 🦀.omatree");
    let mut h = H::blank();
    h.with_picker(vec![Some(file.clone()), Some(file.clone())]);
    h.key(Key::N, CTRL);
    h.type_text("Příliš žluťoučký");
    h.press(Key::Enter);
    h.settle();
    h.key(Key::S, CTRL);
    h.settle();
    assert!(file.exists());
    assert_eq!(h.named(), "Poznámky č. 1 — žluťoučký kůň 🦀.omatree");
    assert_eq!(
        h.host.title,
        "Poznámky č. 1 — žluťoučký kůň 🦀.omatree - OmaTree"
    );
    // Open it again from another notebook through the dialog.
    let mut h2 = H::blank();
    h2.with_picker(vec![Some(file.clone())]);
    h2.ctrl_o();
    assert_eq!(h2.named(), file.file_name().unwrap().to_string_lossy());
    assert_eq!(h2.sel(), "Příliš žluťoučký");
}

#[test]
fn a_notebook_saved_here_is_a_schema_v3_file_the_core_reopens_unchanged() {
    let d = dir("files-v3");
    let file = d.join("v3.omatree");
    let mut h = H::blank();
    h.with_picker(vec![Some(file.clone())]);
    h.key(Key::N, CTRL);
    h.type_text("Root");
    h.press(Key::Enter);
    h.key(Key::N, CTRL | Modifiers::SHIFT);
    h.type_text("Child");
    h.press(Key::Enter);
    h.key(Key::S, CTRL);
    h.settle();
    assert_eq!(&fs::read(&file).unwrap()[60..64], &[0, 0, 0, 3]);
    let doc = Document::open_existing(&file).unwrap();
    assert_eq!(shape(&doc), "Root(Child)");
    assert!(!doc.is_dirty());
}

// --- What replacement resets ----------------------------------------------------------------------------

#[test]
fn editor_state_does_not_cross_notebooks_even_when_their_node_ids_overlap() {
    let (_d, a, b) = two("files-undo");
    let mut h = H::open_tree(&a);
    let ctx = h.ctx.clone();
    let alpha_id = h.id("Alpha");
    h.select("Alpha");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("A1");
    assert_eq!(h.body("Alpha"), "alpha body\nA1");
    h.host.save(&ctx);
    // The other notebook has a note with the very same NodeId.
    h.with_picker(vec![Some(b.clone()), Some(a.clone())]);
    h.host.focus(Focus::Tree);
    h.settle();
    h.ctrl_o();
    assert_eq!(h.id("Gamma"), alpha_id, "ids overlap");
    h.select("Gamma");
    h.key(Key::Z, CTRL);
    assert_eq!(
        h.body("Gamma"),
        "gamma body\n",
        "nothing of Alpha's history reaches Gamma"
    );
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("G1");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Gamma"), "gamma body\n", "its own undo works");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Gamma"), "gamma body\n");
    h.host.focus(Focus::Tree);
    h.settle();
    // Back to the first notebook: Alpha starts with a fresh history and caret.
    h.host.save(&ctx);
    h.ctrl_o();
    assert_eq!(h.named(), "first.omatree");
    h.select("Alpha");
    h.key(Key::Z, CTRL);
    assert_eq!(
        h.body("Alpha"),
        "alpha body\nA1",
        "the old notebook's undo is gone"
    );
}

#[test]
fn replacement_resets_search_expansion_recovery_and_autosave_state() {
    let d = dir("files-reset");
    let (a, b) = (d.join("first.omatree"), d.join("second.omatree"));
    {
        let mut doc = Document::open(&a).unwrap();
        let p = doc.create_root("Parent").unwrap();
        doc.create_child(p, "Kid").unwrap();
        doc.create_root("Gone").unwrap();
        doc.set_expanded(p, true);
        doc.save().unwrap();
        let mut doc = Document::open(&b).unwrap();
        let p = doc.create_root("Other").unwrap();
        doc.create_child(p, "Inner").unwrap();
        doc.set_expanded(p, false);
        doc.save().unwrap();
    }
    let mut h = H::open_tree(&a);
    assert!(h.expanded("Parent"));
    h.trash_note("Gone");
    h.ctrl_f();
    h.type_text("kid");
    assert_eq!(h.found(), ["Kid"]);
    h.recovery_open_close();
    assert_eq!(h.host.recovery_counts().0, 1);
    h.host.focus(Focus::Tree);
    h.settle();
    h.with_picker(vec![Some(b.clone())]);
    // A pending autosave problem of the old document must not follow the user.
    h.host.autosave_blocked = true;
    h.host.autosave_error = Some("old".into());
    h.ctrl_o();
    h.answer_discard();
    assert_eq!(h.named(), "second.omatree");
    assert!(
        !h.host.search_open()
            && h.host.search_query().is_empty()
            && h.host.search_results().is_empty()
    );
    assert!(
        !h.expanded("Other"),
        "the stored expansion of the new notebook"
    );
    assert_eq!(h.vis(), ["Other"]);
    h.recovery_open_close();
    assert_eq!(
        h.host.recovery_counts().0,
        0,
        "no Trash of the old notebook"
    );
    assert!(h.host.autosave_problem().is_none() && !h.host.autosave_blocked());
    assert!(!h.host.is_dirty());
}

#[test]
fn repeated_commands_and_callbacks_run_the_pending_action_once() {
    let (_d, a, b) = two("files-repeat");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b.clone()), Some(a.clone())]);
    h.make_dirty();
    for _ in 0..4 {
        h.key(Key::O, CTRL); // repeated Ctrl+O behind the question is ignored
    }
    assert!(matches!(h.dialog_now(), Dialog::Unsaved { .. }));
    let ctx = h.ctx.clone();
    h.host.unsaved_save(&ctx);
    h.host.unsaved_save(&ctx); // a second callback for the same answer
    h.host.continue_pending(&ctx);
    h.settle();
    assert_eq!(log.borrow().len(), 1, "one dialog, not three");
    assert_eq!(h.named(), "second.omatree");
    // And Ctrl+Alt+N while a notebook is being replaced does not stack either.
    h.ctrl_alt_n();
    h.ctrl_alt_n();
    assert!(!h.has_path());
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn application_shortcuts_stand_down_behind_dialogs_and_in_text_fields() {
    let (_d, a, b) = two("files-shortcuts");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b)]);
    // In the title field, Ctrl+O and Ctrl+Alt+N are not typed and do not open the dialog
    // until the title is committed (they commit it, as F2-then-Ctrl+S does).
    h.go("Beta");
    h.press(Key::F2);
    h.type_text("Beta2");
    h.key(Key::O, CTRL);
    assert_eq!(h.host.title_text(), "Beta2");
    h.settle();
    // Behind the delete question nothing starts.
    let (_d, a, _) = two("files-shortcuts2");
    let mut h = H::open_tree(&a);
    let log2 = h.with_picker(vec![]);
    let ctx = h.ctx.clone();
    h.host.ask_delete(&ctx);
    h.ctrl_o();
    h.ctrl_alt_n();
    assert!(matches!(h.dialog_now(), Dialog::ConfirmDelete(_)));
    assert!(log2.borrow().is_empty());
    assert_eq!(h.named(), "first.omatree");
    let _ = log;
}

#[test]
fn the_file_menu_lists_the_notebook_commands_and_works_from_the_keyboard() {
    let (_d, a, b) = two("files-menu");
    let mut h = H::open_tree(&a);
    let log = h.with_picker(vec![Some(b)]);
    h.key(Key::Tab, Modifiers::SHIFT); // "menu" is the last stop of the strip
    h.press(Key::Enter);
    h.advance(0.3);
    let texts: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    for want in [
        "New notebook",
        "Open\u{2026}",
        "Save",
        "Save As\u{2026}",
        "Recovery\u{2026}",
        "Keyboard shortcuts",
    ] {
        assert!(texts.iter().any(|t| t == want), "{want}: {texts:?}");
    }
    let (open, new_notebook) = (
        h.ctx.format_shortcut(&keymap::OPEN),
        h.ctx.format_shortcut(&keymap::NEW_NOTEBOOK),
    );
    assert!(
        texts.iter().any(|t| *t == open) && texts.iter().any(|t| *t == new_notebook),
        "{open} / {new_notebook}: {texts:?}"
    );
    h.press(Key::Escape);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert!(log.borrow().is_empty());
}

#[test]
fn without_a_native_dialog_open_and_save_as_ask_for_a_typed_path() {
    let (d, a, b) = two("files-typed");
    let mut h = H::open_tree(&a);
    h.host.set_picker(None);
    h.ctrl_o();
    assert!(matches!(h.dialog_now(), Dialog::Open { .. }));
    h.type_text("missing.omatree");
    h.press(Key::Enter);
    assert!(
        matches!(h.dialog_now(), Dialog::Open { error: Some(_), .. }),
        "still asking"
    );
    assert!(!d.join("missing.omatree").exists());
    for _ in 0..15 {
        h.key(Key::Backspace, Modifiers::NONE);
    }
    h.type_text(&b.display().to_string());
    h.press(Key::Enter);
    assert_eq!(h.named(), "second.omatree");
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn f1_names_open_and_new_notebook() {
    let (_d, a, _) = two("files-f1");
    let h = H::open_tree(&a);
    let ctx = h.ctx.clone();
    let sections = h.host.shortcut_sections(&ctx);
    let files = sections.iter().find(|s| s.0 == "Files").unwrap();
    let has = |k: &str, w: &str| files.2.iter().any(|(a, b)| a == k && b.starts_with(w));
    assert!(has(&ctx.format_shortcut(&keymap::OPEN), "Open"));
    assert!(has(
        &ctx.format_shortcut(&keymap::NEW_NOTEBOOK),
        "New notebook"
    ));
    assert!(has(&ctx.format_shortcut(&keymap::SAVE), "Save"));
}
