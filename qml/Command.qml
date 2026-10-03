import QtQuick
import QtQuick.Controls
import org.omatree

// A quiet command: a small fixed-width uppercase label with no chrome. It
// recedes until hovered or focused, and its clickable area is the full height
// of its strip, however small the text is.
AbstractButton {
    id: control

    // Shown as a tooltip after a pause, for example a keyboard shortcut.
    property string hint: ""
    // Shown in the danger colour when hovered or focused (Delete).
    property bool danger: false
    // The current choice of a pair, such as Edit / Preview.
    property bool active: false
    // The resting colour: the muted text colour, a little nearer to the text
    // so that it stays legible on a light theme too.
    readonly property color rest: Qt.tint(Theme.mutedForeground, Qt.alpha(Theme.foreground, 0.35))

    leftPadding: Ui.medium
    rightPadding: Ui.medium
    implicitHeight: Ui.controlHeight
    implicitWidth: label.implicitWidth + leftPadding + rightPadding
    hoverEnabled: true
    // Reachable with Tab, but a click must not take focus from the tree or
    // the editor.
    focusPolicy: Qt.TabFocus

    contentItem: Label {
        id: label

        text: control.text
        horizontalAlignment: Text.AlignHCenter
        verticalAlignment: Text.AlignVCenter
        font.family: Ui.monoFamily
        font.pixelSize: Ui.commandPixelSize
        font.letterSpacing: 0.6
        font.capitalization: Font.AllUppercase
        color: !control.enabled ? Qt.alpha(Theme.mutedForeground, 0.5) : control.down ? Theme.accentSecondary : (control.hovered || control.visualFocus) ? (control.danger ? Theme.danger : Theme.accent) : control.active ? Theme.foreground : control.rest

        Behavior on color {
            ColorAnimation {
                duration: Ui.hoverDuration
            }
        }
    }

    background: Item {
        // Focus (and the current choice of a pair) as a thin line under the text.
        Rectangle {
            anchors.bottom: parent.bottom
            anchors.bottomMargin: Ui.small
            anchors.horizontalCenter: parent.horizontalCenter
            width: label.implicitWidth
            height: control.active ? Ui.bar : Ui.hairline
            color: Theme.accent
            visible: control.active || control.visualFocus
        }
    }

    ToolTip.visible: hovered && hint !== ""
    ToolTip.text: hint
    ToolTip.delay: 700
}
