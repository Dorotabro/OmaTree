import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Left pane: the note tree plus the two creation buttons.
Item {
    id: pane

    required property var notebook
    required property ItemSelectionModel selection

    signal newRootRequested
    signal newChildRequested
    signal deleteRequested

    // Expands ancestors of `index` and scrolls it into view.
    function reveal(index) {
        tree.expandToIndex(index);
        Qt.callLater(() => {
            const row = tree.rowAtIndex(index);
            if (row >= 0)
                tree.positionViewAtRow(row, TableView.Contain);
        });
    }

    function focusTree() {
        tree.forceActiveFocus();
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TreeView {
            id: tree

            Layout.fillWidth: true
            Layout.fillHeight: true
            clip: true
            model: pane.notebook
            selectionModel: pane.selection
            boundsBehavior: Flickable.StopAtBounds
            columnWidthProvider: () => tree.width
            onWidthChanged: forceLayout()

            ScrollBar.vertical: ScrollBar {}

            Keys.onDeletePressed: pane.deleteRequested()

            delegate: TreeViewDelegate {
                id: item

                // Selection follows the current index, whether it was set by
                // a click or by keyboard navigation.
                background: Rectangle {
                    color: item.current ? item.palette.highlight : "transparent"
                }
                contentItem: Label {
                    text: item.model.display
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                    color: item.current ? item.palette.highlightedText : item.palette.windowText
                }

                TapHandler {
                    onTapped: {
                        pane.selection.setCurrentIndex(item.treeView.index(item.row, item.column), ItemSelectionModel.ClearAndSelect);
                        item.treeView.forceActiveFocus();
                    }
                }
            }
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 6
            spacing: 4

            Button {
                text: qsTr("New note")
                flat: true
                onClicked: pane.newRootRequested()
            }
            Button {
                text: qsTr("New child")
                flat: true
                enabled: pane.selection.currentIndex.valid
                onClicked: pane.newChildRequested()
            }
        }
    }

    // Shown instead of an empty-looking tree.
    ColumnLayout {
        anchors.centerIn: parent
        visible: tree.rows === 0
        spacing: 12

        Label {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("No notes yet")
            opacity: 0.6
        }
        Button {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("Create the first note")
            onClicked: pane.newRootRequested()
        }
    }
}
