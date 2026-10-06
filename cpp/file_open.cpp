#include <QtCore/QEvent>
#include <QtCore/QMetaObject>
#include <QtCore/QObject>
#include <QtCore/QString>
#include <QtCore/QUrl>
#include <QtCore/QVariant>
#include <QtGui/QFileOpenEvent>
#include <QtGui/QGuiApplication>
#include <QtGui/QWindow>

namespace {

// macOS does not pass a document that is double-clicked in Finder, or sent
// with "Open With", on the command line: the system sends the running (or just
// started) application a file-open event. This hands such a file to the main
// window's `openFromSystem(url)` (main.qml), which asks about unsaved changes
// exactly as File > Open does. Linux and Windows never see this event.
class FileOpenFilter : public QObject {
public:
    using QObject::QObject;

protected:
    bool eventFilter(QObject *, QEvent *event) override {
        if (event->type() != QEvent::FileOpen) {
            return false;
        }
        const QUrl url = static_cast<QFileOpenEvent *>(event)->url();
        if (!url.isLocalFile()) {
            return true;
        }
        const auto windows = QGuiApplication::topLevelWindows();
        for (QWindow *window : windows) {
            if (QMetaObject::invokeMethod(window, "openFromSystem", Q_ARG(QVariant, url))) {
                break;
            }
        }
        return true;
    }
};

} // namespace

// Called once, after the QGuiApplication exists; a no-op off macOS.
extern "C" void omatree_install_file_open_handler() {
#ifdef Q_OS_MACOS
    if (auto *app = QGuiApplication::instance()) {
        app->installEventFilter(new FileOpenFilter(app));
    }
#endif
}
