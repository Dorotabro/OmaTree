import QtQuick
import org.omatree

// Scenario 2: an edit in a file-backed notebook is saved by itself.
Base {
    function run() {
        const note = mk(null, "Note", "");
        const path = tmp("autosave.omatree");
        check("save as", saveAs(path) === "");
        check("clean after saving", !notebook.dirty);
        const checkpoints = notebook.checkpointCount();

        select(note);
        bodyField().forceActiveFocus();
        bodyField().text = "typed in the editor";
        check("the edit made it dirty", notebook.dirty);
        // The debounce is one second; wait for the save, not a fixed time.
        until("autosave cleans it", () => !notebook.dirty, 8000);
        check("no checkpoint for body editing", notebook.checkpointCount() === checkpoints);

        // What is on disk: open the file again.
        check("reopen", notebook.openFile(url(path)) === "");
        check("the change is in the file", notebook.body(find("Note")) === "typed in the editor");
    }
}
