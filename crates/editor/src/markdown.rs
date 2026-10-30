//! Markdown source analysis. Everything here works on **byte ranges into the original
//! source**; nothing is ever re-serialised. Unsupported constructs simply produce no span
//! and therefore stay visible as raw text.

use pulldown_cmark::{Alignment, CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

/// One table row: its source line and the source range of each cell (the text between two pipes,
/// surrounding spaces included, as reported by the parser).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Row {
    pub range: Range<usize>,
    pub cells: Vec<Range<usize>>,
}

/// A complete GFM table. All ranges are byte offsets into the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Table {
    pub align: Vec<Align>,
    pub header: Row,
    /// The `| --- | :---: |` line (found by scanning: the line after the header).
    pub separator: Range<usize>,
    pub rows: Vec<Row>,
    /// The inline spans (bold, italic, strikethrough, code, link) the parser found inside the cells,
    /// as it reports them for the whole note: the same spans that style the raw source. The grid
    /// painter conceals their markers and styles their text from these; it parses nothing itself.
    pub inline: Vec<Span>,
    /// A hash of `inline` relative to the table's start (see `table::signature_of`).
    pub inline_sig: u64,
}

impl Table {
    pub fn columns(&self) -> usize {
        self.align.len()
    }

    /// Source of the table without its trailing line break.
    pub fn source_range(&self) -> Range<usize> {
        let end = self.rows.last().map_or(self.separator.end, |r| r.range.end);
        self.header.range.start..end
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// Complete top-level GFM table; `range` includes the trailing line break.
    Table(Table),
    Heading(u8),
    Strong,
    Emphasis,
    /// `~~text~~` (GFM); delimiters are the markers.
    Strikethrough,
    Code,
    /// Fenced or indented code block; `body` excludes the fence lines.
    CodeBlock {
        body: Range<usize>,
    },
    /// Thematic break (`---`, `***`, `___`); `range` includes the line's newline.
    Rule,
    /// Whole blockquote (no markers; used for the left border). `depth` 0 = outermost.
    Quote {
        depth: u8,
    },
    /// One quoted source line; its `>` prefix is the marker.
    QuoteLine,
    /// Inline link or `<autolink>`; `url` is the destination as written (not yet validated).
    Link {
        url: String,
    },
    /// GFM task box `[ ]` / `[x]` of a list item that starts its own line. `range` is the three
    /// bytes of the box; the state character is at `range.start + 1`.
    Task {
        checked: bool,
    },
    /// A list item's marker (`-`, `*`, `+`, `1.`, `1)`); coloured, never concealed.
    ListMarker,
    /// A line consisting solely of an image reference, optionally followed by the editor's
    /// `{width=NN%}` annotation. `url` is the destination as written; `width` the annotated
    /// percentage as written (display clamps it); `ann` the annotation's byte range (empty, at the
    /// end of the image reference, when there is none).
    Image {
        url: String,
        width: Option<u16>,
        ann: Range<usize>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Span {
    pub kind: Kind,
    /// Whole construct, delimiters included.
    pub range: Range<usize>,
    /// Delimiter byte ranges that may be concealed when the span is inactive.
    pub markers: Vec<Range<usize>>,
    /// While the caret/selection touches this range (inclusive), markers are revealed.
    pub reveal: Range<usize>,
}

pub fn line_range(src: &str, at: usize) -> Range<usize> {
    let start = src[..at].rfind('\n').map_or(0, |i| i + 1);
    let end = src[at..].find('\n').map_or(src.len(), |i| at + i);
    start..end
}

fn trim_nl(src: &str, r: Range<usize>) -> Range<usize> {
    r.start..r.start + src[r].trim_end_matches(['\n', '\r']).len()
}

/// Setext heading (`Title\n=====`): the underline line is the concealable marker.
fn setext(src: &str, range: &Range<usize>, level: u8) -> Option<Span> {
    let body = trim_nl(src, range.clone());
    let last_start = range.start + src[body.clone()].rfind('\n')? + 1;
    let underline = &src[last_start..body.end];
    let t = underline.trim();
    let indent = underline.len() - underline.trim_start().len();
    if t.is_empty() || indent > 3 || !(t.bytes().all(|b| b == b'=') || t.bytes().all(|b| b == b'-'))
    {
        return None;
    }
    Some(Span {
        kind: Kind::Heading(level),
        range: range.start..last_start,
        markers: std::iter::once(last_start..range.end).collect(),
        reveal: body,
    })
}

/// One `QuoteLine` span per source line that starts with `>` prefixes (nested prefixes included).
fn quote_lines(src: &str, range: &Range<usize>, spans: &mut Vec<Span>) {
    let mut ls = range.start;
    while ls < range.end && ls < src.len() {
        let le = src[ls..].find('\n').map_or(src.len(), |i| ls + i);
        let (mut p, mut marker_end) = (ls, ls);
        loop {
            let b = src.as_bytes();
            let mut q = p;
            while q < le && q < p + 3 && b[q] == b' ' {
                q += 1;
            }
            if q < le && b[q] == b'>' {
                q += 1;
                if q < le && b[q] == b' ' {
                    q += 1;
                }
                p = q;
                marker_end = q;
            } else {
                break;
            }
        }
        if marker_end > ls {
            spans.push(Span {
                kind: Kind::QuoteLine,
                range: ls..le,
                markers: std::iter::once(ls..marker_end).collect(),
                reveal: ls..le,
            });
        }
        ls = le + 1;
    }
}

fn code_block(src: &str, range: Range<usize>, fenced_kind: bool) -> Span {
    let line = line_range(src, range.start);
    let first = &src[line.clone()];
    let indent = first.len() - first.trim_start().len();
    let fence = first.trim_start();
    let c = fence.bytes().next();
    let trimmed = trim_nl(src, range.clone());
    if !(fenced_kind && range.start == line.start && indent < 4 && matches!(c, Some(b'`' | b'~'))) {
        return Span {
            kind: Kind::CodeBlock {
                body: range.clone(),
            },
            range,
            markers: vec![],
            reveal: trimmed,
        };
    }
    let n = fence.bytes().take_while(|b| Some(*b) == c).count();
    let open_end = (line.end + 1).min(range.end);
    let mut markers: Vec<Range<usize>> = std::iter::once(line.start..open_end).collect();
    let mut body_end = range.end;
    // A closing fence is the last line, same char, at least as long, nothing but spaces after.
    if let Some(i) = src[range.start..trimmed.end].rfind('\n') {
        let ls = range.start + i + 1;
        if ls >= open_end {
            let last = &src[ls..trimmed.end];
            let t = last.trim_start();
            let m = t.bytes().take_while(|b| Some(*b) == c).count();
            if last.len() - t.len() < 4 && m >= n && t[m..].trim().is_empty() {
                // Include the line break so the concealed row collapses completely.
                let end = range.end + usize::from(src[range.end..].starts_with('\n'));
                markers.push(ls..end);
                body_end = ls;
            }
        }
    }
    Span {
        kind: Kind::CodeBlock {
            body: open_end..body_end.max(open_end),
        },
        range,
        markers,
        reveal: trimmed,
    }
}

/// Link being assembled: whole range, text range (from child events), destination.
type LinkPending = (Range<usize>, Option<Range<usize>>, String);

/// The text after an image reference: nothing (`Some((None, empty))`) or exactly `{width=NN%}`
/// with one to three digits, directly after the `)`. Anything else is `None`.
fn image_annotation(src: &str, rest: Range<usize>) -> Option<(Option<u16>, Range<usize>)> {
    if rest.is_empty() {
        return Some((None, rest.end..rest.end));
    }
    let digits = src[rest.clone()]
        .strip_prefix("{width=")?
        .strip_suffix("%}")?;
    let ok = (1..=3).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit());
    ok.then(|| (digits.parse().ok(), rest))
}

/// Destination of every image in the document (inline ones included), in order, exactly as
/// written. Images inside code are not images and are not listed.
pub fn image_references(src: &str) -> Vec<String> {
    Parser::new_ext(src, PARSER_OPTIONS)
        .filter_map(|ev| match ev {
            Event::Start(Tag::Image { dest_url, .. }) => Some(dest_url.to_string()),
            _ => None,
        })
        .collect()
}

/// Table being assembled: (source range, alignments, header, body rows).
type PendingTable = (Range<usize>, Vec<Align>, Option<Row>, Vec<Row>);

const PARSER_OPTIONS: Options = Options::ENABLE_TABLES
    .union(Options::ENABLE_TASKLISTS)
    .union(Options::ENABLE_STRIKETHROUGH);

pub fn parse(src: &str) -> Vec<Span> {
    let mut spans: Vec<Span> = Vec::new();
    // Stack of (kind-start info) for constructs whose markers depend on children.
    let mut link_stack: Vec<LinkPending> = Vec::new();
    let mut image_depth = 0usize;
    let mut quote_depth = 0u8;
    let mut item_depth = 0usize;
    // (table range, alignments, header, rows) while inside a table.
    let mut table: Option<PendingTable> = None;
    let mut row: Option<Row> = None;
    for (ev, range) in Parser::new_ext(src, PARSER_OPTIONS).into_offset_iter() {
        match ev {
            Event::Start(Tag::Table(al)) => {
                let top = quote_depth == 0
                    && item_depth == 0
                    && range.start == line_range(src, range.start).start;
                if top {
                    let align = al
                        .iter()
                        .map(|a| match a {
                            Alignment::None => Align::None,
                            Alignment::Left => Align::Left,
                            Alignment::Center => Align::Center,
                            Alignment::Right => Align::Right,
                        })
                        .collect();
                    table = Some((range, align, None, Vec::new()));
                }
            }
            Event::Start(Tag::TableHead | Tag::TableRow) if table.is_some() => {
                row = Some(Row {
                    range: trim_nl(src, range),
                    cells: Vec::new(),
                });
            }
            Event::Start(Tag::TableCell) => {
                if let Some(r) = row.as_mut() {
                    r.cells.push(range);
                }
            }
            Event::End(TagEnd::TableHead) => {
                if let (Some((_, _, h, _)), Some(r)) = (table.as_mut(), row.take()) {
                    *h = Some(r);
                }
            }
            Event::End(TagEnd::TableRow) => {
                if let (Some((_, _, _, rows)), Some(r)) = (table.as_mut(), row.take()) {
                    rows.push(r);
                }
            }
            Event::End(TagEnd::Table) => {
                if let Some((range, align, Some(header), rows)) = table.take() {
                    // The separator is the line right after the header line.
                    let sep_start = (header.range.end + 1).min(src.len());
                    let separator = trim_nl(src, line_range(src, sep_start));
                    // Everything pushed since the table began is inside it (cells hold inline
                    // constructs only), and everything pushed before it starts before it.
                    let mut inline: Vec<Span> = spans
                        .iter()
                        .rev()
                        .take_while(|s| s.range.start >= range.start)
                        .filter(|s| {
                            matches!(
                                s.kind,
                                Kind::Strong
                                    | Kind::Emphasis
                                    | Kind::Strikethrough
                                    | Kind::Code
                                    | Kind::Link { .. }
                            )
                        })
                        .cloned()
                        .collect();
                    inline.reverse();
                    let inline_sig = crate::table::signature_of(header.range.start, &inline);
                    let t = Table {
                        align,
                        header,
                        separator,
                        rows,
                        inline,
                        inline_sig,
                    };
                    let reveal = t.source_range();
                    spans.push(Span {
                        kind: Kind::Table(t),
                        range,
                        markers: vec![],
                        reveal,
                    });
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                let line = line_range(src, range.start);
                if range.start != line.start {
                    continue; // inside a container: stays raw
                }
                let text = &src[line.clone()];
                let hashes = text.bytes().take_while(|b| *b == b'#').count();
                let rest = &text[hashes..];
                if (1..=6).contains(&hashes) && rest.starts_with([' ', '\t']) {
                    let ws = rest.len() - rest.trim_start_matches([' ', '\t']).len();
                    spans.push(Span {
                        kind: Kind::Heading(hashes as u8),
                        range: line.clone(),
                        markers: std::iter::once(line.start..line.start + hashes + ws).collect(),
                        reveal: line,
                    });
                } else if let Some(sp) = setext(src, &range, level as u8) {
                    spans.push(sp);
                }
            }
            Event::Rule => {
                let line = line_range(src, range.start);
                if range.start == line.start {
                    let end = (line.end + 1).min(src.len());
                    spans.push(Span {
                        kind: Kind::Rule,
                        range: line.start..end,
                        markers: vec![],
                        reveal: line,
                    });
                }
            }
            Event::Start(Tag::BlockQuote(_)) => {
                spans.push(Span {
                    kind: Kind::Quote { depth: quote_depth },
                    range: range.clone(),
                    markers: vec![],
                    reveal: trim_nl(src, range.clone()),
                });
                if quote_depth == 0 && range.start == line_range(src, range.start).start {
                    quote_lines(src, &range, &mut spans);
                }
                quote_depth += 1;
            }
            Event::End(TagEnd::BlockQuote(_)) => quote_depth = quote_depth.saturating_sub(1),
            Event::Start(Tag::Strong) | Event::Start(Tag::Emphasis) => {
                let strong = matches!(ev, Event::Start(Tag::Strong));
                let n = if strong { 2 } else { 1 };
                let s = &src[range.clone()];
                if s.len() > 2 * n && s.starts_with(['*', '_']) {
                    spans.push(Span {
                        kind: if strong { Kind::Strong } else { Kind::Emphasis },
                        range: range.clone(),
                        markers: vec![range.start..range.start + n, range.end - n..range.end],
                        reveal: range,
                    });
                }
            }
            Event::Start(Tag::Strikethrough) => {
                let s = &src[range.clone()];
                let n = s.bytes().take_while(|b| *b == b'~').count();
                if (1..=2).contains(&n)
                    && s.len() > 2 * n
                    && s[s.len() - n..].bytes().all(|b| b == b'~')
                {
                    spans.push(Span {
                        kind: Kind::Strikethrough,
                        range: range.clone(),
                        markers: vec![range.start..range.start + n, range.end - n..range.end],
                        reveal: range,
                    });
                }
            }
            Event::TaskListMarker(checked) => {
                let line = line_range(src, range.start);
                let boxed = src
                    .get(range.clone())
                    .is_some_and(|t| t.len() == 3 && t.starts_with('[') && t.ends_with(']'));
                let pre = src[line.start..range.start].trim();
                let marker = matches!(pre, "-" | "*" | "+")
                    || pre.len() > 1
                        && pre.len() <= 10
                        && pre.ends_with(['.', ')'])
                        && pre[..pre.len() - 1].bytes().all(|b| b.is_ascii_digit());
                // Only a box right after its own line's list marker (not in a quote) is a task.
                if boxed && marker {
                    spans.push(Span {
                        kind: Kind::Task { checked },
                        range: range.clone(),
                        markers: vec![],
                        reveal: range,
                    });
                }
            }
            Event::Code(_) => {
                let s = &src[range.clone()];
                let n = s.bytes().take_while(|b| *b == b'`').count();
                if n > 0 && s.len() >= 2 * n {
                    spans.push(Span {
                        kind: Kind::Code,
                        range: range.clone(),
                        markers: vec![range.start..range.start + n, range.end - n..range.end],
                        reveal: range,
                    });
                }
            }
            Event::Start(Tag::CodeBlock(kind)) => {
                spans.push(code_block(
                    src,
                    range,
                    matches!(kind, CodeBlockKind::Fenced(_)),
                ));
            }
            Event::End(TagEnd::Item) => item_depth = item_depth.saturating_sub(1),
            Event::Start(Tag::Item) => {
                item_depth += 1;
                let line = line_range(src, range.start);
                let b = src.as_bytes();
                let n = b[range.start..]
                    .iter()
                    .take_while(|c| c.is_ascii_digit())
                    .count();
                let len = match b.get(range.start + n) {
                    Some(b'-' | b'*' | b'+') if n == 0 => 1,
                    Some(b'.' | b')') if (1..=9).contains(&n) => n + 1,
                    _ => 0,
                };
                // Only items starting their own line (not inside quotes) are styled.
                if len > 0 && src[line.start..range.start].trim().is_empty() {
                    spans.push(Span {
                        kind: Kind::ListMarker,
                        range: range.start..range.start + len,
                        markers: vec![],
                        reveal: range.start..range.start + len,
                    });
                }
            }
            Event::Start(Tag::Link { dest_url, .. }) => {
                link_stack.push((range, None, dest_url.to_string()))
            }
            Event::End(TagEnd::Link) => {
                if let Some((range, Some(inner), url)) = link_stack.pop() {
                    if src[range.clone()].starts_with(['[', '<']) && inner.start > range.start {
                        spans.push(Span {
                            kind: Kind::Link { url },
                            markers: vec![range.start..inner.start, inner.end..range.end],
                            reveal: range.clone(),
                            range,
                        });
                    }
                }
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                image_depth += 1;
                let line = line_range(src, range.start);
                let content = line.start..line.start + src[line.clone()].trim_end().len();
                let alone =
                    src[line.start..range.start].trim().is_empty() && range.end <= content.end;
                // Anything after the image other than a valid `{width=NN%}` leaves the line raw.
                let ann = if alone {
                    image_annotation(src, range.end..content.end)
                } else {
                    None
                };
                if let Some((width, ann)) = ann {
                    spans.push(Span {
                        kind: Kind::Image {
                            url: dest_url.to_string(),
                            width,
                            ann,
                        },
                        range: line.clone(),
                        markers: vec![],
                        reveal: line,
                    });
                }
            }
            Event::End(TagEnd::Image) => image_depth = image_depth.saturating_sub(1),
            _ => {
                // Track the extent of link text from its child events.
                if image_depth == 0 {
                    if let Some((outer, inner, _)) = link_stack.last_mut() {
                        if range.start >= outer.start && range.end <= outer.end && range != *outer {
                            match inner {
                                Some(i) => *i = i.start.min(range.start)..i.end.max(range.end),
                                None => *inner = Some(range),
                            }
                        }
                    }
                }
            }
        }
    }
    spans
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const FIXTURE: &str = "# Heading one\n\nPlain **bold**, *italic*, and `code` here.\n\n## Heading two\n\nAnother paragraph with a [link](https://example.com).\n";

    fn find(spans: &[Span], f: impl Fn(&Kind) -> bool) -> &Span {
        spans.iter().find(|s| f(&s.kind)).expect("span")
    }

    #[test]
    fn fixture_spans() {
        let spans = parse(FIXTURE);
        let h1 = find(&spans, |k| *k == Kind::Heading(1));
        assert_eq!(&FIXTURE[h1.markers[0].clone()], "# ");
        let b = find(&spans, |k| *k == Kind::Strong);
        assert_eq!(&FIXTURE[b.range.clone()], "**bold**");
        assert_eq!(&FIXTURE[b.markers[0].clone()], "**");
        assert_eq!(&FIXTURE[b.markers[1].clone()], "**");
        let i = find(&spans, |k| *k == Kind::Emphasis);
        assert_eq!(&FIXTURE[i.range.clone()], "*italic*");
        let c = find(&spans, |k| *k == Kind::Code);
        assert_eq!(&FIXTURE[c.markers[0].clone()], "`");
        assert_eq!(&FIXTURE[c.range.clone()], "`code`");
        let l = find(&spans, |k| matches!(k, Kind::Link { .. }));
        assert_eq!(&FIXTURE[l.markers[0].clone()], "[");
        assert_eq!(&FIXTURE[l.markers[1].clone()], "](https://example.com)");
        assert!(spans.iter().any(|s| s.kind == Kind::Heading(2)));
    }

    #[test]
    fn unicode_offsets_are_byte_correct() {
        let src = "# Příliš žluťoučký\n\nkůň **🙂 ěščř** a *ž*\n";
        for s in parse(src) {
            // Slicing panics if an offset is not on a char boundary.
            let _ = &src[s.range.clone()];
            for m in &s.markers {
                let _ = &src[m.clone()];
            }
        }
        let spans = parse(src);
        let b = find(&spans, |k| *k == Kind::Strong);
        assert_eq!(&src[b.range.clone()], "**🙂 ěščř**");
    }

    #[test]
    fn unsupported_syntax_has_no_span() {
        let src = "<!-- comment -->\n\n- item one\n- item two\n\n| a | b |\n";
        // Only the list markers are styled; HTML and tables stay raw.
        let spans = parse(src);
        assert_eq!(spans.len(), 2);
        assert!(spans
            .iter()
            .all(|s| s.kind == Kind::ListMarker && s.markers.is_empty()));
        assert_eq!(&src[spans[0].range.clone()], "-");
    }

    #[test]
    fn list_markers() {
        let src = "1. a\n   * nested\n\n10) b\n\n> - quoted\n";
        let spans: Vec<_> = parse(src)
            .into_iter()
            .filter(|s| s.kind == Kind::ListMarker)
            .map(|s| src[s.range].to_string())
            .collect();
        assert_eq!(spans, ["1.", "*", "10)"]);
    }

    #[test]
    fn image_line_only() {
        let spans = parse("a\n\n![alt](assets/x.png)\n\ntext ![i](y.png) more\n");
        let imgs: Vec<_> = spans
            .iter()
            .filter(|s| matches!(s.kind, Kind::Image { .. }))
            .collect();
        assert_eq!(imgs.len(), 1);
        assert_eq!(
            imgs[0].kind,
            Kind::Image {
                url: "assets/x.png".into(),
                width: None,
                ann: 23..23,
            }
        );
    }

    #[test]
    fn nested_and_adjacent() {
        let src = "***both*** and **a *b* c**";
        let spans = parse(src);
        assert!(spans.iter().filter(|s| s.kind == Kind::Strong).count() == 2);
        assert!(spans.iter().filter(|s| s.kind == Kind::Emphasis).count() == 2);
    }

    fn kinds(spans: &[Span], f: impl Fn(&Kind) -> bool) -> Vec<&Span> {
        spans.iter().filter(|s| f(&s.kind)).collect()
    }

    #[test]
    fn blockquote_lines_and_nesting() {
        let src = "> quote **b**\n> další řádek 🙂\nlazy\n\n> > nested\n\nafter\n";
        let spans = parse(src);
        let q = kinds(&spans, |k| matches!(k, Kind::Quote { .. }));
        assert_eq!(q.len(), 3, "outer, outer2, nested");
        assert!(q.iter().any(|s| s.kind == Kind::Quote { depth: 1 }));
        let lines = kinds(&spans, |k| *k == Kind::QuoteLine);
        let markers: Vec<&str> = lines.iter().map(|s| &src[s.markers[0].clone()]).collect();
        assert_eq!(
            markers,
            ["> ", "> ", "> > "],
            "lazy continuation line has no marker"
        );
        for s in &spans {
            let _ = (&src[s.range.clone()], &src[s.reveal.clone()]);
        }
        // Reveal range never includes the newline, so a caret on the next line does not reveal.
        assert!(!src[q[0].reveal.clone()].ends_with('\n'));
    }

    #[test]
    fn thematic_break_vs_setext() {
        let src = "Title\n---\n\nPara\n\n---\n\n***\n\n___\n\n- - -\n\nSetext one\n===\n";
        let spans = parse(src);
        let h: Vec<_> = kinds(&spans, |k| matches!(k, Kind::Heading(_)));
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].kind, Kind::Heading(2));
        assert_eq!(&src[h[0].range.clone()], "Title\n");
        assert_eq!(&src[h[0].markers[0].clone()], "---\n");
        assert_eq!(&src[h[0].reveal.clone()], "Title\n---");
        assert_eq!(h[1].kind, Kind::Heading(1));
        let rules = kinds(&spans, |k| *k == Kind::Rule);
        let texts: Vec<&str> = rules.iter().map(|s| src[s.reveal.clone()].trim()).collect();
        assert_eq!(texts, ["---", "***", "___", "- - -"]);
    }

    #[test]
    fn fenced_code_markers_and_body() {
        let src = "intro\n\n```rust\nlet š = 1;\n\n  indented\n```\nafter\n";
        let spans = parse(src);
        let cb = kinds(&spans, |k| matches!(k, Kind::CodeBlock { .. }));
        assert_eq!(cb.len(), 1);
        let Kind::CodeBlock { body } = &cb[0].kind else {
            unreachable!()
        };
        assert_eq!(&src[body.clone()], "let š = 1;\n\n  indented\n");
        assert_eq!(&src[cb[0].markers[0].clone()], "```rust\n");
        assert_eq!(&src[cb[0].markers[1].clone()], "```\n");
        assert_eq!(
            &src[cb[0].reveal.clone()],
            "```rust\nlet š = 1;\n\n  indented\n```"
        );
    }

    #[test]
    fn unclosed_fence_runs_to_end_of_document() {
        for src in [
            "```\ncode\nmore",
            "```\ncode\n\nmore\n",
            "````\nx\n```\ny\n",
            "~~~\na\n",
        ] {
            let spans = parse(src);
            let cb = kinds(&spans, |k| matches!(k, Kind::CodeBlock { .. }));
            assert_eq!(cb.len(), 1, "{src:?}");
            assert_eq!(cb[0].markers.len(), 1, "no closing marker for {src:?}");
            let Kind::CodeBlock { body } = &cb[0].kind else {
                unreachable!()
            };
            assert_eq!(body.end, cb[0].range.end);
        }
        // The shorter ``` does not close a ```` fence, but a longer one does.
        let spans = parse("```\na\n````\nafter\n");
        let cb = kinds(&spans, |k| matches!(k, Kind::CodeBlock { .. }));
        assert_eq!(cb[0].markers.len(), 2);
    }

    #[test]
    fn indented_code_has_no_markers_and_quote_fence_stays_raw() {
        let spans = parse("para\n\n    indented code\n");
        let cb = kinds(&spans, |k| matches!(k, Kind::CodeBlock { .. }));
        assert_eq!(cb.len(), 1);
        assert!(cb[0].markers.is_empty());
        let spans = parse("> ```\n> in quote\n> ```\n");
        let cb = kinds(&spans, |k| matches!(k, Kind::CodeBlock { .. }));
        assert!(
            cb.iter().all(|s| s.markers.is_empty()),
            "fences inside containers are not concealed"
        );
    }

    #[test]
    fn all_new_ranges_are_char_boundaries_for_unicode() {
        let src = "# Příliš\n\n> žluťoučký 🙂\n\n```\nkůň 🙂\n```\n\nŽ\n---\n\n---\n";
        for s in parse(src) {
            let _ = &src[s.range.clone()];
            let _ = &src[s.reveal.clone()];
            for m in &s.markers {
                let _ = &src[m.clone()];
            }
            if let Kind::CodeBlock { body } = &s.kind {
                let _ = &src[body.clone()];
            }
        }
    }

    const TABLE_DOC: &str = "# Table experiment\n\nParagraph before the table.\n\n| Name | Role | Status |\n| --- | :---: | ---: |\n| Alice | Developer | Active |\n| Bob | Analyst | Pending |\n\nParagraph after the table.\n";

    fn tables(src: &str) -> Vec<(Span, Table)> {
        parse(src)
            .into_iter()
            .filter_map(|s| match s.kind.clone() {
                Kind::Table(t) => Some((s, t)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn complete_table_ranges_alignment_and_cells() {
        let found = tables(TABLE_DOC);
        assert_eq!(found.len(), 1);
        let (sp, t) = &found[0];
        let at = TABLE_DOC.find("| Name").unwrap();
        // Span covers the table lines plus the trailing break; reveal excludes the break.
        assert_eq!(&TABLE_DOC[sp.range.clone()].lines().count(), &4);
        assert!(TABLE_DOC[sp.range.clone()].ends_with("Pending |\n"));
        assert_eq!(sp.range.start, at);
        assert_eq!(
            TABLE_DOC[sp.reveal.clone()].to_string(),
            TABLE_DOC[sp.range.clone()].trim_end()
        );
        assert_eq!(t.align, [Align::None, Align::Center, Align::Right]);
        assert_eq!(t.columns(), 3);
        assert_eq!(
            &TABLE_DOC[t.header.range.clone()],
            "| Name | Role | Status |"
        );
        assert_eq!(&TABLE_DOC[t.separator.clone()], "| --- | :---: | ---: |");
        assert_eq!(t.rows.len(), 2);
        assert_eq!(
            &TABLE_DOC[t.rows[1].range.clone()],
            "| Bob | Analyst | Pending |"
        );
        let cells = |r: &Row| {
            r.cells
                .iter()
                .map(|c| TABLE_DOC[c.clone()].trim())
                .collect::<Vec<_>>()
        };
        assert_eq!(cells(&t.header), ["Name", "Role", "Status"]);
        assert_eq!(cells(&t.rows[0]), ["Alice", "Developer", "Active"]);
        assert_eq!(t.source_range(), sp.reveal);
        // Boundaries: nothing but blank lines between the table and its neighbours.
        assert_eq!(&TABLE_DOC[..at].lines().last(), &Some(""));
        assert!(TABLE_DOC[sp.range.end..].starts_with("\nParagraph after"));
    }

    #[test]
    fn table_with_unicode_text() {
        let src = "| Příliš | 🦀 žluťoučký |\n|:-|-:|\n| kůň úpěl | ďábelské ódy 🙂 |\n";
        let (sp, t) = tables(src).remove(0);
        assert_eq!(t.align, [Align::Left, Align::Right]);
        for c in t.header.cells.iter().chain(&t.rows[0].cells) {
            assert!(src.is_char_boundary(c.start) && src.is_char_boundary(c.end));
        }
        assert_eq!(src[t.rows[0].cells[1].clone()].trim(), "ďábelské ódy 🙂");
        assert_eq!(sp.range, 0..src.len());
    }

    #[test]
    fn incomplete_or_malformed_tables_stay_raw() {
        for src in [
            "| a | b |\n",                      // no separator row
            "| a | b |\n| - |\n| x | y |\n",    // column count mismatch
            "| a | b |\n| -- x |\n| 1 | 2 |\n", // invalid separator
            "a | b\n",                          // plain text with a pipe
            "| a | b |\n|---|---|\n",           // header only is still valid GFM (no body)
        ] {
            let n = tables(src).len();
            if src.ends_with("|---|---|\n") {
                assert_eq!(n, 1, "{src:?}"); // valid: header + separator
            } else {
                assert_eq!(n, 0, "{src:?}");
            }
        }
        // Tables inside a blockquote or list item are not rendered.
        assert!(tables("> | a | b |\n> | - | - |\n> | 1 | 2 |\n").is_empty());
        assert!(tables("- | a | b |\n  | - | - |\n  | 1 | 2 |\n").is_empty());
    }

    /// Parsing and the save path never alter bytes (CRLF and a missing final newline included).
    #[test]
    fn table_documents_round_trip_byte_identically() {
        let dir = crate::test_dir::TestDir::new("table");
        let crlf = TABLE_DOC.replace('\n', "\r\n");
        let no_eol = TABLE_DOC.trim_end().to_string();
        for (i, doc) in [TABLE_DOC.to_string(), crlf, no_eol].iter().enumerate() {
            let _ = parse(doc);
            let p = dir.join(format!("d{i}.md"));
            std::fs::write(&p, doc.as_bytes()).unwrap();
            let loaded = String::from_utf8(std::fs::read(&p).unwrap()).unwrap();
            let _ = parse(&loaded);
            std::fs::write(&p, loaded.as_bytes()).unwrap();
            assert_eq!(std::fs::read(&p).unwrap(), doc.as_bytes(), "doc {i}");
            assert_eq!(tables(doc).len(), 1, "doc {i} still recognised");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn task_spans(src: &str) -> Vec<(String, bool)> {
        parse(src)
            .into_iter()
            .filter_map(|s| match s.kind {
                Kind::Task { checked } => Some((src[s.range].to_string(), checked)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn task_items_checked_and_unchecked() {
        let src = "- [ ] open\n- [x] done\n- [X] DONE\n1. [ ] numbered\n  - [x] nested\n";
        assert_eq!(
            task_spans(src),
            [
                ("[ ]".into(), false),
                ("[x]".into(), true),
                ("[X]".into(), true),
                ("[ ]".into(), false),
                ("[x]".into(), true)
            ]
        );
        // Plain list items and malformed boxes are not tasks.
        for raw in [
            "- item\n",
            "- [] x\n",
            "- [y] x\n",
            "- [ ]x\n",
            "[ ] no list\n",
            "> - [ ] quoted\n",
            "- not [ ] at start\n",
        ] {
            assert!(task_spans(raw).is_empty(), "{raw:?}");
        }
        // The state byte is the middle one.
        let sp = parse("- [x] a")
            .into_iter()
            .find(|s| matches!(s.kind, Kind::Task { .. }))
            .unwrap();
        assert_eq!(sp.range, 2..5);
    }

    #[test]
    fn strikethrough_ranges_markers_and_nesting() {
        let src = "a ~~Příliš 🦀~~ b";
        let sp = parse(src)
            .into_iter()
            .find(|s| s.kind == Kind::Strikethrough)
            .unwrap();
        assert_eq!(&src[sp.range.clone()], "~~Příliš 🦀~~");
        assert_eq!(&src[sp.markers[0].clone()], "~~");
        assert_eq!(&src[sp.markers[1].clone()], "~~");
        let src = "~~**b** *i*~~ and **~~x~~** and [~~l~~](https://x.org)";
        let kinds: Vec<_> = parse(src).into_iter().map(|s| s.kind).collect();
        assert_eq!(
            kinds.iter().filter(|k| **k == Kind::Strikethrough).count(),
            3
        );
        assert!(kinds.contains(&Kind::Strong) && kinds.contains(&Kind::Emphasis));
        assert!(kinds.iter().any(|k| matches!(k, Kind::Link { .. })));
        for raw in ["~~ open", "a ~~b", "~~~~"] {
            assert!(
                !parse(raw).iter().any(|s| s.kind == Kind::Strikethrough),
                "{raw:?}"
            );
        }
    }

    #[test]
    fn links_keep_their_url_title_and_autolink_markers() {
        let src = "[a](https://x.org \"Title\") <https://auto.org> [r](rel/path.md) <me@x.org>";
        let links: Vec<_> = parse(src)
            .into_iter()
            .filter_map(|s| match &s.kind {
                Kind::Link { url } => Some((
                    url.clone(),
                    src[s.range.clone()].to_string(),
                    s.markers.clone(),
                )),
                _ => None,
            })
            .collect();
        assert_eq!(links[0].0, "https://x.org");
        // Everything but the link text (including the title) is a concealable marker.
        assert_eq!(&src[links[0].2[0].clone()], "[");
        assert_eq!(&src[links[0].2[1].clone()], "](https://x.org \"Title\")");
        assert_eq!(links[1].0, "https://auto.org");
        assert_eq!(
            (&src[links[1].2[0].clone()], &src[links[1].2[1].clone()]),
            ("<", ">")
        );
        assert_eq!(links[2].0, "rel/path.md");
        assert_eq!(links[3].0, "me@x.org"); // never an http(s) URL, so never opened
    }

    fn image_spans(src: &str) -> Vec<(String, Option<u16>, String)> {
        parse(src)
            .into_iter()
            .filter_map(|s| match s.kind {
                Kind::Image { url, width, ann } => Some((url, width, src[ann].to_string())),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn image_width_annotation_valid_forms() {
        assert_eq!(
            image_spans("![a](x.png)\n"),
            [("x.png".into(), None, String::new())]
        );
        assert_eq!(
            image_spans("![Příliš 🦀](assets/obrázek.png){width=60%}\n"),
            [("assets/obrázek.png".into(), Some(60), "{width=60%}".into())]
        );
        // Trailing spaces, CRLF, no final newline, out-of-range numbers are all kept as written.
        for (src, w) in [
            ("![a](x.png){width=100%}  \n", 100),
            ("![a](x.png){width=5%}", 5),
            ("![a](x.png){width=020%}\r\n", 20),
            ("![a](x.png){width=150%}", 150),
        ] {
            let v = image_spans(src);
            assert_eq!(v.len(), 1, "{src:?}");
            assert_eq!(v[0].1, Some(w), "{src:?}");
        }
        // The annotation range covers exactly the annotation, so an edit touches nothing else.
        let src = "x\n\n![a](x.png){width=60%} \nz";
        let sp = parse(src)
            .into_iter()
            .find(|s| matches!(s.kind, Kind::Image { .. }))
            .unwrap();
        let Kind::Image { ann, .. } = sp.kind else {
            unreachable!()
        };
        assert_eq!(&src[ann], "{width=60%}");
    }

    #[test]
    fn malformed_image_annotations_leave_the_line_raw() {
        for src in [
            "![a](x.png){width=abc%}",
            "![a](x.png){width=60}",
            "![a](x.png){width=}",
            "![a](x.png){width=%}",
            "![a](x.png){width=1234%}",
            "![a](x.png){width=-5%}",
            "![a](x.png){ width=60% }",
            "![a](x.png) {width=60%}",
            "![a](x.png){width=60%}x",
            "![a](x.png){width=60%}{width=70%}",
            "![a](x.png){height=60%}",
            "![a](x.png){width=60%",
            "text ![a](x.png){width=60%}",
        ] {
            assert!(image_spans(src).is_empty(), "{src:?}");
        }
    }

    #[test]
    fn image_references_cover_every_image_but_not_code() {
        let src = "![a](x.png){width=60%}\n\ntext ![b](assets/ž-ů.png \"t\") and ![c](<my pic.png>)\n\n`![d](no.png)`\n\n```\n![e](no.png)\n```\n\n[l](not-image.png)\n";
        assert_eq!(
            image_references(src),
            ["x.png", "assets/ž-ů.png", "my pic.png"]
        );
        assert!(image_references("plain").is_empty());
    }
}
