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

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

/// The QML the application starts from: always `main.qml`, except in debug
/// builds, where `OMATREE_TEST_QML` may name a file to start from instead.
/// That is how the integration tests in `tests/` drive the real production
/// components without editing them. A release build ignores the variable.
fn entry_point() -> String {
    #[cfg(debug_assertions)]
    if let Some(path) = std::env::var_os("OMATREE_TEST_QML") {
        return format!("file://{}", path.to_string_lossy());
    }
    "qrc:/qt/qml/org/omatree/qml/main.qml".to_string()
}

fn main() {
    run_app();
}

/// Starts the application and runs it until it quits. (Also what the
/// integration tests' child process runs; see `integration_tests.rs`.)
pub fn run_app() {
    // The flat, palette-driven Basic style, so the semantic palette looks the
    // same everywhere. A style the user chose explicitly is left alone.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
    }

    let mut qt_app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(entry_point().as_str()));
    }

    if let Some(app) = qt_app.as_mut() {
        app.exec();
    }
}
