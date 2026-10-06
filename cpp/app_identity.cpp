#include <QtCore/QCoreApplication>
#include <QtCore/QSize>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

#ifdef Q_OS_WIN
#include <shobjidl.h>
#pragma comment(lib, "shell32.lib")
#endif

// The window/application icon, from the PNGs compiled into the executable
// (assets/logo, see build.rs). Qt picks the size it needs. Called once, after
// the QGuiApplication exists and before any window is shown.
//
// Not on macOS inside an application bundle: there QGuiApplication::
// setWindowIcon() also replaces the Dock and app-switcher icon at run time, and
// the raw square artwork would take the place of the bundle's OmaTree.icns,
// which macOS draws in its own rounded tile (the same icon Finder shows). The
// bundle's icon is the right one, so it is left alone. A bare executable on
// macOS (cargo run) has no bundle icon and still gets this one.
extern "C" void omatree_install_window_icon() {
#ifdef Q_OS_MACOS
    if (QCoreApplication::applicationDirPath().endsWith(QStringLiteral(".app/Contents/MacOS"))) {
        return;
    }
#endif
    QIcon icon;
    for (const int size : {32, 64, 128, 256, 512}) {
        icon.addFile(QStringLiteral(":/icons/assets/logo/omatree-icon-%1.png").arg(size),
                     QSize(size, size));
    }
    QGuiApplication::setWindowIcon(icon);
}

// Windows only: the application's own identity for the taskbar and shell (its
// AppUserModelID), so that every OmaTree window and process groups under one
// taskbar button, and the Start menu shortcut and a pinned button are the same
// application as the running window. Without it Windows derives an identity from
// the executable's path. `id` is the application id, the same string as the
// Linux desktop-file id and the macOS bundle identifier. Called once, first
// thing, before any window exists. A no-op elsewhere.
extern "C" void omatree_set_app_user_model_id(const char *id) {
#ifdef Q_OS_WIN
    SetCurrentProcessExplicitAppUserModelID(reinterpret_cast<const wchar_t *>(QString::fromUtf8(id).utf16()));
#else
    Q_UNUSED(id);
#endif
}
