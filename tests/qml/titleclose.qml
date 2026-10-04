import QtQuick
import org.omatree

// Closing the window with a title still being typed: it is committed before
// the save/leave logic, so it reaches the file. The Rust side reads the file.
Base {
    closesItself: true

    function run() {
        mk(null, "Note", "body");
        check("save", saveAs(tmp("titleclose.omatree")) === "");
        select(find("Note"));
        until("editor shows Note", () => titleField().text === "Note");
        titleField().forceActiveFocus();
        titleField().text = "Omega";
        app.discardOnClose = false;
        app.close();
    }
}
