//! PDF export: Markdown + attachments → a real, text-based PDF (A4 portrait).
//!
//! Pipeline, with no egui and no editor state:
//! 1. `parse` turns the Markdown (pulldown-cmark, the same options as the editor) into a short-lived
//!    block model used only for layout. The Markdown string stays the one document.
//! 2. `Lay` places blocks on A4 pages (own greedy line breaking and pagination; text widths come from
//!    the same shaper the PDF writer uses, so lines are measured exactly as they are drawn) and
//!    produces a display list of text runs, rules, rectangles, images and link areas per page.
//! 3. `render` writes that list with [krilla](https://crates.io/crates/krilla): fonts are subset and
//!    embedded with a ToUnicode map (selectable, searchable text), PNG/JPEG data is embedded, links
//!    are real URI annotations.
//!
//! Images come from the same plan as the ZIP export (`export::plan`), i.e. through `AttachmentStore`;
//! a missing or invalid local image fails the export with the reference named. `http(s)` images are
//! never fetched: the alt text is printed (as a link).

use crate::attachments::{self, AttachmentStore};
use crate::export::{self, ExportError};
use crate::links;
use pulldown_cmark::{Alignment, Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::path::Path;

pub const PAGE_W: f32 = 595.28;
pub const PAGE_H: f32 = 841.89;
pub const MARGIN_X: f32 = 56.7;
pub const MARGIN_TOP: f32 = 62.0;
pub const MARGIN_BOTTOM: f32 = 66.0;
const BODY: f32 = 10.5;
const HEADING: [f32; 6] = [22.0, 18.0, 15.0, 13.0, 11.5, 11.0];
const CODE: f32 = 9.0;

// ---------------------------------------------------------------- block model

#[derive(Clone, Debug, Default, PartialEq)]
struct Style {
    bold: bool,
    italic: bool,
    strike: bool,
    code: bool,
    link: Option<String>,
}

#[derive(Clone, Debug)]
struct Span {
    text: String,
    style: Style,
    /// Set for an image inside running text (the text is its alt text).
    image: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

type Cell = Vec<Span>;

#[derive(Debug)]
enum Block {
    Heading(u8, Vec<Span>),
    Para(Vec<Span>),
    Code(String),
    Rule,
    Quote(Vec<Block>),
    List {
        start: Option<u64>,
        items: Vec<Item>,
    },
    Table {
        aligns: Vec<Align>,
        head: Vec<Cell>,
        rows: Vec<Vec<Cell>>,
    },
    Image {
        url: String,
        alt: String,
        width: Option<u16>,
    },
}

#[derive(Debug)]
struct Item {
    checked: Option<bool>,
    blocks: Vec<Block>,
}

enum Container {
    Root(Vec<Block>),
    Quote(Vec<Block>),
    Item(Item),
    List(Option<u64>, Vec<Item>),
}

fn parse_width(s: &str) -> Option<u16> {
    let d = s.trim().strip_prefix("{width=")?.strip_suffix("%}")?;
    ((1..=3).contains(&d.len()) && d.bytes().all(|b| b.is_ascii_digit()))
        .then(|| d.parse().ok())
        .flatten()
}

/// A paragraph holding only an image (and maybe `{width=NN%}`) is an image block; any other image
/// in running text becomes `[alt]` (linked when it is an `http(s)` URL).
fn paragraph(spans: Vec<Span>) -> Block {
    let images: Vec<_> = spans.iter().filter(|s| s.image.is_some()).collect();
    if images.len() == 1 {
        let rest: String = spans
            .iter()
            .filter(|s| s.image.is_none())
            .map(|s| s.text.as_str())
            .collect();
        let rest = rest.trim();
        if rest.is_empty() || parse_width(rest).is_some() {
            let i = images[0];
            return Block::Image {
                url: i.image.clone().unwrap_or_default(),
                alt: i.text.clone(),
                width: parse_width(rest),
            };
        }
    }
    Block::Para(
        spans
            .into_iter()
            .map(|mut s| {
                if let Some(url) = s.image.take() {
                    s.text = format!("[{}]", s.text);
                    s.style.link = links::safe_http_url(&url).map(str::to_owned);
                }
                s
            })
            .collect(),
    )
}

fn parse(markdown: &str) -> Vec<Block> {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut stack = vec![Container::Root(Vec::new())];
    let mut spans: Vec<Span> = Vec::new();
    let (mut bold, mut italic, mut strike) = (0usize, 0usize, 0usize);
    let mut link: Vec<Option<String>> = Vec::new();
    let mut code: Option<String> = None;
    let mut heading: Option<u8> = None;
    let mut image: Option<(String, String)> = None;
    struct T {
        aligns: Vec<Align>,
        head: Vec<Cell>,
        rows: Vec<Vec<Cell>>,
        row: Vec<Cell>,
    }
    let mut table: Option<T> = None;

    fn push(stack: &mut [Container], b: Block) {
        match stack.last_mut() {
            Some(Container::Root(v) | Container::Quote(v)) => v.push(b),
            Some(Container::Item(i)) => i.blocks.push(b),
            _ => {}
        }
    }
    macro_rules! flush {
        () => {
            if !spans.is_empty() {
                let s = std::mem::take(&mut spans);
                if s.iter()
                    .any(|x| !x.text.trim().is_empty() || x.image.is_some())
                {
                    push(&mut stack, paragraph(s));
                }
            }
        };
    }

    for ev in Parser::new_ext(markdown, opts) {
        let style = |code_run: bool| Style {
            bold: bold > 0,
            italic: italic > 0,
            strike: strike > 0,
            code: code_run,
            link: link.iter().rev().find_map(Clone::clone),
        };
        match ev {
            Event::Start(Tag::Paragraph) | Event::End(TagEnd::Paragraph) => {
                if matches!(ev, Event::End(_)) {
                    flush!();
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                flush!();
                heading = Some(level as u8);
            }
            Event::End(TagEnd::Heading(_)) => {
                let s = std::mem::take(&mut spans);
                push(&mut stack, Block::Heading(heading.take().unwrap_or(1), s));
            }
            Event::Start(Tag::BlockQuote(_)) => {
                flush!();
                stack.push(Container::Quote(Vec::new()));
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                flush!();
                if let Some(Container::Quote(v)) = stack.pop() {
                    push(&mut stack, Block::Quote(v));
                }
            }
            Event::Start(Tag::List(start)) => {
                flush!();
                stack.push(Container::List(start, Vec::new()));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some(Container::List(start, items)) = stack.pop() {
                    push(&mut stack, Block::List { start, items });
                }
            }
            Event::Start(Tag::Item) => {
                flush!();
                stack.push(Container::Item(Item {
                    checked: None,
                    blocks: Vec::new(),
                }));
            }
            Event::End(TagEnd::Item) => {
                flush!();
                if let Some(Container::Item(i)) = stack.pop() {
                    if let Some(Container::List(_, items)) = stack.last_mut() {
                        items.push(i);
                    }
                }
            }
            Event::TaskListMarker(c) => {
                if let Some(Container::Item(i)) = stack.last_mut() {
                    i.checked = Some(c);
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                flush!();
                code = Some(String::new());
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some(c) = code.take() {
                    push(&mut stack, Block::Code(c));
                }
            }
            Event::Rule => {
                flush!();
                push(&mut stack, Block::Rule);
            }
            Event::Start(Tag::HtmlBlock) => flush!(),
            Event::End(TagEnd::HtmlBlock) => flush!(),
            Event::Start(Tag::Table(al)) => {
                flush!();
                table = Some(T {
                    aligns: al
                        .iter()
                        .map(|a| match a {
                            Alignment::Center => Align::Center,
                            Alignment::Right => Align::Right,
                            _ => Align::Left,
                        })
                        .collect(),
                    head: vec![],
                    rows: vec![],
                    row: vec![],
                });
            }
            Event::Start(Tag::TableRow) => {
                if let Some(t) = table.as_mut() {
                    t.row.clear();
                }
            }
            Event::Start(Tag::TableCell) => spans.clear(),
            Event::End(TagEnd::TableCell) => {
                let c = std::mem::take(&mut spans);
                if let Some(t) = table.as_mut() {
                    t.row.push(c);
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let Some(t) = table.as_mut() {
                    t.head = std::mem::take(&mut t.row);
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let Some(t) = table.as_mut() {
                    let r = std::mem::take(&mut t.row);
                    t.rows.push(r);
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some(t) = table.take() {
                    push(
                        &mut stack,
                        Block::Table {
                            aligns: t.aligns,
                            head: t.head,
                            rows: t.rows,
                        },
                    );
                }
            }
            Event::Start(Tag::Strong) => bold += 1,
            Event::End(TagEnd::Strong) => bold = bold.saturating_sub(1),
            Event::Start(Tag::Emphasis) => italic += 1,
            Event::End(TagEnd::Emphasis) => italic = italic.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => strike += 1,
            Event::End(TagEnd::Strikethrough) => strike = strike.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => {
                link.push(links::safe_http_url(&dest_url).map(str::to_owned));
            }
            Event::End(TagEnd::Link) => {
                link.pop();
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((dest_url.to_string(), String::new()))
            }
            Event::End(TagEnd::Image) => {
                if let Some((url, alt)) = image.take() {
                    spans.push(Span {
                        text: alt,
                        style: style(false),
                        image: Some(url),
                    });
                }
            }
            Event::Text(t) => {
                if let Some(c) = code.as_mut() {
                    c.push_str(&t);
                } else if let Some((_, alt)) = image.as_mut() {
                    alt.push_str(&t);
                } else {
                    spans.push(Span {
                        text: t.to_string(),
                        style: style(false),
                        image: None,
                    });
                }
            }
            Event::Code(t) => spans.push(Span {
                text: t.to_string(),
                style: style(true),
                image: None,
            }),
            // Raw HTML is not rendered; it is shown as text so nothing is lost.
            Event::Html(t) | Event::InlineHtml(t) => {
                if let Some(c) = code.as_mut() {
                    c.push_str(&t);
                } else {
                    spans.push(Span {
                        text: t.to_string(),
                        style: style(true),
                        image: None,
                    });
                }
            }
            Event::SoftBreak => spans.push(Span {
                text: " ".into(),
                style: style(false),
                image: None,
            }),
            Event::HardBreak => spans.push(Span {
                text: "\n".into(),
                style: style(false),
                image: None,
            }),
            _ => {}
        }
    }
    flush!();
    match stack.pop() {
        Some(Container::Root(v)) => v,
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------- fonts

const SANS: [&[u8]; 4] = [
    include_bytes!("../assets/fonts/LiberationSans-Regular.ttf"),
    include_bytes!("../assets/fonts/LiberationSans-Bold.ttf"),
    include_bytes!("../assets/fonts/LiberationSans-Italic.ttf"),
    include_bytes!("../assets/fonts/LiberationSans-BoldItalic.ttf"),
];
const MONO: &[u8] = include_bytes!("../assets/fonts/LiberationMono-Regular.ttf");
const MONO_ID: usize = 4;
const EMOJI_ID: usize = 5;

struct FontSet {
    faces: Vec<rustybuzz::Face<'static>>,
    kfonts: Vec<krilla::text::Font>,
}

/// A system emoji font, if one is installed (the usual Linux/macOS/Windows locations).
fn emoji_font_bytes() -> Option<&'static [u8]> {
    static BYTES: std::sync::OnceLock<Option<&'static [u8]>> = std::sync::OnceLock::new();
    *BYTES.get_or_init(|| {
        crate::fonts::EMOJI_FONTS
            .iter()
            .find_map(|p| std::fs::read(p).ok())
            .map(|v| &*Box::leak(v.into_boxed_slice()))
    })
}

impl FontSet {
    fn new() -> Result<Self, ExportError> {
        let mut all: Vec<&'static [u8]> = SANS.to_vec();
        all.push(MONO);
        if let Some(e) = emoji_font_bytes() {
            all.push(e);
        }
        let mut faces = Vec::new();
        let mut kfonts = Vec::new();
        for (i, b) in all.into_iter().enumerate() {
            let face = rustybuzz::Face::from_slice(b, 0);
            let font = krilla::text::Font::new(b.into(), 0);
            match (face, font) {
                (Some(f), Some(k)) => {
                    faces.push(f);
                    kfonts.push(k);
                }
                _ if i >= EMOJI_ID => {} // an unusable emoji font is simply not used
                _ => return Err(ExportError::Pdf("cannot load the built-in fonts".into())),
            }
        }
        Ok(Self { faces, kfonts })
    }

    fn has_emoji(&self) -> bool {
        self.faces.len() > EMOJI_ID
    }

    fn primary(&self, st: &Style) -> usize {
        if st.code {
            MONO_ID
        } else {
            usize::from(st.bold) + 2 * usize::from(st.italic)
        }
    }

    /// The font used for `c`: the style's face, else the emoji font, else the face itself (its
    /// `.notdef` box shows that the character is unsupported).
    fn font_for(&self, st: &Style, c: char) -> usize {
        let p = self.primary(st);
        if self.faces[p].glyph_index(c).is_some() {
            p
        } else if self.has_emoji() && self.faces[EMOJI_ID].glyph_index(c).is_some() {
            EMOJI_ID
        } else {
            p
        }
    }

    /// Consecutive characters of `text` that use the same font.
    fn runs(&self, text: &str, st: &Style) -> Vec<(usize, String)> {
        let mut out: Vec<(usize, String)> = Vec::new();
        for c in text.chars() {
            let f = self.font_for(st, c);
            match out.last_mut() {
                Some((lf, s)) if *lf == f => s.push(c),
                _ => out.push((f, c.to_string())),
            }
        }
        out
    }

    fn width(&self, font: usize, text: &str, size: f32) -> f32 {
        let face = &self.faces[font];
        let mut buf = rustybuzz::UnicodeBuffer::new();
        buf.push_str(text);
        buf.guess_segment_properties();
        let out = rustybuzz::shape(face, &[], buf);
        let adv: f32 = out
            .glyph_positions()
            .iter()
            .map(|p| p.x_advance as f32)
            .sum();
        adv / f32::from(face.units_per_em().max(1) as u16) * size
    }

    fn text_width(&self, text: &str, st: &Style, size: f32) -> f32 {
        self.runs(text, st)
            .iter()
            .map(|(f, s)| self.width(*f, s, size))
            .sum()
    }

    fn ascent(&self, font: usize) -> f32 {
        let f = &self.faces[font];
        f.ascender() as f32 / f.units_per_em().max(1) as f32
    }

    fn descent(&self, font: usize) -> f32 {
        let f = &self.faces[font];
        -(f.descender() as f32) / f.units_per_em().max(1) as f32
    }
}

// --------------------------------------------------------------- display list

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

const INK: Rgb = Rgb(0x1f, 0x23, 0x28);
const MUTED: Rgb = Rgb(0x59, 0x60, 0x69);
const LINK: Rgb = Rgb(0x0b, 0x57, 0xd0);
const RULE: Rgb = Rgb(0xc9, 0xcd, 0xd2);
const CODE_BG: Rgb = Rgb(0xf0, 0xf2, 0xf4);
const HEAD_BG: Rgb = Rgb(0xee, 0xf0, 0xf3);
const WHITE: Rgb = Rgb(255, 255, 255);

#[derive(Clone, Debug)]
pub enum Op {
    Text {
        x: f32,
        y: f32,
        font: usize,
        size: f32,
        color: Rgb,
        text: String,
    },
    Rect {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        fill: Option<Rgb>,
        stroke: Option<(Rgb, f32)>,
    },
    Line {
        x1: f32,
        y1: f32,
        x2: f32,
        y2: f32,
        color: Rgb,
        width: f32,
    },
    Image {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        id: usize,
    },
    Link {
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        url: String,
    },
}

/// The laid-out document: one display list per page.
pub struct Layout {
    pub pages: Vec<Vec<Op>>,
    images: Vec<krilla::image::Image>,
    fonts: FontSet,
}

#[derive(Clone)]
struct Frag {
    text: String,
    style: Style,
}

#[derive(Default, Clone)]
struct Line {
    frags: Vec<Frag>,
    width: f32,
}

struct Tok {
    frags: Vec<Frag>,
    space_before: Option<Style>,
}

enum Item2 {
    Tok(Tok),
    Break,
}

fn tokens(spans: &[Span], bold: bool) -> Vec<Item2> {
    let mut out = Vec::new();
    let mut cur: Option<Tok> = None;
    let mut space: Option<Style> = None;
    for sp in spans {
        let mut st = sp.style.clone();
        st.bold |= bold;
        for c in sp.text.chars() {
            if matches!(c, '\u{FE0F}' | '\u{FE0E}' | '\u{200D}' | '\u{200B}' | '\r') {
                continue;
            }
            if c == '\n' {
                if let Some(t) = cur.take() {
                    out.push(Item2::Tok(t));
                }
                out.push(Item2::Break);
                space = None;
            } else if c.is_whitespace() {
                if let Some(t) = cur.take() {
                    out.push(Item2::Tok(t));
                }
                space = Some(st.clone());
            } else {
                let t = cur.get_or_insert_with(|| Tok {
                    frags: Vec::new(),
                    space_before: space.take(),
                });
                match t.frags.last_mut() {
                    Some(f) if f.style == st => f.text.push(c),
                    _ => t.frags.push(Frag {
                        text: c.to_string(),
                        style: st.clone(),
                    }),
                }
            }
        }
    }
    if let Some(t) = cur {
        out.push(Item2::Tok(t));
    }
    out
}

struct Lay<'a> {
    fonts: FontSet,
    plan: &'a export::Plan,
    pages: Vec<Vec<Op>>,
    y: f32,
    color: Rgb,
    /// Page extents (top, bottom) of content placed since a tracker was pushed (for quote bars).
    trackers: Vec<HashMap<usize, (f32, f32)>>,
    images: Vec<krilla::image::Image>,
    image_ids: HashMap<String, usize>,
    in_item: usize,
}

fn top() -> f32 {
    MARGIN_TOP
}

fn bottom() -> f32 {
    PAGE_H - MARGIN_BOTTOM
}

impl<'a> Lay<'a> {
    fn page(&self) -> usize {
        self.pages.len() - 1
    }

    fn new_page(&mut self) {
        self.pages.push(Vec::new());
        self.y = top();
    }

    /// Start a new page unless `h` fits below the cursor (or the cursor is already at a page top).
    fn need(&mut self, h: f32) {
        if self.y + h > bottom() + 0.01 && self.y > top() + 0.5 {
            self.new_page();
        }
    }

    fn emit(&mut self, op: Op) {
        let p = self.page();
        self.pages[p].push(op);
    }

    fn mark(&mut self, t: f32, b: f32) {
        let p = self.page();
        for tr in &mut self.trackers {
            let e = tr.entry(p).or_insert((t, b));
            e.0 = e.0.min(t);
            e.1 = e.1.max(b);
        }
    }

    fn space_width(&self, st: &Style, size: f32) -> f32 {
        self.fonts.text_width(" ", st, size)
    }

    fn frag_width(&self, f: &Frag, size: f32) -> f32 {
        self.fonts.text_width(&f.text, &f.style, size)
    }

    /// Greedy line breaking within `width`; words longer than a line are split by characters.
    fn wrap(&self, spans: &[Span], size: f32, width: f32, bold: bool) -> Vec<Line> {
        let width = width.max(size);
        let mut lines: Vec<Line> = Vec::new();
        let mut cur = Line::default();
        for it in tokens(spans, bold) {
            let tok = match it {
                Item2::Break => {
                    lines.push(std::mem::take(&mut cur));
                    continue;
                }
                Item2::Tok(t) => t,
            };
            let w: f32 = tok.frags.iter().map(|f| self.frag_width(f, size)).sum();
            let sp = match (&tok.space_before, cur.frags.is_empty()) {
                (Some(st), false) => self.space_width(st, size),
                _ => 0.0,
            };
            if cur.width + sp + w <= width + 0.01 {
                if sp > 0.0 {
                    if let Some(st) = &tok.space_before {
                        cur.frags.push(Frag {
                            text: " ".into(),
                            style: st.clone(),
                        });
                    }
                }
                cur.width += sp + w;
                cur.frags.extend(tok.frags);
                continue;
            }
            if !cur.frags.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            if w <= width {
                cur.width = w;
                cur.frags = tok.frags;
            } else {
                // Very long word: break it anywhere.
                for f in tok.frags {
                    for c in f.text.chars() {
                        let cw = self.fonts.text_width(&c.to_string(), &f.style, size);
                        if cur.width + cw > width && !cur.frags.is_empty() {
                            lines.push(std::mem::take(&mut cur));
                        }
                        match cur.frags.last_mut() {
                            Some(l) if l.style == f.style => l.text.push(c),
                            _ => cur.frags.push(Frag {
                                text: c.to_string(),
                                style: f.style.clone(),
                            }),
                        }
                        cur.width += cw;
                    }
                }
            }
        }
        if !cur.frags.is_empty() {
            lines.push(cur);
        }
        lines
    }

    /// Draw one line whose box starts at `top`.
    #[allow(clippy::too_many_arguments)]
    fn place_line(
        &mut self,
        line: &Line,
        x: f32,
        w: f32,
        top: f32,
        size: f32,
        lh: f32,
        align: Align,
        color: Rgb,
    ) {
        let off = match align {
            Align::Left => 0.0,
            Align::Center => ((w - line.width) / 2.0).max(0.0),
            Align::Right => (w - line.width).max(0.0),
        };
        let mut cx = x + off;
        // Merge neighbouring fragments of the same style into one text run.
        let mut pieces: Vec<(String, Style)> = Vec::new();
        for f in &line.frags {
            match pieces.last_mut() {
                Some((t, s)) if *s == f.style => t.push_str(&f.text),
                _ => pieces.push((f.text.clone(), f.style.clone())),
            }
        }
        for (text, st) in pieces {
            let pw = self.fonts.text_width(&text, &st, size);
            let main = self.fonts.primary(&st);
            let asc = self.fonts.ascent(main);
            let desc = self.fonts.descent(main);
            let baseline = top + (lh - (asc + desc) * size) / 2.0 + asc * size;
            if st.code {
                self.emit(Op::Rect {
                    x: cx - 1.0,
                    y: baseline - asc * size - 0.5,
                    w: pw + 2.0,
                    h: (asc + desc) * size + 1.0,
                    fill: Some(CODE_BG),
                    stroke: None,
                });
            }
            let col = if st.link.is_some() { LINK } else { color };
            let mut rx = cx;
            for (font, run) in self.fonts.runs(&text, &st) {
                let rw = self.fonts.width(font, &run, size);
                self.emit(Op::Text {
                    x: rx,
                    y: baseline,
                    font,
                    size,
                    color: col,
                    text: run,
                });
                rx += rw;
            }
            if st.strike {
                self.emit(Op::Line {
                    x1: cx,
                    y1: baseline - size * 0.3,
                    x2: cx + pw,
                    y2: baseline - size * 0.3,
                    color: col,
                    width: 0.7,
                });
            }
            if let Some(url) = &st.link {
                self.emit(Op::Line {
                    x1: cx,
                    y1: baseline + size * 0.13,
                    x2: cx + pw,
                    y2: baseline + size * 0.13,
                    color: LINK,
                    width: 0.6,
                });
                self.emit(Op::Link {
                    x: cx,
                    y: top,
                    w: pw,
                    h: lh,
                    url: url.clone(),
                });
            }
            cx += pw;
        }
        self.mark(top, top + lh);
    }

    fn paragraph(&mut self, spans: &[Span], x: f32, w: f32, size: f32, bold: bool, gap: f32) {
        let lh = size * 1.4;
        let lines = self.wrap(spans, size, w, bold);
        for l in &lines {
            self.need(lh);
            let y = self.y;
            self.place_line(l, x, w, y, size, lh, Align::Left, self.color);
            self.y += lh;
        }
        if !lines.is_empty() {
            self.y += gap;
        }
    }

    fn blocks(&mut self, blocks: &[Block], x: f32, w: f32) -> Result<(), ExportError> {
        for b in blocks {
            self.block(b, x, w)?;
        }
        Ok(())
    }

    fn block(&mut self, b: &Block, x: f32, w: f32) -> Result<(), ExportError> {
        let tight = if self.in_item > 0 { 2.0 } else { 6.0 };
        match b {
            Block::Para(s) => self.paragraph(s, x, w, BODY, false, tight),
            Block::Heading(l, s) => {
                let size = HEADING[usize::from((*l).clamp(1, 6)) - 1];
                let lh = size * 1.3;
                let lines = self.wrap(s, size, w, true);
                if self.y > top() + 0.5 {
                    self.y += size * 0.6;
                }
                // Keep the heading with at least two lines of what follows.
                self.need(lines.len() as f32 * lh + BODY * 2.8);
                for ln in &lines {
                    self.need(lh);
                    let y = self.y;
                    self.place_line(ln, x, w, y, size, lh, Align::Left, INK);
                    self.y += lh;
                }
                if *l <= 2 {
                    let y = self.y + 1.0;
                    self.emit(Op::Line {
                        x1: x,
                        y1: y,
                        x2: x + w,
                        y2: y,
                        color: RULE,
                        width: 0.6,
                    });
                    self.y += 3.0;
                }
                self.y += 4.0;
            }
            Block::Code(c) => self.code(c, x, w),
            Block::Rule => {
                self.need(14.0);
                let y = self.y + 7.0;
                self.emit(Op::Line {
                    x1: x,
                    y1: y,
                    x2: x + w,
                    y2: y,
                    color: RULE,
                    width: 0.8,
                });
                self.mark(self.y, self.y + 14.0);
                self.y += 14.0 + 2.0;
            }
            Block::Quote(inner) => {
                self.trackers.push(HashMap::new());
                let old = self.color;
                self.color = MUTED;
                let r = self.blocks(inner, x + 14.0, w - 14.0);
                self.color = old;
                let tr = self.trackers.pop().unwrap_or_default();
                r?;
                for (p, (t, bt)) in tr {
                    self.pages[p].push(Op::Line {
                        x1: x + 3.0,
                        y1: t,
                        x2: x + 3.0,
                        y2: bt,
                        color: RULE,
                        width: 2.5,
                    });
                }
            }
            Block::List { start, items } => {
                for (i, it) in items.iter().enumerate() {
                    self.list_item(*start, i, it, x, w)?;
                }
                if self.in_item == 0 {
                    self.y += 3.0;
                }
            }
            Block::Table { aligns, head, rows } => self.table(aligns, head, rows, x, w),
            Block::Image { url, alt, width } => self.image(url, alt, *width, x, w)?,
        }
        Ok(())
    }

    fn list_item(
        &mut self,
        start: Option<u64>,
        i: usize,
        it: &Item,
        x: f32,
        w: f32,
    ) -> Result<(), ExportError> {
        let lh = BODY * 1.4;
        let mw = if start.is_some() { 24.0 } else { 16.0 };
        self.need(lh);
        let top = self.y;
        let st = Style::default();
        match (it.checked, start) {
            (Some(done), _) => {
                let bs = BODY * 0.82;
                let by = top + (lh - bs) / 2.0;
                let bx = x + 1.0;
                if done {
                    self.emit(Op::Rect {
                        x: bx,
                        y: by,
                        w: bs,
                        h: bs,
                        fill: Some(Rgb(0x2a, 0x5d, 0xb0)),
                        stroke: Some((Rgb(0x2a, 0x5d, 0xb0), 0.9)),
                    });
                    self.emit(Op::Line {
                        x1: bx + bs * 0.2,
                        y1: by + bs * 0.55,
                        x2: bx + bs * 0.42,
                        y2: by + bs * 0.77,
                        color: WHITE,
                        width: 1.3,
                    });
                    self.emit(Op::Line {
                        x1: bx + bs * 0.42,
                        y1: by + bs * 0.77,
                        x2: bx + bs * 0.82,
                        y2: by + bs * 0.25,
                        color: WHITE,
                        width: 1.3,
                    });
                } else {
                    self.emit(Op::Rect {
                        x: bx,
                        y: by,
                        w: bs,
                        h: bs,
                        fill: Some(WHITE),
                        stroke: Some((Rgb(0x44, 0x4c, 0x56), 0.9)),
                    });
                }
            }
            (None, Some(n)) => {
                let label = format!("{}.", n + i as u64);
                let lw = self.fonts.text_width(&label, &st, BODY);
                let line = Line {
                    frags: vec![Frag {
                        text: label,
                        style: st,
                    }],
                    width: lw,
                };
                self.place_line(
                    &line,
                    x + mw - 5.0 - lw,
                    lw,
                    top,
                    BODY,
                    lh,
                    Align::Left,
                    self.color,
                );
            }
            (None, None) => {
                let line = Line {
                    frags: vec![Frag {
                        text: "\u{2022}".into(),
                        style: st.clone(),
                    }],
                    width: self.fonts.text_width("\u{2022}", &st, BODY),
                };
                self.place_line(
                    &line,
                    x + 4.0,
                    line.width,
                    top,
                    BODY,
                    lh,
                    Align::Left,
                    self.color,
                );
            }
        }
        self.in_item += 1;
        let r = self.blocks(&it.blocks, x + mw, w - mw);
        self.in_item -= 1;
        r
    }

    fn code(&mut self, c: &str, x: f32, w: f32) {
        let lh = CODE * 1.38;
        let st = Style {
            code: true,
            ..Style::default()
        };
        let cw = self.fonts.text_width("M", &st, CODE).max(1.0);
        let per = (((w - 12.0) / cw).floor() as usize).max(1);
        let mut rows: Vec<String> = Vec::new();
        for l in c.trim_end_matches('\n').split('\n') {
            let l = l.replace('\t', "    ").replace('\r', "");
            let chars: Vec<char> = l.chars().collect();
            if chars.is_empty() {
                rows.push(String::new());
            }
            for ch in chars.chunks(per) {
                rows.push(ch.iter().collect());
            }
        }
        if rows.is_empty() {
            rows.push(String::new());
        }
        self.y += 2.0;
        let n = rows.len();
        for (i, r) in rows.iter().enumerate() {
            let pad_top = if i == 0 { 4.0 } else { 0.0 };
            let pad_bot = if i + 1 == n { 4.0 } else { 0.0 };
            self.need(lh + pad_top + pad_bot);
            let y = self.y;
            let first_on_page = (y - top()).abs() < 0.5;
            let pt = if first_on_page { 0.0 } else { pad_top };
            self.emit(Op::Rect {
                x,
                y,
                w,
                h: lh + pt + pad_bot,
                fill: Some(CODE_BG),
                stroke: None,
            });
            let line = Line {
                frags: vec![Frag {
                    text: r.clone(),
                    style: st.clone(),
                }],
                width: cw * r.chars().count() as f32,
            };
            self.place_line(&line, x + 6.0, w - 12.0, y + pt, CODE, lh, Align::Left, INK);
            self.mark(y, y + lh + pt + pad_bot);
            self.y += lh + pt + pad_bot;
        }
        self.y += 6.0;
    }

    fn table(&mut self, aligns: &[Align], head: &[Cell], rows: &[Vec<Cell>], x: f32, w: f32) {
        let ncol = rows
            .iter()
            .map(Vec::len)
            .chain([head.len(), aligns.len()])
            .max()
            .unwrap_or(0);
        if ncol == 0 {
            return;
        }
        let (size, pad) = (9.5_f32, 4.0_f32);
        let lh = size * 1.35;
        let empty: Cell = Vec::new();
        let cell = |r: &[Cell], c: usize| -> Cell { r.get(c).unwrap_or(&empty).clone() };
        // Natural (one line) and minimum (longest word) widths per column.
        let mut want = vec![0.0_f32; ncol];
        let mut min = vec![0.0_f32; ncol];
        // A column is never squeezed below its widest single character.
        let mut floor = vec![2.0 * pad + 8.0; ncol];
        let all: Vec<(&[Cell], bool)> = std::iter::once((head, true))
            .chain(rows.iter().map(|r| (r.as_slice(), false)))
            .collect();
        for (r, is_head) in &all {
            for c in 0..ncol {
                let sp = cell(r, c);
                let lines = self.wrap(&sp, size, 1.0e6, *is_head);
                let nat = lines.iter().map(|l| l.width).fold(0.0, f32::max);
                want[c] = want[c].max(nat + 2.0 * pad);
                for it in tokens(&sp, *is_head) {
                    if let Item2::Tok(t) = it {
                        for f in &t.frags {
                            for ch in f.text.chars() {
                                let cw = self.fonts.text_width(&ch.to_string(), &f.style, size);
                                floor[c] = floor[c].max(cw + 2.0 * pad);
                            }
                        }
                        let tw: f32 = t.frags.iter().map(|f| self.frag_width(f, size)).sum();
                        min[c] = min[c].max(tw + 2.0 * pad);
                    }
                }
            }
        }
        let colw: Vec<f32> = if want.iter().sum::<f32>() <= w {
            want.iter().map(|v| v.max(24.0)).collect()
        } else {
            let mut base: Vec<f32> = min.iter().zip(&floor).map(|(m, f)| m.max(*f)).collect();
            let sum: f32 = base.iter().sum();
            if sum > w {
                // Squeeze the wide columns down towards their floors; if even the floors do not
                // fit (very many columns) they are scaled evenly.
                let fsum: f32 = floor.iter().sum();
                if fsum >= w {
                    base = floor.iter().map(|f| f * w / fsum).collect();
                } else {
                    let scale = (w - fsum) / (sum - fsum);
                    base = base
                        .iter()
                        .zip(&floor)
                        .map(|(b, f)| f + (b - f) * scale)
                        .collect();
                }
            }
            let extra = w - base.iter().sum::<f32>();
            let room: f32 = want.iter().zip(&base).map(|(a, b)| (a - b).max(0.0)).sum();
            base.iter()
                .zip(&want)
                .map(|(b, a)| {
                    if room > 0.0 {
                        b + extra * (a - b).max(0.0) / room
                    } else {
                        *b
                    }
                })
                .collect()
        };
        let aligns: Vec<Align> = (0..ncol)
            .map(|c| aligns.get(c).copied().unwrap_or(Align::Left))
            .collect();
        let mk = |me: &Self, r: &[Cell], is_head: bool| -> Vec<Vec<Line>> {
            (0..ncol)
                .map(|c| me.wrap(&cell(r, c), size, colw[c] - 2.0 * pad, is_head))
                .collect()
        };
        let header: Vec<Vec<Line>> = if head.is_empty() {
            Vec::new()
        } else {
            mk(self, head, true)
        };
        // The header never sits alone at the bottom of a page.
        if !header.is_empty() {
            let hh = header.iter().map(Vec::len).max().unwrap_or(1).max(1) as f32 * lh + 2.0 * pad;
            self.need(hh + lh + 2.0 * pad);
        }
        let ctx = TableCtx {
            colw: &colw,
            aligns: &aligns,
            x,
            size,
            lh,
            pad,
        };
        let mut first = true;
        let mut order: Vec<(Vec<Vec<Line>>, bool)> = Vec::new();
        if !header.is_empty() {
            order.push((header.clone(), true));
        }
        for r in rows {
            order.push((mk(self, r, false), false));
        }
        for (cells, is_head) in order {
            let total = cells.iter().map(Vec::len).max().unwrap_or(0).max(1);
            let mut from = 0;
            while from < total {
                let avail = bottom() - self.y;
                let want_h = (total - from) as f32 * lh + 2.0 * pad;
                let mut count = total - from;
                if want_h > avail + 0.01 {
                    let page_h = bottom() - top();
                    if self.y > top() + 0.5 && want_h <= page_h {
                        self.new_page();
                        if !is_head {
                            self.repeat_header(&ctx, &header);
                        }
                        continue;
                    }
                    let fit = ((avail - 2.0 * pad) / lh).floor().max(0.0) as usize;
                    if fit == 0 && self.y > top() + 0.5 {
                        self.new_page();
                        if !is_head {
                            self.repeat_header(&ctx, &header);
                        }
                        continue;
                    }
                    count = fit.max(1).min(count);
                }
                self.table_chunk(&ctx, &cells, from, count, is_head);
                from += count;
                first = false;
                if from < total {
                    self.new_page();
                    if !is_head {
                        self.repeat_header(&ctx, &header);
                    }
                }
            }
        }
        let _ = first;
        self.y += 8.0;
    }

    fn repeat_header(&mut self, ctx: &TableCtx, header: &[Vec<Line>]) {
        if header.is_empty() {
            return;
        }
        let total = header.iter().map(Vec::len).max().unwrap_or(1).max(1);
        self.table_chunk(ctx, header, 0, total, true);
    }

    fn table_chunk(
        &mut self,
        t: &TableCtx,
        cells: &[Vec<Line>],
        from: usize,
        count: usize,
        is_head: bool,
    ) {
        let h = count as f32 * t.lh + 2.0 * t.pad;
        let y = self.y;
        let mut cx = t.x;
        for (c, lines) in cells.iter().enumerate() {
            let cw = t.colw[c];
            self.emit(Op::Rect {
                x: cx,
                y,
                w: cw,
                h,
                fill: is_head.then_some(HEAD_BG),
                stroke: Some((RULE, 0.5)),
            });
            for (k, l) in lines.iter().skip(from).take(count).enumerate() {
                let ly = y + t.pad + k as f32 * t.lh;
                self.place_line(
                    l,
                    cx + t.pad,
                    cw - 2.0 * t.pad,
                    ly,
                    t.size,
                    t.lh,
                    t.aligns[c],
                    INK,
                );
            }
            cx += cw;
        }
        self.mark(y, y + h);
        self.y += h;
    }

    fn image(
        &mut self,
        url: &str,
        alt: &str,
        width: Option<u16>,
        x: f32,
        w: f32,
    ) -> Result<(), ExportError> {
        let local = attachments::normalize(url)
            .ok()
            .filter(|_| !url.contains("://"));
        let Some(reference) = local else {
            // Remote (never fetched) or otherwise not a local attachment: show the alt text.
            let span = Span {
                text: format!("[{}]", if alt.is_empty() { url } else { alt }),
                style: Style {
                    link: links::safe_http_url(url).map(str::to_owned),
                    ..Style::default()
                },
                image: None,
            };
            self.paragraph(&[span], x, w, BODY, false, 6.0);
            return Ok(());
        };
        let id = match self.image_ids.get(&reference) {
            Some(i) => *i,
            None => {
                let a = self
                    .plan
                    .attachments
                    .iter()
                    .find(|a| a.reference == reference)
                    .ok_or_else(|| ExportError::Pdf(format!("image `{url}` was not loaded")))?;
                let data: krilla::Data = a.bytes.clone().into();
                let img = if a.bytes.starts_with(b"\x89PNG") {
                    krilla::image::Image::from_png(data, true)
                } else if a.bytes.starts_with(&[0xFF, 0xD8]) {
                    krilla::image::Image::from_jpeg(data, true)
                } else {
                    return Err(ExportError::Pdf(format!(
                        "image `{url}` is not PNG or JPEG (the only formats PDF export embeds)"
                    )));
                }
                .map_err(|e| ExportError::Pdf(format!("image `{url}` cannot be embedded: {e}")))?;
                self.images.push(img);
                self.image_ids
                    .insert(reference.clone(), self.images.len() - 1);
                self.images.len() - 1
            }
        };
        let (pw, ph) = self.images[id].size();
        let (pw, ph) = (pw.max(1) as f32, ph.max(1) as f32);
        let mut iw = match width {
            Some(p) => w * f32::from(p.clamp(20, 100)) / 100.0,
            None => (pw * 0.75).min(w),
        };
        let mut ih = iw * ph / pw;
        let page_h = bottom() - top();
        if ih > page_h {
            // Taller than a whole page: scale down (aspect kept) so it fits on one.
            iw *= page_h / ih;
            ih = page_h;
        }
        self.need(ih);
        let y = self.y;
        self.emit(Op::Image {
            x,
            y,
            w: iw,
            h: ih,
            id,
        });
        self.mark(y, y + ih);
        self.y += ih + 8.0;
        Ok(())
    }
}

struct TableCtx<'a> {
    colw: &'a [f32],
    aligns: &'a [Align],
    x: f32,
    size: f32,
    lh: f32,
    pad: f32,
}

/// Lay `markdown` out on A4 pages. Fails if a local image is missing, invalid or not embeddable.
pub fn layout(markdown: &str, store: &dyn AttachmentStore) -> Result<Layout, ExportError> {
    let plan = export::plan(markdown, store, "document.md")?;
    let blocks = parse(markdown);
    let mut lay = Lay {
        fonts: FontSet::new()?,
        plan: &plan,
        pages: vec![Vec::new()],
        y: top(),
        color: INK,
        trackers: Vec::new(),
        images: Vec::new(),
        image_ids: HashMap::new(),
        in_item: 0,
    };
    lay.blocks(&blocks, MARGIN_X, PAGE_W - 2.0 * MARGIN_X)?;
    let mut pages = lay.pages;
    while pages.len() > 1 && pages.last().is_some_and(Vec::is_empty) {
        pages.pop();
    }
    Ok(Layout {
        pages,
        images: lay.images,
        fonts: lay.fonts,
    })
}

fn color(c: Rgb) -> krilla::color::rgb::Color {
    krilla::color::rgb::Color::new(c.0, c.1, c.2)
}

/// Write a laid-out document as PDF bytes.
pub fn render(lay: &Layout, title: &str) -> Result<Vec<u8>, ExportError> {
    use krilla::geom::{Point, Rect, Size, Transform};
    use krilla::paint::{Fill, Stroke};
    let err = |what: &str| ExportError::Pdf(format!("cannot write the PDF ({what})"));
    let mut doc = krilla::Document::new();
    doc.set_metadata(
        krilla::metadata::Metadata::new()
            .title(title.to_owned())
            .creator("OmaTree".to_owned()),
    );
    let settings =
        krilla::page::PageSettings::from_wh(PAGE_W, PAGE_H).ok_or_else(|| err("page size"))?;
    for ops in &lay.pages {
        let mut page = doc.start_page_with(settings.clone());
        let mut links: Vec<(Rect, String)> = Vec::new();
        {
            let mut s = page.surface();
            for op in ops {
                match op {
                    Op::Text {
                        x,
                        y,
                        font,
                        size,
                        color: c,
                        text,
                    } => {
                        s.set_fill(Some(Fill {
                            paint: color(*c).into(),
                            ..Default::default()
                        }));
                        s.set_stroke(None);
                        s.draw_text(
                            Point::from_xy(*x, *y),
                            lay.fonts.kfonts[*font].clone(),
                            *size,
                            text,
                            false,
                            krilla::text::TextDirection::LeftToRight,
                        );
                    }
                    Op::Rect {
                        x,
                        y,
                        w,
                        h,
                        fill,
                        stroke,
                    } => {
                        let Some(r) = Rect::from_xywh(*x, *y, *w, *h) else {
                            continue;
                        };
                        let mut pb = krilla::geom::PathBuilder::new();
                        pb.push_rect(r);
                        let Some(path) = pb.finish() else { continue };
                        s.set_fill(fill.map(|c| Fill {
                            paint: color(c).into(),
                            ..Default::default()
                        }));
                        s.set_stroke(stroke.map(|(c, wd)| Stroke {
                            paint: color(c).into(),
                            width: wd,
                            ..Default::default()
                        }));
                        s.draw_path(&path);
                    }
                    Op::Line {
                        x1,
                        y1,
                        x2,
                        y2,
                        color: c,
                        width,
                    } => {
                        let mut pb = krilla::geom::PathBuilder::new();
                        pb.move_to(*x1, *y1);
                        pb.line_to(*x2, *y2);
                        let Some(path) = pb.finish() else { continue };
                        s.set_fill(None);
                        s.set_stroke(Some(Stroke {
                            paint: color(*c).into(),
                            width: *width,
                            ..Default::default()
                        }));
                        s.draw_path(&path);
                    }
                    Op::Image { x, y, w, h, id } => {
                        let Some(size) = Size::from_wh(*w, *h) else {
                            continue;
                        };
                        s.push_transform(&Transform::from_translate(*x, *y));
                        s.draw_image(lay.images[*id].clone(), size);
                        s.pop();
                    }
                    Op::Link { x, y, w, h, url } => {
                        if let Some(r) = Rect::from_xywh(*x, *y, *w, *h) {
                            links.push((r, url.clone()));
                        }
                    }
                }
            }
            s.finish();
        }
        for (r, url) in links {
            page.add_annotation(
                krilla::annotation::LinkAnnotation::new(
                    r,
                    krilla::annotation::Target::Action(krilla::action::Action::Link(
                        krilla::action::LinkAction::new(url),
                    )),
                )
                .into(),
            );
        }
        page.finish();
    }
    doc.finish().map_err(|e| ExportError::Pdf(format!("{e:?}")))
}

/// What an export produced.
#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub pages: usize,
    pub images: usize,
}

/// Export `markdown` (the current buffer) as a new PDF at `dest`. Reads the store only; the file is
/// published atomically and an existing `dest` is never replaced.
pub fn export_pdf(
    markdown: &str,
    store: &dyn AttachmentStore,
    title: &str,
    dest: &Path,
) -> Result<Summary, ExportError> {
    let lay = layout(markdown, store)?;
    let bytes = render(&lay, title)?;
    export::write_file(&bytes, dest)?;
    Ok(Summary {
        pages: lay.pages.len(),
        images: lay.images.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::attachments::FsStore;
    use lopdf::{Document, Object};
    fn tmp(tag: &str) -> crate::test_dir::TestDir {
        crate::test_dir::TestDir::new(&format!("pdf-{tag}"))
    }

    fn png(w: usize, h: usize, c: egui::Color32) -> Vec<u8> {
        crate::assets::encode_png(&egui::ColorImage::filled([w, h], c)).unwrap()
    }

    /// A store with the given PNG sizes; returns it with the references.
    fn store(sizes: &[(usize, usize)]) -> (FsStore, Vec<String>, crate::test_dir::TestDir) {
        let dir = tmp("store");
        let s = FsStore::new(&dir);
        let refs = sizes
            .iter()
            .map(|(w, h)| {
                s.put(&png(*w, *h, egui::Color32::from_rgb(200, 40, 40)), "png")
                    .unwrap()
            })
            .collect();
        (s, refs, dir)
    }

    /// An empty `FsStore` that keeps its directory alive for as long as the value lives.
    struct Held(FsStore, #[allow(dead_code)] crate::test_dir::TestDir);

    impl AttachmentStore for Held {
        fn put(
            &self,
            bytes: &[u8],
            ext: &str,
        ) -> Result<String, crate::attachments::AttachmentError> {
            self.0.put(bytes, ext)
        }
        fn get(&self, reference: &str) -> Result<Vec<u8>, crate::attachments::AttachmentError> {
            self.0.get(reference)
        }
        fn info(
            &self,
            reference: &str,
        ) -> Result<crate::attachments::AttachmentInfo, crate::attachments::AttachmentError>
        {
            self.0.info(reference)
        }
    }

    fn empty_store() -> Held {
        let dir = tmp("empty");
        Held(FsStore::new(&dir), dir)
    }

    /// E13a: the document names OmaTree as its creator, not the prototype the editor came from.
    #[test]
    fn the_pdf_names_omatree_as_its_creator() {
        let (_lay, pdf) = pdf_of("# T\n\ntext", &empty_store());
        let has = |needle: &[u8]| pdf.windows(needle.len()).any(|w| w == needle);
        assert!(has(b"OmaTree"), "the creator is recorded");
        assert!(!has(b"editor-spike"), "no trace of the prototype's name");
    }

    fn texts(lay: &Layout) -> Vec<Vec<String>> {
        lay.pages
            .iter()
            .map(|p| {
                p.iter()
                    .filter_map(|o| match o {
                        Op::Text { text, .. } => Some(text.clone()),
                        _ => None,
                    })
                    .collect()
            })
            .collect()
    }

    fn all_text(lay: &Layout) -> String {
        texts(lay).concat().join("")
    }

    // ---- reading a PDF back: text via the fonts' ToUnicode maps, links, images ----

    fn hex(t: &str) -> Vec<u8> {
        let t: String = t.chars().filter(|c| c.is_ascii_hexdigit()).collect();
        (0..t.len() / 2)
            .map(|i| u8::from_str_radix(&t[2 * i..2 * i + 2], 16).unwrap())
            .collect()
    }

    fn utf16(b: &[u8]) -> String {
        let u: Vec<u16> = b
            .chunks(2)
            .map(|c| u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]))
            .collect();
        String::from_utf16_lossy(&u)
    }

    /// code → text, from a ToUnicode CMap.
    fn cmap(src: &str) -> HashMap<u16, String> {
        let mut m = HashMap::new();
        let toks = |sec: &str| -> Vec<String> {
            let mut v = Vec::new();
            let mut rest = sec;
            while let Some(i) = rest.find(['<', '[', ']']) {
                if rest[i..].starts_with('<') {
                    let j = rest[i..].find('>').unwrap() + i;
                    v.push(rest[i + 1..j].to_owned());
                    rest = &rest[j + 1..];
                } else {
                    v.push(rest[i..i + 1].to_owned());
                    rest = &rest[i + 1..];
                }
            }
            v
        };
        for part in src.split("beginbfchar").skip(1) {
            let sec = part.split("endbfchar").next().unwrap();
            for p in toks(sec).chunks(2) {
                if p.len() == 2 {
                    let c = hex(&p[0]);
                    m.insert(
                        u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]),
                        utf16(&hex(&p[1])),
                    );
                }
            }
        }
        for part in src.split("beginbfrange").skip(1) {
            let sec = part.split("endbfrange").next().unwrap();
            let t = toks(sec);
            let mut i = 0;
            while i + 2 < t.len() {
                let a = hex(&t[i]);
                let b = hex(&t[i + 1]);
                let (a, b) = (
                    u16::from_be_bytes([a[0], *a.get(1).unwrap_or(&0)]),
                    u16::from_be_bytes([b[0], *b.get(1).unwrap_or(&0)]),
                );
                if t[i + 2] == "[" {
                    let mut j = i + 3;
                    let mut c = a;
                    while t[j] != "]" {
                        m.insert(c, utf16(&hex(&t[j])));
                        c += 1;
                        j += 1;
                    }
                    i = j + 1;
                } else {
                    let base = hex(&t[i + 2]);
                    let s = utf16(&base);
                    let mut chars: Vec<char> = s.chars().collect();
                    for c in a..=b {
                        m.insert(c, chars.iter().collect());
                        if let Some(l) = chars.last_mut() {
                            *l = char::from_u32(*l as u32 + 1).unwrap_or(*l);
                        }
                    }
                    i += 3;
                }
            }
        }
        m
    }

    /// The text of every page, decoded from the content streams exactly as a viewer would.
    fn extract(pdf: &[u8]) -> Vec<String> {
        let doc = Document::load_mem(pdf).unwrap();
        let mut out = Vec::new();
        for (_, id) in doc.get_pages() {
            let fonts = doc.get_page_fonts(id).unwrap();
            let maps: HashMap<Vec<u8>, HashMap<u16, String>> = fonts
                .iter()
                .map(|(name, f)| {
                    let tu = f
                        .get(b"ToUnicode")
                        .and_then(|o| doc.dereference(o))
                        .map(|(_, o)| o.clone())
                        .ok();
                    let m = tu
                        .and_then(|o| {
                            o.as_stream()
                                .ok()
                                .and_then(|s| s.decompressed_content().ok())
                        })
                        .map(|b| cmap(&String::from_utf8_lossy(&b)))
                        .unwrap_or_default();
                    (name.clone(), m)
                })
                .collect();
            let content = lopdf::content::Content::decode(&doc.get_page_content(id)).unwrap();
            let mut cur: Vec<u8> = Vec::new();
            let mut s = String::new();
            let dec = |bytes: &[u8], cur: &Vec<u8>| -> String {
                let m = maps.get(cur);
                bytes
                    .chunks(2)
                    .map(|c| {
                        let code = u16::from_be_bytes([c[0], *c.get(1).unwrap_or(&0)]);
                        m.and_then(|m| m.get(&code))
                            .cloned()
                            .unwrap_or_else(|| "\u{FFFD}".into())
                    })
                    .collect()
            };
            for op in content.operations {
                match op.operator.as_str() {
                    "Tf" => {
                        if let Some(Object::Name(n)) = op.operands.first() {
                            cur = n.clone();
                        }
                    }
                    "Tj" => {
                        if let Some(Object::String(b, _)) = op.operands.first() {
                            s.push_str(&dec(b, &cur));
                        }
                    }
                    "TJ" => {
                        if let Some(Object::Array(a)) = op.operands.first() {
                            for o in a {
                                if let Object::String(b, _) = o {
                                    s.push_str(&dec(b, &cur));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            out.push(s);
        }
        out
    }

    fn uris(pdf: &[u8]) -> Vec<String> {
        let doc = Document::load_mem(pdf).unwrap();
        let mut v = Vec::new();
        for (_, id) in doc.get_pages() {
            for a in doc.get_page_annotations(id).unwrap_or_default() {
                if let Ok(Object::Dictionary(act)) = a
                    .get(b"A")
                    .and_then(|o| doc.dereference(o))
                    .map(|(_, o)| o.clone())
                {
                    if let Ok(Object::String(u, _)) = act.get(b"URI") {
                        v.push(String::from_utf8_lossy(u).into_owned());
                    }
                }
            }
        }
        v
    }

    /// Image XObjects used by the pages, as (width, height) in pixels.
    fn pdf_images(pdf: &[u8]) -> Vec<(i64, i64)> {
        let doc = Document::load_mem(pdf).unwrap();
        let mut v = Vec::new();
        for o in doc.objects.values() {
            if let Object::Stream(s) = o {
                if s.dict
                    .get(b"Subtype")
                    .and_then(Object::as_name)
                    .is_ok_and(|n| n == b"Image")
                {
                    v.push((
                        s.dict.get(b"Width").and_then(Object::as_i64).unwrap_or(0),
                        s.dict.get(b"Height").and_then(Object::as_i64).unwrap_or(0),
                    ));
                }
            }
        }
        v
    }

    fn pdf_of(md: &str, st: &dyn AttachmentStore) -> (Layout, Vec<u8>) {
        let lay = layout(md, st).unwrap();
        let bytes = render(&lay, "Test").unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        (lay, bytes)
    }

    fn content_w() -> f32 {
        PAGE_W - 2.0 * MARGIN_X
    }

    /// Every drawn thing stays on its page, inside the margins.
    fn assert_on_page(lay: &Layout) {
        let eps = 0.6;
        for (pi, page) in lay.pages.iter().enumerate() {
            for op in page {
                match op {
                    Op::Text {
                        x,
                        y,
                        font,
                        size,
                        text,
                        ..
                    } => {
                        let w = lay.fonts.width(*font, text, *size);
                        assert!(
                            *x >= MARGIN_X - eps && x + w <= PAGE_W - MARGIN_X + eps,
                            "p{pi} text {text:?} x={x} w={w}"
                        );
                        assert!(
                            *y >= MARGIN_TOP && *y <= PAGE_H - MARGIN_BOTTOM + 3.0,
                            "p{pi} text {text:?} y={y}"
                        );
                    }
                    Op::Rect { x, y, w, h, .. } => {
                        assert!(
                            *x >= MARGIN_X - 6.5 && x + w <= PAGE_W - MARGIN_X + 6.5,
                            "p{pi} rect x={x} w={w}"
                        );
                        assert!(
                            *y >= MARGIN_TOP - 1.0 && y + h <= PAGE_H - MARGIN_BOTTOM + 1.0,
                            "p{pi} rect y={y} h={h}"
                        );
                    }
                    Op::Image { x, y, w, h, .. } => {
                        assert!(
                            *x >= MARGIN_X - eps && x + w <= PAGE_W - MARGIN_X + eps,
                            "p{pi} image"
                        );
                        assert!(
                            *y >= MARGIN_TOP - eps && y + h <= PAGE_H - MARGIN_BOTTOM + eps,
                            "p{pi} image y={y} h={h}"
                        );
                    }
                    Op::Line { x1, x2, y1, y2, .. } => {
                        assert!(
                            *x1 >= MARGIN_X - 1.0 && *x2 <= PAGE_W - MARGIN_X + 1.0,
                            "p{pi} line"
                        );
                        assert!(
                            *y1 >= MARGIN_TOP - 1.0 && *y2 <= PAGE_H - MARGIN_BOTTOM + 1.0,
                            "p{pi} line y"
                        );
                    }
                    Op::Link { .. } => {}
                }
            }
        }
    }

    const CZ: &str = "Příliš žluťoučký kůň úpěl ďábelské ódy";

    #[test]
    fn headings_and_paragraphs_with_czech_text_are_real_selectable_text() {
        let md = format!("# Nadpis jedna\n\n## Druhý nadpis\n\n###### Šestý\n\n{CZ}.\nDruhý řádek\\\nTvrdý zlom.\n");
        let (lay, pdf) = pdf_of(&md, &empty_store());
        assert_eq!(lay.pages.len(), 1);
        let sizes: Vec<f32> = lay.pages[0]
            .iter()
            .filter_map(|o| match o {
                Op::Text { size, font, .. } if *font == 1 => Some(*size),
                _ => None,
            })
            .collect();
        assert_eq!(sizes, [22.0, 18.0, 11.0], "bold headings, decreasing size");
        let t = extract(&pdf).concat();
        for needle in ["Nadpis jedna", "Druhý nadpis", "Šestý", CZ, "Tvrdý zlom."] {
            assert!(t.contains(needle), "{needle:?} not in extracted text {t:?}");
        }
        assert!(pdf_images(&pdf).is_empty(), "no page screenshots");
        assert_on_page(&lay);
        // The text comes from embedded, subset fonts with a ToUnicode map.
        let doc = Document::load_mem(&pdf).unwrap();
        assert!(doc.objects.values().any(|o| matches!(o, Object::Dictionary(d) if d.get(b"Type").and_then(Object::as_name).is_ok_and(|n| n == b"Font"))));
    }

    #[test]
    fn inline_styles_pick_faces_and_decorations() {
        let (lay, pdf) = pdf_of(
            "Text **tučný** *šikmý* ***oboje*** ~~škrtnutý~~ `kód()`.\n",
            &empty_store(),
        );
        let faces: HashMap<String, usize> = lay.pages[0]
            .iter()
            .filter_map(|o| match o {
                Op::Text { font, text, .. } => Some((text.trim().to_owned(), *font)),
                _ => None,
            })
            .collect();
        assert_eq!(faces["tučný"], 1);
        assert_eq!(faces["šikmý"], 2);
        assert_eq!(faces["oboje"], 3);
        assert_eq!(faces["kód()"], MONO_ID);
        let strikes = lay.pages[0]
            .iter()
            .filter(
                |o| matches!(o, Op::Line { width, color, .. } if *width == 0.7 && *color == INK),
            )
            .count();
        assert_eq!(strikes, 1);
        assert!(
            lay.pages[0]
                .iter()
                .any(|o| matches!(o, Op::Rect { fill: Some(f), .. } if *f == CODE_BG)),
            "inline code background"
        );
        assert!(
            extract(&pdf)
                .concat()
                .contains("Text tučný šikmý oboje škrtnutý kód()."),
            "{:?}",
            extract(&pdf)
        );
    }

    #[test]
    fn lists_tasks_quotes_rules_and_code_blocks() {
        let md = "- jedna\n- dvě\n  - vnořená\n\n3. třetí\n4. čtvrtá\n\n- [ ] otevřený úkol\n- [x] hotový úkol\n\n> citace\n> druhý řádek\n\n---\n\n```\nlet x = 1;\n\tindent\n```\n\nPo kódu.\n";
        let (lay, pdf) = pdf_of(md, &empty_store());
        let t = all_text(&lay);
        assert!(t.contains('\u{2022}') && t.contains("3.") && t.contains("4."));
        let boxes: Vec<Option<Rgb>> = lay.pages[0]
            .iter()
            .filter_map(|o| match o {
                Op::Rect {
                    fill,
                    stroke: Some(_),
                    w,
                    h,
                    ..
                } if (*w - *h).abs() < 0.01 && *w < 12.0 => Some(*fill),
                _ => None,
            })
            .collect();
        assert_eq!(boxes.len(), 2);
        assert_eq!(boxes[0], Some(WHITE), "unchecked box is empty");
        assert_ne!(boxes[1], Some(WHITE), "checked box is filled");
        assert!(
            lay.pages[0]
                .iter()
                .any(|o| matches!(o, Op::Line { width, .. } if *width == 2.5)),
            "quote bar"
        );
        assert!(
            lay.pages[0].iter().any(
                |o| matches!(o, Op::Line { width, color, .. } if *width == 0.8 && *color == RULE)
            ),
            "rule"
        );
        assert!(lay.pages[0].iter().any(|o| matches!(o, Op::Text { font, text, .. } if *font == MONO_ID && text.contains("let x = 1;"))));
        let ex = extract(&pdf).concat();
        for n in [
            "jedna",
            "vnořená",
            "otevřený úkol",
            "hotový úkol",
            "citace druhý řádek",
            "let x = 1;",
            "    indent",
            "Po kódu.",
        ] {
            assert!(ex.contains(n), "{n:?} missing from {ex:?}");
        }
        assert_on_page(&lay);
    }

    #[test]
    fn tables_render_cells_header_background_and_alignment() {
        let md = "| Jméno | Počet | Cena |\n| :--- | :---: | ---: |\n| Žluťoučký kůň | 3 | 10,50 |\n| **tučné** | 42 | `kód` |\n";
        let (lay, pdf) = pdf_of(md, &empty_store());
        let t = extract(&pdf).concat();
        for n in [
            "Jméno",
            "Počet",
            "Cena",
            "Žluťoučký kůň",
            "42",
            "10,50",
            "kód",
        ] {
            assert!(t.contains(n), "{n:?} in {t:?}");
        }
        let heads = lay.pages[0]
            .iter()
            .filter(
                |o| matches!(o, Op::Rect { fill: Some(f), stroke: Some(_), .. } if *f == HEAD_BG),
            )
            .count();
        assert_eq!(heads, 3, "header cells shaded");
        let cells = lay.pages[0]
            .iter()
            .filter(|o| matches!(o, Op::Rect { stroke: Some((c, _)), .. } if *c == RULE))
            .count();
        assert_eq!(cells, 9);
        // Right-aligned column: "10,50" ends at the cell's right padding.
        let x_of = |s: &str| {
            lay.pages[0]
                .iter()
                .find_map(|o| match o {
                    Op::Text {
                        x,
                        text,
                        font,
                        size,
                        ..
                    } if text.contains(s) => Some((*x, lay.fonts.width(*font, text, *size))),
                    _ => None,
                })
                .unwrap()
        };
        let (x1, w1) = x_of("10,50");
        let (x2, w2) = x_of("42");
        assert!(
            (x1 + w1) > (x2 + w2),
            "right-aligned numbers sit right of the centred ones"
        );
        assert_on_page(&lay);
    }

    #[test]
    fn only_http_links_become_clickable_annotations() {
        let md = "Viz [web](https://example.com/a?b=c), <https://auto.org>, [pošta](mailto:a@b.cz), [soubor](file:///etc/passwd), [relativní](doc.md) a [JS](javascript:alert(1)).\n";
        let (lay, pdf) = pdf_of(md, &empty_store());
        assert_eq!(
            uris(&pdf),
            ["https://example.com/a?b=c", "https://auto.org"]
        );
        assert_eq!(
            lay.pages[0]
                .iter()
                .filter(|o| matches!(o, Op::Link { .. }))
                .count(),
            2
        );
        // The unsafe ones are still readable text.
        let t = extract(&pdf).concat();
        assert!(
            t.contains("pošta")
                && t.contains("soubor")
                && t.contains("relativní")
                && t.contains("JS")
        );
    }

    #[test]
    fn images_are_embedded_with_width_percentages_and_aspect_ratio() {
        let (st, r, _keep) = store(&[(1200, 600), (300, 150)]);
        let md = format!("![Celá šířka]({0})\n\n![Šedesát]({0}){{width=60%}}\n\n![Malý]({1})\n\n![Min]({1}){{width=5%}}\n\n![Max]({1}){{width=400%}}\n\n![Opakovaný]({0}){{width=100%}}\n", r[0], r[1]);
        let (lay, pdf) = pdf_of(&md, &st);
        let imgs: Vec<(f32, f32)> = lay
            .pages
            .iter()
            .flatten()
            .filter_map(|o| match o {
                Op::Image { w, h, .. } => Some((*w, *h)),
                _ => None,
            })
            .collect();
        assert_eq!(imgs.len(), 6);
        let w = content_w();
        let want = [w, 0.6 * w, 225.0, 0.2 * w, w, w];
        // First: natural 1200 px × 0.75 is wider than the page, so it fits the content width.
        assert!((imgs[0].0 - w).abs() < 0.01 && (imgs[1].0 - 0.6 * w).abs() < 0.01);
        assert!(
            (imgs[2].0 - 225.0).abs() < 0.01,
            "300 px at 96 dpi = 225 pt"
        );
        assert!((imgs[3].0 - 0.2 * w).abs() < 0.01, "5% clamps to 20%");
        assert!((imgs[4].0 - w).abs() < 0.01, "400% clamps to 100%");
        let _ = want;
        for (iw, ih) in &imgs {
            assert!((iw / ih - 2.0).abs() < 1e-3, "aspect ratio kept");
        }
        // One embedded copy per distinct picture, with the original pixel size.
        let mut px = pdf_images(&pdf);
        px.sort_unstable();
        assert_eq!(px, [(300, 150), (1200, 600)]);
        assert!(
            extract(&pdf).concat().is_empty() || !extract(&pdf).concat().contains("Celá"),
            "alt text is not printed for embedded images"
        );
        assert_on_page(&lay);
    }

    #[test]
    fn remote_and_inline_images_print_their_alt_text() {
        let md = "![Vzdálený obrázek](https://example.com/x.png)\n\nText s ![malý](https://example.com/y.png) obrázkem uvnitř.\n";
        let (_lay, pdf) = pdf_of(md, &empty_store());
        let t = extract(&pdf).concat();
        assert!(
            t.contains("[Vzdálený obrázek]") && t.contains("[malý]"),
            "{t:?}"
        );
        assert!(pdf_images(&pdf).is_empty(), "nothing fetched or embedded");
        assert_eq!(uris(&pdf).len(), 2);
    }

    #[test]
    fn long_documents_paginate_without_overflow_or_empty_pages() {
        let para = format!("{CZ}, {CZ}, {CZ}. ");
        let mut md = String::new();
        for i in 0..120 {
            if i % 15 == 0 {
                md.push_str(&format!("## Kapitola {}\n\n", i / 15));
            }
            md.push_str(&para.repeat(2));
            md.push_str("\n\n");
        }
        let (lay, pdf) = pdf_of(&md, &empty_store());
        assert!(lay.pages.len() >= 5, "{} pages", lay.pages.len());
        assert!(lay.pages.iter().all(|p| !p.is_empty()), "no empty page");
        assert_on_page(&lay);
        let ex = extract(&pdf);
        assert_eq!(ex.len(), lay.pages.len());
        assert!(ex.iter().all(|p| p.contains("Příliš žluťoučký")));
        // No heading is the last thing on a page.
        for (i, page) in lay.pages.iter().enumerate() {
            let last = page.iter().rev().find_map(|o| match o {
                Op::Text { font, size, .. } => Some((*font, *size)),
                _ => None,
            });
            assert!(
                !matches!(last, Some((1, s)) if s > BODY + 1.0),
                "page {i} ends with a heading"
            );
        }
        // The empty document and a trailing blank still give exactly one page.
        assert_eq!(layout("", &empty_store()).unwrap().pages.len(), 1);
        assert_eq!(layout("\n\n\n", &empty_store()).unwrap().pages.len(), 1);
    }

    #[test]
    fn unbreakable_words_long_code_lines_and_wide_cells_stay_inside_the_page() {
        let long = "A".repeat(400);
        let url = format!("https://example.com/{}", "x".repeat(300));
        let md = format!(
            "{long}\n\n`{long}`\n\n{url}\n\n```\n{long}{long}\n```\n\n| a | b |\n| - | - |\n| {long} | krátké |\n| ok | {} |\n",
            "slovo ".repeat(80)
        );
        let (lay, pdf) = pdf_of(&md, &empty_store());
        assert_on_page(&lay);
        let ex = extract(&pdf).concat();
        // Nothing is lost by breaking: all 'A's are still there.
        assert_eq!(
            ex.matches('A').count(),
            400 * 5,
            "{}",
            ex.matches('A').count()
        );
    }

    #[test]
    fn tables_that_cross_pages_repeat_their_header_and_do_not_split_short_rows() {
        let mut md = String::from("| Hlavička A | Hlavička B |\n| --- | --- |\n");
        for i in 0..90 {
            md.push_str(&format!("| řádek {i} | hodnota {i} |\n"));
        }
        md.push_str("\nPo tabulce.\n");
        let (lay, pdf) = pdf_of(&md, &empty_store());
        assert!(lay.pages.len() >= 2);
        let ex = extract(&pdf);
        for (i, p) in ex
            .iter()
            .enumerate()
            .take(lay.pages.len().saturating_sub(0))
        {
            if p.contains("řádek") {
                assert!(
                    p.contains("Hlavička A"),
                    "page {i} lacks the repeated header"
                );
            }
        }
        assert!(ex.last().unwrap().contains("Po tabulce."));
        // Every row's cells lie wholly on one page.
        assert_on_page(&lay);
        // A single huge cell breaks across pages without losing text.
        let big = format!("| h |\n| - |\n| {} |\n\nkonec\n", "dlouhý ".repeat(900));
        let (lay2, pdf2) = pdf_of(&big, &empty_store());
        assert!(lay2.pages.len() >= 2, "{} pages", lay2.pages.len());
        assert_on_page(&lay2);
        let words = extract(&pdf2).concat().matches("dlouhý").count();
        assert_eq!(words, 900);
    }

    #[test]
    fn images_move_to_the_next_page_when_they_do_not_fit_and_tall_ones_are_scaled() {
        let (st, r, _keep) = store(&[(400, 400), (300, 3000)]);
        // Fill most of the first page, then an image that cannot fit in what is left.
        let filler = "Řádek textu.\n\n".repeat(33);
        let md = format!("{filler}![Čtverec]({}){{width=60%}}\n\nKonec.\n", r[0]);
        let (lay, _) = pdf_of(&md, &st);
        assert_eq!(lay.pages.len(), 2);
        let on_page = |p: usize| lay.pages[p].iter().any(|o| matches!(o, Op::Image { .. }));
        assert!(!on_page(0) && on_page(1), "image moved to page 2");
        assert!(
            lay.pages[1]
                .iter()
                .any(|o| matches!(o, Op::Image { y, .. } if (*y - MARGIN_TOP).abs() < 0.01)),
            "at the top of page 2"
        );
        assert_on_page(&lay);
        // An image taller than a page is scaled down to fit one page, aspect kept.
        let md = format!("Před.\n\n![Vysoký]({}){{width=100%}}\n\nPo.\n", r[1]);
        let (lay, _) = pdf_of(&md, &st);
        let (w, h) = lay
            .pages
            .iter()
            .flatten()
            .find_map(|o| match o {
                Op::Image { w, h, .. } => Some((*w, *h)),
                _ => None,
            })
            .unwrap();
        assert!(
            h <= PAGE_H - MARGIN_TOP - MARGIN_BOTTOM + 0.01 && (w / h - 0.1).abs() < 1e-3,
            "{w}x{h}"
        );
        assert_on_page(&lay);
        assert!(lay
            .pages
            .last()
            .unwrap()
            .iter()
            .any(|o| matches!(o, Op::Text { text, .. } if text.contains("Po."))));
    }

    #[test]
    fn missing_invalid_and_unsupported_images_fail_with_the_reference_named() {
        let (st, r, dir) = store(&[(10, 10)]);
        for (md, needle) in [
            ("![x](assets/missing.png)".to_owned(), "assets/missing.png"),
            ("![x](../outside.png)".to_owned(), "../outside.png"),
            ("![x](/etc/passwd)".to_owned(), "/etc/passwd"),
        ] {
            let e = layout(&md, &st).err().expect("must fail").to_string();
            assert!(e.contains(needle), "{e}");
        }
        std::fs::write(dir.join("assets/anim.gif"), b"GIF89a....").unwrap();
        let e = layout("![x](assets/anim.gif)", &st)
            .err()
            .unwrap()
            .to_string();
        assert!(
            e.contains("assets/anim.gif") && e.contains("PNG or JPEG"),
            "{e}"
        );
        std::fs::write(
            dir.join("assets/broken.png"),
            b"\x89PNG\r\n\x1a\nnot really",
        )
        .unwrap();
        let e = layout("![x](assets/broken.png)", &st)
            .err()
            .unwrap()
            .to_string();
        assert!(e.contains("assets/broken.png"), "{e}");
        assert!(layout(&format!("![ok]({})", r[0]), &st).is_ok());
    }

    #[test]
    fn export_refuses_to_overwrite_and_leaves_no_partial_file_on_failure() {
        let (st, r, _keep) = store(&[(10, 10)]);
        let base = tmp("out");
        let dest = base.join("note.pdf");
        let sum = export_pdf(&format!("# T\n\n![i]({})", r[0]), &st, "note.md", &dest).unwrap();
        assert_eq!(
            sum,
            Summary {
                pages: 1,
                images: 1
            }
        );
        let first = std::fs::read(&dest).unwrap();
        assert_eq!(&first[..5], b"%PDF-");
        let e = export_pdf("# Jiný", &st, "note.md", &dest).unwrap_err();
        assert!(matches!(e, ExportError::Conflict(_)), "{e}");
        assert_eq!(
            std::fs::read(&dest).unwrap(),
            first,
            "existing PDF untouched"
        );
        // A failing export (missing image) creates nothing, not even a staging file.
        let d2 = base.join("fail.pdf");
        assert!(export_pdf("![x](assets/none.png)", &st, "n.md", &d2).is_err());
        assert!(!d2.exists());
        let left: Vec<_> = std::fs::read_dir(&base)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(left, ["note.pdf"]);
        // Missing destination folder, and a read-only one.
        assert!(export_pdf("x", &st, "n.md", &base.join("no/such/x.pdf")).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let ro = base.join("ro");
            std::fs::create_dir(&ro).unwrap();
            std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o555)).unwrap();
            if std::fs::write(ro.join("probe"), b"").is_err() {
                assert!(export_pdf("x", &st, "n.md", &ro.join("x.pdf")).is_err());
                assert_eq!(std::fs::read_dir(&ro).unwrap().count(), 0);
            }
            std::fs::set_permissions(&ro, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }

    #[test]
    fn unsupported_markup_stays_readable_and_emoji_do_not_corrupt_text() {
        let md = "<div class=\"x\">HTML blok</div>\n\nText s <b>inline</b> HTML.\n\nEmoji 🦀 a 👨‍👩‍👧 uprostřed.\n\n1. první\n   - vnořená\n     > citace v seznamu\n";
        let (lay, pdf) = pdf_of(md, &empty_store());
        let t = extract(&pdf).concat();
        assert!(
            t.contains("HTML blok") && t.contains("inline") && t.contains("citace v seznamu"),
            "{t:?}"
        );
        assert!(
            t.contains("uprostřed."),
            "text around emoji survives: {t:?}"
        );
        // The emoji either render with an installed emoji font or show that font's missing-glyph box.
        let _ = lay;
    }
}
