//! Mouse drag-and-drop in the tree (ticket E5b), driven by frames of pointer events against a
//! real `Document` on disposable copies. Behaviour follows `TreePane.qml` (zones, outline for a
//! refused drop) and `tests/qml/moveexpanded.qml`; auto-expand and auto-scroll have no Qt
//! counterpart and are specified in `docs/EGUI_MIGRATION_PLAN.md`.

use super::drag::{Kind, Target, Verdict};
use super::tree_tests::{deep, make_deep, shape};
use super::*;
use crate::layout::ROW_H;
use std::time::Duration;

pub(super) fn button(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

impl H {
    pub(super) fn expand_all(&mut self) {
        self.host.set_all_expanded(&self.ctx.clone(), true);
        self.settle();
    }

    /// The pointer position over visible row `title`, at `frac` of its height.
    pub(super) fn at(&self, title: &str, frac: f32) -> Pos2 {
        let (origin, _) = self.host.tree_geom;
        let row = self
            .host
            .tree()
            .visible_position(self.id(title))
            .unwrap_or_else(|| panic!("{title} is not visible"));
        Pos2::new(origin.x + 80.0, origin.y + (row as f32 + frac) * ROW_H)
    }

    pub(super) fn moved(&mut self, to: Pos2) {
        self.frame(vec![Event::PointerMoved(to)], Modifiers::NONE);
    }

    pub(super) fn press_at(&mut self, p: Pos2) {
        self.frame(
            vec![Event::PointerMoved(p), button(p, true)],
            Modifiers::NONE,
        );
    }

    pub(super) fn release_at(&mut self, p: Pos2) {
        self.frame(
            vec![Event::PointerMoved(p), button(p, false)],
            Modifiers::NONE,
        );
        self.settle();
    }

    /// Press on `title` and move to `to` in a few steps, button still down.
    pub(super) fn pick_up(&mut self, title: &str, to: Pos2) {
        let from = self.at(title, 0.5);
        self.moved(from);
        self.press_at(from);
        // Always a real movement first, even when the target is the row itself.
        let away = from + Vec2::new(24.0, 0.0);
        self.moved(away);
        for k in 1..=4 {
            self.moved(away + (to - away) * (k as f32 / 4.0));
        }
    }

    /// A whole drag and drop.
    pub(super) fn drag(&mut self, title: &str, to: Pos2) {
        self.pick_up(title, to);
        self.release_at(to);
    }

    pub(super) fn target(&self) -> Option<Target> {
        self.host.drag.as_ref().and_then(|d| d.target)
    }

    pub(super) fn shape(&self) -> String {
        shape(self.host.document().unwrap())
    }

    pub(super) fn dirty(&self) -> bool {
        self.host.is_dirty()
    }
}

pub(super) fn open_deep(name: &str) -> (crate::tests::TestDir, PathBuf, H) {
    let (d, p) = deep(name);
    let mut h = H::open_tree(&p);
    h.expand_all();
    (d, p, h)
}

pub(super) const ALL: &str = "A(A1(A1a(Leaf)),A2)|B(B1)|C";

// --- Starting a drag -------------------------------------------------------------------------

#[test]
fn a_click_selects_and_only_a_real_movement_starts_a_drag() {
    let (_d, _p, mut h) = open_deep("drag-threshold");
    let c = h.at("C", 0.5);
    // Press and release in place, and a 3 px wobble, are clicks.
    h.moved(c);
    h.press_at(c);
    h.moved(c + Vec2::new(3.0, 0.0));
    assert!(h.host.drag.is_none(), "3 px is not a drag");
    h.release_at(c);
    assert_eq!(h.sel(), "C");
    assert!(h.host.drag.is_none());
    assert_eq!(h.shape(), ALL);
    assert!(!h.dirty());
    // A long press without movement is not a drag either.
    let b = h.at("B", 0.5);
    h.moved(b);
    h.press_at(b);
    h.advance(1.5);
    assert!(h.host.drag.is_none());
    h.release_at(b);
    assert_eq!(h.shape(), ALL);
    // Moving well past the click distance is one, and does not select or toggle anything.
    let before = h.sel();
    let open_b = h.expanded("B");
    h.pick_up("A1", h.at("A2", 0.5));
    assert!(h.host.drag.is_some(), "a drag began");
    assert_eq!(h.sel(), before, "starting a drag does not select");
    assert_eq!(h.expanded("B"), open_b);
    assert_eq!(h.shape(), ALL, "nothing has changed yet");
    assert!(!h.dirty());
}

#[test]
fn a_drag_that_starts_in_the_editor_does_nothing_to_the_tree() {
    let (_d, _p, mut h) = open_deep("drag-editor");
    h.select("C");
    let (_, view) = h.host.tree_geom;
    let from = Pos2::new(view.right() + 200.0, 300.0);
    h.moved(from);
    h.press_at(from);
    for k in 1..6 {
        h.moved(from + Vec2::new(-40.0 * k as f32, 20.0));
    }
    assert!(h.host.drag.is_none());
    h.release_at(from);
    assert_eq!(h.shape(), ALL);
    assert!(!h.dirty());
}

// --- Targets and moves -----------------------------------------------------------------------

#[test]
fn before_and_after_a_sibling() {
    let (_d, _p, mut h) = open_deep("drag-beforeafter");
    h.drag("C", h.at("A", 0.1));
    assert_eq!(h.shape(), "C|A(A1(A1a(Leaf)),A2)|B(B1)");
    assert!(h.dirty());
    assert_eq!(h.sel(), "C", "the moved note is selected");
    // After a closed sibling: A closed, then B dropped just under it.
    h.host.set_expanded(&h.ctx.clone(), h.id("A"), false);
    h.settle();
    h.drag("C", h.at("A", 0.9));
    assert_eq!(h.shape(), "A(A1(A1a(Leaf)),A2)|C|B(B1)");
}

#[test]
fn just_under_an_open_parent_means_its_first_child() {
    let (_d, _p, mut h) = open_deep("drag-firstchild");
    h.drag("C", h.at("B", 0.9));
    assert_eq!(h.shape(), "A(A1(A1a(Leaf)),A2)|B(C,B1)");
}

#[test]
fn reordering_within_one_parent_both_directions() {
    let (_d, _p, mut h) = open_deep("drag-reorder");
    h.drag("A2", h.at("A1", 0.1));
    assert_eq!(h.shape(), "A(A2,A1(A1a(Leaf)))|B(B1)|C");
    h.drag("A2", h.at("Leaf", 0.9)); // after Leaf: a child of A1a's level, a real move
    assert_eq!(h.shape(), "A(A1(A1a(Leaf,A2)))|B(B1)|C");
}

#[test]
fn moving_between_parents_and_roots() {
    let (_d, _p, mut h) = open_deep("drag-parents");
    h.drag("A2", h.at("B", 0.5));
    assert_eq!(h.shape(), "A(A1(A1a(Leaf)))|B(B1,A2)|C");
    // A child to the top level (before C), and a root to the end of the top level.
    h.drag("A1", h.at("C", 0.1));
    assert_eq!(h.shape(), "A|B(B1,A2)|A1(A1a(Leaf))|C");
    let (origin, view) = h.host.tree_geom;
    let below = Pos2::new(origin.x + 80.0, view.bottom() - 4.0);
    h.drag("A", below);
    assert_eq!(h.shape(), "B(B1,A2)|A1(A1a(Leaf))|C|A");
}

#[test]
fn a_whole_subtree_moves_with_its_contents() {
    let (_d, _p, mut h) = open_deep("drag-subtree");
    let before: Vec<String> = ["A1", "A1a", "Leaf"].iter().map(|t| h.body(t)).collect();
    h.drag("A1", h.at("C", 0.5));
    assert_eq!(h.shape(), "A(A2)|B(B1)|C(A1(A1a(Leaf)))");
    let after: Vec<String> = ["A1", "A1a", "Leaf"].iter().map(|t| h.body(t)).collect();
    assert_eq!(before, after);
    assert_eq!(h.sel(), "A1");
}

// --- Refusals and no-ops ---------------------------------------------------------------------

#[test]
fn a_note_cannot_be_dropped_into_itself_or_below_itself() {
    let (_d, _p, mut h) = open_deep("drag-cycle");
    let checkpoints = h.host.document().unwrap().recovery().checkpoints().len();
    for (over, frac) in [
        ("A", 0.5),
        ("A1", 0.5),
        ("Leaf", 0.5),
        ("A1a", 0.1),
        ("Leaf", 0.9),
    ] {
        h.pick_up("A", h.at(over, frac));
        let t = h.target().unwrap_or_else(|| panic!("target over {over}"));
        assert_eq!(t.verdict, Verdict::Refused, "over {over} at {frac}");
        h.release_at(h.at(over, frac));
        assert_eq!(h.shape(), ALL, "refused: nothing changed ({over})");
    }
    assert!(!h.dirty());
    assert_eq!(
        h.host.document().unwrap().recovery().checkpoints().len(),
        checkpoints
    );
}

#[test]
fn a_title_that_already_exists_at_the_destination_refuses_the_drop() {
    let d = dir("drag-conflict");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let a = doc.create_root("A").unwrap();
    doc.create_child(a, "Same").unwrap();
    let b = doc.create_root("B").unwrap();
    doc.create_child(b, "Same").unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_tree(&p);
    h.expand_all();
    let before = h.shape();
    // The second "Same" (under B) onto A, and next to A's own "Same".
    let from = h
        .host
        .tree()
        .rows()
        .iter()
        .rposition(|r| r.title == "Same")
        .unwrap();
    let id = h.host.tree().rows()[from].id;
    let row = h.host.tree().visible_position(id).unwrap();
    let (origin, _) = h.host.tree_geom;
    let pt =
        |row: usize, frac: f32| Pos2::new(origin.x + 80.0, origin.y + (row as f32 + frac) * ROW_H);
    for target in [pt(0, 0.5), pt(1, 0.1), pt(1, 0.9)] {
        h.moved(pt(row, 0.5));
        h.press_at(pt(row, 0.5));
        for k in 1..=4 {
            h.moved(pt(row, 0.5) + (target - pt(row, 0.5)) * (k as f32 / 4.0));
        }
        assert_eq!(h.target().map(|t| t.verdict), Some(Verdict::Refused));
        h.release_at(target);
        assert_eq!(h.shape(), before);
    }
    assert!(!h.dirty());
    // Reordering inside its own parent is never a conflict: B's "Same" is the only child, a no-op.
}

#[test]
fn dropping_where_it_already_is_changes_nothing_and_starts_no_autosave() {
    let (_d, _p, mut h) = open_deep("drag-noop");
    // A1 is already directly before A2; C is last; A is first.
    for (title, over, frac) in [
        ("A1", "A2", 0.1),
        ("A1", "A1", 0.5),
        ("C", "C", 0.9),
        ("A", "A", 0.1),
    ] {
        h.pick_up(title, h.at(over, frac));
        let t = h.target().expect("target");
        if (title, over) != ("A1", "A1") {
            assert_eq!(t.verdict, Verdict::NoOp, "{title} over {over}");
        }
        h.release_at(h.at(over, frac));
        assert_eq!(h.shape(), ALL);
    }
    assert!(!h.dirty(), "no-op drops leave the notebook clean");
    assert!(
        !h.host.edited && h.host.autosave_due.is_none(),
        "and start no autosave"
    );
    h.advance(3.0);
    assert!(!h.dirty());
}

#[test]
fn escape_cancels_a_drag_without_changing_anything() {
    let (_d, _p, mut h) = open_deep("drag-escape");
    h.pick_up("C", h.at("A", 0.1));
    assert_eq!(h.target().map(|t| t.kind), Some(Kind::Before));
    h.frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    assert!(h.target().is_none(), "the indicator is gone");
    // Releasing over a perfectly good target afterwards must not drop.
    h.release_at(h.at("A", 0.1));
    assert!(h.host.drag.is_none());
    assert_eq!(h.shape(), ALL);
    assert!(!h.dirty());
    assert!(!h.host.edited);
}

#[test]
fn releasing_outside_the_tree_cancels_safely() {
    let (_d, _p, mut h) = open_deep("drag-outside");
    let (_, view) = h.host.tree_geom;
    let outside = Pos2::new(view.right() + 300.0, view.center().y);
    h.pick_up("C", outside);
    assert!(h.target().is_none(), "no target over the editor");
    h.release_at(outside);
    assert!(h.host.drag.is_none());
    assert_eq!(h.shape(), ALL);
    assert!(!h.dirty());
}

// --- Indicator and move agree ----------------------------------------------------------------

#[test]
fn the_indicator_is_exactly_the_move_that_happens() {
    let zones = [
        ("A", 0.1),
        ("A", 0.5),
        ("A", 0.9),
        ("A1a", 0.1),
        ("A1a", 0.9),
        ("Leaf", 0.5),
        ("A2", 0.1),
        ("A2", 0.9),
        ("B", 0.1),
        ("B", 0.9),
        ("B1", 0.5),
        ("C", 0.1),
        ("C", 0.5),
        ("C", 0.9),
    ];
    for dragged in ["A1", "B", "C", "A2"] {
        for (over, frac) in zones {
            let (_d, _p, mut h) = open_deep("drag-agree");
            let id = h.id(dragged);
            let to = h.at(over, frac);
            h.pick_up(dragged, to);
            let t = h
                .target()
                .unwrap_or_else(|| panic!("{dragged} over {over}"));
            // Geometry: the row index is the row under the pointer.
            let (origin, _) = h.host.tree_geom;
            assert_eq!(t.row, ((to.y - origin.y) / ROW_H).floor() as usize);
            assert_eq!(t.over, Some(h.id(over)));
            let expect = match t.verdict {
                Verdict::Move { parent, position } => Some((parent, position)),
                _ => None,
            };
            let before = h.shape();
            h.release_at(to);
            let nb = h.host.document().unwrap().notebook();
            let node = nb.get(id).unwrap();
            match expect {
                Some((parent, position)) => {
                    // `usize::MAX` means "last child".
                    let position = if position == usize::MAX {
                        h.host.tree().siblings(id).len() - 1
                    } else {
                        position
                    };
                    assert_eq!(
                        (node.parent_id(), node.position()),
                        (parent, position),
                        "{dragged} over {over} {frac}"
                    );
                    assert!(h.dirty());
                }
                None => assert_eq!(
                    h.shape(),
                    before,
                    "{dragged} over {over} {frac}: {:?}",
                    t.verdict
                ),
            }
            // Kind follows the zone.
            let want = if frac < 0.25 {
                Kind::Before
            } else if frac > 0.75 {
                Kind::After
            } else {
                Kind::Child
            };
            assert_eq!(t.kind, want);
        }
    }
}

// --- Auto-expand -----------------------------------------------------------------------------

#[test]
fn resting_on_a_collapsed_note_opens_it_after_a_pause_and_not_before() {
    let (_d, _p, mut h) = open_deep("drag-hoverexpand");
    h.go("C");
    h.collapse_all();
    h.go("C");
    let before_sel = h.sel();
    let b = h.at("B", 0.5);
    h.pick_up("A", b);
    assert!(!h.expanded("B"));
    h.advance(0.3);
    assert!(!h.expanded("B"), "not yet");
    h.advance(0.5);
    assert!(h.expanded("B"), "after the delay");
    assert!(h.vis().contains(&"B1".to_owned()));
    assert_eq!(h.sel(), before_sel, "opening does not select");
    assert_eq!(h.host.title_text, "C");
    // The row positions moved: the target is recalculated for the pointer's place, not stale.
    let t = h.target().expect("target");
    let (origin, _) = h.host.tree_geom;
    assert_eq!(t.row, ((b.y - origin.y) / ROW_H).floor() as usize);
    // Cancelled: the hover-opened note closes again and nothing was written.
    h.frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    h.release_at(b);
    assert!(!h.expanded("B"), "hover-opening is transient");
    assert!(!h.dirty());
    assert!(!h.host.edited);
}

#[test]
fn moving_away_cancels_a_pending_expansion() {
    let (_d, _p, mut h) = open_deep("drag-hovercancel");
    h.collapse_all();
    h.pick_up("A", h.at("B", 0.5));
    h.advance(0.3);
    h.moved(h.at("C", 0.5));
    h.advance(0.6);
    assert!(!h.expanded("B"), "pointer left before the delay ran out");
    // And hovering a collapsed note's before/after zones never opens it.
    h.moved(h.at("B", 0.1));
    h.advance(1.0);
    assert!(!h.expanded("B"));
    h.release_at(h.at("B", 0.1));
}

#[test]
fn a_note_is_not_opened_over_its_own_subtree_and_a_drop_there_stays_refused() {
    let (_d, _p, mut h) = open_deep("drag-selfhover");
    h.collapse_all();
    h.host.set_expanded(&h.ctx.clone(), h.id("A"), true);
    h.settle();
    // A1 (closed, a child of A) dragged over itself: not opened, refused.
    h.pick_up("A1", h.at("A1", 0.5));
    h.advance(1.0);
    assert!(!h.expanded("A1"));
    assert_eq!(h.target().map(|t| t.verdict), Some(Verdict::Refused));
    h.release_at(h.at("A1", 0.5));
    assert_eq!(h.shape(), ALL);
}

#[test]
fn dropping_into_a_hover_opened_note_keeps_the_way_to_the_moved_note_open() {
    let (_d, _p, mut h) = open_deep("drag-dropopened");
    h.collapse_all();
    h.drag("C", h.at("B", 0.5));
    assert_eq!(h.shape(), "A(A1(A1a(Leaf)),A2)|B(B1,C)");
    assert_eq!(h.sel(), "C");
    assert!(
        h.expanded("B"),
        "revealed, as Qt does when the moved note is selected"
    );
}

// --- Auto-scroll -----------------------------------------------------------------------------

fn make_long(path: &Path, n: usize) {
    let mut doc = Document::open(path).unwrap();
    for i in 0..n {
        doc.create_root(&format!("N{i:03}")).unwrap();
    }
    doc.save().unwrap();
}

#[test]
fn dragging_near_an_edge_scrolls_and_stops_when_the_pointer_leaves() {
    let d = dir("drag-scroll");
    let p = d.join("n.omatree");
    make_long(&p, 120);
    let mut h = H::open_tree(&p);
    h.settle();
    let (origin0, view) = h.host.tree_geom;
    let x = origin0.x + 80.0;
    h.pick_up("N000", Pos2::new(x, view.center().y));
    let mid = h.host.tree_geom.0.y;
    // Near the bottom edge: the content moves up.
    h.moved(Pos2::new(x, view.bottom() - 6.0));
    for _ in 0..10 {
        h.frame(vec![], Modifiers::NONE);
    }
    let down = h.host.tree_geom.0.y;
    assert!(
        down < mid - 50.0,
        "scrolled towards the end: {mid} -> {down}"
    );
    // The target follows the rows under the pointer at the new offset.
    let t = h.target().expect("target");
    let py = view.bottom() - 6.0;
    assert_eq!(
        t.row,
        ((py - h.host.tree_geom.0.y) / ROW_H).floor() as usize
    );
    assert!(t.row > 10);
    // Away from the edge, scrolling stops.
    h.moved(Pos2::new(x, view.center().y));
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    let stopped = h.host.tree_geom.0.y;
    for _ in 0..5 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert_eq!(h.host.tree_geom.0.y, stopped);
    // Near the top it comes back.
    h.moved(Pos2::new(x, view.top() + 4.0));
    for _ in 0..10 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert!(h.host.tree_geom.0.y > stopped + 50.0);
    // Escape and release end it, and nothing was changed by scrolling.
    h.frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    h.release_at(Pos2::new(x, view.center().y));
    let rest = h.host.tree_geom.0.y;
    for _ in 0..5 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert_eq!(h.host.tree_geom.0.y, rest);
    assert!(h.host.drag.is_none());
    assert!(!h.dirty());
}

#[test]
fn a_drop_after_scrolling_lands_where_the_indicator_was() {
    let d = dir("drag-scrolldrop");
    let p = d.join("n.omatree");
    make_long(&p, 120);
    let mut h = H::open_tree(&p);
    h.settle();
    let (_, view) = h.host.tree_geom;
    let x = h.host.tree_geom.0.x + 80.0;
    h.pick_up("N000", Pos2::new(x, view.bottom() - 6.0));
    for _ in 0..12 {
        h.frame(vec![], Modifiers::NONE);
    }
    h.moved(Pos2::new(x, view.center().y));
    for _ in 0..3 {
        h.frame(vec![], Modifiers::NONE);
    }
    // The lower quarter of the row there: "after" it.
    let row = ((view.center().y - h.host.tree_geom.0.y) / ROW_H).floor();
    let at = Pos2::new(x, h.host.tree_geom.0.y + (row + 0.9) * ROW_H);
    h.moved(at);
    h.frame(vec![], Modifiers::NONE);
    let t = h.target().expect("target");
    assert_eq!(t.kind, Kind::After);
    let over = h.host.tree().visible_row(t.row).title.clone();
    h.release_at(at);
    let titles: Vec<String> = h
        .host
        .tree()
        .rows()
        .iter()
        .map(|r| r.title.clone())
        .collect();
    let moved_at = titles.iter().position(|t| t == "N000").unwrap();
    let expected = match t.kind {
        Kind::Before => titles.iter().position(|t| *t == over).unwrap() - 1,
        Kind::After => titles.iter().position(|t| *t == over).unwrap() + 1,
        _ => panic!("roots have no children"),
    };
    assert_eq!(moved_at, expected, "{:?} {over}", t.kind);
}

// --- Persistence, expansion, editor state ----------------------------------------------------

#[test]
fn a_dropped_hierarchy_survives_save_and_reopen() {
    let (_d, p, mut h) = open_deep("drag-persist");
    h.drag("A1", h.at("C", 0.5));
    h.drag("B", h.at("A", 0.1));
    let want = h.shape();
    assert_eq!(want, "B(B1)|A(A2)|C(A1(A1a(Leaf)))");
    h.key(Key::S, Modifiers::COMMAND);
    h.settle();
    assert!(!h.dirty());
    drop(h);
    let h2 = H::open_tree(&p);
    assert_eq!(h2.shape(), want);
}

#[test]
fn an_autosave_follows_a_real_drop() {
    let (_d, p, mut h) = open_deep("drag-autosave");
    h.drag("C", h.at("A", 0.1));
    assert!(h.host.edited || h.host.autosave_due.is_some());
    h.advance(AUTOSAVE_DELAY + 1.0);
    h.advance(1.0);
    assert!(!h.dirty(), "autosaved");
    drop(h);
    assert_eq!(H::open_tree(&p).shape(), "C|A(A1(A1a(Leaf)),A2)|B(B1)");
}

/// `tests/qml/moveexpanded.qml`: an expanded subtree moved into a collapsed parent is, once the
/// path is revealed, exactly as expanded as the document recorded it.
#[test]
fn an_expanded_subtree_moved_into_a_collapsed_parent_keeps_its_recorded_expansion() {
    let d = dir("drag-moveexpanded");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mover = doc.create_root("Mover").unwrap();
    let open1 = doc.create_child(mover, "Open1").unwrap();
    let deep_ = doc.create_child(open1, "Deep").unwrap();
    doc.create_child(deep_, "Leaf").unwrap();
    let shut = doc.create_child(open1, "Shut").unwrap();
    doc.create_child(shut, "Hidden").unwrap();
    let dest = doc.create_root("Dest").unwrap();
    doc.create_child(dest, "Inner").unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open_tree(&p);
    h.expand_all();
    let ctx = h.ctx.clone();
    h.host.set_expanded(&ctx, h.id("Shut"), false);
    h.host.set_expanded(&ctx, h.id("Dest"), false);
    h.settle();
    assert!(!h.dirty());
    let checkpoints = h.host.document().unwrap().recovery().checkpoints().len();

    h.drag("Open1", h.at("Dest", 0.5));
    let nb = h.host.document().unwrap();
    assert_eq!(
        h.shape(),
        "Mover|Dest(Inner,Open1(Deep(Leaf),Shut(Hidden)))"
    );
    assert_eq!(
        nb.recovery().checkpoints().len(),
        checkpoints + 1,
        "one move, one checkpoint"
    );
    // The moved note is selected, so the way to it is open; its own recorded state is untouched.
    assert!(h.expanded("Dest"));
    assert!(
        h.expanded("Open1") && h.expanded("Deep"),
        "recorded open stays open"
    );
    assert!(!h.expanded("Shut"), "recorded closed stays closed");
    assert!(h.vis().contains(&"Leaf".to_owned()));
    assert!(!h.vis().contains(&"Hidden".to_owned()));
    // The same after saving and reopening.
    h.key(Key::S, Modifiers::COMMAND);
    h.settle();
    let visible = h.vis();
    drop(h);
    let h2 = H::open_tree(&p);
    assert_eq!(h2.vis(), visible);
    assert!(h2.expanded("Open1") && h2.expanded("Deep") && !h2.expanded("Shut"));
    assert!(!h2.dirty());
}

#[test]
fn a_moved_note_keeps_its_text_caret_and_undo_history_and_others_are_untouched() {
    let (_d, _p, mut h) = open_deep("drag-editorstate");
    h.dirty_edit("A1a", " typed");
    let text = h.body("A1a");
    assert!(text.ends_with("typed"));
    let caret = h.caret("A1a");
    // Another note, edited too, to prove isolation.
    h.dirty_edit("B1", " other");
    let other = h.body("B1");
    h.drag("A1a", h.at("C", 0.5));
    assert_eq!(h.sel(), "A1a");
    assert_eq!(h.body("A1a"), text);
    assert_eq!(h.body("B1"), other);
    assert_eq!(h.shape(), "A(A1,A2)|B(B1)|C(A1a(Leaf))");
    assert_eq!(
        h.caret("A1a"),
        caret,
        "caret and selection survive the move"
    );
    // Undo reaches only the moved note's own typing.
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::Z, Modifiers::COMMAND);
    assert!(
        !h.body("A1a").ends_with("typed"),
        "its own undo history survived the move"
    );
    assert_eq!(h.body("B1"), other, "the other note is not undone");
    assert_eq!(h.body("A2"), "a2 body\n");
}

#[test]
fn a_cancelled_or_refused_drag_leaves_the_editor_alone() {
    let (_d, _p, mut h) = open_deep("drag-editorsafe");
    h.dirty_edit("C", " keep");
    let len = h.host.buffer().len();
    h.set_caret(len - 1, len - 1);
    h.settle();
    let sel = h.caret("C");
    // Cancelled, then refused.
    h.pick_up("A", h.at("B", 0.5));
    h.frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    h.release_at(h.at("B", 0.5));
    h.drag("A", h.at("Leaf", 0.5));
    assert_eq!(h.sel(), "C");
    assert_eq!(h.caret("C"), sel);
    assert!(h.body("C").ends_with("keep"));
    assert_eq!(h.shape(), ALL);
}

#[test]
fn keyboard_works_after_a_drop() {
    let (_d, _p, mut h) = open_deep("drag-keyboard");
    h.drag("C", h.at("A", 0.1));
    assert_eq!(h.sel(), "C");
    assert!(h.has_focus(Focus::Tree), "the tree keeps the keyboard");
    h.press(Key::ArrowDown);
    assert_eq!(h.sel(), "A");
    h.key(Key::ArrowDown, Modifiers::ALT);
    assert_eq!(h.shape(), "C|B(B1)|A(A1(A1a(Leaf)),A2)");
    let _ = make_deep;
}

#[test]
fn dragging_through_a_large_notebook_stays_cheap_and_only_the_drop_writes() {
    let d = dir("drag-large");
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
    drop(doc);
    let mut h = H::open_tree(&p);
    h.expand_all();
    assert_eq!(h.host.tree().len_visible(), 6300);
    let (origin, view) = h.host.tree_geom;
    let x = origin.x + 80.0;
    h.pick_up("Root 000", Pos2::new(x, view.center().y));
    // Pointer motion across the rows: a frame each, nothing dirtied, nothing rebuilt.
    let before = h.shape_len();
    let t = std::time::Instant::now();
    let frames = 60;
    for k in 0..frames {
        h.moved(Pos2::new(x, view.top() + 40.0 + k as f32 * 7.0));
    }
    let per_frame = t.elapsed() / frames;
    eprintln!("drag frame with 6,300 notes: {per_frame:?}");
    assert!(
        per_frame < Duration::from_millis(250),
        "{per_frame:?} (debug build)"
    );
    assert!(!h.dirty());
    assert_eq!(h.shape_len(), before);
    h.frame(
        vec![Event::Key {
            key: Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
        Modifiers::NONE,
    );
    h.release_at(Pos2::new(x, view.center().y));
    assert!(!h.dirty());
}

impl H {
    fn shape_len(&self) -> usize {
        self.host.tree().rows().len()
    }
}

/// Writes a 120-note flat notebook to `$OMATREE_GUI_FIXTURE` (manual GUI check of auto-scroll).
#[test]
#[ignore]
fn write_long_gui_fixture() {
    let path =
        PathBuf::from(std::env::var_os("OMATREE_GUI_FIXTURE").expect("set OMATREE_GUI_FIXTURE"));
    let _ = fs::remove_file(&path);
    make_long(&path, 120);
}
