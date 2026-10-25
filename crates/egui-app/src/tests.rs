//! Host tests: a real `Document` on copied fixture files, driven frame by frame through egui's
//! headless `Context`. Every test makes its own files under the temp directory; none touches a
//! user's notebook.

use super::*;
use eframe::egui::{Event, Pos2, RawInput, Rect, Vec2};
use omatree_core::document::Document;
use omatree_editor::attachments::AttachmentStore;
use std::fs;

const CTRL: Modifiers = Modifiers::COMMAND;

const PLAN: &str = "# Plán\n\nPříliš žluťoučký kůň úpěl ďábelské ódy 🦀\n\n\
| Název | Počet |\n|---|---|\n| jablka 🍎 | 3 |\n| hrušky | 5 |\n\n- [ ] úkol\n";

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("omatree-egui-{}-{name}", std::process::id()));
    let _ = fs::remove_dir_all(&d);
    fs::create_dir_all(&d).unwrap();
    d
}

/// Roots "Projects" (children "Plan", "Ideas" with a child "Deep") and "Journal".
fn make_fixture(path: &Path) -> Vec<(NodeId, String, String)> {
    let mut doc = Document::open(path).unwrap();
    let projects = doc.create_root("Projects").unwrap();
    let plan = doc.create_child(projects, "Plán").unwrap();
    let ideas = doc.create_child(projects, "Ideas").unwrap();
    let deep = doc.create_child(ideas, "Deep").unwrap();
    let journal = doc.create_root("Journal").unwrap();
    doc.set_body(projects, "# Projects\n\nroot body\n").unwrap();
    doc.set_body(plan, PLAN).unwrap();
    doc.set_body(ideas, "ideas\n").unwrap();
    doc.set_body(deep, "deep note\n").unwrap();
    doc.set_body(journal, "journal 🦀\n").unwrap();
    doc.save().unwrap();
    doc.notebook()
        .depth_first()
        .iter()
        .map(|n| (n.id(), n.title().to_owned(), n.body().to_owned()))
        .collect()
}

/// Titles, parents, positions and bodies straight from disk.
fn snapshot(path: &Path) -> Vec<(u64, Option<u64>, usize, String, String)> {
    let doc = Document::open_existing(path).unwrap();
    doc.notebook()
        .depth_first()
        .iter()
        .map(|n| {
            (
                n.id().get(),
                n.parent_id().map(NodeId::get),
                n.position(),
                n.title().to_owned(),
                n.body().to_owned(),
            )
        })
        .collect()
}

struct H {
    ctx: egui::Context,
    host: Host,
    t: f64,
}

impl H {
    fn open(path: &Path) -> Self {
        let mut h = H {
            ctx: egui::Context::default(),
            host: Host::new(),
            t: 0.0,
        };
        assert!(h.host.open(&h.ctx.clone(), path), "{}", h.host.status());
        h.settle();
        h
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![], Modifiers::NONE);
        }
    }

    fn frame(&mut self, mut events: Vec<Event>, m: Modifiers) {
        events.insert(0, Event::ModifiersChanged(m));
        self.t += 0.05;
        let input = RawInput {
            time: Some(self.t),
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 700.0))),
            focused: true,
            ..Default::default()
        };
        let host = &mut self.host;
        let mut out = self.ctx.run_ui(input, |ui| host.ui(ui));
        out.textures_delta.clear();
    }

    fn key(&mut self, key: Key, m: Modifiers) {
        let e = Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        };
        self.frame(vec![e], m);
        self.frame(vec![], Modifiers::NONE);
    }

    fn type_text(&mut self, s: &str) {
        self.frame(vec![Event::Text(s.to_owned())], Modifiers::NONE);
        self.frame(vec![], Modifiers::NONE);
    }

    fn select(&mut self, title: &str) {
        let id = self.id(title);
        self.host.select(&self.ctx.clone(), id);
        self.settle();
    }

    fn id(&self, title: &str) -> NodeId {
        self.host
            .rows()
            .iter()
            .find(|r| r.title == title)
            .unwrap_or_else(|| panic!("no note {title}"))
            .id
    }

    fn body(&self, title: &str) -> String {
        let id = self.id(title);
        self.host
            .document()
            .unwrap()
            .notebook()
            .get(id)
            .unwrap()
            .body()
            .to_owned()
    }

    fn key_of(&self, title: &str) -> NoteKey {
        key_of(self.host.notebook_id(), self.id(title))
    }

    fn caret(&self, title: &str) -> (usize, usize) {
        let text = self.body(title);
        EditorView::selection(&self.ctx, self.key_of(title), &text)
    }

    fn set_caret(&mut self, a: usize, b: usize) {
        use egui::text::{CCursor, CCursorRange};
        let id = self.host.selected().unwrap();
        let key = key_of(self.host.notebook_id(), id);
        let text = self.host.buffer().to_owned();
        let c = |x: usize| CCursor::new(text[..x].chars().count());
        let wid = key.widget_id();
        let mut st = egui::TextEdit::load_state(&self.ctx, wid).unwrap_or_default();
        st.cursor
            .set_char_range(Some(CCursorRange::two(c(a), c(b))));
        st.store(&self.ctx, wid);
        self.settle();
    }
}

fn bodies(path: &Path) -> Vec<(String, String)> {
    snapshot(path).into_iter().map(|r| (r.3, r.4)).collect()
}

#[test]
fn opens_a_schema_v3_notebook_and_keeps_ids_titles_and_hierarchy() {
    let d = dir("open");
    let p = d.join("n.omatree");
    let nodes = make_fixture(&p);
    // The file's schema version is 3 (SQLite header, user_version at offset 60).
    let raw = fs::read(&p).unwrap();
    assert_eq!(&raw[60..64], &[0, 0, 0, 3]);

    let h = H::open(&p);
    let shown: Vec<_> = h
        .host
        .rows()
        .iter()
        .map(|r| (r.id, r.depth, r.title.as_str()))
        .collect();
    let expected: Vec<_> = nodes
        .iter()
        .map(|(id, t, _)| {
            let depth = match t.as_str() {
                "Projects" | "Journal" => 0,
                "Plán" | "Ideas" => 1,
                _ => 2,
            };
            (*id, depth, t.as_str())
        })
        .collect();
    assert_eq!(shown, expected);
    assert_eq!(
        shown.iter().map(|r| r.2).collect::<Vec<_>>(),
        ["Projects", "Plán", "Ideas", "Deep", "Journal"]
    );
    assert_eq!(h.host.selected(), Some(nodes[0].0), "first note is shown");
    assert!(!h.host.is_dirty());
    assert_eq!(h.host.buffer(), "# Projects\n\nroot body\n");
}

#[test]
fn selecting_other_nodes_loads_their_bodies_without_dirtying() {
    let d = dir("select");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    for (title, body) in [
        ("Plán", PLAN),
        ("Deep", "deep note\n"),
        ("Journal", "journal 🦀\n"),
        ("Projects", "# Projects\n\nroot body\n"),
    ] {
        h.select(title);
        assert_eq!(h.host.buffer(), body);
        assert_eq!(h.host.selected(), Some(h.id(title)));
    }
    assert!(!h.host.is_dirty(), "switching alone never dirties");
    h.host.save(&h.ctx.clone());
    assert_eq!(h.host.status(), "Saved");
}

#[test]
fn typing_reaches_the_document_every_frame_and_is_saved_and_reopened() {
    let d = dir("roundtrip");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let before = snapshot(&p);
    let mut h = H::open(&p);
    h.select("Journal");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("Ahoj, světe! 😀 ");
    assert_eq!(h.body("Journal"), "journal 🦀\nAhoj, světe! 😀 ");
    assert_eq!(h.body("Journal"), h.host.buffer());
    assert!(h.host.is_dirty());
    assert_eq!(snapshot(&p), before, "nothing is written before Ctrl+S");

    h.key(Key::S, CTRL);
    assert_eq!(h.host.status(), "Saved");
    assert!(!h.host.is_dirty());
    drop(h);

    let after = snapshot(&p);
    for (b, a) in before.iter().zip(&after) {
        assert_eq!((b.0, b.1, b.2, &b.3), (a.0, a.1, a.2, &a.3), "structure");
        if a.3 == "Journal" {
            assert_eq!(a.4, "journal 🦀\nAhoj, světe! 😀 ");
        } else {
            assert_eq!(b.4, a.4, "unedited body of {} is byte-identical", a.3);
        }
    }
    let h2 = H::open(&p);
    assert_eq!(h2.body("Journal"), "journal 🦀\nAhoj, světe! 😀 ");
}

#[test]
fn czech_text_emoji_and_a_table_survive_editing_and_saving() {
    let d = dir("czech");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Plán");
    // Caret after "kůň" - a multi-byte position.
    let at = PLAN.find("ďábelské").unwrap();
    h.set_caret(at, at);
    h.type_text("Žluťoučký ");
    h.key(Key::S, CTRL);
    drop(h);
    let want = PLAN.replacen("ďábelské", "Žluťoučký ďábelské", 1);
    let h2 = H::open(&p);
    assert_eq!(h2.body("Plán"), want);
    assert!(want.contains("| jablka 🍎 | 3 |"), "table source intact");
}

#[test]
fn caret_selection_and_undo_are_isolated_per_note() {
    let d = dir("undo");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);

    h.select("Ideas");
    h.set_caret(0, 3);
    h.set_caret(1, 4);
    h.set_caret(6, 6);
    h.type_text("A1");
    assert_eq!(h.body("Ideas"), "ideas\nA1");

    h.select("Journal");
    let jend = h.host.buffer().len();
    h.set_caret(jend, jend);
    h.type_text("B1");
    assert_eq!(h.body("Journal"), "journal 🦀\nB1");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Journal"), "journal 🦀\n", "undo in Journal");
    assert_eq!(h.body("Ideas"), "ideas\nA1", "Ideas is untouched by it");
    h.key(Key::Z, CTRL);
    assert_eq!(
        h.body("Ideas"),
        "ideas\nA1",
        "undo past the start does nothing elsewhere"
    );
    assert_eq!(h.body("Journal"), "journal 🦀\n");

    h.select("Ideas");
    assert_eq!(h.caret("Ideas"), (8, 8), "caret restored after returning");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Ideas"), "ideas\n");
    assert_eq!(h.body("Journal"), "journal 🦀\n");
    h.key(Key::Z, CTRL | Modifiers::SHIFT);
    assert_eq!(h.body("Ideas"), "ideas\nA1", "redo");
    assert_eq!(h.host.buffer(), h.body("Ideas"));
}

#[test]
fn replacing_the_notebook_invalidates_editor_state_and_ids() {
    let d = dir("replace");
    let a = d.join("a.omatree");
    let b = d.join("b.omatree");
    make_fixture(&a);
    make_fixture(&b);
    let mut h = H::open(&a);
    h.select("Journal");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("zzz");
    let old = h.host.notebook_id();
    let old_key = h.key_of("Journal");
    h.host.save(&h.ctx.clone());

    assert!(h.host.open(&h.ctx.clone(), &b));
    h.settle();
    assert_ne!(h.host.notebook_id(), old);
    // Same node numbers, different notebook: no state (and no undo) carries over.
    h.select("Journal");
    assert_eq!(h.host.buffer(), "journal 🦀\n");
    h.key(Key::Z, CTRL);
    assert_eq!(h.host.buffer(), "journal 🦀\n");
    assert!(egui::TextEdit::load_state(&h.ctx, old_key.widget_id()).is_none());
    assert_eq!(bodies(&a)[4].1, "journal 🦀\nzzz");
    assert_eq!(
        bodies(&b)[4].1,
        "journal 🦀\n",
        "the other file is untouched"
    );
}

#[test]
fn a_failed_open_keeps_the_current_notebook_and_explains() {
    let d = dir("failopen");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Deep");
    assert!(!h.host.open(&h.ctx.clone(), &d.join("missing.omatree")));
    assert!(
        h.host.status().contains("could not be accessed"),
        "{}",
        h.host.status()
    );
    assert!(h.host.error().is_none(), "a notebook is still open");
    assert_eq!(h.host.buffer(), "deep note\n");
    assert!(!d.join("missing.omatree").exists(), "never created");
}

#[test]
fn missing_invalid_and_unsupported_files_are_refused_clearly_and_never_modified() {
    let d = dir("invalid");
    let ctx = egui::Context::default();

    let missing = d.join("nope.omatree");
    let mut host = Host::new();
    assert!(!host.open(&ctx, &missing));
    assert!(host.error().unwrap().contains("could not be accessed"));
    assert!(!missing.exists());

    let text = d.join("text.omatree");
    let text_bytes = "this is not a database, just text, long enough to look like a file\n";
    fs::write(&text, text_bytes).unwrap();
    let mut host = Host::new();
    assert!(!host.open(&ctx, &text));
    assert!(host
        .error()
        .unwrap()
        .contains("not a readable OmaTree notebook"));

    let empty = d.join("empty.omatree");
    fs::write(&empty, b"").unwrap();
    let mut host = Host::new();
    assert!(!host.open(&ctx, &empty));
    assert!(host.error().is_some());

    let future = d.join("future.omatree");
    make_fixture(&future);
    let mut raw = fs::read(&future).unwrap();
    raw[60..64].copy_from_slice(&99u32.to_be_bytes());
    fs::write(&future, &raw).unwrap();
    let mut host = Host::new();
    assert!(!host.open(&ctx, &future));
    assert!(host
        .error()
        .unwrap()
        .contains("not a notebook this version of OmaTree can open"));

    assert_eq!(fs::read(&text).unwrap(), text_bytes.as_bytes());
    assert_eq!(fs::read(&empty).unwrap().len(), 0);
    assert_eq!(
        fs::read(&future).unwrap(),
        raw,
        "unsupported file untouched"
    );
    // The window still renders an explanation instead of an editor.
    let mut h = H { ctx, host, t: 0.0 };
    h.settle();
}

#[test]
fn an_external_modification_is_reported_and_nothing_is_lost_or_overwritten() {
    let d = dir("conflict");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Journal");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("mine");

    // Another process (here: another Document) saves a change first.
    let mut other = Document::open_existing(&p).unwrap();
    let j = other.notebook().depth_first().last().unwrap().id();
    other.set_body(j, "theirs\n").unwrap();
    other.save().unwrap();

    h.key(Key::S, CTRL);
    assert!(
        h.host.status().contains("changed on disk"),
        "{}",
        h.host.status()
    );
    assert!(h.host.is_dirty(), "still unsaved");
    assert_eq!(h.host.buffer(), "journal 🦀\nmine", "my text is still here");
    assert_eq!(
        bodies(&p)[4].1,
        "theirs\n",
        "their version was not overwritten"
    );
}

#[test]
fn a_save_that_cannot_write_is_reported_and_the_edit_stays_dirty() {
    use std::os::unix::fs::PermissionsExt;
    let d = dir("saveerr");
    let nb = d.join("nb");
    fs::create_dir_all(&nb).unwrap();
    let p = nb.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Journal");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("x");
    // SQLite needs to create a journal next to the file.
    fs::set_permissions(&nb, fs::Permissions::from_mode(0o555)).unwrap();
    h.key(Key::S, CTRL);
    fs::set_permissions(&nb, fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        h.host.status().contains("could not be saved"),
        "{}",
        h.host.status()
    );
    assert!(h.host.is_dirty());
    assert_eq!(bodies(&p)[4].1, "journal 🦀\n");
    // Once writable again the same Ctrl+S succeeds.
    h.key(Key::S, CTRL);
    assert_eq!(h.host.status(), "Saved");
    assert_eq!(bodies(&p)[4].1, "journal 🦀\nx");
}

#[test]
fn ctrl_shift_s_is_reserved_and_neither_saves_nor_formats() {
    let d = dir("saveas");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Journal");
    h.set_caret(0, 5);
    h.type_text("q");
    h.set_caret(0, 1);
    let before = h.host.buffer().to_owned();
    h.key(Key::S, CTRL | Modifiers::SHIFT);
    assert!(h.host.status().contains("Save As"));
    assert_eq!(h.host.buffer(), before, "no strikethrough markers");
    assert!(h.host.is_dirty(), "did not save");
}

#[test]
fn ctrl_e_is_inline_code_and_strikethrough_is_a_slash_command() {
    let d = dir("keys");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Ideas");
    h.set_caret(0, 5);
    h.key(Key::E, CTRL);
    assert_eq!(h.host.buffer(), "`ideas`\n");
    assert_eq!(h.body("Ideas"), "`ideas`\n");
    assert!(EditorKeys::omatree().strike_command);
}

#[test]
fn pasting_an_image_creates_no_reference_and_explains() {
    let d = dir("paste");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let mut h = H::open(&p);
    h.select("Ideas");
    let n = h.host.buffer().len();
    h.set_caret(n, n);
    let img = egui::ColorImage::filled([8, 8], egui::Color32::BLUE);
    h.frame(
        vec![Event::PasteImage(std::sync::Arc::new(img))],
        Modifiers::NONE,
    );
    h.frame(vec![], Modifiers::NONE);
    assert_eq!(h.body("Ideas"), "ideas\n", "no broken Markdown reference");
    assert!(
        h.host.status().contains("Image paste failed"),
        "{}",
        h.host.status()
    );
    assert!(h.host.status().contains("not supported yet"));
    assert!(!h.host.is_dirty());
    assert!(
        !d.join("assets").exists(),
        "nothing written beside the notebook"
    );
}

#[test]
fn external_and_unavailable_image_references_are_rejected_and_left_in_the_source() {
    let store = UnavailableStore;
    for r in [
        "https://example.com/x.png",
        "http://127.0.0.1/x.png",
        "file:///etc/passwd",
        "/etc/passwd",
        "../secret.png",
        "assets/0123456789abcdef0123456789abcdef.png",
        "C:\\x.png",
    ] {
        assert!(store.get(r).is_err(), "{r}");
        assert!(store.info(r).is_err(), "{r}");
    }
    assert!(store.put(b"png", "png").is_err());

    // A note that references an existing local file is shown but never loads it, and the source
    // bytes are exactly as stored.
    let d = dir("imgref");
    fs::create_dir_all(d.join("assets")).unwrap();
    fs::write(d.join("assets/a.png"), b"not really a png").unwrap();
    let p = d.join("n.omatree");
    let body =
        "![w](https://example.com/x.png)\n\n![l](assets/a.png =120x)\n\n![f](file:///etc/passwd)\n";
    let mut doc = Document::open(&p).unwrap();
    let r = doc.create_root("Pics").unwrap();
    doc.set_body(r, body).unwrap();
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    for _ in 0..5 {
        h.frame(vec![], Modifiers::NONE);
    }
    assert_eq!(h.host.buffer(), body);
    assert!(!h.host.is_dirty());
}

#[test]
fn an_untouched_notebook_copy_leaves_the_original_fixture_byte_identical() {
    let d = dir("copy");
    let master = d.join("master.omatree");
    make_fixture(&master);
    let original = fs::read(&master).unwrap();
    let copy = d.join("copy.omatree");
    fs::copy(&master, &copy).unwrap();
    let mut h = H::open(&copy);
    h.select("Ideas");
    let n = h.host.buffer().len();
    h.set_caret(n, n);
    h.type_text("edit");
    h.key(Key::S, CTRL);
    assert_eq!(fs::read(&master).unwrap(), original);
    assert_ne!(bodies(&copy), bodies(&master));
}

#[test]
fn the_egui_application_has_no_qt_dependency() {
    let manifest = fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")).unwrap();
    let deps = manifest
        .split("[dependencies]")
        .nth(1)
        .unwrap()
        .to_lowercase();
    for banned in ["cxx", "qt", "qml"] {
        assert!(!deps.contains(banned), "dependency mentions {banned}");
    }
}

#[test]
fn rows_are_computed_once_and_indentation_follows_depth() {
    let d = dir("rows");
    let p = d.join("n.omatree");
    make_fixture(&p);
    let h = H::open(&p);
    let depths: Vec<_> = h.host.rows().iter().map(|r| r.depth).collect();
    assert_eq!(depths, [0, 1, 1, 2, 0]);
}

/// Writes the GUI-check notebook to `$OMATREE_GUI_FIXTURE` (manual use only):
/// `OMATREE_GUI_FIXTURE=/tmp/x.omatree cargo test -p omatree-egui -- --ignored write_gui_fixture`
#[test]
#[ignore]
fn write_gui_fixture() {
    let path =
        PathBuf::from(std::env::var_os("OMATREE_GUI_FIXTURE").expect("set OMATREE_GUI_FIXTURE"));
    let _ = fs::remove_file(&path);
    make_fixture(&path);
}

/// Release-mode timings for the numbers in the migration plan:
/// `cargo test --release -p omatree-egui -- --ignored --nocapture timings`
#[test]
#[ignore]
fn timings() {
    fn median(v: &mut [f64]) -> f64 {
        v.sort_by(|a, b| a.total_cmp(b));
        v[v.len() / 2]
    }
    let d = dir("timings");
    let p = d.join("n.omatree");
    let small =
        "# Note\n\nPříliš žluťoučký kůň **tučně** a `kód` 🦀\n\n- [ ] úkol\n- [x] hotovo\n\n"
            .repeat(30);
    let big = "- [ ] úkol číslo jedna s **tučným** textem\n".repeat(5000);
    let mut doc = Document::open(&p).unwrap();
    for (title, body) in [("small A", &small), ("small B", &small), ("big", &big)] {
        let id = doc.create_root(title).unwrap();
        doc.set_body(id, body).unwrap();
    }
    doc.save().unwrap();
    drop(doc);
    let mut h = H::open(&p);
    let ms = |h: &mut H, f: &mut dyn FnMut(&mut H)| {
        let t = std::time::Instant::now();
        f(h);
        t.elapsed().as_secs_f64() * 1000.0
    };
    for (a, b) in [("small A", "small B"), ("small A", "big")] {
        let (mut first, mut back, mut idle) = (vec![], vec![], vec![]);
        for _ in 0..7 {
            h.select(a);
            first.push(ms(&mut h, &mut |h| h.select(b)));
            back.push(ms(&mut h, &mut |h| h.select(a)));
            h.select(b);
            idle.push(ms(&mut h, &mut |h| h.frame(vec![], Modifiers::NONE)));
        }
        println!(
            "switch {a}->{b}: median {:.1} ms (3 frames), back {:.1} ms, idle frame in {b} {:.1} ms",
            median(&mut first),
            median(&mut back),
            median(&mut idle)
        );
    }
    for title in ["small A", "big"] {
        h.select(title);
        let end = h.host.buffer().len();
        h.set_caret(end, end);
        let mut typing = vec![];
        for _ in 0..15 {
            typing.push(ms(&mut h, &mut |h| {
                h.frame(vec![Event::Text("x".into())], Modifiers::NONE)
            }));
        }
        println!(
            "typing one character in {title}: median {:.1} ms",
            median(&mut typing)
        );
    }
    let save = ms(&mut h, &mut |h| h.host.save(&h.ctx.clone()));
    println!("save (set_body write-through already done): {save:.1} ms");
}
