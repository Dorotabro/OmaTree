import QtQuick
import org.omatree

// Scenario 6: Trash and a checkpoint restore.
Base {
    function run() {
        const x = mk(null, "X", "x body");
        const y = mk(x, "Y", "y body");
        mk(y, "Z", "z body");
        mk(null, "Keep", "");
        check("save", saveAs(tmp("recovery.omatree")) === "");
        const staleX = find("X");

        check("move X to Trash", notebook.removeNode(find("X")));
        check("it is gone from the tree", titles().join("|") === "Keep");
        check("Trash has one entry", notebook.trashCount() === 1 && notebook.trashTitle(0) === "X" && notebook.trashSize(0) === 3);
        check("deleting took a checkpoint", notebook.checkpointCount() >= 1);
        check("a stale index is harmless", notebook.body(staleX) === "" || notebook.body(staleX) !== undefined);

        check("restore from Trash", notebook.restoreTrash(0) === "");
        check("hierarchy returns", titles().join("|") === "X(Y(Z))|Keep");
        check("bodies return", notebook.body(find("Z")) === "z body" && notebook.body(find("X")) === "x body");
        check("the entry left Trash", notebook.trashCount() === 0);
        check("no duplicates", notebook.rowCount(tree.rootIndex) === 2);
        select(find("Y"));
        check("the restored note can be selected and shown", titleField().text === "Y" && bodyField().text === "y body");

        // One checkpoint restore: back to the state before the Trash restore.
        const before = notebook.checkpointCount();
        mk(null, "Extra", "");
        check("checkpoint restore", notebook.restoreCheckpoint(0) === "");
        check("selection dropped on the reset", !selection.currentIndex.valid);
        check("the notebook is the checkpoint's", titles().join("|") === "Keep");
        check("restoring checkpointed the state it replaced", notebook.checkpointCount() >= before);
    }
}
