#include <QtCore/QSize>
#include <QtCore/QString>
#include <QtGui/QGuiApplication>
#include <QtGui/QIcon>

// The window/application icon, from the PNGs compiled into the executable
// (assets/logo, see build.rs). Qt picks the size it needs. Called once, after
// the QGuiApplication exists and before any window is shown.
extern "C" void omatree_install_window_icon() {
    QIcon icon;
    for (const int size : {32, 64, 128, 256, 512}) {
        icon.addFile(QStringLiteral(":/icons/assets/logo/omatree-icon-%1.png").arg(size),
                     QSize(size, size));
    }
    QGuiApplication::setWindowIcon(icon);
}
