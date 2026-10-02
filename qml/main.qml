import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

ApplicationWindow {
    id: root

    width: 960
    height: 640
    minimumWidth: 480
    minimumHeight: 320
    visible: true
    title: qsTr("OmaTree")

    // The one and only notebook model, shared by every part of the UI.
    NotebookModel {
        id: notebook
    }

    // The selected note is selection.currentIndex. Nothing else holds an index.
    ItemSelectionModel {
        id: selection
        model: notebook
    }

    readonly property string defaultTitle: qsTr("New note")

    function select(index) {
        selection.setCurrentIndex(index, ItemSelectionModel.ClearAndSelect);
        treePane.reveal(index);
    }

    function createRoot() {
        const index = notebook.createRoot(defaultTitle);
        select(index);
        editorPane.focusTitle();
    }

    function createChild() {
        if (!selection.currentIndex.valid)
            return;
        const index = notebook.createChild(selection.currentIndex, defaultTitle);
        if (!index.valid)
            return;
        select(index);
        editorPane.focusTitle();
    }

    // `selection.currentIndex` is a live reference to the property, not a
    // copy: it changes (or becomes invalid) when the selection changes. Take
    // a real copy, via the model, before clearing the selection.
    function snapshot(index) {
        return notebook.index(index.row, index.column, notebook.parent(index));
    }

    function deleteSelected() {
        if (!selection.currentIndex.valid)
            return;
        const index = snapshot(selection.currentIndex);
        // Drop the selection first so no stale index survives the removal,
        // then fall back to the parent (if any).
        const parentIndex = notebook.parent(index);
        selection.clear();
        selection.clearCurrentIndex();
        notebook.removeNode(index);
        if (parentIndex.valid)
            select(parentIndex);
        treePane.focusTree();
    }

    Shortcut {
        sequence: "Ctrl+N"
        onActivated: root.createRoot()
    }
    Shortcut {
        sequence: "Ctrl+Shift+N"
        onActivated: root.createChild()
    }
    Shortcut {
        sequence: "F2"
        onActivated: if (selection.currentIndex.valid)
            editorPane.focusTitle()
    }

    SplitView {
        anchors.fill: parent

        handle: Item {
            implicitWidth: 7
            Rectangle {
                anchors.horizontalCenter: parent.horizontalCenter
                width: 1
                height: parent.height
                color: SplitHandle.pressed ? root.palette.highlight : root.palette.mid
            }
        }

        TreePane {
            id: treePane
            SplitView.preferredWidth: 260
            SplitView.minimumWidth: 160
            notebook: notebook
            selection: selection
            onNewRootRequested: root.createRoot()
            onNewChildRequested: root.createChild()
            onDeleteRequested: confirmDelete.askAboutSelection()
        }

        EditorPane {
            id: editorPane
            SplitView.fillWidth: true
            SplitView.minimumWidth: 240
            notebook: notebook
            selection: selection
            onDeleteRequested: confirmDelete.askAboutSelection()
        }
    }

    Dialog {
        id: confirmDelete

        function askAboutSelection() {
            if (!selection.currentIndex.valid)
                return;
            const index = selection.currentIndex;
            const name = notebook.data(index, Qt.DisplayRole);
            message.text = notebook.rowCount(index) > 0 ? qsTr("Delete “%1” and everything beneath it?").arg(name) : qsTr("Delete “%1”?").arg(name);
            open();
        }

        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 340
        modal: true
        title: qsTr("Delete note")
        standardButtons: Dialog.Yes | Dialog.No
        onAccepted: root.deleteSelected()

        contentItem: Label {
            id: message
            wrapMode: Text.Wrap
        }
    }
}
