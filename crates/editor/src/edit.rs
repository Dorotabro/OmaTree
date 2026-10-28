//! Pure source-text transformations (no egui). All offsets are UTF-8 byte offsets.

use std::ops::Range;

/// A replacement of `range` with `text`, leaving the caret at byte offset `caret` of the new source.
#[derive(Debug, PartialEq, Eq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub caret: usize,
    /// Selection anchor when the edit should leave a selection (`anchor..caret`); else a plain caret.
    pub anchor: Option<usize>,
}

impl Edit {
    pub fn apply(&self, src: &mut String) {
        src.replace_range(self.range.clone(), &self.text);
    }
}

pub fn char_to_byte(s: &str, chars: usize) -> usize {
    s.char_indices().nth(chars).map_or(s.len(), |(i, _)| i)
}

pub fn byte_to_char(s: &str, byte: usize) -> usize {
    s[..byte].chars().count()
}

/// Insert a Markdown image reference as its own line at `at`.
pub fn insert_image_ref(src: &str, at: usize, url: &str) -> Edit {
    let prefix = if at == 0 || src[..at].ends_with('\n') {
        ""
    } else {
        "\n"
    };
    let suffix = if src[at..].starts_with('\n') {
        ""
    } else {
        "\n"
    };
    let text = format!("{prefix}![image]({url}){suffix}");
    // Caret goes to the start of the line after the image.
    let caret = at + text.len() + usize::from(suffix.is_empty());
    Edit {
        range: at..at,
        text,
        caret,
        anchor: None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlashAction {
    Heading(u8),
    /// Wrap in / pair up this inline delimiter (`**`, `*`, `` ` ``).
    Inline(&'static str),
    Bullet,
    Numbered,
    /// `- [ ] ` task item.
    Task,
    /// Insert literal Markdown; `caret` is a byte offset inside `text`.
    Template {
        text: &'static str,
        caret: usize,
    },
    /// Insert a 2×2 GFM table and select the first data cell.
    Table,
    /// Open the emoji picker.
    Emoji,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlashCommand {
    pub title: &'static str,
    pub keywords: &'static str,
    pub action: SlashAction,
}

const fn cmd(title: &'static str, keywords: &'static str, action: SlashAction) -> SlashCommand {
    SlashCommand {
        title,
        keywords,
        action,
    }
}

pub const SLASH_COMMANDS: &[SlashCommand] = &[
    cmd("Heading 1", "h1 heading title", SlashAction::Heading(1)),
    cmd("Heading 2", "h2 heading", SlashAction::Heading(2)),
    cmd("Heading 3", "h3 heading", SlashAction::Heading(3)),
    cmd("Bold", "bold strong", SlashAction::Inline("**")),
    cmd("Italic", "italic emphasis", SlashAction::Inline("*")),
    cmd(
        "Inline code",
        "inline code backtick",
        SlashAction::Inline("`"),
    ),
    cmd("Bullet list", "bullet list ul", SlashAction::Bullet),
    cmd("Numbered list", "numbered list ol", SlashAction::Numbered),
    cmd("Task list", "task list todo checkbox", SlashAction::Task),
    cmd(
        "Code block",
        "code block fence",
        SlashAction::Template {
            text: "```\n\n```",
            caret: 4,
        },
    ),
    cmd("Table", "table grid gfm", SlashAction::Table),
    cmd("Emoji", "emoji emoticon smiley", SlashAction::Emoji),
];

/// `/strike`: offered only to hosts that have no strikethrough shortcut (see [`slash_filter_with`]).
pub const STRIKE_COMMAND: SlashCommand = cmd(
    "Strikethrough",
    "strike strikethrough delete ~~",
    SlashAction::Inline("~~"),
);

/// The standard slash menu for `query`.
pub fn slash_filter(query: &str) -> Vec<SlashCommand> {
    slash_filter_with(query, false)
}

/// The slash menu for `query`; `strike` adds `/strike` after "Inline code".
pub fn slash_filter_with(query: &str, strike: bool) -> Vec<SlashCommand> {
    let q = query.to_lowercase();
    let at = SLASH_COMMANDS
        .iter()
        .position(|c| c.title == "Inline code")
        .map_or(SLASH_COMMANDS.len(), |i| i + 1);
    let mut all = SLASH_COMMANDS.to_vec();
    if strike {
        all.insert(at, STRIKE_COMMAND);
    }
    all.into_iter()
        .filter(|c| q.is_empty() || c.title.to_lowercase().contains(&q) || c.keywords.contains(&q))
        .collect()
}

/// Is a `/` at byte `pos` a slash-menu trigger (line start or after whitespace)?
pub fn is_slash_trigger(src: &str, pos: usize) -> bool {
    src[pos..].starts_with('/')
        && src[..pos]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace())
}

/// Apply a slash command to the typed `/query` (`trigger..caret`). `None` for [`SlashAction::Emoji`],
/// which opens the picker instead.
pub fn apply_slash(src: &str, trigger: usize, caret: usize, cmd: &SlashCommand) -> Option<Edit> {
    Some(match cmd.action {
        SlashAction::Emoji => return None,
        SlashAction::Heading(l) => set_heading(src, trigger, caret, l),
        SlashAction::Bullet => slash_list(src, trigger, caret, false),
        SlashAction::Numbered => slash_list(src, trigger, caret, true),
        SlashAction::Task => slash_task(src, trigger, caret),
        SlashAction::Inline(d) => Edit {
            range: trigger..caret,
            text: format!("{d}{d}"),
            caret: trigger + d.len(),
            anchor: None,
        },
        SlashAction::Table => slash_table(src, trigger, caret),
        SlashAction::Template { text, caret: c } => {
            let at_line_start = trigger == 0 || src[..trigger].ends_with('\n');
            let prefix = if at_line_start { "" } else { "\n" };
            Edit {
                range: trigger..caret,
                text: format!("{prefix}{text}"),
                caret: trigger + prefix.len() + c,
                anchor: None,
            }
        }
    })
}

const TABLE_TEXT: &str =
    "| Column 1 | Column 2 |\n| --- | --- |\n| Data 1 | Data 2 |\n| Data 3 | Data 4 |";
const TABLE_SELECT: &str = "Data 1";

/// A table must start its own line and be followed by a blank line (or the end), otherwise
/// neighbouring text would be parsed as part of it or as part of a paragraph.
fn slash_table(src: &str, trigger: usize, caret: usize) -> Edit {
    let before = &src[..trigger];
    let prefix = if trigger == 0 || before.ends_with("\n\n") {
        ""
    } else if before.ends_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let rest = &src[caret..];
    let suffix = if rest.is_empty() || rest.starts_with("\n\n") {
        ""
    } else if rest.starts_with('\n') {
        "\n"
    } else {
        "\n\n"
    };
    let start = trigger + prefix.len() + TABLE_TEXT.find(TABLE_SELECT).unwrap();
    Edit {
        range: trigger..caret,
        text: format!("{prefix}{TABLE_TEXT}{suffix}"),
        caret: start + TABLE_SELECT.len(),
        anchor: Some(start),
    }
}

/// Replace the typed `/query` (`trigger..caret`) with the emoji sequence, caret right after it.
pub fn apply_emoji(trigger: usize, caret: usize, glyph: &str) -> Edit {
    Edit {
        range: trigger..caret,
        text: glyph.to_string(),
        caret: trigger + glyph.len(),
        anchor: None,
    }
}

/// Set (`Some`) or remove (`None`) the `{width=NN%}` annotation of an image line, given the byte
/// range the parser reported for it (empty, right after the image reference, when absent). Nothing
/// else on the line is touched.
pub fn image_width_edit(ann: Range<usize>, pct: Option<u16>) -> Edit {
    let text = pct.map(|p| format!("{{width={p}%}}")).unwrap_or_default();
    Edit {
        caret: ann.start + text.len(),
        range: ann,
        text,
        anchor: None,
    }
}

/// Where byte offset `at` ends up after `e` is applied: offsets before the edit stay, offsets after
/// it shift by its length change, offsets inside the replaced text go to the end of the new text.
pub fn map_offset(e: &Edit, at: usize) -> usize {
    if at <= e.range.start {
        at
    } else if at >= e.range.end {
        at - e.range.len() + e.text.len()
    } else {
        e.range.start + e.text.len()
    }
}

/// Byte range of the line(s) containing `a..b` (no trailing newline). A selection that ends exactly
/// at a line start does not include that following line.
fn line_block(src: &str, a: usize, b: usize) -> Range<usize> {
    let b = if b > a && src[..b].ends_with('\n') {
        b - 1
    } else {
        b
    };
    let start = src[..a].rfind('\n').map_or(0, |i| i + 1);
    let end = src[b..].find('\n').map_or(src.len(), |i| b + i);
    start..end
}

/// The line containing the slash query with `/query` removed, plus the query's offset in it.
fn line_without_query(src: &str, trigger: usize, caret: usize) -> (Range<usize>, String, usize) {
    let line = line_block(src, trigger, caret);
    let t = trigger - line.start;
    let mut s = src[line.clone()].to_string();
    s.replace_range(t..t + (caret - trigger), "");
    (line, s, t)
}

/// A list item line: `indent`, marker, and the byte offset where the content starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListKind {
    Bullet(char),
    Number(u64, char),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListLine {
    pub indent: usize,
    pub kind: ListKind,
    /// Offset in the line after the marker and its single following space.
    pub content: usize,
}

/// Parse a list item line (`- x`, `* x`, `+ x`, `1. x`, `1) x`, optionally indented). Anything that
/// could be a thematic break (`* * *`) or lacks the space after the marker is not a list line.
pub fn parse_list_line(line: &str) -> Option<ListLine> {
    let indent = line.len() - line.trim_start_matches([' ', '\t']).len();
    let rest = &line[indent..];
    let b = rest.as_bytes();
    let (kind, mlen) = match b.first()? {
        c @ (b'-' | b'*' | b'+') => (ListKind::Bullet(*c as char), 1),
        b'0'..=b'9' => {
            let n = b.iter().take_while(|c| c.is_ascii_digit()).count();
            let d = *b.get(n)?;
            if n > 9 || !matches!(d, b'.' | b')') {
                return None;
            }
            (ListKind::Number(rest[..n].parse().ok()?, d as char), n + 1)
        }
        _ => return None,
    };
    if b.get(mlen) != Some(&b' ') {
        return None;
    }
    if let ListKind::Bullet(c) = kind {
        let t: String = rest.chars().filter(|x| !x.is_whitespace()).collect();
        if t.len() >= 3 && t.chars().all(|x| x == c) {
            return None;
        }
    }
    Some(ListLine {
        indent,
        kind,
        content: indent + mlen + 1,
    })
}

/// Turn `text` (one or more lines) into a bullet or numbered list. Existing markers are replaced,
/// blank lines are kept as they are. With `toggle`, text that already is entirely a list of the same
/// kind has its markers removed instead.
pub fn list_lines(text: &str, numbered: bool, toggle: bool) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let parsed: Vec<Option<ListLine>> = lines.iter().map(|l| parse_list_line(l)).collect();
    let same = |p: &ListLine| matches!(p.kind, ListKind::Number(..)) == numbered;
    let all_same = toggle
        && lines
            .iter()
            .zip(&parsed)
            .filter(|(l, _)| !l.trim().is_empty())
            .all(|(_, p)| p.as_ref().is_some_and(same))
        && lines.iter().any(|l| !l.trim().is_empty());
    let mut n = 0;
    let mut out = Vec::new();
    for (l, p) in lines.iter().zip(&parsed) {
        if l.trim().is_empty() {
            out.push((*l).to_string());
            continue;
        }
        let (indent, rest) = match p {
            Some(p) => (&l[..p.indent], &l[p.content..]),
            None => {
                let i = l.len() - l.trim_start().len();
                (&l[..i], &l[i..])
            }
        };
        if all_same {
            out.push(format!("{indent}{rest}"));
        } else {
            n += 1;
            let marker = if numbered {
                format!("{n}. ")
            } else {
                "- ".to_string()
            };
            out.push(format!("{indent}{marker}{rest}"));
        }
    }
    out.join("\n")
}

/// List command from the slash menu: current line, query removed, content kept.
fn slash_list(src: &str, trigger: usize, caret: usize, numbered: bool) -> Edit {
    let (line, stripped, t) = line_without_query(src, trigger, caret);
    // `list_lines` leaves blank lines alone; an empty line still gets its first marker.
    let new = if stripped.trim().is_empty() {
        let marker = if numbered { "1. " } else { "- " };
        format!("{stripped}{marker}")
    } else {
        list_lines(&stripped, numbered, false)
    };
    let tail = if stripped.trim().is_empty() {
        0
    } else {
        stripped.len() - t
    };
    Edit {
        caret: line.start + new.len() - tail,
        range: line,
        text: new,
        anchor: None,
    }
}

/// Task command from the slash menu: the line becomes `- [ ] content` (indent kept, an existing
/// bullet or number marker replaced, an existing task left as it is); an empty line gets `- [ ] `.
fn slash_task(src: &str, trigger: usize, caret: usize) -> Edit {
    let (line, stripped, t) = line_without_query(src, trigger, caret);
    let indent_len = |s: &str| s.len() - s.trim_start_matches([' ', '\t']).len();
    let new = if stripped.trim().is_empty() {
        format!("{stripped}- [ ] ")
    } else if let Some(p) = parse_list_line(&stripped) {
        if task_box(&stripped, &p).is_some() {
            stripped.clone()
        } else {
            format!("{}- [ ] {}", &stripped[..p.indent], &stripped[p.content..])
        }
    } else {
        let i = indent_len(&stripped);
        format!("{}- [ ] {}", &stripped[..i], &stripped[i..])
    };
    let tail = if stripped.trim().is_empty() {
        0
    } else {
        stripped.len() - t
    };
    Edit {
        caret: line.start + new.len() - tail,
        range: line,
        text: new,
        anchor: None,
    }
}

/// List formatting for the lines touched by selection `a..b` (shortcut path). Caret ends after the block.
pub fn list_selection(src: &str, a: usize, b: usize, numbered: bool) -> Edit {
    let block = line_block(src, a, b);
    let new = list_lines(&src[block.clone()], numbered, true);
    Edit {
        caret: block.start + new.len(),
        range: block,
        text: new,
        anchor: None,
    }
}

/// Convert the line holding the slash query to heading `level`, keeping its content. An existing
/// heading marker or list marker is replaced, never duplicated.
fn set_heading(src: &str, trigger: usize, caret: usize, level: u8) -> Edit {
    let (line, stripped, t) = line_without_query(src, trigger, caret);
    let lead = stripped.len() - stripped.trim_start().len();
    let mut cut = lead;
    let body = &stripped[lead..];
    let hashes = body.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&hashes) && body[hashes..].starts_with([' ', '\t']) {
        cut = lead + hashes + body[hashes..].len()
            - body[hashes..].trim_start_matches([' ', '\t']).len();
    } else if let Some(p) = parse_list_line(&stripped) {
        cut = p.content;
    }
    let rest = &stripped[cut..];
    let marker = format!("{} ", "#".repeat(usize::from(level.clamp(1, 6))));
    let tail = stripped.len() - t.max(cut);
    Edit {
        caret: line.start + marker.len() + rest.len() - tail,
        text: format!("{marker}{rest}"),
        range: line,
        anchor: None,
    }
}

fn run_before(s: &str, end: usize, c: char) -> usize {
    s[..end].chars().rev().take_while(|x| *x == c).count()
}

fn run_after(s: &str, start: usize, c: char) -> usize {
    s[start..].chars().take_while(|x| *x == c).count()
}

/// Longest run of backticks in `s`.
fn max_backticks(s: &str) -> usize {
    s.split(|c| c != '`').map(str::len).max().unwrap_or(0)
}

/// Wrap one line of text (no newline, no surrounding whitespace) in the inline delimiter.
fn wrap_line(inner: &str, delim: &str) -> String {
    if delim == "`" {
        let fence = "`".repeat(max_backticks(inner) + 1);
        let pad = if inner.starts_with('`') || inner.ends_with('`') {
            " "
        } else {
            ""
        };
        format!("{fence}{pad}{inner}{pad}{fence}")
    } else {
        format!("{delim}{inner}{delim}")
    }
}

/// Toggle `delim` (`**`, `*` or `` ` ``) around the selection `a..b` (bytes, `a <= b`).
///
/// - No selection: insert an empty pair with the caret between.
/// - Selection: whitespace at its ends stays outside the delimiters (`**word **` would not parse);
///   each non-blank line is wrapped separately so emphasis never spans a paragraph break.
/// - Already formatted (delimiters immediately outside or inside the selection): remove them.
///   `*` and `**` count only their own share of a `***` run, so bold and italic nest validly.
///
/// The resulting selection (`anchor..caret`) covers the formatted text.
pub fn format_inline(src: &str, a: usize, b: usize, delim: &str) -> Edit {
    let sel = &src[a..b];
    let lead = sel.len() - sel.trim_start().len();
    let trail = sel.len() - sel.trim_end().len();
    if sel.trim().is_empty() {
        let at = if sel.is_empty() { a } else { b };
        let d = if delim == "`" { "`" } else { delim };
        return Edit {
            range: at..at,
            text: format!("{d}{d}"),
            caret: at + d.len(),
            anchor: None,
        };
    }
    let (s, e) = (a + lead, b - trail);
    let inner = &src[s..e];
    if !inner.contains('\n') {
        let c = delim.chars().next().unwrap();
        let n = delim.len();
        let (before, after) = (run_before(src, s, c), run_after(src, e, c));
        // Delimiters just outside the selection.
        let outside = match delim {
            "*" => before % 2 == 1 && after % 2 == 1,
            "**" | "~~" => before >= 2 && after >= 2,
            _ => before == 1 && after == 1,
        };
        if outside {
            return Edit {
                range: s - n..e + n,
                text: inner.to_string(),
                caret: s - n + inner.len(),
                anchor: Some(s - n),
            };
        }
        // Delimiters included in the selection.
        let (ib, ia) = (run_after(inner, 0, c), run_before(inner, inner.len(), c));
        let included = inner.len() > 2 * n
            && match delim {
                "*" => ib % 2 == 1 && ia % 2 == 1,
                "**" | "~~" => ib >= 2 && ia >= 2,
                _ => ib == 1 && ia == 1,
            };
        if included {
            let un = &inner[n..inner.len() - n];
            return Edit {
                range: s..e,
                text: un.to_string(),
                caret: s + un.len(),
                anchor: Some(s),
            };
        }
    }
    let text = inner
        .split('\n')
        .map(|l| {
            if l.trim().is_empty() {
                l.to_string()
            } else {
                let i = l.len() - l.trim_start().len();
                let j = l.trim_end().len();
                format!("{}{}{}", &l[..i], wrap_line(&l[i..j], delim), &l[j..])
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    Edit {
        caret: s + text.len(),
        range: s..e,
        text,
        anchor: Some(s),
    }
}

/// Enter at the end of a non-empty list item continues the list (numbers increment); on an empty
/// item it removes the marker, ending the list. `None` when the caret is not at the end of a list line.
/// Byte range (relative to the line) of a GFM task box `[ ]` / `[x]` / `[X]` right after the list
/// marker of `p`, if the box is followed by a space or the end of the line.
pub fn task_box(line: &str, p: &ListLine) -> Option<Range<usize>> {
    let b = line.as_bytes().get(p.content..p.content + 3)?;
    let ok = b[0] == b'[' && matches!(b[1], b' ' | b'x' | b'X') && b[2] == b']';
    let after = line.as_bytes().get(p.content + 3);
    (ok && matches!(after, None | Some(b' '))).then(|| p.content..p.content + 3)
}

/// Flip a task box whose `[` is at byte `start`: `[ ]` becomes `[x]`, `[x]`/`[X]` become `[ ]`.
/// Only the single state byte changes, so the edit is one character wide.
pub fn toggle_task(src: &str, start: usize) -> Option<Edit> {
    let state = start + 1;
    let new = match src.as_bytes().get(state)? {
        b' ' => "x",
        b'x' | b'X' => " ",
        _ => return None,
    };
    (src.as_bytes().get(start) == Some(&b'[') && src.as_bytes().get(start + 2) == Some(&b']')).then(
        || Edit {
            range: state..state + 1,
            text: new.to_string(),
            caret: state + 1,
            anchor: None,
        },
    )
}

/// Enter at the end of a non-empty list item continues the list (numbers increment, a task item
/// continues with an unchecked box); on an empty item (or empty task) it removes the marker, ending
/// the list. `None` when the caret is not at the end of a list line.
pub fn list_enter(src: &str, caret: usize) -> Option<Edit> {
    let line = line_block(src, caret, caret);
    if caret != line.end {
        return None;
    }
    let text = &src[line.clone()];
    let p = parse_list_line(text)?;
    let task = task_box(text, &p);
    let body = task.as_ref().map_or(p.content, |t| t.end);
    if text[body..].trim().is_empty() {
        return Some(Edit {
            range: line.clone(),
            text: String::new(),
            caret: line.start,
            anchor: None,
        });
    }
    let marker = match p.kind {
        ListKind::Bullet(c) => format!("{c} "),
        ListKind::Number(n, d) => format!("{}{d} ", n + 1),
    };
    let boxed = if task.is_some() { "[ ] " } else { "" };
    let text = format!("\n{}{marker}{boxed}", &text[..p.indent]);
    Some(Edit {
        caret: caret + text.len(),
        range: caret..caret,
        text,
        anchor: None,
    })
}

/// Backspace with the caret right after a list marker removes the marker (and indent); right after
/// a task box it removes the box only, leaving a plain list item.
pub fn list_backspace(src: &str, caret: usize) -> Option<Edit> {
    let line = line_block(src, caret, caret);
    let text = &src[line.clone()];
    let p = parse_list_line(text)?;
    if let Some(t) = task_box(text, &p) {
        let after = (t.end + usize::from(text[t.end..].starts_with(' '))).min(text.len());
        if caret == line.start + after && t.end < text.len() || caret == line.start + t.end {
            return Some(Edit {
                range: line.start + t.start..caret,
                text: String::new(),
                caret: line.start + t.start,
                anchor: None,
            });
        }
    }
    (caret == line.start + p.content).then(|| Edit {
        range: line.start..caret,
        text: String::new(),
        caret: line.start,
        anchor: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_byte_roundtrip() {
        let s = "Příliš 🙂 ž";
        for (c, (b, _)) in s.char_indices().enumerate() {
            assert_eq!(char_to_byte(s, c), b);
            assert_eq!(byte_to_char(s, b), c);
        }
        assert_eq!(char_to_byte(s, 99), s.len());
    }

    #[test]
    fn image_ref_in_middle_of_line_gets_own_line() {
        let mut s = String::from("ab\ncd ef\n");
        let e = insert_image_ref(&s, 5, "assets/x.png");
        e.apply(&mut s);
        assert_eq!(s, "ab\ncd\n![image](assets/x.png)\n ef\n");
        assert_eq!(&s[e.caret..], " ef\n");
    }

    #[test]
    fn image_ref_on_empty_line_and_eof() {
        let mut s = String::from("a\n\nb");
        let e = insert_image_ref(&s, 2, "u.png");
        e.apply(&mut s);
        assert_eq!(s, "a\n![image](u.png)\nb");
        assert_eq!(&s[e.caret..], "b");
        let mut s = String::from("end");
        let e = insert_image_ref(&s, 3, "u.png");
        e.apply(&mut s);
        assert_eq!(s, "end\n![image](u.png)\n");
        assert_eq!(e.caret, s.len());
    }

    #[test]
    fn slash_trigger_rules() {
        assert!(is_slash_trigger("/", 0));
        assert!(is_slash_trigger("a /", 2));
        assert!(!is_slash_trigger("a/b", 1));
        assert!(!is_slash_trigger("http://x", 5));
    }

    fn run_slash(src: &str, q: &str, title: &str) -> (String, Edit) {
        let trig = src.find(q).unwrap();
        let caret = trig + q.len();
        let cmd = SLASH_COMMANDS.iter().find(|c| c.title == title).unwrap();
        let e = apply_slash(src, trig, caret, cmd).unwrap();
        let mut s = src.to_string();
        e.apply(&mut s);
        (s, e)
    }

    #[test]
    fn heading_from_plain_text_levels() {
        for (t, h) in [
            ("Heading 1", "# "),
            ("Heading 2", "## "),
            ("Heading 3", "### "),
        ] {
            let (s, e) = run_slash("one\nPříliš žluťoučký /h\ntwo", "/h", t);
            assert_eq!(s, format!("one\n{h}Příliš žluťoučký \ntwo"));
            assert_eq!(e.caret, 4 + h.len() + "Příliš žluťoučký ".len());
        }
        // Empty line behaves like the old template.
        let (s, e) = run_slash("a\n/h1\nb", "/h1", "Heading 1");
        assert_eq!(s, "a\n# \nb");
        assert_eq!(e.caret, 4);
    }

    #[test]
    fn heading_conversion_replaces_existing_markers() {
        let (s, _) = run_slash("## Title /h", "/h", "Heading 1");
        assert_eq!(s, "# Title ");
        let (s, _) = run_slash("# Title /h", "/h", "Heading 3");
        assert_eq!(s, "### Title ");
        let (s, _) = run_slash("- item /h", "/h", "Heading 2");
        assert_eq!(s, "## item ");
        // Mid-line query keeps the text after it.
        let (s, e) = run_slash("a /h tail", "/h", "Heading 2");
        assert_eq!(s, "## a  tail");
        assert_eq!(&s[e.caret..], " tail");
    }

    #[test]
    fn inline_selection_wraps() {
        for (d, name) in [("**", "bold"), ("*", "italic"), ("`", "code")] {
            let src = "a Příliš 🦀 ž b";
            let (a, b) = (2, src.len() - 2);
            let e = format_inline(src, a, b, d);
            let mut s = src.to_string();
            e.apply(&mut s);
            assert_eq!(s, format!("a {d}Příliš 🦀 ž{d} b"), "{name}");
            assert_eq!(
                (e.anchor, e.caret),
                (Some(2), 2 + d.len() * 2 + "Příliš 🦀 ž".len())
            );
            assert!(s.is_char_boundary(e.caret));
        }
    }

    #[test]
    fn inline_selection_trims_whitespace_and_multiline() {
        let e = format_inline("x word  y", 1, 8, "**");
        let mut s = "x word  y".to_string();
        e.apply(&mut s);
        assert_eq!(s, "x **word**  y");
        let src = "one two\n\nthree";
        let e = format_inline(src, 0, src.len(), "*");
        let mut s = src.to_string();
        e.apply(&mut s);
        assert_eq!(s, "*one two*\n\n*three*");
    }

    #[test]
    fn inline_empty_selection_inserts_pair() {
        for d in ["**", "*", "`"] {
            let e = format_inline("ab cd", 3, 3, d);
            let mut s = "ab cd".to_string();
            e.apply(&mut s);
            assert_eq!(s, format!("ab {d}{d}cd"));
            assert_eq!(e.caret, 3 + d.len());
            assert_eq!(e.anchor, None);
        }
        // Slash path removes the query.
        let (s, e) = run_slash("x /bold y", "/bold", "Bold");
        assert_eq!(s, "x **** y");
        assert_eq!(e.caret, 4);
    }

    #[test]
    fn inline_toggle_and_valid_nesting() {
        // Toggle off (delimiters outside, then inside the selection).
        let e = format_inline("a **b** c", 4, 5, "**");
        let mut s = "a **b** c".to_string();
        e.apply(&mut s);
        assert_eq!(s, "a b c");
        let e = format_inline("a **b** c", 2, 7, "**");
        let mut s = "a **b** c".to_string();
        e.apply(&mut s);
        assert_eq!(s, "a b c");
        // Italic inside bold nests as `***x***`, and toggles back.
        let e = format_inline("**x**", 2, 3, "*");
        let mut s = "**x**".to_string();
        e.apply(&mut s);
        assert_eq!(s, "***x***");
        let e = format_inline("***x***", 3, 4, "*");
        let mut s = "***x***".to_string();
        e.apply(&mut s);
        assert_eq!(s, "**x**");
        // Bold inside italic.
        let e = format_inline("*x*", 1, 2, "**");
        let mut s = "*x*".to_string();
        e.apply(&mut s);
        assert_eq!(s, "***x***");
        // Code containing backticks gets a longer fence.
        let e = format_inline("a`b", 0, 3, "`");
        let mut s = "a`b".to_string();
        e.apply(&mut s);
        assert_eq!(s, "``a`b``");
    }

    #[test]
    fn lists_from_text_and_slash() {
        let (s, e) = run_slash("Buy milk /b", "/b", "Bullet list");
        assert_eq!(s, "- Buy milk ");
        assert_eq!(e.caret, s.len());
        let (s, e) = run_slash("a\n/bullet", "/bullet", "Bullet list");
        assert_eq!((s.as_str(), e.caret), ("a\n- ", 4));
        let (s, e) = run_slash("a\n/num", "/num", "Numbered list");
        assert_eq!((s.as_str(), e.caret), ("a\n1. ", 5));
        let (s, _) = run_slash("1. x /n", "/n", "Numbered list");
        assert_eq!(s, "1. x ");
        let (s, _) = run_slash("- x /n", "/n", "Numbered list");
        assert_eq!(s, "1. x ");
        // Selection path: several lines, blank lines kept, numbering restarts, toggles off.
        let src = "alpha\nbeta\n\nžluť";
        let e = list_selection(src, 0, src.len(), true);
        assert_eq!(e.text, "1. alpha\n2. beta\n\n3. žluť");
        let e = list_selection(src, 0, 5, false);
        assert_eq!(e.text, "- alpha");
        let src = "- a\n- b";
        assert_eq!(list_selection(src, 0, src.len(), false).text, "a\nb");
        assert_eq!(list_selection(src, 0, src.len(), true).text, "1. a\n2. b");
        // Selection ending at a line start excludes the next line.
        let src = "a\nb";
        assert_eq!(list_selection(src, 0, 2, false).range, 0..1);
    }

    #[test]
    fn list_line_parsing() {
        assert_eq!(parse_list_line("- x").unwrap().content, 2);
        assert_eq!(parse_list_line("  12. x").unwrap().content, 6);
        assert_eq!(
            parse_list_line("3) x").unwrap().kind,
            ListKind::Number(3, ')')
        );
        for bad in ["-x", "---", "* * *", "- - -", "1.x", "x - y", "#", ""] {
            assert!(parse_list_line(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn enter_continues_and_terminates() {
        let mut s = "- one".to_string();
        let e = list_enter(&s, 5).unwrap();
        e.apply(&mut s);
        assert_eq!(s, "- one\n- ");
        assert_eq!(e.caret, s.len());
        // Empty item ends the list.
        let e = list_enter(&s, s.len()).unwrap();
        e.apply(&mut s);
        assert_eq!(s, "- one\n");
        assert_eq!(e.caret, s.len());
        // Numbers and indentation.
        let mut s = "  9. nine".to_string();
        list_enter(&s, s.len()).unwrap().apply(&mut s);
        assert_eq!(s, "  9. nine\n  10. ");
        // Not at end of line, or not a list line: ordinary Enter.
        assert!(list_enter("- one", 3).is_none());
        assert!(list_enter("plain", 5).is_none());
        assert!(list_enter("---", 3).is_none());
        // Middle of a document.
        let mut s = "- a\n\nafter".to_string();
        let e = list_enter(&s, 3).unwrap();
        e.apply(&mut s);
        assert_eq!(s, "- a\n- \n\nafter");
    }

    #[test]
    fn backspace_removes_marker() {
        let mut s = "x\n- item".to_string();
        let e = list_backspace(&s, 4).unwrap();
        e.apply(&mut s);
        assert_eq!(s, "x\nitem");
        assert_eq!(e.caret, 2);
        assert!(list_backspace("- item", 3).is_none());
        assert!(list_backspace("- item", 1).is_none());
        let mut s = "1. ".to_string();
        list_backspace(&s, 3).unwrap().apply(&mut s);
        assert_eq!(s, "");
        // Code-block-ish text without a space is untouched.
        assert!(list_backspace("-item", 1).is_none());
    }

    #[test]
    fn code_block_template_and_non_slash_text() {
        let (s, e) = run_slash("x /fence", "/fence", "Code block");
        assert_eq!(s, "x \n```\n\n```");
        assert_eq!(&s[..e.caret], "x \n```\n");
    }

    #[test]
    fn table_command_inserts_valid_table_and_selects_first_cell() {
        for (src, q) in [
            ("/table", "/table"),
            ("para\n/table", "/table"),
            ("para\n\n/table\n\nafter", "/table"),
            ("a /tab\nnext", "/tab"),
        ] {
            let (s, e) = run_slash(src, q, "Table");
            let a = e.anchor.unwrap();
            assert_eq!(&s[a..e.caret], "Data 1", "{src:?}");
            let spans = crate::markdown::parse(&s);
            let t = spans.iter().find_map(|sp| match &sp.kind {
                crate::markdown::Kind::Table(t) => Some(t),
                _ => None,
            });
            let t = t.unwrap_or_else(|| panic!("no table in {s:?}"));
            assert_eq!(t.columns(), 2);
            assert_eq!(t.rows.len(), 2);
            assert!(!s.contains("/tab"));
        }
        let (s, _) = run_slash("para\n/table", "/table", "Table");
        assert!(s.starts_with("para\n\n| Column 1 | Column 2 |\n| --- | --- |\n"));
    }

    #[test]
    fn filter_all_when_empty() {
        assert_eq!(slash_filter("").len(), SLASH_COMMANDS.len());
        assert!(slash_filter("zzz").is_empty());
    }

    #[test]
    fn menu_contents_and_emoji_insertion() {
        let titles: Vec<_> = SLASH_COMMANDS.iter().map(|c| c.title).collect();
        assert_eq!(
            titles,
            [
                "Heading 1",
                "Heading 2",
                "Heading 3",
                "Bold",
                "Italic",
                "Inline code",
                "Bullet list",
                "Numbered list",
                "Task list",
                "Code block",
                "Table",
                "Emoji"
            ]
        );
        let r = slash_filter("emoji");
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].action, SlashAction::Emoji);
        assert!(apply_slash("/emoji", 0, 6, &r[0]).is_none());

        for g in [
            "\u{2764}\u{FE0F}",
            "🇨🇿",
            "👨\u{200D}👩\u{200D}👧",
            "🦀",
            "👍\u{1F3FD}",
        ] {
            for (src, trig) in [("/emoji", 0), ("# Tit **b/emoji** x", 9), ("a /emoji", 2)] {
                let end = trig + "/emoji".len();
                let end = if src.len() >= end && &src[trig..end] == "/emoji" {
                    end
                } else {
                    continue;
                };
                let mut s = src.to_string();
                let e = apply_emoji(trig, end, g);
                e.apply(&mut s);
                assert_eq!(&s[trig..trig + g.len()], g);
                assert_eq!(e.caret, trig + g.len());
                assert!(!s.contains("emoji"));
                assert!(s.is_char_boundary(e.caret));
            }
        }
    }

    fn enter(src: &str) -> String {
        let mut s = src.to_string();
        let e = list_enter(src, src.len()).unwrap();
        e.apply(&mut s);
        s
    }

    #[test]
    fn toggling_a_task_changes_exactly_one_byte() {
        for (src, start, want) in [
            ("- [ ] Příliš 🦀\n", 2, "- [x] Příliš 🦀\n"),
            ("- [x] a\n", 2, "- [ ] a\n"),
            ("- [X] a", 2, "- [ ] a"),
            ("ž\n1. [ ] n", 6, "ž\n1. [x] n"),
        ] {
            let e = toggle_task(src, start).unwrap();
            assert_eq!(e.range.len(), 1);
            let mut s = src.to_string();
            e.apply(&mut s);
            assert_eq!(s, want);
            assert_eq!(s.len(), src.len());
            assert!(s.is_char_boundary(e.caret));
            // Toggling again flips the same byte back.
            let mut back = s.clone();
            toggle_task(&s, start).unwrap().apply(&mut back);
            assert_eq!(back.len(), src.len());
            assert_eq!(back == src, !src.contains("[X]"));
        }
        assert!(toggle_task("- [y] a", 2).is_none());
        assert!(toggle_task("- a", 2).is_none());
        assert!(toggle_task("", 0).is_none());
    }

    #[test]
    fn enter_continues_tasks_with_an_unchecked_box_and_ends_on_an_empty_one() {
        assert_eq!(enter("- [ ] one"), "- [ ] one\n- [ ] ");
        assert_eq!(enter("- [x] hotovo"), "- [x] hotovo\n- [ ] ");
        assert_eq!(enter("  * [x] Příliš"), "  * [x] Příliš\n  * [ ] ");
        assert_eq!(enter("1. [ ] a"), "1. [ ] a\n2. [ ] ");
        // Empty task (with or without the trailing space) ends the list.
        assert_eq!(enter("- [ ] one\n- [ ] "), "- [ ] one\n");
        assert_eq!(enter("- [x]"), "");
        // Ordinary lists are unchanged, and "[ ]" later in the text is not a task.
        assert_eq!(enter("- item"), "- item\n- ");
        assert_eq!(enter("1. item"), "1. item\n2. ");
        assert_eq!(enter("- a [ ] b"), "- a [ ] b\n- ");
        assert_eq!(enter("- "), "");
        assert_eq!(enter("- [ ]x"), "- [ ]x\n- ");
    }

    #[test]
    fn backspace_after_a_task_box_leaves_a_plain_item() {
        let mut s = "- [ ] text".to_string();
        list_backspace(&s, 6).unwrap().apply(&mut s);
        assert_eq!(s, "- text");
        let mut s = "- [x]".to_string();
        list_backspace(&s, 5).unwrap().apply(&mut s);
        assert_eq!(s, "- ");
        // Backspace in the middle of text is left to the text edit.
        assert!(list_backspace("- [ ] text", 8).is_none());
    }

    #[test]
    fn strikethrough_formatting_toggles_wraps_and_nests() {
        let f = |src: &str, a: usize, b: usize| {
            let e = format_inline(src, a, b, "~~");
            let mut s = src.to_string();
            e.apply(&mut s);
            (s, e)
        };
        let src = "one Příliš 🦀 two";
        let (a, b) = (4, 4 + "Příliš 🦀".len());
        let (s, e) = f(src, a, b);
        assert_eq!(s, "one ~~Příliš 🦀~~ two");
        assert_eq!(&s[e.anchor.unwrap()..e.caret], "~~Příliš 🦀~~");
        // Toggle off, with the selection outside or inside the delimiters.
        let (s2, _) = f(&s, a + 2, b + 2);
        assert_eq!(s2, src);
        let (s3, _) = f(&s, a, b + 4);
        assert_eq!(s3, src);
        // Empty selection inserts a pair.
        let (s4, e4) = f("ab", 1, 1);
        assert_eq!((s4.as_str(), e4.caret), ("a~~~~b", 3));
        // Wrapping already-bold text keeps both valid; removing the strike keeps the bold.
        let (s5, _) = f("**bold**", 2, 6);
        assert_eq!(
            s5,
            "****~~bold~~**".replace("****~~bold~~**", "**~~bold~~**")
        );
        let (s6, _) = f(&s5, 4, 8);
        assert_eq!(s6, "**bold**");
        // Multi-line selections are wrapped per line.
        let (s7, _) = f("a\nb", 0, 3);
        assert_eq!(s7, "~~a~~\n~~b~~");
    }

    #[test]
    fn task_command_is_found_by_task_todo_and_checkbox() {
        for q in ["task", "todo", "checkbox", "Task", "tas"] {
            let r = slash_filter(q);
            assert!(r.iter().any(|c| c.action == SlashAction::Task), "{q}");
        }
        assert_eq!(slash_filter("todo").len(), 1);
        // Next to the other list commands.
        let t: Vec<_> = SLASH_COMMANDS.iter().map(|c| c.title).collect();
        let at = t.iter().position(|x| *x == "Task list").unwrap();
        assert_eq!(&t[at - 2..at], ["Bullet list", "Numbered list"]);
    }

    #[test]
    fn task_command_on_empty_and_populated_lines() {
        // Empty line, caret right after "- [ ] ".
        let (s, e) = run_slash("/task", "/task", "Task list");
        assert_eq!((s.as_str(), e.caret), ("- [ ] ", 6));
        let (s, e) = run_slash("a\n/todo", "/todo", "Task list");
        assert_eq!((s.as_str(), e.caret), ("a\n- [ ] ", 8));
        // Indentation kept.
        let (s, e) = run_slash("  /task", "/task", "Task list");
        assert_eq!((s.as_str(), e.caret), ("  - [ ] ", 8));
        // Text on the line is kept; only the query goes.
        let (s, e) = run_slash("Koupit mléko /task", "/task", "Task list");
        assert_eq!(s, "- [ ] Koupit mléko ");
        assert_eq!(e.caret, s.len());
        let (s, e) = run_slash("Příliš /chec žlutý", "/chec", "Task list");
        assert_eq!(s, "- [ ] Příliš  žlutý");
        assert_eq!(&s[e.caret..], " žlutý");
        // Conversion of bullet and numbered markers, indentation kept.
        let (s, _) = run_slash("- x /t", "/t", "Task list");
        assert_eq!(s, "- [ ] x ");
        let (s, _) = run_slash("* x /t", "/t", "Task list");
        assert_eq!(s, "- [ ] x ");
        let (s, _) = run_slash("12. x /t", "/t", "Task list");
        assert_eq!(s, "- [ ] x ");
        let (s, _) = run_slash("a\n  1. x /t\nb", "/t", "Task list");
        assert_eq!(s, "a\n  - [ ] x \nb");
        // A task stays a task (state kept).
        let (s, _) = run_slash("- [x] done /t", "/t", "Task list");
        assert_eq!(s, "- [x] done ");
        // Empty bullet / number.
        let (s, e) = run_slash("- /t", "/t", "Task list");
        assert_eq!((s.as_str(), e.caret), ("- [ ] ", 6));
        // Result is a task for the parser.
        for src in ["/task", "a /t", "1. x /t", "  /t"] {
            let q = if src.contains("/task") { "/task" } else { "/t" };
            let (s, _) = run_slash(src, q, "Task list");
            assert!(
                crate::markdown::parse(&s)
                    .iter()
                    .any(|sp| matches!(sp.kind, crate::markdown::Kind::Task { checked: false })),
                "{s:?}"
            );
        }
    }

    #[test]
    fn image_width_edit_creates_changes_and_removes_only_the_annotation() {
        let src = "a ž\n![Příliš 🦀](obrázek.png)\nz";
        let end = src.find(")\n").unwrap() + 1;
        let mut s = src.to_string();
        let e = image_width_edit(end..end, Some(60));
        e.apply(&mut s);
        assert_eq!(s, "a ž\n![Příliš 🦀](obrázek.png){width=60%}\nz");
        let ann = end..end + "{width=60%}".len();
        let mut t = s.clone();
        image_width_edit(ann.clone(), Some(35)).apply(&mut t);
        assert_eq!(t, "a ž\n![Příliš 🦀](obrázek.png){width=35%}\nz");
        let mut r = t.clone();
        image_width_edit(end..end + "{width=35%}".len(), None).apply(&mut r);
        assert_eq!(r, src, "reset restores the exact original bytes");
        // Offsets before the edit stay, after it shift, inside it go to the end of the new text.
        let e = image_width_edit(end..end, Some(60));
        assert_eq!(map_offset(&e, 3), 3);
        assert_eq!(map_offset(&e, end), end);
        assert_eq!(map_offset(&e, end + 1), end + 1 + 11);
        let e = image_width_edit(ann, None);
        assert_eq!(map_offset(&e, end + 5), end);
        assert_eq!(map_offset(&e, end + 11 + 2), end + 2);
    }
}
