import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

// The search results, shown in place of the tree while the permanent search
// field has a query. The notebook does the searching and keeps the results;
// this only asks, displays them by position, and reports which one was
// chosen. It never holds a note index.
Item {
    id: pane

    required property var notebook
    // What the search field says.
    property string query: ""

    // The user chose result `index` (Enter or a click).
    signal activateRequested(int index)

    // Reading this inside a binding re-runs it when the results change.
    readonly property int revision: notebook.searchRevision
    readonly property int count: {
        revision;
        return notebook.searchCount();
    }
    readonly property bool active: query.trim() !== ""

    // Forgets the query's results.
    function reset() {
        debounce.stop();
        notebook.clearSearch();
        list.currentIndex = -1;
    }

    // Runs the search now. `fresh` puts the highlight back on the first
    // result (the query changed); otherwise it stays where it was, so a
    // refresh caused by an edit doesn't jump around.
    function search(fresh) {
        const matches = notebook.search(query);
        if (matches === 0)
            list.currentIndex = -1;
        else if (fresh || list.currentIndex < 0)
            list.currentIndex = 0;
        else
            list.currentIndex = Math.min(list.currentIndex, matches - 1);
    }

    // Up and Down in the field move the highlight.
    function moveBy(step) {
        if (step > 0)
            list.incrementCurrentIndex();
        else
            list.decrementCurrentIndex();
    }

    function activateCurrent() {
        if (list.currentIndex >= 0)
            activateRequested(list.currentIndex);
    }

    onQueryChanged: {
        if (!active) {
            // Nothing to search for: clear at once, no waiting.
            reset();
        } else {
            list.currentIndex = 0;
            debounce.restart();
        }
    }

    // One short pause after the last keystroke, or after the notebook
    // changed underneath an open search, then a single search.
    Timer {
        id: debounce

        interval: 120
        onTriggered: pane.search(false)
    }

    Connections {
        target: pane.notebook
        function onDocumentMutated() {
            if (pane.visible && pane.active)
                debounce.restart();
        }
    }

    // A query with no matches says so, quietly.
    Label {
        anchors.horizontalCenter: parent.horizontalCenter
        y: Ui.large
        visible: pane.active && pane.count === 0
        text: qsTr("No matching notes")
        color: Theme.mutedForeground
    }

        ListView {
            id: list

            anchors.fill: parent
            clip: true
            model: pane.count
            currentIndex: -1
            keyNavigationWraps: false
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar {}

            delegate: Rectangle {
                id: row

                required property int index
                readonly property bool current: ListView.isCurrentItem

                width: ListView.view.width
                implicitHeight: texts.implicitHeight + Ui.medium * 2
                color: current ? Qt.alpha(Theme.selection, 0.55) : (hover.over ? Qt.alpha(Theme.surfaceRaised, 0.8) : "transparent")

                // The same accent bar as the selected row of the tree.
                Rectangle {
                    width: Ui.bar
                    height: parent.height
                    color: Theme.accent
                    visible: row.current
                }

                PointerHover {
                    id: hover
                }
                TapHandler {
                    onTapped: pane.activateRequested(row.index)
                }

                ColumnLayout {
                    id: texts

                    anchors.left: parent.left
                    anchors.right: parent.right
                    anchors.verticalCenter: parent.verticalCenter
                    anchors.leftMargin: Ui.medium + Ui.small
                    anchors.rightMargin: Ui.medium
                    spacing: Ui.tiny

                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        color: Theme.foreground
                        font.weight: row.current ? Font.DemiBold : Font.Normal
                        text: {
                            pane.revision;
                            return pane.notebook.searchTitle(row.index);
                        }
                    }
                    // Where it lives, so equal titles in different branches
                    // can be told apart. Nothing for a top-level note.
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pixelSize: 12
                        color: Theme.mutedForeground
                        text: {
                            pane.revision;
                            return pane.notebook.searchPath(row.index);
                        }
                        visible: text !== ""
                    }
                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        font.pixelSize: 12
                        color: Theme.mutedForeground
                        text: {
                            pane.revision;
                            return pane.notebook.searchSnippet(row.index);
                        }
                        visible: text !== ""
                    }
                }
            }
        }
}
