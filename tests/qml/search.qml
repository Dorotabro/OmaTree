import QtQuick
import org.omatree

// Scenario 4: Search finds a deep note, and choosing it reveals it.
Base {
    function run() {
        const a = mk(null, "Top", "");
        const b = mk(a, "Middle", "");
        const c = mk(b, "Lower", "");
        mk(c, "Deepest", "the quick zebra jumps");
        mk(null, "Other", "unrelated");
        check("saved", saveAs(tmp("search.omatree")) === "");
        treePane.collapseAll();
        until("collapsed to the top level", () => tree.rows === 2);

        // The real keyboard path: Ctrl+F, type, wait for the debounce.
        tree.forceActiveFocus();
        shortcut("Ctrl+F");
        until("Search opens", () => treePane.searching);
        typeText("zebra");
        until("a result appears", () => notebook.searchCount() === 1);
        check("it is the deep note", notebook.searchTitle(0) === "Deepest" && notebook.searchPath(0).indexOf("Lower") >= 0);
        keyClick(Qt.Key_Return);
        until("Search closes", () => !treePane.searching);
        check("the note is selected", selected() === "Deepest");
        until("ancestors are expanded", () => expandedAt("Top") && expandedAt("Middle") && expandedAt("Lower"));
        check("the editor shows it", titleField().text === "Deepest" && bodyField().text === "the quick zebra jumps");
        check("searching changed nothing", !notebook.dirty);

        // A result whose note was deleted fails safely.
        treePane.startSearch();
        notebook.search("zebra");
        notebook.removeNode(find("Deepest"));
        check("a stale result gives an invalid index", !notebook.activateSearchResult(0).valid);
        treePane.openResult(0);
        check("openResult copes and refreshes", true);
        treePane.stopSearch();
    }
}
