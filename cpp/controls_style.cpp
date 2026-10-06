#include <QtCore/QByteArray>
#include <QtCore/QtGlobal>

// The flat, palette-driven Basic style of Qt Quick Controls, so the semantic
// palette looks the same everywhere. A style the user chose explicitly (the
// environment variable already set) is left alone. Called once, before the
// QGuiApplication is created.
//
// This is done here, with qputenv, rather than with Rust's std::env::set_var:
// on Windows that only changes the process environment the operating system
// keeps, not the copy the C runtime keeps, and the C runtime's copy is the one
// Qt reads. Qt would then never see the variable and use the native Windows
// style. qputenv changes both (on Linux and macOS it is setenv).
extern "C" void omatree_select_controls_style() {
    if (!qEnvironmentVariableIsSet("QT_QUICK_CONTROLS_STYLE")) {
        qputenv("QT_QUICK_CONTROLS_STYLE", QByteArrayLiteral("Basic"));
    }
}
