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
    title: qsTr("OmaTree — %1%2").arg(notebook.documentName).arg(notebook.dirty ? " *" : "")

    // The one and only notebook model, shared by every part of the UI.
    NotebookModel {
        id: notebook
    }

    // The selected note is selection.currentIndex. Nothing else holds an index.
    ItemSelectionModel {
        id: selection
        model: notebook
    }

    // Replacing the whole notebook resets the model. The selection model
    // would drop its index silently on reset, leaving the editor showing the
    // old note, so clear it explicitly while the old index is still valid.
    Connections {
        target: notebook
        function onModelAboutToBeReset() {
            selection.clear();
            selection.clearCurrentIndex();
        }
    }

    readonly property string defaultTitle: qsTr("New note")

    // Set once the user has chosen to throw away unsaved changes on close.
    property bool discardOnClose: false

    function showError(text) {
        errorMessage.text = text;
        errorDialog.open();
    }

    // Optional notebook path from the command line: `omatree [path]`.
    function openStartupPath() {
        const args = Qt.application.arguments.slice(1);
        if (args.length > 1) {
            showError(qsTr("OmaTree takes at most one notebook path."));
            return;
        }
        if (args.length === 1) {
            const error = notebook.openPath(args[0]);
            if (error !== "")
                showError(error);
        }
    }

    function save() {
        const error = notebook.save();
        if (error !== "")
            showError(error);
        return error === "";
    }

    Component.onCompleted: openStartupPath()

    onClosing: close => {
        if (notebook.dirty && !discardOnClose) {
            close.accepted = false;
            confirmClose.ask();
        }
    }

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
        sequence: "Ctrl+S"
        onActivated: root.save()
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

    Dialog {
        id: confirmClose

        function ask() {
            const name = notebook.documentName;
            if (notebook.hasPath()) {
                message2.text = qsTr("“%1” has unsaved changes.").arg(name);
                standardButtons = Dialog.Save | Dialog.Discard | Dialog.Cancel;
            } else {
                message2.text = qsTr("This notebook has no file, so its changes can't be saved.");
                standardButtons = Dialog.Discard | Dialog.Cancel;
            }
            open();
        }

        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 340
        modal: true
        title: qsTr("Unsaved changes")
        // Save closes only if the save worked; a failure shows the error and
        // keeps the window open.
        onAccepted: {
            if (root.save())
                root.close();
        }
        onDiscarded: {
            root.discardOnClose = true;
            root.close();
        }

        contentItem: Label {
            id: message2
            wrapMode: Text.Wrap
        }
    }

    Dialog {
        id: errorDialog

        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 340
        modal: true
        title: qsTr("OmaTree")
        standardButtons: Dialog.Ok

        contentItem: Label {
            id: errorMessage
            wrapMode: Text.Wrap
        }
    }
}
