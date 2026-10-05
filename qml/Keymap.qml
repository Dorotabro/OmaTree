pragma Singleton

import QtQuick

// The application-wide shortcuts, defined once, and the text that shows them.
//
// An action with a standard platform shortcut uses Qt's StandardKey, so the
// platform's own convention applies. The others are written with `Ctrl`,
// which Qt turns into Command on macOS by itself. Everything shown to the
// user (hints, menus, the F1 reference) is derived from these same
// sequences by Qt, so it names the keys that actually work on the running
// platform: "Ctrl+N" on Linux, "⌘N" on macOS.
//
// Save As has no StandardKey on X11 or Windows, so it is spelled out.
QtObject {
    id: keymap

    readonly property var newNote: [StandardKey.New]
    readonly property var newChild: ["Ctrl+Shift+N"]
    readonly property var open: [StandardKey.Open]
    readonly property var save: [StandardKey.Save]
    readonly property var saveAs: ["Ctrl+Shift+S"]
    readonly property var find: [StandardKey.Find]
    readonly property var togglePreview: ["Ctrl+E"]
    readonly property var moveUp: ["Alt+Up"]
    readonly property var moveDown: ["Alt+Down"]
    readonly property var rename: ["F2"]
    readonly property var help: ["F1"]
    // Where the platform expects a window to be closed with the keyboard
    // and OmaTree has no other key for it (macOS only; see main.qml). Spelled
    // out, because StandardKey.Close depends on the platform theme.
    readonly property var closeWindow: ["Ctrl+W"]

    readonly property bool mac: Qt.platform.os === "osx"

    // A shortcut that never fires: it only knows how to print itself.
    readonly property Component printer: Component {
        Shortcut {
            enabled: false
        }
    }

    // How the platform writes `sequences`.
    function text(sequences) {
        const probe = printer.createObject(null, {
            "sequences": sequences
        });
        const written = probe.nativeText;
        probe.destroy();
        return written;
    }

    // The platform's name for the Ctrl key (the Command key on macOS) in
    // front of `what`: "Ctrl+Click", "⌘Click".
    function withCtrl(what) {
        const key = text(["Ctrl+A"]);
        return key.slice(0, key.length - 1) + what;
    }

    // "Ctrl+X / C / V" on Linux; every key spelled out on macOS, where the
    // symbol alone would be hard to read.
    function cutCopyPaste() {
        const x = text(["Ctrl+X"]);
        if (mac)
            return x + " / " + text(["Ctrl+C"]) + " / " + text(["Ctrl+V"]);
        return x + " / C / V";
    }
}
