import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

// The keyboard reference (F1, or File ▸ Keyboard Shortcuts). Text only, and
// only a reference: nothing in it is clickable and opening it changes nothing
// in the notebook.
//
// Every entry below is a binding that exists in the source: the Shortcuts in
// main.qml and the key handling of TreePane, SearchPane, EditorPane,
// ThemedDialog and Command. Change them together.
ThemedDialog {
    id: dialog

    // The sections, in the order shown: [heading, [[keys, what it does], ...]].
    readonly property var sections: [
        [qsTr("Notes"), [["Ctrl+N", qsTr("New note")], ["Ctrl+Shift+N", qsTr("New child")], ["F2", qsTr("Rename the note")], ["Delete", qsTr("Move the note to Trash (in the tree)")], ["Alt+Up", qsTr("Move the note up")], ["Alt+Down", qsTr("Move the note down")]]],
        [qsTr("Tree"), [["Up / Down", qsTr("Move through the notes")], ["Left / Right", qsTr("Collapse / expand")], ["Ctrl+Left", qsTr("Collapse the subtree")], ["Ctrl+Right", qsTr("Expand the subtree")], ["Ctrl+Click", qsTr("Toggle a whole subtree (marker)")], ["Shift+F10", qsTr("Open the tree menu")], ["Menu key", qsTr("Open the tree menu")]]],
        [qsTr("Editor"), [["Ctrl+E", qsTr("Edit / Preview")], ["Ctrl+Z", qsTr("Undo")], ["Ctrl+Shift+Z", qsTr("Redo")], ["Ctrl+X / C / V", qsTr("Cut / copy / paste")], ["Ctrl+A", qsTr("Select all")]]],
        [qsTr("Search"), [["Ctrl+F", qsTr("Focus the search field")], ["Up / Down", qsTr("Choose a result")], ["Enter", qsTr("Open the result")], ["Escape", qsTr("Clear the search")]]],
        [qsTr("Files"), [["Ctrl+O", qsTr("Open a notebook")], ["Ctrl+S", qsTr("Save")], ["Ctrl+Shift+S", qsTr("Save As")]]],
        [qsTr("General"), [["F1", qsTr("This reference")], ["Escape", qsTr("Close a dialog or menu")], ["Tab / Shift+Tab", qsTr("Move between controls")]]]
    ]

    parent: Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(460, parent.width - 48)
    // As tall as the list needs, but never more than the window allows; a
    // longer list scrolls.
    height: Math.min(implicitHeight, parent.height - 48)
    modal: true
    title: qsTr("Keyboard Shortcuts")
    standardButtons: Dialog.Close
    // The list takes the focus, so the arrow keys and Page Up / Down scroll
    // it; Tab reaches Close, and Enter or Escape closes.
    initialFocusItem: flick

    contentItem: Flickable {
        id: flick

        implicitHeight: column.implicitHeight
        contentHeight: column.implicitHeight
        contentWidth: width
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        ScrollBar.vertical: ScrollBar {}

        function scrollBy(delta) {
            contentY = Math.max(0, Math.min(contentHeight - height, contentY + delta));
        }
        Keys.onUpPressed: scrollBy(-Ui.rowHeight)
        Keys.onDownPressed: scrollBy(Ui.rowHeight)
        Keys.onPressed: event => {
            if (event.key === Qt.Key_PageUp)
                scrollBy(-height);
            else if (event.key === Qt.Key_PageDown)
                scrollBy(height);
            else if (event.key === Qt.Key_Home)
                contentY = 0;
            else if (event.key === Qt.Key_End)
                scrollBy(contentHeight);
            else if (event.key === Qt.Key_F1 || event.key === Qt.Key_Return || event.key === Qt.Key_Enter)
                dialog.close();
            else
                return;
            event.accepted = true;
        }

        ColumnLayout {
            id: column

            width: flick.width - Ui.large
            spacing: 0

            Repeater {
                model: dialog.sections

                ColumnLayout {
                    id: section

                    required property var modelData

                    Layout.fillWidth: true
                    spacing: 0

                    Label {
                        Layout.topMargin: Ui.medium
                        Layout.bottomMargin: Ui.tiny
                        text: section.modelData[0]
                        color: Theme.accentSecondary
                        font.family: Ui.monoFamily
                        font.pixelSize: Ui.commandPixelSize
                        font.letterSpacing: 0.6
                        font.capitalization: Font.AllUppercase
                    }
                    Repeater {
                        model: section.modelData[1]

                        RowLayout {
                            id: entry

                            required property var modelData

                            Layout.fillWidth: true
                            spacing: Ui.medium

                            Label {
                                Layout.preferredWidth: 130
                                text: entry.modelData[0]
                                color: Theme.accent
                                font.family: Ui.monoFamily
                                font.pixelSize: Ui.commandPixelSize
                            }
                            Label {
                                Layout.fillWidth: true
                                text: entry.modelData[1]
                                color: Theme.foreground
                                elide: Text.ElideRight
                            }
                        }
                    }
                }
            }
        }
    }
}
