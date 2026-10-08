import QtQuick
import QtQuick.Controls
import org.omatree

// Escape in the title, body or preview returns to the tree; transient
// surfaces keep their own Escape.
Base {
    function run() {
        mk(null, "First", "first body");
        mk(null, "Second", "second body");
        mk(null, "Third", "third body");
        check("save", saveAs(tmp("escape.omatree")) === "");
        select(find("Second"));
        const checkpoints = notebook.checkpointCount();
        const mutated = [];
        notebook.documentMutated.connect(() => mutated.push(1));

        // Body -> Escape -> tree, and the arrows work at once.
        bodyField().forceActiveFocus();
        check("the body has the focus", bodyField().activeFocus);
        keyClick(Qt.Key_Escape);
        until("Escape focuses the tree", () => tree.activeFocus && !bodyField().activeFocus);
        check("the same note is still selected", selected() === "Second");
        check("the body text is untouched", bodyField().text === "second body" && notebook.body(current()) === "second body");
        keyClick(Qt.Key_Down);
        until("Down moves to the next note immediately", () => selected() === "Third");
        keyClick(Qt.Key_Up);
        keyClick(Qt.Key_Up);
        until("and Up", () => selected() === "First");
        select(find("Second"));

        // Title -> Escape -> tree. The text typed so far stays.
        titleField().forceActiveFocus();
        titleField().selectAll();
        typeText("Renamed");
        keyClick(Qt.Key_Escape);
        until("Escape from the title focuses the tree", () => tree.activeFocus);
        check("the title text was not reverted", titleField().text === "Renamed" || selected() === "Renamed");
        check("still the same note", selected() === "Renamed");
        titleField().text = "Second";
        editorPane.commitTitle();
        select(find("Second"));

        // Preview -> Escape -> tree, mode preserved.
        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview is on and has the keyboard", () => editorPane.previewing && previewField() !== null && app.activeFocusItem === previewKeys());
        keyClick(Qt.Key_Escape);
        until("Escape from the preview focuses the tree", () => tree.activeFocus);
        check("Preview is preserved", editorPane.previewing);
        check("selection is preserved", selected() === "Second");
        keyClick(Qt.Key_Down);
        until("the tree navigates from Preview too", () => selected() === "Third" && editorPane.previewing);
        shortcut("Ctrl+E");
        until("back to Edit", () => !editorPane.previewing);
        bodyField().forceActiveFocus();
        keyClick(Qt.Key_Escape);
        until("tree again", () => tree.activeFocus);
        check("Edit mode preserved", !editorPane.previewing);

        // Enter on a note in the tree starts editing it, from Edit or Preview.
        keyClick(Qt.Key_Return);
        until("Enter in the tree focuses the body", () => bodyField().activeFocus && !editorPane.previewing);
        keyClick(Qt.Key_Escape);
        until("Escape back", () => tree.activeFocus);
        shortcut("Ctrl+E");
        until("Preview", () => editorPane.previewing);
        tree.forceActiveFocus();
        keyClick(Qt.Key_Return);
        until("Enter from Preview switches to Edit and focuses the body", () => !editorPane.previewing && bodyField().activeFocus);
        keyClick(Qt.Key_Escape);
        until("and back to the tree", () => tree.activeFocus);

        // The tree itself: Escape is a no-op.
        const before = selected();
        const rows = tree.rows;
        keyClick(Qt.Key_Escape);
        wait(50);
        check("Escape in the tree does nothing", selected() === before && tree.rows === rows && tree.activeFocus);

        // Nothing here was a change.
        check("no mutation beyond the rename, no checkpoint", mutated.length <= 2 && notebook.checkpointCount() === checkpoints);

        // Search keeps its own Escape.
        const mutatedBefore = mutated.length;
        shortcut("Ctrl+F");
        until("the search field has the focus", () => searchField.focused);
        typeText("body");
        until("a query is active", () => treePane.searching);
        keyClick(Qt.Key_Escape);
        until("Escape clears the query first", () => !treePane.searching && searchField.focused);
        check("focus did not jump to the tree", !tree.activeFocus);
        keyClick(Qt.Key_Escape);
        until("a second Escape returns to the tree", () => tree.activeFocus);

        // A dialog keeps its own Escape, even from the editor.
        select(find("Second"));
        tree.forceActiveFocus();
        keyClick(Qt.Key_Delete);
        until("the delete dialog opens", () => confirmDelete.visible);
        keyClick(Qt.Key_Escape);
        until("Escape closes the dialog", () => !confirmDelete.visible);
        check("and nothing was deleted", titles().length === 3);

        // The shortcuts reference too.
        bodyField().forceActiveFocus();
        shortcut("F1");
        until("F1 opens the reference", () => shortcuts.visible);
        keyClick(Qt.Key_Escape);
        until("Escape closes it", () => !shortcuts.visible);
        check("no extra mutations from any of it", mutated.length === mutatedBefore);

        // No selection: harmless.
        selection.clearCurrentIndex();
        bodyField().forceActiveFocus();
        keyClick(Qt.Key_Escape);
        wait(50);
        check("Escape with no selection does nothing", !selection.currentIndex.valid);
    }
}
