import QtQuick
import QtQuick.Controls
import org.omatree

// Recovery from the keyboard: a real current row, Up / Down, Enter / Space
// restoring exactly that row, tabs by Left / Right on the header only.
Base {
    property var dlg: null
    property var tabs: null
    property var trashList: null
    property var checkpointList: null

    function listOf(which) {
        return which === 0 ? trashList : checkpointList;
    }
    function openRecovery() {
        tree.forceActiveFocus();
        dlg.open();
        until("Recovery opens", () => dlg.visible);
    }
    function closed() {
        until("Recovery is closed", () => !dlg.visible);
    }
    // The row Enter would restore is the marked one.
    function marked(list) {
        return list.activeFocus && list.currentIndex >= 0 && list.currentItem !== null;
    }
    function trashTitles() {
        const t = [];
        for (let i = 0; i < notebook.trashCount(); ++i)
            t.push(notebook.trashTitle(i));
        return t.join("|");
    }

    function run() {
        dlg = findObj(app, o => o.hasOwnProperty("restored") && o.hasOwnProperty("failed"));
        tabs = findObj(dlg, o => o.hasOwnProperty("contentChildren") && o.hasOwnProperty("position") && o.hasOwnProperty("currentIndex"));
        trashList = findObj(dlg, o => o.objectName === "trashList");
        checkpointList = findObj(dlg, o => o.objectName === "checkpointList");
        check("both recovery lists found", trashList !== null && checkpointList !== null);
        check("tab bar found", tabs !== null);

        mk(null, "Keep", "k");
        for (const name of ["D1", "D2", "D3"])
            mk(null, name, name.toLowerCase());
        check("save", saveAs(tmp("recoverykeys.omatree")) === "");
        for (const name of ["D1", "D2", "D3"])
            check("trash " + name, notebook.removeNode(find(name)));
        select(find("Keep"));
        check("three Trash entries, newest first", trashTitles() === "D3|D2|D1");
        check("several checkpoints", notebook.checkpointCount() >= 3);

        // --- Opens on Trash: first row current, list has the keyboard
        openRecovery();
        until("the Trash list has the keyboard", () => trashList.activeFocus);
        check("Trash tab, first row current", tabs.currentIndex === 0 && trashList.currentIndex === 0 && marked(trashList));
        keyClick(Qt.Key_Down);
        check("Down: second row", trashList.currentIndex === 1);
        keyClick(Qt.Key_Down);
        check("Down: third row", trashList.currentIndex === 2);
        keyClick(Qt.Key_Down);
        check("Down on the last row stays", trashList.currentIndex === 2);
        keyClick(Qt.Key_Up);
        check("Up: second row", trashList.currentIndex === 1);
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        check("Up on the first row stays", trashList.currentIndex === 0);
        check("Up / Down moved nothing beneath", selection.currentIndex.valid && notebook.data(selection.currentIndex, Qt.DisplayRole) === "Keep");
        keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Right);
        check("Left / Right in the list do not switch tabs", tabs.currentIndex === 0 && trashList.activeFocus);
        keyClick(Qt.Key_Delete);
        wait(30);
        check("Delete does not reach the tree behind", !confirmDelete.visible && notebook.trashCount() === 3);
        check("Dialog is still open", dlg.visible);

        // --- Enter restores the marked row, and only it
        keyClick(Qt.Key_Down);
        check("second row marked", trashList.currentIndex === 1);
        keyClick(Qt.Key_Return);
        closed();
        check("Enter restored the second row (D2)", trashTitles() === "D3|D1" && titles().indexOf("D2") >= 0);

        // --- Space restores the marked row (reopens on the first row)
        openRecovery();
        until("list has the keyboard again", () => trashList.activeFocus);
        check("reopened on the first row", trashList.currentIndex === 0);
        keyClick(Qt.Key_Space);
        closed();
        check("Space restored the first row (D3)", trashTitles() === "D1" && titles().indexOf("D3") >= 0);

        // --- Escape never restores
        openRecovery();
        keyClick(Qt.Key_Escape);
        closed();
        check("Escape restored nothing", trashTitles() === "D1");

        // --- Tabs: Left / Right on the header
        openRecovery();
        tabs.itemAt(0).forceActiveFocus(Qt.TabFocusReason);
        check("header has the keyboard", tabs.itemAt(0).activeFocus);
        keyClick(Qt.Key_Right);
        until("the keyboard stays on the header", () => tabs.currentItem.activeFocus);
        check("Right: Checkpoints", tabs.currentIndex === 1);
        check("destination has a valid first row", checkpointList.count > 0 && checkpointList.currentIndex === 0);
        check("nothing restored by switching", trashTitles() === "D1");
        keyClick(Qt.Key_Left);
        until("on the header again", () => tabs.currentItem.activeFocus);
        check("Left: Trash " + tabs.currentIndex + " " + trashList.currentIndex, tabs.currentIndex === 0 && trashList.currentIndex === 0);
        // Tab goes header -> list -> Close
        keyClick(Qt.Key_Tab);
        until("Tab reaches the list", () => trashList.activeFocus);
        keyClick(Qt.Key_Tab);
        until("Tab reaches Close", () => dlg.standardButton(Dialog.Close).activeFocus);
        keyClick(Qt.Key_Backtab);
        until("Shift+Tab returns to the list", () => trashList.activeFocus);
        keyClick(Qt.Key_Tab);
        keyClick(Qt.Key_Return);
        closed();
        check("Enter on Close restored nothing", trashTitles() === "D1");

        // --- Mouse RESTORE
        openRecovery();
        const restoreCmd = findObj(trashList.currentItem, o => o.hasOwnProperty("hint") && o.text === "restore");
        check("the RESTORE command is there", restoreCmd !== null && restoreCmd.focusPolicy === Qt.NoFocus);
        mouseClick(restoreCmd);
        closed();
        check("clicking RESTORE restores", trashTitles() === "" && titles().indexOf("D1") >= 0);

        // --- Empty tab: no phantom row, keys do nothing
        openRecovery();
        check("empty Trash: no current row", trashList.count === 0 && trashList.currentIndex === -1);
        until("Close has the focus", () => dlg.standardButton(Dialog.Close).activeFocus);
        keyClick(Qt.Key_Down);
        keyClick(Qt.Key_Up);
        tabs.itemAt(0).forceActiveFocus(Qt.TabFocusReason);
        keyClick(Qt.Key_Right);
        check("header still switches tabs when empty", tabs.currentIndex === 1);
        keyClick(Qt.Key_Left);
        check("and back", tabs.currentIndex === 0 && trashList.currentIndex === -1);
        trashList.forceActiveFocus();
        keyClick(Qt.Key_Return);
        keyClick(Qt.Key_Space);
        check("Enter / Space on an empty list do nothing", dlg.visible && trashList.currentIndex === -1);

        // --- Selection stays valid when entries vanish while open
        keyClick(Qt.Key_Escape);
        closed();
        for (const name of ["D1", "D2"])
            check("trash again " + name, notebook.removeNode(find(name)));
        openRecovery();
        until("list focused", () => trashList.activeFocus);
        keyClick(Qt.Key_Down);
        check("on the last row", trashList.currentIndex === 1);
        check("restore one directly", notebook.restoreTrash(1) === "");
        until("the count follows", () => trashList.count === 1);
        check("current index is still valid", trashList.currentIndex === 0 && marked(trashList));
        check("restore the last directly", notebook.restoreTrash(0) === "");
        until("the list empties", () => trashList.count === 0);
        check("no phantom selection", trashList.currentIndex === -1);
        keyClick(Qt.Key_Escape);
        closed();
    }
}
