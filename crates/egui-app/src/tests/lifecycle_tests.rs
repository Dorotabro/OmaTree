//! Close, Save As, autosave and conflict behaviour (ticket E4.1). Same harness as the parent
//! module: a real `Document` on a private copy, egui driven frame by frame, close requests
//! delivered the way eframe 0.37 delivers them (`ViewportInfo::events`).

use super::*;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn fixture(name: &str) -> (crate::tests::TestDir, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    make_fixture(&p);
    (d, p)
}

fn journal(p: &Path) -> String {
    bodies(p).last().unwrap().1.clone()
}

#[cfg(unix)]
/// Make the notebook's directory unwritable (SQLite cannot create its journal), run `f`, restore.
fn unwritable<T>(dir: &Path, f: impl FnOnce() -> T) -> T {
    fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
    let r = f();
    fs::set_permissions(dir, fs::Permissions::from_mode(0o755)).unwrap();
    r
}

#[test]
fn a_clean_notebook_closes_immediately() {
    let (_d, p) = fixture("close-clean");
    let mut h = H::open(&p);
    h.select("Deep"); // switching alone does not make it dirty
    assert!(h.request_close());
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn closing_with_unsaved_changes_asks_and_cancel_keeps_everything() {
    let (_d, p) = fixture("close-cancel");
    let before = bodies(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    assert!(!h.request_close());
    assert!(matches!(h.host.dialog(), Dialog::Unsaved { .. }));
    h.host.cancel_dialog();
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(!h.closed && h.host.is_dirty());
    assert_eq!(h.host.buffer(), "journal 🦀\nmine");
    assert_eq!(bodies(&p), before, "nothing written");
    // Still editable afterwards.
    h.type_text("!");
    assert_eq!(h.body("Journal"), "journal 🦀\nmine!");
}

#[test]
fn closing_then_save_writes_and_closes() {
    let (_d, p) = fixture("close-save");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    assert!(!h.request_close());
    h.host.unsaved_save(&h.ctx.clone());
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(h.closed, "closes after a successful save");
    assert_eq!(journal(&p), "journal 🦀\nmine");
    assert!(!h.host.is_dirty());
}

#[test]
fn closing_then_discard_needs_a_second_confirmation_and_writes_nothing() {
    let (_d, p) = fixture("close-discard");
    let before = bodies(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    assert!(!h.request_close());
    h.host.unsaved_discard();
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDiscard);
    assert!(!h.request_close(), "a repeated request still asks");
    h.host.cancel_dialog(); // "Keep editing"
    h.settle();
    assert!(!h.closed && h.host.is_dirty());
    assert!(!h.request_close());
    h.host.unsaved_discard();
    h.host.confirm_discard(&h.ctx.clone());
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(h.closed);
    assert_eq!(bodies(&p), before, "discarded edits never reach the file");
}

#[cfg(unix)]
#[test]
fn a_failed_save_during_close_keeps_the_window_dirty_and_editable_and_save_as_recovers() {
    let d = dir("close-fail");
    let nb = d.join("nb");
    fs::create_dir_all(&nb).unwrap();
    let p = nb.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    assert!(!h.request_close());
    unwritable(&nb, || h.host.unsaved_save(&h.ctx.clone()));
    h.settle();
    assert!(!h.closed);
    assert!(h.host.is_dirty());
    let Dialog::Unsaved { error: Some(e) } = h.host.dialog().clone() else {
        panic!("{:?}", h.host.dialog())
    };
    assert!(e.contains("could not be saved"), "{e}");
    assert_eq!(h.host.buffer(), "journal 🦀\nmine");
    assert_eq!(journal(&p), "journal 🦀\n", "the file is as it was");

    // Recovery: Save As to a writable place, which then closes.
    h.host.start_save_as(&h.ctx.clone(), true);
    let target = d.join("rescued.omatree");
    h.host.set_save_as_path(&target.display().to_string());
    h.host.submit_save_as(&h.ctx.clone());
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(h.closed);
    assert_eq!(journal(&target), "journal 🦀\nmine");
    assert_eq!(journal(&p), "journal 🦀\n");
}

#[test]
fn an_external_modification_blocks_saves_keeps_edits_and_requires_resolution_to_close() {
    let (d, p) = fixture("conflict2");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    let mut other = Document::open_existing(&p).unwrap();
    let j = other.notebook().depth_first().last().unwrap().id();
    other.set_body(j, "theirs\n").unwrap();
    other.save().unwrap();
    let theirs = fs::read(&p).unwrap();

    // The autosave that fires reports once and stops.
    h.advance(1.5);
    assert!(h.host.autosave_blocked());
    assert!(
        h.host.status().starts_with("Autosave failed"),
        "{}",
        h.host.status()
    );
    assert!(h.host.status().contains("changed on disk"));
    assert!(h.host.is_dirty());
    let status = h.host.status().to_owned();
    h.type_text("more");
    h.advance(3.0);
    assert_eq!(h.host.status(), status, "no repeated retry or message");
    assert_eq!(
        fs::read(&p).unwrap(),
        theirs,
        "their version was never overwritten"
    );

    // Closing still asks; Save fails the same way and stays open.
    assert!(!h.request_close());
    h.host.unsaved_save(&h.ctx.clone());
    h.settle();
    assert!(!h.closed);
    assert_eq!(
        *h.host.dialog(),
        Dialog::Conflict,
        "E8b: the choices are Save As / Reload / Keep"
    );
    let e = h.host.autosave_problem().unwrap().to_owned();
    assert!(e.contains("Save As"), "the message points to Save As: {e}");

    // Explicit resolution: keep my version under another name; then the window closes.
    h.host.start_save_as(&h.ctx.clone(), true);
    let mine = d.join("mine.omatree");
    h.host.set_save_as_path(&mine.display().to_string());
    h.host.submit_save_as(&h.ctx.clone());
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(h.closed);
    assert_eq!(journal(&mine), "journal 🦀\nminemore");
    assert_eq!(fs::read(&p).unwrap(), theirs);
    assert!(
        !h.host.autosave_blocked(),
        "a successful Save As unblocks autosave"
    );
}

#[test]
fn save_as_writes_a_new_schema_3_notebook_and_continues_there() {
    let (d, p) = fixture("saveas-new");
    let original = fs::read(&p).unwrap();
    let mut h = H::open(&p);
    h.dirty_edit("Journal", " edited");
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    let Dialog::SaveAs { path, .. } = h.host.dialog().clone() else {
        panic!()
    };
    assert!(path.ends_with("n copy.omatree"), "{path}");
    // A bare name gets .omatree and lands next to the current notebook.
    h.host.set_save_as_path("fresh");
    h.host.submit_save_as(&h.ctx.clone());
    h.settle();
    let target = d.join("fresh.omatree");
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(
        h.host.status().starts_with("Saved as fresh.omatree"),
        "{}",
        h.host.status()
    );
    assert_eq!(
        &fs::read(&target).unwrap()[60..64],
        &[0, 0, 0, 3],
        "schema 3, Qt-compatible"
    );
    assert_eq!(journal(&target), "journal 🦀\n edited");
    assert_eq!(fs::read(&p).unwrap(), original, "the old file is untouched");
    // Structure identical, only the body differs.
    let (a, b) = (snapshot(&p), snapshot(&target));
    for (x, y) in a.iter().zip(&b) {
        assert_eq!((x.0, x.1, x.2, &x.3), (y.0, y.1, y.2, &y.3));
    }
    // Later saves go to the new file.
    h.type_text("!");
    h.key(Key::S, CTRL);
    assert_eq!(journal(&target), "journal 🦀\n edited!");
    assert_eq!(fs::read(&p).unwrap(), original);
    assert!(h.host.document().unwrap().is_current_path(&target));
}

#[test]
fn save_as_never_replaces_an_existing_file_without_confirmation() {
    let (d, p) = fixture("saveas-over");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", " x");

    // An unrelated file: asking, refusing, and even confirming cannot damage it.
    let unrelated = d.join("notes.txt");
    fs::write(&unrelated, "precious\n").unwrap();
    h.host.start_save_as(&h.ctx.clone(), false);
    h.host.set_save_as_path(&unrelated.display().to_string());
    h.host.submit_save_as(&h.ctx.clone());
    assert!(matches!(h.host.dialog(), Dialog::Overwrite { .. }));
    h.host.cancel_dialog();
    h.settle();
    assert_eq!(fs::read_to_string(&unrelated).unwrap(), "precious\n");
    h.host.start_save_as(&h.ctx.clone(), false);
    h.host.set_save_as_path(&unrelated.display().to_string());
    h.host.submit_save_as(&h.ctx.clone());
    h.host.confirm_overwrite(&h.ctx.clone());
    let Dialog::SaveAs { error: Some(e), .. } = h.host.dialog().clone() else {
        panic!("{:?}", h.host.dialog())
    };
    assert!(e.contains("not an OmaTree notebook"), "{e}");
    assert_eq!(fs::read_to_string(&unrelated).unwrap(), "precious\n");
    assert!(h.host.is_dirty());
    h.host.cancel_dialog();

    // Another notebook: replaced only after Yes.
    let other = d.join("other.omatree");
    make_fixture(&other);
    let other_before = fs::read(&other).unwrap();
    h.host.start_save_as(&h.ctx.clone(), false);
    h.host.set_save_as_path(&other.display().to_string());
    h.host.submit_save_as(&h.ctx.clone());
    assert!(matches!(h.host.dialog(), Dialog::Overwrite { .. }));
    assert_eq!(
        fs::read(&other).unwrap(),
        other_before,
        "asking changes nothing"
    );
    h.host.confirm_overwrite(&h.ctx.clone());
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(journal(&other), "journal 🦀\n x");
}

#[test]
fn autosave_writes_changed_content_after_the_quiet_period_and_only_then() {
    let (_d, p) = fixture("autosave");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "typed");
    assert_eq!(journal(&p), "journal 🦀\n", "not yet");
    h.advance(0.5);
    assert_eq!(journal(&p), "journal 🦀\n", "still inside the quiet period");
    h.type_text("more"); // another edit restarts the countdown
    h.advance(0.8);
    assert_eq!(journal(&p), "journal 🦀\n");
    h.advance(0.5);
    assert_eq!(journal(&p), "journal 🦀\ntypedmore");
    assert!(!h.host.is_dirty());
    assert_eq!(h.host.status(), "Autosaved");
}

#[test]
fn autosave_does_not_write_when_nothing_changed() {
    let (_d, p) = fixture("autosave-idle");
    let before = fs::read(&p).unwrap();
    let mtime = fs::metadata(&p).unwrap().modified().unwrap();
    let mut h = H::open(&p);
    h.select("Plán");
    h.select("Deep");
    h.advance(5.0);
    assert!(!h.host.is_dirty());
    assert_eq!(fs::read(&p).unwrap(), before);
    assert_eq!(fs::metadata(&p).unwrap().modified().unwrap(), mtime);
    assert_eq!(h.host.status(), "Opened n.omatree", "no autosave happened");
    // An edit that is undone right back is still a change for Document, so it saves once; a
    // plain selection or caret move never does.
    h.set_caret(1, 3);
    h.advance(3.0);
    assert_eq!(fs::read(&p).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn a_failed_autosave_keeps_the_edits_reports_once_and_a_later_save_recovers() {
    let d = dir("autosave-fail");
    let nb = d.join("nb");
    fs::create_dir_all(&nb).unwrap();
    let p = nb.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "typed");
    unwritable(&nb, || h.advance(1.5));
    assert!(h.host.is_dirty(), "failure never clears the dirty state");
    assert_eq!(h.host.buffer(), "journal 🦀\ntyped");
    assert!(
        h.host.status().starts_with("Autosave failed"),
        "{}",
        h.host.status()
    );
    assert!(h.host.autosave_problem().is_some());
    assert_eq!(journal(&p), "journal 🦀\n");
    let msg = h.host.status().to_owned();
    // Idle time alone does not retry or repeat the message.
    h.advance(10.0);
    assert_eq!(h.host.status(), msg);
    // The next edit tries again (the directory is writable now) and succeeds.
    h.type_text("!");
    h.advance(1.5);
    assert!(!h.host.is_dirty());
    assert_eq!(journal(&p), "journal 🦀\ntyped!");
    assert!(h.host.autosave_problem().is_none());
}

#[test]
fn saving_flushes_what_the_editor_still_holds() {
    let (_d, p) = fixture("flush");
    let mut h = H::open(&p);
    h.select("Journal");
    // The buffer is ahead of the document (what a pending editor interaction produces).
    h.host.text.push_str("pending");
    assert_eq!(h.body("Journal"), "journal 🦀\n");
    h.key(Key::S, CTRL);
    assert_eq!(journal(&p), "journal 🦀\npending");
    // The same flush happens before the close question and before Save As.
    h.host.text.push_str("+close");
    assert!(!h.request_close());
    assert!(h.host.is_dirty());
    assert_eq!(h.body("Journal"), "journal 🦀\npending+close");
    h.host.cancel_dialog();
    h.host.text.push_str("+saveas");
    h.host.start_save_as(&h.ctx.clone(), false);
    assert_eq!(h.body("Journal"), "journal 🦀\npending+close+saveas");
}

#[test]
fn repeated_close_requests_never_discard_changes() {
    let (_d, p) = fixture("close-repeat");
    let before = bodies(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    for _ in 0..5 {
        assert!(!h.request_close());
    }
    assert!(matches!(h.host.dialog(), Dialog::Unsaved { .. }));
    h.host.unsaved_discard();
    for _ in 0..5 {
        assert!(!h.request_close(), "the confirmation step is not a bypass");
    }
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDiscard);
    assert!(h.host.is_dirty());
    assert_eq!(bodies(&p), before);
    // Also while a Save As question is open.
    h.host.cancel_dialog();
    h.host.start_save_as(&h.ctx.clone(), false);
    assert!(!h.request_close());
    assert!(matches!(h.host.dialog(), Dialog::SaveAs { .. }));
    assert!(!h.closed);
}

#[test]
fn the_close_question_works_from_the_keyboard() {
    let (_d, p) = fixture("close-keys");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    assert!(!h.request_close());
    // Escape: back to editing.
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(!h.closed);
    // Enter activates the focused default button, Save.
    assert!(!h.request_close());
    h.press(Key::Enter);
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(h.closed, "{:?}", h.host.dialog());
    assert_eq!(journal(&p), "journal 🦀\nmine");
}

#[test]
fn typing_and_shortcuts_are_ignored_while_a_dialog_is_open() {
    let (_d, p) = fixture("modal");
    let mut h = H::open(&p);
    h.dirty_edit("Journal", "mine");
    assert!(!h.request_close());
    h.type_text("zzz");
    h.key(Key::S, CTRL);
    assert_eq!(h.host.buffer(), "journal 🦀\nmine");
    assert!(h.host.is_dirty(), "Ctrl+S did not act behind the dialog");
}

#[test]
fn a_saved_notebook_is_a_schema_3_file_that_roundtrips_for_qt() {
    let (_d, p) = fixture("roundtrip3");
    let before = snapshot(&p);
    let mut h = H::open(&p);
    h.dirty_edit("Plán", " ěšč 🦀");
    h.key(Key::S, CTRL);
    drop(h);
    let raw = fs::read(&p).unwrap();
    assert_eq!(&raw[60..64], &[0, 0, 0, 3]);
    let after = snapshot(&p);
    assert_eq!(before.len(), after.len());
    for (b, a) in before.iter().zip(&after) {
        assert_eq!((b.0, b.1, b.2, &b.3), (a.0, a.1, a.2, &a.3));
        if a.3 != "Plán" {
            assert_eq!(b.4, a.4);
        }
    }
    assert!(after[1].4.ends_with(" ěšč 🦀"));
}

/// P2 replaced the pinned Git dependency with the workspace crate `crates/editor`: the build must need no
/// other repository, so the dependency is a path inside this checkout and nothing in the lock file is a Git source.
#[test]
fn the_editor_is_a_workspace_crate_and_nothing_is_fetched_from_git() {
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let line = manifest
        .lines()
        .find(|l| l.starts_with("omatree-editor"))
        .unwrap();
    assert_eq!(line, "omatree-editor = { path = \"../editor\" }");
    assert!(!line.contains("git"), "{line}");
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let workspace = fs::read_to_string(format!("{root}/Cargo.toml")).unwrap();
    assert!(
        workspace.contains("\"crates/editor\""),
        "the editor is a workspace member"
    );
    let lock = fs::read_to_string(format!("{root}/Cargo.lock")).unwrap();
    assert!(
        !lock.contains("source = \"git+"),
        "no Git source in Cargo.lock"
    );
    assert!(!lock.contains("omatree-egui-editor-spike"));
}
