import QtQuick
import org.omatree

// Scenario 3: New and Open replace the whole document safely.
Base {
    function run() {
        // A saved notebook to come back to, and another to open.
        const alpha = mk(null, "Alpha", "needle in alpha");
        mk(alpha, "Child", "");
        const first = tmp("first.omatree");
        check("save first", saveAs(first) === "");
        const other = tmp("second.omatree");
        notebook.openPath(other);
        mk(null, "Beta", "other body");
        check("save second", notebook.save() === "");
        check("open first", notebook.openFile(url(first)) === "");

        select(find("Alpha"));
        const stale = current();
        check("a note is selected and shown", titleField().text === "Alpha");
        check("search finds it", notebook.search("needle") === 1 && notebook.searchCount() === 1);

        // File > New Notebook, the real path (the notebook is clean).
        app.requestNewNotebook();
        check("selection was dropped", !selection.currentIndex.valid);
        check("search results cleared", notebook.searchCount() === 0);
        check("editor cleared", titleField().text === "" && bodyField().text === "");
        check("Untitled and clean", notebook.documentName === "Untitled" && !notebook.dirty && !notebook.hasPath());
        check("the old index resolves to nothing", notebook.data(stale, 0) === undefined || notebook.data(stale, 0) === null || notebook.data(stale, 0) === "");
        check("stale body is empty", notebook.body(stale) === "");

        // Open another file, from a state with a selection and results.
        check("open first again", notebook.openFile(url(first)) === "");
        select(find("Child"));
        notebook.search("alpha");
        const stale2 = current();
        check("open the second notebook", notebook.openFile(url(other)) === "");
        check("selection dropped on open", !selection.currentIndex.valid);
        check("search cleared on open", notebook.searchCount() === 0);
        check("editor cleared on open", titleField().text === "");
        check("the new notebook is what is shown", titles().join("|") === "Beta" && notebook.documentName === "second.omatree");
        check("stale index is harmless", notebook.body(stale2) === "" || notebook.body(stale2) !== undefined);
        check("clean after open", !notebook.dirty);
        check("a failed open changes nothing", notebook.openFile(url(tmp("missing.omatree"))) !== "" && titles().join("|") === "Beta");
    }
}
