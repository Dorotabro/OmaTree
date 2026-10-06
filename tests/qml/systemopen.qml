import QtQuick
import QtQuick.Controls
import org.omatree

// A notebook the system asks OmaTree to open (a Finder double-click on macOS)
// takes the same road as File > Open: nothing is lost without asking.
Base {
    function run() {
        const unsaved = findObj(app, o => o.hasOwnProperty("continuation"));
        mk(null, "Other", "other");
        const other = tmp("other.omatree");
        check("save the other", saveAs(other) === "");
        notebook.newNotebook();
        mk(null, "Mine", "mine");
        const mine = tmp("poznámky k věci.omatree");
        check("save mine", saveAs(mine) === "");

        // Clean and file-backed: opens straight away.
        app.openFromSystem(url(encodeURI(other)));
        until("a clean notebook is replaced at once", () => notebook.documentName === "other.omatree" && find("Other") !== null);

        // Unsaved and untitled: asked first, and Cancel keeps everything.
        notebook.newNotebook();
        mk(null, "Unsaved", "x");
        app.openFromSystem(url(encodeURI(mine)));
        until("the unsaved notebook is asked about", () => unsaved.visible);
        check("nothing was replaced yet", find("Unsaved") !== null);
        unsaved.reject();
        until("Cancel keeps the notebook", () => !unsaved.visible && find("Unsaved") !== null && notebook.dirty);

        // Discard lets it through.
        app.openFromSystem(url(encodeURI(mine)));
        until("asked again", () => unsaved.visible);
        unsaved.standardButton(Dialog.Discard).clicked();
        until("Discard opens the notebook (spaces and accents intact)", () => notebook.documentName === "poznámky k věci.omatree" && find("Mine") !== null);

        // With a dialog open, the request is ignored.
        shortcut("F1");
        until("F1 is open", () => shortcuts.visible);
        app.openFromSystem(url(encodeURI(other)));
        check("a modal dialog ignores it", notebook.documentName === "poznámky k věci.omatree");
        keyClick(Qt.Key_Escape);
        until("closed", () => !shortcuts.visible);

        // A file that is not a notebook is reported, not opened.
        const bad = tmp("not-a-notebook.omatree");
        const request = new XMLHttpRequest();
        request.open("PUT", url(bad), false);
        request.send("this is not sqlite");
        app.openFromSystem(url(encodeURI(bad)));
        until("an invalid file is reported", () => findObj(app, o => o.hasOwnProperty("standardButtons") && o.title === "OmaTree").visible);
        check("and the notebook stays", notebook.documentName === "poznámky k věci.omatree");
    }
}
