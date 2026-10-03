use cxx_qt_build::{CxxQtBuilder, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.omatree").qml_files([
        "qml/main.qml",
        "qml/TreePane.qml",
        "qml/EditorPane.qml",
        "qml/RecoveryDialog.qml",
        "qml/ThemedDialog.qml",
        "qml/SearchPane.qml",
    ]))
    .qt_module("Quick")
    .files([
        "src/app.rs",
        "src/tree_model.rs",
        "src/theme_model.rs",
        "src/markdown.rs",
    ])
    .cpp_file("cpp/markdown_render.cpp")
    .build();
}
