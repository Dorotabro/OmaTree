//! M1: the key labelled Delete on an Apple keyboard sends Backspace (egui `Key::Backspace`), not
//! the forward-delete `Key::Delete` the tree handled. On macOS it must open the same "Move to
//! Trash?" question; nowhere else may Backspace delete a note, and no text field may be affected.
//! The platform is egui's own `Context::os`, so every test runs on every system.

use super::tree_tests::{deep, shape};
use super::*;
use egui::os::OperatingSystem;

fn mac(path: &Path) -> H {
    let h = H::open_tree(path);
    h.ctx.set_os(OperatingSystem::Mac);
    h
}

#[test]
fn the_mac_delete_key_asks_to_move_the_selected_note_to_trash() {
    let (_d, p) = deep("mk-delete");
    let mut h = mac(&p);
    h.go("B");
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDelete(h.id("B")));
    // The same question as forward-delete gives, with the same safe default: Enter declines.
    h.press(Key::Enter);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
    assert!(!h.host.is_dirty(), "asking changed nothing");
    assert!(h.has_focus(Focus::Tree), "focus is back in the tree");
}

#[test]
fn a_parent_with_descendants_goes_to_trash_whole_and_is_restorable() {
    let (_d, p) = deep("mk-parent");
    let mut h = mac(&p);
    h.go("A");
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDelete(h.id("A")));
    h.press(Key::Tab); // from "Cancel" to "Move to Trash"
    h.press(Key::Enter);
    h.settle();
    assert_eq!(shape(h.host.document().unwrap()), "B(B1)|C");
    let trash = h.host.document().unwrap().recovery().trash();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].node_count(), 5, "A with all four notes beneath it");

    // Recovery brings the whole subtree back, through the existing path.
    let ctx = h.ctx.clone();
    assert!(h.host.restore_trash(&ctx, 0));
    h.settle();
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
    assert_eq!(h.body("Leaf"), "leaf body\n");
}

#[test]
fn a_single_note_is_trashed_with_the_mac_key_and_comes_back() {
    let (_d, p) = deep("mk-single");
    let mut h = mac(&p);
    h.go("A2");
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    h.press(Key::Tab);
    h.press(Key::Enter);
    h.settle();
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)))|B(B1)|C"
    );
    assert_eq!(h.trash_titles(), ["A2"]);
    let ctx = h.ctx.clone();
    assert!(h.host.restore_trash(&ctx, 0));
    h.settle();
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
}

#[test]
fn other_systems_keep_their_keys_backspace_in_the_tree_does_nothing_there() {
    for os in [OperatingSystem::Nix, OperatingSystem::Windows] {
        let (_d, p) = deep("mk-other");
        let mut h = H::open_tree(&p);
        h.ctx.set_os(os);
        h.go("B");
        h.key(Key::Backspace, Modifiers::NONE);
        h.settle();
        assert_eq!(*h.host.dialog(), Dialog::None, "{os:?}");
        // And the Delete key still asks, everywhere, the Mac included.
        h.key(Key::Delete, Modifiers::NONE);
        h.settle();
        assert_eq!(*h.host.dialog(), Dialog::ConfirmDelete(h.id("B")), "{os:?}");
    }
    let (_d, p) = deep("mk-mac-fwd");
    let mut h = mac(&p);
    h.go("B");
    h.key(Key::Delete, Modifiers::NONE);
    h.settle();
    assert_eq!(
        *h.host.dialog(),
        Dialog::ConfirmDelete(h.id("B")),
        "forward-delete still works on a Mac"
    );
}

#[test]
fn modified_backspace_is_not_the_delete_key() {
    // Option+Delete deletes a word in a text field, Command+Delete a line; neither is a request to
    // trash a note.
    let (_d, p) = deep("mk-modified");
    let mut h = mac(&p);
    h.go("B");
    for m in [Modifiers::ALT, Modifiers::COMMAND, Modifiers::CTRL] {
        h.key(Key::Backspace, m);
        h.settle();
        assert_eq!(*h.host.dialog(), Dialog::None, "{m:?}");
    }
}

#[test]
fn nothing_happens_without_a_selection() {
    // An empty untitled notebook has no note to select.
    let mut h = H::bare(Vec2::new(1000.0, 700.0));
    h.ctx.set_os(OperatingSystem::Mac);
    h.host.start_with_args(&h.ctx.clone(), &[]);
    h.host.focus(Focus::Tree);
    h.settle();
    assert!(h.host.selected().is_none());
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn backspace_in_text_fields_edits_text_and_never_asks_to_delete_a_note() {
    let (_d, p) = deep("mk-fields");
    let mut h = mac(&p);
    h.go("B");

    // The note body.
    h.host.focus(Focus::Editor);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None, "editor");
    assert_eq!(h.body("B"), "b body", "the newline was backspaced");

    // The title.
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("xy");
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None, "title");
    assert_eq!(
        h.host.title_text(),
        "x",
        "typing replaced the selected title, Backspace removed one letter"
    );

    // The search field.
    h.host.start_search();
    h.settle();
    h.type_text("abc");
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None, "search");
    assert_eq!(h.host.search_query(), "ab");
    // (The typed title was committed as a rename when the focus left it; that is the title field
    // doing its own job. No note was deleted, and nothing went to Trash.)
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|x(B1)|C"
    );
    assert!(h.trash_titles().is_empty());
}

#[test]
fn backspace_does_nothing_extra_while_a_dialog_is_open() {
    let (_d, p) = deep("mk-dialog");
    let mut h = mac(&p);
    h.go("B");
    h.key(Key::Backspace, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDelete(h.id("B")));
    h.key(Key::Backspace, Modifiers::NONE); // a second press does not stack or confirm
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDelete(h.id("B")));
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
}
