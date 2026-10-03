import QtQuick
import org.omatree

// Scenario 9: the semantic theme follows a (temporary, Omarchy-like) palette
// file, live, without touching the document.
Base {
    function palette(background, accent) {
        return "background = \"" + background + "\"\nforeground = \"#d0d0d0\"\naccent = \"" + accent + "\"\n";
    }
    function writeColors(text) {
        const request = new XMLHttpRequest();
        request.open("PUT", url(params.colors), false);
        request.send(text);
    }

    function run() {
        check("the app follows the test palette", Theme.usingOmarchy());
        until("initial colours", () => Theme.background.toLowerCase() === "#101820" && Theme.accent.toLowerCase() === "#ff8800");

        const note = mk(null, "Note", "# Heading\n\nbody\n");
        check("save", saveAs(tmp("theme.omatree")) === "");
        select(find("Note"));
        const before = Theme.accent;

        writeColors(palette("#f0f0e8", "#0044cc"));
        until("polling picks up the new palette", () => Theme.background.toLowerCase() === "#f0f0e8" && Theme.accent.toLowerCase() === "#0044cc", 8000);
        check("the accent changed", Theme.accent !== before);
        check("document untouched", !notebook.dirty && selected() === "Note");

        tree.forceActiveFocus();
        shortcut("Ctrl+E");
        until("Preview still works", () => editorPane.previewing && previewField() !== null && previewField().getText(0, previewField().length).indexOf("Heading") >= 0);
        check("still the same note", selected() === "Note" && !notebook.dirty);
    }
}
