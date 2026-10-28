//! Turns (source, spans, caret) into an egui `LayoutJob` **whose text is exactly the source**.
//!
//! Concealment is purely presentational: delimiter characters stay in the galley (so egui's
//! own cursor / hit-testing / selection / undo keep working on the real buffer) but are given a
//! near-zero font size and transparent colour while the caret is away from their span.

use crate::markdown::{Kind, Span};
use egui::{text::LayoutJob, Color32, FontFamily, FontId, TextFormat};
use std::ops::Range;

/// Font size used for concealed source. Not zero: egui's glyph cache does not like that.
pub const HIDDEN_SIZE: f32 = 0.1;

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub struct Flags {
    pub heading: u8,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
    pub quote: bool,
    pub list_marker: bool,
    /// Task box whose glyphs stay (for width and hit-testing) but are invisible: the app paints a
    /// checkbox over them.
    pub task_box: bool,
    pub code_block: bool,
    pub hidden: bool,
    pub dim: bool,
    /// Concealed image line that reserves this much height for the painted image.
    pub row_height: Option<f32>,
}

/// Reserved height of an image: (line start byte, destination, annotated width percent).
pub type ImageHeight<'a> = dyn Fn(usize, &str, Option<u16>) -> Option<f32> + 'a;

pub struct Params<'a> {
    pub base_size: f32,
    pub text_color: Color32,
    pub heading_colors: [Color32; 6],
    pub quote_color: Color32,
    pub code_color: Color32,
    pub dim_color: Color32,
    pub accent: Color32,
    pub code_bg: Color32,
    pub bold_family: Option<FontFamily>,
    /// `fonts::bold_drop` for `bold_family`: how far (in ems) bold text must sit lower on its line
    /// to share the default face's baseline. Zero when there is no bold family.
    pub bold_drop: f32,
    /// Physical pixels per point: line boxes are a whole number of pixels (see `format_for`).
    pub pixels_per_point: f32,
    /// Caret / selection as a byte range (start == end for a bare caret).
    pub selection: Range<usize>,
    /// Display height of an image for a destination, `None` if it cannot be shown.
    pub image_height: &'a ImageHeight<'a>,
    /// Height reserved for a rendered table (display height plus padding).
    pub table_height: &'a dyn Fn(&crate::markdown::Table) -> f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImageSlot {
    pub byte_start: usize,
    pub url: String,
    pub height: f32,
    /// Caret is on the line: raw source is shown instead of the painted image.
    pub active: bool,
}

/// Non-text decorations the app paints behind/over the galley rows (all positions are bytes).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decor {
    /// Thematic break drawn on the (concealed) line starting at `at`.
    Rule { at: usize },
    /// Left border for the blockquote covering `range`.
    Quote { range: Range<usize>, depth: u8 },
    /// Background surface behind code lines.
    CodeBlock { range: Range<usize> },
}

pub fn decorations(spans: &[Span], sel: &Range<usize>) -> Vec<Decor> {
    let mut out = Vec::new();
    for s in spans {
        let active = touches(&s.reveal, sel);
        match &s.kind {
            Kind::Rule if !active => out.push(Decor::Rule { at: s.range.start }),
            Kind::Quote { depth } if !s.reveal.is_empty() => {
                out.push(Decor::Quote {
                    range: s.reveal.clone(),
                    depth: *depth,
                });
            }
            Kind::CodeBlock { body } => {
                // Inactive: just the body (fences are concealed). Active: fences included. An empty
                // body keeps the reserved opening-fence row as its surface.
                let r = if active {
                    s.reveal.clone()
                } else if body.is_empty() && s.markers.len() == 2 {
                    s.markers[0].clone()
                } else {
                    body.clone()
                };
                if !r.is_empty() {
                    out.push(Decor::CodeBlock { range: r });
                }
            }
            _ => {}
        }
    }
    out
}

pub fn touches(r: &Range<usize>, sel: &Range<usize>) -> bool {
    sel.start <= r.end && sel.end >= r.start
}

pub fn compute_flags(src: &str, spans: &[Span], p: &Params) -> (Vec<Flags>, Vec<ImageSlot>) {
    let mut flags = vec![Flags::default(); src.len()];
    let mut images = Vec::new();
    for s in spans {
        let active = touches(&s.reveal, &p.selection);
        let mut apply = |r: Range<usize>, f: &dyn Fn(&mut Flags)| {
            for fl in &mut flags[r] {
                f(fl);
            }
        };
        match &s.kind {
            Kind::Table(t) => {
                // Inactive: conceal every source line (0.1 pt) and give the first line the height
                // of the painted table. Active: raw source, unstyled.
                if !active {
                    let h = (p.table_height)(t);
                    let first = t.header.range.clone();
                    apply(s.range.clone(), &|f| f.hidden = true);
                    apply(first.start..(first.end + 1).min(s.range.end), &|f| {
                        f.row_height = Some(h)
                    });
                }
            }
            Kind::Heading(l) => apply(s.range.clone(), &|f| f.heading = *l),
            Kind::Strong => apply(s.range.clone(), &|f| f.bold = true),
            Kind::Emphasis => apply(s.range.clone(), &|f| f.italic = true),
            Kind::Strikethrough => apply(s.range.clone(), &|f| f.strike = true),
            Kind::Task { .. } => {
                if active {
                    apply(s.range.clone(), &|f| f.dim = true);
                } else {
                    apply(s.range.clone(), &|f| f.task_box = true);
                }
            }
            Kind::Code => apply(s.range.clone(), &|f| f.code = true),
            Kind::CodeBlock { body } => {
                apply(s.range.clone(), &|f| f.code_block = true);
                // `` ``` `` + `` ``` `` with nothing between would collapse to nothing when its fences are
                // concealed: keep one code-sized row so the block stays visible and clickable.
                if !active && body.is_empty() && s.markers.len() == 2 {
                    let h = p.base_size * 1.6;
                    apply(s.markers[0].clone(), &|f| f.row_height = Some(h));
                }
            }
            Kind::Quote { .. } => apply(s.range.clone(), &|f| f.quote = true),
            Kind::QuoteLine => {}
            Kind::ListMarker => apply(s.range.clone(), &|f| f.list_marker = true),
            Kind::Rule => {
                if active {
                    apply(s.range.clone(), &|f| f.dim = true);
                } else {
                    let h = p.base_size;
                    apply(s.range.clone(), &|f| {
                        f.hidden = true;
                        f.row_height = Some(h);
                    });
                }
            }
            Kind::Link { .. } => apply(s.range.clone(), &|f| f.link = true),
            Kind::Image { url, width, .. } => {
                if let Some(h) = (p.image_height)(s.range.start, url, *width) {
                    images.push(ImageSlot {
                        byte_start: s.range.start,
                        url: url.clone(),
                        height: h,
                        active,
                    });
                    if !active {
                        apply(s.range.clone(), &|f| {
                            f.hidden = true;
                            f.row_height = Some(h);
                        });
                    } else {
                        apply(s.range.clone(), &|f| f.dim = true);
                    }
                } else {
                    apply(s.range.clone(), &|f| f.dim = true);
                }
            }
        }
        for m in &s.markers {
            if active {
                apply(m.clone(), &|f| f.dim = true);
            } else {
                apply(m.clone(), &|f| f.hidden = true);
            }
        }
    }
    (flags, images)
}

/// Line height as a multiple of the font size.
const BODY_LEADING: f32 = 1.42;
const HEADING_LEADING: f32 = 1.25;
const CODE_LEADING: f32 = 1.38;

fn heading_scale(l: u8) -> f32 {
    match l {
        1 => 1.8,
        2 => 1.45,
        3 => 1.2,
        _ => 1.1,
    }
}

fn format_for(f: &Flags, p: &Params) -> TextFormat {
    let mut size = p.base_size
        * if f.heading > 0 {
            heading_scale(f.heading)
        } else {
            1.0
        };
    let mut family = FontFamily::Proportional;
    let mut color = if f.heading > 0 {
        p.heading_colors[usize::from(f.heading.clamp(1, 6)) - 1]
    } else if f.quote {
        p.quote_color
    } else {
        p.text_color
    };
    if f.list_marker {
        color = p.accent;
    }
    let mut bold = f.bold || f.heading > 0 || f.list_marker;
    if f.code_block {
        family = FontFamily::Monospace;
        size = p.base_size * 0.92;
        bold = false;
        color = p.code_color;
    }
    if f.code {
        family = FontFamily::Monospace;
        size = p.base_size
            * 0.92
            * if f.heading > 0 {
                heading_scale(f.heading)
            } else {
                1.0
            };
        bold = false;
        color = p.code_color;
    }
    if bold {
        if let Some(b) = &p.bold_family {
            family = b.clone();
        }
    }
    let mut fmt = TextFormat {
        font_id: FontId::new(size, family),
        color,
        italics: f.italic,
        ..Default::default()
    };

    if bold && p.bold_family.is_none() {
        // No bold face available: approximate with letter spacing.
        fmt.extra_letter_spacing = size * 0.04;
    }
    if f.code {
        fmt.background = p.code_bg;
    }
    if f.link {
        color = p.accent;
        fmt.underline = egui::Stroke::new(1.0, p.accent);
    }
    fmt.color = color;
    // Leading: a little air between lines of text, tighter in headings (a heading that wraps is
    // one unit) and in code. Concealed source keeps the height its row asks for (below).
    let leading = size
        * if f.heading > 0 {
            HEADING_LEADING
        } else if f.code_block {
            CODE_LEADING
        } else {
            BODY_LEADING
        };
    // A whole number of pixels, as egui rounds the row height to one: glyphs that are centred
    // in the row (links, below) and glyphs that are placed from their own line box (everything
    // else) then agree on where the baseline is, instead of differing by the rounding.
    let leading = (leading * p.pixels_per_point).round() / p.pixels_per_point;
    fmt.line_height = Some(leading);
    if bold && f.heading == 0 && p.bold_family.is_some() && p.bold_drop != 0.0 {
        // The bold face's baseline is not the default face's: egui hangs each glyph from the top
        // of its line box by its own face's ascent. A glyph sits `(row height - line height)`
        // times the vertical alignment, plus half of `line_height - face height`, below the top,
        // so a shorter line box aligned to the bottom (or the top, to go up) moves the bold glyphs
        // by exactly the difference between the faces, before egui rounds the baseline to a
        // pixel. Plain text in the row is untouched; the row stays as tall as it was, because
        // the plain text still sets it. (A row of bold text only is `2 * shift` shorter.)
        let shift = p.bold_drop * size;
        fmt.line_height = Some(leading - 2.0 * shift.abs());
        fmt.valign = if shift > 0.0 {
            egui::Align::Max
        } else {
            egui::Align::Min
        };
    }
    if f.link && !bold {
        // egui draws an underline on the bottom of the glyph's line box, which the leading would
        // push away from the text: a link keeps its natural line box and is centred in the
        // leading instead, so the underline stays under its letters.
        fmt.line_height = None;
        fmt.valign = egui::Align::Center;
    }
    if f.strike {
        fmt.strikethrough = egui::Stroke::new(1.0, color);
    }
    if f.dim {
        fmt.color = p.dim_color;
        fmt.underline = egui::Stroke::NONE;
        if f.strike {
            fmt.strikethrough = egui::Stroke::new(1.0, p.dim_color);
        }
    }
    if f.task_box {
        // Monospace: `[ ]` and `[x]` have the same advance in any font, so the text after the box
        // starts at the same x whether it is checked or not.
        // 0.92 like code text: at full size the monospace face is a point taller than the
        // proportional one and would stretch every task row.
        fmt.font_id = FontId::new(p.base_size * 0.92, FontFamily::Monospace);
        fmt.color = Color32::TRANSPARENT;
    }
    if f.hidden {
        fmt.font_id = FontId::new(HIDDEN_SIZE, FontFamily::Proportional);
        fmt.color = Color32::TRANSPARENT;
        fmt.background = Color32::TRANSPARENT;
        fmt.underline = egui::Stroke::NONE;
        fmt.italics = false;
        fmt.extra_letter_spacing = 0.0;
        fmt.line_height = f.row_height;
    }
    fmt
}

pub fn build_job(
    src: &str,
    spans: &[Span],
    p: &Params,
    wrap_width: f32,
) -> (LayoutJob, Vec<ImageSlot>) {
    let (flags, images) = compute_flags(src, spans, p);
    let mut job = LayoutJob::default();
    job.wrap.max_width = wrap_width;
    let mut start = 0;
    while start < src.len() {
        let mut end = start + 1;
        while end < src.len() && flags[end] == flags[start] {
            end += 1;
        }
        // Style runs always begin/end on char boundaries (all spans come from the parser),
        // but never trust that when slicing user text.
        while !src.is_char_boundary(end) {
            end += 1;
        }
        job.append(&src[start..end], 0.0, format_for(&flags[start], p));
        start = end;
    }
    (job, images)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::parse;

    fn params<'a>(sel: Range<usize>, img: &'a ImageHeight<'a>) -> Params<'a> {
        Params {
            base_size: 15.0,
            text_color: Color32::WHITE,
            heading_colors: [Color32::WHITE; 6],
            quote_color: Color32::WHITE,
            code_color: Color32::WHITE,
            dim_color: Color32::GRAY,
            accent: Color32::BLUE,
            code_bg: Color32::DARK_GRAY,
            bold_family: None,
            bold_drop: 0.0,
            pixels_per_point: 1.0,
            selection: sel,
            image_height: img,
            table_height: &|_| 90.0,
        }
    }

    #[test]
    fn job_text_is_exactly_the_source() {
        let src = "# Příliš\n\n**b🙂** *i* `c` [l](u)\n\n![a](x.png)\n<!-- c -->\n";
        let spans = parse(src);
        let img = |_: usize, _: &str, _: Option<u16>| Some(100.0);
        for caret in [0, 3, 12, src.len()] {
            let (job, _) = build_job(src, &spans, &params(caret..caret, &img), 300.0);
            assert_eq!(job.text, src);
            let covered: usize = job
                .sections
                .iter()
                .map(|s| s.byte_range.end.0 - s.byte_range.start.0)
                .sum();
            assert_eq!(covered, src.len());
        }
    }

    #[test]
    fn markers_hidden_only_when_caret_away() {
        let src = "a **bold** b";
        let spans = parse(src);
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let (f_away, _) = compute_flags(src, &spans, &params(0..0, &img));
        assert!(f_away[2].hidden && f_away[3].hidden && f_away[8].hidden && !f_away[4].hidden);
        // Caret just before, inside, and just after the span reveals it.
        for c in [2, 5, 10] {
            let (f, _) = compute_flags(src, &spans, &params(c..c, &img));
            assert!(!f[2].hidden && f[2].dim, "caret {c}");
        }
        // A selection that merely crosses the span reveals it too.
        let (f, _) = compute_flags(src, &spans, &params(0..12, &img));
        assert!(f[2].dim);
    }

    #[test]
    fn image_line_reserves_height_only_when_inactive() {
        let src = "x\n\n![a](x.png)\n\ny\n";
        let spans = parse(src);
        let img = |_: usize, _: &str, _: Option<u16>| Some(80.0);
        let (f, slots) = compute_flags(src, &spans, &params(0..0, &img));
        assert_eq!(slots.len(), 1);
        assert_eq!(f[3].row_height, Some(80.0));
        let (f, _) = compute_flags(src, &spans, &params(5..5, &img));
        assert_eq!(f[3].row_height, None);
    }

    /// Real egui text layout: concealed markers occupy ~no width, yet every source char stays
    /// in the galley, so egui's cursor maps (char index <-> x position) cover the whole buffer.
    #[test]
    fn galley_keeps_all_chars_and_conceals_width() {
        let ctx = egui::Context::default();
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let mut measured = Vec::new();
        let mut out = ctx.run_ui(Default::default(), |ui| {
            let ctx = ui.ctx().clone();
            for (src, caret) in [("a **b** c", 0usize), ("a **b** c", 4), ("a b c", 0)] {
                let spans = parse(src);
                let (job, _) = build_job(src, &spans, &params(caret..caret, &img), 500.0);
                let galley = ctx.fonts_mut(|f| f.layout_job(job));
                assert_eq!(galley.text(), src);
                // Every char index has a distinct, monotonically non-decreasing x position.
                let xs: Vec<f32> = (0..=src.chars().count())
                    .map(|c| galley.pos_from_cursor(egui::text::CCursor::new(c)).min.x)
                    .collect();
                assert!(xs.windows(2).all(|w| w[0] <= w[1]), "{xs:?}");
                measured.push((galley.size().x, xs));
            }
        });
        out.textures_delta.clear();
        let (hidden, revealed, plain) = (&measured[0], &measured[1], &measured[2]);
        assert!(
            revealed.0 > plain.0 + 8.0,
            "revealed markers must take space"
        );
        assert!(
            (hidden.0 - plain.0).abs() < 3.0,
            "hidden {} vs plain {}",
            hidden.0,
            plain.0
        );
        // The two hidden `**` before `b` are (almost) zero-width.
        assert!(hidden.1[4] - hidden.1[2] < 1.5, "{:?}", hidden.1);
    }

    /// A palette change is colour-only: same text, caret, char->x geometry, and undo history.
    #[test]
    fn colour_change_keeps_text_caret_geometry_and_undo() {
        use egui::{self, text::CCursor, text::CCursorRange, Event, Key, Modifiers, RawInput};
        let ctx = egui::Context::default();
        let id = egui::Id::unique("theme-test");
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let src0 = "a **b** c";
        let mut text = src0.to_owned();
        let mut t = 0.0;
        let mut frame = |text: &mut String, events: Vec<Event>, white: bool| {
            t += 2.0;
            let mut input = RawInput {
                time: Some(t),
                events,
                ..Default::default()
            };
            input.focused = true;
            let mut result = None;
            let mut out = ctx.run_ui(input, |ui| {
                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                    let src = buf.as_str();
                    let mut p = params(0..0, &img);
                    if !white {
                        p.text_color = Color32::from_rgb(10, 20, 30);
                        p.dim_color = Color32::from_rgb(200, 0, 0);
                    }
                    let (job, _) = build_job(src, &parse(src), &p, wrap);
                    ui.ctx().fonts_mut(|f| f.layout_job(job))
                };
                let o = egui::TextEdit::multiline(text)
                    .id(id)
                    .layouter(&mut layouter)
                    .show(ui);
                o.response.request_focus();
                let xs: Vec<f32> = (0..=o.galley.text().chars().count())
                    .map(|c| o.galley.pos_from_cursor(CCursor::new(c)).min.x)
                    .collect();
                result = Some((
                    xs,
                    o.cursor_range
                        .map(|r| (r.primary.index.0, r.secondary.index.0)),
                ));
            });
            out.textures_delta.clear();
            result.unwrap()
        };
        frame(&mut text, vec![], true);
        let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        st.cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(5))));
        st.store(&ctx, id);
        let (xs_a, cur_a) = frame(&mut text, vec![Event::Text("X".into())], true);
        assert_eq!(text, "a **bX** c");
        let (xs_b, cur_b) = frame(&mut text, vec![], false); // different colours only
        assert_eq!(text, "a **bX** c");
        assert_eq!(cur_a, cur_b);
        assert_eq!(xs_a, xs_b, "hit-test geometry must not change with colours");
        let undo = Event::Key {
            key: Key::Z,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::COMMAND,
        };
        frame(&mut text, vec![undo], false);
        assert_eq!(text, src0, "undo history survives a colour change");
    }

    const BLOCKS: &str = "# T\n\n> quoted *e*\n> more\n\n---\n\n```rs\nlet k = \"ů🙂\";\n```\nafter\n\nSetext\n===\n<!-- raw -->\n";

    fn hidden_at(src: &str, caret: usize, byte: usize) -> bool {
        let img = |_: usize, _: &str, _: Option<u16>| None;
        compute_flags(src, &parse(src), &params(caret..caret, &img)).0[byte].hidden
    }

    #[test]
    fn block_constructs_keep_source_and_conceal_by_caret() {
        let src = BLOCKS;
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let spans = parse(src);
        for caret in (0..=src.len()).filter(|c| src.is_char_boundary(*c)) {
            let (job, _) = build_job(src, &spans, &params(caret..caret, &img), 400.0);
            assert_eq!(job.text, src, "caret {caret}");
        }
        let quote = src.find("> quoted").unwrap();
        let fence = src.find("```rs").unwrap();
        let body = src.find("let k").unwrap();
        let close = src.find("```\nafter").unwrap();
        let after = src.find("after").unwrap();
        let rule = src.find("---").unwrap();
        let underline = src.find("===").unwrap();
        // Caret far away (document start): everything concealed.
        for b in [quote, quote + 1, fence, close, rule, underline] {
            assert!(hidden_at(src, 0, b), "byte {b} should be concealed");
        }
        // Caret on the quote line reveals only that line's marker.
        assert!(!hidden_at(src, quote + 4, quote));
        let line2 = src.find("> more").unwrap();
        assert!(hidden_at(src, quote + 4, line2));
        // Caret anywhere in the block (incl. body, end of closing fence) reveals both fences ...
        for c in [fence, body, close, close + 3] {
            assert!(
                !hidden_at(src, c, fence) && !hidden_at(src, c, close),
                "caret {c}"
            );
        }
        // ... but a caret at the start of the next line does not.
        assert!(hidden_at(src, after, fence) && hidden_at(src, after, close));
        // Setext: caret on the title reveals the underline; thematic break reveals on its own line.
        assert!(!hidden_at(src, underline - 3, underline));
        assert!(!hidden_at(src, rule + 1, rule));
        // Fence body is never concealed.
        assert!(!hidden_at(src, 0, body));
    }

    #[test]
    fn no_caret_position_lands_inside_concealed_text() {
        // Invariant: for every caret position, bytes adjacent to the caret are never hidden.
        let src = BLOCKS;
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let spans = parse(src);
        for caret in (0..=src.len()).filter(|c| src.is_char_boundary(*c)) {
            let (flags, _) = compute_flags(src, &spans, &params(caret..caret, &img));
            let next = src[caret..].chars().next().map_or(0, char::len_utf8);
            let prev = src[..caret].chars().next_back().map_or(0, char::len_utf8);
            // A caret that sits *inside* a hidden marker (not at its edge) would be a phantom stop.
            if prev > 0 && next > 0 && flags[caret - prev].hidden && flags[caret].hidden {
                panic!("caret {caret} sits between two concealed chars");
            }
        }
    }

    #[test]
    fn decorations_follow_activity() {
        let src = BLOCKS;
        let spans = parse(src);
        let far = decorations(&spans, &(0..0));
        assert!(far
            .iter()
            .any(|d| matches!(d, Decor::Quote { depth: 0, .. })));
        assert!(far.iter().any(|d| matches!(d, Decor::Rule { .. })));
        let body = src.find("let k").unwrap();
        let cb = |d: &Vec<Decor>| {
            d.iter().find_map(|d| {
                if let Decor::CodeBlock { range } = d {
                    Some(range.clone())
                } else {
                    None
                }
            })
        };
        let inactive = cb(&far).unwrap();
        assert_eq!(
            inactive.start, body,
            "inactive surface covers the body only"
        );
        let near = decorations(&spans, &(body..body));
        assert!(
            cb(&near).unwrap().start < body,
            "active surface includes the fences"
        );
        let rule = src.find("---").unwrap();
        assert!(!decorations(&spans, &(rule..rule))
            .iter()
            .any(|d| matches!(d, Decor::Rule { .. })));
    }

    #[test]
    fn block_styles_use_semantic_colours() {
        let src = "# A\n\n## B\n\n> q\n\n```\ncode\n```\n";
        let spans = parse(src);
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let mut p = params(0..0, &img);
        p.heading_colors = [
            Color32::from_rgb(1, 0, 0),
            Color32::from_rgb(2, 0, 0),
            Color32::from_rgb(3, 0, 0),
            Color32::WHITE,
            Color32::WHITE,
            Color32::WHITE,
        ];
        p.quote_color = Color32::from_rgb(9, 9, 9);
        p.code_color = Color32::from_rgb(7, 7, 7);
        let (job, _) = build_job(src, &spans, &p, 300.0);
        let color_of = |needle: &str| {
            let at = src.find(needle).unwrap();
            job.sections
                .iter()
                .find(|s| s.byte_range.start.0 <= at && at < s.byte_range.end.0)
                .unwrap()
                .format
                .color
        };
        assert_eq!(color_of("A"), p.heading_colors[0]);
        assert_eq!(color_of("B"), p.heading_colors[1]);
        assert_eq!(color_of("q"), p.quote_color);
        assert_eq!(color_of("code"), p.code_color);
    }

    const DOC: &str = "# Table experiment\n\nParagraph before the table.\n\n| Name | Role | Status |\n| --- | :---: | ---: |\n| Alice | Příliš | Active |\n| Bob | 🦀 | Pending |\n\nParagraph after the table.\n";

    fn table_span(src: &str) -> Span {
        parse(src)
            .into_iter()
            .find(|s| matches!(s.kind, Kind::Table(_)))
            .unwrap()
    }

    /// Caret before / inside / after the table, selections crossing its boundaries, and the
    /// one-line-reserved-height layout, all with the text byte-identical.
    #[test]
    fn table_conceals_unless_caret_or_selection_touches_it() {
        let spans = parse(DOC);
        let sp = table_span(DOC);
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let hidden_count = |sel: Range<usize>| {
            let (f, _) = compute_flags(DOC, &spans, &params(sel, &img));
            sp.range.clone().filter(|i| f[*i].hidden).count()
        };
        let (start, end) = (sp.reveal.start, sp.reveal.end);
        let all = sp.range.len();
        // Before (blank line above), after (blank line below): concealed.
        assert_eq!(hidden_count(start - 1..start - 1), all);
        assert_eq!(hidden_count(end + 1..end + 1), all);
        assert_eq!(hidden_count(0..0), all);
        // Touching the first/last source byte, or anywhere inside: raw source.
        for c in [start, start + 5, (start + end) / 2, end] {
            assert_eq!(hidden_count(c..c), 0, "caret {c}");
        }
        // Selections that cross either boundary (or span the whole table) reveal it.
        assert_eq!(hidden_count(start - 5..start + 1), 0);
        assert_eq!(hidden_count(end - 1..end + 5), 0);
        assert_eq!(hidden_count(0..DOC.len()), 0);
        // A selection that stays outside leaves it rendered.
        assert_eq!(hidden_count(0..start - 2), all);
    }

    #[test]
    fn table_reserves_height_on_first_line_only_and_text_is_unchanged() {
        let spans = parse(DOC);
        let sp = table_span(DOC);
        let img = |_: usize, _: &str, _: Option<u16>| None;
        for sel in [0..0, sp.reveal.start..sp.reveal.start, DOC.len()..DOC.len()] {
            let (job, _) = build_job(DOC, &spans, &params(sel, &img), 400.0);
            assert_eq!(job.text, DOC);
        }
        let (f, _) = compute_flags(DOC, &spans, &params(0..0, &img));
        let Kind::Table(t) = &sp.kind else {
            unreachable!()
        };
        let tall: Vec<usize> = (0..DOC.len())
            .filter(|i| f[*i].row_height == Some(90.0))
            .collect();
        // Only the header line (and its break) carries the reserved height.
        assert_eq!(tall.first(), Some(&t.header.range.start));
        assert_eq!(tall.last(), Some(&t.header.range.end));
        assert!(!f[t.separator.start].row_height.is_some());
        // Source lines are 0.1 pt tall: the galley is a few points taller, not 4 text rows taller.
        let ctx = egui::Context::default();
        let mut heights = (0.0, 0.0);
        for (k, sel) in [(0, 0..0), (1, sp.reveal.start..sp.reveal.start)] {
            let (job, _) = build_job(DOC, &spans, &params(sel, &img), 400.0);
            let mut o = ctx.run_ui(Default::default(), |ui| {
                let g = ui.ctx().fonts_mut(|f| f.layout_job(job.clone()));
                if k == 0 {
                    heights.0 = g.size().y
                } else {
                    heights.1 = g.size().y
                }
            });
            o.textures_delta.clear();
        }
        // Rendered: reserved 90 pt row replaces four source rows of body leading (15 pt text).
        assert!(
            (heights.0 - heights.1 - (90.0 - 4.0 * 15.0 * BODY_LEADING)).abs() < 12.0,
            "{heights:?}"
        );
    }

    /// Editing the paragraphs around a rendered table and undoing/redoing: the table source is
    /// never touched and the TextEdit undoer stays coherent while the table is concealed.
    #[test]
    fn undo_redo_around_a_rendered_table() {
        use egui::{self, text::CCursor, text::CCursorRange, Event, Key, Modifiers, RawInput};
        let ctx = egui::Context::default();
        let id = egui::Id::unique("table-undo");
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let mut text = DOC.to_owned();
        let mut t = 0.0;
        let mut frame = |text: &mut String, events: Vec<Event>| {
            t += 2.0;
            let mut input = RawInput {
                time: Some(t),
                events,
                ..Default::default()
            };
            input.focused = true;
            let mut out = ctx.run_ui(input, |ui| {
                let mut layouter = |ui: &egui::Ui, buf: &dyn egui::TextBuffer, wrap: f32| {
                    let src = buf.as_str();
                    let sel = egui::TextEdit::load_state(ui.ctx(), id)
                        .and_then(|s| s.cursor.char_range())
                        .map_or(0..0, |r| {
                            let (a, b) = (r.primary.index.0, r.secondary.index.0);
                            let b2 =
                                |c: usize| src.char_indices().nth(c).map_or(src.len(), |(i, _)| i);
                            b2(a.min(b))..b2(a.max(b))
                        });
                    let (job, _) = build_job(src, &parse(src), &params(sel, &img), wrap);
                    ui.ctx().fonts_mut(|f| f.layout_job(job))
                };
                egui::TextEdit::multiline(text)
                    .id(id)
                    .layouter(&mut layouter)
                    .show(ui)
                    .response
                    .request_focus();
            });
            out.textures_delta.clear();
        };
        frame(&mut text, vec![]);
        // Caret at the end of "Paragraph before the table." (char index == byte index here).
        let c = DOC.find("table.\n").unwrap() + 6;
        let mut st = egui::TextEdit::load_state(&ctx, id).unwrap_or_default();
        st.cursor
            .set_char_range(Some(CCursorRange::one(CCursor::new(c))));
        st.store(&ctx, id);
        frame(&mut text, vec![Event::Text("!".into())]);
        assert!(text.contains("table.!\n"));
        assert_eq!(text.replace('!', ""), DOC, "table source untouched");
        let key = |k, m| Event::Key {
            key: k,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: m,
        };
        frame(&mut text, vec![key(Key::Z, Modifiers::COMMAND)]);
        assert_eq!(text, DOC);
        frame(
            &mut text,
            vec![key(Key::Z, Modifiers::COMMAND | Modifiers::SHIFT)],
        );
        assert!(text.contains("table.!\n"));
    }

    fn flags_at(src: &str, sel: Range<usize>) -> Vec<Flags> {
        let img = |_: usize, _: &str, _: Option<u16>| None;
        compute_flags(src, &parse(src), &params(sel, &img)).0
    }

    #[test]
    fn strikethrough_is_styled_and_its_delimiters_follow_the_caret() {
        let src = "a ~~Příliš~~ b";
        let away = flags_at(src, src.len()..src.len());
        assert!(away[2].hidden && away[3].hidden, "opening ~~ concealed");
        assert!(away[4].strike && !away[4].hidden && !away[0].strike);
        let near = flags_at(src, 6..6);
        assert!(near[2].dim && !near[2].hidden, "revealed while editing");
        // Layout text is untouched and the strike reaches the format.
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let (job, _) = build_job(src, &parse(src), &params(src.len()..src.len(), &img), 300.0);
        assert_eq!(job.text, src);
        assert!(job
            .sections
            .iter()
            .any(|s| s.format.strikethrough.width > 0.0));
    }

    #[test]
    fn strikethrough_nests_with_bold_italic_headings_and_links() {
        let src = "# ~~**H**~~ *a ~~b~~* [~~l~~](u)";
        let f = flags_at(src, src.len()..src.len());
        let at = |needle: &str| f[src.find(needle).unwrap()];
        assert!(at("H").strike && at("H").bold && at("H").heading == 1);
        assert!(at("b~~*").strike && at("b~~*").italic);
        assert!(at("l~~").strike && at("l~~").link);
    }

    #[test]
    fn task_boxes_hide_their_glyphs_unless_touched() {
        let src = "- [ ] a\n- [x] Příliš\n";
        let away = flags_at(src, src.len()..src.len());
        assert!(away[2..5].iter().all(|f| f.task_box));
        assert!(!away[6].task_box, "text stays visible");
        assert!(away[0].list_marker, "list marker styling is kept");
        let on = flags_at(src, 3..3);
        assert!(
            on[2..5].iter().all(|f| !f.task_box && f.dim),
            "raw box while editing it"
        );
        // Same text either way.
        let img = |_: usize, _: &str, _: Option<u16>| None;
        let (job, _) = build_job(src, &parse(src), &params(0..0, &img), 300.0);
        assert_eq!(job.text, src);
    }

    #[test]
    fn empty_fenced_code_block_keeps_a_visible_row() {
        let src = "before\n\n```\n```\n\nafter\n";
        let away = flags_at(src, 0..0);
        let open = src.find("```").unwrap();
        assert!(away[open].hidden && away[open].row_height.is_some_and(|h| h > 10.0));
        let spans = parse(src);
        let d = decorations(&spans, &(0..0));
        assert!(d
            .iter()
            .any(|d| matches!(d, Decor::CodeBlock { range } if range.start == open)));
        // A non-empty block does not reserve extra height.
        let full = flags_at("```\ncode\n```\n", 20..20);
        assert!(full.iter().all(|f| f.row_height.is_none()));
        // Touching it shows the raw fences.
        let on = flags_at(src, open..open);
        assert!(!on[open].hidden && on[open].row_height.is_none());
    }
}
