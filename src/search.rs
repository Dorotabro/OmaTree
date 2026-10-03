//! Simple note search over the active, in-memory notebook. Pure Rust.
//!
//! A note matches when the trimmed, lowercased query occurs in its title or
//! its body (lowercased the same way, with the standard library's Unicode
//! lowercase conversion). There is no ranking, indexing or fuzziness: results
//! are the matching notes with title matches first and body-only matches
//! after, each group in the order the tree shows them. A note appears once.

use crate::notebook::{NodeId, Notebook};

/// How many characters of context the snippet shows before the match.
const CONTEXT_BEFORE: usize = 20;
/// The longest snippet, in characters (an ellipsis may be added to it).
const SNIPPET_LEN: usize = 80;

/// One matching note, with what a result list needs to show it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchResult {
    /// Which note. Internal only: it is never handed to QML.
    pub id: NodeId,
    pub title: String,
    /// Ancestors, outermost first, as "A › B". Empty for a top-level note.
    pub breadcrumb: String,
    /// One short plain-text line: context around the match in the body, or
    /// the start of the body when only the title matched.
    pub snippet: String,
    /// Whether the title matches (the body may match too).
    pub title_match: bool,
}

/// All notes matching `query`, title matches first. An empty or
/// whitespace-only query matches nothing.
pub fn search(notebook: &Notebook, query: &str) -> Vec<SearchResult> {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return Vec::new();
    }

    let mut title_matches = Vec::new();
    let mut body_only = Vec::new();
    for node in notebook.depth_first() {
        let title_match = node.title().to_lowercase().contains(&needle);
        let body_hit = find_in_body(node.body(), &needle);
        if !title_match && body_hit.is_none() {
            continue;
        }
        let result = SearchResult {
            id: node.id(),
            title: node.title().to_string(),
            breadcrumb: notebook.breadcrumb(node.id()),
            snippet: snippet(node.body(), body_hit),
            title_match,
        };
        if title_match {
            title_matches.push(result);
        } else {
            body_only.push(result);
        }
    }
    title_matches.extend(body_only);
    title_matches
}

/// Where `needle` (already lowercase) first occurs in `body`, as a character
/// index into the *original* text. Lowercasing can turn one character into
/// several, so each lowered character remembers which original one it came
/// from; that keeps the position right and the snippet on character
/// boundaries.
fn find_in_body(body: &str, needle: &str) -> Option<usize> {
    let mut lowered = String::with_capacity(body.len());
    let mut origin: Vec<usize> = Vec::with_capacity(body.len());
    for (index, ch) in body.chars().enumerate() {
        for lower in ch.to_lowercase() {
            lowered.push(lower);
            origin.push(index);
        }
    }
    let byte = lowered.find(needle)?;
    let lowered_index = lowered[..byte].chars().count();
    origin.get(lowered_index).copied()
}

/// A short single-line snippet. With a body match at character `hit`: some
/// context before it and the text after it. Otherwise the first non-blank
/// line of the body. Line breaks and runs of whitespace become single spaces.
fn snippet(body: &str, hit: Option<usize>) -> String {
    match hit {
        Some(position) => {
            let chars: Vec<char> = body.chars().collect();
            let start = position.saturating_sub(CONTEXT_BEFORE);
            let end = (start + SNIPPET_LEN).min(chars.len());
            let window: String = chars[start..end].iter().collect();
            let mut line = collapse_whitespace(&window);
            if start > 0 {
                line.insert(0, '…');
            }
            if end < chars.len() {
                line.push('…');
            }
            line
        }
        None => {
            let first = body.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
            let line = collapse_whitespace(first);
            let chars: Vec<char> = line.chars().collect();
            if chars.len() > SNIPPET_LEN {
                let mut cut: String = chars[..SNIPPET_LEN].iter().collect();
                cut.push('…');
                cut
            } else {
                line
            }
        }
    }
}

fn collapse_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn titles(results: &[SearchResult]) -> Vec<&str> {
        results.iter().map(|r| r.title.as_str()).collect()
    }

    fn sample() -> (Notebook, [NodeId; 6]) {
        // OmaTree
        //   Release            (body: "Ship the first version")
        //   Notes about omatree
        // Threatwright
        //   Release            (body: "v0.2 planning")
        //     Checklist        (body: "TODO: write docs")
        let mut nb = Notebook::new();
        let omatree = nb.create_root("OmaTree").unwrap();
        let threat = nb.create_root("Threatwright").unwrap();
        let release1 = nb.create_child(omatree, "Release").unwrap();
        let notes = nb.create_child(omatree, "Notes about omatree").unwrap();
        let release2 = nb.create_child(threat, "Release").unwrap();
        let checklist = nb.create_child(release2, "Checklist").unwrap();
        nb.set_body(release1, "Ship the first version").unwrap();
        nb.set_body(release2, "v0.2 planning").unwrap();
        nb.set_body(checklist, "TODO: write docs").unwrap();
        (nb, [omatree, threat, release1, notes, release2, checklist])
    }

    #[test]
    fn an_empty_notebook_and_blank_queries_find_nothing() {
        assert!(search(&Notebook::new(), "anything").is_empty());
        let (nb, _) = sample();
        assert!(search(&nb, "").is_empty());
        assert!(search(&nb, "   \t\n ").is_empty());
    }

    #[test]
    fn titles_match_exactly_partially_and_ignoring_case() {
        let (nb, [omatree, ..]) = sample();
        for query in ["OmaTree", "omatree", "OMATREE", "omat", "  maTr ", "Tree"] {
            let results = search(&nb, query);
            assert!(results.iter().any(|r| r.id == omatree), "{query:?}");
        }
        assert!(search(&nb, "nothing like this").is_empty());
    }

    #[test]
    fn bodies_match_ignoring_case_and_punctuation_is_literal() {
        let (nb, [_, _, _, _, _, checklist]) = sample();
        for query in ["write docs", "WRITE DOCS", "todo:", "TODO: write"] {
            let results = search(&nb, query);
            assert_eq!(results.len(), 1, "{query:?}");
            assert_eq!(results[0].id, checklist);
            assert!(!results[0].title_match);
        }
        // No regex or operators: "." and "*" are just characters.
        assert!(search(&nb, "v0.2").len() == 1);
        assert!(search(&nb, "v0*2").is_empty());
        assert!(search(&nb, "t.d").is_empty());
    }

    #[test]
    fn a_note_matching_in_title_and_body_appears_once() {
        let mut nb = Notebook::new();
        let id = nb.create_root("Rust notes").unwrap();
        nb.set_body(id, "I like rust. Rust is nice; RUST!").unwrap();
        let results = search(&nb, "rust");
        assert_eq!(results.len(), 1);
        assert!(results[0].title_match);
    }

    #[test]
    fn title_matches_come_first_then_body_only_matches() {
        let mut nb = Notebook::new();
        let body_first = nb.create_root("Aaa").unwrap(); // body-only, earliest in the tree
        let title = nb.create_root("The omatree plan").unwrap();
        let body_second = nb.create_root("Zzz").unwrap();
        nb.set_body(body_first, "about omatree").unwrap();
        nb.set_body(body_second, "more OmaTree").unwrap();
        let results = search(&nb, "omatree");
        assert_eq!(
            results.iter().map(|r| r.id).collect::<Vec<_>>(),
            [title, body_first, body_second]
        );
        assert_eq!(
            results.iter().map(|r| r.title_match).collect::<Vec<_>>(),
            [true, false, false]
        );
    }

    #[test]
    fn tree_order_is_kept_inside_each_group() {
        // Depth first as the tree shows it, not alphabetical and not by id.
        let mut nb = Notebook::new();
        let b = nb.create_root("b item").unwrap();
        let a = nb.create_root("a item").unwrap();
        let b_child = nb.create_child(b, "z item").unwrap();
        let a_child = nb.create_child(a, "y item").unwrap();
        let b_child2 = nb.create_child(b, "c item").unwrap();
        let results = search(&nb, "item");
        assert_eq!(
            results.iter().map(|r| r.id).collect::<Vec<_>>(),
            [b, b_child, b_child2, a, a_child]
        );
    }

    #[test]
    fn the_same_title_in_different_branches_gives_separate_results_with_paths() {
        let (nb, [_, _, release1, _, release2, _]) = sample();
        let results = search(&nb, "release");
        assert_eq!(results.len(), 2);
        assert_eq!(
            (results[0].id, results[0].breadcrumb.as_str()),
            (release1, "OmaTree")
        );
        assert_eq!(
            (results[1].id, results[1].breadcrumb.as_str()),
            (release2, "Threatwright")
        );
    }

    #[test]
    fn breadcrumbs_show_every_ancestor_and_roots_have_none() {
        let (nb, [omatree, _, _, _, _, checklist]) = sample();
        let deep = &search(&nb, "checklist")[0];
        assert_eq!(deep.id, checklist);
        assert_eq!(deep.breadcrumb, "Threatwright › Release");
        let root = search(&nb, "oMaTrEe")
            .into_iter()
            .find(|r| r.id == omatree)
            .unwrap();
        assert_eq!(root.breadcrumb, "", "a top-level note has no path");
        assert_eq!(nb.breadcrumb(checklist), "Threatwright › Release");
    }

    #[test]
    fn a_body_snippet_shows_context_around_the_match() {
        let (nb, _) = sample();
        let hit = &search(&nb, "first version")[0];
        assert_eq!(hit.snippet, "Ship the first version");

        let mut nb = Notebook::new();
        let id = nb.create_root("Long").unwrap();
        let body = format!("{} NEEDLE {}", "before ".repeat(20), "after ".repeat(40));
        nb.set_body(id, &body).unwrap();
        let hit = &search(&nb, "needle")[0];
        assert!(hit.snippet.starts_with('…') && hit.snippet.ends_with('…'));
        assert!(hit.snippet.contains("NEEDLE"));
        assert!(
            hit.snippet.chars().count() <= SNIPPET_LEN + 2,
            "{}",
            hit.snippet
        );
    }

    #[test]
    fn multiline_bodies_become_one_tidy_line() {
        let mut nb = Notebook::new();
        let id = nb.create_root("Notes").unwrap();
        nb.set_body(
            id,
            "first line\r\n\r\n   second\tline  has the TARGET\nlast",
        )
        .unwrap();
        let hit = &search(&nb, "target")[0];
        assert!(
            hit.snippet.ends_with("econd line has the TARGET last"),
            "{}",
            hit.snippet
        );
        assert!(!hit.snippet.contains("  "), "runs of whitespace collapse");
        assert!(!hit.snippet.contains('\n') && !hit.snippet.contains('\t'));
    }

    #[test]
    fn a_title_only_match_previews_the_start_of_the_body() {
        let mut nb = Notebook::new();
        let id = nb.create_root("Project").unwrap();
        nb.set_body(id, "\n\n  The first real line  \nsecond")
            .unwrap();
        let hit = &search(&nb, "project")[0];
        assert!(hit.title_match);
        assert_eq!(hit.snippet, "The first real line");
        let empty = nb.create_root("Project two").unwrap();
        let _ = empty;
        assert_eq!(
            search(&nb, "project two")[0].snippet,
            "",
            "no body, no snippet"
        );
        // A very long first line is cut on a character boundary.
        let long = nb.create_root("Project long").unwrap();
        nb.set_body(long, &"é".repeat(300)).unwrap();
        let cut = &search(&nb, "project long")[0].snippet;
        assert_eq!(cut.chars().count(), SNIPPET_LEN + 1);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn unicode_text_is_searched_and_snippets_never_split_characters() {
        let mut nb = Notebook::new();
        let id = nb.create_root("Café ☕ notes").unwrap();
        let body = format!(
            "{}žluťoučký kůň — naïve 日本語のテスト 🌳 ÉCLAIR {}",
            "ä".repeat(30),
            "ö".repeat(100)
        );
        nb.set_body(id, &body).unwrap();
        for query in [
            "café",
            "CAFÉ",
            "☕",
            "日本語",
            "🌳",
            "éclair",
            "ÉCLAIR",
            "žluťoučký",
            "KŮŇ",
            "naïve",
        ] {
            let results = search(&nb, query);
            assert_eq!(results.len(), 1, "{query:?}");
            // Building the snippet must have produced valid text of sane size.
            assert!(
                results[0].snippet.chars().count() <= SNIPPET_LEN + 2,
                "{query:?}"
            );
        }
        assert!(search(&nb, "ǆ").is_empty());
    }

    #[test]
    fn characters_whose_lowercase_is_longer_still_locate_correctly() {
        // 'İ' (U+0130) lowercases to two characters; the match position must
        // still point at the right place in the original text.
        let mut nb = Notebook::new();
        let id = nb.create_root("Turkish").unwrap();
        nb.set_body(id, "İİİİ marker İİİİ").unwrap();
        let hit = &search(&nb, "marker")[0];
        assert!(hit.snippet.contains("marker"), "{}", hit.snippet);
    }

    #[test]
    fn deleted_renamed_edited_and_moved_notes_are_reflected_by_later_searches() {
        let (mut nb, [omatree, threat, release1, _, release2, checklist]) = sample();
        assert_eq!(search(&nb, "release").len(), 2);

        nb.delete(release1).unwrap();
        let after_delete = search(&nb, "release");
        assert_eq!(after_delete.len(), 1);
        assert_eq!(after_delete[0].id, release2);

        nb.rename(release2, "Launch").unwrap();
        assert!(search(&nb, "release").is_empty());
        assert_eq!(titles(&search(&nb, "launch")), ["Launch"]);

        nb.set_body(checklist, "Totally different words").unwrap();
        assert!(search(&nb, "write docs").is_empty());
        assert_eq!(search(&nb, "different words")[0].id, checklist);

        // Moving it changes only the path.
        assert_eq!(
            search(&nb, "checklist")[0].breadcrumb,
            "Threatwright › Launch"
        );
        nb.move_node(checklist, Some(omatree), 0).unwrap();
        let moved = &search(&nb, "checklist")[0];
        assert_eq!(moved.breadcrumb, "OmaTree");
        let _ = threat;
    }

    #[test]
    fn depth_first_matches_the_tree_order_and_is_a_single_pass() {
        let (nb, [omatree, threat, release1, notes, release2, checklist]) = sample();
        let order: Vec<NodeId> = nb.depth_first().iter().map(|n| n.id()).collect();
        assert_eq!(
            order,
            [omatree, release1, notes, threat, release2, checklist]
        );
    }

    #[test]
    fn searching_changes_nothing() {
        let (nb, _) = sample();
        let before = nb.snapshot_nodes();
        let _ = search(&nb, "release");
        let _ = search(&nb, "zzz");
        assert_eq!(nb.snapshot_nodes(), before);
    }
}
