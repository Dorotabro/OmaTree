import QtQuick
import org.omatree

// A notebook written by a build that stores image attachments (schema version 4), opened by the
// Qt application. `mode "edit"`: open it, change one note through the real editor and save it in
// place; the Rust side then checks that the stored images are byte-for-byte what they were.
// `mode "refuse"`: what a build from before attachments does with such a file; the error it
// reports is printed for the Rust side to check.
Base {
    function run() {
        const error = notebook.openPath(params.notebook);
        if (params.mode === "refuse") {
            check("the notebook is refused", error !== "");
            console.log("T> REFUSED " + error);
            check("nothing was opened", !notebook.hasPath());
            return;
        }
        check("the notebook opens", error === "");
        check("it has a path and is clean", notebook.hasPath() && !notebook.dirty);
        const note = find("Obrázky 🖼");
        check("the note with images is there", note.valid);
        select(note);
        check("its body shows the image references", bodyField().text.indexOf("assets/") >= 0);
        bodyField().forceActiveFocus();
        bodyField().text = params.newBody;
        check("a change is pending", notebook.dirty);
        check("save in place", notebook.save() === "");
        check("saved and clean", !notebook.dirty);
        // The other, untouched note is exactly as before.
        check("the other note is unchanged", notebook.body(find("Druhá")) === params.otherBody);
    }
}
