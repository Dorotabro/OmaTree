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
    color: Theme.background
    // The semantic palette, handed to the stock controls so everything drawn
    // by Qt (buttons, menus, text selection, scroll bars) follows it too.
    palette {
        window: Theme.background
        windowText: Theme.foreground
        base: Theme.background
        text: Theme.foreground
        button: Theme.surfaceRaised
        buttonText: Theme.foreground
        highlight: Theme.selection
        highlightedText: Theme.selectionForeground
        brightText: Theme.dangerForeground
        mid: Theme.border
        midlight: Theme.border
        dark: Theme.border
        light: Theme.surfaceRaised
        shadow: Theme.border
        placeholderText: Theme.mutedForeground
        link: Theme.accent
        toolTipBase: Theme.surfaceRaised
        toolTipText: Theme.foreground
    }
    title: qsTr("OmaTree — %1%2").arg(notebook.documentName).arg(notebook.dirty ? " *" : "")

    // Theme inputs. `Theme` owns the palette; this only tells it what the
    // system's colour scheme is and gives it a slow clock to notice a changed
    // Omarchy theme. Neither touches the notebook.
    Connections {
        target: Qt.styleHints
        function onColorSchemeChanged() {
            Theme.setSystemScheme(Qt.styleHints.colorScheme);
        }
    }
    Timer {
        interval: 500
        repeat: true
        running: Theme.watchesOmarchy()
        onTriggered: Theme.poll()
    }

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
        // Every real change restarts the autosave countdown. (Not `dirty`:
        // it stays true while a user keeps typing.) Untitled notebooks have
        // no file to save to, so nothing is scheduled for them.
        function onDocumentMutated() {
            if (notebook.hasPath())
                autosaveTimer.restart();
        }
    }

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
            else
                warnAboutDuplicateTitles();
        }
    }

    // Notebooks written before sibling names had to be unique may contain
    // duplicates. Nothing is changed for the user; they are told once, right
    // after a notebook is opened or an old checkpoint or Trash entry is
    // restored, and not again while they edit.
    function warnAboutDuplicateTitles() {
        if (notebook.titleConflictCount() > 0)
            showError(qsTr("This notebook contains duplicate note names under the same parent.\nNothing was changed. Rename the duplicates to resolve them.\nOmaTree will prevent new duplicates."));
    }

    // An explicit save (Ctrl+S, the Save button of a dialog): a failure is
    // reported, because the user asked for it.
    function save() {
        const error = notebook.save();
        if (error !== "") {
            showError(error);
            return false;
        }
        autosaveFailureShown = false;
        return true;
    }

    // --- Autosave ----------------------------------------------------------
    // One timer for the open document. When it expires the normal save runs;
    // Document::save decides whether that is an active-only or a full save.
    // Success is silent (the "*" leaves the title). The first failure is
    // reported once; later failures stay quiet until something succeeds.

    // The first autosave failure of this document has been shown.
    property bool autosaveFailureShown: false

    Timer {
        id: autosaveTimer

        interval: 1000
        repeat: false
        onTriggered: root.autosave()
    }

    function autosave() {
        if (!notebook.hasPath() || !notebook.dirty)
            return;
        const error = notebook.save();
        if (error === "") {
            autosaveFailureShown = false;
        } else if (!autosaveFailureShown) {
            autosaveFailureShown = true;
            showError(qsTr("Autosave failed: %1 Your changes are still open in OmaTree. Press Ctrl+S to try again.").arg(error));
        }
    }

    // After something that paused autosave was cancelled or failed.
    function resumeAutosave() {
        if (notebook.hasPath() && notebook.dirty)
            autosaveTimer.restart();
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
        if (notebook.hasPath()) {
            autosaveTimer.stop();
            save();
        } else {
            startSaveAs(null);
        }
    }

    function startSaveAs(then) {
        autosaveTimer.stop();
        afterSaveAs = then;
        saveDialog.open();
    }

    function finishSaveAs(path, overwrite) {
        const error = notebook.saveAs(path, overwrite);
        const then = afterSaveAs;
        afterSaveAs = null;
        if (error !== "") {
            showError(error);
        } else {
            autosaveFailureShown = false;
            runContinuation(then);
        }
    }

    // Before leaving a file-backed notebook: stop the countdown and save any
    // pending changes right now, without any message. True if nothing is
    // left unsaved. A failure is not reported here; the caller falls back to
    // asking the user.
    function flushPendingChanges() {
        if (!notebook.dirty)
            return true;
        if (!notebook.hasPath())
            return false;
        autosaveTimer.stop();
        if (notebook.save() !== "")
            return false;
        autosaveFailureShown = false;
        return true;
    }

    // Runs `then` straight away if nothing would be lost. A file-backed
    // notebook is saved first (quietly); only if that fails, or for an
    // untitled notebook, the user is asked (Save / Discard / Cancel).
    function whenSafeToLeave(then) {
        if (flushPendingChanges())
            then();
        else
            confirmUnsaved.ask(then);
    }

    function requestOpen() {
        whenSafeToLeave(() => openDialog.open());
    }

    // File ▸ New Notebook: the same "may I leave this document?" check as
    // Open and Close, then a fresh untitled document in place of this one.
    function requestNewNotebook() {
        whenSafeToLeave(() => {
            // Out of Search first, so the new document inherits nothing from
            // the old one; the model then resets and clears the selection.
            treePane.stopSearch();
            autosaveTimer.stop();
            notebook.newNotebook();
            autosaveFailureShown = false;
        });
    }

    Component.onCompleted: {
        Theme.setSystemScheme(Qt.styleHints.colorScheme);
        openStartupPath();
    }

    // Closing: a file-backed notebook is flushed and the close simply goes
    // ahead. Otherwise the close is held back and the user is asked; their
    // answer closes the window again, which is why `discardOnClose` exists.
    onClosing: close => {
        if (discardOnClose || flushPendingChanges())
            return;
        close.accepted = false;
        confirmUnsaved.ask(() => {
            root.discardOnClose = true;
            root.close();
        });
    }

    function select(index) {
        // Whatever made this selection should be seen in the tree.
        treePane.stopSearch();
        selection.setCurrentIndex(index, ItemSelectionModel.ClearAndSelect);
        treePane.reveal(index);
    }

    // Alt+Up / Alt+Down: one place up or down among its siblings. The model
    // returns a fresh index, since the old one may be stale after a move.
    function moveSelected(step) {
        if (!selection.currentIndex.valid)
            return;
        const parentIndex = notebook.parent(selection.currentIndex);
        const target = selection.currentIndex.row + step;
        if (target < 0 || target >= notebook.rowCount(parentIndex))
            return;
        const moved = notebook.moveNode(selection.currentIndex, parentIndex, target);
        if (moved.valid) {
            selection.setCurrentIndex(moved, ItemSelectionModel.ClearAndSelect);
            treePane.reveal(moved);
        }
    }

    function createRoot() {
        const index = notebook.createDefaultRoot();
        select(index);
        editorPane.focusTitle();
    }

    function createChild() {
        if (!selection.currentIndex.valid)
            return;
        const index = notebook.createDefaultChild(selection.currentIndex);
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
        sequence: "Alt+Up"
        onActivated: root.moveSelected(-1)
    }
    Shortcut {
        sequence: "Alt+Down"
        onActivated: root.moveSelected(1)
    }
    Shortcut {
        sequence: "Ctrl+F"
        onActivated: treePane.startSearch()
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
                color: SplitHandle.pressed ? Theme.accent : Theme.border
            }
        }

        TreePane {
            id: treePane
            SplitView.preferredWidth: 260
            SplitView.minimumWidth: Math.max(160, treePane.implicitWidth)
            notebook: notebook
            selection: selection
            onNewRootRequested: root.createRoot()
            onNewChildRequested: root.createChild()
            onDeleteRequested: confirmDelete.askAboutSelection()
            onNewNotebookRequested: root.requestNewNotebook()
            onOpenRequested: root.requestOpen()
            onSaveRequested: root.saveCurrent()
            onSaveAsRequested: root.startSaveAs(null)
            onRecoveryRequested: recoveryDialog.open()
        }

        EditorPane {
            id: editorPane
            SplitView.fillWidth: true
            SplitView.minimumWidth: 240
            notebook: notebook
            selection: selection
            onDeleteRequested: confirmDelete.askAboutSelection()
            onRenameFailed: message => root.showError(message)
        }
    }

    ThemedDialog {
        id: confirmDelete

        function askAboutSelection() {
            if (!selection.currentIndex.valid)
                return;
            const index = selection.currentIndex;
            const name = notebook.data(index, Qt.DisplayRole);
            message.text = notebook.rowCount(index) > 0 ? qsTr("Move “%1” and everything beneath it to Trash?").arg(name) : qsTr("Move “%1” to Trash?").arg(name);
            open();
        }

        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 340
        modal: true
        title: qsTr("Move to Trash")
        standardButtons: Dialog.Yes | Dialog.No
        onAccepted: root.deleteSelected()

        contentItem: Label {
            id: message
            wrapMode: Text.Wrap
        }
    }

    // Shown before anything that would drop unsaved changes (Open, close).
    ThemedDialog {
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
        onRejected: {
            continuation = null;
            root.resumeAutosave();
        }

        contentItem: Label {
            id: unsavedText
            wrapMode: Text.Wrap
        }
    }

    ThemedDialog {
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
        onRejected: {
            root.afterSaveAs = null;
            root.resumeAutosave();
        }

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
            if (error !== "") {
                root.showError(error);
                root.resumeAutosave();
            } else {
                autosaveTimer.stop();
                root.autosaveFailureShown = false;
                root.warnAboutDuplicateTitles();
            }
        }
        onRejected: root.resumeAutosave()
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
        onRejected: {
            root.afterSaveAs = null;
            root.resumeAutosave();
        }
    }

    RecoveryDialog {
        id: recoveryDialog

        notebook: notebook
        onFailed: message => root.showError(message)
        onRestored: root.warnAboutDuplicateTitles()
    }

    ThemedDialog {
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
