import QtQuick
import org.omatree

// An expanded subtree moved into a collapsed parent is, once that path is
// revealed, exactly as expanded as the document recorded it: no more, no less.
Base {
    function run() {
        // Mover (open) > Open1 (open) > Deep (open) > Leaf; Open1 > Shut (closed) > Hidden
        const mover = mk(null, "Mover", "");
        const open1 = mk(mover, "Open1", "");
        const deep = mk(open1, "Deep", "");
        mk(deep, "Leaf", "");
        const shut = mk(open1, "Shut", "");
        mk(shut, "Hidden", "");
        // A collapsed destination.
        const dest = mk(null, "Dest", "");
        mk(dest, "Inner", "");
        const path = tmp("moveexpanded.omatree");
        check("save", saveAs(path) === "");

        treePane.expandAll();
        until("all open", () => visibleRowOf("Hidden") >= 0);
        // Close just "Shut" and the destination.
        tree.collapse(visibleRowOf("Shut"));
        tree.collapse(visibleRowOf("Dest"));
        until("Shut and Dest closed", () => !expandedAt("Shut") && !expandedAt("Dest") && visibleRowOf("Hidden") < 0);
        check("the document agrees about Shut", !notebook.shouldExpand(find("Shut")));
        check("and about Open1", notebook.shouldExpand(find("Open1")));
        check("clean so far", !notebook.dirty);
        const mutated = [];
        notebook.documentMutated.connect(() => mutated.push(1));
        const checkpoints = notebook.checkpointCount();

        // Move the open subtree into the collapsed Dest.
        const moved = notebook.moveNode(find("Open1"), find("Dest"), 0);
        check("moved under Dest", moved.valid && notebook.data(notebook.parent(moved), 0) === "Dest");
        check("Dest stays collapsed: the subtree is out of sight", !expandedAt("Dest") && visibleRowOf("Open1") < 0);

        // Reveal the destination path, as selecting the moved note does.
        treePane.reveal(find("Open1"));
        check("the moved node is open immediately", expandedAt("Open1"));
        check("its open descendant is open", expandedAt("Deep") && visibleRowOf("Leaf") >= 0);
        check("its closed descendant stays closed", visibleRowOf("Shut") >= 0 && !expandedAt("Shut") && visibleRowOf("Hidden") < 0);

        // The recorded state is untouched, and nothing counts as a change.
        check("Open1 still recorded open", notebook.shouldExpand(find("Open1")));
        check("Deep still recorded open", notebook.shouldExpand(find("Deep")));
        check("Shut still recorded closed", !notebook.shouldExpand(find("Shut")));
        // Only the move itself counted as a change (one mutation, one checkpoint);
        // re-applying expansion added none.
        check("restoring is not content", mutated.length === 1 && notebook.checkpointCount() === checkpoints + 1);

        // The same, after saving and reopening.
        check("save", notebook.save() === "");
        const saved = [visibleRowOf("Open1") >= 0, expandedAt("Open1"), expandedAt("Deep"), expandedAt("Shut"), visibleRowOf("Hidden") >= 0];
        check("reopen", notebook.openFile(url(path)) === "");
        until("the saved layout is back", () => expandedAt("Dest") && expandedAt("Open1"));
        const again = [visibleRowOf("Open1") >= 0, expandedAt("Open1"), expandedAt("Deep"), expandedAt("Shut"), visibleRowOf("Hidden") >= 0];
        check("same visible state after reopen", saved.join() === again.join());
        check("reopening is clean", !notebook.dirty);
    }
}
