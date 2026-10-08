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

        // The keyboard in Preview. A focused text control, even a read-only one,
        // takes the application-wide shortcuts (Ctrl+E, Ctrl+S, F1, ...) for
        // itself, so Ctrl+E could not switch back to Edit from Preview. (These
        // scenarios fire a Shortcut's `activated` directly, which cannot see
        // that; what they can check is where the keyboard is.) So it is held by
        // a plain item, which also provides what the text did with the keyboard:
        // scrolling and select all.
        const paragraphs = [];
        for (let i = 1; i <= 80; ++i)
            paragraphs.push("Paragraph " + i + " of a long note.");
        mk(null, "Long", "# Top\n\n" + paragraphs.join("\n\n") + "\n");
        select(find("Long"));
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview has the keyboard on the plain item", () => editorPane.previewing && app.activeFocusItem === previewKeys());
        check("the text itself does not have it", !previewField().activeFocus);
        mouseClick(previewField(), 30, 30);
        check("a click in the text leaves the keyboard where it was", app.activeFocusItem === previewKeys() && !previewField().activeFocus);

        const flick = findObj(editorPane, o => o.hasOwnProperty("contentY") && o.hasOwnProperty("contentHeight") && o.hasOwnProperty("flick"));
        check("the long note scrolls", flick !== null && flick.contentHeight > flick.height);
        check("it starts at the top", flick.contentY === 0);
        keyClick(Qt.Key_PageDown);
        until("Page Down scrolls", () => flick.contentY > 0);
        const page = flick.contentY;
        keyClick(Qt.Key_Down);
        until("Down scrolls a little", () => flick.contentY > page);
        keyClick(Qt.Key_End);
        until("End goes to the bottom", () => flick.contentY >= flick.contentHeight - flick.height - 1);
        keyClick(Qt.Key_Home);
        until("Home goes to the top", () => flick.contentY === 0);
        keyClick(Qt.Key_A, Qt.ControlModifier);
        until("Ctrl+A selects the previewed text", () => previewField().selectedText.indexOf("Paragraph 80") >= 0);
        keyClick(Qt.Key_Escape);
        until("Escape still returns to the tree", () => tree.activeFocus);
        check("and Preview stays", editorPane.previewing);
    }
}
