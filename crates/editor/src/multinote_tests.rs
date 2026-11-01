//! E3: several notes in one host, driven frame by frame through the real `EditorView`.

use super::multinote_host::MultiHost;
use super::*;
use crate::attachments::{self, AttachmentStore};
use egui::text::{CCursor, CCursorRange};
fn tmp(tag: &str) -> crate::test_dir::TestDir {
    crate::test_dir::TestDir::new(&format!("e3-{tag}"))
}

fn key(k: Key, m: Modifiers) -> egui::Event {
    egui::Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers: m,
    }
}

thread_local! {
    /// The window size the test host draws into (per test thread).
    static SCREEN: std::cell::Cell<Vec2> = const { std::cell::Cell::new(Vec2::new(1000.0, 700.0)) };
}

const CTRL: Modifiers = Modifiers::COMMAND;
const ALT: Modifiers = Modifiers::ALT;

struct Mh {
    ctx: Context,
    host: MultiHost,
    t: f64,
    opened: Vec<String>,
    #[allow(dead_code)]
    dir: crate::test_dir::TestDir,
}

impl Mh {
    fn new(notes: &[(&str, &str)]) -> Self {
        let dir = tmp("host");
        let store = Box::new(attachments::FsStore::new(dir.clone()));
        let mut h = Mh {
            ctx: Context::default(),
            host: MultiHost::new(1, notes, store),
            t: 0.0,
            opened: Vec::new(),
            dir,
        };
        // Two profiles matter for hosts; the standalone one has Ctrl+Shift+S strikethrough and the
        // task toggle, which these tests do not need. Use the OmaTree profile (set by the host).
        h.settle();
        h
    }

    fn three() -> Self {
        Self::new(&[
            ("A", "Alpha note\n"),
            ("B", "Beta note\n"),
            ("C", "Gamma note\n"),
        ])
    }

    fn k(&self, i: usize) -> NoteKey {
        self.host.keys()[i]
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![]);
        }
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.frame_with(events, Modifiers::NONE);
    }

    fn frame_with(&mut self, mut events: Vec<egui::Event>, m: Modifiers) {
        events.insert(0, egui::Event::ModifiersChanged(m));
        self.t += 0.05;
        let mut input = egui::RawInput {
            time: Some(self.t),
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, SCREEN.with(|s| s.get()))),
            ..Default::default()
        };
        input.focused = true;
        let host = &mut self.host;
        let mut out = self.ctx.run_ui(input, |ui| host.ui(ui));
        for c in &out.platform_output.commands {
            if let egui::OutputCommand::OpenUrl(u) = c {
                self.opened.push(u.url.clone());
            }
        }
        out.textures_delta.clear();
    }

    fn key(&mut self, k: Key, m: Modifiers) {
        self.frame_with(vec![key(k, m)], m);
        self.frame(vec![]);
    }

    fn text(&mut self, s: &str) {
        self.frame(vec![egui::Event::Text(s.to_string())]);
        self.frame(vec![]);
    }

    fn select(&mut self, i: usize) {
        let k = self.k(i);
        self.host.select(&self.ctx.clone(), k);
        self.settle();
    }

    fn set_sel(&mut self, a: usize, b: usize) {
        let k = self.host.selected.unwrap();
        let id = k.widget_id();
        let text = self.host.text_of(k).unwrap().to_owned();
        let mut st = egui::TextEdit::load_state(&self.ctx, id).unwrap_or_default();
        let c = |x: usize| CCursor::new(edit::byte_to_char(&text, x));
        st.cursor
            .set_char_range(Some(CCursorRange::two(c(a), c(b))));
        st.store(&self.ctx, id);
        self.settle();
    }

    fn sel(&self, i: usize) -> (usize, usize) {
        let k = self.k(i);
        EditorView::selection(&self.ctx, k, self.host.text_of(k).unwrap())
    }

    fn txt(&self, i: usize) -> String {
        self.host.text_of(self.k(i)).unwrap().to_owned()
    }

    fn undo(&mut self) {
        self.key(Key::Z, CTRL);
    }

    fn redo(&mut self) {
        self.key(Key::Z, CTRL | Modifiers::SHIFT);
    }

    fn end(&mut self) {
        let k = self.host.selected.unwrap();
        let n = self.host.text_of(k).unwrap().len();
        self.set_sel(n, n);
    }
}

#[test]
fn three_notes_hold_independent_buffers_and_edits_reach_only_the_shown_one() {
    let mut h = Mh::three();
    for (i, add) in ["A1", "B2", "C3"].into_iter().enumerate() {
        h.select(i);
        h.end();
        h.text(add);
    }
    assert_eq!(h.txt(0), "Alpha note\nA1");
    assert_eq!(h.txt(1), "Beta note\nB2");
    assert_eq!(h.txt(2), "Gamma note\nC3");
    // Every reported edit names the note that was shown, and holds that note's text.
    let keys = h.host.keys();
    for (k, text) in &h.host.edits {
        let i = keys.iter().position(|x| x == k).unwrap();
        assert!(
            h.txt(i)
                .starts_with(text.trim_end_matches(|c| c != '\n').trim_end())
                || !text.is_empty()
        );
    }
    let who: Vec<usize> = h
        .host
        .edits
        .iter()
        .map(|(k, _)| keys.iter().position(|x| x == k).unwrap())
        .collect();
    assert!(
        who.windows(2).all(|w| w[0] <= w[1]),
        "edits arrive note by note: {who:?}"
    );
    assert_eq!(
        who.iter().filter(|i| **i == 0).count(),
        who.iter().filter(|i| **i == 0).count()
    );
    assert!(who.contains(&0) && who.contains(&1) && who.contains(&2));
}

#[test]
fn the_full_switching_sequence_restores_text_caret_and_independent_undo_redo() {
    let mut h = Mh::three();
    // 1-2. Note A: caret, text, formatting.
    h.select(0);
    h.set_sel(5, 5);
    h.text(" Příliš");
    h.set_sel(6, 15);
    h.key(Key::B, CTRL);
    let a_formatted = h.txt(0);
    assert_eq!(a_formatted, "Alpha **Příliš** note\n");
    let a_sel = h.sel(0);
    // 3-4. Note B: different text.
    h.select(1);
    h.set_sel(4, 4);
    h.text("-X");
    assert_eq!(h.txt(1), "Beta-X note\n");
    let b_sel = h.sel(1);
    assert_eq!(h.txt(0), a_formatted, "A untouched while B is edited");
    // 5-6. Back to A: content and caret restored.
    h.select(0);
    assert_eq!(h.txt(0), a_formatted);
    assert_eq!(h.sel(0), a_sel);
    // 7. Undo in A: only A changes.
    h.undo();
    assert_eq!(h.txt(0), "Alpha Příliš note\n", "bold undone");
    assert_eq!(h.txt(1), "Beta-X note\n");
    // 8. B: content and undo history untouched.
    h.select(1);
    assert_eq!(h.txt(1), "Beta-X note\n");
    assert_eq!(h.sel(1), b_sel);
    // 9. Undo in B.
    h.undo();
    assert_eq!(h.txt(1), "Beta note\n");
    assert_eq!(h.txt(0), "Alpha Příliš note\n");
    // 10. Redo independently.
    h.redo();
    assert_eq!(h.txt(1), "Beta-X note\n");
    assert_eq!(
        h.txt(0),
        "Alpha Příliš note\n",
        "redo in B does not touch A"
    );
    h.select(0);
    h.redo();
    assert_eq!(h.txt(0), a_formatted);
    assert_eq!(h.txt(1), "Beta-X note\n");
    assert_eq!(h.txt(2), "Gamma note\n", "note C was never touched");
}

#[test]
fn the_hosts_text_is_current_after_every_changed_frame_and_only_then() {
    let mut h = Mh::three();
    h.select(1);
    h.end();
    let before = h.host.edits.len();
    for ch in ["a", "b", "č", "🦀"] {
        h.frame(vec![egui::Event::Text(ch.into())]);
        assert!(h.host.last.changed, "{ch} reported");
        let (k, t) = h.host.edits.last().unwrap();
        assert_eq!(*k, h.k(1));
        assert_eq!(t, &h.txt(1), "the stored text is what the editor shows");
        h.frame(vec![]);
        assert!(!h.host.last.changed);
    }
    assert_eq!(h.host.edits.len(), before + 4, "idle frames add no edit");
    assert_eq!(h.txt(1), "Beta note\nabč🦀");
}

#[test]
fn switching_without_editing_changes_nothing() {
    let mut h = Mh::three();
    let texts: Vec<String> = (0..3).map(|i| h.txt(i)).collect();
    h.select(1);
    h.set_sel(2, 4);
    let sel = h.sel(1);
    for _ in 0..3 {
        for i in [0, 2, 1] {
            h.select(i);
        }
    }
    h.key(Key::ArrowDown, ALT);
    h.key(Key::ArrowUp, ALT);
    assert!(h.host.edits.is_empty(), "{:?}", h.host.edits);
    for (i, t) in texts.iter().enumerate() {
        assert_eq!(&h.txt(i), t);
    }
    h.select(1);
    assert_eq!(h.sel(1), sel);
    h.undo();
    assert_eq!(h.txt(1), texts[1], "no undo history was invented");
}

#[test]
fn keyboard_switching_wraps_and_moves_typing_to_the_new_note() {
    let mut h = Mh::three();
    assert_eq!(h.host.selected, Some(h.k(0)));
    h.key(Key::ArrowDown, ALT);
    assert_eq!(h.host.selected, Some(h.k(1)));
    h.end();
    h.text("!");
    assert_eq!(h.txt(1), "Beta note\n!");
    assert_eq!(h.txt(0), "Alpha note\n");
    h.key(Key::ArrowUp, ALT);
    h.key(Key::ArrowUp, ALT);
    assert_eq!(
        h.host.selected,
        Some(h.k(2)),
        "wraps from the first to the last"
    );
    h.end();
    h.text("?");
    assert_eq!(h.txt(2), "Gamma note\n?");
    assert_eq!(h.txt(1), "Beta note\n!");
}

#[test]
fn czech_text_and_multi_codepoint_emoji_survive_switches_and_undo() {
    let mut h = Mh::three();
    let family = "👨‍👩‍👧";
    h.select(0);
    h.end();
    h.text(&format!("Příliš žluťoučký kůň {family} ✅"));
    let a = h.txt(0);
    h.select(1);
    h.end();
    h.text("ěščřžýáíé");
    h.select(0);
    assert_eq!(h.txt(0), a);
    let (p, q) = h.sel(0);
    assert!(a.is_char_boundary(p) && a.is_char_boundary(q));
    assert!(a.contains(family));
    h.undo();
    let undone = h.txt(0);
    assert!(
        undone.is_char_boundary(undone.len()) && std::str::from_utf8(undone.as_bytes()).is_ok()
    );
    assert_eq!(h.txt(1), "Beta note\něščřžýáíé");
    h.redo();
    assert_eq!(h.txt(0), a);
}

#[test]
fn programmatic_undo_boundaries_hold_across_a_visit_to_another_note() {
    let mut h = Mh::three();
    h.select(0);
    h.end();
    h.text("/");
    h.text("h2");
    h.key(Key::Enter, Modifiers::NONE);
    h.text("Titel");
    assert_eq!(h.txt(0), "Alpha note\n## Titel");
    h.select(1);
    h.end();
    h.text("x");
    h.select(0);
    h.undo();
    assert_eq!(h.txt(0), "Alpha note\n## ", "typing is one step");
    h.undo();
    assert_eq!(h.txt(0), "Alpha note\n/h2", "the command is its own step");
    h.redo();
    h.redo();
    assert_eq!(h.txt(0), "Alpha note\n## Titel");
    assert_eq!(h.txt(1), "Beta note\nx");
}

// ---- pending image edits ----

fn png(dir: &std::path::Path, w: usize, hh: usize) -> String {
    let b = assets::encode_png(&egui::ColorImage::filled(
        [w, hh],
        Color32::from_rgb(200, 60, 60),
    ))
    .unwrap();
    attachments::FsStore::new(dir).put(&b, "png").unwrap()
}

/// Notes A (with an image), B and C; the image is an inactive rendered row at the end of A.
fn with_image() -> (Mh, String) {
    let dir = tmp("img");
    let r = png(&dir, 400, 200);
    let a = format!("Úvod\n\n![obr]({r})\n\nkonec\n");
    let store = Box::new(attachments::FsStore::new(dir.clone()));
    let mut h = Mh {
        ctx: Context::default(),
        host: MultiHost::new(1, &[("A", &a), ("B", "Beta\n"), ("C", "Gamma\n")], store),
        t: 0.0,
        opened: vec![],
        dir,
    };
    h.settle();
    h.end();
    (h, a)
}

fn image_rect(h: &Mh) -> Rect {
    h.host.view.probe.image_rects[0].1
}

fn slider(h: &Mh) -> Rect {
    h.host
        .view
        .probe
        .controls
        .iter()
        .find(|(n, _)| *n == "slider")
        .unwrap()
        .1
}

fn pointer(h: &mut Mh, pos: Pos2, pressed: Option<bool>) {
    h.frame(vec![egui::Event::PointerMoved(pos)]);
    if let Some(p) = pressed {
        h.frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: p,
            modifiers: Modifiers::NONE,
        }]);
    }
}

fn select_image(h: &mut Mh) {
    let c = image_rect(h).center();
    pointer(h, c, Some(true));
    pointer(h, c, Some(false));
    h.settle();
}

#[test]
fn a_mouse_drag_in_progress_is_committed_to_its_own_note_when_leaving_it() {
    let (mut h, a0) = with_image();
    select_image(&mut h);
    let s = slider(&h);
    let p = Pos2::new(s.left() + 30.0, s.center().y);
    pointer(&mut h, p, Some(true));
    pointer(&mut h, Pos2::new(p.x + 40.0, p.y), None);
    assert!(h.host.view.live_width.get().is_some(), "a drag is pending");
    assert_eq!(h.txt(0), a0, "nothing written yet");
    h.select(1);
    let a = h.txt(0);
    assert!(a.contains("{width="), "committed on leaving: {a}");
    assert_eq!(
        a.replace(&a[a.find("{width=").unwrap()..a.find('}').unwrap() + 1], ""),
        a0
    );
    assert_eq!(h.txt(1), "Beta\n");
    assert_eq!(h.txt(2), "Gamma\n");
    assert_eq!(h.host.view.live_width.get(), None);
    assert_eq!(h.host.view.selected_image, None);
    // Returning shows the stored width; undo and redo touch only A.
    h.select(0);
    let pct: f32 = a[a.find("width=").unwrap() + 6..a.find("%}").unwrap()]
        .parse()
        .unwrap();
    let w = image_rect(&h).width();
    let content = h
        .host
        .view
        .probe
        .galley
        .as_ref()
        .unwrap()
        .0
        .job
        .wrap
        .max_width;
    assert!(
        (w - pct / 100.0 * content).abs() < 1.5,
        "{w} vs {pct}% of {content}"
    );
    h.undo();
    assert_eq!(h.txt(0), a0, "one undo removes the width");
    h.redo();
    assert_eq!(h.txt(0), a);
    assert_eq!(h.txt(1), "Beta\n");
}

#[test]
fn a_keyboard_width_adjustment_is_committed_when_leaving_the_note() {
    let (mut h, a0) = with_image();
    select_image(&mut h);
    let sid = h.host.view.slider_id.expect("controls shown");
    h.ctx.memory_mut(|m| m.request_focus(sid));
    h.settle();
    h.key(Key::ArrowLeft, Modifiers::NONE);
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert!(h.host.view.live_width.get().is_some());
    assert_eq!(h.txt(0), a0, "still pending");
    h.key(Key::ArrowDown, ALT);
    assert_eq!(h.host.selected, Some(h.k(1)));
    assert!(h.txt(0).contains("{width="), "keyboard switch committed it");
    assert_eq!(h.txt(1), "Beta\n");
    assert_eq!(h.host.view.live_width.get(), None);
    // Back in A the width is shown; the other notes never saw it.
    h.key(Key::ArrowUp, ALT);
    assert!(image_rect(&h).width() < 400.0 + 0.5);
    assert!(!h.txt(1).contains("width") && !h.txt(2).contains("width"));
    h.undo();
    assert_eq!(h.txt(0), a0);
}

#[test]
fn widths_of_images_in_two_notes_are_independent() {
    let dir = tmp("img2");
    let r = png(&dir, 300, 150);
    let a = format!("![x]({r})\n");
    let store = Box::new(attachments::FsStore::new(dir.clone()));
    let mut h = Mh {
        ctx: Context::default(),
        host: MultiHost::new(1, &[("A", &a), ("B", &a)], store),
        t: 0.0,
        opened: vec![],
        dir,
    };
    h.settle();
    for i in 0..2 {
        h.select(i);
        h.end();
        select_image(&mut h);
        let sid = h.host.view.slider_id.unwrap();
        h.ctx.memory_mut(|m| m.request_focus(sid));
        h.settle();
        for _ in 0..(3 + 4 * i) {
            h.key(Key::ArrowLeft, Modifiers::NONE);
        }
        let k = h.k(i);
        let ctx = h.ctx.clone();
        let mut view = std::mem::take(&mut h.host.view);
        let mut text = h.txt(i);
        view.commit_pending(&ctx, k, &mut text);
        h.host.view = view;
        h.host.notes[i].text = text;
        h.settle();
    }
    let (a, b) = (h.txt(0), h.txt(1));
    assert!(a.contains("{width=") && b.contains("{width="));
    assert_ne!(a, b, "different adjustments, different annotations");
}

// ---- invalidation ----

fn state_exists(h: &Mh, k: NoteKey) -> bool {
    egui::TextEdit::load_state(&h.ctx, k.widget_id()).is_some()
}

#[test]
fn deleting_a_note_forgets_its_state_and_leaves_the_others_alone() {
    let mut h = Mh::three();
    h.select(0);
    h.end();
    h.text("a1");
    h.select(1);
    h.end();
    h.text("b1");
    h.select(2);
    h.end();
    h.text("c1");
    let (a, b, c) = (h.k(0), h.k(1), h.k(2));
    let ctx = h.ctx.clone();
    h.host.delete(&ctx, c);
    h.settle();
    assert!(!state_exists(&h, c) || EditorView::selection(&ctx, c, "") == (0, 0));
    assert_eq!(h.host.keys(), vec![a, b]);
    assert_eq!(
        h.host.selected,
        Some(a),
        "selection moved to a surviving note"
    );
    h.undo();
    assert_eq!(h.txt(0), "Alpha note\n", "A's own undo works");
    assert_eq!(h.txt(1), "Beta note\nb1");
    h.select(1);
    h.undo();
    assert_eq!(h.txt(1), "Beta note\n");
}

#[test]
fn an_external_replacement_cannot_be_undone_into_stale_text_or_stale_caret() {
    let mut h = Mh::three();
    h.select(0);
    h.end();
    h.text("Dlouhý text, který už nebude existovat");
    let a = h.k(0);
    let ctx = h.ctx.clone();
    h.host.replace_body(&ctx, a, "krátký");
    h.settle();
    let (p, q) = h.sel(0);
    assert!(p <= 8 && q <= 8, "caret inside the new text: {p}..{q}");
    h.undo();
    assert_eq!(h.txt(0), "krátký");
    h.text("!");
    assert!(h.txt(0).contains('!') && h.txt(0).starts_with("krátk"));
    assert_eq!(h.txt(1), "Beta note\n");
}

#[test]
fn restoring_a_snapshot_replaces_the_body_without_stale_history() {
    let mut h = Mh::three();
    h.select(1);
    h.end();
    let b = h.k(1);
    h.host.snapshot(b);
    h.text("po snímku");
    h.text(" a ještě");
    let ctx = h.ctx.clone();
    h.host.restore(&ctx, b);
    h.settle();
    assert_eq!(h.txt(1), "Beta note\n");
    h.undo();
    assert_eq!(
        h.txt(1),
        "Beta note\n",
        "undo cannot bring back text from before the restore"
    );
    h.end();
    h.text("nově");
    assert_eq!(h.txt(1), "Beta note\nnově");
    h.undo();
    assert_eq!(h.txt(1), "Beta note\n");
    assert_eq!(h.txt(0), "Alpha note\n");
}

#[test]
fn replacing_the_notebook_makes_old_state_unreachable_even_for_equal_note_numbers() {
    let mut h = Mh::three();
    h.select(0);
    h.end();
    h.text("old edit");
    let old = h.k(0);
    assert_eq!(old, NoteKey::new(1, 1));
    let ctx = h.ctx.clone();
    h.host
        .replace_notebook(&ctx, 2, &[("N1", "new one\n"), ("N2", "new two\n")], None);
    h.settle();
    let new = h.k(0);
    assert_eq!(
        new,
        NoteKey::new(2, 1),
        "same note number, different notebook"
    );
    assert_ne!(old.widget_id(), new.widget_id());
    assert!(!state_exists(&h, old) || EditorView::selection(&ctx, old, "") == (0, 0));
    h.end();
    h.text("!");
    assert_eq!(h.txt(0), "new one\n!");
    h.undo();
    assert_eq!(h.txt(0), "new one\n", "history starts fresh");
    h.undo();
    assert_eq!(
        h.txt(0),
        "new one\n",
        "and holds nothing of the old notebook"
    );
}

#[test]
fn image_caches_are_per_notebook_and_cleared_when_the_notebook_is_replaced() {
    let (d1, d2) = (tmp("nb1"), tmp("nb2"));
    for (d, w, hh) in [(&d1, 400usize, 200usize), (&d2, 100, 100)] {
        std::fs::create_dir_all(d.join("assets")).unwrap();
        let p = assets::encode_png(&egui::ColorImage::filled([w, hh], Color32::RED)).unwrap();
        std::fs::write(d.join("assets/pic.png"), p).unwrap();
    }
    let src = "![x](assets/pic.png)\n\ntext\n";
    let mut h = Mh {
        ctx: Context::default(),
        host: MultiHost::new(
            1,
            &[("A", src)],
            Box::new(attachments::FsStore::new(d1.clone())),
        ),
        t: 0.0,
        opened: vec![],
        dir: d1,
    };
    h.settle();
    h.end();
    let first = h.host.view.probe.image_rects[0].1.height();
    assert!((first - 200.0).abs() < 1.0, "{first}");
    assert!(h.host.view.textures.borrow().keys().all(|(nb, _)| *nb == 1));
    let ctx = h.ctx.clone();
    h.host.replace_notebook(
        &ctx,
        2,
        &[("A", src)],
        Some(Box::new(attachments::FsStore::new(&d2))),
    );
    h.settle();
    h.end();
    let second = h.host.view.probe.image_rects[0].1.height();
    assert!(
        (second - 100.0).abs() < 1.0,
        "the new notebook's own picture: {second}"
    );
    assert!(h.host.view.textures.borrow().keys().all(|(nb, _)| *nb == 2));
}

// ---- transient UI state ----

#[test]
fn an_open_slash_menu_is_dismissed_by_switching_and_inserts_nothing_elsewhere() {
    let mut h = Mh::three();
    h.select(0);
    h.end();
    h.text("/");
    h.text("h");
    assert!(h.host.view.slash.is_some());
    let a_with_query = h.txt(0);
    h.key(Key::ArrowDown, ALT);
    assert!(h.host.view.slash.is_none(), "menu closed by the switch");
    assert_eq!(h.txt(0), a_with_query, "the command was not applied to A");
    h.end();
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(
        h.txt(1),
        "Beta note\n\n",
        "Enter is a plain newline in B, not a menu choice"
    );
    assert_eq!(h.txt(0), a_with_query);
    assert_eq!(h.txt(2), "Gamma note\n");
}

#[test]
fn an_open_emoji_search_is_dismissed_by_switching() {
    let mut h = Mh::three();
    h.select(0);
    h.end();
    h.text("/");
    h.text("emoji");
    h.key(Key::Enter, Modifiers::NONE);
    assert!(
        h.host
            .view
            .slash
            .as_ref()
            .is_some_and(|s| s.picker.is_some()),
        "picker open"
    );
    h.text("crab");
    let a = h.txt(0);
    h.key(Key::ArrowDown, ALT);
    assert!(h.host.view.slash.is_none(), "picker closed by the switch");
    h.end();
    h.text("ahoj");
    assert_eq!(
        h.txt(1),
        "Beta note\nahoj",
        "typing goes to B, not into the picker"
    );
    assert_eq!(h.txt(0), a, "A got no emoji");
    assert!(!h.txt(0).contains('🦀') && !h.txt(1).contains('🦀'));
}

#[test]
fn image_selection_and_controls_do_not_follow_to_the_next_note() {
    let (mut h, _a0) = with_image();
    select_image(&mut h);
    assert!(h.host.view.selected_image.is_some());
    assert!(h.host.view.controls_rect != Rect::NOTHING);
    h.select(1);
    assert_eq!(h.host.view.selected_image, None);
    assert_eq!(h.host.view.controls_rect, Rect::NOTHING);
    assert_eq!(h.host.view.slider_id, None);
    assert!(h.host.view.probe.controls.is_empty() || h.host.view.selected_image.is_none());
    h.select(0);
    assert_eq!(
        h.host.view.selected_image, None,
        "not restored on return either"
    );
}

#[test]
fn a_link_press_does_not_open_after_a_switch() {
    let src = "see [docs](https://example.com/a) here\n";
    let mut h = Mh::new(&[("A", src), ("B", "other\n")]);
    h.end();
    let g = h.host.view.probe.galley.clone().unwrap();
    let at = src.find("docs").unwrap();
    let r =
        g.0.pos_from_cursor(CCursor::new(edit::byte_to_char(src, at)));
    let pos = g.1 + Vec2::new(r.min.x + 3.0, r.center().y);
    // Ctrl+press on the link, then switch notes before the button comes up.
    h.frame_with(vec![egui::Event::PointerMoved(pos)], CTRL);
    h.frame_with(
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: CTRL,
        }],
        CTRL,
    );
    assert!(h.host.view.link_press.is_some(), "the press was recorded");
    h.select(1);
    assert!(h.host.view.link_press.is_none());
    h.frame_with(
        vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: CTRL,
        }],
        CTRL,
    );
    h.settle();
    assert!(h.opened.is_empty(), "nothing opened: {:?}", h.opened);
    assert_eq!(h.txt(1), "other\n");
    assert_eq!(h.txt(0), src);
}

// ---- performance (recorded, not judged) ----

fn large_note(target_bytes: usize) -> String {
    let mut s = String::with_capacity(target_bytes + 1024);
    let mut i = 0usize;
    while s.len() < target_bytes {
        i += 1;
        s.push_str(&format!(
            "## Oddíl {i}: Příliš žluťoučký kůň\n\nOdstavec **tučně**, *kurzívou*, `kód`, ~~škrt~~ a [odkaz](https://example.com/{i}). \
Další věta s emoji 🦀 ✅ a delším textem, aby se řádek zalomil přes více řádků v okně editoru.\n\n\
- [ ] úkol {i}\n- [x] hotovo {i}\n- položka\n\n> citace {i}\n\n| a | b |\n| --- | --- |\n| {i} | x |\n\n```\nkód {i}\n```\n\n"
        ));
    }
    s
}

fn rss_kb() -> Option<u64> {
    let s = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = s.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * 4)
}

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn stats(v: &mut [f64]) -> (f64, f64, f64) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    (
        v[v.len() / 2],
        v.iter().sum::<f64>() / v.len() as f64,
        *v.last().unwrap(),
    )
}

fn measure(bytes: usize, typing_frames: usize) {
    let big = large_note(bytes);
    let n0 = big.len();
    let t = std::time::Instant::now();
    for _ in 0..5 {
        std::hint::black_box(markdown::parse(&big));
    }
    let parse = ms(t.elapsed()) / 5.0;
    // Layout costs on their own: building the LayoutJob from the spans, then egui laying it out
    // (cold: first time; warm: the same job again, answered from egui's galley cache).
    let spans = markdown::parse(&big);
    let pal = Palette::fallback();
    let image_h = |_: usize, _: &str, _: Option<u16>| None;
    let table_h = |_: &markdown::Table| 40.0f32;
    let params = layout::Params {
        base_size: BASE_SIZE,
        text_color: pal.text,
        heading_colors: pal.headings,
        quote_color: pal.quote_text,
        code_color: pal.code_text,
        dim_color: pal.reveal,
        accent: pal.link,
        code_bg: pal.code_bg,
        bold_family: None,
        bold_drop: 0.0,
        pixels_per_point: 1.0,
        selection: 0..0,
        image_height: &image_h,
        table_height: &table_h,
    };
    let t = std::time::Instant::now();
    let (job, _) = layout::build_job(&big, &spans, &params, 700.0);
    let build_job = ms(t.elapsed());
    let sections = job.sections.len();
    let ctx0 = Context::default();
    let mut cold = 0.0;
    let mut warm = Vec::new();
    for i in 0..4 {
        let j = job.clone();
        ctx0.run_ui(egui::RawInput::default(), |_| {})
            .textures_delta
            .clear();
        let t = std::time::Instant::now();
        ctx0.fonts_mut(|f| f.layout_job(j));
        if i == 0 {
            cold = ms(t.elapsed());
        } else {
            warm.push(ms(t.elapsed()));
        }
    }
    eprintln!(
        "PERF   build_job ({} sections): {build_job:.1} ms; egui layout_job cold {cold:.1} ms, repeated {:.1} ms",
        sections,
        warm.iter().sum::<f64>() / warm.len() as f64
    );
    let mut h = Mh::new(&[("small", "klein\n"), ("big", &big), ("other", "x\n")]);
    // Switch cost: the first frame showing the big note (layout from nothing) and the second.
    let k = h.k(1);
    let ctx = h.ctx.clone();
    let t = std::time::Instant::now();
    h.host.select(&ctx, k);
    h.frame(vec![]);
    let first = ms(t.elapsed());
    let t = std::time::Instant::now();
    h.frame(vec![]);
    let second = ms(t.elapsed());
    // Idle frames and typing frames with the caret in the middle of the note.
    let mid = n0 / 2;
    let mid = (0..=mid).rev().find(|i| big.is_char_boundary(*i)).unwrap();
    h.set_sel(mid, mid);
    let mut idle = Vec::new();
    for _ in 0..20 {
        let t = std::time::Instant::now();
        h.frame(vec![]);
        idle.push(ms(t.elapsed()));
    }
    let rss0 = rss_kb();
    let mut typing = Vec::new();
    for i in 0..typing_frames {
        let c = ["a", "b", "č", "e"][i % 4];
        let t = std::time::Instant::now();
        h.frame(vec![egui::Event::Text(c.into())]);
        typing.push(ms(t.elapsed()));
        h.frame(vec![]);
    }
    let rss1 = rss_kb();
    // Same typing with the caret at the end (the usual place to write).
    h.end();
    let mut endtype = Vec::new();
    for _ in 0..20 {
        let t = std::time::Instant::now();
        h.frame(vec![egui::Event::Text("x".into())]);
        endtype.push(ms(t.elapsed()));
        h.frame(vec![]);
    }
    // Switching away and back.
    let mut back = Vec::new();
    for _ in 0..5 {
        h.select(0);
        let t = std::time::Instant::now();
        h.host.select(&ctx, k);
        h.frame(vec![]);
        back.push(ms(t.elapsed()));
        h.frame(vec![]);
    }
    let (im, ia, ix) = stats(&mut idle);
    let (tm, ta, tx) = stats(&mut typing);
    let (em, ea, ex) = stats(&mut endtype);
    let (bm, ba, bx) = stats(&mut back);
    eprintln!(
        "PERF {} KB note ({} B, {} lines)\n  parse (markdown::parse): {parse:.2} ms\n  first frame showing it after select: {first:.1} ms; second: {second:.1} ms\n  switch back into it (cached galley, after visiting another note): median {bm:.1} / mean {ba:.1} / max {bx:.1} ms\n  idle frame: median {im:.2} / mean {ia:.2} / max {ix:.2} ms\n  typing frame, caret mid-note: median {tm:.2} / mean {ta:.2} / max {tx:.2} ms ({typing_frames} frames)\n  typing frame, caret at end: median {em:.2} / mean {ea:.2} / max {ex:.2} ms\n  RSS before/after {typing_frames} edits in the note: {:?} -> {:?} KB",
        n0 / 1000,
        n0,
        big.lines().count(),
        rss0,
        rss1
    );
    // Correctness only: every typed character landed in the big note and nowhere else.
    let note = h.txt(1);
    assert!(note.len() >= n0 + typing_frames + 20);
    assert_eq!(h.txt(0), "klein\n");
    assert_eq!(h.txt(2), "x\n");
}

#[test]
fn a_medium_note_edits_and_switches_correctly() {
    // Same code path as the measurement, small enough for debug builds.
    measure(20_000, 12);
}

/// Release-mode measurement: `cargo test --release large_note -- --ignored --nocapture`.
#[test]
#[ignore = "measurement; run in release mode with --nocapture"]
fn large_note_200_kb_measurements() {
    measure(200_000, 60);
}

/// What retaining undo state costs: egui keeps full-text copies for each undo point.
#[test]
#[ignore = "measurement; run in release mode with --nocapture"]
fn undo_state_memory_for_three_large_notes() {
    let big = large_note(200_000);
    let mut h = Mh::new(&[("a", &big), ("b", &big), ("c", &big)]);
    let base = rss_kb();
    let mut after = vec![];
    for i in 0..3 {
        h.select(i);
        h.end();
        for j in 0..40 {
            h.frame(vec![egui::Event::Text("x".into())]);
            h.frame(vec![]);
            if j % 8 == 7 {
                // Distinct undo points: pause longer than egui's stable time between edits.
                for _ in 0..30 {
                    h.frame(vec![]);
                }
            }
        }
        after.push(rss_kb());
    }
    eprintln!(
        "PERF undo memory: RSS with three 200 KB notes shown once: {:?} KB at start; after 40 edits in each of notes 1..3: {:?} KB",
        base, after
    );
    h.select(0);
    for _ in 0..5 {
        h.undo();
    }
    assert!(h.txt(0).len() <= big.len() + 40);
}

/// Which constructs make a big note expensive per frame? Idle-frame median for 200 KB notes that
/// differ only in markup (recorded, not judged).
#[test]
#[ignore = "measurement; run in release mode with --nocapture"]
fn large_note_idle_cost_by_markup_kind() {
    let para = "Příliš žluťoučký kůň úpěl ďábelské ódy a další věta, aby se řádek zalomil přes více řádků v okně editoru. ";
    let plain: String = (0..)
        .map(|_| format!("{para}\n\n"))
        .scan(0usize, |n, p| {
            *n += p.len();
            (*n < 200_000).then_some(p)
        })
        .collect();
    let styled: String = (0..)
        .map(|i| format!("{para}**tučně** *kurzíva* `kód` [odkaz](https://x.org/{i})\n\n"))
        .scan(0usize, |n, p| {
            *n += p.len();
            (*n < 200_000).then_some(p)
        })
        .collect();
    let tasks: String = (0..)
        .map(|i| format!("- [ ] úkol {i}\n"))
        .scan(0usize, |n, p| {
            *n += p.len();
            (*n < 200_000).then_some(p)
        })
        .collect();
    let tables: String = (0..)
        .map(|i| format!("| a | b |\n| --- | --- |\n| {i} | x |\n\n"))
        .scan(0usize, |n, p| {
            *n += p.len();
            (*n < 200_000).then_some(p)
        })
        .collect();
    for (name, doc) in [
        ("plain paragraphs", plain),
        ("inline styles + links", styled),
        ("task items", tasks),
        ("tables", tables),
    ] {
        let mut h = Mh::new(&[("big", &doc)]);
        h.end();
        let mut idle = Vec::new();
        for _ in 0..8 {
            let t = std::time::Instant::now();
            h.frame(vec![]);
            idle.push(ms(t.elapsed()));
        }
        let (m, _, x) = stats(&mut idle);
        eprintln!(
            "PERF idle frame, 200 KB of {name}: median {m:.1} ms, max {x:.1} ms ({} spans)",
            markdown::parse(&doc).len()
        );
    }
}

// ---- E3.1: per-frame cost by kind of content ----

fn take_prof() -> Vec<(&'static str, f64, u32)> {
    let mut v: Vec<_> = super::PROF.with(|p| {
        p.borrow_mut()
            .drain()
            .map(|(k, (ns, n))| (k, ns as f64 / 1e6, n))
            .collect()
    });
    v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    v
}

fn fixture(kind: &str, bytes: usize) -> String {
    let para = "Příliš žluťoučký kůň úpěl ďábelské ódy a další věta, aby se řádek zalomil přes více řádků v okně editoru. ";
    let gen = |f: &dyn Fn(usize) -> String| -> String {
        let mut s = String::new();
        let mut i = 0;
        while s.len() < bytes {
            s.push_str(&f(i));
            i += 1;
        }
        s
    };
    match kind {
        "plain" => gen(&|_| format!("{para}\n\n")),
        "styled" => {
            gen(&|i| format!("{para}**tučně** *kurzíva* `kód` [odkaz](https://x.org/{i})\n\n"))
        }
        "tasks" => gen(&|i| format!("- [{}] úkol {i}\n", if i % 3 == 0 { "x" } else { " " })),
        "tables" => gen(&|i| format!("| a | b |\n| --- | --- |\n| {i} | x |\n\n")),
        "mixed" => large_note(bytes),
        _ => unreachable!(),
    }
}

struct Row {
    name: String,
    first: f64,
    idle: (f64, f64),
    typing: (f64, f64),
    scroll: (f64, f64),
    back: (f64, f64),
    prof: Vec<(&'static str, f64, u32)>,
    tprof: Vec<(&'static str, f64, u32)>,
}

fn med_max(v: &mut [f64]) -> (f64, f64) {
    let (m, _, x) = stats(v);
    (m, x)
}

fn bench(name: &str, doc: &str, reps: usize) -> Row {
    let mut h = Mh::new(&[("a", "klein\n"), ("big", doc), ("c", "x\n")]);
    let k = h.k(1);
    let ctx = h.ctx.clone();
    take_prof();
    let t = std::time::Instant::now();
    h.host.select(&ctx, k);
    h.frame(vec![]);
    let first = ms(t.elapsed());
    h.frame(vec![]);
    take_prof();
    // Idle frames with the caret in the middle.
    let mid = (0..=doc.len() / 2)
        .rev()
        .find(|i| doc.is_char_boundary(*i))
        .unwrap();
    h.set_sel(mid, mid);
    take_prof();
    let mut idle = Vec::new();
    for _ in 0..reps {
        let t = std::time::Instant::now();
        h.frame(vec![]);
        idle.push(ms(t.elapsed()));
    }
    let mut prof = take_prof();
    for p in &mut prof {
        p.1 /= reps as f64;
        p.2 /= reps as u32;
    }
    take_prof();
    let mut typing = Vec::new();
    for i in 0..reps {
        let c = ["a", "b", "č", "e"][i % 4];
        let t = std::time::Instant::now();
        h.frame(vec![egui::Event::Text(c.into())]);
        typing.push(ms(t.elapsed()));
        h.frame(vec![]);
    }
    let mut tprof = take_prof();
    for p in &mut tprof {
        p.1 /= reps as f64;
    }
    let mut scroll = Vec::new();
    h.frame(vec![egui::Event::PointerMoved(Pos2::new(500.0, 300.0))]);
    for _ in 0..reps {
        let t = std::time::Instant::now();
        h.frame(vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: Vec2::new(0.0, -90.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        }]);
        scroll.push(ms(t.elapsed()));
    }
    let mut back = Vec::new();
    for _ in 0..3 {
        h.select(0);
        let t = std::time::Instant::now();
        h.host.select(&ctx, k);
        h.frame(vec![]);
        back.push(ms(t.elapsed()));
        h.frame(vec![]);
    }
    Row {
        name: name.into(),
        first,
        idle: med_max(&mut idle),
        typing: med_max(&mut typing),
        scroll: med_max(&mut scroll),
        back: med_max(&mut back),
        prof,
        tprof,
    }
}

/// `cargo test --release --lib -- --ignored --nocapture --test-threads=1 perf_matrix`
#[test]
#[ignore = "measurement; run in release mode with --nocapture"]
fn perf_matrix() {
    let reps: usize = std::env::var("PERF_REPS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(9);
    let big: usize = std::env::var("PERF_BYTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200_000);
    let mut rows = Vec::new();
    for kind in ["plain", "styled", "tasks", "tables", "mixed"] {
        rows.push(bench(
            &format!("{} {} KB", kind, big / 1000),
            &fixture(kind, big),
            reps,
        ));
    }
    rows.push(bench(
        "mixed 3 KB (everyday note)",
        &fixture("mixed", 3_000),
        reps,
    ));
    rows.push(bench("tasks 3 KB", &fixture("tasks", 3_000), reps));
    eprintln!("PERF MATRIX (ms; median / max over {reps} frames; first = one frame)");
    eprintln!(
        "PERF {:<28}{:>9}{:>16}{:>16}{:>16}{:>16}",
        "fixture", "first", "idle", "typing", "scroll", "switch-back"
    );
    for r in &rows {
        eprintln!(
            "PERF {:<28}{:>9.1}{:>9.1}/{:<6.1}{:>9.1}/{:<6.1}{:>9.1}/{:<6.1}{:>9.1}/{:<6.1}",
            r.name,
            r.first,
            r.idle.0,
            r.idle.1,
            r.typing.0,
            r.typing.1,
            r.scroll.0,
            r.scroll.1,
            r.back.0,
            r.back.1
        );
    }
    for r in &rows {
        let parts: Vec<String> = r
            .prof
            .iter()
            .filter(|p| p.1 >= 0.05)
            .map(|p| format!("{} {:.2}", p.0, p.1))
            .collect();
        eprintln!(
            "PERF stages per idle frame, {}: {}",
            r.name,
            parts.join(" | ")
        );
        let parts: Vec<String> = r
            .tprof
            .iter()
            .filter(|p| p.1 >= 0.3)
            .map(|p| format!("{} {:.2}", p.0, p.1))
            .collect();
        eprintln!(
            "PERF stages per typed character (typing frame plus the idle frame after it), {}: {}",
            r.name,
            parts.join(" | ")
        );
    }
}

// ---- E3.1: the optimised per-frame passes must give the old answers ----

fn tasks_doc(n: usize) -> String {
    let mut s = String::from("Úvod 🦀 žluťoučký kůň\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "- [{}] úkol č. {i} — příliš žluťoučký\n",
            if i % 3 == 0 { "x" } else { " " }
        ));
    }
    s.push_str("\nkonec\n");
    s
}

fn tables_doc(n: usize) -> String {
    let mut s = String::from("Úvod\n\n");
    for i in 0..n {
        s.push_str(&format!(
            "| a | b |\n| --- | --- |\n| {i} | žluťoučký {i} |\n\ntext {i}\n\n"
        ));
    }
    s
}

fn mixed_doc() -> String {
    let mut s = String::from("# Nadpis 🦀\n\n");
    for i in 0..40 {
        s.push_str(&format!("odstavec č. {i} s **tučným** a [odkazem](https://x.org)\n\n- [ ] úkol {i}\n- [x] hotovo {i}\n\n> citace {i}\n\n---\n\n```\nkód {i}\n```\n\n| h1 | h2 |\n| --- | --- |\n| {i} | x |\n\n"));
    }
    s
}

fn one_note(text: &str) -> Mh {
    let mut h = Mh::new(&[("n", text)]);
    h.settle();
    h
}

fn task_rects(h: &Mh) -> Vec<(usize, Rect)> {
    h.host.view.probe.task_rects.clone()
}

fn scroll_by(h: &mut Mh, dy: f32) {
    h.frame(vec![egui::Event::PointerMoved(Pos2::new(600.0, 300.0))]);
    h.frame(vec![egui::Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: Vec2::new(0.0, dy),
        phase: egui::TouchPhase::Move,
        source: egui::MouseWheelSource::Wheel,
        modifiers: Modifiers::NONE,
    }]);
    h.settle();
}

/// Caret and view to the very top (Ctrl+Home does not move this editor's caret in a headless run,
/// so the caret is set directly and the view is scrolled with the wheel until it stops moving).
fn go_top(h: &mut Mh) {
    h.set_sel(0, 0);
    scroll_until_still(h, 3000.0);
}

fn go_end(h: &mut Mh) {
    h.end();
    scroll_until_still(h, -3000.0);
}

fn scroll_until_still(h: &mut Mh, dy: f32) {
    let probe = |h: &Mh| {
        (
            task_rects(h).first().map(|r| r.0),
            h.host.view.probe.tables.first().copied(),
        )
    };
    let mut last = probe(h);
    for _ in 0..200 {
        scroll_by(h, dy);
        let now = probe(h);
        if now == last {
            return;
        }
        last = now;
    }
}

/// The pre-optimisation algorithm, kept here as the reference the fast one must agree with.
fn ref_task_boxes(
    galley: &egui::Galley,
    text: &str,
    spans: &[Span],
    sel: &std::ops::Range<usize>,
) -> Vec<(usize, bool, Rect)> {
    let mut out = Vec::new();
    for sp in spans {
        let markdown::Kind::Task { checked } = sp.kind else {
            continue;
        };
        if layout::touches(&sp.reveal, sel) || sp.range.end > text.len() {
            continue;
        }
        let at = |b: usize| galley.pos_from_cursor(CCursor::new(edit::byte_to_char(text, b)));
        let (r0, r1) = (at(sp.range.start), at(sp.range.end));
        if r1.min.x <= r0.min.x {
            continue;
        }
        let side = (BASE_SIZE * 0.78).min(r1.min.x - r0.min.x);
        let c = Pos2::new((r0.min.x + r1.min.x) / 2.0, r0.center().y);
        out.push((
            sp.range.start,
            checked,
            Rect::from_center_size(c, Vec2::splat(side)),
        ));
    }
    out
}

fn ref_decor(
    decor: &[layout::Decor],
    galley: &egui::Galley,
    text: &str,
    origin: Pos2,
    width: f32,
    pal: &Palette,
) -> Vec<egui::Shape> {
    let at = |b: usize| galley.pos_from_cursor(CCursor::new(edit::byte_to_char(text, b)));
    let rows_y = |r: &std::ops::Range<usize>| {
        let end = r.start + text[r.clone()].trim_end_matches(['\n', '\r']).len();
        (at(r.start).min.y, at(end).max.y)
    };
    let valid = |r: &std::ops::Range<usize>| {
        r.start <= r.end
            && r.end <= text.len()
            && text.is_char_boundary(r.start)
            && text.is_char_boundary(r.end)
    };
    let mut out = Vec::new();
    for d in decor {
        match d {
            layout::Decor::Rule { at: a } if *a < text.len() && text.is_char_boundary(*a) => {
                let y = origin.y + at(*a).center().y;
                out.push(egui::Shape::line_segment(
                    [Pos2::new(origin.x, y), Pos2::new(origin.x + width, y)],
                    egui::Stroke::new(1.5, pal.rule),
                ));
            }
            layout::Decor::Quote { range, depth } if valid(range) => {
                let (top, bottom) = rows_y(range);
                let x = origin.x - 7.0 - 5.0 * f32::from(*depth);
                out.push(egui::Shape::rect_filled(
                    Rect::from_min_max(
                        Pos2::new(x, origin.y + top),
                        Pos2::new(x + 3.0, origin.y + bottom),
                    ),
                    1.5,
                    pal.quote_border,
                ));
            }
            layout::Decor::CodeBlock { range } if valid(range) => {
                let (top, bottom) = rows_y(range);
                out.push(egui::Shape::rect_filled(
                    Rect::from_min_max(
                        Pos2::new(origin.x - 6.0, origin.y + top - 1.0),
                        Pos2::new(origin.x + width + 6.0, origin.y + bottom + 1.0),
                    ),
                    4.0,
                    pal.code_block_bg,
                ));
            }
            _ => {}
        }
    }
    out
}

#[test]
fn optimised_task_boxes_decorations_and_table_heights_equal_the_reference_algorithm() {
    let text = mixed_doc();
    let mut h = one_note(&text);
    h.end();
    let (galley, _) = h.host.view.probe.galley.clone().unwrap();
    let spans = markdown::parse(&text);
    let offsets = Offsets::build(&text);
    let all = Geo::new(&text, &offsets, &galley, (f32::MIN, f32::MAX));
    // Selection at various places, including inside a task and a table (revealed source).
    let task_at = text.find("[ ]").unwrap();
    let table_at = text.find("| h1").unwrap() + 3;
    for sel in [
        0..0,
        text.len()..text.len(),
        task_at + 1..task_at + 1,
        table_at..table_at + 4,
    ] {
        let fast = task_boxes(&all, &spans, &sel);
        let slow = ref_task_boxes(&galley, &text, &spans, &sel);
        assert_eq!(fast.len(), slow.len());
        assert!(fast.len() >= 70);
        for (f, s) in fast.iter().zip(&slow) {
            assert_eq!((f.start, f.checked, f.rect), (s.0, s.1, s.2));
        }
        let decor = layout::decorations(&spans, &sel);
        let pal = Palette::fallback();
        let origin = Pos2::new(16.0, 40.0);
        assert_eq!(
            decor_shapes(&decor, &all, origin, 700.0, &pal),
            ref_decor(&decor, &galley, &text, origin, 700.0, &pal)
        );
    }
    // Table heights from the cache are the heights of a fresh layout, for several widths.
    let ctx = h.ctx.clone();
    let (heights, layouts) = (RefCell::new(HashMap::new()), RefCell::new(HashMap::new()));
    for wrap in [300.0f32, 500.0, 780.0] {
        for sp in &spans {
            let markdown::Kind::Table(t) = &sp.kind else {
                continue;
            };
            let fresh = table::layout(&ctx, &text, t, wrap, BASE_SIZE, None).height;
            for _ in 0..2 {
                assert_eq!(
                    table_height(&heights, &layouts, &ctx, &text, t, wrap, None),
                    fresh
                );
            }
        }
    }
    assert!(
        heights.borrow().len() >= 40,
        "heights are cached per table and width"
    );
}

#[test]
fn only_checkboxes_near_the_viewport_get_widgets_and_the_visible_ones_are_all_there() {
    let text = tasks_doc(3000);
    let mut h = one_note(&text);
    go_top(&mut h);
    let top = task_rects(&h);
    assert!(
        !top.is_empty() && top.len() < 120,
        "culled to the viewport: {}",
        top.len()
    );
    assert_eq!(
        top[0].0,
        text.find("[x]").unwrap(),
        "the first task is at the top"
    );
    // Every task that is really on screen has a box.
    let on_screen: Vec<_> = top
        .iter()
        .filter(|(_, r)| r.min.y >= 0.0 && r.max.y <= 700.0)
        .collect();
    assert!(on_screen.len() >= 15, "{}", on_screen.len());
    for w in top.windows(2) {
        assert!(w[0].0 < w[1].0 && w[0].1.min.y < w[1].1.min.y, "ordered");
    }
}

#[test]
fn clicking_a_checkbox_at_the_start_middle_and_end_of_a_large_document_toggles_that_task_only() {
    let text = tasks_doc(3000);
    let mut h = one_note(&text);
    for place in ["start", "middle", "end"] {
        match place {
            "start" => h.key(Key::Home, CTRL),
            "end" => h.key(Key::End, CTRL),
            _ => {
                go_top(&mut h);
                for _ in 0..80 {
                    scroll_by(&mut h, -400.0);
                    if task_rects(&h).first().is_some_and(|r| r.0 > text.len() / 2) {
                        break;
                    }
                }
            }
        }
        h.settle();
        let before = h.txt(0);
        let rects = task_rects(&h);
        assert!(!rects.is_empty(), "{place}: tasks visible");
        let (start, rect) = rects[rects.len() / 2];
        assert!(
            rect.min.y >= -5.0 && rect.max.y <= 705.0,
            "{place}: on screen {rect:?}"
        );
        let c = rect.center();
        h.frame(vec![egui::Event::PointerMoved(c)]);
        for pressed in [true, false] {
            h.frame(vec![egui::Event::PointerButton {
                pos: c,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }]);
        }
        h.settle();
        let after = h.txt(0);
        assert_eq!(before.len(), after.len(), "{place}: one byte swapped");
        let diff: Vec<usize> = before
            .bytes()
            .zip(after.bytes())
            .enumerate()
            .filter(|(_, (a, b))| a != b)
            .map(|(i, _)| i)
            .collect();
        assert_eq!(
            diff,
            vec![start + 1],
            "{place}: exactly the clicked box changed"
        );
        let flipped = after.as_bytes()[start + 1];
        assert!(flipped == b'x' || flipped == b' ');
        // One undo step brings it back, wherever the view is scrolled.
        h.undo();
        assert_eq!(h.txt(0), before, "{place}: undo restores");
        h.redo();
        assert_eq!(h.txt(0), after, "{place}: redo");
        h.undo();
    }
}

#[test]
fn undo_and_redo_work_after_interacting_with_off_screen_blocks() {
    let text = tasks_doc(1500);
    let mut h = one_note(&text);
    go_end(&mut h);
    let (start, rect) = task_rects(&h)
        .into_iter()
        .rfind(|(_, r)| r.max.y < 690.0)
        .unwrap();
    let c = rect.center();
    h.frame(vec![egui::Event::PointerMoved(c)]);
    for pressed in [true, false] {
        h.frame(vec![egui::Event::PointerButton {
            pos: c,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }]);
    }
    h.settle();
    let toggled = h.txt(0);
    assert_ne!(toggled, text);
    // Jump to the top and type there; the far-away box is now off screen.
    go_top(&mut h);
    h.text("X");
    assert!(h.txt(0).starts_with('X'));
    assert!(
        !task_rects(&h).iter().any(|(s, _)| *s == start + 1),
        "the toggled task is far away"
    );
    h.undo();
    assert_eq!(h.txt(0), toggled, "typing undone, toggle intact");
    // egui also makes a caret jump an undo point, so a further press may only move the caret;
    // within three presses the toggle is undone, and nothing but the two known texts ever shows.
    let mut presses = 0;
    while h.txt(0) != text && presses < 3 {
        h.undo();
        presses += 1;
        assert!(
            h.txt(0) == text || h.txt(0) == toggled,
            "only known states appear"
        );
    }
    assert_eq!(
        h.txt(0),
        text,
        "toggle undone from the other end of the document"
    );
    let mut presses = 0;
    while !h.txt(0).starts_with('X') && presses < 4 {
        h.redo();
        presses += 1;
    }
    assert!(h.txt(0).starts_with('X') && h.txt(0).len() == text.len() + 1);
}

#[test]
fn tables_straddling_the_viewport_edges_are_painted_and_far_ones_are_not() {
    let text = tables_doc(400);
    let mut h = one_note(&text);
    go_top(&mut h);
    let top = h.host.view.probe.tables.clone();
    assert!(!top.is_empty() && top.len() < 40, "{}", top.len());
    assert_eq!(top[0], text.find("| a").unwrap());
    // Scroll a little at a time: the set of painted tables always includes every table whose first
    // row is on screen, and never one hundreds of screens away.
    let mut last_max: usize = 0;
    for _ in 0..30 {
        scroll_by(&mut h, -170.0);
        let drawn = h.host.view.probe.tables.clone();
        assert!(!drawn.is_empty());
        assert!(drawn.windows(2).all(|w| w[0] < w[1]));
        assert!(drawn[0] >= last_max, "scrolling forward");
        assert!(drawn.len() < 40);
        last_max = drawn[0];
    }
    assert!(last_max > top[0] + 500, "the view moved down the document");
    for _ in 0..30 {
        scroll_by(&mut h, 170.0);
    }
    assert_eq!(
        h.host.view.probe.tables, top,
        "scrolling back repaints exactly the first tables"
    );
}

#[test]
fn a_table_turns_into_its_source_while_the_caret_is_in_it_and_back_when_it_leaves() {
    let text = tables_doc(6);
    let mut h = one_note(&text);
    go_top(&mut h);
    let first = text.find("| a").unwrap();
    assert!(
        h.host.view.probe.tables.contains(&first),
        "rendered as a grid"
    );
    h.set_sel(first + 3, first + 3);
    assert!(
        !h.host.view.probe.tables.contains(&first),
        "source shown for the table with the caret"
    );
    assert!(
        h.host.view.probe.tables.len() >= 4,
        "the other tables stay rendered"
    );
    h.text("Z");
    assert!(h.txt(0).contains("| aZ"), "editable raw source");
    go_top(&mut h);
    assert!(
        h.host.view.probe.tables.contains(&first),
        "grid again after the caret left"
    );
    assert!(
        h.host.view.probe.tables.len() >= 4,
        "the visible tables are painted"
    );
}

// ---- E3.1: cache invalidation ----

fn set_screen(w: f32, hgt: f32) {
    SCREEN.with(|s| s.set(Vec2::new(w, hgt)));
}

fn rects_at_top(text: &str) -> Vec<(usize, Rect)> {
    let mut h = one_note(text);
    go_top(&mut h);
    task_rects(&h)
}

fn same_rects(a: &[(usize, Rect)], b: &[(usize, Rect)]) -> bool {
    let n = a.len().min(b.len());
    n > 5 && a[..n] == b[..n]
}

#[test]
fn inserting_and_removing_lines_before_cached_elements_moves_them_exactly() {
    let text = mixed_doc();
    let mut h = one_note(&text);
    go_top(&mut h);
    let before = task_rects(&h);
    h.set_sel(0, 0);
    h.key(Key::Enter, Modifiers::NONE);
    let moved_text = h.txt(0);
    assert_eq!(moved_text.len(), text.len() + 1);
    go_top(&mut h);
    let after = task_rects(&h);
    let fresh = rects_at_top(&moved_text);
    assert!(
        same_rects(&after, &fresh),
        "a cached view equals a fresh one after the insert"
    );
    let n = before.len().min(after.len()).min(6);
    let dy: Vec<f32> = (0..n)
        .map(|i| after[i].1.min.y - before[i].1.min.y)
        .collect();
    assert!(
        dy.iter().all(|d| (*d - dy[0]).abs() < 0.01 && *d > 5.0),
        "every box moved down by one line: {dy:?}"
    );
    for i in 0..n {
        assert_eq!(
            after[i].0,
            before[i].0 + 1,
            "offsets shift by the inserted byte"
        );
    }
    h.set_sel(1, 1);
    h.key(Key::Backspace, Modifiers::NONE);
    assert_eq!(h.txt(0), text);
    go_top(&mut h);
    assert_eq!(
        task_rects(&h),
        before,
        "removing the line puts everything back"
    );
}

#[test]
fn a_window_width_change_reflows_tasks_and_re_measures_tables() {
    let text = mixed_doc();
    set_screen(1000.0, 700.0);
    let mut h = one_note(&text);
    go_top(&mut h);
    let wide = task_rects(&h);
    for width in [420.0f32, 700.0, 420.0, 1000.0] {
        set_screen(width, 700.0);
        h.settle();
        go_top(&mut h);
        let got = task_rects(&h);
        let fresh = rects_at_top(&text);
        assert!(
            same_rects(&got, &fresh),
            "width {width}: same as a fresh view"
        );
    }
    assert_eq!(
        task_rects(&h),
        wide,
        "back at the first width everything is where it was"
    );
    set_screen(1000.0, 700.0);
}

/// Every cached table height must equal a fresh measurement under the current fonts.
fn assert_no_stale_heights(h: &Mh) {
    let heights = h.host.view.heights.borrow();
    assert!(!heights.is_empty());
    for ((_, wrap, _, _), (body, cached)) in heights.iter() {
        let spans = markdown::parse(body);
        let t = spans
            .iter()
            .find_map(|s| match &s.kind {
                markdown::Kind::Table(t) => Some(t),
                _ => None,
            })
            .unwrap();
        let fresh = table::layout(&h.ctx, body, t, f32::from_bits(*wrap), BASE_SIZE, None).height;
        assert_eq!(*cached, fresh, "stale table height for {body:?}");
    }
}

#[test]
fn a_font_change_re_measures_and_a_theme_change_keeps_geometry() {
    let text = mixed_doc();
    let mut h = one_note(&text);
    go_top(&mut h);
    let before = task_rects(&h);
    // Theme: colours only.
    let mut dark = Palette::fallback();
    dark.link = Color32::from_rgb(9, 99, 199);
    h.host.palette = dark;
    h.settle();
    assert_eq!(task_rects(&h), before, "a colour change moves nothing");
    // Fonts: all glyph metrics scale up.
    let bigger = |ctx: &Context| {
        let mut fd = egui::FontDefinitions::default();
        for d in fd.font_data.values_mut() {
            std::sync::Arc::make_mut(d).tweak.scale = 1.4;
        }
        ctx.set_fonts(fd);
    };
    bigger(&h.ctx);
    h.settle();
    go_top(&mut h);
    let scaled = task_rects(&h);
    assert_ne!(scaled, before, "text got bigger");
    assert_no_stale_heights(&h);
    let mut f = one_note(&text);
    bigger(&f.ctx);
    f.settle();
    go_top(&mut f);
    assert!(
        same_rects(&scaled, &task_rects(&f)),
        "tables re-measured with the new fonts, as in a fresh view"
    );
}

#[test]
fn switching_between_notes_with_different_tables_and_tasks_shows_each_one_correctly() {
    let (a, b, c) = (tables_doc(30), mixed_doc(), tasks_doc(200));
    let mut h = Mh::new(&[("A", &a), ("B", &b), ("C", &c)]);
    h.settle();
    let fresh_tables = |t: &str| {
        let mut f = one_note(t);
        go_top(&mut f);
        f.host.view.probe.tables.clone()
    };
    let want = [
        (fresh_tables(&a), rects_at_top(&a)),
        (fresh_tables(&b), rects_at_top(&b)),
        (fresh_tables(&c), rects_at_top(&c)),
    ];
    for order in [
        vec![0, 1, 2],
        vec![2, 0, 1],
        vec![1, 1, 0],
        vec![0, 2, 1, 0, 2],
    ] {
        for i in order {
            h.select(i);
            go_top(&mut h);
            assert_eq!(
                h.host.view.probe.tables, want[i].0,
                "note {i}: painted tables"
            );
            assert!(
                same_rects(&task_rects(&h), &want[i].1) || want[i].1.len() <= 5,
                "note {i}: checkboxes"
            );
        }
    }
    // Content-keyed caches cannot leak: the same table text in two notes measures the same, and a
    // table that differs in one cell never reuses another's cache entry.
    let t1 = "| a | b |\n| --- | --- |\n| 1 | 2 |\n";
    let t2 = "| a | b |\n| --- | --- |\n| 1 | a much longer cell that wraps over several lines in a narrow column, again and again |\n";
    set_screen(500.0, 700.0);
    let mut x = Mh::new(&[("x", t1), ("y", t2)]);
    x.settle();
    let h1 = x.host.view.heights.borrow().len();
    x.select(1);
    x.settle();
    assert!(
        x.host.view.heights.borrow().len() > h1,
        "a different table gets its own entry"
    );
    let g = x.host.view.probe.galley.clone().unwrap().0.rect.height();
    x.select(0);
    x.settle();
    let g0 = x.host.view.probe.galley.clone().unwrap().0.rect.height();
    assert!(
        g > g0,
        "the long table is taller than the short one: {g} vs {g0}"
    );
    set_screen(1000.0, 700.0);
}

#[test]
fn repeated_scrolling_never_leaves_stale_checkbox_positions() {
    let text = tasks_doc(2500);
    let mut h = one_note(&text);
    go_top(&mut h);
    let top = task_rects(&h);
    for round in 0..4 {
        for _ in 0..25 {
            scroll_by(&mut h, -230.0);
            let rects = task_rects(&h);
            assert!(!rects.is_empty());
            for (start, r) in &rects {
                assert!(
                    h.txt(0)[*start..].starts_with('['),
                    "a task box starts at its bracket"
                );
                assert!(
                    r.width() > 3.0 && r.min.y > -400.0 && r.max.y < 1100.0,
                    "round {round}: {r:?}"
                );
            }
            for w in rects.windows(2) {
                assert!(w[0].0 < w[1].0 && w[0].1.min.y < w[1].1.min.y);
            }
        }
        for _ in 0..25 {
            scroll_by(&mut h, 230.0);
        }
        scroll_until_still(&mut h, 3000.0);
        assert_eq!(
            task_rects(&h),
            top,
            "round {round}: back at the top, the same boxes at the same places"
        );
    }
}

#[test]
fn the_table_height_cache_is_bounded() {
    let mut h = one_note("x\n");
    h.settle();
    let (heights, layouts) = (RefCell::new(HashMap::new()), RefCell::new(HashMap::new()));
    for i in 0..(MAX_HEIGHTS + 300) {
        let text = format!("| a | b |\n| --- | --- |\n| {i} | x |\n");
        let spans = markdown::parse(&text);
        let markdown::Kind::Table(t) = &spans[0].kind else {
            panic!("table")
        };
        table_height(&heights, &layouts, &h.ctx, &text, t, 600.0, None);
        assert!(heights.borrow().len() <= MAX_HEIGHTS);
        assert!(layouts.borrow().len() <= MAX_LAYOUTS + 1);
    }
    assert!(
        heights.borrow().len() < MAX_HEIGHTS,
        "cleared once full, then refilled"
    );
}
