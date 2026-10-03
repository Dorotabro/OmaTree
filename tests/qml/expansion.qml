import QtQuick
import org.omatree

// Scenario 5: expansion follows a moved subtree and survives save and reopen.
Base {
    function run() {
        const a = mk(null, "A", "");
        const a1 = mk(a, "A1", "");
        const a1a = mk(a1, "A1a", "");
        mk(a1a, "Leaf", "");
        mk(a, "A2", "");
        const b = mk(null, "B", "");
        mk(b, "B1", "");
        const path = tmp("expansion.omatree");
        check("save", saveAs(path) === "");

        treePane.collapseAll();
        until("collapsed", () => tree.rows === 2);
        treePane.expandSubtree(visibleRowOf("A"));
        until("recursive expand opens the subtree", () => expandedAt("A") && expandedAt("A1") && expandedAt("A1a") && visibleRowOf("Leaf") >= 0);
        check("B was not touched", !expandedAt("B"));
        check("expansion is not content", !notebook.dirty);
        treePane.collapseSubtree(visibleRowOf("A"));
        until("recursive collapse", () => tree.rows === 2);
        treePane.expandSubtree(visibleRowOf("A"));
        until("open again", () => visibleRowOf("Leaf") >= 0);

        // Move the expanded A1 under B (open, so the move happens in view).
        treePane.expandAll();
        until("everything open", () => visibleRowOf("Leaf") >= 0 && expandedAt("B"));
        const moved = notebook.moveNode(find("A1"), find("B"), 0);
        check("moved", moved.valid && notebook.data(notebook.parent(moved), 0) === "B");
        // As the app does after a move: select the node and reveal it.
        select(moved);
        treePane.reveal(moved);
        until("A1 is still open under B", () => expandedAt("A1") && expandedAt("A1a") && visibleRowOf("Leaf") >= 0);
        check("the document remembers it by node, not by place", notebook.shouldExpand(find("A1")));
        check("save", notebook.save() === "");
        check("reopen", notebook.openFile(url(path)) === "");
        until("saved expansion is restored", () => expandedAt("B") && expandedAt("A1"));
        check("the tree is as saved", titles().join("|") === "A(A2)|B(A1(A1a(Leaf)),B1)");
        check("restoring is not content", !notebook.dirty);
    }
}
