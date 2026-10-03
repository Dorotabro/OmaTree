pub mod app;
mod document;
mod notebook;
mod recovery;
mod storage;
mod theme;
pub mod theme_model;
pub mod tree_model;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    // The flat, palette-driven Basic style, so the semantic palette looks the
    // same everywhere. A style the user chose explicitly is left alone.
    if std::env::var_os("QT_QUICK_CONTROLS_STYLE").is_none() {
        std::env::set_var("QT_QUICK_CONTROLS_STYLE", "Basic");
    }

    let mut qt_app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/org/omatree/qml/main.qml"));
    }

    if let Some(app) = qt_app.as_mut() {
        app.exec();
    }
}
