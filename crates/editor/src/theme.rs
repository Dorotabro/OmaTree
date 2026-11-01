//! Omarchy palette discovery, parsing and mapping to semantic editor colours.
//!
//! Source of truth: `~/.local/state/omarchy/current/theme/colors.toml`. Omarchy replaces the
//! whole `theme` directory on a switch, so nothing is watched by inode: the *path* is re-read
//! at low frequency and the content compared.

use egui::{Color32, Context, Stroke, Visuals};

/// Semantic roles used by the whole UI. Everything is derived from the Omarchy palette.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Palette {
    pub dark: bool,
    pub canvas: Color32,
    pub text: Color32,
    pub muted: Color32,
    /// H1..H6; H1–H3 are the most differentiated.
    pub headings: [Color32; 6],
    pub quote_border: Color32,
    pub quote_text: Color32,
    /// Thematic break line.
    pub rule: Color32,
    pub code_block_bg: Color32,
    pub link: Color32,
    pub code_text: Color32,
    pub code_bg: Color32,
    pub selection: Color32,
    /// Revealed Markdown delimiters.
    pub reveal: Color32,
    pub image_border: Color32,
    pub image_bg: Color32,
    pub menu_surface: Color32,
    pub menu_highlight: Color32,
    pub separator: Color32,
}

pub fn parse_hex(s: &str) -> Option<Color32> {
    let h = s.trim().strip_prefix('#')?;
    if !h.is_ascii() {
        return None;
    }
    let v = |i: usize, n: usize| u8::from_str_radix(&h[i..i + n], 16).ok();
    match h.len() {
        6 => Some(Color32::from_rgb(v(0, 2)?, v(2, 2)?, v(4, 2)?)),
        3 => Some(Color32::from_rgb(
            v(0, 1)? * 17,
            v(1, 1)? * 17,
            v(2, 1)? * 17,
        )),
        _ => None,
    }
}

fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// Visibly coloured (not a grey/neutral accent such as a foreground-coloured `accent`).
fn chromatic(c: Color32) -> bool {
    let (mx, mn) = (c.r().max(c.g()).max(c.b()), c.r().min(c.g()).min(c.b()));
    mx - mn > 36
}

/// Rotate the hue of `c` by `deg` degrees, keeping lightness and saturation.
fn rotate_hue(c: Color32, deg: f32) -> Color32 {
    let (r, g, b) = (
        f32::from(c.r()) / 255.0,
        f32::from(c.g()) / 255.0,
        f32::from(c.b()) / 255.0,
    );
    let (mx, mn) = (r.max(g).max(b), r.min(g).min(b));
    let l = (mx + mn) / 2.0;
    let d = mx - mn;
    if d < 1e-4 {
        return c;
    }
    let sat = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if mx == r {
        ((g - b) / d).rem_euclid(6.0)
    } else if mx == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    let h = (h * 60.0 + deg).rem_euclid(360.0) / 60.0;
    let k = (1.0 - (2.0 * l - 1.0).abs()) * sat;
    let x = k * (1.0 - (h.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h as u32 {
        0 => (k, x, 0.0),
        1 => (x, k, 0.0),
        2 => (0.0, k, x),
        3 => (0.0, x, k),
        4 => (x, 0.0, k),
        _ => (k, 0.0, x),
    };
    let m = l - k / 2.0;
    let u = |v: f32| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    Color32::from_rgb(u(r1), u(g1), u(b1))
}

fn luminance(c: Color32) -> f32 {
    let f = |x: u8| {
        let s = f32::from(x) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

pub fn contrast(a: Color32, b: Color32) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

/// Pull `c` toward `toward` until it reaches `min` contrast against `bg`.
fn ensure_contrast(c: Color32, bg: Color32, toward: Color32, min: f32) -> Color32 {
    let mut out = c;
    for i in 1..=10 {
        if contrast(out, bg) >= min {
            break;
        }
        out = mix(c, toward, i as f32 / 10.0);
    }
    out
}

impl Palette {
    /// Parse and map an Omarchy `colors.toml`. Requires valid `background` and `foreground`;
    /// every other key is optional and an invalid optional colour is treated as absent.
    pub fn from_toml(src: &str) -> Result<Self, String> {
        let table: toml::Table = src.parse().map_err(|e| format!("invalid TOML: {e}"))?;
        let color = |k: &str| table.get(k).and_then(|v| v.as_str()).and_then(parse_hex);
        let required = |k: &str| color(k).ok_or_else(|| format!("missing or invalid `{k}`"));
        let canvas = required("background")?;
        let text = required("foreground")?;
        let dark = match table.get("mode").and_then(|v| v.as_str()) {
            Some("light") => false,
            Some("dark") => true,
            // Unknown/missing mode: infer from the background.
            _ => luminance(canvas) < 0.4,
        };
        let accent = color("accent").unwrap_or(text);
        let bright = color("bright_foreground").unwrap_or(text);
        // First chromatic colour among the given palette keys.
        let pick = |keys: &[&str]| keys.iter().filter_map(|k| color(k)).find(|c| chromatic(*c));

        let secondary = color("dark_foreground").unwrap_or_else(|| mix(text, canvas, 0.45));
        let secondary = ensure_contrast(secondary, canvas, text, 3.0);
        let link_base = pick(&["accent", "blue", "bright_blue", "cyan"]).unwrap_or(accent);
        let link = ensure_contrast(link_base, canvas, text, 3.0);
        let code_bg = mix(canvas, text, 0.10);
        let mut selection = color("selection").unwrap_or_else(|| mix(canvas, accent, 0.30));
        if contrast(selection, canvas) < 1.25 {
            selection = mix(canvas, accent, 0.35);
        }
        // Text must stay legible on top of the selection and the code background.
        let text = ensure_contrast(text, selection, bright, 4.5);
        let separator = mix(canvas, text, 0.15);

        // Headings: a restrained tint (60 % hue, 40 % foreground) of up to three palette colours,
        // otherwise steps derived from the first heading colour. Always >= 4.5:1 on the canvas.
        let tint = |c: Color32| ensure_contrast(mix(bright, c, 0.65), canvas, text, 4.5);
        // No named colour available: rotate the hue of a chromatic H1, or step brightness if neutral.
        let derive = |base: Color32, deg: f32, fade: f32| {
            if chromatic(base) {
                rotate_hue(base, deg)
            } else {
                mix(base, canvas, fade)
            }
        };
        let h1 = pick(&["accent", "blue", "bright_blue"])
            .map_or_else(|| ensure_contrast(bright, canvas, text, 4.5), tint);
        let h2 = pick(&["magenta", "bright_magenta", "orange"])
            .map_or_else(|| derive(h1, 55.0, 0.25), tint);
        let h3 = pick(&["cyan", "bright_cyan", "green", "bright_green"])
            .map_or_else(|| derive(h1, -55.0, 0.45), tint);
        let h4 = mix(text, h1, 0.35);
        let h5 = mix(text, h2, 0.25);
        let h6 = mix(text, canvas, 0.2);
        let headings = [h1, h2, h3, h4, h5, h6].map(|c| ensure_contrast(c, canvas, text, 4.5));

        Ok(Self {
            dark,
            canvas,
            text,
            muted: secondary,
            headings,
            link,
            code_text: ensure_contrast(text, code_bg, bright, 4.5),
            code_bg,
            selection,
            reveal: secondary,
            image_border: color("muted")
                .map_or(separator, |m| ensure_contrast(m, canvas, text, 1.5)),
            image_bg: code_bg,
            menu_surface: mix(canvas, text, 0.06),
            menu_highlight: selection,
            separator,
            quote_border: ensure_contrast(mix(canvas, link, 0.5), canvas, link, 1.8),
            quote_text: ensure_contrast(mix(text, canvas, 0.15), canvas, text, 4.5),
            rule: ensure_contrast(mix(canvas, text, 0.3), canvas, text, 1.8),
            code_block_bg: mix(canvas, text, 0.06),
        })
    }

    /// Built-in palette for machines without Omarchy.
    pub fn fallback() -> Self {
        Self::from_toml(
            "mode = \"dark\"\nbackground = \"#1e1e1e\"\nforeground = \"#d4d4d4\"\naccent = \"#6cb6ff\"\n\
             selection = \"#2f4a66\"\ndark_foreground = \"#8a8a8a\"\nbright_foreground = \"#f0f0f0\"\n",
        )
        .expect("built-in palette is valid")
    }

    /// Install colours into egui's visuals. Colour-only: no sizes, spacing or fonts change.
    pub fn apply(&self, ctx: &Context) {
        let mut v = if self.dark {
            Visuals::dark()
        } else {
            Visuals::light()
        };
        v.override_text_color = Some(self.text);
        v.weak_text_color = Some(self.muted);
        v.panel_fill = self.canvas;
        v.window_fill = self.menu_surface;
        v.window_stroke = Stroke::new(1.0, self.separator);
        v.extreme_bg_color = self.canvas;
        v.faint_bg_color = self.code_bg;
        v.code_bg_color = self.code_bg;
        v.hyperlink_color = self.link;
        v.selection.bg_fill = self.menu_highlight;
        v.selection.stroke = Stroke::new(1.0, self.text);
        v.text_cursor.stroke.color = self.text;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, self.separator);
        v.widgets.noninteractive.bg_fill = self.canvas;
        v.widgets.hovered.weak_bg_fill = mix(self.menu_surface, self.text, 0.08);
        v.widgets.hovered.bg_fill = v.widgets.hovered.weak_bg_fill;
        ctx.set_visuals(v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub const DARK: &str = "mode = \"dark\"\naccent = \"#7aa2f7\"\nselection = \"#292e42\"\nmuted = \"#414868\"\nbackground = \"#1a1b26\"\nforeground = \"#a9b1d6\"\ndark_foreground = \"#565f89\"\nbright_foreground = \"#c0caf5\"\n";
    /// Named-colour palette shape (like Omarchy's full schema); used only as a test fixture.
    pub const NAMED_DARK: &str = "mode = \"dark\"\naccent = \"#dcd7ba\"\nselection = \"#363646\"\nbackground = \"#1f1f28\"\nforeground = \"#dcd7ba\"\ndark_foreground = \"#727169\"\nbright_foreground = \"#dcd7ba\"\nblue = \"#7e9cd8\"\ncyan = \"#6a9589\"\nmagenta = \"#957fb8\"\ngreen = \"#76946a\"\n";
    pub const NAMED_LIGHT: &str = "mode = \"light\"\naccent = \"#2e5cb8\"\nselection = \"#d0d8ee\"\nbackground = \"#f5f3ee\"\nforeground = \"#2b2b33\"\ndark_foreground = \"#6b6b75\"\nbright_foreground = \"#000000\"\nblue = \"#3b6fd0\"\ncyan = \"#2a8c8c\"\nmagenta = \"#8a4fb0\"\ngreen = \"#3c8a3c\"\n";
    pub const LIGHT: &str = "mode = \"light\"\naccent = \"#2e5cb8\"\nselection = \"#d0d8ee\"\nmuted = \"#b8b8c0\"\nbackground = \"#f5f3ee\"\nforeground = \"#2b2b33\"\ndark_foreground = \"#6b6b75\"\nbright_foreground = \"#000000\"\n";

    #[test]
    fn dark_and_light_map_and_stay_legible() {
        for (src, dark) in [(DARK, true), (LIGHT, false)] {
            let p = Palette::from_toml(src).unwrap();
            assert_eq!(p.dark, dark);
            assert!(contrast(p.text, p.canvas) >= 4.5);
            for h in p.headings {
                assert!(contrast(h, p.canvas) >= 4.5);
            }
            assert!(contrast(p.quote_border, p.canvas) >= 1.8 && contrast(p.rule, p.canvas) >= 1.8);
            assert!(contrast(p.quote_text, p.canvas) >= 4.5);
            assert!(contrast(p.reveal, p.canvas) >= 3.0);
            assert!(contrast(p.link, p.canvas) >= 3.0);
            assert!(contrast(p.code_text, p.code_bg) >= 4.5);
            assert!(contrast(p.text, p.selection) >= 4.5, "text on selection");
            assert!(contrast(p.selection, p.canvas) >= 1.25);
            assert_eq!(
                p.canvas,
                parse_hex(if dark { "#1a1b26" } else { "#f5f3ee" }).unwrap()
            );
        }
        assert_ne!(Palette::from_toml(DARK), Palette::from_toml(LIGHT));
    }

    #[test]
    fn missing_optional_keys_and_bad_optional_colours() {
        let p = Palette::from_toml("background = \"#101010\"\nforeground = \"#eeeeee\"\n").unwrap();
        assert!(p.dark, "mode inferred from background");
        let q = Palette::from_toml("background = \"#101010\"\nforeground = \"#eeeeee\"\naccent = \"nope\"\nselection = 5\n").unwrap();
        assert!(contrast(q.link, q.canvas) >= 3.0);
        let l = Palette::from_toml("background = \"#fafafa\"\nforeground = \"#111\"\n").unwrap();
        assert!(!l.dark);
    }

    #[test]
    fn invalid_palettes_are_errors_not_panics() {
        assert!(Palette::from_toml("").is_err());
        assert!(Palette::from_toml("this is = = not toml").is_err());
        assert!(Palette::from_toml("background = \"#zzzzzz\"\nforeground = \"#fff\"").is_err());
        assert!(Palette::from_toml("background = \"#1a1b26\"").is_err());
        assert!(
            parse_hex("#é1234").is_none()
                && parse_hex("#12345").is_none()
                && parse_hex("123456").is_none()
        );
    }

    #[test]
    fn apply_sets_visuals_only() {
        let ctx = Context::default();
        let p = Palette::from_toml(LIGHT).unwrap();
        p.apply(&ctx);
        let v = ctx.global_style().visuals.clone();
        assert!(!v.dark_mode);
        assert_eq!(v.panel_fill, p.canvas);
        assert_eq!(v.hyperlink_color, p.link);
    }

    fn dist(a: Color32, b: Color32) -> i32 {
        (i32::from(a.r()) - i32::from(b.r())).abs()
            + (i32::from(a.g()) - i32::from(b.g())).abs()
            + (i32::from(a.b()) - i32::from(b.b())).abs()
    }

    #[test]
    fn heading_colours_are_distinct_restrained_and_legible_in_every_shape_of_palette() {
        for src in [
            DARK,
            LIGHT,
            NAMED_DARK,
            NAMED_LIGHT,
            "background = \"#101010\"\nforeground = \"#eeeeee\"\n",
        ] {
            let p = Palette::from_toml(src).unwrap();
            let h = p.headings;
            for i in 0..3 {
                for j in (i + 1)..3 {
                    assert!(
                        dist(h[i], h[j]) >= 30,
                        "H{} vs H{} too similar in {src}",
                        i + 1,
                        j + 1
                    );
                }
            }
            for c in h {
                assert!(contrast(c, p.canvas) >= 4.5);
            }
            assert!(contrast(p.text, p.canvas) >= 4.5);
        }
    }

    #[test]
    fn heading_colours_follow_the_palette_not_fixed_values() {
        let a = Palette::from_toml(NAMED_DARK).unwrap();
        let b = Palette::from_toml(NAMED_LIGHT).unwrap();
        assert_ne!(a.headings, b.headings);
        // Named colours are used when present (Kanagawa-like fixture has a neutral accent).
        assert!(dist(a.headings[0], a.text) >= 25 && dist(a.headings[1], a.text) >= 25);
        assert!(
            chromatic(a.link),
            "neutral accent must not become the link colour"
        );
        // Without named colours H2/H3 still differ from H1 (derived).
        let n = Palette::from_toml(DARK).unwrap();
        assert_ne!(n.headings[0], n.headings[1]);
    }
}
