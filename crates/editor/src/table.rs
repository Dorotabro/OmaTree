//! Read-only visual layout and painting of a GFM table (P1 spike).
//!
//! Cells carry inline Markdown (T3): the parser's own inline spans (`Table::inline`) say which bytes
//! of a cell are bold, italic, struck through, code or a link and which are delimiters; [`runs`]
//! turns a cell into styled runs of display text (delimiters concealed, backslash escapes
//! resolved) and one `LayoutJob` per cell mixes the faces, so measuring and painting see the same
//! text. The source is never changed and nothing is parsed here.
//!
//! Layout only: the table is never an editing surface. The editor conceals the table's source
//! lines (see `layout.rs`), reserves `layout().height + TABLE_PAD` on the first concealed row, and
//! the app paints the result into that row. Galleys use `Color32::PLACEHOLDER` for ordinary text,
//! so its colour is applied at paint time and a theme change never invalidates a cached layout.

use crate::markdown::{Align, Kind, Span, Table};
use egui::{
    text::{LayoutJob, TextWrapping},
    Color32, Context, FontFamily, FontId, Galley, Painter, Pos2, Rect, Stroke, StrokeKind, Vec2,
};
use std::sync::Arc;

/// Vertical room added around the table inside the reserved row.
pub const TABLE_PAD: f32 = 12.0;
const PAD_X: f32 = 10.0;
const PAD_Y: f32 = 5.0;
/// Narrowest a column is squeezed to before the whole table is scaled down.
const MIN_COL: f32 = 56.0;
const HARD_MIN_COL: f32 = 24.0;

pub struct TableLayout {
    /// Fonts and line metrics measured once with the layout (they change only with the fonts,
    /// which clear the cache).
    prepared: Prepared,
    /// Styled display runs of every cell, header first (see [`runs`]); theme-independent.
    pub cells: Vec<Vec<Vec<Run>>>,
    pub cols: Vec<f32>,
    /// Row heights, header first.
    pub rows: Vec<f32>,
    pub width: f32,
    pub height: f32,
}

pub struct Colors {
    pub text: Color32,
    pub code_text: Color32,
    pub code_bg: Color32,
    pub link: Color32,
    pub header_bg: Color32,
    pub border: Color32,
    pub header_rule: Color32,
}

/// Display text of a cell: trimmed source, `\|` unescaped. Inline Markdown is shown as written.
/// (The grid shows [`runs`] instead; this is the plain-text form.)
pub fn cell_text(src: &str, r: &std::ops::Range<usize>) -> String {
    src.get(r.clone()).unwrap_or("").trim().replace("\\|", "|")
}

/// A stretch of a cell's display text in one style.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Run {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
    pub link: bool,
}

/// Test switch: render cells without inline styling (see the negative controls in `table_tests`).
#[cfg(test)]
pub(crate) mod tests_off {
    thread_local!(static OFF: std::cell::Cell<bool> = const { std::cell::Cell::new(false) });
    pub fn get() -> bool {
        OFF.with(|o| o.get())
    }
    pub fn set(v: bool) {
        OFF.with(|o| o.set(v));
    }
}

const BOLD: u8 = 1;
const ITALIC: u8 = 2;
const STRIKE: u8 = 4;
const CODE: u8 = 8;
const LINK: u8 = 16;
const HIDDEN: u8 = 32;

/// The display runs of the cell at `cell` (a range from `Table`, spaces included): the trimmed
/// source with every delimiter of an inline span in `inline` removed and the span's style applied
/// to what it encloses, outside code a backslash before ASCII punctuation dropped (the
/// punctuation stays, as a literal), and `\|` read as `|` everywhere, as GFM does in a cell.
/// Anything the parser did not make a span of stays as written.
pub fn runs(src: &str, cell: &std::ops::Range<usize>, inline: &[Span]) -> Vec<Run> {
    #[cfg(test)]
    if tests_off::get() {
        // Negative control: the grid as it was before T3, markers shown as written.
        let text = cell_text(src, cell);
        return if text.is_empty() {
            Vec::new()
        } else {
            vec![Run {
                text,
                ..Default::default()
            }]
        };
    }
    let Some(raw) = src.get(cell.clone()) else {
        return Vec::new();
    };
    let a = cell.start + (raw.len() - raw.trim_start().len());
    let b = a + raw.trim().len();
    if a >= b {
        return Vec::new();
    }
    let mut flags = vec![0u8; b - a];
    let mut mark = |r: &std::ops::Range<usize>, f: u8| {
        if r.start >= a && r.end <= b {
            for x in &mut flags[r.start - a..r.end - a] {
                *x |= f;
            }
        }
    };
    for s in inline {
        if s.range.start < a || s.range.end > b {
            continue;
        }
        match s.kind {
            Kind::Strong => mark(&s.range, BOLD),
            Kind::Emphasis => mark(&s.range, ITALIC),
            Kind::Strikethrough => mark(&s.range, STRIKE),
            Kind::Code => mark(&s.range, CODE),
            Kind::Link { .. } => mark(&s.range, LINK),
            _ => continue,
        }
        for m in &s.markers {
            mark(m, HIDDEN);
        }
    }
    let text = &src[a..b];
    let mut out: Vec<Run> = Vec::new();
    let mut push = |c: char, f: u8| {
        let same = |r: &Run| {
            r.bold == (f & BOLD != 0)
                && r.italic == (f & ITALIC != 0)
                && r.strike == (f & STRIKE != 0)
                && r.code == (f & CODE != 0)
                && r.link == (f & LINK != 0)
        };
        match out.last_mut() {
            Some(r) if same(r) => r.text.push(c),
            _ => out.push(Run {
                text: c.to_string(),
                bold: f & BOLD != 0,
                italic: f & ITALIC != 0,
                strike: f & STRIKE != 0,
                code: f & CODE != 0,
                link: f & LINK != 0,
            }),
        }
    };
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let f = flags[i];
        if f & HIDDEN != 0 {
            continue;
        }
        if c == '\\' {
            if let Some(&(j, n)) = chars.peek() {
                let escapes = n == '|' || (f & CODE == 0 && n.is_ascii_punctuation());
                if escapes && flags[j] & HIDDEN == 0 {
                    // The backslash is concealed; the next character is literal text.
                    chars.next();
                    push(n, flags[j]);
                    continue;
                }
            }
        }
        push(c, f);
    }
    out
}

/// A hash of the inline structure the parser found in a table, relative to its start (`base`).
/// The cached layouts are keyed by the table's source; the same source can parse differently in
/// another document (a reference link whose definition came or went), and this tells them apart.
/// Computed once by the parser (`Table::inline_sig`).
pub fn signature_of(base: usize, inline: &[Span]) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for s in inline {
        std::mem::discriminant(&s.kind).hash(&mut h);
        (s.range.start - base, s.range.end - base).hash(&mut h);
        for m in &s.markers {
            (m.start - base, m.end - base).hash(&mut h);
        }
    }
    h.finish()
}

/// Column widths (padding included) for natural widths `nat`, fitting `max` where possible.
pub fn column_widths(nat: &[f32], max: f32) -> Vec<f32> {
    let total: f32 = nat.iter().sum();
    if total <= max || nat.is_empty() {
        return nat.to_vec();
    }
    let min: Vec<f32> = nat.iter().map(|n| n.min(MIN_COL)).collect();
    let min_sum: f32 = min.iter().sum();
    if min_sum >= max {
        // Even the minimums do not fit: scale evenly, never below a hard floor.
        let k = max / min_sum;
        return min.iter().map(|m| (m * k).max(HARD_MIN_COL)).collect();
    }
    let extra_total: f32 = nat.iter().zip(&min).map(|(n, m)| n - m).sum();
    let room = max - min_sum;
    nat.iter()
        .zip(&min)
        .map(|(n, m)| m + (n - m) * room / extra_total)
        .collect()
}

/// Fonts and metrics the cell jobs are built from.
#[derive(Clone)]
struct Prepared {
    body: FontId,
    head: FontId,
    head_extra: f32,
    bold: Option<FontFamily>,
    /// `fonts::bold_drop` of `bold` (ems); zero without one.
    bold_drop: f32,
    /// Row height of the body and the header face: the line box shared by a cell's mixed faces.
    body_line: f32,
    head_line: f32,
    base: f32,
}

impl Prepared {
    fn new(ctx: &Context, base: f32, bold: Option<&FontFamily>) -> Self {
        let body = FontId::new(base, FontFamily::Proportional);
        let head = FontId::new(base, bold.cloned().unwrap_or(FontFamily::Proportional));
        let (body_line, head_line) = ctx.fonts_mut(|f| (f.row_height(&body), f.row_height(&head)));
        Prepared {
            body,
            head,
            head_extra: if bold.is_some() { 0.0 } else { base * 0.04 },
            bold: bold.cloned(),
            bold_drop: bold.map_or(0.0, |b| crate::fonts::bold_drop(ctx, b)),
            body_line,
            head_line,
            base,
        }
    }

    /// One cell as a layout job. Colours are set here for the styles that have their own (code,
    /// links, strikethrough) and left as `PLACEHOLDER` for ordinary text, which is painted in the
    /// table's text colour; `c` is `None` when only measuring. A cell of plain text is one section
    /// in the face it always had; a cell that mixes faces gets one explicit line box for all of
    /// them, with the bold face's baseline shifted exactly as in the document text
    /// (`layout::format_for`), so the faces share a baseline and the row is as tall as its
    /// tallest face.
    fn job(&self, cell: &[Run], header: bool, c: Option<&Colors>, wrap: f32) -> LayoutJob {
        let (face, extra, line) = if header {
            (&self.head, self.head_extra, self.head_line)
        } else {
            (&self.body, 0.0, self.body_line)
        };
        let shifted = |r: &Run| r.bold && !header && self.bold.is_some() && !r.code;
        let mixed = cell.iter().any(|r| r.code || shifted(r));
        let mut j = LayoutJob {
            wrap: TextWrapping {
                max_width: wrap,
                max_rows: usize::MAX,
                break_anywhere: false,
                overflow_character: None,
            },
            ..Default::default()
        };
        let ink = |pick: fn(&Colors) -> Color32| c.map_or(Color32::PLACEHOLDER, pick);
        let empty = Run::default();
        let list = if cell.is_empty() {
            std::slice::from_ref(&empty)
        } else {
            cell
        };
        for r in list {
            let mut f = egui::TextFormat {
                font_id: face.clone(),
                color: Color32::PLACEHOLDER,
                extra_letter_spacing: extra,
                italics: r.italic,
                ..Default::default()
            };
            if r.bold && !header {
                match &self.bold {
                    Some(b) => f.font_id = FontId::new(self.base, b.clone()),
                    None => f.extra_letter_spacing = self.base * 0.04,
                }
            }
            if r.code {
                // Code keeps its own look: monospace at the document's code size, no emphasis.
                f.font_id = FontId::new(self.base * 0.92, FontFamily::Monospace);
                f.extra_letter_spacing = 0.0;
                f.color = ink(|c| c.code_text);
                f.background = ink(|c| c.code_bg);
            }
            if r.link {
                f.color = ink(|c| c.link);
                f.underline = Stroke::new(1.0, f.color);
            }
            if r.strike {
                let colour = if r.link || r.code {
                    f.color
                } else {
                    ink(|c| c.text)
                };
                f.strikethrough = Stroke::new(1.0, colour);
            }
            if mixed {
                f.line_height = Some(line);
                if shifted(r) && self.bold_drop != 0.0 {
                    let shift = self.bold_drop * self.base;
                    f.line_height = Some(line - 2.0 * shift.abs());
                    f.valign = if shift > 0.0 {
                        egui::Align::Max
                    } else {
                        egui::Align::Min
                    };
                }
            }
            j.append(&r.text, 0.0, f);
        }
        j
    }

    /// Galleys for every cell at the given column widths. Never stored across frames: egui
    /// rebuilds its font atlas when e.g. the light/dark glyph gamma changes, which would leave
    /// retained galleys pointing at stale glyph positions. egui's own galley cache makes
    /// re-requesting them each frame cheap.
    fn galleys(
        &self,
        ctx: &Context,
        cells: &[Vec<Vec<Run>>],
        cols: &[f32],
        c: Option<&Colors>,
    ) -> Vec<Vec<Arc<Galley>>> {
        ctx.fonts_mut(|f| {
            cells
                .iter()
                .enumerate()
                .map(|(r, row)| {
                    row.iter()
                        .zip(cols)
                        .map(|(cell, w)| {
                            f.layout_job(self.job(cell, r == 0, c, (w - 2.0 * PAD_X).max(1.0)))
                        })
                        .collect()
                })
                .collect()
        })
    }
}

/// Measure a table: column widths (fit to `max_width`), row heights and total size.
pub fn layout(
    ctx: &Context,
    src: &str,
    t: &Table,
    max_width: f32,
    base: f32,
    bold: Option<&FontFamily>,
) -> TableLayout {
    let ncols = t.columns().max(1);
    let p = Prepared::new(ctx, base, bold);
    let cells: Vec<Vec<Vec<Run>>> = std::iter::once(&t.header)
        .chain(&t.rows)
        .map(|r| {
            (0..ncols)
                .map(|c| {
                    r.cells
                        .get(c)
                        .map_or_else(Vec::new, |r| runs(src, r, &t.inline))
                })
                .collect()
        })
        .collect();
    let mut nat = vec![0.0f32; ncols];
    ctx.fonts_mut(|f| {
        for (r, row) in cells.iter().enumerate() {
            for (c, n) in nat.iter_mut().enumerate() {
                let g = f.layout_job(p.job(&row[c], r == 0, None, f32::INFINITY));
                *n = n.max(g.size().x + 2.0 * PAD_X);
            }
        }
    });
    let cols = column_widths(&nat, max_width.max(HARD_MIN_COL * ncols as f32));
    let rows: Vec<f32> = p
        .galleys(ctx, &cells, &cols, None)
        .iter()
        .map(|row| row.iter().map(|g| g.size().y).fold(0.0, f32::max) + 2.0 * PAD_Y)
        .collect();
    TableLayout {
        prepared: p,
        width: cols.iter().sum(),
        height: rows.iter().sum(),
        cells,
        cols,
        rows,
    }
}

/// Paint `l` (from [`layout`] with the same arguments) with its top-left at `origin`.
pub fn paint(painter: &Painter, origin: Pos2, l: &TableLayout, t: &Table, c: &Colors) {
    let cells = l
        .prepared
        .galleys(painter.ctx(), &l.cells, &l.cols, Some(c));
    let outer = Rect::from_min_size(origin, Vec2::new(l.width, l.height));
    let head_h = l.rows.first().copied().unwrap_or(0.0);
    painter.rect_filled(
        Rect::from_min_size(origin, Vec2::new(l.width, head_h)),
        0.0,
        c.header_bg,
    );
    let grid = Stroke::new(1.0, c.border);
    let mut y = origin.y;
    for (ri, row) in cells.iter().enumerate() {
        let mut x = origin.x;
        for (ci, g) in row.iter().enumerate() {
            let w = l.cols[ci];
            let inner = w - 2.0 * PAD_X;
            let dx = match t.align.get(ci).copied().unwrap_or(Align::None) {
                Align::Center => ((inner - g.size().x) / 2.0).max(0.0),
                Align::Right => (inner - g.size().x).max(0.0),
                _ => 0.0,
            };
            // Clip to the cell: an unbreakable word wider than a squeezed column must not spill.
            let cell = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, l.rows[ri]));
            painter
                .with_clip_rect(cell.intersect(painter.clip_rect()))
                .galley(Pos2::new(x + PAD_X + dx, y + PAD_Y), g.clone(), c.text);
            x += w;
            if ci + 1 < row.len() {
                painter.line_segment(
                    [Pos2::new(x, origin.y), Pos2::new(x, origin.y + l.height)],
                    grid,
                );
            }
        }
        y += l.rows[ri];
        if ri + 1 < cells.len() {
            let s = if ri == 0 {
                Stroke::new(1.5, c.header_rule)
            } else {
                grid
            };
            painter.line_segment(
                [Pos2::new(origin.x, y), Pos2::new(origin.x + l.width, y)],
                s,
            );
        }
    }
    painter.rect_stroke(outer, 2.0, grid, StrokeKind::Inside);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markdown::{parse, Kind};

    const DOC: &str = "| Name | Role | Status |\n| --- | :---: | ---: |\n| Alice | Developer | Active |\n| Bob | Analyst | Pending |\n";

    fn table(src: &str) -> Table {
        match parse(src).into_iter().find_map(|s| match s.kind {
            Kind::Table(t) => Some(t),
            _ => None,
        }) {
            Some(t) => t,
            None => panic!("no table in {src:?}"),
        }
    }

    fn run(ctx: &Context, src: &str, w: f32) -> TableLayout {
        let t = table(src);
        let mut out = None;
        let mut o = ctx.run_ui(Default::default(), |ui| {
            out = Some(layout(ui.ctx(), src, &t, w, 16.0, None));
        });
        o.textures_delta.clear();
        out.unwrap()
    }

    #[test]
    fn natural_and_long_content_layout() {
        let ctx = Context::default();
        let l = run(&ctx, DOC, 780.0);
        assert_eq!(l.cols.len(), 3);
        assert_eq!(l.rows.len(), 3);
        assert!(l.width <= 780.0 && l.width > 150.0);
        // Alone, short cells stay on one line: all body rows have the same height.
        assert!((l.rows[1] - l.rows[2]).abs() < 0.01);

        let long = format!(
            "| A | B |\n| - | - |\n| {} | short |\n",
            "very long cell content ".repeat(20)
        );
        let wide = run(&ctx, &long, 700.0);
        assert!(wide.width <= 700.0 + 0.01, "{}", wide.width);
        assert!(
            wide.rows[1] > wide.rows[0] * 2.0,
            "long cell wraps: {:?}",
            wide.rows
        );
        // Narrow editor: still fits, never collapses below the hard floor, no panic.
        for w in [300.0, 120.0, 40.0] {
            let n = run(&ctx, &long, w);
            assert!(
                n.cols.iter().all(|c| *c >= HARD_MIN_COL - 0.01),
                "{w}: {:?}",
                n.cols
            );
            assert!(
                n.width <= w.max(HARD_MIN_COL * 2.0) + 0.5,
                "{w}: {}",
                n.width
            );
            assert!(n.height.is_finite() && n.height > 0.0);
        }
    }

    #[test]
    fn column_width_fitting() {
        assert_eq!(column_widths(&[50.0, 60.0], 200.0), vec![50.0, 60.0]);
        let w = column_widths(&[400.0, 80.0, 60.0], 300.0);
        assert!((w.iter().sum::<f32>() - 300.0).abs() < 0.01, "{w:?}");
        assert!(w[0] < 400.0 && w[1] >= MIN_COL.min(80.0) && w[2] >= MIN_COL.min(60.0) - 0.01);
        assert!(column_widths(&[], 10.0).is_empty());
    }

    #[test]
    fn cell_text_trims_and_unescapes() {
        let src = "| a \\| b |  Příliš ž |\n| - | - |\n";
        let t = table(src);
        assert_eq!(cell_text(src, &t.header.cells[0]), "a | b");
        assert_eq!(cell_text(src, &t.header.cells[1]), "Příliš ž");
    }

    /// Not an assertion of speed, a record: layout cost of a 200-row, 6-column table.
    #[test]
    fn large_table_layout_cost_is_recorded() {
        let mut src = String::from("| a | b | c | d | e | f |\n|---|---|---|---|---|---|\n");
        for r in 0..200 {
            src.push_str(&format!(
                "| row {r} | Příliš žluťoučký kůň | x | y | some longer text here | {r} |\n"
            ));
        }
        let ctx = Context::default();
        let (mut first, mut second) = (0.0, 0.0);
        for k in 0..2 {
            let t0 = std::time::Instant::now();
            let l = run(&ctx, &src, 780.0);
            assert_eq!(l.rows.len(), 201);
            let ms = t0.elapsed().as_secs_f64() * 1000.0;
            if k == 0 {
                first = ms
            } else {
                second = ms
            }
        }
        eprintln!("table layout 200x6 (debug build): first {first:.1} ms, repeat {second:.1} ms");
    }
}
