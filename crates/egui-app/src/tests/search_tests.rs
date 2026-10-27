//! Notebook-wide Search (ticket E7a). Behaviour is taken from the Qt app (`SearchPane.qml`,
//! `SearchField.qml`, `TreePane.qml`, `tests/qml/{search,searchheader}.qml`) and from
//! `omatree_core::search`; everything here goes through real frames of a `Host` on a copy of a
//! notebook: keys, clicks, focus, and what is painted.

use super::tree_tests::shape;
use super::*;

/// Top > Middle > Lower > Deepest (a body with "zebra"), then Other, a note *titled* Zebra, a
/// Czech title, an emoji title, and a very long title.
const LONG: &str = "An extraordinarily long note title that no sidebar can hold on one line";

fn make_search(path: &Path) {
    let mut doc = Document::open(path).unwrap();
    let top = doc.create_root("Top").unwrap();
    let middle = doc.create_child(top, "Middle").unwrap();
    let lower = doc.create_child(middle, "Lower").unwrap();
    let deepest = doc.create_child(lower, "Deepest").unwrap();
    let other = doc.create_root("Other").unwrap();
    let zebra = doc.create_root("Zebra crossing").unwrap();
    let czech = doc.create_root("Příliš žluťoučký").unwrap();
    let crab = doc.create_root("Crab 🦀").unwrap();
    let long = doc.create_root(LONG).unwrap();
    for (id, body) in [
        (deepest, "the quick zebra jumps"),
        (other, "unrelated"),
        (zebra, "road"),
        (czech, "Kůň úpěl ďábelské ódy"),
        (crab, "ahoj 🦀 svět"),
        (long, "short"),
    ] {
        doc.set_body(id, body).unwrap();
    }
    doc.save().unwrap();
}

fn search_nb(name: &str) -> (PathBuf, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    make_search(&p);
    (d, p)
}

impl H {
    /// Opened as the app opens it: the tree has the keyboard.
    fn open_search(name: &str) -> (PathBuf, PathBuf, H) {
        let (d, p) = search_nb(name);
        let h = H::open_tree(&p);
        (d, p, h)
    }

    fn ctrl_f(&mut self) {
        self.key(Key::F, CTRL);
    }

    /// Ctrl+F (which selects whatever the field held) and the query typed over it.
    fn search_for(&mut self, query: &str) {
        self.ctrl_f();
        self.type_text(query);
    }

    fn found(&self) -> Vec<String> {
        self.host
            .search_results()
            .iter()
            .map(|r| r.title.clone())
            .collect()
    }

    fn current_title(&self) -> String {
        let i = self.host.search_current().expect("a highlighted result");
        self.host.search_results()[i].title.clone()
    }

    fn searching_has_focus(&self) -> bool {
        self.has_focus(Focus::Search)
    }

    /// Click the painted result whose title is `title`.
    fn click_result(&mut self, title: &str) {
        let t = self.text_in(title, true);
        let at = Pos2::new(t.pos.x + 4.0, t.centre_y());
        self.press_at(at);
        self.release_at(at);
    }

    fn dialog_is_error(&self) -> bool {
        matches!(self.host.dialog(), Dialog::Error(_))
    }
}

// --- Invoking, dismissing, empty and no-result states -----------------------------------------

#[test]
fn ctrl_f_opens_search_with_the_keyboard_and_escape_dismisses_it() {
    let (_d, _p, mut h) = H::open_search("search-invoke");
    assert!(!h.host.search_open());
    h.ctrl_f();
    assert!(h.host.search_open());
    assert!(h.searching_has_focus(), "the field has the keyboard");
    assert!(!h.has_focus(Focus::Tree));
    assert!(!h.host.is_dirty(), "no `f` reached a note");
    h.type_text("zebra");
    assert_eq!(h.host.search_query(), "zebra");
    h.press(Key::Escape);
    assert!(!h.host.search_open());
    assert_eq!(h.host.search_query(), "");
    assert!(h.host.search_results().is_empty());
    assert!(h.has_focus(Focus::Tree), "focus is back on the tree");
    // From the editor too, and Escape returns to the tree rather than the editor.
    h.host.focus(Focus::Editor);
    h.settle();
    h.ctrl_f();
    assert!(h.searching_has_focus());
    h.press(Key::Escape);
    assert!(h.has_focus(Focus::Tree));
    assert!(!h.host.is_dirty());
}

#[test]
fn an_empty_query_shows_the_tree_and_never_searches() {
    let (_d, _p, mut h) = H::open_search("search-empty");
    h.ctrl_f();
    assert!(h.host.search_results().is_empty());
    assert!(h.host.search_current().is_none());
    assert_eq!(h.host.search_scans(), 0);
    h.text_in("Top", true); // the tree is still what the pane shows
    h.text_in("Other", true);
    h.type_text("  \t ");
    assert!(h.host.search_results().is_empty());
    assert_eq!(h.host.search_scans(), 0, "blank is not a query");
    h.text_in("Other", true);
    // Enter with nothing to choose changes nothing.
    h.press(Key::Enter);
    assert!(h.host.search_open());
    assert_eq!(h.sel(), "Top");
}

#[test]
fn a_query_with_no_match_says_so_and_enter_does_nothing() {
    let (_d, _p, mut h) = H::open_search("search-none");
    h.search_for("qqqq");
    assert!(h.host.search_results().is_empty());
    assert!(h.host.search_current().is_none());
    h.text_in("No matching notes", true);
    assert!(
        h.painted().texts.iter().all(|t| t.text != "Other"),
        "the tree is not shown behind it"
    );
    h.press(Key::Enter);
    h.press(Key::ArrowDown);
    assert!(h.host.search_open());
    assert_eq!(h.sel(), "Top");
    assert!(!h.host.is_dirty());
}

#[test]
fn an_empty_field_that_is_left_closes_but_one_with_a_query_stays() {
    let (_d, _p, mut h) = H::open_search("search-leave");
    h.ctrl_f();
    h.host.focus(Focus::Tree);
    h.settle();
    assert!(!h.host.search_open(), "not a persistent toolbar");
    h.search_for("zebra");
    h.host.focus(Focus::Tree);
    h.settle();
    assert!(h.host.search_open(), "a query keeps its results");
    assert_eq!(h.found().len(), 2);
}

// --- What matches and how it is shown ---------------------------------------------------------

#[test]
fn titles_match_ignoring_case_and_a_title_result_has_a_title_match() {
    let (_d, _p, mut h) = H::open_search("search-title");
    h.search_for("OTHER");
    assert_eq!(h.found(), ["Other"]);
    assert!(h.host.search_results()[0].title_match);
    // Painted: title, then the snippet (the start of the body, as the note has no match in it).
    h.text_in("Other", true);
    h.text_in("unrelated", true);
    h.search_for("oth");
    assert_eq!(h.found(), ["Other"]);
}

#[test]
fn a_body_match_shows_its_snippet_and_where_the_note_lives() {
    let (_d, _p, mut h) = H::open_search("search-body");
    h.search_for("quick");
    assert_eq!(h.found(), ["Deepest"]);
    let r = &h.host.search_results()[0];
    assert!(!r.title_match);
    assert_eq!(r.breadcrumb, "Top › Middle › Lower");
    assert!(r.snippet.contains("quick zebra jumps"));
    h.text_in("Deepest", true);
    h.text_in("Top › Middle › Lower", true);
    h.text_in("the quick zebra jumps", true);
}

#[test]
fn title_matches_come_before_body_matches_whatever_the_tree_order() {
    let (_d, _p, mut h) = H::open_search("search-order");
    h.search_for("zebra");
    // Deepest comes first in the tree, but only its body matches.
    assert_eq!(h.found(), ["Zebra crossing", "Deepest"]);
    let flags: Vec<bool> = h
        .host
        .search_results()
        .iter()
        .map(|r| r.title_match)
        .collect();
    assert_eq!(flags, [true, false]);
    assert_eq!(
        h.current_title(),
        "Zebra crossing",
        "the top result is highlighted"
    );
}

#[test]
fn czech_case_folding_and_emoji_are_matched_and_painted() {
    let (_d, _p, mut h) = H::open_search("search-unicode");
    h.search_for("ŽLUŤOUČKÝ");
    assert_eq!(h.found(), ["Příliš žluťoučký"]);
    h.search_for("příliš");
    assert_eq!(h.found(), ["Příliš žluťoučký"]);
    h.text_in("Příliš žluťoučký", true);
    // A body-only match in capitals against lower-case text, snippet on character boundaries.
    h.search_for("ÚPĚL");
    assert_eq!(h.found(), ["Příliš žluťoučký"]);
    assert!(!h.host.search_results()[0].title_match);
    h.text_in("Kůň úpěl ďábelské ódy", true);
    h.search_for("🦀");
    assert_eq!(h.found(), ["Crab 🦀"]);
    h.text_in("Crab 🦀", true);
    h.text_in("ahoj 🦀 svět", true);
}

#[test]
fn a_long_title_is_cut_with_an_ellipsis_and_stays_in_the_pane() {
    let (_d, _p, mut h) = H::open_search("search-long");
    h.search_for("extraordinarily");
    assert_eq!(h.found(), [LONG]);
    let split = h.tree_right();
    let t = h
        .painted()
        .texts
        .into_iter()
        .find(|t| t.pos.x < split && t.text.starts_with("An extraordinarily"))
        .expect("the title is painted");
    assert!(t.elided);
    assert!(t.right() <= split + 0.5, "{} > {split}", t.right());
}

#[test]
fn a_path_too_long_for_the_pane_keeps_the_nearest_ancestors() {
    let d = dir("search-path");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut parent = doc
        .create_root("The outermost note of this notebook")
        .unwrap();
    for name in [
        "Second level of the chain",
        "Third level of the chain",
        "Fourth parent",
    ] {
        parent = doc.create_child(parent, name).unwrap();
    }
    let leaf = doc.create_child(parent, "Leaf").unwrap();
    doc.set_body(leaf, "findme").unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_tree(&p);
    h.search_for("findme");
    let split = h.tree_right();
    let path = h
        .painted()
        .texts
        .into_iter()
        .find(|t| t.pos.x < split && t.text.contains("Fourth parent"))
        .expect("the path is painted");
    assert!(!path.elided, "it was shortened to fit: {:?}", path.text);
    assert!(
        path.text.starts_with("\u{2026} \u{203a} "),
        "{:?}",
        path.text
    );
    assert!(
        path.text.ends_with("Fourth parent"),
        "the parent stays: {:?}",
        path.text
    );
    assert!(path.right() <= split);
}

#[test]
fn results_fit_a_narrow_window() {
    let (_d, p) = search_nb("search-narrow");
    let mut h = H::open_sized(&p, Vec2::new(420.0, 320.0));
    h.host.focus(Focus::Tree);
    h.settle();
    h.search_for("zebra");
    assert_eq!(h.found().len(), 2);
    let split = h.tree_right();
    for t in h.painted().texts {
        if t.pos.x < split && t.text.contains("zebra") {
            assert!(t.right() <= split + 0.5, "{:?}", t.text);
        }
    }
}

// --- Keyboard ---------------------------------------------------------------------------------

#[test]
fn up_and_down_move_the_highlight_without_moving_the_selection_and_enter_chooses() {
    let (_d, _p, mut h) = H::open_search("search-keys");
    h.search_for("zebra");
    assert_eq!(h.host.search_current(), Some(0));
    let chrome = h.chrome();
    let band = |h: &H| {
        h.painted()
            .fills
            .iter()
            .find(|f| f.fill == chrome.selected_focus && f.rect.width() > 100.0)
            .map(|f| f.rect.top())
            .expect("the highlighted result is painted")
    };
    let first = band(&h);
    h.press(Key::ArrowDown);
    assert_eq!(h.host.search_current(), Some(1));
    assert!(band(&h) > first, "the highlight moved down");
    h.press(Key::ArrowDown);
    assert_eq!(
        h.host.search_current(),
        Some(1),
        "it stops at the last result"
    );
    h.press(Key::ArrowUp);
    h.press(Key::ArrowUp);
    assert_eq!(h.host.search_current(), Some(0), "and at the first");
    assert_eq!(h.sel(), "Top", "the note selection did not move");
    assert!(
        h.searching_has_focus(),
        "the arrows did not move the keyboard"
    );
    h.press(Key::ArrowDown);
    h.press(Key::Enter);
    assert_eq!(h.sel(), "Deepest");
    assert!(!h.host.search_open(), "choosing closes Search");
    assert!(h.has_focus(Focus::Tree), "and the tree has the keyboard");
    assert_eq!(h.host.buffer(), "the quick zebra jumps");
}

#[test]
fn escape_after_choosing_nothing_leaves_the_selection_and_focus_sensible() {
    let (_d, _p, mut h) = H::open_search("search-escape");
    h.go("Other");
    h.search_for("zebra");
    h.press(Key::ArrowDown);
    h.press(Key::Escape);
    assert!(!h.host.search_open());
    assert_eq!(h.sel(), "Other");
    assert!(h.has_focus(Focus::Tree));
    h.press(Key::ArrowUp);
    assert_eq!(h.sel(), "Top", "the tree answers the arrows again");
}

#[test]
fn tab_and_shift_tab_leave_the_field_without_typing_or_closing_the_results() {
    let (_d, _p, mut h) = H::open_search("search-tab");
    h.search_for("zebra");
    h.press(Key::Tab);
    assert!(!h.searching_has_focus());
    assert_eq!(h.host.search_query(), "zebra", "no tab character");
    assert!(h.host.search_open());
    assert_eq!(h.found().len(), 2);
    h.key(Key::Tab, Modifiers::SHIFT);
    h.key(Key::Tab, Modifiers::SHIFT);
    // Shift+Tab walks backwards through the focusable widgets; Ctrl+F always comes back.
    h.ctrl_f();
    assert!(h.searching_has_focus());
}

#[test]
fn keys_belong_to_the_editor_when_the_editor_has_them() {
    let (_d, _p, mut h) = H::open_search("search-editor-keys");
    h.go("Other");
    h.search_for("zebra");
    h.host.focus(Focus::Editor);
    h.settle();
    assert!(h.has_focus(Focus::Editor));
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.press(Key::ArrowDown);
    h.press(Key::Enter);
    h.type_text("x");
    assert_eq!(
        h.sel(),
        "Other",
        "Enter in the editor does not choose a result"
    );
    assert_eq!(h.host.buffer(), "unrelated\nx", "it is a new line");
    assert!(h.host.search_open());
    // Escape from the editor goes back towards the tree. While results replace the tree that is
    // the results' own field, so the arrows work on them at once.
    h.press(Key::Escape);
    assert!(h.searching_has_focus());
    assert!(h.host.search_open());
    assert_eq!(
        h.host.search_query(),
        "zebra",
        "the query was not selected away"
    );
    h.press(Key::Enter);
    assert_eq!(h.sel(), "Zebra crossing");
}

#[test]
fn ctrl_f_does_nothing_behind_a_dialog_or_during_a_drag() {
    let (_d, _p, mut h) = H::open_search("search-modal");
    h.go("Other");
    let ctx = h.ctx.clone();
    h.host.ask_delete(&ctx);
    assert!(matches!(h.host.dialog(), Dialog::ConfirmDelete(_)));
    h.ctrl_f();
    assert!(!h.host.search_open());
    assert!(matches!(h.host.dialog(), Dialog::ConfirmDelete(_)));
    h.press(Key::Escape);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert!(h.has_focus(Focus::Tree));
    // Tree keys still work once it is closed again.
    h.press(Key::ArrowDown);
    assert_eq!(h.sel(), "Zebra crossing");
}

#[test]
fn a_drag_cannot_be_hijacked_by_ctrl_f() {
    let (_d, _p, mut h) = H::open_search("search-drag");
    h.expand_all();
    let to = h.at("Other", 0.5);
    h.pick_up("Zebra crossing", to);
    assert!(h.host.drag.is_some());
    h.ctrl_f();
    assert!(!h.host.search_open(), "Search waits for the drop");
    h.release_at(to);
}

// --- Choosing a result --------------------------------------------------------------------------

#[test]
fn a_result_inside_a_collapsed_branch_is_revealed_selected_and_shown() {
    // tests/qml/search.qml, step by step.
    let (_d, p, mut h) = H::open_search("search-reveal");
    h.collapse_all();
    assert_eq!(
        h.vis(),
        [
            "Top",
            "Other",
            "Zebra crossing",
            "Příliš žluťoučký",
            "Crab 🦀",
            LONG
        ]
    );
    h.search_for("zebra");
    h.press(Key::ArrowDown); // Deepest (body match)
    assert_eq!(h.current_title(), "Deepest");
    assert!(
        h.host.search_results()[1].breadcrumb.contains("Lower"),
        "the path says where it lives"
    );
    h.press(Key::Enter);
    assert!(!h.host.search_open() && h.host.search_query().is_empty());
    assert_eq!(h.sel(), "Deepest");
    assert!(h.expanded("Top") && h.expanded("Middle") && h.expanded("Lower"));
    assert!(h.vis().contains(&"Deepest".to_owned()));
    assert_eq!(
        h.host.buffer(),
        "the quick zebra jumps",
        "the editor shows it"
    );
    assert_eq!(h.host.title_text(), "Deepest");
    assert!(!h.host.is_dirty(), "searching changed nothing");
    h.advance(2.0);
    assert!(!h.host.is_dirty());
    assert_eq!(snapshot(&p).len(), 9, "and nothing was written");
    // The row is on screen, not merely open.
    let row = h.text_in("Deepest", true);
    let view = h.host.tree_geom.1;
    assert!(view.contains(Pos2::new(row.pos.x + 2.0, row.centre_y())));
}

#[test]
fn a_deeply_nested_note_is_selected_and_scrolled_into_view() {
    let d = dir("search-deep");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    for i in 0..60 {
        doc.create_root(&format!("filler {i}")).unwrap();
    }
    let mut parent = doc.create_root("chain 0").unwrap();
    for level in 1..=8 {
        parent = doc.create_child(parent, &format!("chain {level}")).unwrap();
    }
    let needle = doc.create_child(parent, "Needle").unwrap();
    doc.set_body(needle, "hidden at the bottom").unwrap();
    for i in 0..40 {
        doc.create_root(&format!("after {i}")).unwrap();
    }
    doc.set_all_expanded(false);
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_tree(&p);
    h.collapse_all();
    h.search_for("needle");
    assert_eq!(h.found(), ["Needle"]);
    assert_eq!(
        h.host.search_results()[0].breadcrumb.matches('›').count(),
        8,
        "nine ancestors"
    );
    h.press(Key::Enter);
    h.advance(1.0); // egui scrolls smoothly
    assert_eq!(h.sel(), "Needle");
    assert_eq!(h.host.tree().row(h.id("Needle")).unwrap().depth, 9);
    let row = h.text_in("Needle", true);
    assert!(h
        .host
        .tree_geom
        .1
        .contains(Pos2::new(row.pos.x + 2.0, row.centre_y())));
    assert!(!h.host.is_dirty());
}

#[test]
fn clicking_a_result_chooses_it_like_enter() {
    let (_d, _p, mut h) = H::open_search("search-click");
    h.collapse_all();
    h.search_for("zebra");
    h.click_result("Deepest");
    assert_eq!(h.sel(), "Deepest");
    assert!(!h.host.search_open());
    assert!(h.has_focus(Focus::Tree));
    assert!(h.expanded("Lower"));
    assert!(!h.host.is_dirty());
}

#[test]
fn hovering_a_result_does_not_change_the_highlight_or_the_notebook() {
    let (_d, _p, mut h) = H::open_search("search-hover");
    h.search_for("zebra");
    let t = h.text_in("Deepest", true);
    h.moved(Pos2::new(t.pos.x + 4.0, t.centre_y()));
    assert_eq!(h.host.search_current(), Some(0));
    assert_eq!(h.sel(), "Top");
    assert!(h.host.search_open());
}

#[test]
fn choosing_a_note_keeps_each_notes_caret_and_undo_apart() {
    let (_d, _p, mut h) = H::open_search("search-state");
    h.select("Other");
    h.set_caret(3, 8);
    h.type_text("P1");
    let first_caret = h.caret("Other");
    let first_text = h.body("Other");
    h.select("Zebra crossing");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("I1");
    let second_caret = h.caret("Zebra crossing");
    assert_eq!(h.body("Zebra crossing"), "roadI1");

    // Away and back through Search.
    h.host.focus(Focus::Tree);
    h.settle();
    h.search_for("unrP1");
    assert_eq!(h.found(), ["Other"], "the edited text is what is found");
    h.press(Key::Enter);
    assert_eq!(h.sel(), "Other");
    assert_eq!(h.host.buffer(), first_text);
    assert_eq!(
        h.caret("Other"),
        first_caret,
        "its own caret, not the other note's"
    );
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::Z, CTRL);
    assert_ne!(h.body("Other"), first_text, "its own undo step");
    assert_eq!(
        h.body("Zebra crossing"),
        "roadI1",
        "nothing else was undone"
    );
    h.host.focus(Focus::Tree);
    h.settle();
    h.search_for("roadi1");
    assert_eq!(h.found(), ["Zebra crossing"]);
    h.press(Key::Enter);
    assert_eq!(h.sel(), "Zebra crossing");
    assert_eq!(h.caret("Zebra crossing"), second_caret);
    assert_eq!(h.body("Zebra crossing"), "roadI1");
}

// --- Live synchronisation -----------------------------------------------------------------------

#[test]
fn unsaved_edits_are_searchable_and_search_follows_further_typing() {
    let (_d, p, mut h) = H::open_search("search-unsaved");
    h.dirty_edit("Other", " xyzzy");
    assert!(h.host.is_dirty());
    h.host.focus(Focus::Tree);
    h.settle();
    h.search_for("xyzzy");
    assert_eq!(h.found(), ["Other"]);
    assert!(
        !snapshot(&p).iter().any(|r| r.4.contains("xyzzy")),
        "still only in memory"
    );
    // Typing in the note while the results are up refreshes them, in debounced scans.
    h.host.focus(Focus::Editor);
    h.settle();
    let before = h.host.search_scans();
    h.type_text(" quagga");
    h.type_text("s");
    h.advance(0.3);
    assert!(
        h.host.search_scans() > before,
        "the edit refreshed the results"
    );
    assert!(h.host.search_scans() <= before + 2);
    assert_eq!(h.found(), ["Other"]);
    h.search_for("quaggas");
    assert_eq!(h.found(), ["Other"], "the text typed a moment ago");
    // And text removed disappears from the results.
    h.host.focus(Focus::Editor);
    h.settle();
    let end = h.host.buffer().len();
    h.set_caret(end - 1, end);
    h.press(Key::Backspace);
    h.advance(0.3);
    assert!(h.found().is_empty(), "no stale body is kept");
}

#[test]
fn many_keystrokes_in_a_note_cause_few_scans_and_an_idle_search_causes_none() {
    let (_d, _p, mut h) = H::open_search("search-scans");
    h.search_for("unrelated");
    assert_eq!(h.found(), ["Other"]);
    let scans = h.host.search_scans();
    for _ in 0..60 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert_eq!(h.host.search_scans(), scans, "idle frames do not search");
    h.go("Other");
    h.search_for("unrelated");
    let scans = h.host.search_scans();
    h.host.focus(Focus::Editor);
    h.settle();
    for _ in 0..10 {
        h.type_text("a");
    }
    h.advance(0.5);
    let made = h.host.search_scans() - scans;
    assert!(
        (1..=6).contains(&made),
        "{made} scans for ten keystrokes over half a second"
    );
}

#[test]
fn a_slow_notebook_waits_for_a_pause_in_the_typing_and_enter_never_chooses_a_stale_list() {
    let (_d, _p, mut h) = H::open_search("search-slow");
    h.ctrl_f();
    h.host.search.last_scan = 1.0; // as if the last scan had taken a second
    let scans = h.host.search_scans();
    for ch in ["z", "e", "b"] {
        h.frame(vec![Event::Text(ch.into())], Modifiers::NONE);
    }
    assert_eq!(h.host.search_scans(), scans, "nothing is scanned mid-word");
    h.advance(0.3);
    assert_eq!(h.host.search_scans(), scans + 1, "one scan after the pause");
    assert_eq!(h.found(), ["Zebra crossing", "Deepest"]);
    // A query that has not been searched yet is searched before Enter chooses.
    h.host.search.last_scan = 1.0;
    h.frame(vec![Event::Text("r".into())], Modifiers::NONE); // "zebr"
    let enter = Event::Key {
        key: Key::Enter,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: Modifiers::NONE,
    };
    h.frame(vec![Event::Text("a".into()), enter], Modifiers::NONE);
    assert_eq!(
        h.sel(),
        "Zebra crossing",
        "chosen from the results for \"zebra\""
    );
}

#[test]
fn renaming_a_note_updates_the_results() {
    let (_d, _p, mut h) = H::open_search("search-rename");
    h.search_for("zebra");
    assert_eq!(h.found(), ["Zebra crossing", "Deepest"]);
    h.host.select(&h.ctx.clone(), h.id("Zebra crossing"));
    h.press(Key::F2);
    assert!(h.has_focus(Focus::Title));
    h.type_text("Quagga");
    h.press(Key::Enter);
    h.advance(0.2);
    assert_eq!(h.found(), ["Deepest"], "the renamed note no longer matches");
    h.search_for("quagga");
    assert_eq!(h.found(), ["Quagga"]);
    assert!(h.host.search_open());
}

#[test]
fn creating_and_deleting_notes_update_the_results() {
    let (_d, _p, mut h) = H::open_search("search-create-delete");
    h.search_for("new note");
    assert!(h.found().is_empty());
    // Ctrl+N works with the results up; the new note is found at once.
    h.key(Key::N, CTRL);
    h.press(Key::Escape); // title -> tree
    h.advance(0.2);
    assert_eq!(h.found(), ["New note"]);
    // Delete it (through the question) and it is gone again.
    h.search_for("zebra");
    assert_eq!(h.found().len(), 2);
    let ctx = h.ctx.clone();
    let id = h.id("Zebra crossing");
    h.host.select(&ctx, id);
    h.host.confirm_delete(&ctx, id);
    h.advance(0.2);
    assert_eq!(h.found(), ["Deepest"]);
    assert_eq!(h.host.search_current(), Some(0));
}

#[test]
fn moving_and_reordering_keep_each_result_attached_to_its_own_note() {
    let d = dir("search-move");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let b = doc.create_root("b item").unwrap();
    let a = doc.create_root("a item").unwrap();
    let b_child = doc.create_child(b, "z item").unwrap();
    doc.create_child(a, "y item").unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_tree(&p);
    h.expand_all();
    h.search_for("item");
    let ids = |h: &H| -> Vec<(String, NodeId)> {
        h.host
            .search_results()
            .iter()
            .map(|r| (r.title.clone(), r.id))
            .collect()
    };
    let before = ids(&h);
    assert_eq!(
        before.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
        ["b item", "z item", "a item", "y item"]
    );
    // Move "a item" above "b item": the order follows the tree, the identities stay.
    let ctx = h.ctx.clone();
    h.host.select(&ctx, a);
    h.host.move_selected(&ctx, -1);
    h.advance(0.2);
    let after = ids(&h);
    assert_eq!(
        after.iter().map(|(t, _)| t.as_str()).collect::<Vec<_>>(),
        ["a item", "y item", "b item", "z item"]
    );
    for (title, id) in &after {
        let was = before.iter().find(|(t, _)| t == title).unwrap().1;
        assert_eq!(*id, was, "{title} kept its id");
    }
    // Reparent: "z item" under "a item".
    h.host.drop_note(&ctx, b_child, Some(a), 0);
    h.advance(0.2);
    let moved = h
        .host
        .search_results()
        .iter()
        .find(|r| r.id == b_child)
        .expect("still a result");
    assert_eq!(moved.breadcrumb, "a item");
    assert_eq!(
        shape(h.host.document().unwrap()),
        "a item(z item,y item)|b item"
    );
}

#[test]
fn a_result_whose_note_has_gone_selects_nothing_and_refreshes_the_list() {
    // tests/qml/search.qml: "a stale result gives an invalid index".
    let (_d, _p, mut h) = H::open_search("search-stale");
    h.search_for("zebra");
    assert_eq!(h.found(), ["Zebra crossing", "Deepest"]);
    let ctx = h.ctx.clone();
    let doomed = h.id("Deepest");
    h.host.confirm_delete(&ctx, doomed); // the list has not been refreshed yet
    assert_eq!(h.found().len(), 2, "the list still names the deleted note");
    let sel = h.host.selected();
    h.host.activate_result(&ctx, 1);
    assert_eq!(h.host.selected(), sel, "nothing was selected");
    assert!(h.host.search_open(), "Search stays open");
    h.advance(0.2);
    assert_eq!(h.found(), ["Zebra crossing"]);
    // An index past the end is ignored too.
    h.host.activate_result(&ctx, 7);
    assert_eq!(h.host.selected(), sel);
    assert!(!h.host.is_dirty() || h.host.document().unwrap().recovery().trash().len() == 1);
}

#[test]
fn an_invalid_pending_title_blocks_navigation_and_is_not_discarded() {
    let (_d, _p, mut h) = H::open_search("search-invalid-title");
    h.go("Other");
    h.press(Key::F2);
    assert!(h.has_focus(Focus::Title));
    h.press(Key::Backspace); // the selected title is deleted: empty, refused
    assert_eq!(h.host.title_text(), "");
    // Reaching for Search commits (and is refused) like any other leaving of the title.
    h.ctrl_f();
    assert!(h.dialog_is_error(), "the refusal is explained");
    h.press(Key::Enter);
    assert_eq!(*h.host.dialog(), Dialog::None);
    assert_eq!(
        h.host.title_text(),
        "",
        "the typed text is still there to fix"
    );
    assert_eq!(h.sel(), "Other");
    // Typing a query and choosing a result must not leave the note either.
    if !h.searching_has_focus() {
        h.ctrl_f();
    }
    h.type_text("zebra");
    assert_eq!(h.found().len(), 2);
    h.press(Key::Enter);
    assert!(h.dialog_is_error(), "navigation was refused");
    assert_eq!(h.sel(), "Other");
    assert!(h.host.search_open(), "the results are still there");
    assert_eq!(h.host.title_text(), "");
    h.press(Key::Enter); // dismiss
                         // Fixing the title unblocks it.
    h.host.set_focus(Focus::Title);
    h.settle();
    h.type_text("Other");
    h.press(Key::Enter);
    assert_eq!(h.host.title_text(), "Other");
    assert_eq!(h.sel(), "Other");
}

#[test]
fn a_mouse_choice_with_an_invalid_pending_title_is_refused_too() {
    let (_d, _p, mut h) = H::open_search("search-invalid-click");
    h.go("Other");
    h.search_for("zebra");
    // The title is edited behind the open results (as a click into it would do).
    h.host.set_focus(Focus::Title);
    h.settle();
    h.press(Key::Backspace);
    assert_eq!(h.host.title_text(), "");
    let ctx = h.ctx.clone();
    h.host.activate_result(&ctx, 0);
    assert!(h.dialog_is_error());
    assert_eq!(h.sel(), "Other");
    assert!(h.host.search_open());
}

// --- Nothing about Search is content ------------------------------------------------------------

#[test]
fn searching_and_choosing_never_dirty_or_write_the_notebook() {
    let (_d, p, mut h) = H::open_search("search-inert");
    let before = snapshot(&p);
    let checkpoints = h.host.document().unwrap().recovery().checkpoints().len();
    h.collapse_all();
    for q in ["zebra", "Ž", "🦀", "qqqq", "e"] {
        h.search_for(q);
        h.press(Key::ArrowDown);
        h.press(Key::ArrowUp);
    }
    h.search_for("quick");
    h.press(Key::Enter);
    h.advance(3.0);
    assert!(!h.host.is_dirty());
    assert!(!h.host.edited);
    assert_eq!(snapshot(&p), before);
    assert_eq!(
        h.host.document().unwrap().recovery().checkpoints().len(),
        checkpoints
    );
    assert!(h.host.document().unwrap().recovery().trash().is_empty());
}

#[test]
fn opening_another_notebook_drops_the_query_and_every_result() {
    let (d, _p, mut h) = H::open_search("search-replace");
    h.search_for("zebra");
    assert_eq!(h.found().len(), 2);
    let other = d.join("other.omatree");
    make_fixture(&other);
    assert!(h.host.open(&h.ctx.clone(), &other));
    h.settle();
    assert!(!h.host.search_open());
    assert_eq!(h.host.search_query(), "");
    assert!(h.host.search_results().is_empty());
    h.search_for("zebra");
    assert!(h.host.search_results().is_empty(), "this notebook has none");
}

// --- The rest of the keyboard still works -----------------------------------------------------

#[test]
fn tree_navigation_and_the_menu_are_unchanged_once_search_has_been_used() {
    let (_d, _p, mut h) = H::open_search("search-regress");
    h.search_for("zebra");
    h.press(Key::Escape);
    h.press(Key::ArrowDown);
    assert_eq!(h.sel(), "Other");
    h.press(Key::ArrowRight);
    h.press(Key::ArrowDown);
    assert_eq!(h.sel(), "Zebra crossing");
    h.key(Key::F10, Modifiers::SHIFT);
    assert!(
        egui::Popup::is_any_open(&h.ctx),
        "Shift+F10 still opens the menu"
    );
    h.press(Key::Escape);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    h.press(Key::Enter);
    assert!(h.has_focus(Focus::Editor), "Enter still edits the body");
    h.press(Key::Escape);
    assert!(h.has_focus(Focus::Tree));
}

#[test]
fn results_stay_legible_in_every_theme_and_follow_a_theme_change() {
    use super::theme_tests::{install, LATTE, TOKYO};
    use crate::theme::{MUTED_MIN, TEXT_MIN};
    use omatree_editor::theme::contrast;
    let (d, _p, mut h) = H::open_search("search-colours");
    let low = "mode = \"dark\"\naccent = \"#8a8f98\"\nselection = \"#3d4046\"\n\
muted = \"#3a3c40\"\nbackground = \"#2a2c30\"\ndark_background = \"#24262a\"\n\
lighter_background = \"#32353a\"\nforeground = \"#80848c\"\ndark_foreground = \"#5c6068\"\n\
bright_foreground = \"#9a9ea6\"\nred = \"#8c6468\"\ngreen = \"#6c8a6c\"\nmagenta = \"#8a7a9a\"\n\
yellow = \"#9a8a68\"\n";
    h.search_for("zebra");
    // Default theme, then three Omarchy fixtures swapped in while the results are open.
    for fixture in [None, Some(TOKYO), Some(LATTE), Some(low)] {
        if let Some(text) = fixture {
            let colours = install(&d, text);
            h.follow(&colours);
            h.polls(2);
        }
        assert!(h.host.search_open());
        assert_eq!(h.found().len(), 2, "the results survive a theme change");
        let ch = h.chrome();
        let surface = crate::theme::colour(h.host.theme_palette().surface);
        let texts = h.painted().texts;
        let find = |s: &str| {
            texts
                .iter()
                .find(|t| t.text == s && t.pos.x < h.tree_right())
                .unwrap_or_else(|| panic!("no painted {s:?}"))
                .clone()
        };
        // The highlighted result's title sits on the highlight, the others on the pane.
        assert!(contrast(find("Zebra crossing").colour, ch.selected_focus) >= TEXT_MIN);
        assert!(contrast(find("Deepest").colour, surface) >= TEXT_MIN);
        assert!(contrast(find("Top › Middle › Lower").colour, surface) >= MUTED_MIN);
        assert!(contrast(find("the quick zebra jumps").colour, surface) >= MUTED_MIN);
    }
}

/// Release-mode numbers for the migration plan (6,300 notes, a short body each):
/// `cargo test --release -p omatree-egui -- --ignored --nocapture search_timings`
#[test]
#[ignore]
fn search_timings() {
    fn median(v: &mut [f64]) -> f64 {
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    }
    let d = dir("search-timings");
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
            let text = if i == 299 && j == 19 {
                format!("{body}needle in the last note")
            } else {
                body.clone()
            };
            doc.set_body(c, &text).unwrap();
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
    let mut idle_tree = vec![];
    for _ in 0..30 {
        idle_tree.push(ms(&mut h, &mut |h| h.frame(vec![], Modifiers::NONE)));
    }
    println!(
        "idle frame, tree, 6,300 notes: {:.2} ms",
        median(&mut idle_tree)
    );

    // The keystroke that makes a query: the frame that scans and draws.
    let mut scan_few = vec![];
    let mut scan_many = vec![];
    for _ in 0..9 {
        h.ctrl_f();
        h.host.search.last_scan = 0.0;
        scan_few.push(ms(&mut h, &mut |h| {
            h.frame(vec![Event::Text("needle".into())], Modifiers::NONE)
        }));
        h.press(Key::Escape);
        h.ctrl_f();
        h.host.search.last_scan = 0.0;
        scan_many.push(ms(&mut h, &mut |h| {
            h.frame(vec![Event::Text("child".into())], Modifiers::NONE)
        }));
        h.press(Key::Escape);
    }
    println!(
        "query frame (scan + draw): 1 result {:.2} ms, 6,000 results {:.2} ms",
        median(&mut scan_few),
        median(&mut scan_many)
    );
    h.ctrl_f();
    h.host.search.last_scan = 0.0;
    h.type_text("child");
    println!("results found: {}", h.found().len());
    let mut idle_results = vec![];
    for _ in 0..30 {
        idle_results.push(ms(&mut h, &mut |h| h.frame(vec![], Modifiers::NONE)));
    }
    println!(
        "idle frame, search open, {} results: {:.2} ms",
        h.found().len(),
        median(&mut idle_results)
    );
    let scans = h.host.search_scans();
    let mut step = vec![];
    for _ in 0..30 {
        step.push(ms(&mut h, &mut |h| {
            h.frame(
                vec![Event::Key {
                    key: Key::ArrowDown,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
                Modifiers::NONE,
            )
        }));
    }
    println!(
        "Down through the results: {:.2} ms per key; scans during idle and navigation: {}",
        median(&mut step),
        h.host.search_scans() - scans
    );
    // Editing a note while 6,000 results are showing: one scan per debounce window.
    h.host.focus(Focus::Editor);
    h.settle();
    let mut typing = vec![];
    for _ in 0..15 {
        typing.push(ms(&mut h, &mut |h| {
            h.frame(vec![Event::Text("x".into())], Modifiers::NONE)
        }));
    }
    println!(
        "typing in a note, search open on 6,000 results: median {:.2} ms, max {:.2} ms",
        median(&mut typing.clone()),
        typing.iter().cloned().fold(0.0, f64::max)
    );
    // Navigation to a result deep in the tree, from a collapsed tree.
    h.press(Key::Escape);
    h.host.stop_search();
    h.collapse_all();
    h.ctrl_f();
    h.host.search.last_scan = 0.0;
    h.type_text("needle");
    let go = ms(&mut h, &mut |h| {
        h.frame(
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            }],
            Modifiers::NONE,
        )
    });
    let next = ms(&mut h, &mut |h| h.frame(vec![], Modifiers::NONE));
    println!(
        "choose the result in the last note of a collapsed tree: {go:.2} ms for the Enter frame, {next:.2} ms for the one that shows the tree; selected {}",
        h.sel()
    );
}
