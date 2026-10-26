//! The unified theme pipeline (ticket E6a): core palette -> egui visuals + editor palette, live
//! switching, fallbacks and safety. Parsing, path discovery and watcher confirmation are tested
//! once, in `omatree_core::theme`; these tests use the real core watcher on fixture files and
//! check what the host does with its results. Everything is headless (frames of a real `Host`).

use super::*;
use crate::theme::{colour, editor_palette, visuals};
use omatree_core::theme::{Palette as Core, Rgb};
use omatree_editor::theme::contrast;

const TOKYO: &str =
    "mode = \"dark\"\naccent = \"#7aa2f7\"\nselection = \"#292e42\"\nmuted = \"#414868\"\n\
background = \"#1a1b26\"\ndark_background = \"#13141c\"\nlighter_background = \"#24283b\"\n\
foreground = \"#a9b1d6\"\ndark_foreground = \"#565f89\"\nbright_foreground = \"#c0caf5\"\n\
red = \"#f7768e\"\ngreen = \"#9ece6a\"\nmagenta = \"#bb9af7\"\nyellow = \"#e0af68\"\n";
const LATTE: &str = "mode = \"light\"\naccent = \"#1e66f5\"\nselection = \"#ccd0da\"\nmuted = \"#9ca0b0\"\n\
background = \"#eff1f5\"\ndark_background = \"#e6e9ef\"\nlighter_background = \"#dce0e8\"\n\
foreground = \"#4c4f69\"\ndark_foreground = \"#6c6f85\"\nbright_foreground = \"#232634\"\nred = \"#d20f39\"\n";
/// Light by its own `mode`, with a background the brightness heuristic would call dark.
const EXPLICIT_LIGHT: &str =
    "mode = \"light\"\nbackground = \"#3a3a3a\"\nforeground = \"#f0f0f0\"\n";

const RICH: &str = "# Heading one\n\n## Heading two\n\nA [link](https://example.org) and `code` and **bold** 🦀\n\n\
```rust\nfn main() {}\n```\n\n| Název | Počet |\n|---|---|\n| jablka 🍎 | 3 |\n\n- [x] done\n- [ ] todo\n\n\
> quote\n\n![alt text](attachment:missing-image)\n";

fn colours_path(d: &Path) -> PathBuf {
    d.join("omarchy/current/theme/colors.toml")
}

fn install(d: &Path, text: &str) -> PathBuf {
    let p = colours_path(d);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(&p, text).unwrap();
    p
}

/// What Omarchy does on a switch: a whole new theme directory replaces the old one.
fn replace_theme_dir(d: &Path, text: &str) {
    let theme = d.join("omarchy/current/theme");
    let next = d.join("omarchy/current/theme.next");
    let _ = fs::remove_dir_all(&next);
    fs::create_dir_all(&next).unwrap();
    fs::write(next.join("colors.toml"), text).unwrap();
    let _ = fs::remove_dir_all(&theme);
    fs::rename(&next, &theme).unwrap();
}

fn rich_notebook(name: &str) -> (PathBuf, PathBuf) {
    let d = dir(name);
    let p = d.join("n.omatree");
    let mut doc = Document::open(&p).unwrap();
    let a = doc.create_root("Rich").unwrap();
    let b = doc.create_child(a, "Child").unwrap();
    let c = doc.create_root("Other").unwrap();
    doc.set_body(a, RICH).unwrap();
    doc.set_body(b, "child body\n").unwrap();
    doc.set_body(c, "other body\n").unwrap();
    doc.save().unwrap();
    (d, p)
}

impl H {
    /// Follow a fixture colours file and let the first frames settle.
    fn follow(&mut self, path: &Path) {
        self.host.follow_omarchy(Some(path.to_owned()));
        self.settle();
    }

    /// Time passes with the app idle, long enough for `n` watcher polls.
    fn polls(&mut self, n: usize) {
        for _ in 0..n {
            self.advance(0.6);
        }
    }

    fn core(&self) -> Core {
        *self.host.theme_palette()
    }

    fn egui_visuals(&self) -> egui::Visuals {
        self.ctx.global_style().visuals.clone()
    }
}

fn core_of(text: &str) -> Core {
    omatree_core::theme::parse_omarchy(text).unwrap()
}

// --- Mapping -----------------------------------------------------------------------------------

#[test]
fn every_core_role_reaches_the_egui_visuals() {
    for p in [Core::DARK, Core::LIGHT, core_of(TOKYO), core_of(LATTE)] {
        let ed = editor_palette(&p);
        let v = visuals(&p, &ed);
        assert_eq!(v.dark_mode, p.dark);
        assert_eq!(v.panel_fill, colour(p.background), "background");
        assert_eq!(v.extreme_bg_color, colour(p.surface), "surface");
        assert_eq!(v.window_fill, colour(p.surface_raised), "surface_raised");
        assert_eq!(v.faint_bg_color, colour(p.surface_raised));
        assert_eq!(v.weak_text_color, Some(colour(p.muted_foreground)));
        assert_eq!(
            v.widgets.noninteractive.fg_stroke.color,
            colour(p.foreground)
        );
        assert_eq!(v.widgets.inactive.fg_stroke.color, colour(p.foreground));
        assert_eq!(v.hyperlink_color, colour(p.accent), "accent");
        assert_eq!(v.window_stroke.color, colour(p.border), "border");
        assert_eq!(v.widgets.noninteractive.bg_stroke.color, colour(p.border));
        assert_eq!(v.widgets.active.bg_fill, colour(p.selection), "selection");
        assert_eq!(
            v.widgets.active.fg_stroke.color,
            colour(p.selection_foreground)
        );
        assert_eq!(v.selection.stroke.color, colour(p.selection_foreground));
        assert_eq!(v.error_fg_color, colour(p.danger), "danger");
        assert_eq!(v.warn_fg_color, colour(p.warning), "warning");
        assert_eq!(v.text_cursor.stroke.color, colour(p.foreground));
        assert_eq!(
            v.override_text_color, None,
            "buttons may colour their own text"
        );
        // `accent_secondary` and `positive` drive the editor's H2/H3, `danger_foreground` the
        // delete button; they are checked in their own tests.
        let _ = (p.accent_secondary, p.positive, p.danger_foreground);
    }
}

#[test]
fn the_host_installs_the_palette_into_egui_and_pins_egui_to_it() {
    let (_d, p) = rich_notebook("theme-install");
    let mut h = H::open(&p);
    h.settle();
    let v = h.egui_visuals();
    assert_eq!(
        v.panel_fill,
        colour(Core::DARK.background),
        "built-in dark when unknown"
    );
    assert_eq!(h.ctx.theme(), egui::Theme::Dark);
    // A light Omarchy theme while the system says dark: egui follows the palette, not the system.
    let d2 = dir("theme-install-light");
    let colours = install(&d2, LATTE);
    h.system = Some(egui::Theme::Dark);
    h.follow(&colours);
    assert_eq!(h.ctx.theme(), egui::Theme::Light);
    assert_eq!(
        h.egui_visuals().panel_fill,
        colour(core_of(LATTE).background)
    );
    assert!(!h.egui_visuals().dark_mode);
}

#[test]
fn the_same_palette_reaches_the_editor_and_keeps_its_contrast_rules() {
    for (name, p) in [
        ("dark", Core::DARK),
        ("light", Core::LIGHT),
        ("tokyo", core_of(TOKYO)),
        ("latte", core_of(LATTE)),
    ] {
        let e = editor_palette(&p);
        assert_eq!(e.dark, p.dark, "{name}");
        assert_eq!(
            e.canvas,
            colour(p.background),
            "{name}: canvas is the background"
        );
        // Chrome text and editor text are the same colour whenever the editor has no reason to
        // adjust it for legibility on the selection.
        if contrast(colour(p.foreground), colour(p.selection)) >= 4.5 {
            assert_eq!(e.text, colour(p.foreground), "{name}: text");
        }
        assert_eq!(e.muted, e.reveal);
        // The editor's own safeguards still hold on whatever the host gave it.
        assert!(contrast(e.text, e.canvas) >= 4.5, "{name}: text");
        for hd in e.headings {
            assert!(contrast(hd, e.canvas) >= 4.5, "{name}: heading");
        }
        assert!(contrast(e.link, e.canvas) >= 3.0, "{name}: link");
        assert!(contrast(e.code_text, e.code_bg) >= 4.5, "{name}: code");
        assert!(contrast(e.quote_text, e.canvas) >= 4.5, "{name}: quote");
        assert!(
            contrast(e.quote_border, e.canvas) >= 1.8,
            "{name}: quote rule"
        );
        assert!(
            contrast(e.reveal, e.canvas) >= 3.0,
            "{name}: revealed delimiters"
        );
        assert!(
            contrast(e.text, e.selection) >= 4.5,
            "{name}: text on selection"
        );
        assert!(
            contrast(e.selection, e.canvas) >= 1.25,
            "{name}: selection visible"
        );
        // And the selection role the chrome shows is the editor's selection where unadjusted.
        let v = visuals(&p, &e);
        assert_eq!(v.selection.bg_fill, e.selection);
    }
    // A contrived theme whose explicit mode contradicts its colours: the mode still decides.
    assert!(!editor_palette(&core_of(EXPLICIT_LIGHT)).dark);
    // The theme's own colours steer the Markdown roles: a different accent, different headings.
    assert_ne!(
        editor_palette(&core_of(TOKYO)).headings[0],
        editor_palette(&core_of(LATTE)).headings[0]
    );
}

#[test]
fn chrome_text_is_legible_on_every_surface_it_sits_on() {
    for (name, p) in [("dark", Core::DARK), ("light", Core::LIGHT)] {
        for (role, bg) in [
            ("background", p.background),
            ("surface", p.surface),
            ("raised", p.surface_raised),
        ] {
            assert!(
                contrast(colour(p.foreground), colour(bg)) >= 4.5,
                "{name}: foreground on {role}"
            );
            assert!(
                contrast(colour(p.muted_foreground), colour(bg)) >= 3.0,
                "{name}: muted on {role}"
            );
        }
        assert!(
            contrast(colour(p.danger_foreground), colour(p.danger)) >= 3.0,
            "{name}: destructive button label"
        );
        assert!(
            contrast(colour(p.selection_foreground), colour(p.selection)) >= 4.5,
            "{name}"
        );
    }
}

// --- Source selection and fallbacks ----------------------------------------------------------

#[test]
fn without_omarchy_the_system_scheme_picks_a_built_in_palette() {
    let (_d, p) = rich_notebook("theme-fallback");
    let mut h = H::open_tree(&p);
    assert!(!h.host.using_omarchy());
    assert_eq!(h.core(), Core::DARK, "unknown scheme: OmaTree Dark");
    h.system = Some(egui::Theme::Light);
    h.settle();
    assert_eq!(h.core(), Core::LIGHT);
    assert_eq!(h.ctx.theme(), egui::Theme::Light);
    h.system = Some(egui::Theme::Dark);
    h.settle();
    assert_eq!(h.core(), Core::DARK);
    assert_eq!(h.egui_visuals().panel_fill, colour(Core::DARK.background));
}

#[test]
fn a_missing_omarchy_file_means_the_built_in_palette() {
    let (d, p) = rich_notebook("theme-absent");
    let mut h = H::open(&p);
    h.follow(&colours_path(&d.join("nowhere")));
    assert!(!h.host.using_omarchy());
    assert_eq!(h.core(), Core::DARK);
}

#[test]
fn an_omarchy_theme_beats_the_system_scheme_and_its_explicit_mode_is_honoured() {
    let (_d, p) = rich_notebook("theme-mode");
    let mut h = H::open(&p);
    let d = dir("theme-mode-fixture");
    h.system = Some(egui::Theme::Dark);
    h.follow(&install(&d, LATTE));
    assert!(h.host.using_omarchy());
    assert!(!h.core().dark);
    assert_eq!(h.ctx.theme(), egui::Theme::Light);
    // `mode = "light"` wins over a background the brightness heuristic would call dark.
    h.follow(&install(&d, EXPLICIT_LIGHT));
    assert!(!h.core().dark, "explicit mode, not luminance");
    assert!(!h.egui_visuals().dark_mode);
    assert!(!h.host.palette().dark);
    // And a system change does not unseat an Omarchy theme.
    h.system = Some(egui::Theme::Light);
    h.settle();
    h.system = Some(egui::Theme::Dark);
    h.settle();
    assert_eq!(h.core(), core_of(EXPLICIT_LIGHT));
}

#[test]
fn a_gap_or_a_broken_replacement_keeps_the_last_good_theme_without_a_flash() {
    let (_d, p) = rich_notebook("theme-gap");
    let mut h = H::open(&p);
    let d = dir("theme-gap-fixture");
    let colours = install(&d, TOKYO);
    h.follow(&colours);
    let tokyo = core_of(TOKYO);
    assert_eq!(h.core(), tokyo);
    let applied = h.host.theme_applications();
    // Missing for a moment (mid-replacement).
    fs::remove_dir_all(d.join("omarchy/current/theme")).unwrap();
    h.polls(3);
    assert_eq!(
        h.core(),
        tokyo,
        "no fallback flash while the file is missing"
    );
    // Back, unchanged: nothing to do.
    install(&d, TOKYO);
    h.polls(3);
    assert_eq!(
        h.host.theme_applications(),
        applied,
        "same theme again changes nothing"
    );
    // A malformed replacement, and one that lacks the required colours.
    for bad in [
        "this is = = not toml",
        "accent = \"#ffffff\"\n",
        "background = \"red\"\nforeground = \"#fff\"\n",
    ] {
        replace_theme_dir(&d, bad);
        h.polls(3);
        assert_eq!(h.core(), tokyo, "kept through: {bad}");
        assert_eq!(h.host.theme_applications(), applied);
    }
    // A valid one after the broken ones is applied (once it has been confirmed).
    replace_theme_dir(&d, LATTE);
    h.polls(1);
    assert_eq!(h.core(), tokyo, "one sighting is not enough");
    h.polls(2);
    assert_eq!(h.core(), core_of(LATTE));
    assert_eq!(h.host.theme_applications(), applied + 1);
}

#[test]
fn an_idle_app_applies_nothing_and_only_wakes_up_to_poll() {
    let (_d, p) = rich_notebook("theme-idle");
    let mut h = H::open_tree(&p);
    let d = dir("theme-idle-fixture");
    h.follow(&install(&d, TOKYO));
    let applied = h.host.theme_applications();
    for _ in 0..40 {
        h.frame(vec![], Modifiers::NONE);
    }
    h.polls(5);
    assert_eq!(
        h.host.theme_applications(),
        applied,
        "unchanged frames install nothing"
    );
}

// --- Live switching ----------------------------------------------------------------------------

#[test]
fn switching_themes_live_keeps_everything_the_user_had() {
    let (_d0, p) = rich_notebook("theme-live");
    let mut h = H::open_tree(&p);
    let d = dir("theme-live-fixture");
    h.follow(&install(&d, TOKYO));
    h.host.set_all_expanded(&h.ctx.clone(), true);
    h.settle();
    h.select("Child");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("typed");
    h.select("Rich");
    let sel = (3, 12);
    h.set_caret(sel.0, sel.1);
    let before_text = h.body("Rich");
    let child_before = h.body("Child");
    let rows_before = h.host.rows().len();
    let selected_before = h.host.selected();
    let geom_before = h.host.tree_geom;
    let caret_before = h.caret("Rich");
    // Start clean, so "not dirty" afterwards cannot be an autosave's doing.
    h.key(Key::S, CTRL);
    h.settle();
    assert!(!h.host.is_dirty());
    let dirty_before = h.host.is_dirty();
    let edited_before = h.host.edited;

    for next in [LATTE, TOKYO, LATTE] {
        replace_theme_dir(&d, next);
        h.polls(3);
        assert_eq!(h.core(), core_of(next));
        assert_eq!(
            h.egui_visuals().panel_fill,
            colour(core_of(next).background)
        );
        assert_eq!(
            h.host.palette().canvas,
            colour(core_of(next).background),
            "editor too"
        );
        assert_eq!(h.host.palette().dark, core_of(next).dark);
    }
    assert_eq!(h.body("Rich"), before_text, "note text, byte for byte");
    assert_eq!(h.body("Child"), child_before);
    assert_eq!(h.host.buffer(), before_text);
    assert_eq!(h.caret("Rich"), caret_before, "caret and selection");
    assert_eq!(h.host.selected(), selected_before);
    assert_eq!(h.host.rows().len(), rows_before);
    assert_eq!(h.host.tree().len_visible(), rows_before, "expansion kept");
    assert_eq!(h.host.tree_geom.1, geom_before.1);
    assert_eq!(h.host.tree_geom.0, geom_before.0, "tree did not scroll");
    assert_eq!(
        h.host.is_dirty(),
        dirty_before,
        "a theme change is not a content change"
    );
    assert_eq!(h.host.edited, edited_before);
}

#[test]
fn per_note_undo_survives_a_theme_change() {
    let (_d0, p) = rich_notebook("theme-undo");
    let mut h = H::open(&p);
    let d = dir("theme-undo-fixture");
    h.follow(&install(&d, TOKYO));
    h.select("Other");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("XY");
    h.select("Rich");
    let end = h.host.buffer().len();
    h.set_caret(end, end);
    h.type_text("Z");
    replace_theme_dir(&d, LATTE);
    h.polls(3);
    assert!(!h.core().dark);
    h.key(Key::Z, CTRL);
    assert!(!h.body("Rich").ends_with('Z'), "its own typing is undone");
    assert!(
        h.body("Other").ends_with("XY"),
        "the other note is untouched"
    );
    h.select("Other");
    h.key(Key::Z, CTRL);
    assert_eq!(h.body("Other"), "other body\n");
}

#[test]
fn an_open_dialog_stays_open_and_works_across_a_theme_change() {
    let (_d0, p) = rich_notebook("theme-dialog");
    let mut h = H::open_tree(&p);
    let d = dir("theme-dialog-fixture");
    h.follow(&install(&d, TOKYO));
    h.go("Other");
    h.press(Key::Delete);
    assert!(matches!(h.host.dialog(), Dialog::ConfirmDelete(_)));
    replace_theme_dir(&d, LATTE);
    h.polls(3);
    assert!(!h.core().dark);
    assert!(
        matches!(h.host.dialog(), Dialog::ConfirmDelete(_)),
        "still asking"
    );
    // The dialog's destructive button still works afterwards (Tab to Yes, Enter).
    h.press(Key::Tab);
    h.press(Key::Enter);
    assert!(h.host.rows().iter().all(|r| r.title != "Other"), "deleted");
}

#[test]
fn the_delete_button_uses_the_palettes_danger_pair() {
    for p in [Core::DARK, Core::LIGHT] {
        assert_ne!(p.danger, p.danger_foreground);
        assert!(contrast(colour(p.danger_foreground), colour(p.danger)) >= 3.0);
    }
}

#[test]
fn a_note_with_every_kind_of_content_renders_under_both_themes_without_changing() {
    let (_d0, p) = rich_notebook("theme-rich");
    let mut h = H::open(&p);
    let d = dir("theme-rich-fixture");
    h.follow(&install(&d, TOKYO));
    h.select("Rich");
    let fonts = h.ctx.fonts_mut(|f| {
        f.layout_no_wrap(
            "Hg|".to_owned(),
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        )
        .size()
    });
    let ppp = h.ctx.pixels_per_point();
    for next in [LATTE, TOKYO, LATTE, TOKYO] {
        replace_theme_dir(&d, next);
        h.polls(3);
        for _ in 0..3 {
            h.frame(vec![], Modifiers::NONE);
        }
        assert_eq!(h.body("Rich"), RICH, "byte for byte");
        assert_eq!(h.host.buffer(), RICH);
    }
    assert!(!h.host.is_dirty());
    // Fonts were neither reinstalled nor rescaled: glyph metrics and scale are as before.
    let after = h.ctx.fonts_mut(|f| {
        f.layout_no_wrap(
            "Hg|".to_owned(),
            egui::FontId::proportional(14.0),
            egui::Color32::WHITE,
        )
        .size()
    });
    assert_eq!(fonts, after);
    assert_eq!(ppp, h.ctx.pixels_per_point());
    // The editor is on the newest palette (its table cache is dropped by the editor itself on a
    // palette change; see the editor library's own tests for the glyph regression).
    assert_eq!(h.host.palette().canvas, colour(core_of(TOKYO).background));
}

#[test]
fn a_theme_change_does_not_touch_the_file_on_disk() {
    let (_d0, p) = rich_notebook("theme-disk");
    let before = fs::read(&p).unwrap();
    let mut h = H::open(&p);
    let d = dir("theme-disk-fixture");
    h.follow(&install(&d, TOKYO));
    h.select("Rich");
    for next in [LATTE, TOKYO] {
        replace_theme_dir(&d, next);
        h.polls(3);
    }
    h.advance(AUTOSAVE_DELAY + 2.0);
    assert!(!h.host.is_dirty());
    drop(h);
    assert_eq!(fs::read(&p).unwrap(), before, "no write because of a theme");
}

#[test]
fn a_palette_is_a_few_dozen_colour_lookups_not_per_frame_work() {
    // Rebuilding the egui visuals from the palette is cheap enough to do on a change, and the
    // host does it only then (see `an_idle_app_applies_nothing...`).
    let p = core_of(TOKYO);
    let ed = editor_palette(&p);
    let t = std::time::Instant::now();
    for _ in 0..200 {
        std::hint::black_box(visuals(&p, &ed));
    }
    let per = t.elapsed() / 200;
    eprintln!("visuals(): {per:?}");
    assert!(per < std::time::Duration::from_millis(5), "{per:?}");
    let _ = Rgb(0, 0, 0);
}

/// Only the theme, in a bare context: what it asks of the event loop.
fn theme_delay(
    ctx: &egui::Context,
    theme: &mut crate::theme::ThemeState,
    t: f64,
) -> std::time::Duration {
    let input = RawInput {
        time: Some(t),
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0))),
        ..Default::default()
    };
    let mut out = ctx.run_ui(input, |ui| theme.update(ui.ctx()));
    out.textures_delta.clear();
    out.viewport_output[&egui::ViewportId::ROOT].repaint_delay
}

#[test]
fn the_theme_asks_for_wake_ups_only_while_it_has_something_to_watch() {
    let ctx = egui::Context::default();
    // No Omarchy (other desktops, Windows, macOS): never a wake-up on the theme's account.
    let mut none = crate::theme::ThemeState::new(None);
    for k in 0..3 {
        // The first frames install the palette (and egui settles its style).
        theme_delay(&ctx, &mut none, k as f64);
    }
    for k in 3..8 {
        assert_eq!(
            theme_delay(&ctx, &mut none, k as f64),
            std::time::Duration::MAX
        );
    }
    // Following a file: a wake-up before the next poll, asked for again on every frame.
    let d = dir("theme-wake");
    let mut watching = crate::theme::ThemeState::new(Some(install(&d, TOKYO)));
    for k in 0..3 {
        theme_delay(&ctx, &mut watching, 9.0 + k as f64 * 0.01); // installs the palette
    }
    for k in 0..5 {
        let delay = theme_delay(&ctx, &mut watching, 10.0 + k as f64 * 0.1);
        assert!(
            delay > std::time::Duration::ZERO && delay <= std::time::Duration::from_millis(600),
            "frame {k}: {delay:?}"
        );
    }
    // The filesystem is only looked at on the poll beat: a changed file is not noticed between.
    replace_theme_dir(&d, LATTE);
    let before = *watching.palette();
    theme_delay(&ctx, &mut watching, 10.55);
    assert_eq!(*watching.palette(), before);
}

/// Writes the rich notebook to `$OMATREE_GUI_FIXTURE` (manual GUI check of theme switching).
#[test]
#[ignore]
fn write_rich_gui_fixture() {
    let path =
        PathBuf::from(std::env::var_os("OMATREE_GUI_FIXTURE").expect("set OMATREE_GUI_FIXTURE"));
    let _ = fs::remove_file(&path);
    let mut doc = Document::open(&path).unwrap();
    let a = doc.create_root("Rich").unwrap();
    let b = doc.create_child(a, "Child").unwrap();
    let c = doc.create_root("Other").unwrap();
    doc.set_body(a, RICH).unwrap();
    doc.set_body(b, "child body\n").unwrap();
    doc.set_body(c, "other body\n").unwrap();
    doc.save().unwrap();
}
