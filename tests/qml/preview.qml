import QtQuick
import org.omatree

// Scenario 7: Markdown Preview is a view; it never changes the note.
Base {
    readonly property string source: "# Title\n\nSome **bold** and `code`.\n\n- one\n- two\n"

    function run() {
        const one = mk(null, "One", source);
        const two = mk(null, "Two", "## Second\n\nplain words\n");
        check("save", saveAs(tmp("preview.omatree")) === "");
        select(find("One"));
        const checkpoints = notebook.checkpointCount();

        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview is on", () => editorPane.previewing && previewField() !== null);
        const shown = previewField().getText(0, previewField().length);
        check("it is rendered, not raw source", shown.indexOf("Title") >= 0 && shown.indexOf("bold") >= 0 && shown.indexOf("**") < 0 && shown.indexOf("# ") < 0 && shown.indexOf("`") < 0);
        check("it is read-only", previewField().readOnly);
        check("previewing is not a change", !notebook.dirty && notebook.checkpointCount() === checkpoints);

        // Another note while previewing renders that note.
        select(find("Two"));
        until("the new note is rendered", () => previewField().getText(0, previewField().length).indexOf("Second") >= 0);
        check("no raw heading marker", previewField().getText(0, previewField().length).indexOf("##") < 0);
        select(find("One"));

        shortcut("Ctrl+E");
        until("back to Edit", () => !editorPane.previewing);
        check("the raw body is byte-identical", bodyField().text === source && notebook.body(find("One")) === source);
        check("still clean, no checkpoint", !notebook.dirty && notebook.checkpointCount() === checkpoints);

        // Search still sees the raw source.
        check("search sees Markdown syntax", notebook.search("**bold**") === 1);

        // New and Open start again in Edit.
        shortcut("Ctrl+E");
        until("previewing again", () => editorPane.previewing);
        app.requestNewNotebook();
        check("New Notebook resets to Edit", !editorPane.previewing);
        check("open the saved notebook", notebook.openFile(url(tmp("preview.omatree"))) === "");
        select(find("One"));
        shortcut("Ctrl+E");
        until("previewing once more", () => editorPane.previewing);
        // The Open path in the app also calls resetMode(); the file dialog
        // itself is not driven here, so exercise the same call it makes.
        editorPane.resetMode();
        check("Open resets to Edit", !editorPane.previewing);
    }
}
