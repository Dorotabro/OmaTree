import QtQuick
import QtQuick.Controls
import org.omatree

// Shortcuts are the platform's own, and everything that names them (hints,
// menus, the F1 reference) names the keys that really work here: Ctrl on
// Linux and Windows, Command on macOS.
Base {
    // How each application shortcut reads on the running platform.
    function written(linux, mac) {
        return Qt.platform.os === "osx" ? mac : linux;
    }
    function allText() {
        let out = [];
        for (const section of shortcuts.sections) {
            out.push(section[0]);
            for (const entry of section[1])
                out.push(entry[0] + " " + entry[1]);
        }
        return out;
    }

    function run() {
        const expected = [["Ctrl+N", "⌘N"], ["Ctrl+Shift+N", "⇧⌘N"], ["Ctrl+O", "⌘O"], ["Ctrl+S", "⌘S"], ["Ctrl+Shift+S", "⇧⌘S"], ["Ctrl+F", "⌘F"], ["Ctrl+E", "⌘E"]];
        for (const [linux, mac] of expected) {
            const found = findShortcut(linux);
            check(linux + " is bound, and reads " + written(linux, mac), found !== null && found.nativeText === written(linux, mac) && found.enabled);
        }
        check("Keymap names the same keys", Keymap.text(Keymap.newNote) === written("Ctrl+N", "⌘N") && Keymap.text(Keymap.find) === written("Ctrl+F", "⌘F") && Keymap.text(Keymap.saveAs) === written("Ctrl+Shift+S", "⇧⌘S"));

        // The F1 reference shows the keys of the running platform.
        const text = allText().join("\n");
        const rows = {};
        for (const section of shortcuts.sections)
            for (const entry of section[1])
                rows[entry[1]] = entry[0];
        check("F1: New note", rows["New note"] === written("Ctrl+N", "⌘N"));
        check("F1: Save As", rows["Save As"] === written("Ctrl+Shift+S", "⇧⌘S"));
        check("F1: Focus the search field", rows["Focus the search field"] === written("Ctrl+F", "⌘F"));
        check("F1: Edit / Preview", rows["Edit / Preview"] === written("Ctrl+E", "⌘E"));
        check("F1: Cut / copy / paste", rows["Cut / copy / paste"] === written("Ctrl+X / C / V", "⌘X / ⌘C / ⌘V"));
        check("F1: Expand the subtree", rows["Expand the subtree"] === written("Ctrl+Right", "⌘→"));
        check("F1: the subtree marker click", rows["Toggle a whole subtree (marker)"] === written("Ctrl+Click", "⌘Click"));
        if (Qt.platform.os === "osx")
            check("F1 never tells a Mac user to press Ctrl", text.indexOf("Ctrl") < 0);
        else
            check("F1 keeps its Ctrl wording", text.indexOf("Ctrl+N New note") >= 0 && text.indexOf("⌘") < 0);

        // The hints and menu entries name the same keys as the shortcuts.
        const newNote = command("note");
        check("the New note hint names the shortcut", newNote !== null && newNote.hint === "New note   " + written("Ctrl+N", "⌘N"));
        const edit = findObj(editorPane, o => o.hasOwnProperty("hint") && o.text === "edit");
        check("the Edit hint names the shortcut", edit !== null && edit.hint === "Edit the note   " + written("Ctrl+E", "⌘E"));

        // Close the window: the Mac's own key, through the usual unsaved-changes
        // question; no such key elsewhere.
        const closer = findShortcut("Ctrl+W");
        if (Qt.platform.os === "osx") {
            check("Command+W is bound", closer !== null && closer.nativeText === "⌘W" && closer.enabled);
            check("F1 lists it", rows["Close the window"] === "⌘W");
            mk(null, "Unsaved", "x");
            const asked = findObj(app, o => o.hasOwnProperty("continuation"));
            closer.activated();
            until("Command+W with unsaved changes asks first", () => asked.visible && app.visible);
            asked.reject();
            until("and Cancel keeps the window and the notebook", () => !asked.visible && app.visible && notebook.dirty);
        } else {
            check("no Close shortcut outside macOS", closer === null && rows["Close the window"] === undefined);
        }

        // OmaTree's own dialogs are English, whatever the system language.
        const unsaved = findObj(app, o => o.hasOwnProperty("continuation"));
        const overwrite = findObj(app, o => o.hasOwnProperty("ask") && o.hasOwnProperty("path"));
        const errors = findObj(app, o => o.hasOwnProperty("standardButtons") && o.title === "OmaTree");
        const labels = [[confirmDelete, Dialog.Yes, "Yes"], [confirmDelete, Dialog.No, "No"], [overwrite, Dialog.Yes, "Yes"], [overwrite, Dialog.No, "No"], [unsaved, Dialog.Discard, "Discard"], [unsaved, Dialog.Cancel, "Cancel"], [errors, Dialog.Ok, "OK"], [shortcuts, Dialog.Close, "Close"]];
        const dialogs = [confirmDelete, unsaved, overwrite, errors, shortcuts];
        for (const d of dialogs) {
            d.open();
            until("a dialog opens", () => d.opened);
            for (const [dialog, which, english] of labels) {
                if (dialog !== d)
                    continue;
                check("button " + english + " reads " + english, dialog.standardButton(which).text === english);
            }
            if (d === unsaved)
                check("button Save reads Save (or Save As… when untitled)", d.standardButton(Dialog.Save).text === (notebook.hasPath() ? "Save" : "Save As…"));
            d.close();
            until("and closes", () => !d.visible);
        }
    }
}
