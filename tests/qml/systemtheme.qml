import QtQuick
import org.omatree

// The built-in theme: with no Omarchy theme in use, the palette follows the
// system colour scheme, live, on every platform. Off Linux there is no
// Omarchy provider at all, even when an Omarchy-like colours file exists
// where Linux would look for one (`params.omarchyLike`).
Base {
    readonly property string darkBackground: "#252a33"
    readonly property string lightBackground: "#f2f4f7"
    readonly property string darkAccent: "#82b8c8"
    readonly property string lightAccent: "#557f9d"

    function run() {
        const linux = Qt.platform.os === "linux";
        check("no Omarchy theme is in use", !Theme.usingOmarchy());
        if (!linux)
            check("there is no Omarchy location to watch off Linux", !Theme.watchesOmarchy());

        const note = mk(null, "Note", "body");
        check("save", saveAs(tmp("systemtheme.omatree")) === "");
        select(find("Note"));

        // The palette follows the scheme the application is told about.
        Theme.setSystemScheme(Qt.Dark);
        until("dark system: OmaTree Dark", () => Theme.background.toLowerCase() === darkBackground && Theme.accent.toLowerCase() === darkAccent);
        Theme.setSystemScheme(Qt.Light);
        until("light system: OmaTree Light", () => Theme.background.toLowerCase() === lightBackground && Theme.accent.toLowerCase() === lightAccent);
        Theme.setSystemScheme(0);
        until("an unknown scheme: OmaTree Dark", () => Theme.background.toLowerCase() === darkBackground);

        // And the application passes the system's own scheme on, live. Qt's
        // headless platform (offscreen) reports no scheme and ignores an
        // override; a real platform plugin takes it, so there it is checked.
        Qt.styleHints.colorScheme = Qt.Light;
        wait(100);
        if (Qt.styleHints.colorScheme === Qt.Light) {
            until("the system turns light: the palette follows", () => Theme.background.toLowerCase() === lightBackground);
            Qt.styleHints.colorScheme = Qt.Dark;
            until("the system turns dark: the palette follows", () => Theme.background.toLowerCase() === darkBackground);
            Qt.styleHints.colorScheme = Qt.Unknown;
        } else {
            console.log("T> NOTE this platform cannot override the system colour scheme (" + Qt.platform.pluginName + "); the live link is not checked");
        }
        check("still no Omarchy theme", !Theme.usingOmarchy());
        check("document untouched", !notebook.dirty && selected() === "Note");
    }
}
