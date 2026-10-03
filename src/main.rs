pub mod app;
mod document;
mod notebook;
mod recovery;
mod storage;
pub mod tree_model;

use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};

fn main() {
    let mut qt_app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();

    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/org/omatree/qml/main.qml"));
    }

    if let Some(app) = qt_app.as_mut() {
        app.exec();
    }
}
