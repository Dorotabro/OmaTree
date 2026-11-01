//! Fast byte-offset and galley-position lookups for the per-frame decoration passes.
//!
//! The editor needs the screen position of many source offsets every frame (task boxes, tables,
//! rules, quote bars). Two lookups it used were linear: `str::chars().count()` over the text prefix
//! (byte offset to character index) and egui's `Galley::pos_from_cursor`, which scans the rows from
//! the top. With thousands of blocks in one note that made a frame cost seconds. These types give
//! the same answers in constant and logarithmic time.
//!
//! Nothing here keeps font-dependent data across frames except [`Offsets`], which depends only on
//! the text. [`RowIndex`] is built from one galley and must not outlive it (it is a per-frame value).

use egui::epaint::text::cursor::LayoutCursor;
use egui::text::{CCursor, CharIndex};
use egui::{Galley, Rect};
use std::ops::Range;

/// Bytes between the checkpoints of [`Offsets`].
const STRIDE: usize = 512;

/// Byte to character index for one text, computed once per text.
///
/// `marks[i]` is the number of characters that start before byte `i * STRIDE`; a query adds the
/// characters starting between that checkpoint and the queried byte (at most `STRIDE` bytes of
/// work). Pure ASCII text needs no table at all.
#[derive(Debug, Default)]
pub struct Offsets {
    ascii: bool,
    len: usize,
    marks: Vec<u32>,
}

fn starts_char(b: u8) -> bool {
    b & 0xC0 != 0x80
}

impl Offsets {
    pub fn build(text: &str) -> Self {
        if text.is_ascii() {
            return Offsets {
                ascii: true,
                len: text.len(),
                marks: Vec::new(),
            };
        }
        let mut marks = Vec::with_capacity(text.len() / STRIDE + 1);
        let mut chars = 0u32;
        for (i, b) in text.bytes().enumerate() {
            if i % STRIDE == 0 {
                marks.push(chars);
            }
            chars += u32::from(starts_char(b));
        }
        Offsets {
            ascii: false,
            len: text.len(),
            marks,
        }
    }

    /// Whether this index was built for a text of this length (a cheap staleness check; callers
    /// rebuild whenever the text itself changes).
    pub fn fits(&self, text: &str) -> bool {
        self.len == text.len()
    }

    /// `text[..byte].chars().count()`; `byte` must be a char boundary of the text this was built for.
    pub fn char_of(&self, text: &str, byte: usize) -> usize {
        debug_assert!(self.fits(text));
        let byte = byte.min(text.len());
        if self.ascii {
            return byte;
        }
        let block = byte / STRIDE;
        let from = block * STRIDE;
        self.marks[block] as usize
            + text.as_bytes()[from..byte]
                .iter()
                .filter(|b| starts_char(**b))
                .count()
    }

    /// The byte offset of character `chars` (`text.len()` past the end).
    pub fn byte_of(&self, text: &str, chars: usize) -> usize {
        if self.ascii {
            return chars.min(text.len());
        }
        // Last checkpoint with at most `chars` characters before it.
        let block = self
            .marks
            .partition_point(|m| (*m as usize) <= chars)
            .max(1)
            - 1;
        let mut seen = self.marks[block] as usize;
        let bytes = text.as_bytes();
        let mut i = block * STRIDE;
        while i < bytes.len() {
            if starts_char(bytes[i]) {
                if seen == chars {
                    return i;
                }
                seen += 1;
            }
            i += 1;
        }
        text.len()
    }
}

/// Character index to row/column for one galley, in logarithmic time.
///
/// Reproduces `Galley::layout_from_cursor` for a cursor with `prefer_next_row == false`: the first
/// row whose last character position is at or after the index (so an index at a soft wrap belongs
/// to the end of the earlier row, as in egui).
pub struct RowIndex<'g> {
    galley: &'g Galley,
    starts: Vec<usize>,
    ends: Vec<usize>,
}

impl<'g> RowIndex<'g> {
    pub fn new(galley: &'g Galley) -> Self {
        let mut starts = Vec::with_capacity(galley.rows.len());
        let mut ends = Vec::with_capacity(galley.rows.len());
        let mut at = 0usize;
        for row in &galley.rows {
            starts.push(at);
            ends.push(at + row.char_count_excluding_newline().0);
            at += row.char_count_including_newline().0;
        }
        RowIndex {
            galley,
            starts,
            ends,
        }
    }

    /// Same value as `galley.pos_from_cursor(CCursor::new(char_index))`.
    pub fn pos(&self, char_index: usize) -> Rect {
        let row = self.ends.partition_point(|e| *e < char_index);
        match self.starts.get(row) {
            Some(start) if *start <= char_index => {
                self.galley.pos_from_layout_cursor(&LayoutCursor {
                    row,
                    column: CharIndex(char_index - start),
                })
            }
            // Past the last row, or an index no row claims: let egui answer.
            _ => self.galley.pos_from_cursor(CCursor::new(char_index)),
        }
    }

    /// The character range of the rows that overlap the vertical span `y0..=y1` (galley space).
    pub fn chars_in_y(&self, y0: f32, y1: f32) -> Range<usize> {
        let rows = &self.galley.rows;
        let first = rows.partition_point(|r| r.max_y() < y0);
        let last = rows.partition_point(|r| r.min_y() <= y1);
        if first >= last {
            return 0..0;
        }
        self.starts[first]..self.ends[last - 1] + 1
    }
}

/// Everything a decoration pass needs to place source offsets: the text, its offset index, a row
/// index for the current galley and the visible range (in bytes), so off-screen blocks cost nothing.
pub struct Geo<'a> {
    pub text: &'a str,
    pub offsets: &'a Offsets,
    pub rows: RowIndex<'a>,
    /// Bytes of the source that are on screen, plus a margin.
    pub visible: Range<usize>,
}

impl<'a> Geo<'a> {
    /// `view` is the visible vertical span in galley space.
    pub fn new(text: &'a str, offsets: &'a Offsets, galley: &'a Galley, view: (f32, f32)) -> Self {
        let rows = RowIndex::new(galley);
        let chars = rows.chars_in_y(view.0, view.1);
        let visible = offsets.byte_of(text, chars.start)..offsets.byte_of(text, chars.end);
        Geo {
            text,
            offsets,
            rows,
            visible,
        }
    }

    /// Position (galley space, zero-width rect) of a source byte offset.
    pub fn pos(&self, byte: usize) -> Rect {
        self.rows.pos(self.offsets.char_of(self.text, byte))
    }

    /// Does this source range touch the visible part of the document?
    pub fn is_visible(&self, r: &Range<usize>) -> bool {
        r.end >= self.visible.start && r.start <= self.visible.end
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::text::LayoutJob;
    use egui::{Color32, Context, FontId};

    fn galley_of(text: &str, wrap: f32) -> std::sync::Arc<Galley> {
        let ctx = Context::default();
        let mut job = LayoutJob::simple(
            text.to_owned(),
            FontId::proportional(16.0),
            Color32::WHITE,
            wrap,
        );
        job.wrap.max_width = wrap;
        let mut g = None;
        let mut out = ctx.run_ui(Default::default(), |ui| {
            g = Some(ui.ctx().fonts_mut(|f| f.layout_job(job.clone())));
        });
        out.textures_delta.clear();
        g.unwrap()
    }

    const SAMPLE: &str = "Příliš žluťoučký kůň úpěl ďábelské ódy, a další věta se zalomí přes několik řádků.\n\nDruhý odstavec 🦀 s emoji 👨‍👩‍👧 a tabulátorem.\n\n\nPoslední řádek bez konce";

    #[test]
    fn offsets_agree_with_chars_count_and_char_indices_for_every_boundary() {
        for text in [
            "",
            "ascii only\nsecond line",
            SAMPLE,
            &SAMPLE.repeat(40),
            "ž",
            "a🦀b",
        ] {
            let o = Offsets::build(text);
            assert!(o.fits(text));
            for (n, (b, _)) in text.char_indices().chain([(text.len(), ' ')]).enumerate() {
                assert_eq!(o.char_of(text, b), text[..b].chars().count(), "char_of {b}");
                assert_eq!(o.byte_of(text, n), b, "byte_of {n}");
            }
            assert_eq!(o.byte_of(text, usize::MAX / 2), text.len());
        }
    }

    #[test]
    fn row_index_gives_the_same_rect_as_egui_for_every_character_index() {
        for (text, wrap) in [
            (SAMPLE.to_owned(), 220.0),
            (SAMPLE.repeat(5), 180.0),
            ("x".to_owned(), 50.0),
            (String::new(), 100.0),
            ("a\n".repeat(30), 100.0),
        ] {
            let g = galley_of(&text, wrap);
            let idx = RowIndex::new(&g);
            let n = text.chars().count();
            for i in 0..=n + 3 {
                let want = g.pos_from_cursor(CCursor::new(i));
                assert_eq!(idx.pos(i), want, "char {i} of {n} (wrap {wrap})");
            }
        }
    }

    #[test]
    fn visible_ranges_cover_exactly_the_rows_in_view() {
        let text = SAMPLE.repeat(30);
        let g = galley_of(&text, 200.0);
        let o = Offsets::build(&text);
        let all = Geo::new(&text, &o, &g, (f32::MIN, f32::MAX));
        assert_eq!(all.visible, 0..text.len());
        let mid = g.rect.height() / 2.0;
        let part = Geo::new(&text, &o, &g, (mid, mid + 100.0));
        assert!(part.visible.start > 0 && part.visible.end < text.len());
        assert!(
            text.is_char_boundary(part.visible.start) && text.is_char_boundary(part.visible.end)
        );
        // Every row's start that lies in the window is inside the range; one far away is not.
        let y_in = part.pos(part.visible.start).min.y;
        assert!(y_in <= mid + 100.0 && y_in >= mid - 40.0);
        assert!(part.is_visible(&(part.visible.start..part.visible.start + 1)));
        assert!(!part.is_visible(&(0..1)));
        assert!(!part.is_visible(&(text.len() - 1..text.len())));
        let none = Geo::new(&text, &o, &g, (-500.0, -400.0));
        assert!(none.visible.is_empty());
    }
}
