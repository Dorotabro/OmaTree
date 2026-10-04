import QtQuick
import org.omatree

// A new note or child always starts in Edit; browsing existing notes keeps
// the mode.
Base {
    function run() {
        mk(null, "Existing", "# Heading\n\ntext");
        mk(null, "Other", "other body");
        check("save", saveAs(tmp("newnote.omatree")) === "");
        const checkpoints = notebook.checkpointCount();
        const mutated = [];
        notebook.documentMutated.connect(() => mutated.push(1));

        // Browsing keeps Preview.
        select(find("Existing"));
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview is on", () => editorPane.previewing);
        select(find("Other"));
        check("moving to another existing note stays in Preview", editorPane.previewing);

        // Ctrl+N from Preview.
        shortcut("Ctrl+N");
        until("a new note exists and is selected", () => selected() === "New note");
        check("it starts in Edit", !editorPane.previewing);
        until("the title has the focus", () => titleField().activeFocus);
        check("the body is editable and visible", bodyField().visible && !bodyField().readOnly);
        check("exactly one mutation for the creation", mutated.length === 1 && notebook.checkpointCount() === checkpoints);
        keyClick(Qt.Key_Return);
        until("Enter in the title moves on to the body", () => bodyField().activeFocus);
        typeText("typed");
        check("typing reaches the new note", notebook.body(current()) === "typed");

        // Ctrl+Shift+N from Preview.
        select(find("Existing"));
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview again", () => editorPane.previewing);
        const count = mutated.length;
        shortcut("Ctrl+Shift+N");
        until("a child is created", () => notebook.rowCount(find("Existing")) === 1);
        check("the child starts in Edit", !editorPane.previewing);
        until("title focus", () => titleField().activeFocus);
        check("one more mutation only", mutated.length === count + 1);

        // The footer commands.
        select(find("Other"));
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview for the footer", () => editorPane.previewing);
        command("note").forceActiveFocus();
        keyClick(Qt.Key_Return);
        until("+ NOTE starts in Edit", () => !editorPane.previewing && titleField().activeFocus);
        select(find("Other"));
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview again", () => editorPane.previewing);
        command("child").forceActiveFocus();
        keyClick(Qt.Key_Return);
        until("+ CHILD starts in Edit", () => !editorPane.previewing && titleField().activeFocus);

        // A creation that cannot happen changes nothing: no child without a note.
        selection.clear();
        selection.clearCurrentIndex();
        select(find("Existing"));
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview before the refused creation", () => editorPane.previewing);
        selection.clearCurrentIndex();
        const kept = editorPane.previewing;
        shortcut("Ctrl+Shift+N");
        wait(100);
        check("a refused creation leaves the mode alone", editorPane.previewing === kept);
    }
}
