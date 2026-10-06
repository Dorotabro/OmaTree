use cxx_qt_build::{CxxQtBuilder, QResource, QResources, QmlFile, QmlModule};

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.omatree").qml_files([
        QmlFile::from("qml/Ui.qml").singleton(true),
        QmlFile::from("qml/Keymap.qml").singleton(true),
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
    .cpp_file("cpp/app_identity.cpp")
    .cpp_file("cpp/keyboard_policy.cpp")
    .cpp_file("cpp/file_open.cpp")
    // The application icon, compiled into the executable (see
    // cpp/app_identity.cpp), so the window has it however OmaTree is started.
    .qrc_resources(
        QResources::new().resource(QResource::new().prefix("/icons").files([
            "assets/logo/omatree-icon-32.png",
            "assets/logo/omatree-icon-64.png",
            "assets/logo/omatree-icon-128.png",
            "assets/logo/omatree-icon-256.png",
            "assets/logo/omatree-icon-512.png",
        ])),
    )
    .build();
}
