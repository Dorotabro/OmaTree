import QtQuick
import org.omatree

// Another connection changes the file after this one opened it. The Rust
// side makes that change when the scenario says it is ready.
Base {
    function exists(path) {
        try {
            const request = new XMLHttpRequest();
            request.open("GET", url(path), false);
            request.send();
            return request.status === 200 || request.responseText !== "";
        } catch (e) {
            return false;
        }
    }
    function touch(path) {
        const request = new XMLHttpRequest();
        request.open("PUT", url(path), false);
        request.send("x");
    }

    function run() {
        const note = mk(null, "Note", "original");
        const path = tmp("conflict.omatree");
        check("save", saveAs(path) === "");
        select(find("Note"));

        // Let the other writer commit to the file, then edit.
        touch(tmp("ready"));
        until("the other writer is done", () => exists(tmp("done")), 15000);
        bodyField().forceActiveFocus();
        bodyField().text = "mine";
        check("a change is pending", notebook.dirty);

        const message = notebook.save();
        check("save is refused with the conflict message", message.indexOf("changed on disk") >= 0 && message.indexOf("Save As") >= 0);
        check("the document is still dirty and holds my text", notebook.dirty && notebook.body(find("Note")) === "mine");
        // Autosave: it must not write either, and it reports once.
        wait(1500);
        until("autosave tried and gave up, still dirty", () => notebook.dirty && app.autosaveFailureShown === true, 8000);
        check("explicit Save keeps reporting it", notebook.save().indexOf("changed on disk") >= 0);

        // Save As is the way out.
        const copy = tmp("mine.omatree");
        check("save as works", notebook.saveAs(copy, false) === "");
        check("and the document is clean", !notebook.dirty && notebook.documentName === "mine.omatree");
    }
}
