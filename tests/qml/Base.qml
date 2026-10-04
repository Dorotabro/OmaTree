import QtQuick
import QtQuick.Controls
import QtTest
import org.omatree

// What every scenario shares: finding the application's objects, bounded
// waits, and small helpers for building notebooks. A scenario derives from
// this and implements `run()`.
TestCase {
    id: base

    property var app: null
    property var params: ({})
    property int fails: 0
    // The scenario closes the window itself, as part of what it tests.
    property bool closesItself: false

    name: "scenario"
    when: false

    // --- Results ---------------------------------------------------------
    function check(what, condition) {
        if (!condition)
            fails++;
        console.log("T> " + (condition ? "PASS " : "FAIL ") + what);
        return condition;
    }

    // Waits for `condition()` for at most `ms`, polling; never indefinitely.
    function until(what, condition, ms) {
        const limit = Date.now() + (ms === undefined ? 5000 : ms);
        while (!condition() && Date.now() < limit)
            wait(20);
        return check(what, condition());
    }

    function execute() {
        try {
            bind();
            // Shortcuts only fire in the active window.
            app.requestActivate();
            const limit = Date.now() + 3000;
            while (!app.active && Date.now() < limit)
                wait(20);
            check("the application window is active", app.active);
            // The test process is started with libtest's own arguments, which
            // OmaTree takes for notebook paths and complains about at startup.
            // That message is no part of any scenario: close it first.
            const startup = findObj(app, o => o.hasOwnProperty("standardButtons") && o.title === "OmaTree");
            if (startup && startup.visible) {
                startup.close();
                const settle = Date.now() + 2000;
                while (startup.visible && Date.now() < settle)
                    wait(20);
                wait(50);
            }
            run();
        } catch (e) {
            fails++;
            console.log("T> FAIL exception: " + e);
        }
        console.log("T> DONE fails=" + fails);
        // Closing the last window ends the application.
        if (!closesItself) {
            app.discardOnClose = true;
            app.close();
        }
    }

    // --- Finding the application's parts -----------------------------------
    function findObj(obj, predicate) {
        if (!obj)
            return null;
        if (predicate(obj))
            return obj;
        const lists = [obj.data, obj.contentData, obj.children];
        for (let l = 0; l < lists.length; ++l) {
            const kids = lists[l];
            if (!kids)
                continue;
            for (let i = 0; i < kids.length; ++i) {
                const found = findObj(kids[i], predicate);
                if (found)
                    return found;
            }
        }
        return null;
    }

    property var notebook: null
    property var selection: null
    property var treePane: null
    property var editorPane: null
    property var tree: null
    property var searchPane: null
    property var searchField: null
    property var confirmDelete: null
    property var shortcuts: null
    property var recovery: null

    function bind() {
        notebook = findObj(app, o => o.hasOwnProperty("shouldExpand") && o.hasOwnProperty("openPath"));
        selection = findObj(app, o => o.hasOwnProperty("currentIndex") && o.hasOwnProperty("setCurrentIndex") && o.hasOwnProperty("clearCurrentIndex"));
        treePane = findObj(app, o => o.hasOwnProperty("startSearch") && o.hasOwnProperty("openResult"));
        editorPane = findObj(app, o => o.hasOwnProperty("toggleMode") && o.hasOwnProperty("resetMode"));
        tree = findObj(treePane, o => o.hasOwnProperty("rows") && o.hasOwnProperty("expandToIndex"));
        searchPane = findObj(treePane, o => o.hasOwnProperty("reset") && o.hasOwnProperty("activateCurrent"));
        searchField = findObj(treePane, o => o.hasOwnProperty("focusAll") && o.hasOwnProperty("moveRequested"));
        confirmDelete = findObj(app, o => o.hasOwnProperty("askAboutSelection"));
        shortcuts = findObj(app, o => o.hasOwnProperty("sections"));
        recovery = findObj(app, o => o.hasOwnProperty("trashCount") && o.hasOwnProperty("checkpointCount"));
        if (!searchField || !notebook || !selection || !treePane || !editorPane || !tree || !confirmDelete || !shortcuts)
            throw new Error("could not find the application's parts");
    }

    // The editor's two text controls, as they are on screen.
    function titleField() {
        return findObj(editorPane, o => o.hasOwnProperty("echoMode") && o.hasOwnProperty("placeholderText"));
    }
    function bodyField() {
        return findObj(editorPane, o => o.hasOwnProperty("wrapMode") && o.hasOwnProperty("placeholderText") && !o.hasOwnProperty("echoMode"));
    }
    function previewField() {
        return findObj(editorPane, o => o.hasOwnProperty("textFormat") && o.readOnly === true);
    }
    function dialogOf(predicate) {
        return findObj(app, predicate);
    }

    // --- Notebooks -----------------------------------------------------------
    function tmp(name) {
        return params.dir + "/" + name;
    }

    // Appends a note (top level when `parent` is null) and names it.
    function mk(parent, title, body) {
        const index = parent === null ? notebook.createDefaultRoot() : notebook.createDefaultChild(parent);
        notebook.rename(index, title);
        if (body)
            notebook.setBody(index, body);
        return index;
    }

    // A fresh index for the note called `title`, searched through the whole
    // notebook (collapsed branches too); null if there is none.
    function find(title, parent) {
        const from = parent === undefined ? tree.rootIndex : parent;
        for (let r = 0; r < notebook.rowCount(from); ++r) {
            const index = notebook.index(r, 0, from);
            if (notebook.data(index, 0) === title)
                return index;
            const deeper = find(title, index);
            if (deeper)
                return deeper;
        }
        return null;
    }
    function titles(parent) {
        const out = [];
        const p = parent === undefined ? tree.rootIndex : parent;
        for (let r = 0; r < notebook.rowCount(p); ++r) {
            const index = notebook.index(r, 0, p);
            const kids = titles(index);
            out.push(kids.length ? notebook.data(index, 0) + "(" + kids.join(",") + ")" : notebook.data(index, 0));
        }
        return out;
    }

    function select(index) {
        selection.setCurrentIndex(index, ItemSelectionModel.ClearAndSelect);
        wait(30);
    }
    // A real copy of an index: `selection.currentIndex` is a live reference
    // that changes with the selection.
    function snap(index) {
        return notebook.index(index.row, index.column, notebook.parent(index));
    }
    function current() {
        return snap(selection.currentIndex);
    }
    function selected() {
        return selection.currentIndex.valid ? notebook.data(selection.currentIndex, 0) : "";
    }
    function url(path) {
        return "file://" + path;
    }
    // A saved, file-backed copy of the current document.
    function saveAs(path) {
        return notebook.saveAs(path, true);
    }
    // Application-wide shortcuts (the `Shortcut` items of main.qml): QtTest's
    // synthetic key events do not go through Qt's shortcut map, so this fires
    // the one bound to `sequence`, which checks that it exists and is wired
    // to its action. Keys handled by items (Delete, Escape, Shift+F10, the
    // editors) are sent as real key events instead.
    function shortcut(sequence) {
        const found = findObj(app, o => o.hasOwnProperty("sequence") && o.hasOwnProperty("activated") && o.sequence.toString() === sequence);
        if (!check("a shortcut " + sequence + " exists", found !== null))
            return;
        found.activated();
        wait(30);
    }
    // A footer command by its label.
    function command(text) {
        return findObj(treePane, o => o.hasOwnProperty("text") && o.text === text && o.hasOwnProperty("hint"));
    }
    // The N or Y shortcut of a Yes / No dialog. Fired directly, like
    // `shortcut`; its `enabled` says whether the keystroke would act at all.
    function dialogShortcut(dialog, key) {
        return key === "N" ? dialog.noShortcut : dialog.yesShortcut;
    }
    function typeText(text) {
        for (let i = 0; i < text.length; ++i)
            keyClick(text[i]);
    }
    function visibleRowOf(title) {
        for (let r = 0; r < tree.rows; ++r) {
            if (notebook.data(tree.index(r, 0), 0) === title)
                return r;
        }
        return -1;
    }
    function expandedAt(title) {
        const r = visibleRowOf(title);
        return r >= 0 && tree.isExpanded(r);
    }
}
