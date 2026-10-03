import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

// Left pane: the note tree plus the two creation buttons.
Item {
    id: pane

    // The footer decides how narrow the pane can get without overflowing.
    implicitWidth: footer.implicitWidth + Ui.medium * 2
    clip: true

    required property var notebook
    required property ItemSelectionModel selection

    signal newRootRequested
    signal newChildRequested
    signal deleteRequested
    signal newNotebookRequested
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

    // --- Search ------------------------------------------------------------
    // While searching, the result list takes the tree's place. The tree, its
    // expansion and the selection stay exactly as they were underneath.

    property bool searching: false

    function startSearch() {
        searching = true;
        searchPane.open();
    }

    // Leaves search without touching the selection (Escape, or a note having
    // been chosen).
    function stopSearch() {
        if (!searching)
            return;
        searching = false;
        searchPane.reset();
        focusTree();
    }

    // The user picked result `index`. The note is looked up again now, as a
    // new index, and then shown in its real place in the tree.
    function openResult(index) {
        const found = notebook.activateSearchResult(index);
        if (!found.valid) {
            // The note is gone (or the list was stale): select nothing, and
            // bring the list up to date.
            searchPane.search(false);
            return;
        }
        stopSearch();
        selection.setCurrentIndex(found, ItemSelectionModel.ClearAndSelect);
        reveal(found);
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
    // Where a drop would go: "" (nowhere), "before", "after", "child", "end";
    // or "invalid" over a row that would refuse it (nothing happens on release).
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

    // `rowRect` is the whole row under the pointer, outlined when the drop
    // there would be refused (a cycle, or a name that already exists there).
    function setDrop(kind, parentIndex, row, rect, rowRect) {
        if (!notebook.canDrop(dragIndex, parentIndex)) {
            dropKind = "invalid";
            dropRect = rowRect;
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
            setDrop("end", rootIndex, notebook.rowCount(rootIndex), Qt.rect(0, bottom - 1, tree.width, 2), Qt.rect(0, bottom - 1, tree.width, 2));
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
        const rowRect = Qt.rect(0, top, tree.width, row.height);
        const parentIndex = notebook.parent(index);
        if (fraction < 0.25) {
            setDrop("before", parentIndex, index.row, Qt.rect(0, top - 1, tree.width, 2), rowRect);
        } else if (fraction > 0.75) {
            if (tree.isExpanded(cell.y) && notebook.rowCount(index) > 0)
                // Just under an open parent is where its first child would go.
                setDrop("after", index, 0, Qt.rect(0, top + row.height - 1, tree.width, 2), rowRect);
            else
                setDrop("after", parentIndex, index.row + 1, Qt.rect(0, top + row.height - 1, tree.width, 2), rowRect);
        } else {
            setDrop("child", index, notebook.rowCount(index), Qt.rect(0, top, tree.width, row.height), rowRect);
        }
    }

    function endDrag() {
        const drop = dropKind;
        if (drop !== "" && drop !== "invalid") {
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

    Rectangle {
        anchors.fill: parent
        color: Theme.surface
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        SearchPane {
            id: searchPane

            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: pane.searching
            notebook: pane.notebook
            onActivateRequested: index => pane.openResult(index)
            onCloseRequested: pane.stopSearch()
        }

        TreeView {
            id: tree

            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !pane.searching
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
                // A line for between rows, an outline for "as a child", and a
                // quiet danger outline where the drop would be refused.
                color: (pane.dropKind === "child" || pane.dropKind === "invalid") ? "transparent" : Theme.accent
                border.width: (pane.dropKind === "child" || pane.dropKind === "invalid") ? 2 : 0
                border.color: pane.dropKind === "invalid" ? Theme.danger : Theme.accent
            }

            // Keyboard focus: a thin accent edge along the top of the tree,
            // and the selected row's bar turning accent (see the delegate).
            Rectangle {
                parent: tree
                z: 90
                width: tree.width
                height: Ui.hairline
                color: Theme.accent
                visible: tree.activeFocus
            }

            delegate: TreeViewDelegate {
                id: item

                // A compact fixed row, whatever the control style would choose.
                implicitHeight: Ui.rowHeight
                leftMargin: Ui.medium
                rightMargin: Ui.medium

                // The current note: a quiet tinted row with a thin bar on its
                // left edge, which is accent while the tree has the keyboard
                // and a hairline colour otherwise. Hover is weaker still.
                background: Rectangle {
                    color: item.current ? Qt.alpha(Theme.selection, 0.55) : (item.hovered ? Qt.alpha(Theme.surfaceRaised, 0.8) : "transparent")

                    Behavior on color {
                        ColorAnimation {
                            duration: Ui.hoverDuration
                        }
                    }

                    Rectangle {
                        width: Ui.bar
                        height: parent.height
                        color: item.treeView.activeFocus ? Theme.accent : Theme.border
                        visible: item.current
                    }
                }

                // A small disclosure marker. Its box is a full row tall so it
                // is easy to hit; the template toggles the row when it is clicked.
                indicator: Item {
                    x: item.leftMargin + item.depth * item.indentation
                    y: (item.height - height) / 2
                    implicitWidth: Ui.indent + Ui.small
                    implicitHeight: Ui.rowHeight
                    visible: item.isTreeNode && item.hasChildren

                    Label {
                        anchors.centerIn: parent
                        text: item.expanded ? "▾" : "▸"
                        font.pixelSize: 14
                        color: item.current ? Theme.accent : Theme.mutedForeground
                    }
                }

                contentItem: Label {
                    text: item.model.display
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                    color: item.current ? Theme.foreground : Theme.foreground
                    font.weight: item.current ? Font.DemiBold : Font.Normal
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

        Rectangle {
            Layout.fillWidth: true
            implicitHeight: Ui.hairline
            color: Theme.border
        }

        // The command strip: quiet fixed-width commands instead of buttons.
        RowLayout {
            id: footer

            Layout.fillWidth: true
            Layout.preferredHeight: Ui.controlHeight
            Layout.leftMargin: Ui.small
            Layout.rightMargin: Ui.small
            spacing: 0

            Command {
                text: qsTr("+ note")
                hint: qsTr("New note   Ctrl+N")
                onClicked: pane.newRootRequested()
            }
            Command {
                text: qsTr("+ child")
                hint: qsTr("New child note   Ctrl+Shift+N")
                enabled: pane.selection.currentIndex.valid
                onClicked: pane.newChildRequested()
            }
            Item {
                Layout.fillWidth: true
            }
            Command {
                text: qsTr("search")
                hint: qsTr("Search notes   Ctrl+F")
                onClicked: pane.startSearch()
            }
            // The only document controls on screen: one small menu.
            Command {
                id: fileButton

                text: qsTr("file")
                onClicked: fileMenu.popup(fileButton, 0, -fileMenu.implicitHeight - Ui.small)

                Menu {
                    id: fileMenu

                    padding: Ui.small
                    background: Rectangle {
                        implicitWidth: 230
                        color: Theme.surfaceRaised
                        border.width: Ui.hairline
                        border.color: Theme.border
                        radius: Ui.radius
                    }

                    CommandMenuItem {
                        text: qsTr("New Notebook")
                        onTriggered: pane.newNotebookRequested()
                    }
                    CommandMenuItem {
                        text: qsTr("Open…\tCtrl+O")
                        onTriggered: pane.openRequested()
                    }
                    CommandMenuItem {
                        text: qsTr("Save\tCtrl+S")
                        onTriggered: pane.saveRequested()
                    }
                    CommandMenuItem {
                        text: qsTr("Save As…\tCtrl+Shift+S")
                        onTriggered: pane.saveAsRequested()
                    }
                    MenuSeparator {
                        padding: Ui.small
                        contentItem: Rectangle {
                            implicitHeight: Ui.hairline
                            color: Theme.border
                        }
                    }
                    CommandMenuItem {
                        text: qsTr("Recovery…")
                        onTriggered: pane.recoveryRequested()
                    }
                }
            }
        }
    }

    // A menu row: the label, and a muted fixed-width shortcut hint after a
    // tab in its text. The highlighted row gets a tint and an accent edge.
    component CommandMenuItem: MenuItem {
        id: row

        readonly property var parts: text.split("\t")

        implicitHeight: Ui.controlHeight - Ui.small
        leftPadding: Ui.medium
        rightPadding: Ui.medium

        contentItem: RowLayout {
            spacing: Ui.large

            Label {
                Layout.fillWidth: true
                text: row.parts[0]
                color: Theme.foreground
                elide: Text.ElideRight
                verticalAlignment: Text.AlignVCenter
            }
            Label {
                visible: row.parts.length > 1
                text: row.parts.length > 1 ? row.parts[1] : ""
                color: Theme.mutedForeground
                font.family: Ui.monoFamily
                font.pixelSize: Ui.commandPixelSize
                verticalAlignment: Text.AlignVCenter
            }
        }
        background: Rectangle {
            color: row.highlighted ? Qt.alpha(Theme.selection, 0.55) : "transparent"

            Rectangle {
                width: Ui.bar
                height: parent.height
                color: Theme.accent
                visible: row.highlighted
            }
        }
    }

    // Shown instead of an empty-looking tree.
    ColumnLayout {
        anchors.centerIn: parent
        visible: tree.rows === 0 && !pane.searching
        spacing: Ui.small

        Label {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("No notes yet")
            color: Theme.mutedForeground
        }
        Command {
            Layout.alignment: Qt.AlignHCenter
            text: qsTr("+ create the first note")
            onClicked: pane.newRootRequested()
        }
    }
}
