//! T3: inline Markdown inside table cells. The runs a cell is turned into, the geometry of the
//! styled grid, and the editor behaviour around it (raw source with the caret inside, the grid
//! outside, bytes never touched), driven through the real `EditorView`.

use super::multinote_host::MultiHost;
use crate::markdown::{parse, Kind, Table};
use crate::table::{self, runs, Run};
use crate::theme::Palette;
use egui::{Context, Event, Key, Modifiers, Pos2, Rect, Vec2};
use std::sync::Arc;

fn table_of(src: &str) -> Table {
    parse(src)
        .into_iter()
        .find_map(|s| match s.kind {
            Kind::Table(t) => Some(t),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no table in {src:?}"))
}

/// The runs of body cell (row, col) of a one-table document.
fn cell(src: &str, row: usize, col: usize) -> Vec<Run> {
    let t = table_of(src);
    let r = if row == 0 {
        &t.header
    } else {
        &t.rows[row - 1]
    };
    runs(src, &r.cells[col], &t.inline)
}

/// A one-cell body table around `cell_src`.
fn one(cell_src: &str) -> Vec<Run> {
    cell(&format!("| H |\n| --- |\n| {cell_src} |\n"), 1, 0)
}

fn r(text: &str, flags: &str) -> Run {
    Run {
        text: text.to_owned(),
        bold: flags.contains('b'),
        italic: flags.contains('i'),
        strike: flags.contains('s'),
        code: flags.contains('c'),
        link: flags.contains('l'),
    }
}

fn shown(runs: &[Run]) -> String {
    runs.iter().map(|r| r.text.as_str()).collect()
}

// --- Runs -----------------------------------------------------------------------------------------

#[test]
fn bold_italic_bold_italic_code_strike_and_link_cells() {
    assert_eq!(one("**Fixed**"), vec![r("Fixed", "b")]);
    assert_eq!(one("*soft*"), vec![r("soft", "i")]);
    assert_eq!(one("_soft_"), vec![r("soft", "i")]);
    assert_eq!(one("***both***"), vec![r("both", "bi")]);
    assert_eq!(one("`let x`"), vec![r("let x", "c")]);
    assert_eq!(one("~~gone~~"), vec![r("gone", "s")]);
    assert_eq!(
        one("[docs](https://example.com/a?b=1&c=2)"),
        vec![r("docs", "l")]
    );
    assert_eq!(
        one("<https://example.com>"),
        vec![r("https://example.com", "l")]
    );
}

#[test]
fn mixed_adjacent_and_nested_styles_in_one_cell() {
    assert_eq!(
        one("plain **bold** and `code` and ~~gone~~ [link](https://x.org)"),
        vec![
            r("plain ", ""),
            r("bold", "b"),
            r(" and ", ""),
            r("code", "c"),
            r(" and ", ""),
            r("gone", "s"),
            r(" ", ""),
            r("link", "l"),
        ]
    );
    assert_eq!(
        one("**a *b* c**"),
        vec![r("a ", "b"), r("b", "bi"), r(" c", "b")]
    );
    assert_eq!(one("**bold**`code`"), vec![r("bold", "b"), r("code", "c")]);
    assert_eq!(
        one("[**strong link**](https://x.org)"),
        vec![r("strong link", "bl")]
    );
    assert_eq!(one("~~**both**~~"), vec![r("both", "bs")]);
    // Code inside emphasis keeps being code; the emphasis does not reach into it as a face.
    assert_eq!(
        one("**a `c` b**"),
        vec![r("a ", "b"), r("c", "bc"), r(" b", "b")]
    );
}

#[test]
fn header_cells_carry_their_formatting_too() {
    let src = "| **Name** | `code` | plain |\n| --- | --- | --- |\n| a | b | c |\n";
    assert_eq!(cell(src, 0, 0), vec![r("Name", "b")]);
    assert_eq!(cell(src, 0, 1), vec![r("code", "c")]);
    assert_eq!(cell(src, 0, 2), vec![r("plain", "")]);
}

#[test]
fn escapes_pipes_and_malformed_delimiters() {
    // Escaped punctuation is literal text with the backslash concealed.
    assert_eq!(one(r"\*not bold\*"), vec![r("*not bold*", "")]);
    assert_eq!(one(r"a \\ b"), vec![r(r"a \ b", "")]);
    assert_eq!(one(r"\# \[x\] \`y\`"), vec![r("# [x] `y`", "")]);
    // A backslash before a letter is just a backslash.
    assert_eq!(one(r"C:\temp"), vec![r(r"C:\temp", "")]);
    // Escaped pipes, in ordinary text and in code.
    assert_eq!(one(r"a \| b"), vec![r("a | b", "")]);
    assert_eq!(one(r"`a \| b`"), vec![r("a | b", "c")]);
    // Backslashes in code stay (only `\|` is read as `|`).
    assert_eq!(one(r"`\*x\*`"), vec![r(r"\*x\*", "c")]);
    // Literal backticks around a code span.
    assert_eq!(one("`` a`b ``"), vec![r(" a`b ", "c")]);
    // Unmatched or unsupported delimiters stay as written.
    for raw in [
        "**open",
        "*a",
        "~~x",
        "[a](",
        "a * b * c",
        "snake_case_name",
        "2 ** 3",
        "`tick",
    ] {
        let got = one(raw);
        assert_eq!(shown(&got), raw, "{raw}");
        assert!(
            got.iter()
                .all(|r| r.text.is_empty() || (!r.bold && !r.code)),
            "{raw}"
        );
    }
}

#[test]
fn empty_whitespace_unicode_and_emoji_cells() {
    let src = "| a | b | c |\n| --- | --- | --- |\n|  |   | x |\n";
    assert!(cell(src, 1, 0).is_empty());
    assert!(cell(src, 1, 1).is_empty());
    assert_eq!(cell(src, 1, 2), vec![r("x", "")]);
    assert_eq!(
        one("**Příliš žluťoučký** kůň `ěšč`"),
        vec![r("Příliš žluťoučký", "b"), r(" kůň ", ""), r("ěšč", "c")]
    );
    assert_eq!(
        one("🦀 **🦀** `🦀` ~~日本語~~"),
        vec![
            r("🦀 ", ""),
            r("🦀", "b"),
            r(" ", ""),
            r("🦀", "c"),
            r(" ", ""),
            r("日本語", "s")
        ]
    );
}

#[test]
fn lf_and_crlf_give_the_same_runs() {
    let lf = "| **A** | `b` |\n| :-- | --: |\n| x \\| y | ~~z~~ |\n";
    let crlf = lf.replace('\n', "\r\n");
    for row in 0..2 {
        for col in 0..2 {
            assert_eq!(cell(lf, row, col), cell(&crlf, row, col), "{row},{col}");
        }
    }
    assert_eq!(cell(&crlf, 1, 0), vec![r("x | y", "")]);
}

#[test]
fn plain_cells_are_one_plain_run_exactly_as_before() {
    let src = "| Name | Role |\n| --- | --- |\n| Alice | Developer, QA |\n";
    let t = table_of(src);
    for row in std::iter::once(&t.header).chain(&t.rows) {
        for c in &row.cells {
            let got = runs(src, c, &t.inline);
            assert_eq!(got, vec![r(&table::cell_text(src, c), "")]);
        }
    }
}

#[test]
fn negative_control_without_styled_cells_the_checks_above_fail() {
    let styled = || one("**Fixed**") == vec![r("Fixed", "b")];
    assert!(styled());
    table::tests_off::set(true);
    let off = styled();
    let shown_off = shown(&one("**Fixed**"));
    table::tests_off::set(false);
    assert!(
        !off,
        "with styling disabled the bold-cell check does not pass"
    );
    assert_eq!(shown_off, "**Fixed**", "the markers show, as before T3");
}

// --- Geometry -------------------------------------------------------------------------------------

fn lay(ctx: &Context, src: &str, w: f32, bold: Option<&egui::FontFamily>) -> table::TableLayout {
    let t = table_of(src);
    let mut out = None;
    let mut o = ctx.run_ui(Default::default(), |ui| {
        out = Some(table::layout(ui.ctx(), src, &t, w, 16.0, bold));
    });
    o.textures_delta.clear();
    out.unwrap()
}

fn bold_family(ctx: &Context) -> Option<egui::FontFamily> {
    crate::fonts::install_bold(ctx)
}

#[test]
fn a_mixed_cell_is_one_flow_whose_height_stays_near_a_plain_cell() {
    let ctx = Context::default();
    let bold = bold_family(&ctx);
    let plain = "| H |\n| --- |\n| plain words only here |\n";
    let mixed = "| H |\n| --- |\n| plain **bold** `code` ~~gone~~ [link](https://x.org) |\n";
    let p = lay(&ctx, plain, 780.0, bold.as_ref());
    let m = lay(&ctx, mixed, 780.0, bold.as_ref());
    assert!(m.rows[1] >= p.rows[1] - 0.5, "{:?} vs {:?}", m.rows, p.rows);
    assert!(
        m.rows[1] <= p.rows[1] * 1.3,
        "one line of mixed faces is not much taller: {:?} vs {:?}",
        m.rows,
        p.rows
    );
    // Markers take no room: the styled column is narrower than the raw text would need.
    let raw = "| H |\n| --- |\n| plain \\*\\*bold\\*\\* \\`code\\` \\~\\~gone\\~\\~ \\[link\\]\\(https://x.org\\) |\n";
    let r = lay(&ctx, raw, 780.0, bold.as_ref());
    assert!(m.cols[0] < r.cols[0]);
}

#[test]
fn wrapped_styled_cells_grow_their_row_and_columns_keep_their_alignment_data() {
    let ctx = Context::default();
    let bold = bold_family(&ctx);
    let long = "very **long bold words** and `long code words` and ~~long gone words~~ ".repeat(8);
    let src = format!("| A | B |\n| :-- | --: |\n| {long} | short |\n");
    let one_line = lay(
        &ctx,
        "| A | B |\n| :-- | --: |\n| x | short |\n",
        780.0,
        bold.as_ref(),
    );
    for w in [700.0, 420.0, 300.0, 120.0] {
        let l = lay(&ctx, &src, w, bold.as_ref());
        assert!(l.width <= w.max(48.0) + 0.5, "{w}: {}", l.width);
        assert!(
            l.rows[1] > one_line.rows[1] * 2.0,
            "{w}: wraps: {:?}",
            l.rows
        );
        assert!(l.height.is_finite());
    }
    let t = table_of(&src);
    assert_eq!(
        t.align,
        vec![crate::markdown::Align::Left, crate::markdown::Align::Right]
    );
}

// --- The editor around it -------------------------------------------------------------------------

const DOC: &str = "Intro paragraph.\n\n| Style | Example |\n| :-- | --: |\n| bold | **Fixed** |\n| code | `a \\| b` |\n| link | [site](https://example.com) |\n| both | ***Both*** and ~~gone~~ |\n| czech | **Příliš** 🦀 |\n\nAfter the table.\n";

struct Th {
    ctx: Context,
    host: MultiHost,
    t: f64,
    opened: Vec<String>,
    shapes: Vec<egui::epaint::ClippedShape>,
    screen: Vec2,
    #[allow(dead_code)]
    dir: crate::test_dir::TestDir,
}

impl Th {
    fn new(notes: &[(&str, &str)]) -> Self {
        let dir = crate::test_dir::TestDir::new("t3-host");
        let store = Box::new(crate::attachments::FsStore::new(dir.clone()));
        let mut h = Th {
            ctx: Context::default(),
            host: MultiHost::new(1, notes, store),
            t: 0.0,
            opened: Vec::new(),
            shapes: Vec::new(),
            screen: Vec2::new(1000.0, 700.0),
            dir,
        };
        h.settle();
        h
    }

    fn frame(&mut self, events: Vec<Event>, m: Modifiers) {
        let mut events = events;
        events.insert(0, Event::ModifiersChanged(m));
        self.t += 0.05;
        let input = egui::RawInput {
            time: Some(self.t),
            events,
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.screen)),
            focused: true,
            ..Default::default()
        };
        let host = &mut self.host;
        let mut out = self.ctx.run_ui(input, |ui| host.ui(ui));
        for c in &out.platform_output.commands {
            if let egui::OutputCommand::OpenUrl(u) = c {
                self.opened.push(u.url.clone());
            }
        }
        out.textures_delta.clear();
        self.shapes = std::mem::take(&mut out.shapes);
    }

    fn settle(&mut self) {
        for _ in 0..3 {
            self.frame(vec![], Modifiers::NONE);
        }
    }

    fn text(&self) -> String {
        let k = self.host.selected.unwrap();
        self.host.text_of(k).unwrap().to_owned()
    }

    fn set_sel(&mut self, a: usize, b: usize) {
        use egui::text::{CCursor, CCursorRange};
        let k = self.host.selected.unwrap();
        let text = self.text();
        let id = k.widget_id();
        let mut st = egui::TextEdit::load_state(&self.ctx, id).unwrap_or_default();
        let c = |x: usize| CCursor::new(text[..x].chars().count());
        st.cursor
            .set_char_range(Some(CCursorRange::two(c(a), c(b))));
        st.store(&self.ctx, id);
        self.settle();
    }

    fn drawn(&self) -> usize {
        self.host.view.probe.tables.len()
    }

    fn key(&mut self, k: Key, m: Modifiers) {
        let e = Event::Key {
            key: k,
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

    /// Every galley painted this frame with its position.
    fn galleys(&self) -> Vec<(Arc<egui::Galley>, Pos2)> {
        fn walk(s: &egui::Shape, out: &mut Vec<(Arc<egui::Galley>, Pos2)>) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                egui::Shape::Text(t) => out.push((t.galley.clone(), t.pos)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        for s in &self.shapes {
            walk(&s.shape, &mut out);
        }
        out
    }

    /// The grid's cell galley whose display text is `text`.
    fn cell(&self, text: &str) -> Option<(Arc<egui::Galley>, Pos2)> {
        self.galleys().into_iter().find(|(g, _)| g.job.text == text)
    }

    fn click(&mut self, pos: Pos2) {
        self.frame(vec![Event::PointerMoved(pos)], Modifiers::NONE);
        let b = |pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        self.frame(vec![b(true)], Modifiers::NONE);
        self.frame(vec![b(false)], Modifiers::NONE);
        self.frame(vec![], Modifiers::NONE);
    }
}

fn in_table(src: &str) -> usize {
    src.find("**Fixed**").unwrap()
}

#[test]
fn outside_the_table_the_grid_shows_formatting_and_not_the_markers() {
    let mut h = Th::new(&[("n", DOC)]);
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1, "the grid is painted");
    for shown in ["Fixed", "a | b", "site", "Both and gone", "Příliš 🦀"] {
        assert!(h.cell(shown).is_some(), "{shown}");
    }
    let g = h.galleys();
    for (gal, _) in &g {
        if gal.job.text.contains('\n') {
            continue; // the document itself: its table rows are concealed
        }
        for marker in ["**", "~~", "](", "`"] {
            assert!(
                !gal.job.text.contains(marker),
                "{marker} shown in cell {:?}",
                gal.job.text
            );
        }
    }
    // Bold has its own run, code its own, the link its own.
    let (gal, _) = h.cell("Fixed").expect("bold cell");
    assert_eq!(gal.job.sections.len(), 1);
    let (gal, _) = h.cell("site").expect("link cell");
    assert!(gal.job.sections[0].format.underline.width > 0.0);
    assert_eq!(gal.job.sections[0].format.color, h.host.palette.link);
    let (gal, _) = h.cell("a | b").expect("code cell");
    assert_eq!(
        gal.job.sections[0].format.font_id.family,
        egui::FontFamily::Monospace
    );
    assert_eq!(gal.job.sections[0].format.color, h.host.palette.code_text);
    assert_eq!(
        gal.job.sections[0].format.background,
        h.host.palette.code_bg
    );
    // Bold and italic together are one run that is both.
    let (gal, _) = h.cell("Both and gone").expect("mixed cell");
    let fs: Vec<_> = gal
        .job
        .sections
        .iter()
        .map(|s| (s.format.italics, s.format.strikethrough.width > 0.0))
        .collect();
    assert_eq!(fs, vec![(true, false), (false, false), (false, true)]);
}

#[test]
fn the_source_bytes_never_change_across_rendering_and_caret_transitions() {
    let mut h = Th::new(&[("n", DOC)]);
    let inside = in_table(DOC);
    for sel in [0, inside, inside + 4, DOC.len(), 3, inside, DOC.len()] {
        h.set_sel(sel, sel);
        assert_eq!(h.text(), DOC, "caret at {sel}");
    }
    // A selection across the table reveals it, and still changes nothing.
    h.set_sel(2, DOC.len() - 2);
    assert_eq!(h.drawn(), 0);
    assert_eq!(h.text(), DOC);
}

#[test]
fn caret_inside_shows_raw_markdown_and_outside_the_grid_returns_at_once() {
    let mut h = Th::new(&[("n", DOC)]);
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1);
    h.set_sel(in_table(DOC), in_table(DOC));
    assert_eq!(h.drawn(), 0, "raw table while the caret is in it");
    // The raw table is in the document galley, markers included.
    let doc = h
        .galleys()
        .into_iter()
        .map(|(g, _)| g.job.text.clone())
        .find(|t| t.contains('\n'))
        .unwrap();
    assert!(doc.contains("**Fixed**") && doc.contains("~~gone~~"));
    h.set_sel(DOC.len(), DOC.len());
    assert_eq!(h.drawn(), 1, "back to the grid");
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1);
}

#[test]
fn clicking_a_rendered_table_enters_its_source_and_a_link_cell_opens_nothing() {
    let src = "Intro.\n\n| a | b |\n| --- | --- |\n| [bad](javascript:alert(1)) | [ok](https://example.com) |\n\nEnd.\n";
    assert!(crate::links::safe_http_url("javascript:alert(1)").is_none());
    let mut h = Th::new(&[("n", src)]);
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1);
    for cell in ["bad", "ok"] {
        let (g, p) = h.cell(cell).expect(cell);
        h.click(p + Vec2::new(4.0, g.size().y / 2.0));
        assert!(
            h.opened.is_empty(),
            "a click on a table link opens nothing: {:?}",
            h.opened
        );
        assert_eq!(h.text(), src);
        assert_eq!(
            h.drawn(),
            0,
            "the click put the caret in the table: raw source"
        );
        h.set_sel(0, 0);
    }
}

#[test]
fn editing_the_raw_table_then_undo_and_redo_restore_the_exact_bytes() {
    let mut h = Th::new(&[("n", DOC)]);
    let at = DOC.find("Fixed").unwrap() + 5;
    h.set_sel(at, at);
    h.type_text("X");
    let edited = h.text();
    assert_eq!(edited, DOC.replacen("Fixed", "FixedX", 1));
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1, "the grid is back, with the edit");
    assert!(h.cell("FixedX").is_some());
    assert_eq!(h.text(), edited, "rendering changed nothing");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.text(), DOC, "undo");
    h.key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT);
    assert_eq!(h.text(), edited, "redo");
    h.key(Key::Z, Modifiers::COMMAND);
    assert_eq!(h.text(), DOC);
}

#[test]
fn switching_notes_while_a_table_is_raw_keeps_both_notes_intact() {
    let mut h = Th::new(&[("n", DOC), ("m", "other note\n")]);
    h.set_sel(in_table(DOC), in_table(DOC));
    assert_eq!(h.drawn(), 0);
    h.key(Key::ArrowDown, Modifiers::ALT);
    assert_eq!(h.text(), "other note\n");
    h.key(Key::ArrowUp, Modifiers::ALT);
    assert_eq!(h.text(), DOC);
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1);
    assert_eq!(h.text(), DOC);
}

#[test]
fn crlf_documents_render_and_keep_their_bytes() {
    let crlf = DOC.replace('\n', "\r\n");
    let mut h = Th::new(&[("n", &crlf)]);
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1);
    assert!(h.cell("Fixed").is_some());
    assert_eq!(h.text(), crlf);
    h.set_sel(crlf.find("Fixed").unwrap(), crlf.find("Fixed").unwrap());
    assert_eq!(h.text(), crlf);
}

#[test]
fn light_and_dark_palettes_colour_the_styled_runs_from_the_palette() {
    let dark = Palette::from_toml("mode = \"dark\"\nbackground = \"#1a1b26\"\nforeground = \"#c0caf5\"\naccent = \"#7aa2f7\"\n").unwrap();
    let light = Palette::from_toml("mode = \"light\"\nbackground = \"#f5f3ee\"\nforeground = \"#2b2b33\"\naccent = \"#2e5cb8\"\n").unwrap();
    assert_ne!(dark.link, light.link);
    let mut h = Th::new(&[("n", DOC)]);
    for p in [dark, light] {
        h.host.palette = p;
        h.set_sel(0, 0);
        h.frame(vec![], Modifiers::NONE);
        let (g, _) = h.cell("site").unwrap();
        assert_eq!(g.job.sections[0].format.color, p.link);
        let (g, _) = h.cell("a | b").unwrap();
        assert_eq!(g.job.sections[0].format.color, p.code_text);
        assert_eq!(g.job.sections[0].format.background, p.code_bg);
        let (g, _) = h.cell("Both and gone").unwrap();
        let strike = g.job.sections[2].format.strikethrough;
        assert_eq!(strike.color, p.text);
    }
}

#[test]
fn narrow_windows_keep_every_cell_inside_the_table_and_the_window() {
    for w in [900.0f32, 520.0, 420.0] {
        let mut h = Th::new(&[("n", DOC)]);
        h.screen = Vec2::new(w, 700.0);
        h.settle();
        h.set_sel(0, 0);
        assert_eq!(h.drawn(), 1, "{w}");
        for (g, p) in h.galleys() {
            if g.job.text.contains('\n') || g.job.text.is_empty() {
                continue;
            }
            assert!(p.x >= 0.0, "{w}: {:?}", g.job.text);
            // A cell's text never starts past the right edge of the window.
            assert!(p.x < w, "{w}: {:?} at {}", g.job.text, p.x);
        }
    }
}

#[test]
fn alignment_columns_still_place_their_text() {
    let mut h = Th::new(&[("n", DOC)]);
    h.set_sel(0, 0);
    let (bold_col, _) = (h.cell("bold").unwrap(), ());
    let right = h.cell("Fixed").unwrap();
    let left = bold_col;
    assert!(right.1.x > left.1.x + 100.0, "right column to the right");
    // `--:` right-aligns: the cell text ends at the same x in every row of that column.
    let ends: Vec<f32> = ["Fixed", "a | b", "site"]
        .iter()
        .map(|t| {
            let (g, p) = h.cell(t).unwrap();
            p.x + g.size().x
        })
        .collect();
    assert!(
        (ends[0] - ends[1]).abs() < 0.6 && (ends[1] - ends[2]).abs() < 0.6,
        "{ends:?}"
    );
}

#[test]
fn a_document_of_plain_tables_is_laid_out_exactly_as_before() {
    // Plain cells: one section each, the body face, no explicit line box: the shape of the
    // pre-T3 jobs, so plain tables measure and paint as they always did.
    let doc = "x\n\n| A | B |\n| --- | :---: |\n| one | two words |\n\ny\n";
    let mut h = Th::new(&[("n", doc)]);
    h.set_sel(0, 0);
    assert_eq!(h.drawn(), 1);
    for t in ["one", "two words"] {
        let (g, _) = h.cell(t).unwrap();
        assert_eq!(g.job.sections.len(), 1);
        let f = &g.job.sections[0].format;
        assert_eq!(f.line_height, None);
        assert_eq!(f.font_id.family, egui::FontFamily::Proportional);
        assert!(!f.italics && f.strikethrough.width == 0.0 && f.underline.width == 0.0);
    }
}

// --- Cost -----------------------------------------------------------------------------------------

fn styled_doc(tables: usize, rows: usize) -> String {
    let mut s = String::from("Intro\n\n");
    for t in 0..tables {
        s.push_str("| Name | Notes | Code |\n| --- | :---: | ---: |\n");
        for r in 0..rows {
            s.push_str(&format!(
                "| **row {t}.{r}** | some *longer* text with [a link](https://x.org/{r}) | `code {r}` ~~old~~ |\n"
            ));
        }
        s.push_str("\nparagraph\n\n");
    }
    s
}

fn wrapped_doc(styled: bool) -> String {
    let mut s = String::from("Intro\n\n| A | B |\n| --- | --- |\n");
    for r in 0..30 {
        let cellt = if styled {
            "very **long bold words** and `long code words` and ~~long gone words~~ ".repeat(6)
        } else {
            "very long bold words and long code words and long gone words ".repeat(6)
        };
        s.push_str(&format!("| {cellt} | {r} |\n"));
    }
    s.push_str("\nend\n");
    s
}

fn plain_doc(tables: usize, rows: usize) -> String {
    let mut s = String::from("Intro\n\n");
    for t in 0..tables {
        s.push_str("| Name | Notes | Code |\n| --- | :---: | ---: |\n");
        for r in 0..rows {
            s.push_str(&format!(
                "| row {t}.{r} | some longer text with a link | code {r} old |\n"
            ));
        }
        s.push_str("\nparagraph\n\n");
    }
    s
}

/// A record, not an assertion of speed: frames of a styled and a plain document with the same
/// shape, and typing in a paragraph beside them (debug build; see the T3 record for release).
#[test]
fn styled_cells_cost_is_recorded() {
    for (name, styled, plain) in [
        ("small", styled_doc(1, 4), plain_doc(1, 4)),
        ("200 rows", styled_doc(1, 200), plain_doc(1, 200)),
        ("40 tables", styled_doc(40, 6), plain_doc(40, 6)),
        ("long wrapped cells", wrapped_doc(true), wrapped_doc(false)),
    ] {
        let mut ms = Vec::new();
        for src in [&plain, &styled] {
            let cold = std::time::Instant::now();
            let mut h = Th::new(&[("n", src.as_str())]);
            h.set_sel(0, 0);
            let cold = cold.elapsed().as_secs_f64() * 1000.0;
            let t0 = std::time::Instant::now();
            for _ in 0..10 {
                h.frame(vec![], Modifiers::NONE);
            }
            let steady = t0.elapsed().as_secs_f64() * 100.0;
            let t0 = std::time::Instant::now();
            for _ in 0..10 {
                h.type_text("x");
            }
            let typing = t0.elapsed().as_secs_f64() * 50.0;
            ms.push((cold, steady, typing));
        }
        eprintln!(
            "T3 cost {name}: open (cold) plain {:.1} / styled {:.1} ms; frame plain {:.2} / styled {:.2} ms; typing frame plain {:.2} / styled {:.2} ms",
            ms[0].0, ms[1].0, ms[0].1, ms[1].1, ms[0].2, ms[1].2
        );
    }
}

#[test]
fn the_raw_table_shows_every_delimiter_and_the_rendered_one_hides_them_in_its_source() {
    let mut h = Th::new(&[("n", DOC)]);
    let marker_colours = |h: &Th| -> Vec<egui::Color32> {
        let (g, _) = h
            .galleys()
            .into_iter()
            .find(|(g, _)| g.job.text.contains('\n'))
            .unwrap();
        g.job
            .sections
            .iter()
            .filter(|s| {
                matches!(
                    &g.job.text[s.byte_range.start.0..s.byte_range.end.0],
                    "**" | "~~" | "`"
                )
            })
            .map(|s| s.format.color)
            .collect()
    };
    // Caret inside the table: all of the table's delimiters are on show, in the reveal colour.
    h.set_sel(in_table(DOC), in_table(DOC));
    let raw = marker_colours(&h);
    assert!(raw.len() >= 8, "{raw:?}");
    assert!(raw.iter().all(|c| *c == h.host.palette.reveal), "{raw:?}");
    // Caret outside: the (concealed) source of the table has none of them visible.
    h.set_sel(0, 0);
    assert!(marker_colours(&h)
        .iter()
        .all(|c| *c == egui::Color32::TRANSPARENT));
    assert_eq!(h.text(), DOC);
}
