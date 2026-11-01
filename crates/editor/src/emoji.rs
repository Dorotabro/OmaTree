//! Emoji search: the full Unicode catalogue (from the `emojis` crate, no runtime I/O) behind a
//! compact curated "common" set. Pure data; no egui.
//!
//! `EMOJIS` holds the common set shown for an empty query, plus extra search aliases. To add one,
//! append an `e(glyph, name, keywords)` line (the name is only a fallback; the catalogue's Unicode
//! name is displayed). Glyphs with U+FE0F, ZWJ or regional indicators are written with `\u{..}`
//! escapes so the exact sequence survives editors that normalise invisible characters.

use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Emoji {
    pub glyph: &'static str,
    pub name: &'static str,
    /// Extra search aliases (curated entries only), lowercase.
    pub keywords: &'static str,
}

const fn e(glyph: &'static str, name: &'static str, keywords: &'static str) -> Emoji {
    Emoji {
        glyph,
        name,
        keywords,
    }
}

pub const EMOJIS: &[Emoji] = &[
    // Expressions
    e(
        "🧐",
        "face with monocle",
        "monocle inspect curious investigate",
    ),
    e("🤡", "clown face", "clown joker circus"),
    e("🙈", "see-no-evil monkey", "monkey shy embarrassed hide"),
    e("😀", "grinning face", "smile happy"),
    e(
        "😅",
        "grinning face with sweat",
        "sweat smile relief nervous phew",
    ),
    e("😂", "face with tears of joy", "lol laugh funny cry"),
    e("🤣", "rolling on the floor laughing", "rofl lmao"),
    e("🙂", "slightly smiling face", "smile ok"),
    e("😉", "winking face", "wink"),
    e("😊", "smiling face with smiling eyes", "blush happy"),
    e("😍", "smiling face with heart-eyes", "love adore"),
    e("😘", "face blowing a kiss", "kiss love"),
    e("😎", "smiling face with sunglasses", "cool sunglasses"),
    e("🤔", "thinking face", "hmm think consider"),
    e("🤨", "face with raised eyebrow", "skeptical suspicious"),
    e("😐", "neutral face", "meh blank"),
    e("🙄", "face with rolling eyes", "eyeroll whatever"),
    e("😴", "sleeping face", "sleep zzz tired"),
    e("😢", "crying face", "sad tear"),
    e("😭", "loudly crying face", "sob cry sad"),
    e("😡", "enraged face", "angry mad rage"),
    e("😱", "face screaming in fear", "scream shock scared"),
    e("🤯", "exploding head", "mind blown shocked"),
    e("🥳", "partying face", "party celebrate birthday"),
    e("🥺", "pleading face", "puppy eyes please"),
    e("😇", "smiling face with halo", "angel innocent"),
    e("🤗", "smiling face with open hands", "hug"),
    e("🫡", "saluting face", "salute respect"),
    e("😬", "grimacing face", "awkward grimace"),
    e("🤓", "nerd face", "nerd geek"),
    e("😈", "smiling face with horns", "devil evil"),
    e("💀", "skull", "dead death"),
    e("👻", "ghost", "boo halloween"),
    e("🤖", "robot", "bot ai"),
    e("💩", "pile of poo", "poop"),
    // Gestures
    e("👍", "thumbs up", "yes approve like +1 ok"),
    e("👎", "thumbs down", "no dislike -1"),
    e("👏", "clapping hands", "clap applause bravo"),
    e("🙌", "raising hands", "hooray celebrate praise"),
    e("🙏", "folded hands", "pray please thanks"),
    e("🤝", "handshake", "deal agreement"),
    e("👋", "waving hand", "wave hello bye"),
    e("👌", "OK hand", "ok perfect"),
    e("\u{270C}\u{FE0F}", "victory hand", "peace"),
    e("🤞", "crossed fingers", "luck hope"),
    e("💪", "flexed biceps", "strong muscle"),
    e("👀", "eyes", "look see watching"),
    e("🧠", "brain", "mind smart"),
    e("\u{270D}\u{FE0F}", "writing hand", "write"),
    e("👉", "backhand index pointing right", "point right"),
    e("👆", "backhand index pointing up", "point up"),
    // People
    e(
        "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}",
        "family: man, woman, girl",
        "family parents child",
    ),
    e("🧑‍💻", "technologist", "developer programmer coder"),
    // Hearts and symbols
    e("\u{2764}\u{FE0F}", "red heart", "love heart"),
    e("💔", "broken heart", "heartbreak sad"),
    e("💙", "blue heart", "heart"),
    e("💚", "green heart", "heart"),
    e("🔥", "fire", "hot lit flame"),
    e("✨", "sparkles", "shiny new magic"),
    e("⭐", "star", "favorite"),
    e("🎉", "party popper", "tada celebrate congrats"),
    e("💯", "hundred points", "100 perfect score"),
    e("💡", "light bulb", "idea"),
    e("\u{2705}", "check mark button", "done yes ok tick complete"),
    e("\u{2714}\u{FE0F}", "check mark", "done tick"),
    e("\u{2611}\u{FE0F}", "check box with check", "todo done"),
    e("\u{274C}", "cross mark", "no wrong delete x"),
    e("\u{2753}", "question mark", "why help"),
    e("\u{2757}", "exclamation mark", "important alert"),
    e("\u{26A0}\u{FE0F}", "warning", "caution alert danger"),
    e("🚫", "prohibited", "forbidden no"),
    e("⛔", "no entry", "stop forbidden"),
    e("ℹ️", "information", "info note"),
    e("➡️", "right arrow", "next"),
    e("⬅️", "left arrow", "back previous"),
    e("🔁", "repeat button", "loop repeat"),
    e(
        "🔄",
        "counterclockwise arrows button",
        "refresh sync reload",
    ),
    e("\u{1F7E2}", "green circle", "ok go status"),
    e("\u{1F7E1}", "yellow circle", "warning status"),
    e("\u{1F534}", "red circle", "stop error status"),
    e("🇨🇿", "flag: Czechia", "czech czechia flag cz"),
    e("🇺🇸", "flag: United States", "usa america flag us"),
    e("🇬🇧", "flag: United Kingdom", "uk britain flag gb"),
    // Objects and technical
    e("🚀", "rocket", "launch ship deploy fast"),
    e("💻", "laptop", "computer code"),
    e("\u{2328}\u{FE0F}", "keyboard", "type keys"),
    e("🖥️", "desktop computer", "monitor screen"),
    e("🔒", "locked", "lock secure private"),
    e("🔓", "unlocked", "unlock open"),
    e("🔑", "key", "password secret"),
    e("📝", "memo", "note write document pencil"),
    e("📄", "page facing up", "document file"),
    e("📁", "file folder", "directory"),
    e("📌", "pushpin", "pin important"),
    e("📎", "paperclip", "attachment"),
    e("🔗", "link", "chain url"),
    e("🔍", "magnifying glass tilted left", "search find zoom"),
    e("📅", "calendar", "date schedule"),
    e("⏰", "alarm clock", "time reminder"),
    e("📦", "package", "box release crate"),
    e("🏷️", "label", "tag"),
    e("🗑️", "wastebasket", "trash delete bin"),
    e("\u{2699}\u{FE0F}", "gear", "settings config"),
    e("🛠️", "hammer and wrench", "tools build fix"),
    e("🔧", "wrench", "tool fix"),
    e("🧪", "test tube", "test experiment"),
    e("🐛", "bug", "insect defect"),
    e("🐞", "lady beetle", "bug ladybug"),
    e("📊", "bar chart", "stats graph"),
    e("📈", "chart increasing", "growth up"),
    e("📉", "chart decreasing", "down decline"),
    e("🧩", "puzzle piece", "plugin extension"),
    e("🔔", "bell", "notification alert"),
    e("📧", "e-mail", "email mail message"),
    e("📷", "camera", "photo picture"),
    e("🎯", "bullseye", "target goal"),
    e("🏆", "trophy", "winner award"),
    e("🎨", "artist palette", "art design colors"),
    e("📚", "books", "read library docs"),
    e("🔬", "microscope", "science research"),
    e("☕", "hot beverage", "coffee tea"),
    e("🍕", "pizza", "food"),
    e("🥒", "cucumber", "pickle vegetable"),
    e("🍎", "red apple", "fruit"),
    e("🍺", "beer mug", "drink"),
    e("🎵", "musical note", "music"),
    // Animals and nature
    e("🦀", "crab", "rust ferris crustacean"),
    e("🐧", "penguin", "linux tux"),
    e("🐍", "snake", "python"),
    e("🐱", "cat face", "kitten pet"),
    e("🐶", "dog face", "puppy pet"),
    e("🦊", "fox", "firefox"),
    e("🐙", "octopus", "github"),
    e("🐢", "turtle", "slow"),
    e("🦄", "unicorn", "magic"),
    e("🐝", "honeybee", "bee"),
    e("🦉", "owl", "wise night"),
    e("🌳", "deciduous tree", "tree plant forest"),
    e("🌱", "seedling", "sprout grow new"),
    e("🌍", "globe showing Europe-Africa", "world earth"),
    e("\u{2600}\u{FE0F}", "sun", "sunny weather"),
    e("🌙", "crescent moon", "night"),
    e("\u{2744}\u{FE0F}", "snowflake", "cold winter"),
    e("⚡", "high voltage", "lightning fast power"),
];

/// One searchable catalogue entry (built once).
struct Entry {
    emoji: Emoji,
    name_lc: String,
    keywords: String,
    /// Position in the curated common set; ranks ahead of the rest on ties.
    common: Option<usize>,
    /// A non-default skin-tone variant: only searched when the query mentions tones.
    toned: bool,
}

fn index() -> &'static [Entry] {
    static INDEX: OnceLock<Vec<Entry>> = OnceLock::new();
    INDEX.get_or_init(|| {
        let alias = |g: &str| {
            EMOJIS
                .iter()
                .enumerate()
                .find(|(_, c)| emojis::get(c.glyph).is_some_and(|x| x.as_str() == g))
        };
        let mk = |x: &'static emojis::Emoji, toned: bool| {
            let curated = alias(x.as_str());
            let mut kw: Vec<String> = x.shortcodes().map(|c| c.replace('_', " ")).collect();
            if let Some((_, c)) = curated {
                kw.push(c.keywords.to_string());
            }
            Entry {
                emoji: Emoji {
                    glyph: x.as_str(),
                    name: x.name(),
                    keywords: "",
                },
                name_lc: x.name().to_lowercase(),
                keywords: kw.join(" ").to_lowercase(),
                common: curated.map(|(i, _)| i),
                toned,
            }
        };
        // `iter()` yields one default-tone entry per emoji; tone variants hang off `skin_tones()`.
        let mut all = Vec::new();
        for x in emojis::iter() {
            all.push(mk(x, false));
            for v in x.skin_tones().into_iter().flatten() {
                if v.skin_tone() != Some(emojis::SkinTone::Default) {
                    all.push(mk(v, true));
                }
            }
        }
        all
    })
}

fn words(s: &str) -> impl Iterator<Item = &str> {
    s.split(|c: char| !c.is_alphanumeric() && c != '+')
        .filter(|w| !w.is_empty())
}

/// Rank one entry against lowercase query tokens (lower is better); `None` if any token misses.
fn score(en: &Entry, tokens: &[String]) -> Option<u32> {
    let mut total = 0;
    for t in tokens {
        let t = t.as_str();
        let s = if en.name_lc == t || words(&en.keywords).any(|k| k == t) {
            0
        } else if words(&en.name_lc).any(|w| w.starts_with(t)) {
            1
        } else if words(&en.keywords).any(|w| w.starts_with(t)) {
            2
        } else if en.name_lc.contains(t) || en.keywords.contains(t) {
            3
        } else {
            return None;
        };
        total += s;
    }
    Some(total)
}

/// The compact common set (empty query).
pub fn common() -> Vec<Emoji> {
    EMOJIS
        .iter()
        .filter_map(|c| emojis::get(c.glyph))
        .map(|x| Emoji {
            glyph: x.as_str(),
            name: x.name(),
            keywords: "",
        })
        .collect()
}

/// Emoji matching every whitespace-separated token of `query`, best first. An empty query returns
/// the common set; any query searches the whole catalogue (skin-tone variants only when the query
/// mentions skin, tone, light, medium or dark).
pub fn search(query: &str) -> Vec<Emoji> {
    let tokens: Vec<String> = query
        .replace('_', " ")
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    if tokens.is_empty() {
        return common();
    }
    let tones = tokens.iter().any(|t| {
        ["skin", "tone", "light", "medium", "dark"]
            .iter()
            .any(|k| k.starts_with(t.as_str()))
    });
    let mut hits: Vec<(u32, usize, usize, Emoji)> = index()
        .iter()
        .enumerate()
        .filter(|(_, en)| tones || !en.toned)
        .filter_map(|(i, en)| {
            score(en, &tokens).map(|s| (s, en.common.unwrap_or(usize::MAX), i, en.emoji))
        })
        .collect();
    hits.sort_by_key(|(s, c, i, _)| (*s, *c, *i));
    hits.into_iter().map(|(.., em)| em).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(q: &str) -> &'static str {
        search(q)[0].glyph
    }

    #[test]
    fn requested_searches() {
        assert_eq!(first("crab"), "🦀");
        assert_eq!(first("sweat"), "😅");
        assert_eq!(first("rust"), "🦀");
        assert_eq!(first("monocle"), "🧐");
        assert_eq!(first("clown"), "🤡");
        assert!(search("monkey").iter().any(|e| e.glyph == "🙈"));
        assert!(
            search("monkey").len() >= 4,
            "full catalogue, not the common set"
        );
    }

    #[test]
    fn empty_query_is_compact_common_set() {
        let c = search("");
        assert!(c.len() < 200 && c.len() >= 100);
        for g in ["🧐", "🤡", "🙈", "😅", "🦀"] {
            assert!(c.iter().any(|e| e.glyph == g), "{g}");
        }
        assert!(emojis::iter().count() > 1500);
        assert!(search("zzzqqq").is_empty());
    }

    #[test]
    fn catalogue_beyond_common_set() {
        // Not in the curated table, found through the Unicode name / gemoji shortcode.
        assert!(search("hippopotamus").iter().any(|e| e.glyph == "🦛"));
        assert!(search("ice_cream").iter().any(|e| e.glyph == "🍨"));
        assert!(!EMOJIS.iter().any(|e| e.glyph == "🦛"));
    }

    #[test]
    fn multi_token_case_and_sequences() {
        assert_eq!(first("RED heart"), "\u{2764}\u{FE0F}");
        assert!(search("czech")
            .iter()
            .any(|e| e.glyph == "\u{1F1E8}\u{1F1FF}"));
        assert!(search("family")
            .iter()
            .any(|e| e.glyph.contains('\u{200D}')));
    }

    #[test]
    fn skin_tones_only_on_request() {
        let plain = search("thumbs up");
        assert!(plain.iter().all(|e| !e.name.contains("skin tone")));
        let toned = search("thumbs up skin");
        let t = toned
            .iter()
            .find(|e| e.name.contains("medium skin tone"))
            .unwrap();
        assert!(t.glyph.starts_with('\u{1F44D}') && t.glyph.contains('\u{1F3FD}'));
    }

    #[test]
    fn curated_table_is_sane() {
        let mut seen = std::collections::HashSet::new();
        for em in EMOJIS {
            let cat = emojis::get(em.glyph)
                .unwrap_or_else(|| panic!("not in catalogue: {} {:?}", em.name, em.glyph));
            assert!(seen.insert(cat.as_str()), "duplicate {}", em.name);
            assert_eq!(em.keywords, em.keywords.to_lowercase());
            // The picker inserts the catalogue's canonical sequence.
        }
        assert_eq!(common().len(), EMOJIS.len());
    }
}
