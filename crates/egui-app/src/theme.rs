//! The egui host's theme pipeline (ticket E6a).
//!
//! `omatree_core::theme` is the only authority on which colours OmaTree uses: it finds the active
//! Omarchy `colors.toml` (honouring `XDG_STATE_HOME`), parses it with OmaTree's fallbacks,
//! confirms changes over two polls, keeps the last good palette through gaps and broken files, and
//! falls back to OmaTree Dark/Light by the system's colour scheme. This module only *translates*
//! the resulting semantic [`core::Palette`]:
//!
//! * into egui `Visuals` for every native widget ([`visuals`]), and
//! * into the embedded editor's palette ([`editor_palette`]).
//!
//! The editor never reads files or the environment; it receives the palette the host chose, and
//! it keeps its own contrast safeguards (the editor's constructor is used, not bypassed).
//! Nothing is parsed or read per frame: the watcher is polled twice a second, and egui is only
//! touched when the chosen palette actually differs from the one in use.

use eframe::egui::{self, Color32, Stroke, Visuals};
use omatree_core::theme::{self as core, OmarchyWatcher, Rgb, Scheme};
use omatree_editor::theme::Palette as EditorPalette;
use std::path::PathBuf;

/// How often the Omarchy file is looked at (as in the Qt app's `main.qml`).
pub const POLL_SECS: f64 = 0.5;

pub fn colour(c: Rgb) -> Color32 {
    Color32::from_rgb(c.0, c.1, c.2)
}

/// `a` moved `t` of the way towards `b`.
pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (f32::from(x) + (f32::from(y) - f32::from(x)) * t).round() as u8;
    Color32::from_rgb(l(a.r(), b.r()), l(a.g(), b.g()), l(a.b(), b.b()))
}

/// egui's report of the operating system's colour scheme.
pub fn scheme_of(system: Option<egui::Theme>) -> Scheme {
    match system {
        Some(egui::Theme::Dark) => Scheme::Dark,
        Some(egui::Theme::Light) => Scheme::Light,
        None => Scheme::Unknown,
    }
}

/// The core palette as the editor's palette. The editor derives its Markdown roles (headings,
/// links, code, quotes, selection, revealed delimiters) and enforces contrast on them itself, so
/// this hands it the semantic colours under the names its constructor reads:
///
/// | core role                 | editor input key                                   |
/// |---------------------------|----------------------------------------------------|
/// | `dark`                    | `mode`                                             |
/// | `background`, `foreground`| `background`, `foreground`                         |
/// | `accent`                  | `accent` (headings H1, links)                      |
/// | `accent_secondary`        | `magenta` (H2)                                     |
/// | `positive`                | `green` (H3)                                       |
/// | `selection`               | `selection`                                        |
/// | `muted_foreground`        | `dark_foreground` (muted text, revealed delimiters)|
/// | `selection_foreground`    | `bright_foreground`                                |
/// | `border`                  | `muted` (image borders)                            |
pub fn editor_palette(p: &core::Palette) -> EditorPalette {
    let hex = |c: Rgb| c.to_hex();
    let src = format!(
        "mode = \"{}\"\nbackground = \"{}\"\nforeground = \"{}\"\naccent = \"{}\"\n\
         magenta = \"{}\"\ngreen = \"{}\"\nselection = \"{}\"\ndark_foreground = \"{}\"\n\
         bright_foreground = \"{}\"\nmuted = \"{}\"\n",
        if p.dark { "dark" } else { "light" },
        hex(p.background),
        hex(p.foreground),
        hex(p.accent),
        hex(p.accent_secondary),
        hex(p.positive),
        hex(p.selection),
        hex(p.muted_foreground),
        hex(p.selection_foreground),
        hex(p.border),
    );
    // All values above are valid `#rrggbb`, so this cannot fail; the fallback is belt and braces.
    EditorPalette::from_toml(&src).unwrap_or_else(|_| EditorPalette::fallback())
}

/// The core palette as egui visuals (colours only: no sizes, spacing or fonts).
///
/// Qt parity: the tree and the status strip sit on `surface`, the editor on `background`; menus,
/// popups and dialogs on `surface_raised`; hairlines are `border`; selection is `selection`;
/// destructive emphasis is `danger` with `danger_foreground`.
pub fn visuals(p: &core::Palette, ed: &EditorPalette) -> Visuals {
    let mut v = if p.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };
    let (background, surface, raised) = (
        colour(p.background),
        colour(p.surface),
        colour(p.surface_raised),
    );
    let (fg, muted, border) = (
        colour(p.foreground),
        colour(p.muted_foreground),
        colour(p.border),
    );
    let (accent, selection, on_selection) = (
        colour(p.accent),
        colour(p.selection),
        colour(p.selection_foreground),
    );
    let hover = mix(raised, fg, 0.10);

    v.override_text_color = None;
    v.weak_text_color = Some(muted);
    v.panel_fill = background;
    v.window_fill = raised;
    v.window_stroke = Stroke::new(1.0, border);
    v.extreme_bg_color = surface;
    v.faint_bg_color = raised;
    v.code_bg_color = ed.code_bg;
    v.hyperlink_color = accent;
    v.error_fg_color = colour(p.danger);
    v.warn_fg_color = colour(p.warning);
    // The editor's selection is the core one, adjusted by the editor if it would be invisible.
    v.selection.bg_fill = ed.selection;
    v.selection.stroke = Stroke::new(1.0, on_selection);
    v.text_cursor.stroke.color = fg;

    let w = &mut v.widgets;
    w.noninteractive.bg_fill = surface;
    w.noninteractive.weak_bg_fill = surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, fg);
    w.inactive.bg_fill = raised;
    w.inactive.weak_bg_fill = raised;
    w.inactive.bg_stroke = Stroke::new(1.0, border);
    w.inactive.fg_stroke = Stroke::new(1.0, fg);
    w.hovered.bg_fill = hover;
    w.hovered.weak_bg_fill = hover;
    w.hovered.bg_stroke = Stroke::new(1.0, mix(border, fg, 0.3));
    w.hovered.fg_stroke = Stroke::new(1.5, fg);
    w.active.bg_fill = selection;
    w.active.weak_bg_fill = selection;
    w.active.bg_stroke = Stroke::new(1.0, accent);
    w.active.fg_stroke = Stroke::new(2.0, on_selection);
    w.open.bg_fill = hover;
    w.open.weak_bg_fill = hover;
    w.open.bg_stroke = Stroke::new(1.0, border);
    w.open.fg_stroke = Stroke::new(1.0, fg);
    v
}

/// What the host currently shows, and where it comes from.
pub struct ThemeState {
    watcher: OmarchyWatcher,
    /// The last valid Omarchy palette (kept through gaps and broken files, as in Qt).
    omarchy: Option<core::Palette>,
    scheme: Scheme,
    current: core::Palette,
    editor: EditorPalette,
    applied: bool,
    next_poll: f64,
    /// How many times a palette was installed into egui (for tests: unchanged frames add none).
    applications: u64,
}

impl ThemeState {
    /// `path` is the Omarchy colours file to follow, or none (non-Linux, no Omarchy): the host
    /// passes `core::default_omarchy_colors_path()`; tests pass a fixture.
    pub fn new(path: Option<PathBuf>) -> Self {
        let mut watcher = OmarchyWatcher::new(path);
        let omarchy = watcher.load_now();
        let current = core::select(omarchy, Scheme::Unknown);
        ThemeState {
            watcher,
            omarchy,
            scheme: Scheme::Unknown,
            editor: editor_palette(&current),
            current,
            applied: false,
            next_poll: 0.0,
            applications: 0,
        }
    }

    pub fn palette(&self) -> &core::Palette {
        &self.current
    }

    pub fn editor(&self) -> &EditorPalette {
        &self.editor
    }

    pub fn applications(&self) -> u64 {
        self.applications
    }

    pub fn using_omarchy(&self) -> bool {
        self.omarchy.is_some()
    }

    /// Once per frame: follow the system scheme, poll the watcher when it is due, and install a
    /// palette into egui only when the chosen one differs from the one in use.
    pub fn update(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        let scheme = scheme_of(ctx.system_theme());
        let mut inputs_changed = !self.applied || scheme != self.scheme;
        self.scheme = scheme;
        if self.watcher.is_active() {
            if now >= self.next_poll {
                self.next_poll = now + POLL_SECS;
                if let Some(palette) = self.watcher.poll() {
                    self.omarchy = Some(palette);
                    inputs_changed = true;
                }
            }
            // eframe keeps only the latest repaint request, so ask again every frame.
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(
                (self.next_poll - now).max(0.01),
            ));
        }
        if inputs_changed {
            let chosen = core::select(self.omarchy, scheme);
            if !self.applied || chosen != self.current {
                self.current = chosen;
                self.editor = editor_palette(&chosen);
                self.install(ctx);
            }
            self.applied = true;
        }
    }

    /// Install the current palette: egui's theme follows the palette (not the system), so a
    /// system switch can never flip egui to a style that was never filled in.
    fn install(&mut self, ctx: &egui::Context) {
        let theme = if self.current.dark {
            egui::Theme::Dark
        } else {
            egui::Theme::Light
        };
        ctx.set_theme(theme);
        ctx.set_visuals_of(theme, visuals(&self.current, &self.editor));
        ctx.request_repaint();
        self.applications += 1;
    }
}
