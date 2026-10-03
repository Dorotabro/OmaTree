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

use crate::theme::{default_omarchy_colors_path, select, OmarchyWatcher, Palette, Scheme};

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
        let hex = |c: crate::theme::Rgb| QString::from(c.to_hex().as_str());
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
        let scheme = Scheme::from_qt(scheme);
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
