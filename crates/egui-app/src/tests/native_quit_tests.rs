//! M1: operating-system quit requests (macOS Command+Q, the Dock's Quit, log-out) meet the same
//! unsaved-changes guard as a click on the window's close button. The platform hook is not under
//! test here (see `native_quit/macos.rs` for that); these drive the host through its inbox and read
//! back the answer it would give AppKit, so the whole decision is tested on every platform.

use super::*;
use std::cell::RefCell;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;
use std::sync::atomic::Ordering;

type Answers = Rc<RefCell<Vec<bool>>>;

fn fixture(name: &str) -> (TestDir, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    make_fixture(&p);
    (d, p)
}

fn journal(p: &Path) -> String {
    bodies(p).last().unwrap().1.clone()
}

impl H {
    /// Record what the host tells the operating system.
    fn watch_quit(&mut self) -> Answers {
        let answers: Answers = Rc::default();
        let sink = answers.clone();
        self.host
            .set_native_quit_answer(Box::new(move |allow| sink.borrow_mut().push(allow)));
        answers
    }

    /// What the platform hook does when the quit command arrives.
    fn raise_quit(&mut self) {
        self.host.native_quit_inbox().store(true, Ordering::Release);
    }

    /// A quit request, and the frames the host needs to answer it.
    fn native_quit(&mut self) {
        self.raise_quit();
        for _ in 0..4 {
            self.frame(vec![], Modifiers::NONE);
        }
    }

    pub(super) fn frames(&mut self, n: usize) {
        for _ in 0..n {
            self.frame(vec![], Modifiers::NONE);
        }
    }
}

#[test]
fn a_clean_notebook_quits_at_once_and_is_told_so_exactly_once() {
    let (_d, p) = fixture("nq-clean");
    let mut h = H::open(&p);
    let answers = h.watch_quit();
    h.native_quit();
    assert_eq!(*answers.borrow(), [true]);
    assert_eq!(*h.host.dialog(), Dialog::None, "nothing to ask");
    h.frames(3);
    assert_eq!(answers.borrow().len(), 1, "one request, one answer");
}

#[test]
fn quitting_with_unsaved_changes_asks_exactly_as_closing_the_window_does() {
    let (_d, p) = fixture("nq-parity");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let answers = h.watch_quit();
    h.native_quit();
    assert_eq!(*h.host.dialog(), Dialog::Unsaved { error: None });
    assert!(
        answers.borrow().is_empty(),
        "AppKit is left waiting while the question is open"
    );

    // The window's own close request reaches the identical state.
    let (_d2, p2) = fixture("nq-parity2");
    let mut g = H::open(&p2);
    g.dirty_edit("Journal", "mine");
    assert!(!g.request_close());
    assert_eq!(g.host.dialog(), h.host.dialog());
}

#[test]
fn cancel_keeps_the_application_and_the_notebook_and_tells_the_system_no() {
    let (_d, p) = fixture("nq-cancel");
    let before = bodies(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let answers = h.watch_quit();
    h.native_quit();
    h.host.cancel_dialog();
    h.frames(3);
    assert_eq!(*answers.borrow(), [false]);
    assert!(h.host.is_dirty() && !h.closed && !h.host.quit_allowed());
    assert_eq!(h.host.buffer(), "journal 🦀\nmine");
    assert_eq!(bodies(&p), before, "nothing written");
    // The request was fully resolved: the next Command+Q asks again.
    h.native_quit();
    assert_eq!(*h.host.dialog(), Dialog::Unsaved { error: None });
    assert_eq!(answers.borrow().len(), 1, "and is again left waiting");
}

#[test]
fn save_completes_before_the_system_is_told_it_may_quit() {
    let (_d, p) = fixture("nq-save");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let answers = h.watch_quit();
    h.native_quit();
    assert!(answers.borrow().is_empty());
    h.host.unsaved_save(&h.ctx.clone());
    h.frames(4);
    assert_eq!(*answers.borrow(), [true]);
    assert_eq!(journal(&p), "journal 🦀\nmine", "written before the yes");
    assert!(!h.host.is_dirty());
}

#[test]
fn an_untitled_notebook_is_saved_through_save_as_before_quitting() {
    let d = dir("nq-untitled");
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    h.host.start_with_args(&h.ctx.clone(), &[]);
    h.settle();
    h.host.create_root(&h.ctx.clone());
    h.settle();
    h.type_text("typed into an untitled notebook");
    let answers = h.watch_quit();
    h.native_quit();
    assert_eq!(*h.host.dialog(), Dialog::Unsaved { error: None });

    // Save with no file yet means choosing one; Cancel there drops the quit.
    h.host.unsaved_save(&h.ctx.clone());
    assert!(matches!(h.host.dialog(), Dialog::SaveAs { then: true, .. }));
    assert!(answers.borrow().is_empty());
    h.host.cancel_dialog();
    h.frames(3);
    assert_eq!(
        *answers.borrow(),
        [false],
        "a cancelled Save As cancels the quit"
    );
    assert!(h.host.is_dirty());

    // Choosing a file saves, then quits.
    h.native_quit();
    h.host.unsaved_save(&h.ctx.clone());
    let target = d.join("rescued.omatree");
    h.host.set_save_as_path(&target.display().to_string());
    h.host.submit_save_as(&h.ctx.clone());
    h.frames(4);
    assert_eq!(*answers.borrow(), [false, true]);
    // (The text went wherever the new note's focus was; it is in the file, whichever field.)
    let saved = bodies(&target);
    assert!(
        saved
            .iter()
            .any(|(t, b)| t.contains("typed into an untitled")
                || b.contains("typed into an untitled")),
        "{saved:?}"
    );
}

#[test]
fn discard_needs_the_established_confirmation() {
    let (_d, p) = fixture("nq-discard");
    let before = bodies(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let answers = h.watch_quit();
    h.native_quit();
    h.host.unsaved_discard();
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDiscard);
    h.frames(3);
    assert!(answers.borrow().is_empty(), "one click is not enough");
    h.native_quit();
    assert_eq!(
        *h.host.dialog(),
        Dialog::ConfirmDiscard,
        "a repeat keeps the confirmation"
    );
    assert!(answers.borrow().is_empty());

    h.host.cancel_dialog(); // "Keep editing"
    h.frames(3);
    assert_eq!(*answers.borrow(), [false]);
    assert!(h.host.is_dirty());

    h.native_quit();
    h.host.unsaved_discard();
    h.host.confirm_discard(&h.ctx.clone());
    h.frames(4);
    assert_eq!(*answers.borrow(), [false, true]);
    assert_eq!(bodies(&p), before, "discarded edits never reach the file");
}

#[cfg(unix)]
#[test]
fn a_failed_save_never_lets_the_system_quit() {
    let d = dir("nq-fail");
    let nb = d.join("nb");
    fs::create_dir_all(&nb).unwrap();
    let p = nb.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let answers = h.watch_quit();
    h.native_quit();
    fs::set_permissions(&nb, fs::Permissions::from_mode(0o555)).unwrap();
    h.host.unsaved_save(&h.ctx.clone());
    h.frames(4);
    fs::set_permissions(&nb, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(matches!(
        h.host.dialog(),
        Dialog::Unsaved { error: Some(_) }
    ));
    assert!(answers.borrow().is_empty(), "still waiting, and not a yes");
    assert!(h.host.is_dirty());
    assert_eq!(journal(&p), "journal 🦀\n", "the file is as it was");

    // A repeated request while the failure is shown keeps it.
    h.native_quit();
    assert!(matches!(
        h.host.dialog(),
        Dialog::Unsaved { error: Some(_) }
    ));
    assert!(!answers.borrow().contains(&true));
    h.host.cancel_dialog();
    h.frames(3);
    assert_eq!(*answers.borrow(), [false]);
    assert_eq!(
        h.host.buffer(),
        "journal 🦀\nmine",
        "the edit is still in memory"
    );
}

#[test]
fn an_external_change_is_never_overwritten_by_a_quit() {
    let (_d, p) = fixture("nq-conflict");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let mut other = Document::open_existing(&p).unwrap();
    let j = other.notebook().depth_first().last().unwrap().id();
    other.set_body(j, "theirs\n").unwrap();
    other.save().unwrap();
    let theirs = fs::read(&p).unwrap();

    let answers = h.watch_quit();
    h.native_quit();
    h.host.unsaved_save(&h.ctx.clone());
    h.frames(4);
    assert_eq!(*h.host.dialog(), Dialog::Conflict);
    assert!(answers.borrow().is_empty());
    assert_eq!(
        fs::read(&p).unwrap(),
        theirs,
        "their version was never overwritten"
    );
    assert_eq!(h.host.buffer(), "journal 🦀\nmine", "mine is still here");

    h.native_quit(); // repeated, while the conflict question is open
    assert_eq!(*h.host.dialog(), Dialog::Conflict);
    assert!(answers.borrow().is_empty());
    h.host.cancel_dialog(); // "Keep editing"
    h.frames(3);
    assert_eq!(*answers.borrow(), [false]);
    assert_eq!(fs::read(&p).unwrap(), theirs);
}

#[test]
fn repeated_quit_requests_never_bypass_an_open_confirmation() {
    let (_d, p) = fixture("nq-repeat");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let answers = h.watch_quit();
    for _ in 0..6 {
        h.native_quit();
    }
    assert_eq!(*h.host.dialog(), Dialog::Unsaved { error: None });
    assert!(
        answers.borrow().is_empty(),
        "never a yes, never an early no"
    );
    assert!(h.host.is_dirty() && !h.host.quit_allowed());
    // Several requests in one frame are one request.
    for _ in 0..3 {
        h.raise_quit();
    }
    h.frames(3);
    assert!(answers.borrow().is_empty());
    h.host.cancel_dialog();
    h.frames(3);
    assert_eq!(
        *answers.borrow(),
        [false],
        "answered once, whatever the number of requests"
    );
}

#[test]
fn a_quit_while_another_question_is_open_is_refused_and_leaves_that_question_alone() {
    let (_d, p) = fixture("nq-other");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    h.host.new_notebook(&h.ctx.clone()); // "replace this notebook?" asks about unsaved changes
    h.frames(2);
    assert_eq!(*h.host.dialog(), Dialog::Unsaved { error: None });
    let answers = h.watch_quit();
    h.native_quit();
    assert_eq!(*answers.borrow(), [false], "the system is told no, at once");
    assert_eq!(*h.host.dialog(), Dialog::Unsaved { error: None });
    assert!(h.host.is_dirty());
    // The question that was open still does what it said: Cancel drops the New, nothing else.
    h.host.cancel_dialog();
    h.frames(3);
    assert!(h.host.is_dirty() && !h.closed);
    assert_eq!(h.host.buffer(), "journal 🦀\nmine");
}

#[test]
fn text_typed_in_the_instant_before_the_quit_is_part_of_what_is_asked_about() {
    // Regression (found on the real Mac): the quit key arrives, with the last keystrokes still
    // queued. egui applies queued text in `ui`, after `logic`, so a request judged in the same
    // frame saw a clean notebook, answered yes and lost the typing. It is judged one frame later.
    let (_d, p) = fixture("nq-race");
    let mut h = H::open(&p);
    h.select("Journal");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    assert!(!h.host.is_dirty(), "a clean notebook");
    let answers = h.watch_quit();
    h.raise_quit();
    h.frame(vec![Event::Text("last words".to_owned())], Modifiers::NONE);
    h.frames(3);
    assert_eq!(
        *h.host.dialog(),
        Dialog::Unsaved { error: None },
        "the typing was seen"
    );
    assert!(
        answers.borrow().is_empty(),
        "so the system was not told it may quit"
    );
    assert_eq!(h.host.buffer(), "journal 🦀\nlast words");
    assert_eq!(
        journal(&p),
        "journal 🦀\n",
        "and nothing was written behind the question"
    );
}

#[test]
fn without_a_platform_hook_nothing_changes() {
    // Linux and Windows never raise the flag and never install an answer: the default is inert.
    let (_d, p) = fixture("nq-inert");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    h.frames(10);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.host.is_dirty());
}
