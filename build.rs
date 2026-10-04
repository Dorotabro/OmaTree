use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.omatree").qml_files([
        QmlFile::from("qml/Ui.qml").singleton(true),
        QmlFile::from("qml/main.qml"),
        QmlFile::from("qml/TreePane.qml"),
        QmlFile::from("qml/EditorPane.qml"),
        QmlFile::from("qml/RecoveryDialog.qml"),
        QmlFile::from("qml/ThemedDialog.qml"),
        QmlFile::from("qml/SearchPane.qml"),
        QmlFile::from("qml/SearchField.qml"),
        QmlFile::from("qml/Command.qml"),
        QmlFile::from("qml/PointerHover.qml"),
        QmlFile::from("qml/KeyboardShortcuts.qml"),
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
