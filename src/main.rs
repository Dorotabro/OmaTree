pub mod app;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_instantiation() {
        let _app = app::OmaTreeAppRust::default();
    }
}
