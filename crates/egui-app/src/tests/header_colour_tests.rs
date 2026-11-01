//! T2: the colour picker in the note header. It reads and writes the one tag T1 stores, so these
//! tests drive the real host through pointer and keyboard events and look at what was painted.
//! Disposable notebooks throughout.

use super::colour_tests::{focused_label, opened};
use super::drag_tests::button;
use super::tree_tests::deep;
use super::*;
use crate::layout::{HEADER_DOT, HEADER_FULL_MIN, HEADER_H};
use egui::Color32;
use omatree_core::notebook::Colour;

impl H {
    /// The header's six swatches, in painted order: each colour's dot, left to right.
    fn swatches(&self) -> Vec<(Colour, Pos2)> {
        let ch = self.chrome();
        let mut dots: Vec<_> = self
            .header_dots()
            .into_iter()
            .filter(|d| d.fill != Color32::TRANSPARENT && (d.radius - HEADER_DOT).abs() < 1.01)
            .filter_map(|d| {
                Colour::ALL
                    .into_iter()
                    .find(|c| ch.tag(*c) == d.fill)
                    .map(|c| (c, d.centre))
            })
            .collect();
        dots.sort_by(|a, b| a.1.x.total_cmp(&b.1.x));
        dots
    }

    fn swatch(&self, c: Colour) -> Pos2 {
        self.swatches()
            .into_iter()
            .find(|(k, _)| *k == c)
            .unwrap_or_else(|| panic!("no header swatch for {c:?}"))
            .1
    }

    /// The colour the header marks as selected: the one with the outline ring around it.
    fn ringed(&self) -> Option<Colour> {
        let rings: Vec<_> = self
            .header_dots()
            .into_iter()
            .filter(|d| d.fill == Color32::TRANSPARENT && d.radius > HEADER_DOT + 2.0)
            .collect();
        let sw = self.swatches();
        let marked: Vec<_> = sw
            .iter()
            .filter(|(_, p)| rings.iter().any(|r| (r.centre - *p).length() < 0.5))
            .map(|(c, _)| *c)
            .collect();
        // A focus ring is drawn the same way; only the selection ring is in the text colour.
        let ink = self.chrome().text;
        let marked: Vec<_> = marked
            .into_iter()
            .filter(|c| {
                let at = self.swatch(*c);
                self.header_dots().iter().any(|r| {
                    r.fill == Color32::TRANSPARENT
                        && (r.centre - at).length() < 0.5
                        && r.radius > HEADER_DOT + 2.0
                        && self.ring_colour(r.centre) == Some(ink)
                })
            })
            .collect();
        assert!(marked.len() <= 1, "one selection at most: {marked:?}");
        marked.first().copied()
    }

    /// The stroke colour of the painted ring centred at `at`.
    fn ring_colour(&self, at: Pos2) -> Option<Color32> {
        fn walk(s: &egui::Shape, at: Pos2, out: &mut Option<Color32>) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, at, out)),
                egui::Shape::Circle(c)
                    if c.fill == Color32::TRANSPARENT && (c.center - at).length() < 0.5 =>
                {
                    *out = Some(c.stroke.color)
                }
                _ => {}
            }
        }
        let mut out = None;
        for s in &self.shapes {
            walk(&s.shape, at, &mut out);
        }
        out
    }

    fn click_at(&mut self, p: Pos2) {
        self.frame(vec![Event::PointerMoved(p)], Modifiers::NONE);
        self.press_at(p);
        self.release_at(p);
    }

    fn click_swatch(&mut self, c: Colour) {
        let p = self.swatch(c);
        self.click_at(p);
    }

    fn a11y_names(&self, role: egui::accesskit::Role) -> Vec<String> {
        let u = self.a11y.as_ref().expect("enable_accesskit first");
        u.nodes
            .iter()
            .filter(|(_, n)| n.role() == role)
            .filter_map(|(_, n)| n.label().map(str::to_owned))
            .collect()
    }

    fn header_names(&self) -> Vec<String> {
        self.a11y_names(egui::accesskit::Role::RadioButton)
            .into_iter()
            .filter(|l| l.contains("note colour"))
            .collect()
    }
}

fn dir_title(h: &H) -> String {
    h.host.title_text().to_owned()
}

#[test]
fn six_colours_render_in_the_palettes_order_at_the_right_of_the_title() {
    let (_d, p) = deep("t2-order");
    let mut h = opened(&p);
    h.go("A2");
    let sw = h.swatches();
    assert_eq!(
        sw.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        Colour::ALL.to_vec(),
        "Red, Orange, Yellow, Green, Blue, Purple"
    );
    let right = h.screen.x;
    let first = sw[0].1.x;
    assert!(
        first > right - 200.0 && first < right,
        "in the header's right region"
    );
    for w in sw.windows(2) {
        assert!(w[1].1.x - w[0].1.x >= 2.0 * HEADER_DOT, "no overlap: {w:?}");
        assert!((w[0].1.y - w[1].1.y).abs() < 0.01, "one row");
    }
    for (_, p) in &sw {
        assert!(p.y < HEADER_H, "inside the header");
        assert!(p.x + HEADER_DOT < right, "inside the window");
    }
    assert!(h.ringed().is_none(), "an untagged note has no selection");
}

#[test]
fn the_active_notes_colour_is_ringed_and_named_selected() {
    let (_d, p) = deep("t2-selected");
    let mut h = opened(&p);
    h.go("A2");
    h.ctx.enable_accesskit();
    h.settle();
    let none = h.header_names();
    assert_eq!(none.len(), 6, "{none:?}");
    assert!(none.iter().all(|n| !n.contains("selected")), "{none:?}");
    assert!(none.contains(&"Red note colour".to_owned()));
    h.click_swatch(Colour::Green);
    assert_eq!(h.ringed(), Some(Colour::Green));
    let names = h.header_names();
    assert!(
        names.contains(&"Green note colour, selected".to_owned()),
        "{names:?}"
    );
    assert_eq!(names.iter().filter(|n| n.contains("selected")).count(), 1);
}

#[test]
fn clicking_an_unselected_colour_assigns_it_and_the_tree_follows_at_once() {
    let (_d, p) = deep("t2-assign");
    let mut h = opened(&p);
    h.go("B1");
    h.click_swatch(Colour::Blue);
    assert_eq!(h.colour_of("B1"), Some(Colour::Blue));
    assert_eq!(h.doc_colour("B1"), Some(Colour::Blue));
    assert!(h.host.is_dirty(), "a modification like any other");
    assert!(
        h.painted()
            .dots
            .iter()
            .any(|d| d.fill == h.chrome().tag(Colour::Blue)),
        "the tree row shows the dot"
    );
    h.click_swatch(Colour::Red);
    assert_eq!(h.colour_of("B1"), Some(Colour::Red), "replaced, not added");
    assert_eq!(h.ringed(), Some(Colour::Red));
    assert_eq!(h.sel(), "B1", "the same note is still selected");
}

#[test]
fn clicking_the_selected_colour_clears_the_tag() {
    let (_d, p) = deep("t2-clear");
    let mut h = opened(&p);
    h.go("B1");
    h.click_swatch(Colour::Purple);
    h.click_swatch(Colour::Purple);
    assert_eq!(h.colour_of("B1"), None);
    assert_eq!(h.doc_colour("B1"), None);
    assert!(h.ringed().is_none());
    assert!(h.painted().dots.is_empty(), "and the tree's dot is gone");
    assert_eq!(h.host.status(), "Colour cleared");
}

#[test]
fn switching_notes_updates_the_selection_and_the_two_pickers_agree() {
    let (_d, p) = deep("t2-switch");
    let mut h = opened(&p);
    h.go("A");
    h.click_swatch(Colour::Orange);
    h.go("A2");
    assert!(h.ringed().is_none(), "an untagged note after a tagged one");
    h.click_swatch(Colour::Yellow);
    h.go("A");
    assert_eq!(h.ringed(), Some(Colour::Orange));
    h.go("A2");
    assert_eq!(h.ringed(), Some(Colour::Yellow));
    // The tree menu changes the same tag, and the header shows it.
    h.pick_by_keyboard("Purple");
    assert_eq!(h.colour_of("A2"), Some(Colour::Purple));
    assert_eq!(h.ringed(), Some(Colour::Purple), "tree → header");
    // And the header clears what the menu set: one authoritative tag.
    h.click_swatch(Colour::Purple);
    assert_eq!(h.doc_colour("A2"), None, "header → document");
    assert_eq!(h.colour_of("A2"), None, "and the tree row");
    h.go("A");
    assert_eq!(
        h.ringed(),
        Some(Colour::Orange),
        "the other note is untouched"
    );
}

#[test]
fn rapid_changes_leave_the_last_one_everywhere() {
    let (_d, p) = deep("t2-rapid");
    let mut h = opened(&p);
    h.go("C");
    // One click per frame, no settling in between, then the whole palette backwards and forwards.
    let order: Vec<Colour> = Colour::ALL
        .into_iter()
        .chain(Colour::ALL.into_iter().rev())
        .chain([Colour::Blue, Colour::Blue, Colour::Blue])
        .collect();
    for c in &order {
        let p = h.swatch(*c);
        h.frame(
            vec![Event::PointerMoved(p), button(p, true)],
            Modifiers::NONE,
        );
        h.frame(
            vec![Event::PointerMoved(p), button(p, false)],
            Modifiers::NONE,
        );
    }
    h.settle();
    // Blue three times is assign, clear, assign.
    assert_eq!(h.doc_colour("C"), Some(Colour::Blue));
    assert_eq!(h.colour_of("C"), Some(Colour::Blue));
    assert_eq!(h.ringed(), Some(Colour::Blue));
    assert_eq!(h.sel(), "C");
}

#[test]
fn a_header_change_autosaves_and_survives_reopening() {
    let (_d, p) = deep("t2-autosave");
    let mut h = opened(&p);
    h.go("B1");
    h.click_swatch(Colour::Green);
    assert!(h.host.is_dirty());
    h.advance(1.5);
    assert!(!h.host.is_dirty(), "autosaved");
    drop(h);
    let mut h2 = opened(&p);
    h2.go("B1");
    assert_eq!(
        h2.ringed(),
        Some(Colour::Green),
        "the header shows it after a restart"
    );
    assert_eq!(h2.colour_of("B1"), Some(Colour::Green));
}

#[test]
fn quitting_right_after_a_header_change_cannot_bypass_the_dirty_guard() {
    let (_d, p) = deep("t2-close");
    let mut h = opened(&p);
    h.go("A");
    h.click_swatch(Colour::Red);
    assert!(!h.request_close(), "closing asks first");
    assert!(matches!(h.host.dialog(), Dialog::Unsaved { .. }));

    let (_d2, p2) = deep("t2-quit");
    let mut g = opened(&p2);
    g.go("A");
    let answers = g.watch_quit();
    g.click_swatch(Colour::Red);
    g.raise_quit();
    g.frames(4);
    assert_eq!(*g.host.dialog(), Dialog::Unsaved { error: None });
    assert!(
        answers.borrow().is_empty(),
        "the system was not told it may quit"
    );
    assert_eq!(
        Document::open_existing(&p2)
            .unwrap()
            .notebook()
            .depth_first()[0]
            .colour(),
        None,
        "nothing was written behind the question"
    );
}

#[test]
fn the_keyboard_reaches_moves_picks_and_leaves_without_touching_the_title() {
    let (_d, p) = deep("t2-keys");
    let mut h = opened(&p);
    h.ctx.enable_accesskit();
    h.go("A2");
    h.host.focus(Focus::Title);
    h.settle();
    let title = dir_title(&h);
    assert_eq!(focused_label(&h), "Note title");
    // One Tab from the title lands on the first swatch.
    h.press(Key::Tab);
    assert_eq!(focused_label(&h), "Red note colour");
    h.press(Key::ArrowRight);
    h.press(Key::ArrowRight);
    assert_eq!(focused_label(&h), "Yellow note colour");
    h.press(Key::ArrowLeft);
    assert_eq!(focused_label(&h), "Orange note colour");
    h.press(Key::ArrowLeft);
    h.press(Key::ArrowLeft);
    assert_eq!(focused_label(&h), "Red note colour", "stops at the end");
    for _ in 0..8 {
        h.press(Key::ArrowRight);
    }
    assert_eq!(
        focused_label(&h),
        "Purple note colour",
        "and at the other end"
    );
    // Enter selects, Space on the selected colour clears.
    h.press(Key::ArrowLeft);
    h.press(Key::Enter);
    assert_eq!(h.colour_of("A2"), Some(Colour::Blue));
    assert_eq!(
        focused_label(&h),
        "Blue note colour, selected",
        "focus stays on the swatch"
    );
    h.press(Key::Space);
    assert_eq!(h.colour_of("A2"), None);
    assert_eq!(dir_title(&h), title, "the title was never touched");
    assert_eq!(h.host.title_text(), "A2");
    assert!(h.host.is_dirty());
}

#[test]
fn leaving_the_swatches_goes_where_the_key_says() {
    let (_d, p) = deep("t2-focus");
    let mut h = opened(&p);
    h.ctx.enable_accesskit();
    h.go("A2");
    // Tab on: the editor. Shift+Tab: back to the title. Escape: the tree.
    h.host.focus(Focus::Title);
    h.settle();
    h.press(Key::Tab);
    h.press(Key::Tab);
    assert!(
        h.has_focus(Focus::Editor),
        "Tab leaves the swatches for the editor"
    );
    h.host.focus(Focus::Title);
    h.settle();
    h.press(Key::Tab);
    h.key(Key::Tab, Modifiers::SHIFT);
    assert!(h.has_focus(Focus::Title), "Shift+Tab returns to the title");
    assert_eq!(focused_label(&h), "Note title");
    h.press(Key::Tab);
    h.press(Key::Escape);
    assert!(h.has_focus(Focus::Tree), "Escape returns to the tree");
    assert_eq!(h.sel(), "A2");
    assert_eq!(h.colour_of("A2"), None, "and changed nothing");
    assert!(!h.host.is_dirty());
}

#[test]
fn narrow_windows_collapse_the_swatches_into_one_control_with_the_same_choices() {
    let (_d, p) = deep("t2-narrow");
    let width = 330.0;
    assert!(width < HEADER_FULL_MIN + 170.0);
    let mut h = H::open_sized(&p, Vec2::new(width, 600.0));
    h.ctx.enable_accesskit();
    h.go("A2");
    assert!(h.swatches().is_empty(), "the six are gone");
    let dots = h.header_dots();
    assert!(
        dots.iter()
            .all(|d| d.centre.x + d.radius <= width && d.centre.x - d.radius >= 0.0),
        "inside the window: {dots:?}"
    );
    let button = h
        .a11y_names(egui::accesskit::Role::Button)
        .into_iter()
        .find(|n| n.starts_with("Note colour"))
        .expect("one named control");
    assert_eq!(button, "Note colour, none");
    // Its centre is the right-most thing in the header.
    let ctl = dots
        .iter()
        .max_by(|a, b| a.centre.x.total_cmp(&b.centre.x))
        .unwrap()
        .centre;
    h.click_at(ctl);
    assert!(egui::Popup::is_any_open(&h.ctx), "it opens the picker");
    let swatches: Vec<_> = h
        .a11y_names(egui::accesskit::Role::RadioButton)
        .into_iter()
        .filter(|l| Colour::ALL.iter().any(|c| c.label() == l))
        .collect();
    assert_eq!(swatches.len(), 6, "the same six: {swatches:?}");
    // Escape cancels.
    h.press(Key::Escape);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(h.colour_of("A2"), None);
    assert!(!h.host.is_dirty(), "cancelled: nothing changed");
    // Pick one by keyboard: the control opens, Tab to the swatch, Enter.
    h.click_at(ctl);
    for _ in 0..4 {
        if focused_label(&h) == "Green" {
            break;
        }
        h.press(Key::Tab);
    }
    assert_eq!(focused_label(&h), "Green");
    h.press(Key::Enter);
    assert_eq!(h.colour_of("A2"), Some(Colour::Green));
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(
        h.a11y_names(egui::accesskit::Role::Button)
            .into_iter()
            .find(|n| n.starts_with("Note colour")),
        Some("Note colour, green".to_owned())
    );
    // And it clears: open, "No colour".
    h.click_at(ctl);
    for _ in 0..12 {
        if focused_label(&h) == "No colour" {
            break;
        }
        h.press(Key::Tab);
    }
    h.press(Key::Enter);
    assert_eq!(h.colour_of("A2"), None);
    // The title can still be edited at this width.
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("x");
    assert_eq!(
        h.host.title_text(),
        "x",
        "typing replaces the selected title: it is editable"
    );
}

#[test]
fn a_very_long_unicode_title_never_runs_under_the_swatches() {
    let (_d, p) = deep("t2-long");
    let mut h = opened(&p);
    h.ctx.enable_accesskit();
    h.go("A2");
    let long = "Ünïcödé 🦀 заметка 日本語のとても長いタイトル ".repeat(5);
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text(&long);
    h.press(Key::Enter);
    h.advance(1.5);
    assert!(!h.host.is_dirty(), "the long title was accepted and saved");
    drop(h);
    for width in [1000.0_f32, 700.0, 420.0, 330.0] {
        let mut g = H::open_sized(&p, Vec2::new(width, 600.0));
        g.ctx.enable_accesskit();
        let id = g
            .host
            .rows()
            .iter()
            .find(|r| r.title.starts_with("Ünï"))
            .expect("the long title")
            .id;
        assert!(g.host.select(&g.ctx.clone(), id));
        g.settle();
        let u = g.a11y.as_ref().unwrap();
        let title = u
            .nodes
            .iter()
            .find(|(_, n)| n.label() == Some("Note title"))
            .map(|(_, n)| n.bounds().unwrap())
            .expect("the title field");
        let marks: Vec<_> = g.header_dots();
        let left = marks
            .iter()
            .map(|d| d.centre.x - d.radius)
            .fold(f32::MAX, f32::min);
        assert!(
            f64::from(left) >= title.x1 - 0.5,
            "{width}: the picker starts at {left}, the title ends at {}",
            title.x1
        );
        assert!(
            marks.iter().all(|d| d.centre.x + d.radius <= width),
            "{width}"
        );
    }
}

#[test]
fn both_themes_draw_the_swatches_in_their_own_palette() {
    let (_d, p) = deep("t2-theme");
    let mut h = opened(&p);
    h.go("A2");
    h.click_swatch(Colour::Green);
    for (theme, name) in [(egui::Theme::Dark, "dark"), (egui::Theme::Light, "light")] {
        h.system = Some(theme);
        h.advance(1.0);
        let ch = h.chrome();
        let sw = h.swatches();
        assert_eq!(sw.len(), 6, "{name}");
        for c in Colour::ALL {
            assert!(
                h.header_dots().iter().any(|d| d.fill == ch.tag(c)),
                "{name} {c:?}"
            );
        }
        assert_eq!(h.ringed(), Some(Colour::Green), "{name}: still selected");
    }
    assert_eq!(
        h.colour_of("A2"),
        Some(Colour::Green),
        "the stored identity did not change"
    );
}

#[test]
fn no_picker_without_an_active_note() {
    let mut h = H::bare(Vec2::new(900.0, 600.0));
    h.settle();
    assert!(h.host.selected().is_none());
    assert!(h.header_dots().is_empty());
    let (_d, p) = deep("t2-none");
    let mut g = opened(&p);
    g.go("A2");
    assert_eq!(g.swatches().len(), 6);
}

#[test]
fn a_header_change_leaves_the_markdown_bytes_and_the_editors_undo_alone() {
    let (_d, p) = deep("t2-body");
    let mut h = opened(&p);
    h.dirty_edit("A2", "typed");
    let typed = h.body("A2");
    let before = h.host.buffer().to_owned();
    let caret = h.caret("A2");
    h.click_swatch(Colour::Red);
    h.click_swatch(Colour::Blue);
    h.click_swatch(Colour::Blue);
    assert_eq!(h.body("A2"), typed, "the body is the same bytes");
    assert_eq!(h.host.buffer(), before);
    assert_eq!(h.host.title_text(), "A2");
    assert_eq!(h.caret("A2"), caret, "caret kept");
    // Ctrl+Z still undoes the typing, not the colour.
    h.host.focus(Focus::Editor);
    h.settle();
    h.key(Key::Z, Modifiers::COMMAND);
    assert_ne!(h.body("A2"), typed, "the typing was undone");
    assert!(!h.body("A2").ends_with("typed"));
    assert_eq!(
        h.doc_colour("A2"),
        None,
        "the colour was neither undone nor redone: it is the last click's"
    );
}
