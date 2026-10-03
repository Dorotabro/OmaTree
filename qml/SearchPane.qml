import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

// Search results, shown in place of the tree while searching. The notebook
// does the searching and keeps the results; this only asks, displays them by
// position, and reports which one was chosen. It never holds a note index.
Item {
    id: pane

    required property var notebook

    // The user chose result `index` (Enter or a click).
    signal activateRequested(int index)
    // Escape.
    signal closeRequested

    // Reading this inside a binding re-runs it when the results change.
    readonly property int revision: notebook.searchRevision
    readonly property int count: {
        revision;
        return notebook.searchCount();
    }
    readonly property bool hasQuery: field.text.trim() !== ""

    // Focus the field with what was typed last still selected.
    function open() {
        field.forceActiveFocus();
        field.selectAll();
        search(false);
    }

    // Empties the field and forgets the results.
    function reset() {
        debounce.stop();
        field.text = "";
        notebook.clearSearch();
    }

    // Runs the search now. `fresh` puts the highlight back on the first
    // result (the query changed); otherwise it stays where it was, so a
    // refresh caused by an edit doesn't jump around.
    function search(fresh) {
        const matches = notebook.search(field.text);
        if (matches === 0)
            list.currentIndex = -1;
        else if (fresh || list.currentIndex < 0)
            list.currentIndex = 0;
        else
            list.currentIndex = Math.min(list.currentIndex, matches - 1);
    }

    function activateCurrent() {
        if (list.currentIndex >= 0)
            activateRequested(list.currentIndex);
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
            if (pane.visible && pane.hasQuery)
                debounce.restart();
        }
        function onModelReset() {
            if (pane.visible && pane.hasQuery)
                debounce.restart();
        }
    }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TextField {
            id: field

            Layout.fillWidth: true
            Layout.margins: 8
            placeholderText: qsTr("Search notes")
            color: Theme.foreground
            placeholderTextColor: Theme.mutedForeground
            selectionColor: Theme.selection
            selectedTextColor: Theme.selectionForeground
            background: Rectangle {
                color: Theme.background
                radius: 3
                border.width: 1
                border.color: field.activeFocus ? Theme.accent : Theme.border
            }

            onTextChanged: {
                if (text.trim() === "") {
                    // Nothing to search for: clear at once, no waiting.
                    debounce.stop();
                    pane.search(true);
                } else {
                    list.currentIndex = 0;
                    debounce.restart();
                }
            }
            Keys.onDownPressed: list.incrementCurrentIndex()
            Keys.onUpPressed: list.decrementCurrentIndex()
            Keys.onReturnPressed: pane.activateCurrent()
            Keys.onEnterPressed: pane.activateCurrent()
            Keys.onEscapePressed: pane.closeRequested()
        }

        Label {
            Layout.fillWidth: true
            Layout.leftMargin: 10
            Layout.rightMargin: 10
            Layout.bottomMargin: 4
            color: Theme.mutedForeground
            font.pixelSize: 12
            elide: Text.ElideRight
            text: !pane.hasQuery ? qsTr("Type to search notes") : (pane.count === 0 ? qsTr("No matching notes") : (pane.count === 1 ? qsTr("1 match") : qsTr("%1 matches").arg(pane.count)))
        }

        ListView {
            id: list

            Layout.fillWidth: true
            Layout.fillHeight: true
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
                implicitHeight: texts.implicitHeight + 12
                color: current ? Theme.selection : (hover.hovered ? Theme.surfaceRaised : "transparent")

                HoverHandler {
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
                    anchors.leftMargin: 10
                    anchors.rightMargin: 10
                    spacing: 1

                    Label {
                        Layout.fillWidth: true
                        elide: Text.ElideRight
                        color: row.current ? Theme.selectionForeground : Theme.foreground
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
                        color: row.current ? Theme.selectionForeground : Theme.mutedForeground
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
                        color: row.current ? Theme.selectionForeground : Theme.mutedForeground
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
}
