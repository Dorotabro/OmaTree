//! The one `Theme` QML singleton: the current semantic palette. All
//! OmaTree-owned QML reads its colours from here and never from literals.
//!
//! Choosing the palette (Omarchy, or OmaTree Dark/Light by system scheme) and
//! following Omarchy's live changes is `theme.rs`; this file only exposes the
//! result to QML. It touches nothing but presentation: no notebook, document,
//! selection or autosave state is reachable from here.

use core::pin::Pin;

use cxx_qt::CxxQtType;
use cxx_qt_lib::QString;

use omatree_core::theme::{default_omarchy_colors_path, select, OmarchyWatcher, Palette, Scheme};

/// From `Qt::ColorScheme` (0 unknown, 1 light, 2 dark).
fn scheme_from_qt(value: i32) -> Scheme {
    match value {
        1 => Scheme::Light,
        2 => Scheme::Dark,
        _ => Scheme::Unknown,
    }
}

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qml_singleton]
        #[qproperty(QString, background, READ, NOTIFY)]
        #[qproperty(QString, surface, READ, NOTIFY)]
        #[qproperty(QString, surface_raised, cxx_name = "surfaceRaised", READ, NOTIFY)]
        #[qproperty(QString, foreground, READ, NOTIFY)]
        #[qproperty(QString, muted_foreground, cxx_name = "mutedForeground", READ, NOTIFY)]
        #[qproperty(QString, accent, READ, NOTIFY)]
        #[qproperty(QString, selection, READ, NOTIFY)]
        #[qproperty(
            QString,
            selection_foreground,
            cxx_name = "selectionForeground",
            READ,
            NOTIFY
        )]
        #[qproperty(QString, border, READ, NOTIFY)]
        #[qproperty(QString, danger, READ, NOTIFY)]
        #[qproperty(
            QString,
            danger_foreground,
            cxx_name = "dangerForeground",
            READ,
            NOTIFY
        )]
        #[qproperty(QString, accent_secondary, cxx_name = "accentSecondary", READ, NOTIFY)]
        #[qproperty(QString, positive, READ, NOTIFY)]
        #[qproperty(QString, warning, READ, NOTIFY)]
        type Theme = super::ThemeRust;
    }

    unsafe extern "RustQt" {
        /// The system's `Qt.styleHints.colorScheme` (0 unknown, 1 light,
        /// 2 dark). Called at startup and whenever it changes.
        #[qinvokable]
        #[cxx_name = "setSystemScheme"]
        fn set_system_scheme(self: Pin<&mut Theme>, scheme: i32);

        /// Looks at the active Omarchy theme once; call it on a slow timer.
        /// Applies a new palette only once a change is confirmed.
        #[qinvokable]
        fn poll(self: Pin<&mut Theme>);

        /// Whether there is an Omarchy theme location to follow here at all
        /// (Linux only). Not whether Omarchy is installed.
        #[qinvokable]
        #[cxx_name = "watchesOmarchy"]
        fn watches_omarchy(self: &Theme) -> bool;

        /// Whether the colours currently come from an Omarchy theme.
        #[qinvokable]
        #[cxx_name = "usingOmarchy"]
        fn using_omarchy(self: &Theme) -> bool;
    }
}

pub struct ThemeRust {
    scheme: Scheme,
    omarchy: Option<Palette>,
    watcher: OmarchyWatcher,
    background: QString,
    surface: QString,
    surface_raised: QString,
    foreground: QString,
    muted_foreground: QString,
    accent: QString,
    selection: QString,
    selection_foreground: QString,
    border: QString,
    danger: QString,
    danger_foreground: QString,
    accent_secondary: QString,
    positive: QString,
    warning: QString,
}

impl Default for ThemeRust {
    fn default() -> Self {
        // Read Omarchy right away so the first frame already has its colours.
        let mut watcher = OmarchyWatcher::new(default_omarchy_colors_path());
        let omarchy = watcher.load_now();
        let palette = select(omarchy, Scheme::Unknown);
        let hex = |c: omatree_core::theme::Rgb| QString::from(c.to_hex().as_str());
        ThemeRust {
            scheme: Scheme::Unknown,
            omarchy,
            watcher,
            background: hex(palette.background),
            surface: hex(palette.surface),
            surface_raised: hex(palette.surface_raised),
            foreground: hex(palette.foreground),
            muted_foreground: hex(palette.muted_foreground),
            accent: hex(palette.accent),
            selection: hex(palette.selection),
            selection_foreground: hex(palette.selection_foreground),
            border: hex(palette.border),
            danger: hex(palette.danger),
            danger_foreground: hex(palette.danger_foreground),
            accent_secondary: hex(palette.accent_secondary),
            positive: hex(palette.positive),
            warning: hex(palette.warning),
        }
    }
}

/// Updates one property from the palette, notifying only if it changed.
macro_rules! sync_role {
    ($this:ident, $palette:ident, $role:ident, $changed:ident) => {
        let value = QString::from($palette.$role.to_hex().as_str());
        if $this.rust().$role != value {
            $this.as_mut().rust_mut().$role = value;
            $this.as_mut().$changed();
        }
    };
}

impl qobject::Theme {
    /// Recomputes the palette from the current inputs and publishes whatever
    /// changed.
    fn refresh(mut self: Pin<&mut Self>) {
        let palette = select(self.rust().omarchy, self.rust().scheme);
        sync_role!(self, palette, background, background_changed);
        sync_role!(self, palette, surface, surface_changed);
        sync_role!(self, palette, surface_raised, surface_raised_changed);
        sync_role!(self, palette, foreground, foreground_changed);
        sync_role!(self, palette, muted_foreground, muted_foreground_changed);
        sync_role!(self, palette, accent, accent_changed);
        sync_role!(self, palette, selection, selection_changed);
        sync_role!(
            self,
            palette,
            selection_foreground,
            selection_foreground_changed
        );
        sync_role!(self, palette, border, border_changed);
        sync_role!(self, palette, danger, danger_changed);
        sync_role!(self, palette, danger_foreground, danger_foreground_changed);
        sync_role!(self, palette, accent_secondary, accent_secondary_changed);
        sync_role!(self, palette, positive, positive_changed);
        sync_role!(self, palette, warning, warning_changed);
    }

    fn set_system_scheme(mut self: Pin<&mut Self>, scheme: i32) {
        let scheme = scheme_from_qt(scheme);
        if self.rust().scheme != scheme {
            self.as_mut().rust_mut().scheme = scheme;
            self.refresh();
        }
    }

    fn poll(mut self: Pin<&mut Self>) {
        if let Some(palette) = self.as_mut().rust_mut().watcher.poll() {
            self.as_mut().rust_mut().omarchy = Some(palette);
            self.refresh();
        }
    }

    fn watches_omarchy(&self) -> bool {
        self.rust().watcher.is_active()
    }

    fn using_omarchy(&self) -> bool {
        self.rust().omarchy.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qt_colour_scheme_values_map_to_schemes() {
        assert_eq!(scheme_from_qt(1), Scheme::Light);
        assert_eq!(scheme_from_qt(2), Scheme::Dark);
        assert_eq!(scheme_from_qt(0), Scheme::Unknown);
        assert_eq!(scheme_from_qt(99), Scheme::Unknown);
    }

    #[test]
    fn qml_contains_no_literal_ui_colours() {
        // All colour decisions flow from the semantic Theme. A literal
        // #rgb / #rrggbb / #aarrggbb string in any QML file is a regression.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("qml");
        let mut checked = 0;
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_none_or(|e| e != "qml") {
                continue;
            }
            checked += 1;
            let text = std::fs::read_to_string(&path).unwrap();
            for (n, line) in text.lines().enumerate() {
                let code = line.split("//").next().unwrap_or("");
                let mut rest = code;
                while let Some(at) = rest.find('#') {
                    let digits: String = rest[at + 1..]
                        .chars()
                        .take_while(|c| c.is_ascii_hexdigit())
                        .collect();
                    assert!(
                        !matches!(digits.len(), 3 | 4 | 6 | 8),
                        "{}:{} has a literal colour: {line}",
                        path.display(),
                        n + 1
                    );
                    rest = &rest[at + 1..];
                }
            }
        }
        assert!(checked >= 6, "looked at {checked} QML files");
    }
}
