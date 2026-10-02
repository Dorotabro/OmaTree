use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.omatree").qml_file("qml/main.qml"))
        .qt_module("Quick")
        .files(["src/app.rs", "src/tree_model.rs"])
        .build();
}
