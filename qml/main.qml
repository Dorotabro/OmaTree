import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
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

    // --- Document workflow -------------------------------------------------
    // The document only changes (path, contents, dirty) inside the model, and
    // only after an Open or Save As has fully succeeded. QML just sequences
    // the dialogs. A "continuation" is a function to run once the step before
    // it has succeeded; it is dropped on cancel or failure.

    // What to do after a pending Save As succeeds (or null).
    property var afterSaveAs: null

    function runContinuation(then) {
        if (then)
            then();
    }

    // Ctrl+S: save in place, or Save As if the notebook has no file yet.
    function saveCurrent() {
        if (notebook.hasPath())
            save();
        else
            startSaveAs(null);
    }

    function startSaveAs(then) {
        afterSaveAs = then;
        saveDialog.open();
    }

    function finishSaveAs(path, overwrite) {
        const error = notebook.saveAs(path, overwrite);
        const then = afterSaveAs;
        afterSaveAs = null;
        if (error !== "")
            showError(error);
        else
            runContinuation(then);
    }

    // Runs `then` straight away if nothing would be lost, otherwise asks the
    // user first (Save / Discard / Cancel).
    function whenSafeToLeave(then) {
        if (notebook.dirty)
            confirmUnsaved.ask(then);
        else
            then();
    }

    function requestOpen() {
        whenSafeToLeave(() => openDialog.open());
    }

    function requestClose() {
        whenSafeToLeave(() => {
            root.discardOnClose = true;
            root.close();
        });
    }

    Component.onCompleted: openStartupPath()

    onClosing: close => {
        if (notebook.dirty && !discardOnClose) {
            close.accepted = false;
            requestClose();
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
        onActivated: root.saveCurrent()
    }
    Shortcut {
        sequence: "Ctrl+Shift+S"
        onActivated: root.startSaveAs(null)
    }
    Shortcut {
        sequence: "Ctrl+O"
        onActivated: root.requestOpen()
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
            onOpenRequested: root.requestOpen()
            onSaveRequested: root.saveCurrent()
            onSaveAsRequested: root.startSaveAs(null)
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

    // Shown before anything that would drop unsaved changes (Open, close).
    Dialog {
        id: confirmUnsaved

        // Run after the user saves or discards.
        property var continuation: null

        function ask(then) {
            continuation = then;
            unsavedText.text = notebook.hasPath() ? qsTr("“%1” has unsaved changes.").arg(notebook.documentName) : qsTr("This notebook has not been saved yet.");
            open();
        }

        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 340
        modal: true
        title: qsTr("Unsaved changes")
        standardButtons: Dialog.Save | Dialog.Discard | Dialog.Cancel
        // The Save button reads "Save As…" for a notebook with no file, and
        // Discard is not "Close without Saving", since it is also used by Open.
        onAboutToShow: {
            const save = standardButton(Dialog.Save);
            if (save)
                save.text = notebook.hasPath() ? qsTr("Save") : qsTr("Save As…");
            const discard = standardButton(Dialog.Discard);
            if (discard)
                discard.text = qsTr("Discard");
        }
        // Save: carry on only if the save worked. A failed or cancelled save
        // keeps the current notebook and drops the pending action.
        onAccepted: {
            const then = continuation;
            continuation = null;
            if (notebook.hasPath()) {
                if (root.save())
                    root.runContinuation(then);
            } else {
                root.startSaveAs(then);
            }
        }
        // Discard only lets the pending action proceed; the notebook itself
        // is not touched here.
        onDiscarded: {
            const then = continuation;
            continuation = null;
            root.runContinuation(then);
        }
        onRejected: continuation = null

        contentItem: Label {
            id: unsavedText
            wrapMode: Text.Wrap
        }
    }

    Dialog {
        id: confirmOverwrite

        property string path: ""

        function ask(target) {
            path = target;
            overwriteText.text = qsTr("“%1” already exists. Replace it with this notebook?").arg(target.split("/").pop());
            open();
        }

        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 340
        modal: true
        title: qsTr("Replace file")
        standardButtons: Dialog.Yes | Dialog.No
        onAccepted: root.finishSaveAs(path, true)
        onRejected: root.afterSaveAs = null

        contentItem: Label {
            id: overwriteText
            wrapMode: Text.Wrap
        }
    }

    FileDialog {
        id: openDialog

        title: qsTr("Open notebook")
        fileMode: FileDialog.OpenFile
        nameFilters: [qsTr("OmaTree notebooks (*.omatree)"), qsTr("All files (*)")]
        onAccepted: {
            const error = notebook.openFile(selectedFile);
            if (error !== "")
                root.showError(error);
        }
    }

    FileDialog {
        id: saveDialog

        title: qsTr("Save notebook as")
        fileMode: FileDialog.SaveFile
        nameFilters: [qsTr("OmaTree notebooks (*.omatree)"), qsTr("All files (*)")]
        // We confirm overwrites ourselves, after `.omatree` has been appended.
        options: FileDialog.DontConfirmOverwrite
        onAccepted: {
            const path = notebook.saveAsTarget(selectedFile);
            if (path === "") {
                root.afterSaveAs = null;
                root.showError(qsTr("Please choose a file on this computer."));
            } else if (notebook.needsOverwriteConfirmation(path)) {
                confirmOverwrite.ask(path);
            } else {
                root.finishSaveAs(path, false);
            }
        }
        onRejected: root.afterSaveAs = null
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
