//! Visual cohesion of the host's chrome (ticket E6b), checked as geometry and colour of what the
//! frames actually paint: positions of text, filled rows, guide lines and drop indicators, read
//! back from the shapes of the last frame, never as pixel equality. The behaviour under the
//! chrome (keys, selection, drag and drop, dialogs, save) has its own tests; the ones here
//! confirm it is unchanged by the polish.

use super::drag_tests::{open_deep, ALL};
use super::theme_tests::{core_of, install, rich_notebook, LATTE, TOKYO};
use super::tree_tests::shape;
use super::*;
use crate::layout::{BAR_H, GUTTER, INDENT, ROW_H, TREE_LEFT, TREE_MAX_SHARE, TREE_MIN};
use crate::theme::{colour, readable, Chrome, MUTED_MIN, TEXT_MIN};
use eframe::egui::epaint::Shape;
use eframe::egui::Color32;
use omatree_core::theme::{Palette as Core, Rgb};
use omatree_editor::theme::contrast;

/// A theme whose every pair is close in brightness: the worst case for legibility.
const LOW: &str = "mode = \"dark\"\naccent = \"#8a8f98\"\nselection = \"#3d4046\"\n\
muted = \"#3a3c40\"\nbackground = \"#2a2c30\"\ndark_background = \"#24262a\"\n\
lighter_background = \"#32353a\"\nforeground = \"#80848c\"\ndark_foreground = \"#5c6068\"\n\
bright_foreground = \"#9a9ea6\"\nred = \"#8c6468\"\ngreen = \"#6c8a6c\"\nmagenta = \"#8a7a9a\"\n\
yellow = \"#9a8a68\"\n";

#[derive(Clone, Debug)]
pub(super) struct Text {
    pub(super) text: String,
    pub(super) pos: Pos2,
    pub(super) size: Vec2,
    pub(super) colour: Color32,
    pub(super) elided: bool,
    pub(super) rows: usize,
    /// The slices of the text that carry a background (a highlighted match).
    pub(super) marked: Vec<String>,
}

impl Text {
    pub(super) fn centre_y(&self) -> f32 {
        self.pos.y + self.size.y / 2.0
    }

    pub(super) fn right(&self) -> f32 {
        self.pos.x + self.size.x
    }
}

#[derive(Clone, Debug)]
pub(super) struct Fill {
    pub(super) rect: Rect,
    pub(super) fill: Color32,
    pub(super) stroke: egui::Stroke,
}

#[derive(Clone, Debug)]
pub(super) struct Line {
    pub(super) a: Pos2,
    pub(super) b: Pos2,
}

#[derive(Default)]
pub(super) struct Painted {
    pub(super) texts: Vec<Text>,
    pub(super) fills: Vec<Fill>,
    pub(super) lines: Vec<Line>,
}

fn collect(shape: &Shape, out: &mut Painted) {
    match shape {
        Shape::Vec(v) => v.iter().for_each(|s| collect(s, out)),
        Shape::Text(t) => {
            let section = t.galley.job.sections.first();
            let mut colour = section.map_or(t.fallback_color, |s| s.format.color);
            if colour == Color32::PLACEHOLDER {
                colour = t.override_text_color.unwrap_or(t.fallback_color);
            }
            out.texts.push(Text {
                text: t.galley.text().to_owned(),
                pos: t.pos,
                size: t.galley.size(),
                colour,
                elided: t.galley.elided,
                rows: t.galley.rows.len(),
                marked: t
                    .galley
                    .job
                    .sections
                    .iter()
                    .filter(|s| s.format.background != Color32::TRANSPARENT)
                    .map(|s| t.galley.job.text[s.byte_range.start.0..s.byte_range.end.0].to_owned())
                    .collect(),
            });
        }
        Shape::Rect(r) => out.fills.push(Fill {
            rect: r.rect,
            fill: r.fill,
            stroke: r.stroke,
        }),
        Shape::LineSegment { points, .. } => out.lines.push(Line {
            a: points[0],
            b: points[1],
        }),
        _ => {}
    }
}

impl H {
    pub(super) fn painted(&self) -> Painted {
        let mut p = Painted::default();
        for s in &self.shapes {
            collect(&s.shape, &mut p);
        }
        p
    }

    pub(super) fn chrome(&self) -> Chrome {
        *self.host.theme.chrome()
    }

    /// The right edge of the tree pane.
    pub(super) fn tree_right(&self) -> f32 {
        self.host.tree_geom.1.right()
    }

    pub(super) fn tree_width(&self) -> f32 {
        self.host.tree_geom.1.width()
    }

    /// Painted text equal to `s`, in the tree pane (`true`) or to its right.
    pub(super) fn text_in(&self, s: &str, tree: bool) -> Text {
        let split = self.tree_right();
        self.painted()
            .texts
            .into_iter()
            .find(|t| t.text == s && (t.pos.x < split) == tree)
            .unwrap_or_else(|| panic!("no painted {s:?} (tree: {tree})"))
    }

    pub(super) fn texts_starting(&self, s: &str) -> Vec<Text> {
        self.painted()
            .texts
            .into_iter()
            .filter(|t| t.text.starts_with(s))
            .collect()
    }

    /// The filled row rectangle painted for visible row `i` (selection or hover), if any.
    pub(super) fn row_fill(&self, i: usize, colour: Color32) -> Option<Fill> {
        let top = self.host.tree_geom.0.y + i as f32 * ROW_H;
        self.painted().fills.into_iter().find(|f| {
            f.fill == colour
                && (f.rect.top() - top).abs() < 0.01
                && (f.rect.height() - ROW_H).abs() < 0.01
        })
    }

    pub(super) fn row_of(&self, title: &str) -> usize {
        self.host
            .tree()
            .visible_position(self.id(title))
            .unwrap_or_else(|| panic!("{title} is not visible"))
    }
}

fn rich(name: &str) -> (PathBuf, PathBuf) {
    rich_notebook(name)
}

// --- Alignment -----------------------------------------------------------------------------------

#[test]
fn the_first_tree_row_and_the_note_title_share_a_centre_line() {
    let d = dir("polish-centre");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let h = H::open_tree(&p);
    let row = h.text_in("Projects", true);
    let title = h.text_in("Projects", false);
    assert!(
        (row.centre_y() - title.centre_y()).abs() <= 2.0,
        "row {} vs title {}",
        row.centre_y(),
        title.centre_y()
    );
}

#[test]
fn the_title_the_body_and_the_status_text_hang_on_one_axis() {
    let d = dir("polish-axis");
    let p = d.join("n.omatree");
    make_fixture(&p);
    for size in [
        Vec2::new(1000.0, 700.0),
        Vec2::new(640.0, 480.0),
        Vec2::new(420.0, 320.0),
    ] {
        let mut h = H::open_sized(&p, size);
        h.settle();
        let title = h.text_in("Projects", false);
        let body = h.painted();
        let body = body
            .texts
            .iter()
            .find(|t| t.text.contains("root body"))
            .expect("the body is painted");
        let status = h
            .texts_starting("Opened")
            .into_iter()
            .next()
            .expect("the status message is painted");
        assert!(
            (title.pos.x - body.pos.x).abs() < 0.6,
            "{size:?}: title {} vs body {}",
            title.pos.x,
            body.pos.x
        );
        assert!(
            (status.pos.x - body.pos.x).abs() < 0.6,
            "{size:?}: status {} vs body {}",
            status.pos.x,
            body.pos.x
        );
        // The axis is the editor's own 16-point padding from the pane edge (plus its separator).
        let from_edge = body.pos.x - h.tree_right();
        assert!(
            (GUTTER - 0.5..=GUTTER + 2.0).contains(&from_edge),
            "{size:?}: {from_edge}"
        );
    }
}

#[test]
fn the_bottom_bars_have_one_height_and_one_top_edge() {
    let d = dir("polish-bars");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let h = H::open_tree(&p);
    // Both bars draw a hairline along their top edge: the same y across the whole window.
    let screen_h = h.screen.y;
    let want = screen_h - BAR_H + 0.5;
    let lines = h.painted().lines;
    let along: Vec<&Line> = lines
        .iter()
        .filter(|l| (l.a.y - want).abs() < 0.01 && l.a.y == l.b.y)
        .collect();
    assert_eq!(along.len(), 2, "tree footer and status bar: {along:?}");
    let min = along
        .iter()
        .map(|l| l.a.x.min(l.b.x))
        .fold(f32::MAX, f32::min);
    let max = along
        .iter()
        .map(|l| l.a.x.max(l.b.x))
        .fold(f32::MIN, f32::max);
    assert!(min <= 0.5 && max >= h.screen.x - 0.5, "{min}..{max}");
    // ... with no gap where the two bars meet.
    let mut spans: Vec<(f32, f32)> = along
        .iter()
        .map(|l| (l.a.x.min(l.b.x), l.a.x.max(l.b.x)))
        .collect();
    spans.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(spans[1].0 <= spans[0].1 + 1.5, "{spans:?}");
}

// --- Tree --------------------------------------------------------------------------------------

#[test]
fn indentation_is_one_step_per_level_and_every_guide_sits_in_its_column() {
    let (_d, _p, h) = open_deep("polish-indent");
    let x_of = |t: &str| h.text_in(t, true).pos.x;
    for (parent, child) in [("A", "A1"), ("A1", "A1a"), ("A1a", "Leaf"), ("A", "A2")] {
        assert!(
            (x_of(child) - x_of(parent) - INDENT).abs() < 0.01,
            "{parent} -> {child}"
        );
    }
    assert!((x_of("A1") - x_of("A2")).abs() < 0.01, "siblings align");
    // Guides: a vertical line in the column of every non-root row's parent level.
    let origin = h.host.tree_geom.0;
    let col_x = |level: usize| {
        (origin.x + TREE_LEFT + level as f32 * INDENT + (INDENT / 2.0).floor()).round() + 0.5
    };
    let lines = h.painted().lines;
    let vertical: Vec<&Line> = lines.iter().filter(|l| l.a.x == l.b.x).collect();
    for (i, row) in (0..h.host.tree().len_visible()).map(|i| (i, h.host.tree().visible_row(i))) {
        if row.depth == 0 {
            continue;
        }
        let top = origin.y + i as f32 * ROW_H;
        let x = col_x(row.depth - 1);
        assert!(
            vertical
                .iter()
                .any(|l| l.a.x == x && (l.a.y - top).abs() < 0.01),
            "{}: elbow at x {x}",
            row.title
        );
    }
    let allowed: Vec<f32> = (0..6).map(col_x).collect();
    for l in &vertical {
        let in_tree = l.a.x < h.tree_right() && l.a.y >= origin.y;
        if in_tree && l.a.y < h.screen.y - BAR_H {
            assert!(
                allowed.contains(&l.a.x),
                "stray vertical line at x {}",
                l.a.x
            );
        }
    }
}

#[test]
fn selected_hovered_and_focused_rows_look_different_and_move_with_their_row() {
    let (_d, _p, mut h) = open_deep("polish-states");
    let ch = h.chrome();
    let surface = colour(h.host.theme_palette().surface);
    let all = [ch.selected_focus, ch.selected_blur, ch.hover_fill, surface];
    for (i, a) in all.iter().enumerate() {
        for b in &all[i + 1..] {
            assert_ne!(a, b, "states must not share a fill");
        }
    }
    // The tree has the keyboard: the selected row (first) is filled strongly, with the accent bar.
    assert!(
        h.row_fill(0, ch.selected_focus).is_some(),
        "focused selection"
    );
    let bar = h
        .painted()
        .fills
        .into_iter()
        .find(|f| f.rect.width() == 2.0 && (f.rect.top() - h.host.tree_geom.0.y).abs() < 0.01)
        .expect("selection bar");
    assert_eq!(bar.fill, ch.accent);
    // Hover another row: it gets the hover fill and nothing else changes.
    let over = h.at("A1a", 0.5);
    h.moved(over);
    assert!(
        h.row_fill(h.row_of("A1a"), ch.hover_fill).is_some(),
        "hover"
    );
    assert!(
        h.row_fill(0, ch.selected_focus).is_some(),
        "selection stays"
    );
    // The keyboard moves to the editor: the selection stays, quieter.
    h.host.focus(Focus::Editor);
    h.moved(Pos2::new(900.0, 400.0));
    h.settle();
    assert!(
        h.row_fill(0, ch.selected_blur).is_some(),
        "unfocused selection"
    );
    assert!(h.row_fill(0, ch.selected_focus).is_none());
    assert!(
        h.row_fill(h.row_of("A1a"), ch.hover_fill).is_none(),
        "hover left"
    );
}

#[test]
fn long_note_titles_are_cut_with_an_ellipsis_inside_the_row() {
    let d = dir("polish-long");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let long = "Very long note title ".repeat(8);
    let a = doc.create_root(long.trim()).unwrap();
    let b = doc
        .create_child(a, &"Nested and also long ".repeat(8))
        .unwrap();
    doc.set_body(b, "body\n").unwrap();
    doc.save().unwrap();
    let mut h = H::open_tree(&p);
    h.host.set_all_expanded(&h.ctx.clone(), true);
    h.settle();
    for prefix in ["Very long", "Nested and"] {
        let row = h
            .texts_starting(prefix)
            .into_iter()
            .find(|t| t.pos.x < h.tree_right())
            .unwrap();
        assert!(row.elided, "{prefix}: cut");
        assert_eq!(row.rows, 1, "{prefix}: one line");
        assert!(
            row.right() <= h.tree_right() + 0.01,
            "{prefix}: inside the pane"
        );
    }
    // The title field shows the long title on one line and stays inside the editor column.
    let title = h
        .texts_starting("Very long")
        .into_iter()
        .find(|t| t.pos.x > h.tree_right())
        .unwrap();
    assert_eq!(title.rows, 1);
    assert!(title.pos.x < h.screen.x);
}

#[test]
fn deep_nesting_keeps_every_row_inside_the_pane() {
    let d = dir("polish-nest");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut parent = doc.create_root("Level 0").unwrap();
    for i in 1..14 {
        parent = doc.create_child(parent, &format!("Level {i}")).unwrap();
    }
    doc.save().unwrap();
    let mut h = H::open_tree(&p);
    h.host.set_all_expanded(&h.ctx.clone(), true);
    h.settle();
    let mut last_x = 0.0;
    for i in 0..14 {
        let t = h.text_in(&format!("Level {i}"), true);
        assert!(t.pos.x > last_x, "indentation grows with depth");
        last_x = t.pos.x;
        assert!(
            t.right() <= h.tree_right() + 0.01,
            "Level {i} fits the pane"
        );
    }
}

#[test]
fn only_the_rows_in_view_are_painted_in_a_large_notebook() {
    let d = dir("polish-virtual");
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
    h.host.set_all_expanded(&h.ctx.clone(), true);
    h.settle();
    assert_eq!(h.host.tree().len_visible(), 6300);
    let rows = h
        .painted()
        .texts
        .into_iter()
        .filter(|t| {
            (t.text.starts_with("Root ") || t.text.starts_with("Child "))
                && t.pos.x < h.tree_right()
        })
        .count();
    let fit = (h.screen.y / ROW_H).ceil() as usize;
    assert!(rows <= fit + 2, "{rows} rows painted for a window of {fit}");
    assert!(rows >= fit / 2);
}

// --- Window sizes -----------------------------------------------------------------------------

#[test]
fn small_windows_keep_a_usable_tree_and_editor() {
    let d = dir("polish-small");
    let p = d.join("n.omatree");
    make_fixture(&p);
    for (w, hgt) in [(520.0, 480.0), (360.0, 300.0), (300.0, 220.0)] {
        let mut h = H::open_sized(&p, Vec2::new(w, hgt));
        h.settle();
        let tree = h.tree_width();
        assert!(tree >= TREE_MIN - 2.0, "{w}: tree {tree} too narrow");
        assert!(
            tree <= (w * TREE_MAX_SHARE).max(TREE_MIN) + 0.5,
            "{w}: tree {tree}"
        );
        let editor = w - tree;
        assert!(editor >= 120.0, "{w}: editor {editor}");
        let title = h.text_in("Projects", false);
        assert!(
            title.pos.x < w && title.right() <= w + 0.5,
            "{w}: title inside"
        );
        assert!(h.host.selected().is_some());
    }
    // A window that shrinks keeps the tree within its limits too.
    let mut h = H::open(&p);
    h.settle();
    assert!(h.tree_width() <= 260.5);
    h.screen = Vec2::new(360.0, 300.0);
    h.settle();
    assert!(h.tree_width() <= (360.0 * TREE_MAX_SHARE).max(TREE_MIN) + 0.5);
}

#[test]
fn a_long_status_message_wraps_to_two_lines_and_the_rest_is_a_tooltip_away() {
    let d = dir("polish-status");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open_tree(&p);
    h.host.status = "Autosave failed: disk is full ".repeat(30);
    h.settle();
    let t = h.texts_starting("Autosave failed").pop().unwrap();
    assert!(t.rows <= 2, "{} rows", t.rows);
    assert!(t.elided);
    assert!(t.right() <= h.screen.x + 0.5);
    // A failure is shown as one.
    assert_eq!(t.colour, h.chrome().danger_text);
    // A short ordinary message is not.
    h.host.status = "Saved".to_owned();
    h.settle();
    let t = h.texts_starting("Saved").pop().unwrap();
    assert_eq!(t.colour, h.chrome().secondary);
}

#[test]
fn action_feedback_fades_but_failures_stay_longer_and_state_markers_stay() {
    let d = dir("polish-fade");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    assert_eq!(h.texts_starting("Opened").len(), 1, "fresh");
    h.advance(3.0);
    assert_eq!(h.texts_starting("Opened").len(), 1, "still there");
    h.advance(4.0);
    assert!(
        h.texts_starting("Opened").is_empty(),
        "faded after a few seconds"
    );
    // The message itself is kept for anyone who asks.
    assert!(h.host.status().starts_with("Opened"));
    // A failure outlives ordinary feedback.
    h.host.status = "Could not move the note: no".to_owned();
    h.settle();
    h.advance(8.0);
    assert_eq!(h.texts_starting("Could not move").len(), 1);
    h.advance(10.0);
    assert!(h.texts_starting("Could not move").is_empty());
    // The document's state is not feedback: it is shown for as long as it is true, however old
    // the last message is, and goes when the note is saved.
    h.host.autosave_blocked = true; // keep the edit unsaved
    h.dirty_edit("Projects", "x");
    h.advance(30.0);
    assert!(h.host.is_dirty());
    assert_eq!(h.texts_starting("\u{25cf} Unsaved changes").len(), 1);
    h.host.autosave_blocked = false;
    h.key(Key::S, CTRL);
    h.settle();
    assert!(!h.host.is_dirty());
    assert!(h.texts_starting("\u{25cf}").is_empty());
}

#[test]
fn the_status_bar_separates_document_state_from_what_just_happened() {
    let d = dir("polish-state");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    // Clean: no "no unsaved changes" chatter.
    assert!(h.texts_starting("\u{25cf}").is_empty());
    assert!(h.texts_starting("No unsaved").is_empty());
    assert!(h.texts_starting("Images are not").is_empty());
    // Dirty: one quiet marker on the right.
    h.dirty_edit("Projects", "more ");
    let marker = h.texts_starting("\u{25cf} Unsaved changes");
    assert_eq!(marker.len(), 1);
    assert!(marker[0].pos.x > h.screen.x / 2.0);
    assert_eq!(marker[0].colour, h.chrome().warning_text);
    // Images are supported now (E10): a note that mentions one gets no capability notice, wide or
    // narrow window.
    h.dirty_edit("Projects", "\n![alt](attachment:x)\n");
    assert!(h.texts_starting("Images are not").is_empty());
    h.screen = Vec2::new(520.0, 480.0);
    h.settle();
    assert!(h.texts_starting("Images are not").is_empty());
    // An autosave that stopped is called what it is.
    h.host.autosave_error = Some("disk is full".to_owned());
    h.settle();
    let stopped = h.texts_starting("\u{25cf} Autosave stopped");
    assert_eq!(stopped.len(), 1);
    assert_eq!(stopped[0].colour, h.chrome().danger_text);
}

#[test]
fn a_refused_title_is_shown_beside_the_title_once() {
    let d = dir("polish-title-error");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open_tree(&p);
    h.host.focus(Focus::Title);
    h.settle();
    h.frame(
        vec![Event::Key {
            key: Key::A,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: CTRL,
        }],
        CTRL,
    );
    h.type_text("Journal"); // a sibling's name
    h.press(Key::Enter);
    h.settle();
    assert!(matches!(h.host.dialog(), Dialog::Error(_)));
    h.press(Key::Enter); // OK
    h.settle();
    assert!(matches!(h.host.dialog(), Dialog::None));
    let msg = "A note named \"Journal\" already exists here.";
    let shown = h.texts_starting(msg);
    assert_eq!(shown.len(), 1, "once, not also in the status bar");
    assert_eq!(shown[0].colour, h.chrome().danger_text);
    // It sits between the title field and the editor, on the shared axis.
    let title = h.text_in("Journal", false);
    assert!(shown[0].pos.y > title.pos.y);
    assert!((shown[0].pos.x - title.pos.x).abs() < 0.6);
    // Editing the title takes the message away from the title (the status bar keeps saying what
    // happened last, as it does for any action).
    h.type_text("s");
    let limit = h.screen.y - BAR_H;
    assert!(h.texts_starting(msg).iter().all(|t| t.pos.y > limit));
}

// --- Colour ---------------------------------------------------------------------------------------

fn palettes() -> Vec<(&'static str, Core)> {
    let grey = |g: u8| Rgb(g, g, g);
    vec![
        ("dark", Core::DARK),
        ("light", Core::LIGHT),
        ("tokyo", core_of(TOKYO)),
        ("latte", core_of(LATTE)),
        ("low contrast", core_of(LOW)),
        (
            "mid-grey danger",
            Core {
                danger: grey(0x80),
                danger_foreground: grey(0x88),
                ..Core::DARK
            },
        ),
        (
            "faint everything",
            Core {
                foreground: grey(0x55),
                muted_foreground: grey(0x45),
                accent: grey(0x50),
                danger: grey(0x58),
                danger_foreground: grey(0x5c),
                warning: grey(0x4c),
                ..Core::DARK
            },
        ),
    ]
}

#[test]
fn the_destructive_button_reads_in_every_palette() {
    for (name, p) in palettes() {
        let c = Chrome::of(&p);
        let ratio = contrast(c.on_danger, c.danger_fill);
        assert!(ratio >= TEXT_MIN, "{name}: {ratio:.2}:1");
    }
    // The case that prompted this: the Tokyo-style label colour on its red measured 1.64:1.
    let tokyo = core_of(TOKYO);
    assert!(contrast(colour(tokyo.danger_foreground), colour(tokyo.danger)) < 2.0);
    assert!(contrast(Chrome::of(&tokyo).on_danger, colour(tokyo.danger)) >= TEXT_MIN);
    // A palette that already reads keeps its own label colour.
    let dark = Core::DARK;
    if contrast(colour(dark.danger_foreground), colour(dark.danger)) >= TEXT_MIN {
        assert_eq!(Chrome::of(&dark).on_danger, colour(dark.danger_foreground));
    }
}

#[test]
fn chrome_text_selected_rows_and_hints_stay_legible_in_every_palette() {
    for (name, p) in palettes() {
        let c = Chrome::of(&p);
        let surfaces = [p.background, p.surface, p.surface_raised].map(colour);
        for s in surfaces {
            assert!(contrast(c.text, s) >= TEXT_MIN, "{name}: text on {s:?}");
            assert!(
                contrast(c.secondary, s) >= TEXT_MIN,
                "{name}: secondary on {s:?}"
            );
            assert!(contrast(c.muted, s) >= MUTED_MIN, "{name}: muted on {s:?}");
            assert!(
                contrast(c.danger_text, s) >= TEXT_MIN,
                "{name}: error text on {s:?}"
            );
            assert!(
                contrast(c.warning_text, s) >= TEXT_MIN,
                "{name}: warning on {s:?}"
            );
            assert!(
                contrast(c.accent, s) >= MUTED_MIN,
                "{name}: accent on {s:?}"
            );
        }
        for fill in [c.selected_focus, c.selected_blur] {
            assert!(
                contrast(c.selected_text, fill) >= TEXT_MIN,
                "{name}: selected row"
            );
        }
        assert!(
            contrast(c.text, c.hover_fill) >= TEXT_MIN,
            "{name}: hovered row"
        );
        // Hover and selection are visible against the tree's surface, not just against each other.
        let surface = colour(p.surface);
        assert_ne!(c.hover_fill, surface, "{name}");
        assert_ne!(c.selected_blur, surface, "{name}");
        // Disabled menu entries and flat actions: still readable (egui fades their text).
        let v = visuals_of(&p);
        let faded = v.gray_out(c.text);
        assert!(
            contrast(faded, colour(p.surface_raised)) >= 2.0,
            "{name}: disabled entry {:.2}:1",
            contrast(faded, colour(p.surface_raised))
        );
    }
}

fn visuals_of(p: &Core) -> egui::Visuals {
    crate::theme::visuals(p, &crate::theme::editor_palette(p))
}

#[test]
fn readable_changes_only_what_does_not_read() {
    let bg = Color32::from_rgb(0x1a, 0x1b, 0x26);
    let fine = Color32::from_rgb(0xc0, 0xca, 0xf5);
    assert_eq!(readable(fine, &[bg], TEXT_MIN), fine);
    let faint = Color32::from_rgb(0x41, 0x48, 0x68);
    let fixed = readable(faint, &[bg], TEXT_MIN);
    assert_ne!(fixed, faint);
    assert!(contrast(fixed, bg) >= TEXT_MIN);
    // It moves toward the colour it came from, not to a different hue: blue stays the largest.
    assert!(fixed.b() >= fixed.r());
}

#[test]
fn every_dialog_button_row_uses_the_palette_and_the_dialog_survives_a_theme_change() {
    let (d, p) = rich("polish-dialog-theme");
    let mut h = H::open_tree(&p);
    let colours = install(&d, TOKYO);
    h.follow(&colours);
    h.press(Key::Delete);
    assert!(matches!(h.host.dialog(), Dialog::ConfirmDelete(_)));
    h.advance(0.5); // the modal fades in
    let tokyo = core_of(TOKYO);
    let label = |h: &H| {
        h.texts_starting("Move to Trash")
            .into_iter()
            .find(|t| t.colour != h.chrome().text)
            .expect("the destructive button's label")
    };
    let before = label(&h);
    assert_eq!(before.colour, Chrome::of(&tokyo).on_danger);
    assert!(h
        .painted()
        .fills
        .iter()
        .any(|f| f.fill == colour(tokyo.danger) && f.rect.contains(before.pos)));
    // The theme changes under the open question.
    install(&d, LATTE);
    h.polls(3);
    assert!(
        matches!(h.host.dialog(), Dialog::ConfirmDelete(_)),
        "still asking"
    );
    let latte = core_of(LATTE);
    let after = label(&h);
    assert_eq!(after.colour, Chrome::of(&latte).on_danger);
    assert!(contrast(after.colour, colour(latte.danger)) >= TEXT_MIN);
    // Nothing was deleted and nothing became dirty.
    assert_eq!(h.host.rows().len(), 3);
    assert!(!h.host.is_dirty());
    h.press(Key::Escape);
    assert!(matches!(h.host.dialog(), Dialog::None));
}

// --- Keyboard, menus and dialogs ----------------------------------------------------------------

#[test]
fn keys_and_clicks_still_reach_the_rows_they_are_painted_on() {
    let (_d, _p, mut h) = open_deep("polish-keys");
    assert_eq!(h.sel(), "A");
    h.press(Key::ArrowDown);
    h.press(Key::ArrowDown);
    assert_eq!(h.sel(), "A1a");
    h.press(Key::ArrowLeft); // A1a is closed already? it has a child: collapses it
    h.press(Key::ArrowUp);
    assert_eq!(h.sel(), "A1");
    // A click on a row's painted text selects exactly that row.
    for title in ["B1", "C", "A2"] {
        let t = h.text_in(title, true);
        let at = Pos2::new(t.pos.x + 4.0, t.centre_y());
        h.press_at(at);
        h.release_at(at);
        assert_eq!(h.sel(), title);
    }
    // A click on the disclosure marker toggles, on the row elsewhere selects only.
    let before = h.expanded("B");
    let t = h.text_in("B", true);
    let marker = Pos2::new(t.pos.x - INDENT / 2.0 - 2.0, t.centre_y());
    h.press_at(marker);
    h.release_at(marker);
    assert_ne!(h.expanded("B"), before, "the triangle toggles");
}

#[test]
fn dialogs_open_on_the_safe_answer_and_escape_enter_and_tab_work_as_before() {
    let (_d, _p, mut h) = open_deep("polish-dialog-keys");
    h.press(Key::ArrowDown);
    let before = shape(h.host.document().unwrap());
    // Enter on a fresh delete question is "Cancel".
    h.press(Key::Delete);
    assert!(matches!(h.host.dialog(), Dialog::ConfirmDelete(_)));
    h.press(Key::Enter);
    assert!(matches!(h.host.dialog(), Dialog::None));
    assert_eq!(shape(h.host.document().unwrap()), before);
    // Escape cancels.
    h.press(Key::Delete);
    h.press(Key::Escape);
    assert!(matches!(h.host.dialog(), Dialog::None));
    assert_eq!(shape(h.host.document().unwrap()), before);
    // Tab then Enter is the deliberate "Move to Trash".
    h.press(Key::Delete);
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert!(matches!(h.host.dialog(), Dialog::None));
    assert_ne!(shape(h.host.document().unwrap()), before);
    // The unsaved-changes question opens on Save; Discard needs a second, explicit answer.
    h.dirty_edit("C", "x");
    assert!(!h.request_close());
    assert!(matches!(h.host.dialog(), Dialog::Unsaved { .. }));
    h.press(Key::Escape);
    assert!(matches!(h.host.dialog(), Dialog::None));
    assert!(!h.closed);
}

#[test]
fn the_context_menu_opens_from_the_keyboard_and_its_danger_entry_is_marked_by_words_too() {
    let (_d, _p, mut h) = open_deep("polish-menu");
    h.key(Key::F10, Modifiers::SHIFT);
    h.settle();
    assert!(egui::Popup::is_any_open(&h.ctx));
    let trash = h.texts_starting("Move to Trash");
    assert_eq!(trash.len(), 1);
    assert_eq!(trash[0].colour, h.chrome().danger_text);
    h.press(Key::Escape);
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(h.sel(), "A");
}

// --- Drag and drop ------------------------------------------------------------------------------

#[test]
fn drop_indicators_keep_their_meaning_and_sit_on_the_row_edges() {
    let (_d, _p, mut h) = open_deep("polish-drop");
    let ch = h.chrome();
    let origin = h.host.tree_geom.0;
    let width = h.host.tree_geom.1.width();

    // Before a row: a two-point line along its top edge, in the accent colour.
    let to = h.at("A2", 0.05);
    h.pick_up("C", to);
    let t = h.target().expect("a target");
    assert_eq!(t.row, h.row_of("A2"));
    let y = origin.y + t.row as f32 * ROW_H;
    let line = h
        .painted()
        .fills
        .into_iter()
        .find(|f| f.fill == ch.accent && (f.rect.height() - 2.0).abs() < 0.01)
        .expect("a before-line");
    assert!(
        (line.rect.center().y - y).abs() < 0.01,
        "on the row's top edge"
    );
    assert!((line.rect.left() - origin.x).abs() < 0.01 && (line.rect.width() - width).abs() < 0.5);
    h.release_at(Pos2::new(5.0, 5.0)); // outside the tree: nothing happens
    assert_eq!(shape(h.host.document().unwrap()), ALL);

    // Inside a row: an outline of the whole row.
    let to = h.at("B", 0.5);
    h.pick_up("C", to);
    let outline = h
        .painted()
        .fills
        .into_iter()
        .find(|f| f.stroke.color == ch.accent && f.stroke.width == 2.0)
        .expect("an inside outline");
    assert!((outline.rect.height() - (ROW_H - 2.0)).abs() < 0.01);
    h.release_at(Pos2::new(5.0, 5.0));

    // Refused (a note into its own subtree): the same outline in the danger colour.
    let to = h.at("A1a", 0.5);
    h.pick_up("A", to);
    let refused = h
        .painted()
        .fills
        .into_iter()
        .find(|f| f.stroke.color == ch.danger_text && f.stroke.width == 2.0)
        .expect("a refusal outline");
    assert!((refused.rect.height() - (ROW_H - 2.0)).abs() < 0.01);
    h.release_at(Pos2::new(5.0, 5.0));
    assert_eq!(shape(h.host.document().unwrap()), ALL);
}

#[test]
fn a_whole_drag_and_drop_still_moves_the_note_after_the_polish() {
    let (_d, _p, mut h) = open_deep("polish-drag-moves");
    let to = h.at("A2", 0.9);
    h.drag("C", to);
    assert_eq!(
        shape(h.host.document().unwrap()),
        "A(A1(A1a(Leaf)),A2,C)|B(B1)"
    );
    assert!(h.host.is_dirty());
}

// --- Styling changes nothing but styling ---------------------------------------------------------

#[test]
fn hovering_resizing_menus_and_theme_changes_never_touch_content_or_dirty_state() {
    let (d, p) = rich("polish-inert");
    let mut h = H::open_tree(&p);
    let on_disk = fs::read(&p).unwrap();
    let buffer = h.host.buffer().to_owned();
    h.host.focus(Focus::Editor);
    h.settle();
    h.set_caret(2, 9);
    h.settle();
    let selection = h.caret("Rich");
    assert_ne!(selection.0, selection.1, "a real range");
    let colours = install(&d, TOKYO);
    h.follow(&colours);
    // Hover every row, open and close a menu, resize, switch theme.
    for i in 0..3 {
        h.moved(Pos2::new(
            60.0,
            h.host.tree_geom.0.y + (i as f32 + 0.5) * ROW_H,
        ));
    }
    h.moved(Pos2::new(900.0, 300.0));
    for size in [Vec2::new(520.0, 400.0), Vec2::new(1000.0, 700.0)] {
        h.screen = size;
        h.settle();
    }
    install(&d, LATTE);
    h.polls(3);
    install(&d, TOKYO);
    h.polls(3);
    assert_eq!(h.host.buffer(), buffer);
    assert!(!h.host.is_dirty());
    assert_eq!(fs::read(&p).unwrap(), on_disk, "nothing was written");
    assert_eq!(
        h.caret("Rich"),
        selection,
        "the editor's selection is untouched"
    );
    // The Markdown source is still what the editor was given, byte for byte.
    assert_eq!(h.body("Rich"), buffer);
}

#[test]
fn the_editor_keeps_its_own_padding_and_renders_the_rich_note_in_a_small_window() {
    let (_d, p) = rich("polish-rich");
    let mut wide = H::open(&p);
    wide.settle();
    let mut narrow = H::open_sized(&p, Vec2::new(520.0, 480.0));
    narrow.settle();
    for h in [&wide, &narrow] {
        let painted = h.painted();
        // Headings, table cells and task text are painted by the editor, at the editor's axis.
        for want in ["Heading one", "Heading two", "Název", "jablka"] {
            assert!(
                painted.texts.iter().any(|t| t.text.contains(want)),
                "{want} is painted"
            );
        }
        let heading = painted
            .texts
            .iter()
            .find(|t| t.text.contains("Heading one"))
            .unwrap();
        let axis = h.tree_right() + GUTTER;
        assert!(
            heading.pos.x >= axis - 0.5 && heading.pos.x <= axis + 40.0,
            "heading at {} (axis {axis})",
            heading.pos.x
        );
    }
}

#[test]
fn the_sidebar_actions_are_flat_text_reachable_by_keyboard_and_disabled_means_muted() {
    let (_d, _p, mut h) = open_deep("polish-actions");
    let ch = h.chrome();
    let note = h.text_in("+ note", true);
    let child = h.text_in("+ child", true);
    assert_eq!(note.colour, ch.secondary);
    assert_eq!(child.colour, ch.secondary);
    // No button frames: nothing button-sized is filled behind them while idle (the bar's own
    // background is the only fill under them).
    let small = h
        .painted()
        .fills
        .iter()
        .filter(|f| {
            f.rect.contains(note.pos) && f.fill != Color32::TRANSPARENT && f.rect.width() < 120.0
        })
        .count();
    assert_eq!(small, 0, "a flat action has no box");
    // Shift+Tab from the tree reaches "file", then "+ child"; Enter does what its click does.
    let n = h.host.rows().len();
    h.key(Key::Tab, Modifiers::SHIFT);
    h.key(Key::Tab, Modifiers::SHIFT);
    h.press(Key::Enter);
    h.settle();
    assert_eq!(h.host.rows().len(), n + 1, "'+ child' by keyboard");
}

#[test]
fn with_nothing_selected_the_child_action_is_muted_and_does_nothing() {
    let d = dir("polish-empty");
    let p = d.join("n.omatree");
    Document::open(&p).unwrap().save().unwrap();
    let mut h = H::open_tree(&p);
    assert!(h.host.rows().is_empty());
    let ch = h.chrome();
    assert_eq!(h.texts_starting("+ child").pop().unwrap().colour, ch.muted);
    assert_eq!(
        h.texts_starting("+ note").pop().unwrap().colour,
        ch.secondary
    );
    let t = h.texts_starting("+ child").pop().unwrap();
    let at = Pos2::new(t.pos.x + 4.0, t.centre_y());
    h.press_at(at);
    h.release_at(at);
    assert!(h.host.rows().is_empty(), "a disabled action does nothing");
    // The way in is still offered.
    assert!(!h.texts_starting("+ create the first note").is_empty());
}
