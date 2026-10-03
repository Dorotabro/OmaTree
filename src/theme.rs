//! Semantic colour palette and where it comes from. Pure Rust.
//!
//! The UI only ever sees a `Palette` of semantic roles. One is chosen from:
//!
//! 1. the active Omarchy theme (Linux only, when a valid one is found),
//! 2. otherwise OmaTree Dark or OmaTree Light, following the system's colour
//!    scheme (OmaTree Dark when that is unknown).
//!
//! Nothing here knows about any particular Omarchy theme by name.

use std::path::{Path, PathBuf};

/// An opaque colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    const fn hex(value: u32) -> Self {
        Rgb((value >> 16) as u8, (value >> 8) as u8, value as u8)
    }

    /// Parses `#RRGGBB` (surrounding whitespace allowed, any case).
    pub fn parse(text: &str) -> Option<Rgb> {
        let digits = text.trim().strip_prefix('#')?;
        if digits.len() != 6 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        u32::from_str_radix(digits, 16).ok().map(Rgb::hex)
    }

    /// `#rrggbb`, which is what QML colour properties accept.
    pub fn to_hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Perceived brightness from 0.0 (black) to 1.0 (white).
    fn luminance(self) -> f64 {
        (0.2126 * f64::from(self.0) + 0.7152 * f64::from(self.1) + 0.0722 * f64::from(self.2))
            / 255.0
    }
}

/// The semantic roles every OmaTree-owned UI element draws from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    pub background: Rgb,
    pub surface: Rgb,
    pub surface_raised: Rgb,
    pub foreground: Rgb,
    pub muted_foreground: Rgb,
    pub accent: Rgb,
    pub selection: Rgb,
    pub selection_foreground: Rgb,
    pub border: Rgb,
    pub danger: Rgb,
    pub danger_foreground: Rgb,
}

impl Palette {
    /// OmaTree Dark. The only place these values are written down, together
    /// with `LIGHT`.
    pub const DARK: Palette = Palette {
        background: Rgb::hex(0x252A33),
        surface: Rgb::hex(0x2D3440),
        surface_raised: Rgb::hex(0x37404D),
        foreground: Rgb::hex(0xE6EAF0),
        muted_foreground: Rgb::hex(0xAAB4C0),
        accent: Rgb::hex(0x82B8C8),
        selection: Rgb::hex(0x496E88),
        selection_foreground: Rgb::hex(0xF4F7FA),
        border: Rgb::hex(0x46515F),
        danger: Rgb::hex(0xBE6670),
        danger_foreground: Rgb::hex(0xFFFFFF),
    };

    /// OmaTree Light.
    pub const LIGHT: Palette = Palette {
        background: Rgb::hex(0xF2F4F7),
        surface: Rgb::hex(0xE7EBF0),
        surface_raised: Rgb::hex(0xDCE2E8),
        foreground: Rgb::hex(0x29313B),
        muted_foreground: Rgb::hex(0x65717E),
        accent: Rgb::hex(0x557F9D),
        selection: Rgb::hex(0xB8D3E0),
        selection_foreground: Rgb::hex(0x20303D),
        border: Rgb::hex(0xC8D0D9),
        danger: Rgb::hex(0xB4525C),
        danger_foreground: Rgb::hex(0xFFFFFF),
    };

    /// Every role, by name, for iteration in tests.
    #[cfg(test)]
    pub fn roles(&self) -> [(&'static str, Rgb); 11] {
        [
            ("background", self.background),
            ("surface", self.surface),
            ("surfaceRaised", self.surface_raised),
            ("foreground", self.foreground),
            ("mutedForeground", self.muted_foreground),
            ("accent", self.accent),
            ("selection", self.selection),
            ("selectionForeground", self.selection_foreground),
            ("border", self.border),
            ("danger", self.danger),
            ("dangerForeground", self.danger_foreground),
        ]
    }
}

/// The operating system's colour scheme as Qt reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Light,
    Dark,
    Unknown,
}

impl Scheme {
    /// From `Qt::ColorScheme` (0 unknown, 1 light, 2 dark).
    pub fn from_qt(value: i32) -> Scheme {
        match value {
            1 => Scheme::Light,
            2 => Scheme::Dark,
            _ => Scheme::Unknown,
        }
    }
}

/// The palette to show: a valid Omarchy palette wins; otherwise the built-in
/// one for the system scheme, with OmaTree Dark when it is unknown.
pub fn select(omarchy: Option<Palette>, scheme: Scheme) -> Palette {
    match (omarchy, scheme) {
        (Some(palette), _) => palette,
        (None, Scheme::Light) => Palette::LIGHT,
        (None, _) => Palette::DARK,
    }
}

// ---- Omarchy ---------------------------------------------------------------

/// Why an Omarchy palette was not used.
#[derive(Debug, PartialEq, Eq)]
pub enum OmarchyError {
    /// Not valid TOML.
    Malformed(String),
    /// A required colour (`background` or `foreground`) is absent.
    Missing(&'static str),
    /// A required colour is not a `#RRGGBB` string.
    Invalid(&'static str),
}

impl std::fmt::Display for OmarchyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OmarchyError::Malformed(why) => write!(f, "not valid TOML: {why}"),
            OmarchyError::Missing(key) => write!(f, "required colour `{key}` is missing"),
            OmarchyError::Invalid(key) => write!(f, "colour `{key}` is not a #RRGGBB value"),
        }
    }
}

/// Where Omarchy keeps the active theme's colours: under `$XDG_STATE_HOME`
/// when that is set to something, otherwise under `$HOME/.local/state`.
/// The arguments are the environment values, so this is easy to test.
pub fn omarchy_colors_path(xdg_state_home: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let state = match xdg_state_home.filter(|s| !s.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(home.filter(|h| !h.is_empty())?).join(".local/state"),
    };
    Some(state.join("omarchy/current/theme/colors.toml"))
}

/// The active Omarchy colours file for this process, on Linux only. Other
/// platforms have no Omarchy, so they always use the built-in palettes.
pub fn default_omarchy_colors_path() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let xdg = std::env::var("XDG_STATE_HOME").ok();
    let home = std::env::var("HOME").ok();
    omarchy_colors_path(xdg.as_deref(), home.as_deref())
}

/// Maps an Omarchy `colors.toml` onto OmaTree's roles.
///
/// | OmaTree role          | Omarchy key, then fallbacks                    |
/// |-----------------------|------------------------------------------------|
/// | background            | `background` (required)                        |
/// | foreground            | `foreground` (required)                        |
/// | surface               | `dark_background`, `darker_background`         |
/// | surfaceRaised         | `lighter_background`                           |
/// | mutedForeground       | `dark_foreground`, `muted`                     |
/// | accent                | `accent`, `blue`                               |
/// | selection             | `selection`, `selection_background`            |
/// | selectionForeground   | `bright_foreground`, `foreground`              |
/// | border                | `muted`                                        |
/// | danger                | `red`, `bright_red`                            |
/// | dangerForeground      | `bright_foreground`, `foreground`              |
///
/// A role none of these supply (or whose values are not valid colours) falls
/// back to the same role of the built-in palette matching the theme's `mode`
/// (or, without a usable `mode`, the brightness of its background). Unknown
/// keys are ignored, and the theme's own name plays no part.
pub fn parse_omarchy(text: &str) -> Result<Palette, OmarchyError> {
    let table: toml::Table = text
        .parse()
        .map_err(|e: toml::de::Error| OmarchyError::Malformed(e.message().to_string()))?;

    let lookup = |key: &str| table.get(key).and_then(|v| v.as_str()).and_then(Rgb::parse);
    let required = |key: &'static str| match table.get(key) {
        None => Err(OmarchyError::Missing(key)),
        Some(value) => value
            .as_str()
            .and_then(Rgb::parse)
            .ok_or(OmarchyError::Invalid(key)),
    };
    let background = required("background")?;
    let foreground = required("foreground")?;

    let dark = match table.get("mode").and_then(|v| v.as_str()) {
        Some(mode) if mode.eq_ignore_ascii_case("light") => false,
        Some(mode) if mode.eq_ignore_ascii_case("dark") => true,
        _ => background.luminance() < 0.5,
    };
    let base = if dark { Palette::DARK } else { Palette::LIGHT };

    // The first valid colour among `keys`, else the built-in role.
    let pick = |keys: &[&str], builtin: Rgb| keys.iter().find_map(|k| lookup(k)).unwrap_or(builtin);
    let bright_or_foreground = pick(&["bright_foreground"], foreground);

    Ok(Palette {
        background,
        foreground,
        surface: pick(&["dark_background", "darker_background"], base.surface),
        surface_raised: pick(&["lighter_background"], base.surface_raised),
        muted_foreground: pick(&["dark_foreground", "muted"], base.muted_foreground),
        accent: pick(&["accent", "blue"], base.accent),
        selection: pick(&["selection", "selection_background"], base.selection),
        selection_foreground: bright_or_foreground,
        border: pick(&["muted"], base.border),
        danger: pick(&["red", "bright_red"], base.danger),
        danger_foreground: bright_or_foreground,
    })
}

/// Follows the active Omarchy theme while the application runs.
///
/// Omarchy replaces the whole active theme directory when it changes theme,
/// so nothing here holds on to a file or inode: every `poll` resolves the
/// path afresh. A change only counts once the same new content has been seen
/// on two polls in a row, which absorbs the several writes of one switch and
/// the short moment the file may be missing or half written. During such a
/// moment the current palette simply stays.
pub struct OmarchyWatcher {
    path: Option<PathBuf>,
    /// The content that was last turned into a palette (or rejected).
    applied: Option<String>,
    /// New content seen once, waiting for confirmation.
    pending: Option<String>,
    /// So a broken file is mentioned once, not on every poll.
    warned: bool,
}

impl OmarchyWatcher {
    pub fn new(path: Option<PathBuf>) -> Self {
        OmarchyWatcher {
            path,
            applied: None,
            pending: None,
            warned: false,
        }
    }

    /// Whether there is anything to watch on this platform.
    pub fn is_active(&self) -> bool {
        self.path.is_some()
    }

    /// The palette of the file as it is right now, if it is valid. For
    /// startup, when there is no reason to wait.
    pub fn load_now(&mut self) -> Option<Palette> {
        let text = std::fs::read_to_string(self.path.as_ref()?).ok()?;
        self.pending = None;
        self.applied = Some(text.clone());
        self.parse_reporting(&text)
    }

    /// Looks at the file once. Returns a palette when a new, confirmed, valid
    /// one has appeared; `None` for everything else (no change, still
    /// settling, missing, unreadable, malformed).
    pub fn poll(&mut self) -> Option<Palette> {
        let path = self.path.as_ref()?;
        let Ok(text) = std::fs::read_to_string(path) else {
            // Mid-replacement, or no Omarchy: keep what is shown.
            self.pending = None;
            return None;
        };
        if self.applied.as_deref() == Some(text.as_str()) {
            self.pending = None;
            return None;
        }
        if self.pending.as_deref() != Some(text.as_str()) {
            self.pending = Some(text);
            return None;
        }
        self.pending = None;
        self.applied = Some(text.clone());
        self.parse_reporting(&text)
    }

    fn parse_reporting(&mut self, text: &str) -> Option<Palette> {
        match parse_omarchy(text) {
            Ok(palette) => {
                self.warned = false;
                Some(palette)
            }
            Err(error) => {
                if !self.warned {
                    eprintln!("omatree: ignoring the Omarchy theme: {error}");
                    self.warned = true;
                }
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    const TOKYO: &str = r##"
        mode = "dark"
        accent = "#7aa2f7"
        selection = "#292e42"
        muted = "#414868"
        background = "#1a1b26"
        dark_background = "#13141c"
        darker_background = "#0e0e14"
        lighter_background = "#24283b"
        foreground = "#a9b1d6"
        dark_foreground = "#565f89"
        light_foreground = "#b4bee6"
        bright_foreground = "#c0caf5"
        red = "#f7768e"
        bright_red = "#ff7a93"
        blue = "#7aa2f7"
    "##;

    const LATTE: &str = r##"
        mode = "light"
        accent = "#1e66f5"
        selection = "#ccd0da"
        muted = "#9ca0b0"
        background = "#eff1f5"
        dark_background = "#e6e9ef"
        lighter_background = "#dce0e8"
        foreground = "#4c4f69"
        dark_foreground = "#6c6f85"
        bright_foreground = "#232634"
        red = "#d20f39"
    "##;

    fn rgb(hex: &str) -> Rgb {
        Rgb::parse(hex).unwrap()
    }

    // ---- built-in palettes ----

    #[test]
    fn the_built_in_palettes_define_every_role() {
        for palette in [Palette::DARK, Palette::LIGHT] {
            let names: Vec<&str> = palette.roles().iter().map(|(n, _)| *n).collect();
            assert_eq!(
                names,
                [
                    "background",
                    "surface",
                    "surfaceRaised",
                    "foreground",
                    "mutedForeground",
                    "accent",
                    "selection",
                    "selectionForeground",
                    "border",
                    "danger",
                    "dangerForeground"
                ]
            );
        }
    }

    #[test]
    fn the_built_in_values_are_the_specified_colours_and_parse_back() {
        assert_eq!(Palette::DARK.background.to_hex(), "#252a33");
        assert_eq!(Palette::DARK.selection.to_hex(), "#496e88");
        assert_eq!(Palette::DARK.danger.to_hex(), "#be6670");
        assert_eq!(Palette::LIGHT.background.to_hex(), "#f2f4f7");
        assert_eq!(Palette::LIGHT.accent.to_hex(), "#557f9d");
        assert_eq!(Palette::LIGHT.danger_foreground.to_hex(), "#ffffff");
        for palette in [Palette::DARK, Palette::LIGHT] {
            for (name, colour) in palette.roles() {
                assert_eq!(Rgb::parse(&colour.to_hex()), Some(colour), "{name}");
            }
        }
    }

    #[test]
    fn dark_and_light_differ_and_danger_is_independent_of_accent() {
        assert_ne!(Palette::DARK.background, Palette::LIGHT.background);
        assert!(Palette::DARK.background.luminance() < Palette::LIGHT.background.luminance());
        for palette in [Palette::DARK, Palette::LIGHT] {
            assert_ne!(palette.danger, palette.accent);
            assert_ne!(palette.danger, palette.selection);
        }
    }

    #[test]
    fn colour_parsing_is_strict_about_the_format() {
        assert_eq!(Rgb::parse("#7aa2f7"), Some(Rgb(0x7a, 0xa2, 0xf7)));
        assert_eq!(Rgb::parse("  #7AA2F7 "), Some(Rgb(0x7a, 0xa2, 0xf7)));
        for bad in [
            "", "#", "7aa2f7", "#7aa2f", "#7aa2f7f", "#gggggg", "red", "#7a a2f7",
        ] {
            assert_eq!(Rgb::parse(bad), None, "{bad:?}");
        }
    }

    // ---- Omarchy mapping ----

    #[test]
    fn a_valid_dark_theme_maps_onto_the_semantic_roles() {
        let p = parse_omarchy(TOKYO).unwrap();
        assert_eq!(p.background, rgb("#1a1b26"));
        assert_eq!(p.surface, rgb("#13141c"), "dark_background");
        assert_eq!(p.surface_raised, rgb("#24283b"), "lighter_background");
        assert_eq!(p.foreground, rgb("#a9b1d6"));
        assert_eq!(p.muted_foreground, rgb("#565f89"), "dark_foreground");
        assert_eq!(p.accent, rgb("#7aa2f7"));
        assert_eq!(p.selection, rgb("#292e42"));
        assert_eq!(p.selection_foreground, rgb("#c0caf5"), "bright_foreground");
        assert_eq!(p.border, rgb("#414868"), "muted");
        assert_eq!(p.danger, rgb("#f7768e"), "red");
        assert_eq!(p.danger_foreground, rgb("#c0caf5"));
    }

    #[test]
    fn a_valid_light_theme_maps_the_same_way() {
        let p = parse_omarchy(LATTE).unwrap();
        assert_eq!(p.background, rgb("#eff1f5"));
        assert_eq!(p.surface, rgb("#e6e9ef"));
        assert_eq!(p.surface_raised, rgb("#dce0e8"));
        assert_eq!(p.foreground, rgb("#4c4f69"));
        assert_eq!(p.accent, rgb("#1e66f5"));
        assert_eq!(p.danger, rgb("#d20f39"));
        assert_eq!(p.selection_foreground, rgb("#232634"));
    }

    #[test]
    fn missing_optional_colours_use_the_documented_fallbacks() {
        // Only the two required colours, in dark mode: every other role comes
        // from OmaTree Dark, but the theme's own colours are kept.
        let p =
            parse_omarchy("mode = \"dark\"\nbackground = \"#101010\"\nforeground = \"#eeeeee\"")
                .unwrap();
        assert_eq!(p.background, rgb("#101010"));
        assert_eq!(p.foreground, rgb("#eeeeee"));
        assert_eq!(p.surface, Palette::DARK.surface);
        assert_eq!(p.accent, Palette::DARK.accent);
        assert_eq!(p.danger, Palette::DARK.danger);
        assert_eq!(
            p.selection_foreground,
            rgb("#eeeeee"),
            "falls back to foreground"
        );
        assert_eq!(p.danger_foreground, rgb("#eeeeee"));

        // Light mode falls back to the light built-ins.
        let p =
            parse_omarchy("mode = \"light\"\nbackground = \"#fafafa\"\nforeground = \"#111111\"")
                .unwrap();
        assert_eq!(p.border, Palette::LIGHT.border);

        // One missing key doesn't discard the rest of an otherwise good theme.
        let without_selection = TOKYO.replace("selection = \"#292e42\"", "");
        let p = parse_omarchy(&without_selection).unwrap();
        assert_eq!(p.selection, Palette::DARK.selection);
        assert_eq!(p.accent, rgb("#7aa2f7"));
    }

    #[test]
    fn alternative_keys_are_used_when_the_preferred_one_is_missing() {
        let text = "background = \"#101010\"\nforeground = \"#eeeeee\"\n\
                    blue = \"#3366ff\"\nbright_red = \"#ff0000\"\nmuted = \"#777777\"\n\
                    darker_background = \"#050505\"\nselection_background = \"#224466\"";
        let p = parse_omarchy(text).unwrap();
        assert_eq!(p.accent, rgb("#3366ff"));
        assert_eq!(p.danger, rgb("#ff0000"));
        assert_eq!(
            p.muted_foreground,
            rgb("#777777"),
            "muted when dark_foreground is absent"
        );
        assert_eq!(p.surface, rgb("#050505"));
        assert_eq!(p.selection, rgb("#224466"));
    }

    #[test]
    fn mode_is_inferred_from_the_background_when_absent() {
        let dark = parse_omarchy("background = \"#111111\"\nforeground = \"#ffffff\"").unwrap();
        assert_eq!(dark.surface, Palette::DARK.surface);
        let light = parse_omarchy("background = \"#f0f0f0\"\nforeground = \"#000000\"").unwrap();
        assert_eq!(light.surface, Palette::LIGHT.surface);
    }

    #[test]
    fn a_theme_without_background_or_foreground_is_rejected() {
        assert_eq!(
            parse_omarchy("foreground = \"#ffffff\""),
            Err(OmarchyError::Missing("background"))
        );
        assert_eq!(
            parse_omarchy("background = \"#000000\""),
            Err(OmarchyError::Missing("foreground"))
        );
        assert!(parse_omarchy("").is_err());
    }

    #[test]
    fn malformed_colours_and_toml_fail_or_fall_back_safely() {
        // Required colour invalid: rejected, not guessed.
        assert_eq!(
            parse_omarchy("background = \"nope\"\nforeground = \"#ffffff\""),
            Err(OmarchyError::Invalid("background"))
        );
        assert_eq!(
            parse_omarchy("background = \"#000000\"\nforeground = 5"),
            Err(OmarchyError::Invalid("foreground"))
        );
        // Optional colour invalid: that role falls back, the theme survives.
        let p =
            parse_omarchy(&TOKYO.replace("#7aa2f7\"\nselection", "banana\"\nselection")).unwrap();
        assert_eq!(p.background, rgb("#1a1b26"));
        // Not TOML at all.
        assert!(matches!(
            parse_omarchy("this is = = not toml ["),
            Err(OmarchyError::Malformed(_))
        ));
    }

    #[test]
    fn unknown_keys_and_theme_names_are_ignored() {
        let with_extras = format!(
            "name = \"My Custom Theme\"\nauthor = \"somebody\"\n{TOKYO}\ncolor0 = \"#000000\"\n[extra]\nthing = 1"
        );
        assert_eq!(parse_omarchy(&with_extras), parse_omarchy(TOKYO));
        // The same colours under any name give the same palette: there is no
        // theme-name behaviour to find.
        for name in ["tokyo-night", "catppuccin", "matte-black", "x", ""] {
            let named = format!("theme = \"{name}\"\n{TOKYO}");
            assert_eq!(parse_omarchy(&named), parse_omarchy(TOKYO), "{name:?}");
        }
        let source = include_str!("theme.rs");
        let production = &source[..source.find("#[cfg(test)]").unwrap()];
        for name in [
            "tokyo-night",
            "catppuccin",
            "matte-black",
            "\"nord\"",
            "gruvbox",
            "rose-pine",
            "everforest",
        ] {
            assert!(
                !production.to_lowercase().contains(name),
                "{name} appears in the code"
            );
        }
    }

    // ---- provider selection ----

    #[test]
    fn a_valid_omarchy_palette_beats_the_system_scheme() {
        let omarchy = parse_omarchy(TOKYO).unwrap();
        for scheme in [Scheme::Dark, Scheme::Light, Scheme::Unknown] {
            assert_eq!(select(Some(omarchy), scheme), omarchy);
        }
    }

    #[test]
    fn without_omarchy_the_system_scheme_picks_the_built_in_palette() {
        assert_eq!(select(None, Scheme::Dark), Palette::DARK);
        assert_eq!(select(None, Scheme::Light), Palette::LIGHT);
        assert_eq!(
            select(None, Scheme::Unknown),
            Palette::DARK,
            "deterministic fallback"
        );
        assert_eq!(Scheme::from_qt(1), Scheme::Light);
        assert_eq!(Scheme::from_qt(2), Scheme::Dark);
        assert_eq!(Scheme::from_qt(0), Scheme::Unknown);
        assert_eq!(Scheme::from_qt(99), Scheme::Unknown);
    }

    #[test]
    fn missing_or_invalid_omarchy_data_selects_the_built_in_palette() {
        let dir = TempDir::new();
        // Nothing there at all.
        let mut missing = OmarchyWatcher::new(Some(dir.join("absent/colors.toml")));
        assert_eq!(select(missing.load_now(), Scheme::Light), Palette::LIGHT);
        // Present but invalid.
        let bad = dir.write("bad.toml", "background = \"#000000\"");
        let mut invalid = OmarchyWatcher::new(Some(bad));
        assert_eq!(select(invalid.load_now(), Scheme::Dark), Palette::DARK);
        // No Omarchy on this platform.
        let mut none = OmarchyWatcher::new(None);
        assert!(!none.is_active());
        assert_eq!(none.load_now(), None);
        assert_eq!(none.poll(), None);
    }

    // ---- paths ----

    #[test]
    fn the_omarchy_path_prefers_xdg_state_home_then_home() {
        let suffix = "omarchy/current/theme/colors.toml";
        assert_eq!(
            omarchy_colors_path(Some("/state"), Some("/home/u")),
            Some(PathBuf::from("/state").join(suffix))
        );
        assert_eq!(
            omarchy_colors_path(None, Some("/home/u")),
            Some(PathBuf::from("/home/u/.local/state").join(suffix))
        );
        assert_eq!(
            omarchy_colors_path(Some(""), Some("/home/u")),
            Some(PathBuf::from("/home/u/.local/state").join(suffix)),
            "an empty XDG_STATE_HOME counts as unset"
        );
        assert_eq!(omarchy_colors_path(None, None), None);
        assert_eq!(omarchy_colors_path(Some(""), Some("")), None);
    }

    // ---- live reload ----

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir =
                std::env::temp_dir().join(format!("omatree-theme-{}-{n}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }

        fn join(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }

        fn write(&self, name: &str, text: &str) -> PathBuf {
            let path = self.join(name);
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, text).unwrap();
            path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Like Omarchy: `current/theme` is a whole directory, swapped in place.
    fn install_theme(root: &TempDir, text: &str) -> PathBuf {
        let staging = root.join("current/theme.new");
        let _ = std::fs::remove_dir_all(&staging);
        std::fs::create_dir_all(&staging).unwrap();
        std::fs::write(staging.join("colors.toml"), text).unwrap();
        let live = root.join("current/theme");
        let _ = std::fs::remove_dir_all(&live);
        std::fs::rename(&staging, &live).unwrap();
        live.join("colors.toml")
    }

    #[test]
    fn changing_the_colours_reloads_the_palette_after_confirmation() {
        let dir = TempDir::new();
        let path = dir.write("colors.toml", TOKYO);
        let mut watcher = OmarchyWatcher::new(Some(path.clone()));
        assert_eq!(watcher.load_now(), Some(parse_omarchy(TOKYO).unwrap()));
        assert_eq!(watcher.poll(), None, "unchanged");

        std::fs::write(&path, LATTE).unwrap();
        assert_eq!(watcher.poll(), None, "first sighting only waits");
        assert_eq!(
            watcher.poll(),
            Some(parse_omarchy(LATTE).unwrap()),
            "confirmed"
        );
        assert_eq!(watcher.poll(), None, "and then it is quiet again");
    }

    #[test]
    fn replacing_the_whole_theme_directory_is_noticed() {
        let dir = TempDir::new();
        let path = install_theme(&dir, TOKYO);
        let mut watcher = OmarchyWatcher::new(Some(path.clone()));
        assert!(watcher.load_now().is_some());

        // The directory (and the file inside it) is a different one now.
        install_theme(&dir, LATTE);
        assert_eq!(watcher.poll(), None);
        assert_eq!(watcher.poll(), Some(parse_omarchy(LATTE).unwrap()));
        install_theme(&dir, TOKYO);
        assert_eq!(watcher.poll(), None);
        assert_eq!(watcher.poll(), Some(parse_omarchy(TOKYO).unwrap()));
    }

    #[test]
    fn a_burst_of_changes_produces_one_effective_update() {
        let dir = TempDir::new();
        let path = dir.write("colors.toml", TOKYO);
        let mut watcher = OmarchyWatcher::new(Some(path.clone()));
        watcher.load_now();

        let mut updates = Vec::new();
        // Several different writes between polls, as one theme switch can do.
        for (i, text) in [LATTE, "background = \"#000000\"", TOKYO, LATTE, LATTE]
            .iter()
            .enumerate()
        {
            std::fs::write(&path, text).unwrap();
            if let Some(p) = watcher.poll() {
                updates.push((i, p));
            }
        }
        assert_eq!(updates.len(), 1, "{updates:?}");
        assert_eq!(updates[0].1, parse_omarchy(LATTE).unwrap());
        for _ in 0..3 {
            assert_eq!(watcher.poll(), None);
        }
    }

    #[test]
    fn a_theme_that_is_missing_for_a_moment_is_tolerated() {
        let dir = TempDir::new();
        let path = install_theme(&dir, TOKYO);
        let mut watcher = OmarchyWatcher::new(Some(path.clone()));
        let before = watcher.load_now().unwrap();

        // Mid-swap: the old directory is gone, the new one not there yet.
        std::fs::remove_dir_all(dir.join("current/theme")).unwrap();
        for _ in 0..4 {
            assert_eq!(watcher.poll(), None, "no crash, no change, no error");
        }
        // And a half-written file does not count either.
        std::fs::create_dir_all(dir.join("current/theme")).unwrap();
        std::fs::write(&path, "backgr").unwrap();
        assert_eq!(watcher.poll(), None);
        std::fs::write(&path, "backgr").unwrap();
        assert_eq!(
            watcher.poll(),
            None,
            "malformed content is rejected quietly"
        );

        // The theme comes back, with new colours: now it applies.
        install_theme(&dir, LATTE);
        assert_eq!(watcher.poll(), None);
        let after = watcher.poll().expect("applied once readable");
        assert_ne!(after, before);
        assert_eq!(after, parse_omarchy(LATTE).unwrap());
    }

    #[test]
    fn the_same_theme_returning_after_a_gap_changes_nothing() {
        let dir = TempDir::new();
        let path = install_theme(&dir, TOKYO);
        let mut watcher = OmarchyWatcher::new(Some(path.clone()));
        watcher.load_now();
        std::fs::remove_dir_all(dir.join("current/theme")).unwrap();
        assert_eq!(watcher.poll(), None);
        install_theme(&dir, TOKYO);
        assert_eq!(watcher.poll(), None);
        assert_eq!(watcher.poll(), None, "identical content is not an update");
    }

    #[test]
    fn omarchy_appearing_later_is_picked_up() {
        let dir = TempDir::new();
        let path = dir.join("current/theme/colors.toml");
        let mut watcher = OmarchyWatcher::new(Some(path));
        assert_eq!(watcher.load_now(), None);
        install_theme(&dir, LATTE);
        assert_eq!(watcher.poll(), None);
        assert_eq!(watcher.poll(), Some(parse_omarchy(LATTE).unwrap()));
    }
}
