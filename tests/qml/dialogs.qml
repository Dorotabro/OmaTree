import QtQuick
import QtQuick.Controls
import org.omatree

// Dialog keyboard behaviour: one visible focus that is also what Enter will
// press, arrows move it, N / Y / Escape answer, and nothing beneath reacts.
Base {
    function button(dialog, which) {
        return dialog.standardButton(which);
    }
    // Exactly the focused button shows the focus mark, and nothing else does.
    function onlyFocused(dialog, which) {
        const all = [Dialog.Yes, Dialog.No, Dialog.Save, Dialog.Discard, Dialog.Cancel, Dialog.Ok, Dialog.Close];
        for (let i = 0; i < all.length; ++i) {
            const b = dialog.standardButton(all[i]);
            if (!b)
                continue;
            const mine = all[i] === which;
            if (b.activeFocus !== mine)
                return false;
        }
        return true;
    }
    function ask() {
        tree.forceActiveFocus();
        keyClick(Qt.Key_Delete);
        until("the Trash dialog opens", () => confirmDelete.visible);
    }
    function rowsNow() {
        return notebook.rowCount(tree.rootIndex);
    }
    function restore() {
        if (notebook.trashCount() > 0)
            check("restore for the next case", notebook.restoreTrash(0) === "");
        select(find("Two"));
    }

    function run() {
        const confirmUnsaved = findObj(app, o => o.hasOwnProperty("continuation"));
        const confirmOverwrite = findObj(app, o => o.hasOwnProperty("ask") && o.hasOwnProperty("path"));
        mk(null, "One", "1");
        mk(null, "Two", "2");
        mk(null, "Three", "3");
        check("save", saveAs(tmp("dialogs.omatree")) === "");
        select(find("Two"));
        const three = rowsNow();

        // --- Move to Trash: the reported workflow, key by key
        ask();
        const no = button(confirmDelete, Dialog.No), yes = button(confirmDelete, Dialog.Yes);
        until("the safe NO has the focus, and only it shows it", () => onlyFocused(confirmDelete, Dialog.No));
        keyClick(Qt.Key_Right);
        until("Right: YES has the focus and NO no longer shows it", () => onlyFocused(confirmDelete, Dialog.Yes) && !no.activeFocus);
        keyClick(Qt.Key_Left);
        until("Left: NO again", () => onlyFocused(confirmDelete, Dialog.No) && !yes.activeFocus);
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Right);
        until("the arrows wrap: Right from YES is NO", () => onlyFocused(confirmDelete, Dialog.No));
        keyClick(Qt.Key_Left);
        until("and Left from NO is YES", () => onlyFocused(confirmDelete, Dialog.Yes));
        check("while open, nothing beneath reacts: application shortcuts stand down", !findObj(app, o => o.hasOwnProperty("sequence") && o.hasOwnProperty("enabled") && o.sequence.toString() === "Ctrl+N").enabled);
        keyClick(Qt.Key_Return);
        until("Right + Enter moves the note to Trash", () => !confirmDelete.visible && rowsNow() === three - 1 && notebook.trashCount() === 1);
        restore();

        // Delete -> N
        ask();
        const n = dialogShortcut(confirmDelete, "N");
        check("N is a shortcut of this dialog and active while it is open", n !== null && n.enabled);
        n.activated();
        until("N answers No", () => !confirmDelete.visible);
        check("the note remains", rowsNow() === three && selected() === "Two");
        check("the shortcut is off when the dialog is closed", !n.enabled);
        until("focus is back on the tree", () => tree.activeFocus);
        keyClick(Qt.Key_Down);
        until("and the arrows work at once", () => selected() === "Three");
        select(find("Two"));

        // Delete -> Escape
        ask();
        keyClick(Qt.Key_Escape);
        until("Escape answers No", () => !confirmDelete.visible);
        check("the note remains and keeps the selection", rowsNow() === three && selected() === "Two");
        until("focus is back on the tree", () => tree.activeFocus);
        keyClick(Qt.Key_Up);
        until("and Up works at once", () => selected() === "One");
        select(find("Two"));

        // Delete -> Y
        ask();
        const y = dialogShortcut(confirmDelete, "Y");
        check("Y is active", y !== null && y.enabled);
        y.activated();
        until("Y answers Yes", () => !confirmDelete.visible && rowsNow() === three - 1);
        restore();

        // Delete -> Enter, with the initial NO
        ask();
        keyClick(Qt.Key_Return);
        until("Enter activates the initial NO", () => !confirmDelete.visible);
        check("the note remains", rowsNow() === three);

        // Delete -> Space on the focused NO, then Space on a focused YES
        ask();
        keyClick(Qt.Key_Space);
        until("Space activates the focused NO", () => !confirmDelete.visible && rowsNow() === three);
        ask();
        keyClick(Qt.Key_Right);
        keyClick(Qt.Key_Space);
        until("Right + Space activates YES", () => !confirmDelete.visible && rowsNow() === three - 1);
        restore();

        // A second Delete while the dialog is open does not stack another.
        ask();
        keyClick(Qt.Key_Delete);
        wait(50);
        check("Delete in the dialog does nothing", confirmDelete.visible && notebook.trashCount() === 0);
        keyClick(Qt.Key_Escape);
        until("closed", () => !confirmDelete.visible);

        // --- Save / Discard / Cancel
        let ran = false;
        confirmUnsaved.ask(() => { ran = true; });
        until("the unsaved dialog opens", () => confirmUnsaved.visible);
        until("the safe SAVE has the focus, alone", () => onlyFocused(confirmUnsaved, Dialog.Save));
        let steps = 0;
        while (!button(confirmUnsaved, Dialog.Discard).activeFocus && steps++ < 4)
            keyClick(Qt.Key_Right);
        until("the arrows reach DISCARD, and it alone shows the focus", () => onlyFocused(confirmUnsaved, Dialog.Discard));
        keyClick(Qt.Key_Tab);
        check("Tab still moves the focus", !button(confirmUnsaved, Dialog.Discard).activeFocus);
        while (!button(confirmUnsaved, Dialog.Discard).activeFocus && steps++ < 8)
            keyClick(Qt.Key_Left);
        keyClick(Qt.Key_Return);
        until("Enter on the focused DISCARD discards", () => !confirmUnsaved.visible && ran);
        ran = false;
        confirmUnsaved.ask(() => { ran = true; });
        until("again", () => confirmUnsaved.visible);
        keyClick(Qt.Key_Escape);
        until("Escape is Cancel", () => !confirmUnsaved.visible);
        check("and the pending action did not run", !ran);
        check("a three-way dialog has no N / Y", !confirmUnsaved.binary);

        // --- Replace file (a destructive Yes / No)
        confirmOverwrite.ask(tmp("over.omatree"));
        until("the replace dialog opens", () => confirmOverwrite.visible);
        until("the safe NO has the focus", () => onlyFocused(confirmOverwrite, Dialog.No));
        keyClick(Qt.Key_Right);
        until("Right: YES", () => onlyFocused(confirmOverwrite, Dialog.Yes));
        keyClick(Qt.Key_Escape);
        until("Escape does not replace", () => !confirmOverwrite.visible);
        confirmOverwrite.ask(tmp("over.omatree"));
        until("again", () => confirmOverwrite.visible);
        dialogShortcut(confirmOverwrite, "N").activated();
        until("N does not replace", () => !confirmOverwrite.visible && notebook.documentName === "dialogs.omatree");
        confirmOverwrite.ask(tmp("over.omatree"));
        until("a third time", () => confirmOverwrite.visible);
        dialogShortcut(confirmOverwrite, "Y").activated();
        until("Y replaces", () => !confirmOverwrite.visible && notebook.documentName === "over.omatree");

        // --- Informational (an error)
        app.showError("something happened");
        until("the message opens", () => errorDialog().visible);
        until("its only button has the focus", () => onlyFocused(errorDialog(), Dialog.Ok));
        check("it has no N / Y", !errorDialog().binary);
        keyClick(Qt.Key_Return);
        until("Enter acknowledges it", () => !errorDialog().visible);
        app.showError("again");
        until("and again", () => errorDialog().visible);
        keyClick(Qt.Key_Escape);
        until("Escape closes it", () => !errorDialog().visible);

        // --- Recovery: a list with a Close button
        const recoveryDialog = findObj(app, o => o.hasOwnProperty("restored") && o.hasOwnProperty("failed"));
        recoveryDialog.open();
        until("Recovery opens", () => recoveryDialog.visible);
        until("Close has the focus", () => onlyFocused(recoveryDialog, Dialog.Close));
        keyClick(Qt.Key_Escape);
        until("Escape closes Recovery", () => !recoveryDialog.visible);

        // --- F1
        tree.forceActiveFocus();
        shortcut("F1");
        until("F1 opens the reference", () => shortcuts.visible);
        keyClick(Qt.Key_F1);
        until("F1 closes it", () => !shortcuts.visible);
        shortcut("F1");
        until("opens again", () => shortcuts.visible);
        keyClick(Qt.Key_Escape);
        until("Escape closes it", () => !shortcuts.visible);
    }

    function errorDialog() {
        return findObj(app, o => o.hasOwnProperty("standardButtons") && o.title === "OmaTree");
    }
}
