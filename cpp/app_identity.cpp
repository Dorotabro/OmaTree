#include <QtCore/QCoreApplication>
#include <QtCore/QSize>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

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
