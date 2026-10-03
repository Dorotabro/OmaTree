import QtQuick
import org.omatree

// Scenario 1: an Untitled notebook, a small tree, saved and opened again.
Base {
    function run() {
        check("starts Untitled and clean", notebook.documentName === "Untitled" && !notebook.dirty && !notebook.hasPath());
        check("starts empty", notebook.rowCount(tree.rootIndex) === 0);

        // The real New note / New child paths, as the shortcuts use them.
        app.createRoot();
        check("a root note exists and is current", notebook.rowCount(tree.rootIndex) === 1 && selection.currentIndex.valid);
        check("creating made the document dirty", notebook.dirty);
        const root = current();
        check("rename", notebook.rename(root, "Projects") === "");
        app.createChild();
        const child = current();
        check("a child exists under the root", notebook.rowCount(root) === 1);

        // Rename and edit through the real editor controls, as typed.
        until("the editor shows the new child", () => titleField().text !== "" && selected() === titleField().text);
        titleField().text = "Ideas";
        editorPane.commitTitle();
        check("rename through the title field", notebook.data(child, 0) === "Ideas");
        bodyField().forceActiveFocus();
        bodyField().text = "first idea";
        check("typing in the editor reaches the notebook", notebook.body(child) === "first idea");

        // Selecting the other note shows it.
        select(root);
        check("selection shows the other note", titleField().text === "Projects" && bodyField().text === "");
        select(find("Ideas"));
        check("and back again", titleField().text === "Ideas" && bodyField().text === "first idea");

        const path = tmp("lifecycle.omatree");
        check("untitled cannot save in place", notebook.hasPath() === false);
        check("save as", saveAs(path) === "");
        check("saved: clean, named", !notebook.dirty && notebook.documentName === "lifecycle.omatree");

        check("open it again", notebook.openFile(url(path)) === "");
        check("hierarchy and titles survive", titles().join("|") === "Projects(Ideas)");
        check("body survives", notebook.body(find("Ideas")) === "first idea");
        check("opening leaves it clean", !notebook.dirty);
    }
}
