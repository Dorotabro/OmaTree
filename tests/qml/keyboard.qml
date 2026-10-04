import QtQuick
import QtQuick.Controls
import org.omatree

// Scenario 10: a small keyboard, help and dialog smoke test.
Base {
    function run() {
        const note = mk(null, "Note", "body");
        mk(note, "Child", "");
        check("save", saveAs(tmp("keyboard.omatree")) === "");
        select(find("Note"));
        const checkpoints = notebook.checkpointCount();
        tree.forceActiveFocus();

        shortcut("F1");
        until("F1 opens the shortcuts", () => shortcuts.visible);
        keyClick(Qt.Key_Escape);
        until("Escape closes them", () => !shortcuts.visible);

        shortcut("Ctrl+F");
        until("Ctrl+F focuses the search field", () => searchField.focused);
        keyClick(Qt.Key_Escape);
        until("Escape with no query returns to the tree", () => tree.activeFocus && !searchField.focused);

        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Ctrl+E previews", () => editorPane.previewing);
        shortcut("Ctrl+E");
        until("Ctrl+E edits again", () => !editorPane.previewing);

        tree.forceActiveFocus();
        keyClick(Qt.Key_F10, Qt.ShiftModifier);
        const menu = findObj(treePane, o => o.hasOwnProperty("branch") && o.hasOwnProperty("onNote"));
        until("Shift+F10 opens the tree menu", () => menu.visible);
        keyClick(Qt.Key_Escape);
        until("Escape closes the menu", () => !menu.visible);

        // A confirmation dialog: safe default, Escape cancels.
        tree.forceActiveFocus();
        keyClick(Qt.Key_Delete);
        until("Delete asks first", () => confirmDelete.visible);
        check("the default is the safe answer", confirmDelete.defaultButton === Dialog.No);
        check("focus starts on the safe button", app.activeFocusItem === confirmDelete.standardButton(Dialog.No));
        keyClick(Qt.Key_Escape);
        until("Escape cancels", () => !confirmDelete.visible);
        check("nothing was deleted", titles().join("|") === "Note(Child)");

        check("none of it touched the document", !notebook.dirty && notebook.checkpointCount() === checkpoints && selected() === "Note");
    }
}
