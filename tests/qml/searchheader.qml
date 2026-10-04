import QtQuick
import org.omatree

// The permanent search header: always there, tree below when the query is
// empty, results below when it is not, and lined up with the editor's header.
Base {
    function separators() {
        const found = [];
        findObj(app, o => {
            if (o.objectName === "headerSeparator" && found.indexOf(o) < 0)
                found.push(o);
            return false;
        });
        return found;
    }
    function bottomOf(item) {
        return item.mapToItem(null, 0, item.height).y;
    }

    function run() {
        const top = mk(null, "Top", "");
        const mid = mk(top, "Middle", "");
        mk(mid, "Deep", "the quick zebra");
        mk(null, "Other", "unrelated");
        check("save", saveAs(tmp("searchheader.omatree")) === "");
        select(find("Other"));
        const checkpoints = notebook.checkpointCount();
        const mutated = [];
        notebook.documentMutated.connect(() => mutated.push(1));

        // 1-2: always there; an empty query shows the tree.
        check("the search field exists and is visible", searchField.visible && searchField.height > 0);
        check("an empty query shows the tree, not results", tree.visible && !searchPane.visible && !treePane.searching);

        // Alignment with the editor header (approximate: a small tolerance).
        const lines = separators();
        check("two header separators exist", lines.length === 2);
        if (lines.length === 2)
            check("the separators line up", Math.abs(bottomOf(lines[0]) - bottomOf(lines[1])) <= 1.5);

        // 3: Ctrl+F focuses the field.
        tree.forceActiveFocus();
        shortcut("Ctrl+F");
        until("Ctrl+F focuses the field", () => searchField.focused);

        // 4: typing shows results in the body, the field stays.
        typeText("zebra");
        until("typing shows results", () => treePane.searching && notebook.searchCount() === 1 && searchPane.visible && !tree.visible);
        check("the field is still there", searchField.visible && searchField.text === "zebra");

        // 5: Escape clears and restores the tree; the selection is unchanged.
        keyClick(Qt.Key_Escape);
        until("Escape clears the query and restores the tree", () => searchField.text === "" && tree.visible && !searchPane.visible);
        check("the selection is unchanged", selected() === "Other");
        check("focus stays in the field after clearing", searchField.focused);
        keyClick(Qt.Key_Escape);
        until("a second Escape returns to the tree", () => tree.activeFocus);

        // No-match message.
        shortcut("Ctrl+F");
        typeText("qqqq");
        until("no results", () => treePane.searching && notebook.searchCount() === 0);
        keyClick(Qt.Key_Escape);
        until("cleared", () => searchField.text === "");

        // 6-7: activating a result clears Search, reveals and selects.
        treePane.collapseAll();
        until("collapsed", () => tree.rows === 2);
        shortcut("Ctrl+F");
        typeText("zebra");
        until("a result", () => notebook.searchCount() === 1);
        keyClick(Qt.Key_Return);
        until("Enter clears the query, shows the tree", () => searchField.text === "" && tree.visible);
        check("the deep note is selected", selected() === "Deep");
        until("and revealed", () => expandedAt("Top") && expandedAt("Middle") && visibleRowOf("Deep") >= 0);

        // 8-10: none of it was content.
        check("no dirty state, no mutation, no checkpoint", !notebook.dirty && mutated.length === 0 && notebook.checkpointCount() === checkpoints);
        wait(1300);
        check("and nothing was autosaved", !notebook.dirty && mutated.length === 0);

        // 11-12: the footer.
        check("the footer has no SEARCH", command("search") === null);
        check("the footer has + NOTE, + CHILD and FILE", command("note") !== null && command("child") !== null && command("file") !== null);
        const before = tree.rows;
        command("note").forceActiveFocus();
        keyClick(Qt.Key_Return);
        until("+ NOTE creates a note", () => tree.rows === before + 1);
        command("file").forceActiveFocus();
        keyClick(Qt.Key_Return);
        const menu = findObj(treePane, o => o.hasOwnProperty("popup") && o.hasOwnProperty("contentModel") && o.visible);
        until("FILE opens its menu", () => menu !== null && menu.visible);
        keyClick(Qt.Key_Escape);

        // 13: F1.
        tree.forceActiveFocus();
        shortcut("F1");
        until("F1 opens the shortcuts", () => shortcuts.visible);
        keyClick(Qt.Key_Escape);
        until("and Escape closes them", () => !shortcuts.visible);

        // 14: New and Open clear the query.
        shortcut("Ctrl+F");
        typeText("zebra");
        until("a query is active", () => treePane.searching);
        check("save before replacing", notebook.save() === "");
        app.requestNewNotebook();
        check("New Notebook clears the query", searchField.text === "" && !treePane.searching && tree.visible);
        shortcut("Ctrl+F");
        typeText("x");
        until("a query again", () => treePane.searching);
        check("open", notebook.openFile(url(tmp("searchheader.omatree"))) === "");
        check("Open clears the query", searchField.text === "" && !treePane.searching && tree.visible);
    }
}
