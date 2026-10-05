#include <QtGui/QGuiApplication>
#include <QtGui/QStyleHints>

// macOS lets Tab skip buttons and tab headers unless the user has turned on
// Full Keyboard Access, which would leave the dialog buttons and Recovery's
// Checkpoints tab out of the keyboard's reach. OmaTree draws its own focus
// mark everywhere, so there Tab visits every control, as it already does on
// the other platforms. Called once, after the QGuiApplication exists.
extern "C" void omatree_apply_keyboard_policy() {
#ifdef Q_OS_MACOS
    QGuiApplication::styleHints()->setTabFocusBehavior(Qt::TabFocusAllControls);
#endif
}
