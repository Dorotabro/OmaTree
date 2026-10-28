//! Read-only visual layout and painting of a GFM table (P1 spike).
//!
//! Layout only: the table is never an editing surface. The editor conceals the table's source
//! lines (see `layout.rs`), reserves `layout().height + TABLE_PAD` on the first concealed row, and
//! the app paints the result into that row. Galleys use `Color32::PLACEHOLDER`, so colours are
//! applied at paint time and a theme change never invalidates a cached layout.

use crate::markdown::{Align, Table};
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
    pub cols: Vec<f32>,
    /// Row heights, header first.
    pub rows: Vec<f32>,
    pub width: f32,
    pub height: f32,
}

pub struct Colors {
    pub text: Color32,
    pub header_bg: Color32,
    pub border: Color32,
    pub header_rule: Color32,
}

/// Display text of a cell: trimmed source, `\|` unescaped. Inline Markdown is shown as written.
pub fn cell_text(src: &str, r: &std::ops::Range<usize>) -> String {
    src.get(r.clone()).unwrap_or("").trim().replace("\\|", "|")
}

fn job(text: String, font: FontId, wrap: f32, extra: f32) -> LayoutJob {
    let mut j = LayoutJob::simple(text, font, Color32::PLACEHOLDER, wrap);
    j.wrap = TextWrapping {
        max_width: wrap,
        max_rows: usize::MAX,
        break_anywhere: false,
        overflow_character: None,
    };
    for s in &mut j.sections {
        s.format.extra_letter_spacing = extra;
    }
    j
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

struct Prepared {
    texts: Vec<Vec<String>>,
    body: FontId,
    head: FontId,
    head_extra: f32,
}

impl Prepared {
    fn new(src: &str, t: &Table, base: f32, bold: Option<&FontFamily>) -> Self {
        let ncols = t.columns().max(1);
        let texts = std::iter::once(&t.header)
            .chain(&t.rows)
            .map(|r| {
                (0..ncols)
                    .map(|c| r.cells.get(c).map_or(String::new(), |r| cell_text(src, r)))
                    .collect()
            })
            .collect();
        Prepared {
            texts,
            body: FontId::new(base, FontFamily::Proportional),
            head: FontId::new(base, bold.cloned().unwrap_or(FontFamily::Proportional)),
            head_extra: if bold.is_some() { 0.0 } else { base * 0.04 },
        }
    }

    fn job(&self, row: usize, col: usize, wrap: f32) -> LayoutJob {
        let (font, extra) = if row == 0 {
            (self.head.clone(), self.head_extra)
        } else {
            (self.body.clone(), 0.0)
        };
        job(self.texts[row][col].clone(), font, wrap, extra)
    }

    /// Galleys for every cell at the given column widths. Never stored across frames: egui
    /// rebuilds its font atlas when e.g. the light/dark glyph gamma changes, which would leave
    /// retained galleys pointing at stale glyph positions. egui's own galley cache makes
    /// re-requesting them each frame cheap.
    fn galleys(&self, ctx: &Context, cols: &[f32]) -> Vec<Vec<Arc<Galley>>> {
        ctx.fonts_mut(|f| {
            (0..self.texts.len())
                .map(|r| {
                    (0..cols.len())
                        .map(|c| f.layout_job(self.job(r, c, (cols[c] - 2.0 * PAD_X).max(1.0))))
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
    let p = Prepared::new(src, t, base, bold);
    let mut nat = vec![0.0f32; ncols];
    ctx.fonts_mut(|f| {
        for r in 0..p.texts.len() {
            for (c, n) in nat.iter_mut().enumerate() {
                let g = f.layout_job(p.job(r, c, f32::INFINITY));
                *n = n.max(g.size().x + 2.0 * PAD_X);
            }
        }
    });
    let cols = column_widths(&nat, max_width.max(HARD_MIN_COL * ncols as f32));
    let rows: Vec<f32> = p
        .galleys(ctx, &cols)
        .iter()
        .map(|row| row.iter().map(|g| g.size().y).fold(0.0, f32::max) + 2.0 * PAD_Y)
        .collect();
    TableLayout {
        width: cols.iter().sum(),
        height: rows.iter().sum(),
        cols,
        rows,
    }
}

/// Paint `l` (from [`layout`] with the same arguments) with its top-left at `origin`.
#[allow(clippy::too_many_arguments)]
pub fn paint(
    painter: &Painter,
    origin: Pos2,
    l: &TableLayout,
    t: &Table,
    src: &str,
    base: f32,
    bold: Option<&FontFamily>,
    c: &Colors,
) {
    let cells = Prepared::new(src, t, base, bold).galleys(painter.ctx(), &l.cols);
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
