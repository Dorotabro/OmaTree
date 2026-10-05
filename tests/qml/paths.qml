import QtQuick
import org.omatree

// Paths the way a native Open / Save As dialog and the command line hand them
// over: with spaces, accents, Czech and other non-ASCII letters, in nested
// folders. Each must save as typed (with `.omatree` added), autosave, be
// recognised as existing, and reopen with its content. `params.dirs` lists
// existing directories to use.
Base {
    function run() {
        for (let i = 0; i < params.dirs.length; ++i) {
            const dir = params.dirs[i];
            const what = "[" + dir.split("/").slice(-2).join("/") + "] ";
            const typed = dir + "/poznámky";
            const wanted = typed + ".omatree";

            // What FileDialog.selectedFile is: a percent-encoded file: URL.
            const target = notebook.saveAsTarget("file://" + encodeURI(typed));
            check(what + "Save As keeps the name and adds .omatree", target === wanted);
            check(what + "nothing there yet, so no overwrite question", !notebook.needsOverwriteConfirmation(target));

            notebook.newNotebook();
            mk(null, "Note", "first");
            check(what + "Save As", notebook.saveAs(target, false) === "" && notebook.hasPath() && !notebook.dirty);
            check(what + "the notebook is named after the file", notebook.documentName === "poznámky.omatree");
            check(what + "saving to the notebook's own file never asks", !notebook.needsOverwriteConfirmation(target));

            select(find("Note"));
            bodyField().forceActiveFocus();
            bodyField().text = "autosaved " + i;
            check(what + "the edit made it dirty", notebook.dirty);
            until(what + "autosave cleans it", () => !notebook.dirty, 8000);

            notebook.newNotebook();
            check(what + "from another notebook, Save As to that file asks before replacing it", notebook.needsOverwriteConfirmation(target));
            check(what + "reopen through the dialog's URL", notebook.openFile("file://" + encodeURI(wanted)) === "");
            check(what + "the autosaved text is there", notebook.body(find("Note")) === "autosaved " + i);
            notebook.newNotebook();
            check(what + "reopen through a command-line path", notebook.openPath(wanted) === "");
            check(what + "and again", notebook.body(find("Note")) === "autosaved " + i && notebook.documentName === "poznámky.omatree");
        }
    }
}
