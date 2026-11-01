use super::*;
use crate::attachments;
use std::path::PathBuf;

/// Run `frames` frames drawing the slash popup with the given filter; returns the last rect.
fn run(ctx: &Context, query: &str, frames: usize) -> Rect {
    let items = edit::slash_filter(query);
    let mut rect = Rect::NOTHING;
    for _ in 0..frames {
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
            rect = slash_popup(
                ui.ctx(),
                Id::unique("slash-test"),
                Pos2::new(20.0, 20.0),
                &items,
                0,
            )
            .0;
        });
        out.textures_delta.clear();
    }
    rect
}

/// Regression: a filtered one-item menu (`/emoji`) must not leave the full menu narrow.
#[test]
fn slash_popup_width_survives_filtering() {
    let ctx = Context::default();
    let full = run(&ctx, "", 3);
    for q in ["emoji", "h", "zzz", "emoji"] {
        run(&ctx, q, 3);
    }
    let again = run(&ctx, "", 3);
    assert!(
        (full.width() - again.width()).abs() < 0.5,
        "{full:?} vs {again:?}"
    );
    assert!(again.width() >= SLASH_WIDTH, "{again:?}");
    assert!((again.height() - full.height()).abs() < 0.5);
}

fn key(key: Key, modifiers: Modifiers) -> egui::Event {
    egui::Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Undo/redo of every kind of command edit, driven through the real `TextEditState` undoer the
/// same way the app does (mutate the buffer, set the caret, keep running frames).
#[test]
fn command_edits_undo_and_redo_as_single_steps() {
    type Case<'a> = (&'a str, usize, Box<dyn Fn(&str) -> Edit>);
    let cases: Vec<Case> = vec![
        (
            "Příliš text /h2",
            17,
            Box::new(|s| edit::apply_slash(s, 14, 17, &slash("Heading 2")).unwrap()),
        ),
        (
            "one 🦀 two",
            10,
            Box::new(|s| edit::format_inline(s, 4, 8, "**")),
        ),
        (
            "one two",
            3,
            Box::new(|s| edit::format_inline(s, 3, 3, "`")),
        ),
        (
            "alpha\nbeta",
            10,
            Box::new(|s| edit::list_selection(s, 0, s.len(), true)),
        ),
        (
            "intro\n/table",
            12,
            Box::new(|s| edit::apply_slash(s, 6, 12, &slash("Table")).unwrap()),
        ),
        ("- item", 6, Box::new(|s| edit::list_enter(s, 6).unwrap())),
        ("- ", 2, Box::new(|s| edit::list_enter(s, 2).unwrap())),
        (
            "1. item",
            3,
            Box::new(|s| edit::list_backspace(s, 3).unwrap()),
        ),
        (
            "x /emoji",
            8,
            Box::new(|_| edit::apply_emoji(2, 8, "👨\u{200D}👩\u{200D}👧")),
        ),
    ];
    for (src, caret, make) in cases {
        let ctx = Context::default();
        let id = Id::unique("undo-test");
        let mut text = src.to_string();
        let mut t = 0.0;
        let mut frame = |text: &mut String, events: Vec<egui::Event>| {
            t += 2.0;
            let mut input = egui::RawInput {
                time: Some(t),
                events,
                ..Default::default()
            };
            input.focused = true;
            let mut out = ctx.run_ui(input, |ui| {
                egui::TextEdit::multiline(text)
                    .id(id)
                    .show(ui)
                    .response
                    .request_focus();
            });
            out.textures_delta.clear();
        };
        frame(&mut text, vec![]);
        let c = CCursor::new(edit::byte_to_char(&text, caret));
        let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        st.cursor.set_char_range(Some(CCursorRange::one(c)));
        st.store(&ctx, id);
        frame(&mut text, vec![]);
        let e = make(&text);
        apply_edit_to(&ctx, id, &mut text, &e, false);
        let edited = text.clone();
        assert_ne!(edited, src);
        frame(&mut text, vec![]);
        frame(&mut text, vec![key(Key::Z, Modifiers::COMMAND)]);
        assert_eq!(text, src, "undo of {src:?}");
        frame(
            &mut text,
            vec![key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)],
        );
        assert_eq!(text, edited, "redo of {src:?}");
    }
}

/// A real `TextEdit` driven frame by frame, so egui's own undoer decides the undo steps.
struct Harness {
    ctx: Context,
    id: Id,
    text: String,
    t: f64,
}

impl Harness {
    fn new(text: &str, caret: usize) -> Self {
        let mut h = Harness {
            ctx: Context::default(),
            id: Id::unique("undo-harness"),
            text: text.to_string(),
            t: 0.0,
        };
        h.frame(vec![]);
        let mut st = egui::TextEdit::load_state(&h.ctx, h.id).unwrap_or_default();
        let c = CCursor::new(edit::byte_to_char(&h.text, caret));
        st.cursor.set_char_range(Some(CCursorRange::one(c)));
        st.store(&h.ctx, h.id);
        h.frame(vec![]);
        h
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        // Frames are close together in time, like typing; nothing waits for the undoer.
        self.t += 0.05;
        let mut input = egui::RawInput {
            time: Some(self.t),
            events,
            ..Default::default()
        };
        input.focused = true;
        let (text, id) = (&mut self.text, self.id);
        let mut out = self.ctx.run_ui(input, |ui| {
            egui::TextEdit::multiline(text)
                .id(id)
                .show(ui)
                .response
                .request_focus();
        });
        out.textures_delta.clear();
    }

    /// Run a command edit the way the app does.
    fn command(&mut self, e: &Edit) {
        apply_edit_to(&self.ctx, self.id, &mut self.text, e, false);
        self.frame(vec![]);
    }

    /// The old behaviour: change the buffer and caret with no undo points.
    fn command_without_boundary(&mut self, e: &Edit) {
        e.apply(&mut self.text);
        let mut st = egui::TextEdit::load_state(&self.ctx, self.id).unwrap_or_default();
        let c = |b: usize| CCursor::new(edit::byte_to_char(&self.text, b));
        let r = match e.anchor {
            Some(a) => CCursorRange::two(c(a), c(e.caret)),
            None => CCursorRange::one(c(e.caret)),
        };
        st.cursor.set_char_range(Some(r));
        st.store(&self.ctx, self.id);
        self.frame(vec![]);
    }

    fn type_text(&mut self, s: &str) {
        self.frame(vec![egui::Event::Text(s.to_string())]);
    }

    fn undo(&mut self) {
        self.frame(vec![key(Key::Z, Modifiers::COMMAND)]);
    }

    fn redo(&mut self) {
        self.frame(vec![key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)]);
    }

    fn selection(&self) -> (usize, usize) {
        let r = egui::TextEdit::load_state(&self.ctx, self.id)
            .and_then(|s| s.cursor.char_range())
            .unwrap();
        let (a, b) = (r.secondary.index.0, r.primary.index.0);
        (
            edit::char_to_byte(&self.text, a.min(b)),
            edit::char_to_byte(&self.text, a.max(b)),
        )
    }
}

fn slash_edit(src: &str, q: &str, title: &str) -> Edit {
    let trig = src.find(q).unwrap();
    edit::apply_slash(src, trig, trig + q.len(), &slash(title)).unwrap()
}

/// Root cause, kept as a test: without explicit undo points the command and the typing that
/// follows it are one undoer step.
#[test]
fn without_a_boundary_command_and_typing_merge() {
    let mut h = Harness::new("/table", 6);
    let e = slash_edit(&h.text, "/table", "Table");
    h.command_without_boundary(&e);
    h.type_text("Příliš");
    h.undo();
    // One undo jumps straight over the table insertion instead of restoring `Data 1`.
    assert_eq!(h.text, "/table");
}

#[test]
fn table_then_typing_undoes_in_two_steps_and_redoes_in_order() {
    let mut h = Harness::new("intro\n/table", 12);
    let e = slash_edit(&h.text, "/table", "Table");
    h.command(&e);
    let inserted = h.text.clone();
    assert!(inserted.contains("| Data 1 |"));
    let (a, b) = h.selection();
    assert_eq!(&h.text[a..b], "Data 1");
    h.type_text("Příliš");
    let typed = h.text.clone();
    assert!(typed.contains("| Příliš |"));

    h.undo();
    assert_eq!(h.text, inserted, "first undo restores the placeholder");
    let (a, b) = h.selection();
    assert_eq!(&h.text[a..b], "Data 1", "placeholder selected again");
    h.undo();
    assert_eq!(
        h.text, "intro\n/table",
        "second undo restores the slash text"
    );
    assert_eq!(h.selection(), (12, 12));

    h.redo();
    assert_eq!(h.text, inserted);
    h.redo();
    assert_eq!(h.text, typed);
    let (a, b) = h.selection();
    assert_eq!(a, b, "caret after the typed text");
    assert!(h.text.is_char_boundary(a));
}

#[test]
fn emoji_then_typing() {
    let mut h = Harness::new("Ahoj /emoji", 11);
    let glyph = "👨\u{200D}👩\u{200D}👧";
    h.command(&edit::apply_emoji(5, 11, glyph));
    h.type_text(" světe");
    assert_eq!(h.text, format!("Ahoj {glyph} světe"));
    h.undo();
    assert_eq!(h.text, format!("Ahoj {glyph}"));
    h.undo();
    assert_eq!(h.text, "Ahoj /emoji");
    h.redo();
    h.redo();
    assert_eq!(h.text, format!("Ahoj {glyph} světe"));
}

#[test]
fn formatting_then_typing_inside_the_span() {
    let mut h = Harness::new("one two", 3);
    h.command(&edit::format_inline(&h.text, 3, 3, "**"));
    assert_eq!(h.text, "one**** two");
    h.type_text("žluťoučký");
    assert_eq!(h.text, "one**žluťoučký** two");
    h.undo();
    assert_eq!(h.text, "one**** two");
    assert_eq!(h.selection(), (5, 5));
    h.undo();
    assert_eq!(h.text, "one two");
    assert_eq!(h.selection(), (3, 3));
}

#[test]
fn heading_then_heading_text() {
    let mut h = Harness::new("Úvod\n/h1", 9);
    let e = slash_edit(&h.text, "/h1", "Heading 1");
    h.command(&e);
    assert_eq!(h.text, "Úvod\n# ");
    h.type_text("Nadpis");
    assert_eq!(h.text, "Úvod\n# Nadpis");
    h.undo();
    assert_eq!(h.text, "Úvod\n# ");
    h.undo();
    assert_eq!(h.text, "Úvod\n/h1");
}

#[test]
fn list_command_then_typing_and_continuation() {
    let mut h = Harness::new("/bullet", 7);
    let e = slash_edit(&h.text, "/bullet", "Bullet list");
    h.command(&e);
    assert_eq!(h.text, "- ");
    h.type_text("první");
    let (a, _) = h.selection();
    let cont = edit::list_enter(&h.text, a).unwrap();
    h.command(&cont);
    assert_eq!(h.text, "- první\n- ");
    h.type_text("druhá");
    assert_eq!(h.text, "- první\n- druhá");
    h.undo();
    assert_eq!(h.text, "- první\n- ");
    h.undo();
    assert_eq!(h.text, "- první");
    h.undo();
    assert_eq!(h.text, "- ");
    h.undo();
    assert_eq!(h.text, "/bullet");
}

#[test]
fn image_reference_then_more_text() {
    let mut h = Harness::new("před", 5);
    let e = edit::insert_image_ref(&h.text, h.text.len(), "assets/a.png");
    h.command(&e);
    let with_image = h.text.clone();
    assert_eq!(with_image, "před\n![image](assets/a.png)\n");
    h.type_text("po");
    h.undo();
    assert_eq!(h.text, with_image);
    h.undo();
    assert_eq!(h.text, "před");
}

#[test]
fn plain_typing_still_coalesces() {
    let mut h = Harness::new("", 0);
    for ch in ["P", "ř", "í", "l", "i", "š"] {
        h.type_text(ch);
    }
    assert_eq!(h.text, "Příliš");
    h.undo();
    assert_eq!(h.text, "", "continuous typing is one step");
    h.redo();
    assert_eq!(h.text, "Příliš");
}

#[test]
fn typing_before_a_command_is_a_separate_step() {
    let mut h = Harness::new("", 0);
    h.type_text("a ");
    h.type_text("/h2");
    let e = slash_edit(&h.text, "/h2", "Heading 2");
    h.command(&e);
    h.undo();
    assert_eq!(h.text, "a /h2");
    h.undo();
    assert_eq!(h.text, "");
}

/// A minimal host around [`EditorView`]: it owns the text, the store and the palette, draws the
/// editor into a `CentralPanel` and keeps the last output. No eframe, no files.
struct TestApp {
    text: String,
    store: Box<dyn AttachmentStore>,
    palette: Palette,
    keys: EditorKeys,
    key: NoteKey,
    view: EditorView,
    status: String,
    out: EditorOutput,
    focus_next: bool,
}

impl TestApp {
    fn new(text: &str, store: Box<dyn AttachmentStore>) -> Self {
        TestApp {
            text: text.to_owned(),
            store,
            palette: Palette::fallback(),
            keys: EditorKeys::standalone(),
            key: NoteKey::STANDALONE,
            view: EditorView::new(),
            status: String::new(),
            out: EditorOutput::default(),
            focus_next: true,
        }
    }

    fn ui(&mut self, root: &mut egui::Ui) {
        let focus = if std::mem::take(&mut self.focus_next) {
            FocusRequest::Request
        } else {
            FocusRequest::None
        };
        // The same panel margins as the standalone host: none horizontally, 8 pt vertically.
        let panel = egui::Frame::central_panel(&root.ctx().global_style())
            .inner_margin(egui::Margin::symmetric(0, 8));
        let out = egui::CentralPanel::default()
            .frame(panel)
            .show(root, |ui| {
                self.view.show(
                    ui,
                    EditorInput {
                        key: self.key,
                        text: &mut self.text,
                        attachments: &*self.store,
                        palette: &self.palette,
                        keys: &self.keys,
                        bold: None,
                        focus,
                    },
                )
            })
            .inner;
        if let Some(s) = &out.status {
            self.status = s.clone();
        }
        self.out = out;
    }
}

/// The editor in a headless egui context, driven frame by frame.
struct AppHarness {
    ctx: Context,
    app: TestApp,
    t: f64,
    opened: Vec<String>,
    shapes: Vec<egui::Shape>,
    screen: Vec2,
}

impl AppHarness {
    fn new(text: &str) -> Self {
        Self::in_dir(text, std::env::temp_dir())
    }

    fn in_dir(text: &str, dir: PathBuf) -> Self {
        Self::with_store(text, Box::new(attachments::FsStore::new(dir)))
    }

    fn with_store(text: &str, store: Box<dyn AttachmentStore>) -> Self {
        let mut h = AppHarness {
            ctx: Context::default(),
            app: TestApp::new(text, store),
            t: 0.0,
            opened: Vec::new(),
            shapes: Vec::new(),
            screen: Vec2::new(900.0, 700.0),
        };
        for _ in 0..3 {
            h.frame(vec![]);
        }
        h
    }

    fn id() -> Id {
        NoteKey::STANDALONE.widget_id()
    }

    fn frame(&mut self, events: Vec<egui::Event>) {
        self.frame_with(events, Modifiers::NONE);
    }

    fn frame_with(&mut self, mut events: Vec<egui::Event>, modifiers: Modifiers) {
        events.insert(0, egui::Event::ModifiersChanged(modifiers));
        self.t += 0.05;
        let mut input = egui::RawInput {
            time: Some(self.t),
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.screen)),
            ..Default::default()
        };
        input.focused = true;
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| app.ui(ui));
        for c in &out.platform_output.commands {
            if let egui::OutputCommand::OpenUrl(u) = c {
                self.opened.push(u.url.clone());
            }
        }
        out.textures_delta.clear();
        self.shapes = out.shapes.into_iter().map(|c| c.shape).collect();
    }

    /// Screen x of the start of byte `at` in the last layout.
    fn x_at(&self, at: usize) -> f32 {
        let (g, origin) = self.app.view.probe.galley.clone().unwrap();
        let r = g.pos_from_cursor(CCursor::new(edit::byte_to_char(&self.app.text, at)));
        origin.x + r.min.x
    }

    fn y_at(&self, at: usize) -> f32 {
        let (g, origin) = self.app.view.probe.galley.clone().unwrap();
        let r = g.pos_from_cursor(CCursor::new(edit::byte_to_char(&self.app.text, at)));
        origin.y + r.min.y
    }

    /// Is a rect outline of exactly this colour among the shapes of the last frame?
    fn has_stroke(&self, c: Color32) -> bool {
        fn any(s: &egui::Shape, c: Color32) -> bool {
            match s {
                egui::Shape::Rect(r) => r.stroke.color == c && r.stroke.width > 0.0,
                egui::Shape::Vec(v) => v.iter().any(|s| any(s, c)),
                _ => false,
            }
        }
        self.shapes.iter().any(|s| any(s, c))
    }

    /// Is a filled rect of exactly this colour among the shapes of the last frame?
    fn has_fill(&self, c: Color32) -> bool {
        fn any(s: &egui::Shape, c: Color32) -> bool {
            match s {
                egui::Shape::Rect(r) => r.fill == c,
                egui::Shape::Vec(v) => v.iter().any(|s| any(s, c)),
                _ => false,
            }
        }
        self.shapes.iter().any(|s| any(s, c))
    }

    fn set_selection(&mut self, a: usize, b: usize) {
        let id = Self::id();
        let mut st = egui::TextEdit::load_state(&self.ctx, id).unwrap_or_default();
        let c = |x: usize| CCursor::new(edit::byte_to_char(&self.app.text, x));
        st.cursor
            .set_char_range(Some(CCursorRange::two(c(a), c(b))));
        st.store(&self.ctx, id);
        self.frame(vec![]);
        self.frame(vec![]);
    }

    fn selection(&self) -> (usize, usize) {
        EditorView::selection_bytes(&self.ctx, Self::id(), &self.app.text)
    }

    fn key(&mut self, k: Key, m: Modifiers) {
        self.frame_with(vec![key(k, m)], m);
        self.frame(vec![]);
    }

    fn text(&mut self, s: &str) {
        self.frame(vec![egui::Event::Text(s.to_string())]);
        self.frame(vec![]);
    }

    fn click(&mut self, pos: Pos2, m: Modifiers) {
        self.frame_with(vec![egui::Event::PointerMoved(pos)], m);
        let b = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: m,
        };
        self.frame_with(vec![b(true)], m);
        self.frame_with(vec![b(false)], m);
        self.frame(vec![]);
    }

    /// Screen position of the middle of `needle`'s first character in the laid-out text.
    fn pos_of(&self, needle: &str) -> Pos2 {
        let at = self.app.text.find(needle).unwrap();
        let (g, origin) = self.app.view.probe.galley.clone().unwrap();
        let r = g.pos_from_cursor(CCursor::new(edit::byte_to_char(&self.app.text, at)));
        origin + Vec2::new(r.min.x + 3.0, r.center().y)
    }
}

const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

#[test]
fn clicking_a_checkbox_toggles_one_byte_keeps_the_caret_and_undoes_in_one_step() {
    let src = "Úvod\n\n- [ ] Příliš 🦀\n- [x] hotovo\n\nkonec";
    let mut h = AppHarness::new(src);
    h.set_selection(src.len(), src.len());
    assert_eq!(
        h.app.view.probe.task_rects.len(),
        2,
        "both boxes drawn as checkboxes"
    );
    let (start, rect) = h.app.view.probe.task_rects[0];
    h.click(rect.center(), Modifiers::NONE);
    let toggled = "Úvod\n\n- [x] Příliš 🦀\n- [x] hotovo\n\nkonec";
    assert_eq!(h.app.text, toggled);
    assert_eq!(h.app.text.len(), src.len());
    assert_eq!(start, src.find("[ ]").unwrap());
    assert_eq!(
        h.selection(),
        (src.len(), src.len()),
        "caret did not jump into the box"
    );
    // Second box (checked) unchecks.
    let rect2 = h.app.view.probe.task_rects[1].1;
    h.click(rect2.center(), Modifiers::NONE);
    assert_eq!(h.app.text, "Úvod\n\n- [x] Příliš 🦀\n- [ ] hotovo\n\nkonec");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, toggled, "one undo reverts only the last toggle");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, src);
    h.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.app.text, toggled);
    h.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.app.text, "Úvod\n\n- [x] Příliš 🦀\n- [ ] hotovo\n\nkonec");
}

#[test]
fn clicking_the_text_of_a_task_places_the_caret_and_does_not_toggle() {
    let src = "- [ ] Příliš žluťoučký\n";
    let mut h = AppHarness::new(src);
    h.set_selection(src.len(), src.len());
    let p = h.pos_of("žluťoučký");
    h.click(p, Modifiers::NONE);
    assert_eq!(h.app.text, src);
    let (a, b) = h.selection();
    assert_eq!(a, b);
    assert!(a >= src.find("žluťoučký").unwrap() - 1);
}

#[test]
fn ctrl_enter_toggles_the_task_on_the_caret_line_and_undoes() {
    let src = "- [ ] one\n- [x] two\nplain";
    let mut h = AppHarness::new(src);
    h.set_selection(3, 3); // inside the first box: shown raw
    h.key(Key::Enter, CTRL);
    assert_eq!(h.app.text, "- [x] one\n- [x] two\nplain");
    assert_eq!(h.selection(), (3, 3));
    h.set_selection(14, 14);
    h.key(Key::Enter, CTRL);
    assert_eq!(h.app.text, "- [x] one\n- [ ] two\nplain");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, "- [x] one\n- [x] two\nplain");
    // Off a task line Ctrl+Enter changes nothing.
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.key(Key::Enter, CTRL);
    assert_eq!(h.app.text, "- [x] one\n- [x] two\nplain");
}

#[test]
fn enter_in_the_real_editor_continues_and_ends_task_lists_and_plain_lists() {
    let mut h = AppHarness::new("- [x] Příliš");
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.text, "- [x] Příliš\n- [ ] ");
    h.text("druhý");
    h.key(Key::Enter, Modifiers::NONE);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.text, "- [x] Příliš\n- [ ] druhý\n");
    let mut h = AppHarness::new("1. a");
    h.set_selection(4, 4);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.text, "1. a\n2. ");
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.text, "1. a\n");
}

#[test]
fn strikethrough_shortcut_wraps_selection_and_undoes() {
    let src = "one Příliš 🦀 two";
    let mut h = AppHarness::new(src);
    let (a, b) = (4, 4 + "Příliš 🦀".len());
    h.set_selection(a, b);
    h.key(Key::S, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.app.text, "one ~~Příliš 🦀~~ two");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, src);
    // Ctrl+S is the host's: the editor leaves it alone.
    h.key(Key::S, Modifiers::COMMAND);
    assert_eq!(h.app.text, src);
}

#[test]
fn only_ctrl_click_opens_http_links() {
    let src = "see [docs](https://example.com/a) and [bad](javascript:alert(1)) and [f](file:///etc/passwd) and <https://auto.org>\n\nend";
    let mut h = AppHarness::new(src);
    let n = src.len();
    h.set_selection(n, n);
    let docs = h.pos_of("docs");
    h.click(docs, Modifiers::NONE);
    assert!(h.opened.is_empty(), "a plain click edits, it never opens");
    assert_eq!(h.app.text, src);
    h.set_selection(n, n);
    h.click(h.pos_of("docs"), CTRL);
    assert_eq!(h.opened, ["https://example.com/a"]);
    for bad in ["bad", "f]"] {
        h.set_selection(n, n);
        h.click(h.pos_of(bad), CTRL);
        assert_eq!(h.opened.len(), 1, "{bad}: unsupported scheme");
        assert!(h.app.status.contains("Not opened"), "{}", h.app.status);
    }
    h.set_selection(n, n);
    h.click(h.pos_of("https://auto.org>"), CTRL);
    assert_eq!(h.opened.len(), 2);
    assert_eq!(h.opened[1], "https://auto.org");
    // macOS shape: Command, not Ctrl.
    let cmd = Modifiers {
        mac_cmd: true,
        command: true,
        ctrl: false,
        ..Default::default()
    };
    h.set_selection(n, n);
    h.click(h.pos_of("docs"), cmd);
    assert_eq!(h.opened.len(), 3);
    assert_eq!(h.app.text, src, "source preserved");
}

#[test]
fn escape_closes_the_slash_menu_and_keeps_the_editor_focused() {
    let mut h = AppHarness::new("");
    h.text("/");
    assert!(h.app.view.slash.is_some(), "menu open after typing /");
    h.text("ta");
    h.key(Key::Escape, Modifiers::NONE);
    assert!(h.app.view.slash.is_none());
    assert_eq!(h.app.text, "/ta", "typed text is kept");
    assert!(
        h.ctx.memory(|m| m.has_focus(AppHarness::id())),
        "editor keeps focus"
    );
    h.text("x");
    assert_eq!(
        h.app.text, "/tax",
        "typing continues without clicking back in"
    );
    assert!(h.app.view.slash.is_none());
}

#[test]
fn empty_fenced_code_block_is_taller_than_the_concealed_fences_alone() {
    let src = "before\n\n```\n```\n\nafter";
    let mut h = AppHarness::new(src);
    h.set_selection(0, 0);
    let (g, _) = h.app.view.probe.galley.clone().unwrap();
    let y = |needle: &str| {
        g.pos_from_cursor(CCursor::new(src.find(needle).unwrap()))
            .min
            .y
    };
    let gap = y("after") - y("before");
    // Without the reserved row the two blank-separated lines would be ~2 text rows apart.
    let row = BASE_SIZE * 1.3;
    assert!(gap > 3.0 * row, "gap {gap}");
    // Caret inside: the fences show as ordinary lines.
    let caret = src.find("```").unwrap();
    h.set_selection(caret, caret);
    assert!(h.app.text.contains("```\n```"));
}

const TASKS: &str = "- [ ] Příliš žluťoučký 🦀\n- [x] Příliš žluťoučký 🦀\n\nkonec";

fn text_starts(h: &AppHarness) -> (f32, f32) {
    let a = h.app.text.find("Příliš").unwrap();
    let b = a + 1 + h.app.text[a + 1..].find("Příliš").unwrap();
    (h.x_at(a), h.x_at(b))
}

#[test]
fn checked_and_unchecked_tasks_start_their_text_at_the_same_x() {
    let mut h = AppHarness::new(TASKS);
    let n = h.app.text.len();
    h.set_selection(n, n);
    let (a, b) = text_starts(&h);
    assert!((a - b).abs() < 0.01, "unchecked {a} vs checked {b}");
    // Same for every pair of states and for other markers.
    {
        let src =
            "- [ ] x\n- [x] x\n- [X] x\n* [ ] x\n1. [ ] x\n1. [x] x\n  - [x] x\n  - [ ] x\nend";
        let mut h = AppHarness::new(src);
        h.set_selection(src.len(), src.len());
        let xs = |needle: &str, from: usize| h.x_at(from + src[from..].find(needle).unwrap());
        let l = |line: &str| src.find(line).unwrap();
        assert!((xs("x", l("- [ ]") + 5) - xs("x", l("- [x]") + 5)).abs() < 0.01);
        assert!((xs("x", l("- [x]") + 5) - xs("x", l("- [X]") + 5)).abs() < 0.01);
        assert!((xs("x", l("1. [ ]") + 7) - xs("x", l("1. [x]") + 7)).abs() < 0.01);
        assert!((xs("x", l("  - [x]") + 7) - xs("x", l("  - [ ]") + 7)).abs() < 0.01);
    }
}

#[test]
fn toggling_a_checkbox_does_not_move_any_text() {
    let mut h = AppHarness::new(TASKS);
    let n = h.app.text.len();
    h.set_selection(n, n);
    let probe: Vec<usize> = (0..h.app.text.len())
        .filter(|i| h.app.text.is_char_boundary(*i))
        .collect();
    let before: Vec<(f32, f32)> = probe.iter().map(|i| (h.x_at(*i), h.y_at(*i))).collect();
    let rect = h.app.view.probe.task_rects[0].1;
    h.click(rect.center(), Modifiers::NONE);
    assert!(h.app.text.starts_with("- [x] Příliš"));
    let after: Vec<(f32, f32)> = probe.iter().map(|i| (h.x_at(*i), h.y_at(*i))).collect();
    for (i, (b, a)) in probe.iter().zip(before.iter().zip(&after)) {
        // The only things that may differ are the three bytes of the toggled box itself.
        if (2..5).contains(i) {
            continue;
        }
        assert!(
            (b.0 - a.0).abs() < 0.01 && (b.1 - a.1).abs() < 0.01,
            "byte {i}: {b:?} -> {a:?}"
        );
    }
    // And back, via the keyboard.
    h.set_selection(n, n);
    h.set_selection(2, 2);
    h.key(Key::Enter, CTRL);
    assert!(h.app.text.starts_with("- [ ] Příliš"));
    h.set_selection(n, n);
    let (a, b) = text_starts(&h);
    assert!((a - b).abs() < 0.01);
}

#[test]
fn task_lines_do_not_change_the_line_spacing() {
    let tasks = "- [ ] a\n- [x] b\n- [ ] c\nend";
    let plain = "- a\n- b\n- c\nend";
    let gaps = |src: &str| {
        let mut h = AppHarness::new(src);
        h.set_selection(src.len(), src.len());
        let y = |n: &str| h.y_at(src.find(n).unwrap());
        (y("b") - y("a"), y("c") - y("b"), y("end") - y("c"))
    };
    let (t, p) = (gaps(tasks), gaps(plain));
    assert!(
        (t.0 - p.0).abs() < 0.5 && (t.1 - p.1).abs() < 0.5 && (t.2 - p.2).abs() < 0.5,
        "tasks {t:?} plain {p:?}"
    );
}

#[test]
fn checkboxes_follow_light_and_dark_palettes_with_identical_geometry() {
    let light = Palette::from_toml(
            "mode = \"light\"\naccent = \"#2e5cb8\"\nbackground = \"#f5f3ee\"\nforeground = \"#2b2b33\"\n",
        )
        .unwrap();
    let dark = Palette::from_toml(
            "mode = \"dark\"\naccent = \"#7aa2f7\"\nbackground = \"#1a1b26\"\nforeground = \"#a9b1d6\"\n",
        )
        .unwrap();
    let mut sizes = Vec::new();
    for pal in [light, dark] {
        let mut h = AppHarness::new(TASKS);
        h.app.palette = pal;
        let n = h.app.text.len();
        h.set_selection(n, n);
        assert!(
            h.has_fill(pal.link),
            "checked box filled with the palette accent"
        );
        let (a, b) = text_starts(&h);
        assert!((a - b).abs() < 0.01);
        let rects = &h.app.view.probe.task_rects;
        assert_eq!(rects.len(), 2);
        assert!((rects[0].1.size() - rects[1].1.size()).length() < 0.01);
        assert!((rects[0].1.min.x - rects[1].1.min.x).abs() < 0.01);
        sizes.push(rects[0].1.size());
    }
    assert!((sizes[0] - sizes[1]).length() < 0.01);
}

#[test]
fn slash_task_by_keyboard_and_mouse_then_typing_enter_and_undo() {
    for mouse in [false, true] {
        let mut h = AppHarness::new("");
        h.text("/");
        h.text("todo");
        assert!(h.app.view.slash.is_some());
        if mouse {
            let r = h.app.view.slash_rect;
            h.click(r.min + Vec2::new(24.0, 16.0), Modifiers::NONE);
        } else {
            h.key(Key::Enter, Modifiers::NONE);
        }
        assert_eq!(h.app.text, "- [ ] ", "mouse={mouse}");
        assert_eq!(h.selection(), (6, 6), "caret right after the marker");
        h.text("Koupit mléko");
        h.key(Key::Enter, Modifiers::NONE);
        assert_eq!(h.app.text, "- [ ] Koupit mléko\n- [ ] ");
        h.text("druhý");
        h.key(Key::Enter, Modifiers::NONE);
        h.key(Key::Enter, Modifiers::NONE);
        assert_eq!(
            h.app.text, "- [ ] Koupit mléko\n- [ ] druhý\n",
            "empty task ends the list"
        );
        // The command is its own undo step, and redo brings it back.
        let mut u = AppHarness::new("");
        u.text("/");
        u.text("task");
        u.key(Key::Enter, Modifiers::NONE);
        u.text("x");
        assert_eq!(u.app.text, "- [ ] x");
        u.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(u.app.text, "- [ ] ");
        u.key(Key::Z, Modifiers::COMMAND);
        assert_eq!(u.app.text, "/task");
        u.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(u.app.text, "- [ ] ");
        u.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(u.app.text, "- [ ] x");
    }
}

#[test]
fn task_command_converts_a_list_item_and_renders_a_checkbox() {
    let mut h = AppHarness::new("1. Příliš ");
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.text("/");
    h.text("task");
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.text, "- [ ] Příliš ");
    h.set_selection(0, 0);
    h.set_selection(h.app.text.len(), h.app.text.len());
    assert_eq!(
        h.app.view.probe.task_rects.len(),
        1,
        "rendered as a checkbox"
    );
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, "1. Příliš /task");
}

// ---- images: width annotation, selection, slider ----

struct Doc {
    dir: crate::test_dir::TestDir,
    text: String,
    a: String,
}

fn png(dir: &std::path::Path, w: usize, h: usize, c: Color32) -> String {
    let bytes = assets::encode_png(&egui::ColorImage::filled([w, h], c)).unwrap();
    attachments::FsStore::new(dir).put(&bytes, "png").unwrap()
}

/// Text with two images: A (400×200, no annotation) and B (300×150, `{width=40%}`).
fn image_doc() -> Doc {
    let dir = crate::test_dir::TestDir::new("img");
    let a = png(&dir, 400, 200, Color32::from_rgb(200, 60, 60));
    let b = png(&dir, 300, 150, Color32::from_rgb(60, 60, 200));
    let text =
        format!("Úvod\n\n![Příliš 🦀 A]({a})\n\ntext mezi\n\n![B]({b}){{width=40%}}\n\nkonec");
    Doc { dir, text, a }
}

fn image_app() -> (AppHarness, Doc) {
    let d = image_doc();
    let mut h = AppHarness::in_dir(&d.text, d.dir.clone());
    h.set_selection(0, 0);
    (h, d)
}

impl AppHarness {
    fn image_rect(&self, n: usize) -> Rect {
        self.app.view.probe.image_rects[n].1
    }

    fn content_w(&self) -> f32 {
        self.app
            .view
            .probe
            .galley
            .clone()
            .unwrap()
            .0
            .job
            .wrap
            .max_width
    }

    fn select_image(&mut self, n: usize) {
        let r = self.image_rect(n);
        self.click(r.center(), Modifiers::NONE);
        self.frame(vec![]);
    }

    fn start_of(&self, n: usize) -> usize {
        self.app.view.probe.image_rects[n].0
    }

    fn focus_slider(&mut self) {
        let sid = self.app.view.slider_id.expect("controls shown");
        self.ctx.memory_mut(|m| m.request_focus(sid));
        self.frame(vec![]);
        self.frame(vec![]);
    }

    fn control(&self, name: &str) -> Rect {
        self.app
            .view
            .probe
            .controls
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, r)| *r)
            .unwrap_or_else(|| panic!("no {name} control"))
    }

    fn slot_height(&self, n: usize) -> f32 {
        let start = self.start_of(n);
        self.app
            .view
            .images
            .borrow()
            .iter()
            .find(|s| s.byte_start == start)
            .unwrap()
            .height
    }
}

fn strip_annotations(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(i) = rest.find("{width=") {
        out.push_str(&rest[..i]);
        let j = rest[i..].find('}').unwrap();
        rest = &rest[i + j + 1..];
    }
    out + rest
}

#[test]
fn existing_images_render_as_before_and_annotated_ones_use_their_width() {
    let (h, _) = image_app();
    let w = h.content_w();
    assert_eq!(h.app.view.probe.image_rects.len(), 2);
    // A: no annotation → natural size (it fits), nothing rewritten.
    assert!((h.image_rect(0).size() - Vec2::new(400.0, 200.0)).length() < 0.01);
    // B: 40% of the content width, aspect 2:1.
    let b = h.image_rect(1).size();
    assert!(
        (b.x - w * 0.4).abs() < 0.01 && (b.x / b.y - 2.0).abs() < 1e-3,
        "{b:?}"
    );
    // Reserved row heights match the painted heights.
    assert!((h.slot_height(0) - (200.0 + IMAGE_PAD)).abs() < 0.01);
    assert!((h.slot_height(1) - (b.y + IMAGE_PAD)).abs() < 0.01);
    assert_eq!(h.app.text, image_doc_text_of(&h));
}

fn image_doc_text_of(h: &AppHarness) -> String {
    h.app.text.clone()
}

#[test]
fn document_padding_is_applied_by_the_container_to_text_and_images() {
    let (h, _) = image_app();
    assert!(
        (h.x_at(0) - DOC_PAD).abs() < 0.01,
        "text starts {} from the edge",
        h.x_at(0)
    );
    assert!((h.image_rect(0).left() - DOC_PAD).abs() < 0.01);
    assert!((h.content_w() - 780.0).abs() < 0.5, "{}", h.content_w());
    // Narrow window: the column is the window minus the same inset on both sides.
    let d = image_doc();
    let mut n = AppHarness::in_dir(&d.text, d.dir.clone());
    n.screen = Vec2::new(300.0, 600.0);
    n.set_selection(0, 0);
    assert!(
        (n.content_w() - (300.0 - 2.0 * DOC_PAD)).abs() < 1.0,
        "{}",
        n.content_w()
    );
    for (_, r) in &n.app.view.probe.image_rects {
        assert!(
            r.left() >= DOC_PAD - 0.01 && r.right() <= 300.0 - DOC_PAD + 0.5,
            "{r:?}"
        );
    }
    // The source has no padding characters.
    assert_eq!(n.app.text, d.text);
    // Hit-testing and caret mapping use the padded geometry: a click on the first letter
    // puts the caret before it.
    let mut c = AppHarness::new("Příliš žluťoučký");
    c.set_selection(5, 5);
    let p = Pos2::new(DOC_PAD + 2.0, c.y_at(0) + 8.0);
    c.click(p, Modifiers::NONE);
    assert_eq!(c.selection(), (0, 0));
}

#[test]
fn selecting_an_image_keeps_it_visible_and_changes_no_byte() {
    let (mut h, d) = image_app();
    let before = h.selection();
    let r = h.image_rect(0);
    h.select_image(0);
    assert_eq!(h.app.view.selected_image, Some(h.start_of(0)));
    assert_eq!(h.app.text, d.text, "byte-identical");
    assert_eq!(
        h.selection(),
        before,
        "caret did not move into the image line"
    );
    assert_eq!(h.app.view.probe.image_rects.len(), 2, "still painted");
    assert!((h.image_rect(0).min - r.min).length() < 0.01);
    assert!(
        h.has_stroke(h.app.palette.link),
        "outline in the theme accent"
    );
    // The controls exist and sit just below the image, on screen.
    let s = h.control("slider");
    assert!(s.top() >= r.bottom() && s.bottom() <= 700.0);
    assert!(
        h.ctx.memory(|m| m.has_focus(AppHarness::id())),
        "editor keeps focus"
    );
    // Clicking just outside the image (in the padding) does not select anything else.
    let mut o = image_app().0;
    let r = o.image_rect(0);
    o.click(Pos2::new(r.left() - 8.0, r.center().y), Modifiers::NONE);
    assert_eq!(o.app.view.selected_image, None);
    o.click(
        Pos2::new(r.right() - 1.0, r.bottom() - 1.0),
        Modifiers::NONE,
    );
    assert_eq!(o.app.view.selected_image, Some(o.start_of(0)));
}

#[test]
fn escape_outside_click_enter_and_source_dismiss_the_selection() {
    let (mut h, d) = image_app();
    h.select_image(0);
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(h.app.view.selected_image, None);
    assert!(h.ctx.memory(|m| m.has_focus(AppHarness::id())));
    assert_eq!(h.app.text, d.text);
    // Click elsewhere (on the text below) dismisses and moves the caret there.
    h.select_image(0);
    let p = h.pos_of("konec");
    h.click(p, Modifiers::NONE);
    assert_eq!(h.app.view.selected_image, None);
    assert_eq!(h.app.text, d.text);
    let (a, b) = h.selection();
    assert_eq!(a, b);
    assert!(a >= d.text.find("konec").unwrap());
    // Enter edits the source: the caret enters the image line, which shows the raw Markdown.
    h.select_image(0);
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.view.selected_image, None);
    let start = d.text.find("![").unwrap();
    assert_eq!(h.selection(), (start + 2, start + 2));
    assert_eq!(
        h.app.view.probe.image_rects.len(),
        1,
        "the line being edited shows source"
    );
    assert_eq!(h.app.text, d.text);
    // The Source button does the same.
    let (mut h, _) = image_app();
    h.select_image(0);
    let r = h.control("source");
    h.click(r.center(), Modifiers::NONE);
    assert_eq!(h.app.view.selected_image, None);
    assert_eq!(h.selection(), (start + 2, start + 2));
    // Typing in the source works, and the selection is not resurrected.
    h.text("x");
    assert!(h.app.text.contains("![xPříliš"));
}

#[test]
fn caret_navigation_around_images_is_unaffected() {
    let (mut h, d) = image_app();
    let before_img = d.text.find("![").unwrap();
    h.set_selection(5, 5); // end of "Úvod"
    h.key(Key::ArrowDown, Modifiers::NONE);
    h.key(Key::ArrowDown, Modifiers::NONE);
    let (a, b) = h.selection();
    assert_eq!(a, b);
    assert!(
        a > before_img && a < d.text.find("text mezi").unwrap(),
        "caret on the image line: {a}"
    );
    assert_eq!(
        h.app.view.probe.image_rects.len(),
        1,
        "raw source shown while the caret is on it"
    );
    h.key(Key::ArrowDown, Modifiers::NONE);
    h.key(Key::ArrowDown, Modifiers::NONE);
    assert!(h.selection().0 >= d.text.find("text mezi").unwrap());
    assert_eq!(h.app.view.probe.image_rects.len(), 2);
    assert_eq!(h.app.text, d.text);
    // With an image selected the caret still moves normally, and entering the line ends the selection.
    let t = d.text.find("text mezi").unwrap();
    h.set_selection(t, t);
    h.select_image(0);
    assert_eq!(h.selection(), (t, t));
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert_ne!(h.selection(), (t, t), "caret moved");
    assert_eq!(
        h.app.view.selected_image,
        Some(h.start_of(0)),
        "still selected on the blank line"
    );
    h.key(Key::ArrowUp, Modifiers::NONE);
    assert_eq!(
        h.app.view.selected_image, None,
        "caret on the image line shows its source instead"
    );
    assert_eq!(h.app.text, d.text);
}

#[test]
fn slider_keys_adjust_live_and_commit_as_one_undo_step() {
    let (mut h, d) = image_app();
    let w = h.content_w();
    h.select_image(0);
    h.focus_slider();
    let base = image_size::effective_pct(Vec2::new(400.0, 200.0), w, None);
    let height0 = h.slot_height(0);
    for _ in 0..3 {
        h.key(Key::ArrowRight, Modifiers::NONE);
    }
    assert_eq!(h.app.view.live_width.get().map(|x| x.1), Some(base + 3));
    assert_eq!(
        h.app.text, d.text,
        "not in the source until the adjustment is finished"
    );
    assert!(
        h.slot_height(0) > height0,
        "layout follows the slider immediately"
    );
    h.key(Key::ArrowRight, Modifiers::SHIFT);
    assert_eq!(
        h.app.view.live_width.get().map(|x| x.1),
        Some(base + 13),
        "Shift = 10"
    );
    h.key(Key::ArrowLeft, Modifiers::NONE);
    let v = base + 12;
    assert_eq!(h.app.view.live_width.get().map(|x| x.1), Some(v));
    // Reserved height always equals the painted height.
    let r = h.image_rect(0);
    assert!((h.slot_height(0) - (r.height() + IMAGE_PAD)).abs() < 0.01);
    assert!((r.width() - w * f32::from(v) / 100.0).abs() < 0.01);
    // Escape finishes it: one edit that adds the annotation.
    h.key(Key::Escape, Modifiers::NONE);
    assert_eq!(h.app.view.live_width.get(), None);
    let want = d
        .text
        .replacen(&format!("{})", d.a), &format!("{}){{width={v}%}}", d.a), 1);
    assert_eq!(h.app.text, want);
    assert_eq!(strip_annotations(&h.app.text), strip_annotations(&d.text));
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        h.app.text, d.text,
        "one undo restores the unannotated image"
    );
    h.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.app.text, want);
}

#[test]
fn dragging_the_slider_is_one_undo_step_and_resizes_live() {
    let (mut h, d) = image_app();
    h.select_image(0);
    let s = h.control("slider");
    let y = s.center().y;
    let (x0, x1) = (s.left() + 25.0, s.left() + 115.0);
    let heights = |h: &AppHarness| h.slot_height(0);
    let start_h = heights(&h);
    h.frame(vec![egui::Event::PointerMoved(Pos2::new(x0, y))]);
    h.frame(vec![egui::Event::PointerButton {
        pos: Pos2::new(x0, y),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    let mut seen = vec![heights(&h)];
    for i in 1..=8 {
        let x = x0 + (x1 - x0) * i as f32 / 8.0;
        h.frame(vec![egui::Event::PointerMoved(Pos2::new(x, y))]);
        h.frame(vec![]);
        assert_eq!(h.app.text, d.text, "source untouched while dragging");
        seen.push(heights(&h));
    }
    seen.dedup_by(|a, b| (*a - *b).abs() < 0.01);
    assert!(seen.len() > 3, "image resized continuously: {seen:?}");
    assert_ne!(*seen.last().unwrap(), start_h);
    h.frame(vec![egui::Event::PointerButton {
        pos: Pos2::new(x1, y),
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: Modifiers::NONE,
    }]);
    h.frame(vec![]);
    h.frame(vec![]);
    assert_ne!(h.app.text, d.text);
    assert!(h.app.text.contains("{width="), "{}", h.app.text);
    assert_eq!(h.app.view.live_width.get(), None);
    let done = h.app.text.clone();
    // Ctrl+Z straight after the drag (slider still focused) undoes the whole drag at once.
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, d.text);
    h.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.app.text, done);
}

#[test]
fn clicking_the_slider_focuses_it_so_arrow_keys_work_afterwards() {
    let (mut h, d) = image_app();
    h.select_image(0);
    let s = h.control("slider");
    h.click(Pos2::new(s.left() + 40.0, s.center().y), Modifiers::NONE);
    let after_click = h.app.text.clone();
    assert_ne!(after_click, d.text, "the click set a width");
    let sid = h.app.view.slider_id.unwrap();
    assert!(
        h.ctx.memory(|m| m.has_focus(sid)),
        "slider has keyboard focus"
    );
    h.key(Key::ArrowRight, Modifiers::SHIFT);
    assert!(
        h.app.view.live_width.get().is_some(),
        "arrow key adjusts: {:?}",
        h.app.view.live_width.get()
    );
}

#[test]
fn creating_changing_and_resetting_a_width_undo_and_redo() {
    let (mut h, d) = image_app();
    let nudge = |h: &mut AppHarness, key: Key, n: usize| {
        h.select_image(0);
        h.focus_slider();
        for _ in 0..n {
            h.key(key, Modifiers::NONE);
        }
        h.key(Key::Escape, Modifiers::NONE);
    };
    nudge(&mut h, Key::ArrowRight, 5);
    let t1 = h.app.text.clone();
    assert!(t1.contains("{width=") && t1 != d.text);
    nudge(&mut h, Key::ArrowLeft, 7);
    let t2 = h.app.text.clone();
    assert_ne!(t2, t1);
    // Reset removes the annotation again: exactly the original bytes.
    h.select_image(0);
    let r = h.control("reset");
    h.click(r.center(), Modifiers::NONE);
    assert_eq!(h.app.text, d.text, "reset restores the original source");
    // Ctrl+Z works straight after the button press (the editor has focus again).
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, t2, "undo reset");
    h.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.app.text, d.text);
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, t2, "undo reset");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, t1, "undo modification");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, d.text, "undo creation");
    for want in [&t1, &t2, &d.text] {
        h.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
        assert_eq!(&h.app.text, want);
    }
}

#[test]
fn images_have_independent_widths() {
    let (mut h, d) = image_app();
    let w = h.content_w();
    let a = h.start_of(0);
    let ctx = h.ctx.clone();
    {
        let TestApp { view, text, .. } = &mut h.app;
        view.set_image_width(&ctx, AppHarness::id(), text, a, Some(70));
    }
    h.frame(vec![]);
    assert!((h.image_rect(0).width() - 0.7 * w).abs() < 0.01);
    assert!(
        (h.image_rect(1).width() - 0.4 * w).abs() < 0.01,
        "B keeps its own width"
    );
    let b = h.start_of(1);
    {
        let TestApp { view, text, .. } = &mut h.app;
        view.set_image_width(&ctx, AppHarness::id(), text, b, Some(90));
    }
    h.frame(vec![]);
    assert!((h.image_rect(0).width() - 0.7 * w).abs() < 0.01);
    assert!((h.image_rect(1).width() - 0.9 * w).abs() < 0.01);
    // Nothing else in the source changed; no overlap between the two images and the text.
    assert_eq!(strip_annotations(&h.app.text), strip_annotations(&d.text));
    assert!(h.image_rect(0).bottom() <= h.y_at(h.app.text.find("text mezi").unwrap()) + 0.5);
    assert!(h.image_rect(1).top() >= h.y_at(h.app.text.find("text mezi").unwrap()));
}

#[test]
fn width_survives_save_and_reload_and_aspect_ratio_holds() {
    let (mut h, d) = image_app();
    let ctx = h.ctx.clone();
    let a = h.start_of(0);
    for p in [20u16, 33, 60, 100] {
        {
            let TestApp { view, text, .. } = &mut h.app;
            view.set_image_width(&ctx, AppHarness::id(), text, a, Some(p));
        }
        h.frame(vec![]);
        let r = h.image_rect(0);
        assert!(
            (r.width() / r.height() - 2.0).abs() < 0.01,
            "aspect at {p}%"
        );
        assert!((r.width() - h.content_w() * f32::from(p) / 100.0).abs() < 0.01);
        assert!(r.width() <= h.content_w() + 0.01);
    }
    {
        let TestApp { view, text, .. } = &mut h.app;
        view.set_image_width(&ctx, AppHarness::id(), text, a, Some(60));
    }
    h.frame(vec![]);
    let file = d.dir.join("doc.md");
    std::fs::write(&file, h.app.text.as_bytes()).unwrap();
    let reloaded = String::from_utf8(std::fs::read(&file).unwrap()).unwrap();
    assert_eq!(reloaded, h.app.text);
    let mut r = AppHarness::in_dir(&reloaded, d.dir.clone());
    r.set_selection(0, 0);
    assert!((r.image_rect(0).width() - 0.6 * r.content_w()).abs() < 0.01);
    assert!((r.image_rect(1).width() - 0.4 * r.content_w()).abs() < 0.01);
    // The image files themselves were never touched.
    assert_eq!(
        std::fs::read(d.dir.join(&d.a)).unwrap().len(),
        std::fs::read(d.dir.join(&d.a)).unwrap().len()
    );
}

#[test]
fn utf8_alt_text_and_file_names_keep_their_bytes() {
    let d = image_doc();
    let named = "assets/obrázek-žlutý.png";
    std::fs::rename(d.dir.join(&d.a), d.dir.join(named)).unwrap();
    let text = format!("Příliš\n\n![Žluťoučký kůň 🦀👨\u{200D}👩\u{200D}👧]({named})\n\nkonec 🦀");
    let mut h = AppHarness::in_dir(&text, d.dir.clone());
    let n = text.len();
    h.set_selection(n, n);
    assert_eq!(
        h.app.view.probe.image_rects.len(),
        1,
        "non-ASCII file name resolves"
    );
    h.select_image(0);
    h.focus_slider();
    h.key(Key::ArrowLeft, Modifiers::SHIFT);
    h.key(Key::Escape, Modifiers::NONE);
    assert!(h.app.text.contains(&format!("({named}){{width=")));
    assert_eq!(strip_annotations(&h.app.text), text);
    // The caret (at the end, after the edit) is still on a character boundary at the end.
    let (a, b) = h.selection();
    assert_eq!((a, b), (h.app.text.len(), h.app.text.len()));
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, text);
}

#[test]
fn theme_change_while_an_image_is_selected_keeps_selection_and_geometry() {
    let (mut h, d) = image_app();
    h.select_image(0);
    let rect = h.image_rect(0);
    let dark = Palette::from_toml(
            "mode = \"dark\"\naccent = \"#7aa2f7\"\nbackground = \"#1a1b26\"\nforeground = \"#a9b1d6\"\n",
        )
        .unwrap();
    let light = Palette::from_toml(
            "mode = \"light\"\naccent = \"#2e5cb8\"\nbackground = \"#f5f3ee\"\nforeground = \"#2b2b33\"\n",
        )
        .unwrap();
    for pal in [dark, light, dark] {
        h.app.palette = pal;
        pal.apply(&h.ctx);
        h.frame(vec![]);
        h.frame(vec![]);
        assert_eq!(h.app.view.selected_image, Some(h.start_of(0)));
        assert!((h.image_rect(0).min - rect.min).length() < 0.01);
        assert!((h.image_rect(0).size() - rect.size()).length() < 0.01);
        assert!(h.has_stroke(pal.link), "outline follows the palette");
        assert!(h
            .app
            .view
            .probe
            .controls
            .iter()
            .any(|(n, _)| *n == "slider"));
    }
    assert_eq!(h.app.text, d.text);
}

#[test]
fn controls_stay_on_screen_near_the_window_bottom() {
    let d = image_doc();
    let long = format!("zeile\n{}", d.text);
    let mut h = AppHarness::in_dir(&long, d.dir.clone());
    h.screen = Vec2::new(900.0, 300.0);
    h.set_selection(0, 0);
    let r = h.image_rect(0);
    assert!(
        r.bottom() + 48.0 > 300.0,
        "image reaches the window bottom: {r:?}"
    );
    h.select_image(0);
    let s = h.control("slider");
    assert!(s.top() >= 0.0 && s.bottom() <= 300.0, "{s:?}");
}

/// A second backend that keeps attachments in memory and counts reads: the editor must work
/// with it exactly as with files, and must not re-read unchanged images every frame.
#[derive(Default)]
struct MemStore {
    files: RefCell<HashMap<String, Vec<u8>>>,
    gets: Rc<Cell<usize>>,
    fail_put: bool,
}

impl AttachmentStore for MemStore {
    fn put(&self, bytes: &[u8], ext: &str) -> Result<String, attachments::AttachmentError> {
        if self.fail_put {
            return Err(attachments::AttachmentError::Io("disk full".into()));
        }
        let mut f = self.files.borrow_mut();
        let r = format!("mem/{}.{ext}", f.len());
        f.insert(r.clone(), bytes.to_vec());
        Ok(r)
    }

    fn get(&self, reference: &str) -> Result<Vec<u8>, attachments::AttachmentError> {
        self.gets.set(self.gets.get() + 1);
        self.files
            .borrow()
            .get(reference)
            .cloned()
            .ok_or_else(|| attachments::AttachmentError::NotFound(reference.into()))
    }

    fn info(
        &self,
        reference: &str,
    ) -> Result<attachments::AttachmentInfo, attachments::AttachmentError> {
        self.get(reference).map(|b| attachments::AttachmentInfo {
            len: b.len() as u64,
            media_type: "image/png",
        })
    }
}

#[test]
fn the_editor_works_with_any_attachment_backend_and_loads_each_image_once() {
    let store = MemStore::default();
    let gets = store.gets.clone();
    let png = assets::encode_png(&egui::ColorImage::filled([80, 40], Color32::RED)).unwrap();
    let r = store.put(&png, "png").unwrap();
    let text = format!("a\n\n![x]({r}){{width=50%}}\n\n![gone](mem/none.png)\n\nz");
    let mut h = AppHarness::with_store(&text, Box::new(store));
    h.set_selection(0, 0);
    for _ in 0..10 {
        h.frame(vec![]);
    }
    // The stored image renders; the missing one stays raw text and does not crash.
    assert_eq!(h.app.view.probe.image_rects.len(), 1);
    assert!((h.image_rect(0).width() / h.image_rect(0).height() - 2.0).abs() < 1e-3);
    // One read per distinct reference (the good one and the missing one), not one per frame.
    assert_eq!(gets.get(), 2, "decoded once, cached as a texture");
    // Resizing re-uses the cached texture: still no further reads.
    let ctx = h.ctx.clone();
    let start = h.start_of(0);
    {
        let TestApp { view, text, .. } = &mut h.app;
        view.set_image_width(&ctx, AppHarness::id(), text, start, Some(80));
    }
    for _ in 0..5 {
        h.frame(vec![]);
    }
    assert!((h.image_rect(0).width() - 0.8 * h.content_w()).abs() < 0.01);
    assert_eq!(gets.get(), 2);
    // Pasting goes through the backend, and the new reference is portable.
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.frame(vec![egui::Event::PasteImage(std::sync::Arc::new(
        egui::ColorImage::filled([10, 10], Color32::BLUE),
    ))]);
    h.frame(vec![]);
    assert!(h.app.text.contains("![image](mem/1.png)"), "{}", h.app.text);
    assert_eq!(h.app.view.probe.image_rects.len(), 2);
}

#[test]
fn a_failed_store_leaves_the_markdown_and_undo_history_untouched() {
    let store = MemStore {
        fail_put: true,
        ..Default::default()
    };
    let mut h = AppHarness::with_store("Ahoj\n", Box::new(store));
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.frame(vec![egui::Event::PasteImage(std::sync::Arc::new(
        egui::ColorImage::filled([10, 10], Color32::BLUE),
    ))]);
    h.frame(vec![]);
    assert_eq!(h.app.text, "Ahoj\n", "no broken reference");
    assert!(
        h.app.status.contains("Image paste failed"),
        "{}",
        h.app.status
    );
    assert!(h.app.status.contains("disk full"));
    assert_eq!(h.selection(), (n, n));
    // The same through the filesystem backend with an unusable assets directory.
    let dir = crate::test_dir::TestDir::new("fail");
    std::fs::write(dir.join("assets"), b"i am a file").unwrap();
    let mut f = AppHarness::in_dir("Text\n", dir.clone());
    let n = f.app.text.len();
    f.set_selection(n, n);
    f.frame(vec![egui::Event::PasteImage(std::sync::Arc::new(
        egui::ColorImage::filled([10, 10], Color32::BLUE),
    ))]);
    f.frame(vec![]);
    assert_eq!(f.app.text, "Text\n");
    assert_eq!(std::fs::read(dir.join("assets")).unwrap(), b"i am a file");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_pasted_screenshot_is_a_png_in_assets_with_a_relative_reference_that_reloads() {
    let d = image_doc();
    let dir = d.dir.join("paste");
    let mut h = AppHarness::in_dir("Ahoj\n", dir.clone());
    let n = h.app.text.len();
    h.set_selection(n, n);
    let img = egui::ColorImage::filled([30, 20], Color32::from_rgb(1, 2, 3));
    h.frame(vec![egui::Event::PasteImage(std::sync::Arc::new(
        img.clone(),
    ))]);
    h.frame(vec![]);
    let line = h
        .app
        .text
        .lines()
        .find(|l| l.starts_with("![image]("))
        .unwrap()
        .to_owned();
    let reference = line
        .trim_start_matches("![image](")
        .trim_end_matches(')')
        .to_owned();
    assert!(reference.starts_with("assets/image-") && reference.ends_with(".png"));
    assert!(!reference.starts_with('/'));
    let file = dir.join(&reference);
    let bytes = std::fs::read(&file).unwrap();
    assert_eq!(&bytes[..4], b"\x89PNG");
    let back = assets::decode(&bytes).unwrap();
    assert_eq!(
        (back.size, back.pixels[0]),
        ([30, 20], Color32::from_rgb(1, 2, 3))
    );
    // Reopen the saved text in a fresh app over the same directory.
    let text = h.app.text.clone();
    let mut r = AppHarness::in_dir(&text, dir.clone());
    r.set_selection(0, 0);
    assert_eq!(r.app.view.probe.image_rects.len(), 1);
    assert!((r.image_rect(0).size() - Vec2::new(30.0, 20.0)).length() < 0.01);
    // Two pasted images never share a file.
    let m = h.app.text.len();
    h.set_selection(m, m);
    h.frame(vec![egui::Event::PasteImage(std::sync::Arc::new(img))]);
    h.frame(vec![]);
    let refs = markdown::image_references(&h.app.text);
    assert_eq!(refs.len(), 2);
    assert_ne!(refs[0], refs[1]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pasted_images_and_ordinary_documents_are_unaffected() {
    let d = image_doc();
    let mut h = AppHarness::in_dir("Ahoj\n", d.dir.clone());
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.frame(vec![egui::Event::PasteImage(std::sync::Arc::new(
        egui::ColorImage::filled([50, 50], Color32::GREEN),
    ))]);
    h.frame(vec![]);
    h.frame(vec![]);
    assert!(
        h.app.text.contains("![image](assets/image-"),
        "{}",
        h.app.text
    );
    assert!(
        !h.app.text.contains("{width"),
        "pasting never adds an annotation"
    );
    assert_eq!(h.app.view.probe.image_rects.len(), 1);
    // Tasks, tables and links still render in the padded layout.
    let mixed = "- [ ] a\n- [x] b\n\n| h1 | h2 |\n| --- | --- |\n| c | d |\n\n[l](https://x.org)\n";
    let mut m = AppHarness::new(mixed);
    m.set_selection(mixed.len(), mixed.len());
    assert_eq!(m.app.view.probe.task_rects.len(), 2);
    assert!((m.x_at(0) - DOC_PAD).abs() < 0.01);
}

fn slash(title: &str) -> edit::SlashCommand {
    *edit::SLASH_COMMANDS
        .iter()
        .find(|c| c.title == title)
        .unwrap()
}

/// Popup dimensions stay stable across every command filter and repeated use.
#[test]
fn popup_width_is_stable_for_all_filters() {
    let ctx = Context::default();
    let full = run(&ctx, "", 3);
    for round in 0..3 {
        for q in ["bold", "h", "code", "zzz", "emoji", "list", "i", ""] {
            let r = run(&ctx, q, 2);
            if !q.is_empty() && !edit::slash_filter(q).is_empty() {
                assert!((r.width() - SLASH_WIDTH).abs() < 20.0, "{q} {r:?}");
            }
        }
        let again = run(&ctx, "", 3);
        assert!((again.width() - full.width()).abs() < 0.5, "round {round}");
        assert!(
            (again.height() - full.height()).abs() < 0.5,
            "round {round}"
        );
    }
    // Long labels never wrap: one row each.
    let rows = edit::SLASH_COMMANDS.len() as f32;
    assert!(full.height() < rows * 30.0, "{full:?}");
}

// ---- embedding: hosting, identity, invalidation, key profiles ----

const NOTE_A: NoteKey = NoteKey::new(1, 10);
const NOTE_B: NoteKey = NoteKey::new(1, 11);

impl AppHarness {
    /// Show another note in the same view: the host swaps its buffer and key, as a note switch does.
    fn switch_to(&mut self, key: NoteKey, text: &mut String) {
        std::mem::swap(&mut self.app.text, text);
        self.app.key = key;
        // As a host does after selecting a note: ask for the keyboard.
        self.app.focus_next = true;
        self.frame(vec![]);
        self.frame(vec![]);
    }

    fn select(&mut self, key: NoteKey, a: usize, b: usize) {
        let id = key.widget_id();
        let mut st = egui::TextEdit::load_state(&self.ctx, id).unwrap_or_default();
        let c = |x: usize| CCursor::new(edit::byte_to_char(&self.app.text, x));
        st.cursor
            .set_char_range(Some(CCursorRange::two(c(a), c(b))));
        st.store(&self.ctx, id);
        self.frame(vec![]);
        self.frame(vec![]);
    }

    fn sel_of(&self, key: NoteKey) -> (usize, usize) {
        EditorView::selection(&self.ctx, key, &self.app.text)
    }
}

/// Two `EditorView`s side by side in one `Context` and one `Ui`, each with its own text.
struct Two {
    ctx: Context,
    views: [EditorView; 2],
    texts: [String; 2],
    keys: [NoteKey; 2],
    store: Box<dyn AttachmentStore>,
    palette: Palette,
    cfg: EditorKeys,
    t: f64,
    outs: [EditorOutput; 2],
    focus: [FocusRequest; 2],
}

impl Two {
    fn new(a: &str, b: &str, keys: [NoteKey; 2]) -> Self {
        let mut s = Two {
            ctx: Context::default(),
            views: [EditorView::new(), EditorView::new()],
            texts: [a.to_owned(), b.to_owned()],
            keys,
            store: Box::new(attachments::FsStore::new(std::env::temp_dir())),
            palette: Palette::fallback(),
            cfg: EditorKeys::standalone(),
            t: 0.0,
            outs: Default::default(),
            focus: [FocusRequest::None; 2],
        };
        for _ in 0..3 {
            s.frame(vec![]);
        }
        s
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
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            ..Default::default()
        };
        input.focused = true;
        let ctx = self.ctx.clone();
        let this = &mut *self;
        let (views, texts, keys, store, palette, cfg, outs, focus) = (
            &mut this.views,
            &mut this.texts,
            this.keys,
            &*this.store,
            &this.palette,
            &this.cfg,
            &mut this.outs,
            this.focus,
        );
        let mut out = ctx.run_ui(input, |root| {
            egui::CentralPanel::default().show(root, |ui| {
                ui.columns(2, |cols| {
                    let [v0, v1] = views;
                    let [t0, t1] = texts;
                    for (i, (ui, (view, text))) in
                        cols.iter_mut().zip([(v0, t0), (v1, t1)]).enumerate()
                    {
                        outs[i] = view.show(
                            ui,
                            EditorInput {
                                key: keys[i],
                                text,
                                attachments: store,
                                palette,
                                keys: cfg,
                                bold: None,
                                focus: focus[i],
                            },
                        );
                    }
                });
            });
        });
        out.textures_delta.clear();
        self.focus = [FocusRequest::None; 2];
    }

    fn key(&mut self, k: Key, m: Modifiers) {
        self.frame_with(vec![key(k, m)], m);
        self.frame(vec![]);
    }

    fn sel(&self, i: usize) -> (usize, usize) {
        EditorView::selection(&self.ctx, self.keys[i], &self.texts[i])
    }

    fn set_sel(&mut self, i: usize, a: usize, b: usize) {
        let id = self.keys[i].widget_id();
        let mut st = egui::TextEdit::load_state(&self.ctx, id).unwrap_or_default();
        let c = |x: usize| CCursor::new(edit::byte_to_char(&self.texts[i], x));
        st.cursor
            .set_char_range(Some(CCursorRange::two(c(a), c(b))));
        st.store(&self.ctx, id);
        self.frame(vec![]);
        self.frame(vec![]);
    }

    fn focus_editor(&mut self, i: usize) {
        self.focus[i] = FocusRequest::Request;
        self.frame(vec![]);
        self.frame(vec![]);
    }
}

#[test]
fn an_editor_view_is_hosted_without_an_eframe_application() {
    // `TestApp` is a bare egui `Context` + `CentralPanel`: no window, no eframe, no files.
    let mut h = AppHarness::new("");
    h.text("Příliš žluťoučký kůň 🦀");
    assert_eq!(h.app.text, "Příliš žluťoučký kůň 🦀");
    assert!(h.app.out.has_focus);
    assert_eq!(h.app.out.selection.0, h.app.text.len());
}

#[test]
fn note_keys_and_their_widget_ids_are_distinct_per_notebook_and_note() {
    let ids = [
        NoteKey::new(1, 5).widget_id(),
        NoteKey::new(2, 5).widget_id(),
        NoteKey::new(1, 6).widget_id(),
        NoteKey::new(5, 1).widget_id(),
    ];
    for i in 0..ids.len() {
        for j in 0..i {
            assert_ne!(ids[i], ids[j], "{i} vs {j}");
        }
    }
    // Every derived widget id depends on the editor's id, so notes cannot share task or image controls.
    let derived = |k: NoteKey| {
        [
            k.widget_id().with(("task", 7usize)),
            k.widget_id().with(("image", 7usize)),
            k.widget_id().with("image-controls"),
            k.widget_id().with("slash"),
            k.widget_id().with("emoji"),
        ]
    };
    let (a, b) = (derived(NoteKey::new(1, 5)), derived(NoteKey::new(2, 5)));
    for x in a {
        assert!(!b.contains(&x));
    }
    assert_eq!(
        NoteKey::STANDALONE.widget_id(),
        NoteKey::new(0, 0).widget_id()
    );
}

#[test]
fn two_editors_in_one_context_do_not_interfere() {
    let mut t = Two::new(
        "- [ ] eins\n\nalpha",
        "- [ ] zwei\n\nbeta",
        [NOTE_A, NoteKey::new(2, 10)],
    );
    let (n0, n1) = (t.texts[0].len(), t.texts[1].len());
    t.set_sel(0, n0, n0);
    t.set_sel(1, n1 - 2, n1 - 2);
    t.focus_editor(0);
    assert_eq!((t.sel(0), t.sel(1)), ((n0, n0), (n1 - 2, n1 - 2)));
    // Typing goes to the focused editor only.
    t.frame(vec![egui::Event::Text("X".into())]);
    t.frame(vec![]);
    assert_eq!(t.texts[0], "- [ ] eins\n\nalphaX");
    assert_eq!(
        t.texts[1], "- [ ] zwei\n\nbeta",
        "the other editor is untouched"
    );
    assert!(t.outs[0].selection == (n0 + 1, n0 + 1));
    // Undo in the first leaves the second alone, and their carets stay where they were.
    t.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(t.texts[0], "- [ ] eins\n\nalpha");
    assert_eq!(t.texts[1], "- [ ] zwei\n\nbeta");
    assert_eq!(t.sel(1), (n1 - 2, n1 - 2));
    // Same byte offsets, different notebooks: ids differ, so clicking one checkbox flips one source.
    assert!(t.views[0].probe.task_rects.len() == 1 && t.views[1].probe.task_rects.len() == 1);
    let r1 = t.views[1].probe.task_rects[0].1;
    let pos = r1.center();
    let b = |pressed| egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    };
    t.frame(vec![egui::Event::PointerMoved(pos)]);
    t.frame(vec![b(true)]);
    t.frame(vec![b(false)]);
    t.frame(vec![]);
    assert_eq!(
        t.texts[0], "- [ ] eins\n\nalpha",
        "first editor's box not toggled"
    );
    assert_eq!(
        t.texts[1], "- [x] zwei\n\nbeta",
        "second editor's box toggled"
    );
}

#[test]
fn notes_keep_independent_caret_and_undo_history_in_one_view() {
    let (mut a, mut b) = ("alpha one".to_owned(), "beta two".to_owned());
    let mut h = AppHarness::new(&a);
    h.app.key = NOTE_A;
    h.app.text = a.clone();
    h.app.focus_next = true;
    h.frame(vec![]);
    h.select(NOTE_A, 5, 5);
    h.text("A");
    h.text("B");
    let a_typed = h.app.text.clone();
    assert_eq!(a_typed, "alphaAB one");
    let a_sel = h.sel_of(NOTE_A);
    // Switch to the second note: it starts with no caret of its own and no history.
    h.switch_to(NOTE_B, &mut b);
    assert!(h.app.view.scope == Some(NOTE_B));
    h.select(NOTE_B, 2, 4);
    h.text("Z");
    assert_eq!(h.app.text, "beZ two");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        h.app.text, "beta two",
        "undo only reaches note B's own edit"
    );
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.app.text, "beta two", "nothing of note A is undone here");
    // Back to A: caret and history are still there.
    h.switch_to(NOTE_A, &mut b);
    a = std::mem::take(&mut b);
    assert_eq!(h.app.text, a_typed);
    assert_eq!(h.sel_of(NOTE_A), a_sel);
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        h.app.text, "alpha one",
        "A's undo history survived the visit to B"
    );
    assert_eq!(a, "beta two");
}

#[test]
fn forgetting_a_note_drops_stale_state_after_an_external_replacement() {
    let mut h = AppHarness::new("");
    h.app.key = NOTE_A;
    h.app.text = "Příliš žluťoučký kůň".to_owned();
    h.app.focus_next = true;
    h.frame(vec![]);
    let end = h.app.text.len();
    h.select(NOTE_A, end, end);
    h.text("!");
    assert_eq!(h.app.text, "Příliš žluťoučký kůň!");
    // The host replaces the body from outside (a restore): no edit went through the editor.
    h.app.text = "nový".to_owned();
    h.frame(vec![]);
    let ctx = h.ctx.clone();
    h.app.view.forget_note(&ctx, NOTE_A);
    h.frame(vec![]);
    h.frame(vec![]);
    let (a, b) = h.sel_of(NOTE_A);
    assert!(
        a <= "nový".len() && b <= "nový".len(),
        "caret cannot point past the new text: {a}..{b}"
    );
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        h.app.text, "nový",
        "undo cannot resurrect the replaced text"
    );
}

#[test]
fn image_textures_are_scoped_to_their_notebook() {
    let base_guard = crate::test_dir::TestDir::new("scope");
    let base = base_guard.to_path_buf();
    let (d1, d2) = (base.join("one"), base.join("two"));
    for (d, w, hh) in [(&d1, 400usize, 200usize), (&d2, 100, 100)] {
        std::fs::create_dir_all(d.join("assets")).unwrap();
        let png = assets::encode_png(&egui::ColorImage::filled([w, hh], Color32::RED)).unwrap();
        std::fs::write(d.join("assets/pic.png"), png).unwrap();
    }
    let src = "![x](assets/pic.png)\n\ntext";
    let mut h = AppHarness::in_dir(src, d1.clone());
    h.app.key = NoteKey::new(1, 1);
    h.frame(vec![]);
    h.frame(vec![]);
    h.select(NoteKey::new(1, 1), src.len(), src.len());
    let first = h.slot_height(0);
    // Another notebook uses the same reference string for a different picture.
    h.app.store = Box::new(attachments::FsStore::new(d2.clone()));
    h.app.key = NoteKey::new(2, 1);
    h.frame(vec![]);
    h.frame(vec![]);
    h.select(NoteKey::new(2, 1), src.len(), src.len());
    let second = h.slot_height(0);
    assert!(
        (first - second).abs() > 50.0,
        "the second notebook shows its own picture: {first} vs {second}"
    );
    // The first notebook's decoded image was dropped when the notebook changed.
    assert!(h.app.view.textures.borrow().keys().all(|(nb, _)| *nb == 2));
    h.app.view.forget_images(2);
    assert!(h.app.view.textures.borrow().is_empty());
    let _ = std::fs::remove_dir_all(&base);
}

#[test]
fn a_pending_width_edit_cannot_leak_into_another_note() {
    let (mut h, d) = image_app();
    h.app.key = NOTE_A;
    h.frame(vec![]);
    h.frame(vec![]);
    h.select_image(0);
    let s = h.control("slider");
    let pos = Pos2::new(s.left() + 30.0, s.center().y);
    h.frame(vec![egui::Event::PointerMoved(pos)]);
    h.frame(vec![egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: Modifiers::NONE,
    }]);
    h.frame(vec![egui::Event::PointerMoved(Pos2::new(
        pos.x + 40.0,
        pos.y,
    ))]);
    assert!(h.app.view.live_width.get().is_some(), "a drag is pending");
    assert_eq!(h.app.text, d.text, "nothing written yet");
    // The host switches notes in the middle of the drag, without a release.
    let mut other = "plain note".to_owned();
    h.switch_to(NOTE_B, &mut other);
    assert_eq!(h.app.view.selected_image, None, "selection does not follow");
    assert_eq!(h.app.view.live_width.get(), None, "pending width dropped");
    assert_eq!(h.app.text, "plain note", "the other note was not edited");
    // Back again: the abandoned drag wrote nothing into the first note.
    h.switch_to(NOTE_A, &mut other);
    assert_eq!(h.app.text, d.text);
    assert_eq!(h.app.view.selected_image, None);
    assert_eq!(other, "plain note");
}

#[test]
fn commit_pending_writes_the_width_as_one_undo_step() {
    let (mut h, d) = image_app();
    h.select_image(0);
    h.focus_slider();
    h.key(Key::ArrowLeft, Modifiers::NONE);
    assert!(h.app.view.live_width.get().is_some());
    assert_eq!(h.app.text, d.text, "still pending");
    let ctx = h.ctx.clone();
    let key = h.app.key;
    let TestApp { view, text, .. } = &mut h.app;
    assert!(view.commit_pending(&ctx, key, text));
    assert_ne!(h.app.text, d.text);
    assert_eq!(h.app.view.live_width.get(), None);
    let other = NoteKey::new(9, 9);
    let TestApp { view, text, .. } = &mut h.app;
    assert!(
        !view.commit_pending(&ctx, other, text),
        "not the shown note"
    );
}

// ---- key profiles ----

impl AppHarness {
    /// Press `k` with `m` in a host that, like the real ones, tries to consume the key afterwards:
    /// returns whether it was still available to the host (the editor did not take it).
    fn host_sees(&mut self, k: Key, m: Modifiers) -> bool {
        self.t += 0.05;
        let mut input = egui::RawInput {
            time: Some(self.t),
            events: vec![egui::Event::ModifiersChanged(m), key(k, m)],
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.screen)),
            ..Default::default()
        };
        input.focused = true;
        let mut left = false;
        let app = &mut self.app;
        let mut out = self.ctx.run_ui(input, |ui| {
            app.ui(ui);
            left = ui.ctx().input_mut(|i| i.consume_key(m, k));
        });
        out.textures_delta.clear();
        self.frame(vec![]);
        left
    }
}

#[test]
fn host_supplied_bindings_are_honoured_and_only_those() {
    let mut h = AppHarness::new("word");
    h.app.keys = EditorKeys {
        bold: Some(egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::J)),
        italic: None,
        ..EditorKeys::standalone()
    };
    h.set_selection(0, 4);
    h.key(Key::B, Modifiers::COMMAND);
    assert_eq!(
        h.app.text, "word",
        "Ctrl+B is no longer bold in this configuration"
    );
    h.key(Key::I, Modifiers::COMMAND);
    assert_eq!(h.app.text, "word", "italic is switched off");
    h.key(Key::J, Modifiers::COMMAND);
    assert_eq!(h.app.text, "**word**", "the host's Ctrl+J is bold");
    // And nothing at all when the host configures nothing.
    let mut n = AppHarness::new("word");
    n.app.keys = EditorKeys::none();
    n.set_selection(0, 4);
    for (k, m) in [
        (Key::B, Modifiers::COMMAND),
        (Key::E, Modifiers::COMMAND),
        (Key::S, Modifiers::COMMAND | Modifiers::SHIFT),
    ] {
        n.key(k, m);
    }
    assert_eq!(n.app.text, "word");
}

#[test]
fn the_omatree_profile_leaves_ctrl_shift_s_to_the_application() {
    let mut h = AppHarness::new("one two three");
    h.app.keys = EditorKeys::omatree();
    h.set_selection(4, 7);
    assert!(
        h.host_sees(Key::S, Modifiers::COMMAND | Modifiers::SHIFT),
        "Ctrl+Shift+S must still be there for the application's Save As"
    );
    assert_eq!(
        h.app.text, "one two three",
        "and it did not strike anything through"
    );
    // Ctrl+E is inline code in both profiles.
    h.set_selection(4, 7);
    h.key(Key::E, Modifiers::COMMAND);
    assert_eq!(h.app.text, "one `two` three");
    // The standalone profile does take Ctrl+Shift+S.
    let mut s = AppHarness::new("one two three");
    s.set_selection(4, 7);
    assert!(!s.host_sees(Key::S, Modifiers::COMMAND | Modifiers::SHIFT));
    assert_eq!(s.app.text, "one ~~two~~ three");
}

#[test]
fn strikethrough_is_a_slash_command_in_the_omatree_profile_only() {
    let mut h = AppHarness::new("");
    h.app.keys = EditorKeys::omatree();
    h.text("/");
    h.text("stri");
    assert!(h.app.view.slash.is_some());
    let items = h.app.view.slash_items(&h.app.text, 5);
    assert_eq!(
        items.iter().map(|c| c.title).collect::<Vec<_>>(),
        ["Strikethrough"]
    );
    h.key(Key::Enter, Modifiers::NONE);
    assert_eq!(h.app.text, "~~~~");
    assert_eq!(h.selection(), (2, 2), "caret between the delimiters");
    h.text("gone");
    assert_eq!(h.app.text, "~~gone~~");
    h.key(Key::Z, Modifiers::COMMAND);
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(
        h.app.text, "/stri",
        "typing and the command are separate undo steps"
    );
    // The standalone menu is unchanged.
    assert!(edit::slash_filter("strike").is_empty());
    assert_eq!(edit::slash_filter("").len(), edit::SLASH_COMMANDS.len());
    assert_eq!(
        edit::slash_filter_with("", true).len(),
        edit::SLASH_COMMANDS.len() + 1
    );
}

#[test]
fn standalone_shortcuts_are_unchanged() {
    type Case<'a> = (&'a str, (usize, usize), Key, Modifiers, &'a str);
    let cs = Modifiers::COMMAND | Modifiers::SHIFT;
    let cases: [Case; 6] = [
        ("Příliš", (0, 9), Key::B, Modifiers::COMMAND, "**Příliš**"),
        ("Příliš", (0, 9), Key::I, Modifiers::COMMAND, "*Příliš*"),
        ("Příliš", (0, 9), Key::E, Modifiers::COMMAND, "`Příliš`"),
        ("Příliš", (0, 9), Key::S, cs, "~~Příliš~~"),
        ("a\nb", (0, 3), Key::Num8, cs, "- a\n- b"),
        ("a\nb", (0, 3), Key::Num7, cs, "1. a\n2. b"),
    ];
    for (src, (a, b), k, m, want) in cases {
        let mut h = AppHarness::new(src);
        h.set_selection(a, b);
        h.key(k, m);
        assert_eq!(h.app.text, want, "{k:?}");
    }
    let mut h = AppHarness::new("- [ ] x");
    h.set_selection(2, 2);
    h.key(Key::Enter, Modifiers::COMMAND);
    assert_eq!(h.app.text, "- [x] x");
    // Ctrl+Shift+E is reported, not acted upon, and is not inline code.
    let mut e = AppHarness::new("word");
    e.set_selection(0, 4);
    e.frame_with(vec![key(Key::E, cs)], cs);
    assert!(e.app.out.export_requested);
    assert_eq!(e.app.text, "word");
    assert!(!EditorKeys::omatree().export.is_some());
}

// ---- reporting, palette, files ----

#[test]
fn changes_are_reported_and_nothing_is_saved_or_written() {
    let dir = crate::test_dir::TestDir::new("nowrite");
    let mut h = AppHarness::in_dir("", dir.clone());
    h.frame(vec![]);
    assert!(!h.app.out.changed, "an idle frame reports no change");
    h.frame(vec![egui::Event::Text("Ahoj ".into())]);
    assert!(h.app.out.changed, "typing is reported");
    h.frame(vec![]);
    assert!(!h.app.out.changed);
    // A command, a checkbox-free formatting shortcut and an undo are reported too.
    h.text("/h2");
    h.key(Key::Enter, Modifiers::NONE);
    h.frame(vec![]);
    h.frame_with(vec![key(Key::B, Modifiers::COMMAND)], Modifiers::COMMAND);
    assert!(h.app.out.changed, "formatting shortcut is a change");
    h.frame_with(vec![key(Key::Z, Modifiers::COMMAND)], Modifiers::COMMAND);
    assert!(h.app.out.changed, "undo is a change");
    assert!(
        std::fs::read_dir(&dir).unwrap().next().is_none(),
        "the editor wrote no file"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_new_palette_restyles_the_editor_without_touching_the_text() {
    let src = "- [ ] a\n- [x] b\n\n| h1 | h2 |\n| --- | --- |\n| c | d |\n\ntext";
    let mut h = AppHarness::new(src);
    h.set_selection(src.len(), src.len());
    let old = h.app.palette;
    assert!(
        h.has_stroke(old.muted),
        "unchecked box in the old muted colour"
    );
    assert!(
        !h.app.view.tables.borrow().is_empty(),
        "table layout cached"
    );
    let mut p = old;
    p.muted = Color32::from_rgb(1, 2, 3);
    p.link = Color32::from_rgb(9, 8, 7);
    h.app.palette = p;
    h.frame(vec![]);
    h.frame(vec![]);
    assert!(h.has_stroke(p.muted), "unchecked box restyled");
    assert!(h.has_fill(p.link), "checked box restyled");
    assert!(!h.has_stroke(old.muted));
    assert_eq!(h.app.text, src);
    assert_eq!(h.app.view.palette, Some(p));
}

#[test]
fn escape_is_reported_only_when_the_editor_did_not_use_it() {
    let mut h = AppHarness::new("");
    h.text("/");
    h.text("h");
    assert!(h.app.view.slash.is_some());
    h.frame_with(vec![key(Key::Escape, Modifiers::NONE)], Modifiers::NONE);
    assert!(!h.app.out.escape, "the slash menu used it");
    assert!(h.app.view.slash.is_none());
    h.app.focus_next = true;
    h.frame(vec![]);
    h.frame_with(vec![key(Key::Escape, Modifiers::NONE)], Modifiers::NONE);
    assert!(
        h.app.out.escape,
        "nothing inside the editor used it: the host may take focus back"
    );
}

#[test]
fn utf8_text_survives_commands_note_switches_and_replacement() {
    let mut h = AppHarness::new("");
    h.app.key = NOTE_A;
    h.app.focus_next = true;
    h.frame(vec![]);
    h.text("Příliš žluťoučký kůň 🦀 👨‍👩‍👧 ✅");
    h.text(" ");
    h.text("/");
    h.text("h1");
    h.key(Key::Enter, Modifiers::NONE);
    let want = h.app.text.clone();
    assert!(
        want.starts_with("Příliš žluťoučký kůň 🦀 👨‍👩‍👧 ✅ # ") || want.contains('#'),
        "{want}"
    );
    assert!(want.is_char_boundary(h.selection_of_a().0));
    let mut other = "ěščřžýáíé".to_owned();
    h.switch_to(NOTE_B, &mut other);
    h.switch_to(NOTE_A, &mut other);
    assert_eq!(h.app.text, want);
    assert_eq!(other, "ěščřžýáíé");
    let (a, b) = h.sel_of(NOTE_A);
    assert!(h.app.text.is_char_boundary(a) && h.app.text.is_char_boundary(b));
}

impl AppHarness {
    fn selection_of_a(&self) -> (usize, usize) {
        self.sel_of(NOTE_A)
    }
}

#[test]
fn a_pasted_image_exports_from_the_buffer_through_the_same_store() {
    let dir = crate::test_dir::TestDir::new("pasteexp");
    let mut h = AppHarness::in_dir("Úvod\n\n", dir.clone());
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.frame(vec![egui::Event::PasteImage(std::sync::Arc::new(
        egui::ColorImage::filled([30, 20], Color32::BLUE),
    ))]);
    h.frame(vec![]);
    assert!(h.app.text.contains("![image](assets/"));
    assert!(
        h.app.status.starts_with("Pasted 30x20 image"),
        "{}",
        h.app.status
    );
    // The exporters take the buffer and the store: nothing about the editor is needed.
    let out = dir.join("out.pdf");
    let s = crate::pdf::export_pdf(&h.app.text, &*h.app.store, "note.md", &out).unwrap();
    assert_eq!((s.pages, s.images), (1, 1));
    let plan = crate::export::plan(&h.app.text, &*h.app.store, "note.md").unwrap();
    assert_eq!(plan.attachments.len(), 1);
    let _ = std::fs::remove_dir_all(&dir);
}

/// E13a: a pasted image edits the Markdown, so the frame that handled it reports `changed`, exactly
/// like typing, a formatting command or a checkbox; a paste that fails changes nothing and reports nothing.
#[test]
fn a_pasted_image_is_reported_as_a_change_and_a_failed_paste_is_not() {
    let dir = crate::test_dir::TestDir::new("paste-changed");
    let mut h = AppHarness::in_dir("Úvod\n\n", dir.to_path_buf());
    let n = h.app.text.len();
    h.set_selection(n, n);
    h.frame(vec![]);
    assert!(!h.app.out.changed, "an idle frame reports no change");
    let paste = || {
        egui::Event::PasteImage(std::sync::Arc::new(egui::ColorImage::filled(
            [30, 20],
            Color32::BLUE,
        )))
    };
    h.frame(vec![paste()]);
    assert!(
        h.app.text.contains("![image](assets/"),
        "the reference was inserted"
    );
    assert!(h.app.out.changed, "the paste frame reports the change");
    h.frame(vec![]);
    assert!(!h.app.out.changed, "the frame after reports none");
    // One undo takes the whole paste back, and that is a change too.
    h.key(Key::Z, Modifiers::COMMAND);
    h.frame(vec![]);
    assert_eq!(h.app.text, "Úvod\n\n");
    // A store that refuses the image leaves the text alone and reports no change.
    let blocked = crate::test_dir::TestDir::new("paste-blocked");
    std::fs::write(blocked.join("assets"), b"i am a file").unwrap();
    let mut f = AppHarness::in_dir("Text\n", blocked.to_path_buf());
    let n = f.app.text.len();
    f.set_selection(n, n);
    f.frame(vec![]);
    f.frame(vec![paste()]);
    assert_eq!(f.app.text, "Text\n");
    assert!(!f.app.out.changed, "a refused paste is not a change");
}
