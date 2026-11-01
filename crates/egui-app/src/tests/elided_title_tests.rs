//! R2: right-clicking a tree row whose title is cut with an ellipsis, while the pointer rests on it.
//! The elided-title tooltip and the row's context menu used to share one popup id, which tripped an
//! egui `debug_assert!` (debug builds only). These tests run in the debug profile, where it is armed.

use super::*;
use omatree_core::notebook::Colour;

const LONG: &str = "A deliberately long title that cannot possibly fit in the narrow tree pane";

fn notebook(name: &str) -> (TestDir, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    doc.create_root("Short").unwrap();
    doc.create_root(LONG).unwrap();
    doc.save().unwrap();
    (d, p)
}

fn secondary(p: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos: p,
        button: egui::PointerButton::Secondary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

impl H {
    /// The pointer rests on the elided row for a few frames (the tooltip shows), then right-clicks.
    fn right_click_elided(&mut self) -> Pos2 {
        let t = self
            .painted()
            .texts
            .into_iter()
            .find(|t| t.text.starts_with("A deliberately") && t.elided)
            .expect("the long title is drawn elided");
        let p = Pos2::new(t.pos.x + 10.0, t.centre_y());
        self.moved(p);
        self.settle();
        self.frame(vec![secondary(p, true)], Modifiers::NONE);
        self.frame(vec![secondary(p, false)], Modifiers::NONE);
        self.settle();
        p
    }
}

#[test]
fn right_clicking_an_elided_title_under_the_pointer_opens_its_menu_without_asserting() {
    let (_d, p) = notebook("r2-elided");
    let mut h = H::open_sized(&p, Vec2::new(420.0, 600.0));
    h.right_click_elided();
    // The menu is the row's own menu, and the row was selected by the click.
    assert_eq!(h.sel(), LONG);
    let texts: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    assert!(texts.iter().any(|t| t == "No colour"), "{texts:?}");
}

#[test]
fn the_menu_of_an_elided_row_still_picks_a_colour_and_the_tree_goes_on_working() {
    let (_d, p) = notebook("r2-elided-pick");
    let mut h = H::open_sized(&p, Vec2::new(420.0, 600.0));
    h.ctx.enable_accesskit();
    h.settle();
    h.right_click_elided();
    // Close the pointer's menu, then use the keyboard's own route on the same row.
    h.press(Key::Escape);
    h.go(LONG);
    h.pick_by_keyboard("Blue");
    assert_eq!(h.colour_of(LONG), Some(Colour::Blue));
    h.press(Key::ArrowUp);
    assert_eq!(h.sel(), "Short");
    h.press(Key::ArrowDown);
    assert_eq!(h.sel(), LONG);
}

#[test]
fn the_tooltip_of_an_elided_title_still_shows_when_no_menu_is_open() {
    let (_d, p) = notebook("r2-elided-tip");
    let mut h = H::open_sized(&p, Vec2::new(420.0, 600.0));
    let t = h
        .painted()
        .texts
        .into_iter()
        .find(|t| t.text.starts_with("A deliberately"))
        .unwrap();
    h.moved(Pos2::new(t.pos.x + 10.0, t.centre_y()));
    h.settle();
    let n = h.painted().texts.iter().filter(|t| t.text == LONG).count();
    assert!(n >= 1, "the full title is shown, in the tooltip");
}

#[test]
fn the_tooltip_gives_way_to_the_menu_of_the_same_row() {
    let (_d, p) = notebook("r2-elided-menu");
    let mut h = H::open_sized(&p, Vec2::new(420.0, 600.0));
    h.go(LONG);
    let full = |h: &H| h.painted().texts.iter().filter(|t| t.text == LONG).count();
    let t = h
        .painted()
        .texts
        .into_iter()
        .find(|t| t.text.starts_with("A deliberately") && t.elided)
        .unwrap();
    h.moved(Pos2::new(t.pos.x + 10.0, t.centre_y()));
    h.settle();
    let with_tooltip = full(&h);
    h.right_click_elided();
    let texts: Vec<String> = h.painted().texts.into_iter().map(|t| t.text).collect();
    assert!(texts.iter().any(|t| t == "No colour"), "the menu is open");
    assert_eq!(
        full(&h),
        with_tooltip - 1,
        "the tooltip is gone, the menu is there"
    );
}
