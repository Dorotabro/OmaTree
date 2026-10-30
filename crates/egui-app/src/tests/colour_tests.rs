//! T1: colour tags in the egui application: the picker, the tree's marks, persistence through the
//! host's own lifecycle, and the guards. Disposable notebooks throughout.

use super::tree_tests::{deep, shape};
use super::*;
use egui::accesskit::Role;
use omatree_core::notebook::Colour;

/// The label of the widget that has the keyboard, from the accessibility update.
pub(super) fn focused_label(h: &H) -> String {
    let u = h.a11y.as_ref().expect("enable_accesskit first");
    u.nodes
        .iter()
        .find(|(id, _)| *id == u.focus)
        .and_then(|(_, n)| n.label().map(|l| l.trim().to_owned()))
        .unwrap_or_else(|| "<none>".into())
}

/// The accessible name of the tree row for `title` (its label), if it is on screen.
fn row_name(h: &H, title: &str) -> Option<String> {
    let u = h.a11y.as_ref()?;
    u.nodes
        .iter()
        .filter(|(_, n)| n.role() == Role::TreeItem)
        .filter_map(|(_, n)| n.label())
        .find(|l| *l == title || l.starts_with(&format!("{title},")))
        .map(str::to_owned)
}

pub(super) fn opened(path: &Path) -> H {
    let mut h = H::open_tree(path);
    h.ctx.enable_accesskit();
    h.settle();
    h
}

impl H {
    /// Shift+F10, then Tab until the widget called `label` has the keyboard.
    fn menu_to(&mut self, label: &str) {
        self.key(Key::F10, Modifiers::SHIFT);
        self.settle();
        for _ in 0..24 {
            if focused_label(self) == label {
                return;
            }
            self.press(Key::Tab);
        }
        panic!("never reached {label:?}; at {:?}", focused_label(self));
    }

    /// The whole keyboard route: open the note's menu, reach the swatch, press Enter.
    pub(super) fn pick_by_keyboard(&mut self, label: &str) {
        self.menu_to(label);
        self.press(Key::Enter);
        self.settle();
    }

    pub(super) fn colour_of(&self, title: &str) -> Option<Colour> {
        self.host.tree().row(self.id(title)).unwrap().colour
    }

    pub(super) fn doc_colour(&self, title: &str) -> Option<Colour> {
        self.host
            .document()
            .unwrap()
            .notebook()
            .get(self.id(title))
            .unwrap()
            .colour()
    }
}

#[test]
fn a_new_notebook_has_no_tags_and_untagged_rows_paint_no_marks() {
    let (_d, p) = deep("t1-none");
    let h = opened(&p);
    for r in h.host.rows() {
        assert_eq!((r.colour, r.below), (None, 0), "{}", r.title);
        assert_eq!(
            tree_ui::tag_gutter(r, false),
            0.0,
            "an untagged row is as it was"
        );
    }
    assert!(h.painted().dots.is_empty(), "nothing coloured is drawn");
}

#[test]
fn the_picker_assigns_each_colour_from_the_keyboard_alone() {
    for colour in Colour::ALL {
        let (_d, p) = deep("t1-each");
        let mut h = opened(&p);
        h.go("A2");
        h.pick_by_keyboard(colour.label());
        assert_eq!(h.colour_of("A2"), Some(colour), "{colour:?}");
        assert_eq!(h.doc_colour("A2"), Some(colour), "the Document holds it");
        assert!(!egui::Popup::is_any_open(&h.ctx), "the menu closed");
        assert_eq!(h.sel(), "A2", "the same note is still selected");
        assert!(h.has_focus(Focus::Tree), "focus is back on the tree");
        assert_eq!(h.host.status(), format!("Colour: {}", colour.label()));
    }
}

#[test]
fn replacing_and_clearing_work_and_no_colour_is_distinct_from_the_colours() {
    let (_d, p) = deep("t1-replace");
    let mut h = opened(&p);
    h.go("B");
    h.pick_by_keyboard("Green");
    h.pick_by_keyboard("Purple");
    assert_eq!(h.colour_of("B"), Some(Colour::Purple), "replaced");
    // "No colour" is a command, not a swatch: its own label, its own role.
    h.menu_to("No colour");
    h.press(Key::Enter);
    h.settle();
    assert_eq!(h.colour_of("B"), None, "cleared");
    assert_eq!(h.host.status(), "Colour cleared");
    // With nothing to clear the command is off, so Tab passes it by.
    h.key(Key::F10, Modifiers::SHIFT);
    h.settle();
    let mut seen = Vec::new();
    for _ in 0..24 {
        h.press(Key::Tab);
        seen.push(focused_label(&h));
    }
    assert!(!seen.iter().any(|l| l == "No colour"), "{seen:?}");
}

#[test]
fn arrow_keys_move_between_swatches_and_enter_picks() {
    let (_d, p) = deep("t1-arrows");
    let mut h = opened(&p);
    h.go("C");
    h.menu_to("Red");
    h.press(Key::ArrowRight);
    h.press(Key::ArrowRight);
    assert_eq!(focused_label(&h), "Yellow");
    h.press(Key::ArrowLeft);
    assert_eq!(focused_label(&h), "Orange");
    h.press(Key::ArrowLeft);
    h.press(Key::ArrowLeft);
    assert_eq!(focused_label(&h), "Red", "the row stops at its ends");
    for _ in 0..8 {
        h.press(Key::ArrowRight);
    }
    assert_eq!(focused_label(&h), "Purple");
    h.press(Key::Enter);
    h.settle();
    assert_eq!(h.colour_of("C"), Some(Colour::Purple));
    assert!(h.has_focus(Focus::Tree));
}

#[test]
fn escape_closes_the_picker_and_changes_nothing() {
    let (_d, p) = deep("t1-escape");
    let mut h = opened(&p);
    h.go("A1");
    h.pick_by_keyboard("Blue");
    h.advance(1.5);
    assert!(!h.host.is_dirty());
    h.menu_to("Green");
    h.press(Key::Escape);
    h.settle();
    assert!(!egui::Popup::is_any_open(&h.ctx));
    assert_eq!(h.colour_of("A1"), Some(Colour::Blue), "still Blue");
    assert!(!h.host.is_dirty(), "nothing was modified");
    assert!(h.has_focus(Focus::Tree));
    assert_eq!(h.sel(), "A1");
}

#[test]
fn a_tag_edit_autosaves_and_survives_reopening_the_file() {
    let (_d, p) = deep("t1-autosave");
    let mut h = opened(&p);
    h.go("B1");
    h.pick_by_keyboard("Orange");
    assert!(h.host.is_dirty(), "a tag edit is a modification");
    h.advance(1.5);
    assert!(!h.host.is_dirty(), "autosaved like any other edit");
    drop(h);
    let re = Document::open_existing(&p).unwrap();
    let b1 = re
        .notebook()
        .depth_first()
        .into_iter()
        .find(|n| n.title() == "B1")
        .unwrap();
    assert_eq!(b1.colour(), Some(Colour::Orange));
    let h2 = H::open_tree(&p);
    assert_eq!(
        h2.colour_of("B1"),
        Some(Colour::Orange),
        "and the tree shows it after a restart"
    );
}

#[test]
fn quitting_right_after_a_tag_edit_cannot_bypass_the_dirty_guard() {
    // The window's close request.
    let (_d, p) = deep("t1-close");
    let mut h = opened(&p);
    h.go("A");
    h.pick_by_keyboard("Red");
    assert!(h.host.is_dirty());
    assert!(!h.request_close(), "closing asks first");
    assert!(matches!(h.host.dialog(), Dialog::Unsaved { .. }));
    // Discard needs its confirmation and loses only the tag.
    h.host.cancel_dialog();
    h.settle();

    // The operating system's quit (macOS Command+Q), in the same frame as the edit.
    let (_d2, p2) = deep("t1-quit");
    let mut g = opened(&p2);
    g.go("A");
    let answers = g.watch_quit();
    g.pick_by_keyboard("Red");
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
fn rename_move_and_reorder_keep_the_tag_in_the_tree() {
    let (_d, p) = deep("t1-rename");
    let mut h = opened(&p);
    h.go("B");
    h.pick_by_keyboard("Yellow");
    h.go("B");
    h.key(Key::ArrowUp, Modifiers::ALT); // reorder among the roots
    assert_eq!(h.colour_of("B"), Some(Colour::Yellow));
    h.host.focus(Focus::Title);
    h.settle();
    h.type_text("Bee");
    h.key(Key::Enter, Modifiers::NONE);
    h.settle();
    assert_eq!(h.colour_of("Bee"), Some(Colour::Yellow), "renamed");
    assert_eq!(
        shape(h.host.document().unwrap()),
        "Bee(B1)|A(A1(A1a(Leaf)),A2)|C"
    );
}

#[test]
fn trash_and_restore_keep_the_whole_subtrees_tags() {
    let (_d, p) = deep("t1-trash");
    let mut h = opened(&p);
    h.go("A");
    h.pick_by_keyboard("Green");
    h.go("A1a");
    h.pick_by_keyboard("Purple");
    h.go("A");
    let ctx = h.ctx.clone();
    let id = h.id("A");
    h.host.confirm_delete(&ctx, id);
    h.settle();
    assert_eq!(shape(h.host.document().unwrap()), "B(B1)|C");
    assert!(h.host.restore_trash(&ctx, 0));
    h.settle();
    assert_eq!(h.colour_of("A"), Some(Colour::Green));
    assert_eq!(h.colour_of("A1a"), Some(Colour::Purple));
    assert_eq!(
        h.colour_of("A1"),
        None,
        "the note between them was never tagged"
    );
}

#[test]
fn a_checkpoint_restores_the_tags_it_held() {
    let (_d, p) = deep("t1-checkpoint");
    let mut h = opened(&p);
    h.go("C");
    h.pick_by_keyboard("Blue");
    h.host.doc.as_mut().unwrap().create_checkpoint("tagged");
    let at = h.host.document().unwrap().recovery().checkpoints().len() - 1;
    h.pick_by_keyboard("Red");
    assert_eq!(h.colour_of("C"), Some(Colour::Red));
    let ctx = h.ctx.clone();
    assert!(h.host.restore_checkpoint(&ctx, at));
    h.settle();
    assert_eq!(h.colour_of("C"), Some(Colour::Blue));
}

#[test]
fn parent_and_child_tags_are_independent_and_a_closed_parent_only_hints_at_its_children() {
    let (_d, p) = deep("t1-below");
    let mut h = opened(&p);
    h.go("A2");
    h.pick_by_keyboard("Blue");
    h.go("A");
    h.pick_by_keyboard("Red");
    let a = h.host.tree().row(h.id("A")).unwrap().clone();
    assert_eq!(a.colour, Some(Colour::Red), "its own tag");
    assert_eq!(
        a.below,
        tree::colour_bit(Colour::Blue),
        "what is inside is not its own"
    );
    assert_eq!(h.colour_of("A2"), Some(Colour::Blue));
    let a1 = h.host.tree().row(h.id("A1")).unwrap();
    assert_eq!(
        (a1.colour, a1.below),
        (None, 0),
        "a sibling branch is unaffected"
    );
}

#[test]
fn a_closed_branch_shows_a_small_cue_and_an_open_one_shows_the_notes_themselves() {
    let (_d, p) = deep("t1-cue");
    let mut h = opened(&p);
    let ctx = h.ctx.clone();
    h.go("A1a");
    h.pick_by_keyboard("Purple"); // under A > A1 > A1a: A1a's own child "Leaf" stays untagged
    h.go("Leaf");
    h.pick_by_keyboard("Green");
    let chrome = h.chrome();
    // Everything open: two dots of radius 4 (A1a, Leaf), no cues.
    let big = |h: &H| h.painted().dots.iter().filter(|d| d.radius > 3.0).count();
    let mini = |h: &H| h.painted().dots.iter().filter(|d| d.radius < 3.0).count();
    assert_eq!((big(&h), mini(&h)), (2, 0));
    // Close A1a: its own purple dot stays, and a tiny green cue appears beside it for Leaf.
    let a1a = h.id("A1a");
    h.host.set_expanded(&ctx, a1a, false);
    h.settle();
    let dots = h.painted().dots;
    assert_eq!(dots.iter().filter(|d| d.radius > 3.0).count(), 1);
    let cue: Vec<_> = dots.iter().filter(|d| d.radius < 3.0).collect();
    assert_eq!(cue.len(), 1);
    assert_eq!(cue[0].fill, chrome.tag(Colour::Green));
    let own = dots.iter().find(|d| d.radius > 3.0).unwrap();
    assert_eq!(own.fill, chrome.tag(Colour::Purple));
    assert!(
        cue[0].centre.x < own.centre.x,
        "the cue sits left of the parent's own dot"
    );
    // Close A as well: A shows only a cue (A1a's purple and Leaf's green), never a dot of its own.
    let a = h.id("A");
    h.host.set_expanded(&ctx, a, false);
    h.settle();
    let dots = h.painted().dots;
    assert_eq!(
        dots.iter().filter(|d| d.radius > 3.0).count(),
        0,
        "A has no tag of its own"
    );
    let mut fills: Vec<_> = dots.iter().map(|d| d.fill).collect();
    fills.sort_by_key(|c| c.to_array());
    let mut want = vec![chrome.tag(Colour::Purple), chrome.tag(Colour::Green)];
    want.sort_by_key(|c| c.to_array());
    assert_eq!(fills, want);
    // After clearing the tags beneath, the cue goes.
    h.host.set_expanded(&ctx, a, true);
    h.host.set_expanded(&ctx, a1a, true);
    h.settle();
    for t in ["A1a", "Leaf"] {
        h.go(t);
        h.menu_to("No colour");
        h.press(Key::Enter);
        h.settle();
    }
    h.host.set_expanded(&ctx, a, false);
    h.settle();
    assert!(h.painted().dots.is_empty());
}

#[test]
fn tags_follow_moves_deletes_and_restores_in_the_cue() {
    let (_d, p) = deep("t1-cue-move");
    let mut h = opened(&p);
    h.go("A2");
    h.pick_by_keyboard("Red");
    assert_ne!(h.host.tree().row(h.id("A")).unwrap().below, 0);
    // Move the tagged note under B: A no longer has it beneath it, B does.
    let ctx = h.ctx.clone();
    let (a2, b) = (h.id("A2"), h.id("B"));
    h.host.drop_note(&ctx, a2, Some(b), 0);
    h.settle();
    assert_eq!(h.host.tree().row(h.id("A")).unwrap().below, 0);
    assert_eq!(
        h.host.tree().row(h.id("B")).unwrap().below,
        tree::colour_bit(Colour::Red)
    );
    // Delete it: B is plain again; restore: tagged again.
    h.host.confirm_delete(&ctx, a2);
    h.settle();
    assert_eq!(h.host.tree().row(h.id("B")).unwrap().below, 0);
    assert!(h.host.restore_trash(&ctx, 0));
    h.settle();
    assert_eq!(
        h.host.tree().row(h.id("B")).unwrap().below,
        tree::colour_bit(Colour::Red)
    );
}

#[test]
fn a_tag_edit_leaves_the_editor_search_and_markdown_alone() {
    let (_d, p) = deep("t1-untouched");
    let mut h = opened(&p);
    h.select("A1");
    let end = h.host.buffer().len();
    h.set_caret(1, end.min(4));
    h.host.start_search();
    h.settle();
    h.type_text("leaf");
    h.settle();
    let results = h.host.search_query().to_owned();
    // Tag it while the search is showing (the keyboard route is tested above).
    let (caret, body) = (h.caret("A1"), h.body("A1"));
    let a1 = h.id("A1");
    h.host.set_colour(a1, Some(Colour::Blue));
    h.settle();
    assert_eq!(
        h.caret("A1"),
        caret,
        "the caret and selection are where they were"
    );
    assert_eq!(
        h.body("A1"),
        body,
        "the Markdown source is byte for byte the same"
    );
    assert_eq!(h.host.search_query(), results, "Search is untouched");
    assert!(
        !h.host
            .document()
            .unwrap()
            .notebook()
            .get(h.id("A1"))
            .unwrap()
            .body()
            .contains("blue"),
        "no marker in the note text"
    );
}

#[test]
fn accessible_names_say_the_colour_and_the_hidden_tags() {
    let (_d, p) = deep("t1-names");
    let mut h = opened(&p);
    assert_eq!(
        row_name(&h, "A").as_deref(),
        Some("A"),
        "untagged rows keep their plain name"
    );
    h.go("A1");
    h.pick_by_keyboard("Orange");
    h.go("A1");
    h.settle();
    assert_eq!(row_name(&h, "A1").as_deref(), Some("A1, orange tag"));
    let ctx = h.ctx.clone();
    let a = h.id("A");
    h.host.set_expanded(&ctx, a, false);
    h.settle();
    assert_eq!(
        row_name(&h, "A").as_deref(),
        Some("A, contains coloured notes")
    );
    h.host.set_expanded(&ctx, a, true);
    h.go("A1");
    h.settle();
    // The swatches are named buttons with a checked state, and "No colour" is a plain button.
    h.key(Key::F10, Modifiers::SHIFT);
    h.settle();
    let u = h.a11y.as_ref().unwrap();
    let swatches: Vec<_> = u
        .nodes
        .iter()
        .filter(|(_, n)| n.role() == Role::RadioButton)
        .map(|(_, n)| (n.label().unwrap_or("").to_owned(), n.toggled()))
        .filter(|(l, _)| !l.ends_with("note colour") && !l.ends_with("selected"))
        .collect();
    assert_eq!(swatches.len(), 6, "{swatches:?}");
    for c in Colour::ALL {
        assert!(swatches.iter().any(|(l, _)| l == c.label()), "{c:?}");
    }
    let orange = u
        .nodes
        .iter()
        .find(|(_, n)| n.role() == Role::RadioButton && n.label() == Some("Orange"))
        .map(|(_, n)| n)
        .unwrap();
    assert!(
        orange.is_selected() == Some(true)
            || orange.toggled() == Some(egui::accesskit::Toggled::True),
        "the current colour says so: {orange:?}"
    );
}

#[test]
fn colours_are_distinct_readable_and_follow_the_theme() {
    let (_d, p) = deep("t1-theme");
    let mut h = opened(&p);
    h.go("A2");
    h.pick_by_keyboard("Green");
    let measure = |h: &H| {
        let chrome = h.chrome();
        let pal = h.host.theme_palette();
        let surfaces = [
            theme::colour(pal.background),
            theme::colour(pal.surface),
            theme::colour(pal.surface_raised),
            chrome.selected_focus,
            chrome.selected_blur,
        ];
        for t in chrome.tags {
            for s in surfaces {
                assert!(
                    theme::contrast(t, s) >= theme::MUTED_MIN - 0.01,
                    "{t:?} on {s:?}"
                );
            }
        }
        // Six different colours, far enough apart to tell apart.
        for i in 0..6 {
            for j in i + 1..6 {
                let (a, b) = (chrome.tags[i], chrome.tags[j]);
                let d = (i32::from(a.r()) - i32::from(b.r())).abs()
                    + (i32::from(a.g()) - i32::from(b.g())).abs()
                    + (i32::from(a.b()) - i32::from(b.b())).abs();
                assert!(d > 60, "{i} and {j} are too alike: {a:?} {b:?}");
            }
        }
        chrome.tag(Colour::Green)
    };
    h.system = Some(egui::Theme::Dark);
    h.advance(1.0);
    let dark = measure(&h);
    assert!(
        h.painted().dots.iter().any(|d| d.fill == dark),
        "the dot is drawn in the dark theme's green"
    );
    h.system = Some(egui::Theme::Light);
    h.advance(1.0);
    let light = measure(&h);
    assert_ne!(dark, light, "the colour follows the theme");
    assert!(
        h.painted().dots.iter().any(|d| d.fill == light),
        "and the dot changed with it"
    );
    assert_eq!(
        h.colour_of("A2"),
        Some(Colour::Green),
        "the stored identity did not"
    );
}

#[test]
fn deep_nesting_in_a_narrow_window_keeps_titles_readable_and_marks_inside_the_pane() {
    let d = dir("t1-narrow");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut parent = doc.create_root("Level 0 with a fairly long title").unwrap();
    let mut ids = vec![parent];
    for i in 1..9 {
        parent = doc
            .create_child(parent, &format!("Level {i} with a fairly long title"))
            .unwrap();
        ids.push(parent);
    }
    doc.set_colour(*ids.last().unwrap(), Some(Colour::Purple))
        .unwrap();
    doc.set_colour(ids[0], Some(Colour::Red)).unwrap();
    doc.save().unwrap();
    for width in [900.0_f32, 520.0, 380.0] {
        let mut h = H::open_sized(&p, Vec2::new(width, 600.0));
        let ctx = h.ctx.clone();
        h.host.set_all_expanded(&ctx, true);
        h.settle();
        let painted = h.painted();
        let right = h.tree_right();
        assert_eq!(painted.dots.len(), 2, "{width}");
        for dot in &painted.dots {
            assert!(
                dot.centre.x + dot.radius <= right + 0.5,
                "{width}: inside the tree pane"
            );
        }
        // The deepest title still shows something, and never runs under its dot.
        let deepest = painted
            .texts
            .iter()
            .find(|t| t.text.starts_with("Level 8"))
            .expect("deepest row drawn");
        let dot = painted
            .dots
            .iter()
            .find(|d| (d.centre.y - deepest.centre_y()).abs() < 1.0)
            .unwrap();
        if width >= 500.0 {
            assert!(
                deepest.right() < dot.centre.x - dot.radius,
                "{width}: the title stops before the dot"
            );
            assert!(
                deepest.text.chars().count() >= 2,
                "{width}: {:?}",
                deepest.text
            );
        }
    }
}

#[test]
fn a_large_tree_computes_its_cues_in_one_pass() {
    let d = dir("t1-large");
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let mut tagged = None;
    for i in 0..200 {
        let r = doc.create_root(&format!("Root {i:03}")).unwrap();
        for j in 0..20 {
            let c = doc
                .create_child(r, &format!("Child {i:03}-{j:02}"))
                .unwrap();
            if i == 123 && j == 7 {
                tagged = Some((r, c));
            }
        }
    }
    let (root, child) = tagged.unwrap();
    doc.set_colour(child, Some(Colour::Orange)).unwrap();
    doc.save().unwrap();
    let h = H::open_tree(&p);
    for r in h.host.rows() {
        let want = if r.id == root {
            tree::colour_bit(Colour::Orange)
        } else {
            0
        };
        assert_eq!(r.below, want, "{}", r.title);
    }
    assert_eq!(
        h.host.tree().row(child).unwrap().colour,
        Some(Colour::Orange)
    );
}
