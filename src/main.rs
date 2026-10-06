// A Windows release build is a GUI program. Without this it is a console
// program, and starting it from Explorer opens a console window beside the
// application. (Debug builds, so `cargo run` and `cargo test`, keep the
// console for their output.)
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

pub mod app;
mod document;
#[cfg(test)]
mod integration_tests;
pub mod markdown;
mod notebook;
mod recovery;
mod search;
mod storage;
mod theme;
pub mod theme_model;
pub mod tree_model;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};

extern "C" {
    /// `cpp/app_identity.cpp`: sets the window icon from the compiled-in PNGs.
    fn omatree_install_window_icon();
    /// `cpp/controls_style.cpp`: selects the Basic Qt Quick Controls style.
    fn omatree_select_controls_style();
    /// `cpp/keyboard_policy.cpp`: lets Tab reach every control on macOS.
    fn omatree_apply_keyboard_policy();
    /// `cpp/file_open.cpp`: macOS Finder "open document" events.
    fn omatree_install_file_open_handler();
}

/// How the desktop knows OmaTree: the name, and the desktop-file id (without
/// `.desktop`) that the launcher, task switcher and Wayland compositor match
/// the window to. It is the application id of the packaged launcher, not the
/// internal QML module URI `org.omatree`.
const DESKTOP_FILE_NAME: &str = "io.github.Dorotabro.OmaTree";

/// The QML the application starts from: always `main.qml`, except in debug
/// builds, where `OMATREE_TEST_QML` may name a file to start from instead.
/// That is how the integration tests in `tests/` drive the real production
/// components without editing them. A release build ignores the variable.
fn entry_point() -> QUrl {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("OMATREE_TEST_QML") {
        // Qt builds the URL, so a drive letter, backslashes, spaces and
        // non-ASCII letters all come out as a valid `file:` URL.
        return QUrl::from_local_file(&QString::from(path.to_string_lossy().as_ref()));
    }
    QUrl::from("qrc:/qt/qml/org/omatree/qml/main.qml")
}

fn main() {
    run_app();
}

/// Starts the application and runs it until it quits. (Also what the
/// integration tests' child process runs; see `integration_tests.rs`.)
pub fn run_app() {
    // The flat, palette-driven Basic style, so the semantic palette looks the
    // same everywhere. A style the user chose explicitly is left alone.
    // SAFETY: a plain C++ function with no arguments, called once on the main
    // thread before any other thread or Qt object exists.
    unsafe { omatree_select_controls_style() };

    let mut qt_app = QGuiApplication::new();
    // The application name is what the desktop calls it. (The display name is
    // left at its default: setting it would make Qt add " — OmaTree" to every
    // window title, which already starts with the name.)
    if let Some(mut app) = qt_app.as_mut() {
        app.as_mut().set_application_name(&QString::from("OmaTree"));
        app.as_mut()
            .set_application_version(&QString::from(env!("CARGO_PKG_VERSION")));
    }
    QGuiApplication::set_desktop_file_name(&QString::from(DESKTOP_FILE_NAME));
    // SAFETY: a plain C++ function with no arguments, called once on the main
    // thread after the QGuiApplication exists.
    unsafe { omatree_install_window_icon() };
    // SAFETY: as above.
    unsafe { omatree_apply_keyboard_policy() };
    // SAFETY: as above.
    unsafe { omatree_install_file_open_handler() };
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&entry_point());
    }

    if let Some(app) = qt_app.as_mut() {
        app.exec();
    }
}
