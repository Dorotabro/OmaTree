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
    signal openRequested
    signal saveRequested
    signal saveAsRequested
    signal recoveryRequested

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

    // --- Drag and drop -----------------------------------------------------
    // A row is dragged with the pointer. Where it would land is worked out
    // from the pointer position (upper/lower quarter of a row: before/after
    // it; the middle: as its child; empty space below the rows: the end of
    // the top level). NotebookModel does the move and stays the only
    // authority on the hierarchy; nothing here keeps a copy of it.

    property bool dragging: false
    // Fresh index of the row being dragged (a copy, taken when the drag began).
    property var dragIndex: null
    // Where a drop would go: "" (nowhere), "before", "after", "child", "end".
    property string dropKind: ""
    property var dropParent: null
    property int dropRow: 0
    property rect dropRect: Qt.rect(0, 0, 0, 0)

    function clearDrop() {
        dropKind = "";
    }

    function beginDrag(index) {
        dragIndex = index;
        dragging = true;
        clearDrop();
    }

    function setDrop(kind, parentIndex, row, rect) {
        if (!notebook.canDrop(dragIndex, parentIndex)) {
            clearDrop();
            return;
        }
        dropKind = kind;
        dropParent = parentIndex;
        dropRow = row;
        dropRect = rect;
    }

    // `scenePosition` is the pointer position in window coordinates.
    function updateDrag(scenePosition) {
        const p = tree.mapFromItem(null, scenePosition.x, scenePosition.y);
        if (p.x < 0 || p.y < 0 || p.x > tree.width || p.y > tree.height) {
            clearDrop();
            return;
        }
        const cell = tree.cellAtPosition(p.x, p.y, true);
        if (cell.y < 0) {
            // Empty space below the last row: the end of the top level.
            const rootIndex = tree.rootIndex;
            const last = tree.itemAtCell(Qt.point(0, tree.rows - 1));
            const bottom = last ? last.mapToItem(tree, 0, last.height).y : 0;
            setDrop("end", rootIndex, notebook.rowCount(rootIndex), Qt.rect(0, bottom - 1, tree.width, 2));
            return;
        }
        const row = tree.itemAtCell(Qt.point(cell.x, cell.y));
        if (!row) {
            clearDrop();
            return;
        }
        const index = tree.index(cell.y, 0);
        const top = row.mapToItem(tree, 0, 0).y;
        const fraction = (p.y - top) / row.height;
        const parentIndex = notebook.parent(index);
        if (fraction < 0.25) {
            setDrop("before", parentIndex, index.row, Qt.rect(0, top - 1, tree.width, 2));
        } else if (fraction > 0.75) {
            if (tree.isExpanded(cell.y) && notebook.rowCount(index) > 0)
                // Just under an open parent is where its first child would go.
                setDrop("after", index, 0, Qt.rect(0, top + row.height - 1, tree.width, 2));
            else
                setDrop("after", parentIndex, index.row + 1, Qt.rect(0, top + row.height - 1, tree.width, 2));
        } else {
            setDrop("child", index, notebook.rowCount(index), Qt.rect(0, top, tree.width, row.height));
        }
    }

    function endDrag() {
        const drop = dropKind;
        if (drop !== "") {
            // The result is a fresh index for the node; the old one may no
            // longer be valid, so select the node again from it.
            const moved = notebook.dropNode(dragIndex, dropParent, dropRow);
            if (moved.valid) {
                selection.setCurrentIndex(moved, ItemSelectionModel.ClearAndSelect);
                reveal(moved);
            }
        }
        dragging = false;
        dragIndex = null;
        dropParent = null;
        clearDrop();
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

            // Where the dragged row would land.
            Rectangle {
                parent: tree
                z: 100
                visible: pane.dragging && pane.dropKind !== ""
                x: pane.dropRect.x
                y: pane.dropRect.y
                width: pane.dropRect.width
                height: pane.dropRect.height
                color: pane.dropKind === "child" ? "transparent" : pane.palette.highlight
                border.width: pane.dropKind === "child" ? 2 : 0
                border.color: pane.palette.highlight
            }

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

                DragHandler {
                    acceptedButtons: Qt.LeftButton
                    // The row itself stays put; only the indicator moves.
                    target: null
                    onActiveChanged: {
                        if (active)
                            pane.beginDrag(item.treeView.index(item.row, item.column));
                        else
                            pane.endDrag();
                    }
                    onCentroidChanged: {
                        if (active)
                            pane.updateDrag(centroid.scenePosition);
                    }
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
            Item {
                Layout.fillWidth: true
            }
            // The only document controls on screen: one small menu.
            Button {
                id: fileButton

                text: qsTr("File")
                flat: true
                onClicked: fileMenu.popup(fileButton, 0, -fileMenu.implicitHeight)

                Menu {
                    id: fileMenu

                    MenuItem {
                        text: qsTr("Open…\tCtrl+O")
                        onTriggered: pane.openRequested()
                    }
                    MenuItem {
                        text: qsTr("Save\tCtrl+S")
                        onTriggered: pane.saveRequested()
                    }
                    MenuItem {
                        text: qsTr("Save As…\tCtrl+Shift+S")
                        onTriggered: pane.saveAsRequested()
                    }
                    MenuSeparator {}
                    MenuItem {
                        text: qsTr("Recovery…")
                        onTriggered: pane.recoveryRequested()
                    }
                }
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
