import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

// The permanent search field at the top of the navigation pane: a "/" prompt
// and an unboxed field, over a separator that lines up with the one under the
// editor's header. It only holds the query and reports keys; TreePane decides
// what they mean.
Item {
    id: header

    property alias text: field.text
    readonly property bool focused: field.activeFocus

    // Up (-1) and Down (+1) pressed in the field.
    signal moveRequested(int step)
    // Enter.
    signal activateRequested
    // Escape.
    signal escapeRequested

    // Focus the field with the current query selected.
    function focusAll() {
        field.forceActiveFocus();
        field.selectAll();
    }

    implicitHeight: Ui.headerHeight

    RowLayout {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.leftMargin: Ui.medium
        anchors.rightMargin: Ui.medium
        height: Ui.headerHeight - Ui.hairline
        spacing: Ui.small

        Label {
            text: "/"
            color: field.activeFocus ? Theme.accent : Theme.mutedForeground
            font.family: Ui.monoFamily
            font.pixelSize: 15
        }
        TextField {
            id: field

            Layout.fillWidth: true
            leftPadding: 0
            placeholderText: qsTr("search notes")
            color: Theme.foreground
            placeholderTextColor: Theme.mutedForeground
            selectionColor: Theme.selection
            selectedTextColor: Theme.selectionForeground
            background: null

            Keys.onDownPressed: header.moveRequested(1)
            Keys.onUpPressed: header.moveRequested(-1)
            Keys.onReturnPressed: header.activateRequested()
            Keys.onEnterPressed: header.activateRequested()
            Keys.onEscapePressed: header.escapeRequested()
        }
    }

    // The separator, quietly accent while the field has the keyboard.
    Rectangle {
        objectName: "headerSeparator"
        anchors.bottom: parent.bottom
        width: parent.width
        height: Ui.hairline
        color: field.activeFocus ? Theme.accent : Theme.border
    }
}
