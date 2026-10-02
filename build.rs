use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.omatree").qml_files([
        "qml/main.qml",
        "qml/TreePane.qml",
        "qml/EditorPane.qml",
    ]))
    .qt_module("Quick")
    .files(["src/app.rs", "src/tree_model.rs"])
    .build();
}
