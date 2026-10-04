import QtQuick
import QtQuick.Controls
import org.omatree

// A title being typed is never silently lost: it is committed against its own
// note before that note is left, and a refused one blocks the move.
Base {
    function errorDialog() {
        return findObj(app, o => o.hasOwnProperty("standardButtons") && o.title === "OmaTree");
    }
    function typeTitle(text) {
        titleField().forceActiveFocus();
        titleField().selectAll();
        typeText(text);
    }
    // What a click on a tree row does: select it, then focus the tree.
    function clickRow(title) {
        selection.setCurrentIndex(find(title), ItemSelectionModel.ClearAndSelect);
        tree.forceActiveFocus();
    }

    function run() {
        mk(null, "Alpha", "a body");
        mk(null, "Beta", "b body");
        const path = tmp("titles.omatree");
        check("save", saveAs(path) === "");

        // 1. Typed, then another note chosen (the title never lost focus first).
        select(find("Alpha"));
        until("editor shows Alpha", () => titleField().text === "Alpha");
        typeTitle("AlphaOne");
        selection.setCurrentIndex(find("Beta"), ItemSelectionModel.ClearAndSelect);
        until("the rename persisted", () => titles().indexOf("AlphaOne") === 0);
        check("and the other note is shown", selected() === "Beta" && titleField().text === "Beta");
        tree.forceActiveFocus();

        // 2. Search activation: the same, through the real result path.
        select(find("AlphaOne"));
        until("editor shows AlphaOne", () => titleField().text === "AlphaOne");
        typeTitle("AlphaTwo");
        notebook.search("beta");
        treePane.openResult(0);
        until("rename persisted through a search result", () => titles().indexOf("AlphaTwo") === 0 && selected() === "Beta");

        // 2b. Keyboard navigation in the tree, with the title still focused
        // first, then Escape: nothing is lost.
        select(find("AlphaTwo"));
        until("editor shows AlphaTwo", () => titleField().text === "AlphaTwo");
        typeTitle("AlphaThree");
        keyClick(Qt.Key_Escape);
        until("Escape committed it and focused the tree", () => tree.activeFocus && titles().indexOf("AlphaThree") === 0);
        keyClick(Qt.Key_Down);
        until("and the arrows move on", () => selected() === "Beta");

        // 3. Ctrl+N.
        select(find("AlphaThree"));
        until("editor shows AlphaThree", () => titleField().text === "AlphaThree");
        typeTitle("AlphaFour");
        shortcut("Ctrl+N");
        until("Ctrl+N: new note selected", () => selected() === "New note");
        check("Ctrl+N: the rename persisted", titles().indexOf("AlphaFour") === 0);
        check("Ctrl+N: the new note starts in Edit with the title focused", !editorPane.previewing && titleField().activeFocus);

        // 4. Ctrl+Shift+N.
        select(find("AlphaFour"));
        until("editor shows AlphaFour", () => titleField().text === "AlphaFour");
        typeTitle("AlphaFive");
        shortcut("Ctrl+Shift+N");
        until("Ctrl+Shift+N: a child exists", () => notebook.rowCount(find("AlphaFive")) === 1);
        check("Ctrl+Shift+N: the rename persisted", titles()[0].indexOf("AlphaFive") === 0);

        // 8-10. A refused title blocks the move and stays correctable.
        select(find("Beta"));
        until("editor shows Beta", () => titleField().text === "Beta");
        typeTitle("AlphaFive"); // taken by a sibling
        selection.setCurrentIndex(find("AlphaFive"), ItemSelectionModel.ClearAndSelect);
        until("a conflicting title shows the message", () => errorDialog().visible);
        check("the move was refused: still on Beta", selected() === "Beta");
        check("the typed text is still there", titleField().text === "AlphaFive");
        check("the notebook title is unchanged", notebook.data(current(), 0) === "Beta");
        errorDialog().close();
        until("message closed", () => !errorDialog().visible);
        // Creating a note is refused the same way.
        const roots = notebook.rowCount(tree.rootIndex);
        shortcut("Ctrl+N");
        wait(100);
        check("Ctrl+N is refused while the title is invalid", notebook.rowCount(tree.rootIndex) === roots);
        until("the message again", () => errorDialog().visible);
        errorDialog().close();
        until("closed", () => !errorDialog().visible);
        // Empty is refused too.
        typeTitle("");
        titleField().text = "";
        selection.setCurrentIndex(find("AlphaFive"), ItemSelectionModel.ClearAndSelect);
        until("an empty title shows the message", () => errorDialog().visible);
        check("and blocks the move", selected() === "Beta" && titleField().text === "");
        errorDialog().close();
        until("message closed again", () => !errorDialog().visible);
        // Corrected, it commits and the move works.
        titleField().forceActiveFocus();
        titleField().text = "Beta Two";
        selection.setCurrentIndex(find("AlphaFive"), ItemSelectionModel.ClearAndSelect);
        until("corrected: the rename persisted and the move happened", () => selected() === "AlphaFive" && titles().join("|").indexOf("Beta Two") >= 0);

        // 5. New Notebook: committed, then saved by the normal leave logic.
        select(find("Beta Two"));
        until("editor shows Beta Two", () => titleField().text === "Beta Two");
        typeTitle("Gamma");
        app.requestNewNotebook();
        check("New Notebook went ahead", notebook.documentName === "Untitled");
        check("open the old file again", notebook.openFile(url(path)) === "");
        check("the pending rename was saved before New", titles().join("|").indexOf("Gamma") >= 0);

        // 6. Open: an invalid pending title stops it; a valid one is committed.
        select(find("Gamma"));
        until("editor shows Gamma", () => titleField().text === "Gamma");
        titleField().text = "";
        app.requestOpen();
        until("an invalid title stops Open", () => errorDialog().visible);
        errorDialog().close();
        check("no file dialog opened", !findObj(app, o => o.hasOwnProperty("fileMode") && o.visible));
        // (A valid pending title is committed by the same first line of
        // Open as of New, above; the native file dialog it would then open is
        // deliberately not started in this headless suite.)
    }
}
