//! The hierarchical tree and its keyboard (ticket E5a). Behaviour is taken from the Qt app
//! (`TreePane.qml`, `main.qml`, `tests/qml/{expansion,moveexpanded,keyboard,newnote,titles,
//! titleclose,escape}.qml`, and the running app for Left/Right/Page keys), and checked here
//! through frames of a real `Document` on disposable copies.

use super::*;
use std::time::Duration;

/// A (A1 (A1a (Leaf)), A2), B (B1), and a lone C.
pub(super) fn make_deep(path: &Path) {
    let mut doc = Document::open(path).unwrap();
    let a = doc.create_root("A").unwrap();
    let a1 = doc.create_child(a, "A1").unwrap();
    let a1a = doc.create_child(a1, "A1a").unwrap();
    let leaf = doc.create_child(a1a, "Leaf").unwrap();
    let a2 = doc.create_child(a, "A2").unwrap();
    let b = doc.create_root("B").unwrap();
    let b1 = doc.create_child(b, "B1").unwrap();
    let c = doc.create_root("C").unwrap();
    for (id, body) in [
        (a, "a body\n"),
        (a1, "a1 body\n"),
        (a1a, "a1a body\n"),
        (leaf, "leaf body\n"),
        (a2, "a2 body\n"),
        (b, "b body\n"),
        (b1, "b1 body\n"),
        (c, "c body\n"),
    ] {
        doc.set_body(id, body).unwrap();
    }
    doc.save().unwrap();
}

pub(super) fn deep(name: &str) -> (crate::tests::TestDir, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    make_deep(&p);
    (d, p)
}

/// "A(A1(A1a(Leaf)),A2)|B(B1)|C": the real hierarchy, whatever is open.
pub(super) fn shape(doc: &Document) -> String {
    fn walk(doc: &Document, id: NodeId) -> String {
        let nb = doc.notebook();
        let title = nb.get(id).unwrap().title().to_owned();
        let kids: Vec<String> = nb
            .children(id)
            .unwrap()
            .iter()
            .map(|n| walk(doc, n.id()))
            .collect();
        if kids.is_empty() {
            title
        } else {
            format!("{title}({})", kids.join(","))
        }
    }
    doc.notebook()
        .roots()
        .iter()
        .map(|n| walk(doc, n.id()))
        .collect::<Vec<_>>()
        .join("|")
}

impl H {
    /// Opened the way the app opens: the tree has the keyboard.
    pub(super) fn open_tree(path: &Path) -> Self {
        let mut h = H::open(path);
        h.host.focus(Focus::Tree);
        h.settle();
        h
    }

    pub(super) fn vis(&self) -> Vec<String> {
        let t = self.host.tree();
        (0..t.len_visible())
            .map(|i| t.visible_row(i).title.clone())
            .collect()
    }

    pub(super) fn sel(&self) -> String {
        let id = self.host.selected().expect("a selection");
        self.host.tree().row(id).unwrap().title.clone()
    }

    pub(super) fn has_focus(&self, f: Focus) -> bool {
        let id = match f {
            Focus::Tree => Host::tree_id(),
            Focus::Title => self.host.title_id(),
            Focus::Editor => {
                key_of(self.host.notebook_id(), self.host.selected().unwrap()).widget_id()
            }
            Focus::Search => Host::search_id(),
        };
        self.ctx.memory(|m| m.has_focus(id))
    }

    pub(super) fn expanded(&self, title: &str) -> bool {
        self.host.document().unwrap().is_expanded(self.id(title))
    }

    pub(super) fn go(&mut self, title: &str) {
        let id = self.id(title);
        assert!(self.host.select(&self.ctx.clone(), id), "select {title}");
        self.host.focus(Focus::Tree);
        self.settle();
    }

    pub(super) fn collapse_all(&mut self) {
        self.host.set_all_expanded(&self.ctx.clone(), false);
        self.settle();
    }

    pub(super) fn trash_titles(&self) -> Vec<String> {
        self.host
            .document()
            .unwrap()
            .recovery()
            .trash()
            .iter()
            .map(|t| t.title().to_owned())
            .collect()
    }
}

// --- Structure, expansion, selection ---------------------------------------------------------

#[test]
fn rows_follow_the_real_hierarchy_and_only_open_branches_are_shown() {
    let (_d, p) = deep("tree-shape");
    let h = H::open_tree(&p);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
    // Nothing was stored as open: only the top level shows.
    assert_eq!(h.vis(), ["A", "B", "C"]);
    let rows: Vec<_> = h
        .host
        .rows()
        .iter()
        .map(|r| (r.title.as_str(), r.depth, r.has_children))
        .collect();
    assert_eq!(
        rows,
        [
            ("A", 0, true),
            ("A1", 1, true),
            ("A1a", 2, true),
            ("Leaf", 3, false),
            ("A2", 1, false),
            ("B", 0, true),
            ("B1", 1, false),
            ("C", 0, false)
        ]
    );
    // Branch guides: A1 has a later sibling (A2), so its descendants carry a guide in column 0.
    let a1a = h.host.tree().row(h.id("A1a")).unwrap();
    assert!(!h.host.tree().row(h.id("A1")).unwrap().last);
    assert!(h.host.tree().row(h.id("A2")).unwrap().last);
    assert_eq!(a1a.guides & 1, 1, "guide through A1a for A1's column");
    assert_eq!(h.host.tree().row(h.id("Leaf")).unwrap().guides, 0b01);
}

#[test]
fn expansion_follows_the_qt_scenario_and_is_view_state_only() {
    let (_d, p) = deep("tree-expand");
    let before = fs::read(&p).unwrap();
    let mut h = H::open_tree(&p);
    let ctx = h.ctx.clone();
    h.collapse_all();
    assert_eq!(h.vis(), ["A", "B", "C"]);
    h.host.set_subtree_expanded(&ctx, h.id("A"), true);
    assert_eq!(h.vis(), ["A", "A1", "A1a", "Leaf", "A2", "B", "C"]);
    assert!(h.expanded("A") && h.expanded("A1") && h.expanded("A1a"));
    assert!(!h.expanded("B"), "B was not touched");
    assert!(!h.host.is_dirty(), "expansion is not content");
    h.host.set_subtree_expanded(&ctx, h.id("A"), false);
    assert_eq!(h.vis(), ["A", "B", "C"]);
    h.host.set_subtree_expanded(&ctx, h.id("A"), true);
    assert!(h.vis().contains(&"Leaf".to_owned()));
    // Plain expand/collapse of one note leaves its descendants as they were.
    h.host.set_expanded(&ctx, h.id("A1"), false);
    assert_eq!(h.vis(), ["A", "A1", "A2", "B", "C"]);
    assert!(
        h.expanded("A1a"),
        "the document still remembers A1a as open"
    );
    h.host.set_expanded(&ctx, h.id("A1"), true);
    assert!(h.vis().contains(&"Leaf".to_owned()));
    // A note without children cannot be opened.
    h.host.set_expanded(&ctx, h.id("A2"), true);
    assert!(!h.expanded("A2"));
    assert_eq!(fs::read(&p).unwrap(), before, "nothing written yet");
    assert!(!h.host.is_dirty());
}

#[test]
fn expansion_is_saved_with_the_notebook_and_restored_on_reopen() {
    let (_d, p) = deep("tree-persist");
    let mut h = H::open_tree(&p);
    let ctx = h.ctx.clone();
    h.host.set_subtree_expanded(&ctx, h.id("A"), true);
    h.host.set_expanded(&ctx, h.id("B"), true);
    h.host.set_expanded(&ctx, h.id("A1"), false);
    let shown = h.vis();
    h.key(Key::S, CTRL);
    assert_eq!(h.host.status(), "Saved");
    drop(h);
    let h2 = H::open_tree(&p);
    assert_eq!(h2.vis(), shown);
    assert!(h2.expanded("A") && h2.expanded("A1a") && !h2.expanded("A1") && h2.expanded("B"));
    assert!(!h2.host.is_dirty(), "reopening is clean");
    assert_eq!(
        shape(h2.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
}

#[test]
fn expansion_alone_never_autosaves_but_a_clean_close_still_keeps_it() {
    let (_d, p) = deep("tree-viewclose");
    let mtime = fs::metadata(&p).unwrap().modified().unwrap();
    let mut h = H::open_tree(&p);
    let ctx = h.ctx.clone();
    h.host.set_expanded(&ctx, h.id("A"), true);
    h.advance(5.0);
    assert_eq!(
        fs::metadata(&p).unwrap().modified().unwrap(),
        mtime,
        "no autosave for view state"
    );
    assert!(!h.host.is_dirty());
    assert!(h.request_close(), "nothing to ask about");
    drop(h);
    let h2 = H::open_tree(&p);
    assert!(h2.expanded("A"), "the open note was written by the close");
}

#[test]
fn up_and_down_move_through_the_visible_rows_without_wrapping() {
    let (_d, p) = deep("tree-updown");
    let mut h = H::open_tree(&p);
    assert_eq!(h.sel(), "A");
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(h.sel(), "A", "no wrap at the top");
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.sel(), "B", "A's children are hidden, so Down skips them");
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.sel(), "C");
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.sel(), "C", "no wrap at the bottom");
    assert_eq!(
        h.host.buffer(),
        "c body\n",
        "the editor follows the selection"
    );
    h.go("A");
    h.key(Key::ArrowRight, Modifiers::NONE);
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.sel(), "A1", "once A is open Down enters it");
    assert!(
        h.has_focus(Focus::Tree),
        "the tree keeps the keyboard while navigating"
    );
    assert!(!h.host.is_dirty());
}

#[test]
fn left_and_right_only_toggle_and_ctrl_arrows_do_the_whole_subtree() {
    let (_d, p) = deep("tree-leftright");
    let mut h = H::open_tree(&p);
    // Right opens a closed branch; Right again does nothing (the Qt TreeView does not descend).
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert!(h.expanded("A"));
    assert_eq!(h.vis(), ["A", "A1", "A2", "B", "C"]);
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(h.sel(), "A");
    assert_eq!(h.vis().len(), 5);
    // Left closes an open branch; Left on a closed one or a leaf does nothing.
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert!(!h.expanded("A"));
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert_eq!(h.sel(), "A", "Left does not move to the parent");
    h.go("C");
    h.key(Key::ArrowLeft, Modifiers::NONE);
    h.key(Key::ArrowRight, Modifiers::NONE);
    assert_eq!(h.sel(), "C");
    // Ctrl+Right opens the subtree, Ctrl+Left closes it.
    h.go("A");
    h.key(Key::ArrowRight, CTRL);
    assert_eq!(h.vis(), ["A", "A1", "A1a", "Leaf", "A2", "B", "C"]);
    h.key(Key::ArrowLeft, CTRL);
    assert_eq!(h.vis(), ["A", "B", "C"]);
    assert!(!h.expanded("A1a"), "the whole subtree was closed");
    assert!(!h.host.is_dirty());
}

#[test]
fn a_hidden_selection_moves_to_the_nearest_visible_ancestor() {
    let (_d, p) = deep("tree-hidden");
    let mut h = H::open_tree(&p);
    let ctx = h.ctx.clone();
    h.host.set_subtree_expanded(&ctx, h.id("A"), true);
    h.go("Leaf");
    assert_eq!(h.host.buffer(), "leaf body\n");
    h.host.set_expanded(&ctx, h.id("A1"), false);
    assert_eq!(
        h.sel(),
        "A1",
        "Leaf is hidden inside A1, so A1 becomes current"
    );
    assert_eq!(h.host.buffer(), "a1 body\n");
    h.host.set_subtree_expanded(&ctx, h.id("A"), true);
    h.go("Leaf");
    h.host.set_all_expanded(&ctx, false);
    assert_eq!(h.sel(), "A", "collapse all leaves the top-level ancestor");
    assert_eq!(h.vis(), ["A", "B", "C"]);
    assert!(!h.host.is_dirty());
}

#[test]
fn selecting_a_hidden_note_reveals_it() {
    let (_d, p) = deep("tree-reveal");
    let mut h = H::open_tree(&p);
    h.go("Leaf");
    assert_eq!(h.vis(), ["A", "A1", "A1a", "Leaf", "A2", "B", "C"]);
    assert!(h.expanded("A") && h.expanded("A1") && h.expanded("A1a"));
    assert!(!h.host.is_dirty(), "revealing is view state");
}

#[test]
fn enter_goes_to_the_editor_and_escape_comes_back_but_does_nothing_in_the_tree() {
    let (_d, p) = deep("tree-enter");
    let mut h = H::open_tree(&p);
    assert!(h.has_focus(Focus::Tree));
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(h.sel(), "A");
    assert!(h.has_focus(Focus::Tree), "Escape in the tree does nothing");
    h.key(Key::Enter, Modifiers::NONE);
    h.settle();
    assert!(h.has_focus(Focus::Editor), "Enter starts editing the note");
    h.key(Key::Escape, Modifiers::NONE);
    h.settle();
    assert!(
        h.has_focus(Focus::Tree),
        "Escape in the editor returns to the tree"
    );
    assert_eq!(h.host.buffer(), "a body\n");
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.sel(), "B", "the arrows work at once");
}

#[test]
fn tree_keys_do_not_fire_while_text_is_being_edited() {
    let (_d, p) = deep("tree-typing");
    let mut h = H::open_tree(&p);
    h.host.set_expanded(&h.ctx.clone(), h.id("A"), true);
    h.host.focus(Focus::Editor);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    let before = h.host.buffer().to_owned();
    for k in [
        Key::ArrowDown,
        Key::ArrowUp,
        Key::ArrowRight,
        Key::ArrowLeft,
    ] {
        h.key(k, Modifiers::NONE);
    }
    h.set_caret(end, end);
    h.key(Key::Delete, Modifiers::NONE); // at the end of the text: nothing to delete
    h.key(Key::ArrowRight, CTRL);
    assert_eq!(h.sel(), "A", "the selection did not move");
    assert_eq!(
        *h.host.dialog(),
        Dialog::None,
        "Delete is a text key in the editor"
    );
    assert!(h.expanded("A"), "Left/Right moved the caret, not the tree");
    assert_eq!(h.host.buffer(), before);
    // The same in the title field.
    h.host.focus(Focus::Title);
    h.settle();
    h.key(Key::ArrowDown, Modifiers::NONE);
    h.key(Key::Delete, Modifiers::NONE);
    assert_eq!(h.sel(), "A");
    assert_eq!(*h.host.dialog(), Dialog::None);
}

#[test]
fn page_keys_move_by_a_page_and_stop_at_the_ends() {
    let d = dir("tree-page");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    for i in 0..60 {
        doc.create_root(&format!("Note {i:02}")).unwrap();
    }
    doc.save().unwrap();
    let mut h = H::open_tree(&p);
    h.key(Key::PageDown, Modifiers::NONE);
    let first = h
        .host
        .tree()
        .visible_position(h.host.selected().unwrap())
        .unwrap();
    assert!(first > 1 && first < 60, "moved by a page: {first}");
    for _ in 0..10 {
        h.key(Key::PageDown, Modifiers::NONE);
    }
    assert_eq!(h.sel(), "Note 59");
    for _ in 0..10 {
        h.key(Key::PageUp, Modifiers::NONE);
    }
    assert_eq!(h.sel(), "Note 00");
}

// --- Creating ------------------------------------------------------------------------------

#[test]
fn ctrl_n_makes_a_root_note_with_the_title_ready_to_type() {
    let (_d, p) = deep("tree-newroot");
    let mut h = H::open_tree(&p);
    let checkpoints = h.host.document().unwrap().recovery().checkpoints().len();
    h.key(Key::N, CTRL);
    h.settle();
    assert_eq!(h.sel(), "New note");
    assert_eq!(h.vis().last().unwrap(), "New note");
    assert!(h.has_focus(Focus::Title), "the title has the focus");
    assert_eq!(h.host.title_text(), "New note");
    assert!(h.host.is_dirty());
    assert_eq!(
        h.host.document().unwrap().recovery().checkpoints().len(),
        checkpoints,
        "creating takes no checkpoint"
    );
    // The whole title is selected, so typing replaces it; Enter commits and moves to the body.
    h.type_text("Fresh");
    h.key(Key::Enter, Modifiers::NONE);
    h.settle();
    assert_eq!(h.sel(), "Fresh");
    assert!(h.has_focus(Focus::Editor));
    h.type_text("typed");
    assert_eq!(h.body("Fresh"), "typed");
    // A second default note gets the next free name.
    h.key(Key::N, CTRL);
    h.settle();
    assert_eq!(h.sel(), "New note");
    h.key(Key::N, CTRL);
    h.settle();
    assert_eq!(h.sel(), "New note 2");
}

#[test]
fn ctrl_shift_n_makes_a_child_of_the_current_note_and_opens_the_parent() {
    let (_d, p) = deep("tree-newchild");
    let mut h = H::open_tree(&p);
    h.go("C");
    h.key(Key::N, CTRL | Modifiers::SHIFT);
    h.settle();
    assert_eq!(h.sel(), "New note");
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C(New note)"
    );
    assert!(h.expanded("C"), "the parent opens so the child is visible");
    assert!(h.vis().contains(&"New note".to_owned()));
    assert!(h.has_focus(Focus::Title));
    // With nothing selected a child cannot be made and nothing changes.
    let ctx = h.ctx.clone();
    h.host.selected = None;
    let before = shape(h.host.document().unwrap());
    h.host.create_child(&ctx);
    assert_eq!(shape(h.host.document().unwrap()), before);
}

// --- Renaming ------------------------------------------------------------------------------

#[test]
fn f2_renames_and_every_way_of_finishing_commits() {
    let (_d, p) = deep("tree-rename");
    let mut h = H::open_tree(&p);
    h.key(Key::F2, Modifiers::NONE);
    h.settle();
    assert!(h.has_focus(Focus::Title), "F2 focuses the title");
    h.type_text("Alpha");
    assert!(h.host.title_pending());
    assert_eq!(h.vis()[0], "A", "not written yet");
    h.key(Key::Enter, Modifiers::NONE);
    h.settle();
    assert_eq!(h.vis()[0], "Alpha");
    assert!(h.host.is_dirty());
    assert!(h.has_focus(Focus::Editor), "Enter moves on to the body");
    // Escape commits too and returns to the tree; the text is not reverted.
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("Alpha2");
    h.key(Key::Escape, Modifiers::NONE);
    h.settle();
    assert_eq!(h.vis()[0], "Alpha2");
    assert!(h.has_focus(Focus::Tree));
    // Leaving the field (here by moving the selection) commits against its own note.
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("Alpha3");
    h.go("B");
    assert_eq!(h.vis()[0], "Alpha3");
    assert_eq!(h.sel(), "B");
    // Surrounding blanks are trimmed; an unchanged title is not a change.
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("  Bee  ");
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.sel(), "Bee");
}

#[test]
fn a_refused_title_keeps_the_text_blocks_the_move_and_shows_the_qt_message() {
    let (_d, p) = deep("tree-refuse");
    let mut h = H::open_tree(&p);
    h.go("B");
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("A"); // taken by a sibling at the top level
    let ctx = h.ctx.clone();
    let id_c = h.id("C");
    assert!(!h.host.select(&ctx, id_c), "the move is refused");
    h.settle();
    assert_eq!(h.sel(), "B");
    assert_eq!(h.host.title_text(), "A", "the typed text is still there");
    assert_eq!(
        *h.host.dialog(),
        Dialog::Error("A note named \"A\" already exists here.".to_owned())
    );
    assert_eq!(
        h.host
            .document()
            .unwrap()
            .notebook()
            .get(h.id("B"))
            .unwrap()
            .title(),
        "B"
    );
    h.press(Key::Enter); // OK
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(
        h.has_focus(Focus::Title),
        "focus returns to the field to be corrected"
    );
    // The same text is refused when creating a note, and again when saving.
    let notes = h.host.rows().len();
    h.key(Key::N, CTRL);
    assert_eq!(
        h.host.rows().len(),
        notes,
        "Ctrl+N is refused while the title is invalid"
    );
    assert!(matches!(h.host.dialog(), Dialog::Error(_)));
    h.press(Key::Escape);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    // An empty title is refused too.
    h.host.title_text.clear();
    assert!(!h.host.select(&ctx, id_c));
    assert_eq!(
        *h.host.dialog(),
        Dialog::Error("A note needs a name.".to_owned())
    );
    h.press(Key::Enter);
    h.settle();
    // Corrected, it commits and the move works.
    h.host.title_text = "Beta".to_owned();
    assert!(h.host.select(&ctx, id_c));
    assert_eq!(h.sel(), "C");
    assert!(h.vis().contains(&"Beta".to_owned()));
}

#[test]
fn a_pending_title_is_part_of_save_and_close_and_an_invalid_one_stops_them() {
    let (_d, p) = deep("tree-titlesave");
    let mut h = H::open_tree(&p);
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("Omega");
    h.key(Key::S, CTRL);
    assert_eq!(h.host.status(), "Saved");
    assert_eq!(
        snapshot(&p)[0].3,
        "Omega",
        "the title typed so far reached the file"
    );
    // Closing with a title still typed: committed first, then the dirty question.
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("Omega2");
    assert!(!h.request_close());
    assert!(matches!(h.host.dialog(), Dialog::Unsaved { .. }));
    assert_eq!(h.vis()[0], "Omega2", "committed before asking");
    h.host.cancel_dialog();
    h.settle();
    // An invalid title keeps the window.
    h.host.focus(Focus::Title);
    h.settle();
    h.host.title_text = "B".to_owned();
    assert!(!h.request_close());
    assert!(matches!(h.host.dialog(), Dialog::Error(_)));
    assert!(!h.closed);
}

// --- Moving --------------------------------------------------------------------------------

#[test]
fn alt_up_and_down_move_a_note_among_its_siblings_with_its_subtree() {
    let (_d, p) = deep("tree-move");
    let mut h = H::open_tree(&p);
    let ctx = h.ctx.clone();
    h.host.set_subtree_expanded(&ctx, h.id("A"), true);
    let checkpoints = h.host.document().unwrap().recovery().checkpoints().len();
    h.go("A1");
    h.key(Key::ArrowDown, Modifiers::ALT);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A2,A1(A1a(Leaf)))|B(B1)|C"
    );
    assert_eq!(h.sel(), "A1", "the moved note stays selected");
    assert!(h.expanded("A1a"), "its open state moved with it");
    assert!(h.host.is_dirty());
    assert_eq!(
        h.host.document().unwrap().recovery().checkpoints().len(),
        checkpoints + 1,
        "one checkpoint per real move"
    );
    h.key(Key::ArrowDown, Modifiers::ALT);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A2,A1(A1a(Leaf)))|B(B1)|C",
        "already last: nothing happens"
    );
    assert_eq!(
        h.host.document().unwrap().recovery().checkpoints().len(),
        checkpoints + 1
    );
    h.key(Key::ArrowUp, Modifiers::ALT);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
    h.go("A");
    h.key(Key::ArrowUp, Modifiers::ALT);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C",
        "first: no move"
    );
    // Top level moves too, from the editor as well (the shortcut is application-wide).
    h.go("C");
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::ArrowUp, Modifiers::ALT);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|C|B(B1)"
    );
    assert_eq!(h.sel(), "C");
    assert_eq!(h.host.buffer(), "c body\n");
}

#[test]
fn a_moved_note_keeps_its_body_and_the_move_survives_save_and_reopen() {
    let (_d, p) = deep("tree-move-save");
    let mut h = H::open_tree(&p);
    h.go("B");
    h.key(Key::ArrowUp, Modifiers::ALT);
    h.key(Key::S, CTRL);
    drop(h);
    let h2 = H::open_tree(&p);
    assert_eq!(
        shape(h2.host.document().unwrap()),
        "B(B1)|A(A1(A1a(Leaf)),A2)|C"
    );
    assert_eq!(h2.body("B"), "b body\n");
    assert_eq!(h2.body("A"), "a body\n");
}

// --- Deleting ------------------------------------------------------------------------------

#[test]
fn delete_asks_first_and_escape_or_no_changes_nothing() {
    let (_d, p) = deep("tree-delask");
    let mut h = H::open_tree(&p);
    h.go("B");
    h.key(Key::Delete, Modifiers::NONE);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDelete(h.id("B")));
    h.press(Key::Escape);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C"
    );
    assert!(!h.host.is_dirty());
    assert!(h.trash_titles().is_empty());
    assert!(h.has_focus(Focus::Tree), "focus is back where it was");
    // The default answer is No: Enter declines.
    h.key(Key::Delete, Modifiers::NONE);
    h.settle();
    h.press(Key::Enter);
    h.settle();
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.trash_titles().is_empty());
}

#[test]
fn deleting_a_child_goes_to_trash_and_selects_its_parent() {
    let (_d, p) = deep("tree-delchild");
    let mut h = H::open_tree(&p);
    h.go("A2");
    h.key(Key::Delete, Modifiers::NONE);
    h.settle();
    h.press(Key::Tab); // from "No" to "Yes"
    h.press(Key::Enter);
    h.settle();
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)))|B(B1)|C"
    );
    assert_eq!(h.trash_titles(), ["A2"]);
    assert_eq!(h.sel(), "A", "the parent is selected");
    assert!(h.has_focus(Focus::Tree));
    assert!(h.host.is_dirty());
    assert_eq!(h.host.buffer(), "a body\n");
    assert!(h.host.status().contains("Trash"));
}

#[test]
fn deleting_a_subtree_takes_everything_below_and_a_top_level_note_selects_a_neighbour() {
    let (_d, p) = deep("tree-delroot");
    let mut h = H::open_tree(&p);
    let checkpoints = h.host.document().unwrap().recovery().checkpoints().len();
    h.go("A");
    let ctx = h.ctx.clone();
    h.host.ask_delete(&ctx);
    assert_eq!(*h.host.dialog(), Dialog::ConfirmDelete(h.id("A")));
    let id = h.id("A");
    h.host.confirm_delete(&ctx, id);
    h.settle();
    assert_eq!(shape(h.host.document().unwrap()), "B(B1)|C");
    let trash = h.host.document().unwrap().recovery().trash();
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].node_count(), 5, "A with all four notes beneath it");
    assert_eq!(
        h.host.document().unwrap().recovery().checkpoints().len(),
        checkpoints + 1,
        "one checkpoint before the delete"
    );
    assert_eq!(
        h.sel(),
        "B",
        "a top-level note is replaced by its next neighbour"
    );
    // Last top-level note: the previous one.
    h.go("C");
    let id = h.id("C");
    h.host.confirm_delete(&ctx, id);
    assert_eq!(h.sel(), "B");
    // Deleting everything leaves an empty tree and an empty editor.
    let id = h.id("B");
    h.host.confirm_delete(&ctx, id);
    h.settle();
    assert!(h.host.selected().is_none() && h.host.rows().is_empty());
    assert_eq!(h.host.buffer(), "");
}

#[test]
fn deleting_flushes_the_editor_first_and_forgets_the_state_of_every_deleted_note() {
    let (_d, p) = deep("tree-delstate");
    let mut h = H::open_tree(&p);
    let ctx = h.ctx.clone();
    h.host.set_subtree_expanded(&ctx, h.id("A"), true);
    // Give two notes of the subtree caret and undo state, then edit A's own text unsaved.
    h.go("Leaf");
    h.host.focus(Focus::Editor);
    h.settle();
    h.type_text("L1");
    let leaf_key = h.key_of("Leaf");
    assert!(egui::TextEdit::load_state(&h.ctx, leaf_key.widget_id()).is_some());
    h.go("A");
    h.host.focus(Focus::Editor);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("latest");
    let a_key = h.key_of("A");
    // The buffer is ahead of nothing here (write-through), but force the worst case anyway.
    h.host.text.push('!');
    h.host.ask_delete(&ctx);
    let id = h.id("A");
    h.host.confirm_delete(&ctx, id);
    h.settle();
    let trash = h.host.document().unwrap().recovery().trash();
    assert_eq!(
        trash[0].root().body(),
        "a body\nlatest!",
        "Trash holds the latest text"
    );
    for k in [a_key, leaf_key] {
        assert!(
            egui::TextEdit::load_state(&h.ctx, k.widget_id()).is_none(),
            "caret and undo of a deleted note are gone"
        );
    }
    // Ctrl+Z after the delete can only touch what is shown now, and nothing is left to undo.
    let shown = h.host.buffer().to_owned();
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::Z, CTRL);
    assert_eq!(h.host.buffer(), shown);
    assert_eq!(shape(h.host.document().unwrap()), "B(B1)|C");
}

#[test]
fn ctrl_z_after_moving_between_notes_by_keyboard_only_undoes_the_note_shown() {
    let (_d, p) = deep("tree-undo");
    let mut h = H::open_tree(&p);
    h.key(Key::Enter, Modifiers::NONE);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("one");
    h.key(Key::Escape, Modifiers::NONE);
    h.settle();
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert_eq!(h.sel(), "B");
    h.key(Key::Enter, Modifiers::NONE);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("two");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("B"), "b body\n", "undo in B");
    assert_eq!(h.body("A"), "a body\none", "A is untouched by it");
    h.key(Key::Escape, Modifiers::NONE);
    h.settle();
    h.key(Key::ArrowUp, Modifiers::NONE);
    h.key(Key::Enter, Modifiers::NONE);
    h.settle();
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("A"), "a body\n");
    assert_eq!(h.body("B"), "b body\n");
}

#[test]
fn the_context_menu_opens_from_the_keyboard_and_escape_closes_it() {
    let (_d, p) = deep("tree-menu");
    let mut h = H::open_tree(&p);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    let layers = |h: &H| h.ctx.memory(|m| m.areas().visible_layer_ids().len());
    let base = layers(&h);
    h.key(Key::F10, Modifiers::SHIFT);
    h.settle();
    assert!(
        egui::Popup::is_any_open(&h.ctx),
        "Shift+F10 opens the note's menu"
    );
    assert_eq!(
        layers(&h),
        base + 1,
        "and it is actually drawn (an overlay layer exists)"
    );
    h.press(Key::Escape);
    h.settle();
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(layers(&h), base, "the overlay is gone");
    assert_eq!(h.sel(), "A");
    assert!(!h.host.is_dirty(), "none of it touched the document");
}

#[test]
fn creating_renaming_and_moving_autosave_like_any_content_change() {
    let (_d, p) = deep("tree-autosave");
    let mut h = H::open_tree(&p);
    h.key(Key::N, CTRL);
    h.settle();
    h.type_text("Fresh");
    h.key(Key::Enter, Modifiers::NONE);
    h.advance(1.5);
    assert!(!h.host.is_dirty(), "autosaved");
    assert_eq!(
        shape(&Document::open_existing(&p).unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|C|Fresh"
    );
    h.go("Fresh");
    h.key(Key::ArrowUp, Modifiers::ALT);
    h.advance(1.5);
    assert_eq!(
        shape(&Document::open_existing(&p).unwrap()),
        "A(A1(A1a(Leaf)),A2)|B(B1)|Fresh|C"
    );
}

#[test]
fn the_tree_scales_to_a_large_notebook() {
    let d = dir("tree-large");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    for i in 0..300 {
        let r = doc.create_root(&format!("Root {i:03}")).unwrap();
        for j in 0..20 {
            doc.create_child(r, &format!("Child {i:03}-{j:02}"))
                .unwrap();
        }
    }
    doc.save().unwrap();
    let mut h = H::open_tree(&p);
    assert_eq!(h.host.rows().len(), 6300);
    assert_eq!(h.host.tree().len_visible(), 300);
    let ctx = h.ctx.clone();
    h.host.set_all_expanded(&ctx, true);
    assert_eq!(h.host.tree().len_visible(), 6300);
    // An idle frame draws only what is in view.
    let t = std::time::Instant::now();
    for _ in 0..20 {
        h.frame(vec![], Modifiers::NONE);
    }
    let per_frame = t.elapsed() / 20;
    eprintln!("idle frame with 6,300 notes: {per_frame:?}");
    assert!(
        per_frame < Duration::from_millis(250),
        "{per_frame:?} (debug build)"
    );
}

/// Writes the nested GUI-check notebook to `$OMATREE_GUI_FIXTURE` (manual use only).
#[test]
#[ignore]
fn write_deep_gui_fixture() {
    let path =
        PathBuf::from(std::env::var_os("OMATREE_GUI_FIXTURE").expect("set OMATREE_GUI_FIXTURE"));
    let _ = fs::remove_file(&path);
    make_deep(&path);
}

#[test]
fn a_pending_autosave_keeps_asking_for_a_wake_up_even_when_nothing_else_repaints() {
    // eframe replaces the previous repaint request with each frame's, so an idle app (tree
    // focused, no caret blinking) would never reach the end of the countdown otherwise. Only
    // `logic` runs here, so nothing else can be asking for a repaint.
    let (_d, p) = deep("tree-wakeup");
    let mut h = H::open_tree(&p);
    let ctx = h.ctx.clone();
    let delay = |host: &mut Host, t: f64| {
        let input = RawInput {
            time: Some(t),
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| host.logic(ui.ctx()));
        out.viewport_output[&egui::ViewportId::ROOT].repaint_delay
    };
    let _ = delay(&mut h.host, 100.0);
    h.host.edited = true;
    let first = delay(&mut h.host, 100.1);
    assert!(
        first < Duration::from_secs(2),
        "countdown started: {first:?}"
    );
    for t in [100.2, 100.5, 101.0] {
        let d = delay(&mut h.host, t);
        assert!(
            d > Duration::ZERO && d < Duration::from_secs(2),
            "asked again on every frame at t={t}: {d:?}"
        );
    }
}
