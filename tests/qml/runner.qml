import QtQuick
import org.omatree

// The entry point the integration tests start the real application from (see
// tests/integration.rs and src/main.rs). It creates the production main.qml
// window untouched, then runs one scenario inside it. `params.json`, next to
// this file, names the scenario and carries its inputs.
Item {
    id: runner

    property var app: null
    property var scenario: null

    function readParams() {
        const request = new XMLHttpRequest();
        request.open("GET", Qt.resolvedUrl("params.json"), false);
        request.send();
        return JSON.parse(request.responseText);
    }

    function fail(message) {
        console.log("T> FAIL " + message);
        console.log("T> DONE fails=1");
        if (app) {
            app.discardOnClose = true;
            app.close();
        } else {
            Qt.exit(1);
        }
    }

    function start() {
        const params = readParams();
        const main = Qt.createComponent("qrc:/qt/qml/org/omatree/qml/main.qml");
        app = main.createObject(null);
        if (!app) {
            fail("main.qml did not load: " + main.errorString());
            return;
        }
        const component = Qt.createComponent(Qt.resolvedUrl(params.scenario + ".qml"));
        // The scenario is a child of the application window, so key events
        // sent by QtTest reach the real window.
        scenario = component.createObject(app.contentItem, {
            "app": app,
            "params": params
        });
        if (!scenario) {
            fail("scenario did not load: " + component.errorString());
            return;
        }
        scenario.execute();
    }

    Timer {
        interval: 50
        running: true
        onTriggered: runner.start()
    }

    // Nothing may hang: the scenario is over after this, one way or another.
    Timer {
        interval: 60000
        running: true
        onTriggered: runner.fail("scenario timed out")
    }
}
