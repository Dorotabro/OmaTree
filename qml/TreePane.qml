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
    // Enter on the current note: start editing it.
    signal editRequested
    signal newChildRequested
    signal deleteRequested
    signal newNotebookRequested
    signal openRequested
    signal saveRequested
    signal saveAsRequested
    signal recoveryRequested
    signal shortcutsRequested

    // --- Expansion ---------------------------------------------------------
    // The TreeView owns what is open on screen. The model (that is, the
    // document) remembers which notes are open, so it can be saved with the
    // notebook and restored. This is view state only: it never marks the
    // notebook changed. `syncing` is set while this file itself opens or
    // closes rows, so the TreeView's own expanded/collapsed signals are not
    // mistaken for the user doing it.

    property bool syncing: false

    // Opens the rows the document says are open (after a notebook was
    // loaded or replaced, or rows were inserted or moved). A row opened here
    // brings its own children into the list, which the same pass then meets.
    function restoreExpansion() {
        syncing = true;
        // (Not `tree.rows`: after a reset it only catches up once the view
        // has laid itself out, while `index()` already follows the model.)
        for (let row = 0;; ++row) {
            const index = tree.index(row, 0);
            if (!index.valid)
                break;
            if (!tree.isExpanded(row) && notebook.rowCount(index) > 0 && notebook.shouldExpand(index))
                tree.expand(row);
        }
        syncing = false;
    }

    // After rows were closed, the current note may have been hidden inside
    // them; the nearest visible ancestor becomes current instead.
    function keepSelectionVisible() {
        let index = selection.currentIndex;
        if (!index.valid || tree.rowAtIndex(index) >= 0)
            return;
        while (index.valid && tree.rowAtIndex(index) < 0)
            index = notebook.parent(index);
        if (index.valid)
            selection.setCurrentIndex(index, ItemSelectionModel.ClearAndSelect);
    }

    // The recursive operations use TreeView's own: nothing here walks the
    // tree. The document is told the same thing, for it to remember.
    function expandSubtree(row) {
        if (row < 0)
            return;
        syncing = true;
        tree.expandRecursively(row);
        syncing = false;
        notebook.setSubtreeExpanded(tree.index(row, 0), true);
    }

    function collapseSubtree(row) {
        if (row < 0)
            return;
        const index = tree.index(row, 0);
        syncing = true;
        tree.collapseRecursively(row);
        syncing = false;
        notebook.setSubtreeExpanded(index, false);
        keepSelectionVisible();
    }

    function expandAll() {
        syncing = true;
        tree.expandRecursively();
        syncing = false;
        notebook.setAllExpanded(true);
    }

    function collapseAll() {
        syncing = true;
        tree.collapseRecursively();
        syncing = false;
        notebook.setAllExpanded(false);
        keepSelectionVisible();
    }

    // Ctrl+Right / Ctrl+Left: the current note's whole subtree.
    function currentRow() {
        return selection.currentIndex.valid ? tree.rowAtIndex(selection.currentIndex) : -1;
    }

    Connections {
        target: pane.notebook
        // A different notebook, or rows that came back or moved: open again
        // whatever the document remembers as open.
        function onModelReset() {
            Qt.callLater(pane.restoreExpansion);
        }
        function onRowsInserted() {
            Qt.callLater(pane.restoreExpansion);
        }
        function onRowsMoved() {
            Qt.callLater(pane.restoreExpansion);
        }
    }

    // Expands ancestors of `index` and scrolls it into view. They stay open
    // (and are remembered as open) afterwards.
    function reveal(index) {
        notebook.expandAncestors(index);
        syncing = true;
        tree.expandToIndex(index);
        syncing = false;
        // Rows that just came into view (a moved subtree under a path that
        // was closed) open as the document remembers, and no further.
        restoreExpansion();
        Qt.callLater(() => {
            const row = tree.rowAtIndex(index);
            if (row >= 0)
                tree.positionViewAtRow(row, TableView.Contain);
        });
    }

    // The tree has the keyboard to start with; Ctrl+F or a click moves it.
    Component.onCompleted: focusTree()

    function focusTree() {
        tree.forceActiveFocus();
    }

    // --- Search ------------------------------------------------------------
    // The search field is always at the top of the pane. While it has a query
    // the result list takes the tree's place below it; the tree, its
    // expansion and the selection stay exactly as they were underneath.

    readonly property bool searching: searchPane.active

    // Ctrl+F: the field, with the last query selected.
    function startSearch() {
        searchField.focusAll();
    }

    // Empties the query, which brings the tree back. Touches no selection.
    function stopSearch() {
        searchField.text = "";
        searchPane.reset();
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
        focusTree();
    }

    // A different notebook (New, Open, a restored checkpoint) starts without
    // the old query.
    Connections {
        target: pane.notebook
        function onModelAboutToBeReset() {
            pane.stopSearch();
        }
    }

    // --- Context menu ------------------------------------------------------
    // Right-clicking a note makes it the current one, so every action in the
    // menu is about that note. Over empty space only the tree-wide actions
    // are available.

    property int contextRow: -1
    // Whether the note under the pointer has anything below it.
    property bool contextBranch: false

    function openContextMenu(position) {
        const cell = tree.cellAtPosition(position.x, position.y, true);
        showContextMenu(cell.y, position.x, position.y);
    }

    // Shift+F10 and the Menu key: the menu for the current note, just below
    // its row, or for the tree alone if no note is current.
    function openContextMenuFromKeyboard() {
        const row = currentRow();
        if (row < 0) {
            showContextMenu(-1, Ui.large, Ui.large);
            return;
        }
        tree.positionViewAtRow(row, TableView.Contain);
        const item = tree.itemAtCell(Qt.point(0, row));
        const y = item ? item.mapToItem(tree, 0, item.height).y : Ui.large;
        showContextMenu(row, Ui.large * 3, y);
    }

    function showContextMenu(row, x, y) {
        contextRow = row;
        contextBranch = row >= 0 && notebook.rowCount(tree.index(row, 0)) > 0;
        if (row >= 0) {
            selection.setCurrentIndex(tree.index(row, 0), ItemSelectionModel.ClearAndSelect);
            tree.forceActiveFocus();
        }
        contextMenu.popup(tree, x, y);
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

        SearchField {
            id: searchField

            Layout.fillWidth: true
            onMoveRequested: step => searchPane.moveBy(step)
            onActivateRequested: searchPane.activateCurrent()
            // Escape clears a query; with none, it returns to the tree.
            onEscapeRequested: {
                if (pane.searching)
                    pane.stopSearch();
                else
                    pane.focusTree();
            }
        }

        SearchPane {
            id: searchPane

            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: pane.searching
            notebook: pane.notebook
            query: searchField.text
            onActivateRequested: index => pane.openResult(index)
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
            Keys.onPressed: event => {
                const ctrlOnly = (event.modifiers & Qt.ControlModifier) && !(event.modifiers & (Qt.ShiftModifier | Qt.AltModifier));
                if (ctrlOnly && event.key === Qt.Key_Right) {
                    pane.expandSubtree(pane.currentRow());
                    event.accepted = true;
                } else if (ctrlOnly && event.key === Qt.Key_Left) {
                    pane.collapseSubtree(pane.currentRow());
                    event.accepted = true;
                } else if (event.key === Qt.Key_Tab || event.key === Qt.Key_Backtab) {
                    // The view would move between its own cells (there is
                    // only one column); Tab must leave it instead.
                    const forward = event.key === Qt.Key_Tab && !(event.modifiers & Qt.ShiftModifier);
                    const next = tree.nextItemInFocusChain(forward);
                    if (next && next !== tree)
                        next.forceActiveFocus(forward ? Qt.TabFocusReason : Qt.BacktabFocusReason);
                    event.accepted = true;
                } else if ((event.key === Qt.Key_Return || event.key === Qt.Key_Enter) && !event.modifiers && pane.selection.currentIndex.valid) {
                    pane.editRequested();
                    event.accepted = true;
                } else if (event.key === Qt.Key_Menu || (event.key === Qt.Key_F10 && event.modifiers === Qt.ShiftModifier)) {
                    pane.openContextMenuFromKeyboard();
                    event.accepted = true;
                }
            }

            // What the user did to a row, by plain click or key: remembered
            // by the document. (Ctrl+click and Ctrl+arrows are recursive and
            // are handled by the row and the keys above.)
            onExpanded: (row, depth) => {
                if (pane.syncing)
                    return;
                pane.notebook.setExpanded(tree.index(row, 0), true);
                // Children that come into view keep their remembered state.
                Qt.callLater(pane.restoreExpansion);
            }
            onCollapsed: (row, recursively) => {
                if (!pane.syncing)
                    pane.notebook.setExpanded(tree.index(row, 0), false);
            }

            TapHandler {
                acceptedButtons: Qt.RightButton
                onTapped: eventPoint => pane.openContextMenu(eventPoint.position)
            }

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

            // Keyboard focus is the selected row's bar turning accent (see the
            // delegate). It used to be an accent line along the top of the
            // tree, which would now sit against the search header's separator.

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
                    color: item.current ? Qt.alpha(Theme.selection, 0.55) : (rowHover.over ? Qt.alpha(Theme.surfaceRaised, 0.8) : "transparent")

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

                // One narrow column per level: the disclosure marker of a
                // note sits in its own column, and the guide lines below run
                // through the middle of the columns to its left.
                indentation: Ui.indent

                // What the guides need to know about this row, asked of the
                // model again whenever the shape of the tree changes.
                readonly property bool lastSibling: {
                    item.treeView.model.structureRevision;
                    return item.treeView.model.isLastSibling(item.treeView.index(item.row, item.column));
                }
                readonly property int guideColumns: {
                    item.treeView.model.structureRevision;
                    return item.treeView.model.guideColumns(item.treeView.index(item.row, item.column));
                }

                // Branch guides: continuing lines of the ancestors that have
                // more siblings below, then this note's own elbow, which
                // stops at its middle if it is the last sibling. Plain
                // geometry in the border colour, a little stronger (towards
                // the accent) on the current row.
                Item {
                    id: guides

                    readonly property color line: item.current ? Qt.tint(Theme.border, Qt.alpha(Theme.accent, 0.5)) : Theme.border
                    // The middle of the column that belongs to `level`.
                    function columnX(level) {
                        return item.leftMargin + level * Ui.indent + Math.floor(Ui.indent / 2);
                    }

                    anchors.fill: parent
                    visible: item.isTreeNode && item.depth > 0

                    Repeater {
                        model: Math.max(0, item.depth - 1)

                        Rectangle {
                            required property int index

                            visible: ((item.guideColumns >> index) & 1) === 1
                            x: guides.columnX(index)
                            width: Ui.hairline
                            height: item.height
                            color: guides.line
                        }
                    }
                    // The elbow: down from the row's top (to the bottom too,
                    // unless this is the last sibling) and across to the note.
                    Rectangle {
                        x: guides.columnX(item.depth - 1)
                        width: Ui.hairline
                        height: item.lastSibling ? Math.ceil(item.height / 2) : item.height
                        color: guides.line
                    }
                    Rectangle {
                        x: guides.columnX(item.depth - 1)
                        y: Math.floor(item.height / 2)
                        // Short of a disclosure marker; all the way to the
                        // text for a note with nothing below it.
                        width: item.hasChildren ? Math.floor(Ui.indent / 2) - 1 : Ui.indent + Ui.small
                        height: Ui.hairline
                        color: guides.line
                    }
                }

                // A small disclosure marker. Its box is a full row tall so it
                // is easy to hit; the template toggles the row when it is clicked.
                // It is the only mark that says whether a note has anything
                // below it; a note without children has none.
                indicator: Item {
                    x: item.leftMargin + item.depth * Ui.indent
                    y: (item.height - height) / 2
                    implicitWidth: Ui.indent
                    implicitHeight: Ui.rowHeight
                    visible: item.isTreeNode && item.hasChildren

                    Label {
                        anchors.centerIn: parent
                        text: item.expanded ? "▾" : "▸"
                        font.pixelSize: 21
                        // Closed branches hide something, so they are the
                        // brighter marker; open ones recede into the guides.
                        color: item.current ? Theme.accent : (item.expanded ? Theme.mutedForeground : Theme.foreground)
                    }
                }

                contentItem: Label {
                    text: item.model.display
                    elide: Text.ElideRight
                    verticalAlignment: Text.AlignVCenter
                    color: item.current ? Theme.foreground : Theme.foreground
                    font.weight: item.current ? Font.DemiBold : Font.Normal
                }

                // The row's own hover, which cannot go stale behind a popup.
                PointerHover {
                    id: rowHover
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

                // Ctrl+click on the disclosure marker: the whole subtree, by
                // TreeView's own recursive operations. Only sees clicks made
                // with Ctrl held, whatever the template itself did with them.
                TapHandler {
                    property bool wasExpanded: false

                    acceptedModifiers: Qt.ControlModifier
                    gesturePolicy: TapHandler.ReleaseWithinBounds
                    onPressedChanged: {
                        if (pressed)
                            wasExpanded = item.expanded;
                    }
                    onTapped: eventPoint => {
                        const left = item.leftMargin + item.depth * Ui.indent;
                        if (!item.hasChildren || eventPoint.position.x < left || eventPoint.position.x >= left + Ui.indent)
                            return;
                        if (wasExpanded)
                            pane.collapseSubtree(item.row);
                        else
                            pane.expandSubtree(item.row);
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
                glyph: "+"
                text: qsTr("note")
                hint: qsTr("New note   %1").arg(Keymap.text(Keymap.newNote))
                onClicked: pane.newRootRequested()
            }
            Command {
                glyph: "+"
                text: qsTr("child")
                hint: qsTr("New child note   %1").arg(Keymap.text(Keymap.newChild))
                enabled: pane.selection.currentIndex.valid
                onClicked: pane.newChildRequested()
            }
            Item {
                Layout.fillWidth: true
            }
            // The only document controls on screen: one small menu.
            Command {
                id: fileButton

                text: qsTr("file")
                tone: Qt.alpha(Theme.accentSecondary, 0.85)
                onClicked: fileMenu.popup(fileButton, 0, -fileMenu.implicitHeight - Ui.small)

                CommandMenu {
                    id: fileMenu

                    CommandMenuItem {
                        text: qsTr("New Notebook")
                        onTriggered: pane.newNotebookRequested()
                    }
                    CommandMenuItem {
                        text: qsTr("Open…\t%1").arg(Keymap.text(Keymap.open))
                        onTriggered: pane.openRequested()
                    }
                    CommandMenuItem {
                        text: qsTr("Save\t%1").arg(Keymap.text(Keymap.save))
                        onTriggered: pane.saveRequested()
                    }
                    CommandMenuItem {
                        text: qsTr("Save As…\t%1").arg(Keymap.text(Keymap.saveAs))
                        onTriggered: pane.saveAsRequested()
                    }
                    CommandMenuSeparator {}
                    CommandMenuItem {
                        text: qsTr("Recovery…")
                        onTriggered: pane.recoveryRequested()
                    }
                    CommandMenuSeparator {}
                    CommandMenuItem {
                        text: qsTr("Keyboard Shortcuts\t%1").arg(Keymap.text(Keymap.help))
                        onTriggered: pane.shortcutsRequested()
                    }
                }
            }
        }
    }

    // The tree's context menu, in the same command-menu language as File.
    CommandMenu {
        id: contextMenu

        readonly property bool onNote: pane.contextRow >= 0
        readonly property bool branch: pane.contextBranch

        CommandMenuItem {
            text: qsTr("New note\t%1").arg(Keymap.text(Keymap.newNote))
            onTriggered: pane.newRootRequested()
        }
        CommandMenuItem {
            text: qsTr("New child\t%1").arg(Keymap.text(Keymap.newChild))
            enabled: contextMenu.onNote
            onTriggered: pane.newChildRequested()
        }
        CommandMenuSeparator {}
        CommandMenuItem {
            text: qsTr("Expand subtree\t%1").arg(Keymap.text(["Ctrl+Right"]))
            enabled: contextMenu.branch
            onTriggered: pane.expandSubtree(pane.contextRow)
        }
        CommandMenuItem {
            text: qsTr("Collapse subtree\t%1").arg(Keymap.text(["Ctrl+Left"]))
            enabled: contextMenu.branch
            onTriggered: pane.collapseSubtree(pane.contextRow)
        }
        CommandMenuItem {
            text: qsTr("Expand all")
            enabled: tree.rows > 0
            onTriggered: pane.expandAll()
        }
        CommandMenuItem {
            text: qsTr("Collapse all")
            enabled: tree.rows > 0
            onTriggered: pane.collapseAll()
        }
        CommandMenuSeparator {}
        CommandMenuItem {
            text: qsTr("Move to Trash\tDelete")
            danger: true
            enabled: contextMenu.onNote
            onTriggered: pane.deleteRequested()
        }
    }

    // A popup of commands: flat, raised a little, with a hairline border.
    component CommandMenu: Menu {
        padding: Ui.small
        // See PointerHover.
        onAboutToShow: Ui.popupSerial++
        onClosed: Ui.popupSerial++
        background: Rectangle {
            implicitWidth: 230
            color: Theme.surfaceRaised
            border.width: Ui.hairline
            border.color: Theme.border
            radius: Ui.radius
        }
    }

    component CommandMenuSeparator: MenuSeparator {
        padding: Ui.small
        contentItem: Rectangle {
            implicitHeight: Ui.hairline
            color: Theme.border
        }
    }

    // A menu row: the label, and a muted fixed-width shortcut hint after a
    // tab in its text. The highlighted row gets a tint and an accent edge
    // (red for a destructive command, which is red at rest as well).
    component CommandMenuItem: MenuItem {
        id: row

        property bool danger: false
        readonly property var parts: text.split("\t")

        implicitHeight: Ui.controlHeight - Ui.small
        leftPadding: Ui.medium
        rightPadding: Ui.medium

        contentItem: RowLayout {
            spacing: Ui.large
            opacity: row.enabled ? 1 : 0.4

            Label {
                Layout.fillWidth: true
                text: row.parts[0]
                color: row.danger ? Theme.danger : Theme.foreground
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
            color: row.highlighted ? Qt.alpha(row.danger ? Theme.danger : Theme.selection, 0.55) : "transparent"

            Rectangle {
                width: Ui.bar
                height: parent.height
                color: row.danger ? Theme.danger : Theme.accent
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
            glyph: "+"
            text: qsTr("create the first note")
            onClicked: pane.newRootRequested()
        }
    }
}
