pub mod app;
// Not yet wired to the UI; exposed through CXX-Qt in a later ticket.
#[allow(dead_code)]
mod notebook;
// Not yet wired to the application.
#[allow(dead_code)]
mod storage;

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
