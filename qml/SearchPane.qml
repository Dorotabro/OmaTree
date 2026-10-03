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

        // The query line, like a command prompt: a "/" and an underline.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Ui.medium
            Layout.rightMargin: Ui.medium
            Layout.topMargin: Ui.small
            spacing: Ui.small

            Label {
                text: "/"
                color: field.activeFocus ? Theme.accent : Theme.mutedForeground
                font.family: Ui.monoFamily
                font.pixelSize: 15
            }
            TextField {
                id: field

                Layout.fillWidth: true
                leftPadding: 0
                placeholderText: qsTr("search notes")
                color: Theme.foreground
                placeholderTextColor: Theme.mutedForeground
                selectionColor: Theme.selection
                selectedTextColor: Theme.selectionForeground
                background: Rectangle {
                    color: "transparent"

                    Rectangle {
                        anchors.bottom: parent.bottom
                        width: parent.width
                        height: Ui.hairline
                        color: field.activeFocus ? Theme.accent : Theme.border
                    }
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
        }

        Label {
            Layout.fillWidth: true
            Layout.leftMargin: Ui.medium + Ui.small + 9
            Layout.rightMargin: Ui.medium
            Layout.topMargin: Ui.small
            Layout.bottomMargin: Ui.small
            color: Theme.mutedForeground
            font.family: Ui.monoFamily
            font.pixelSize: Ui.commandPixelSize
            elide: Text.ElideRight
            text: !pane.hasQuery ? qsTr("type to search notes") : (pane.count === 0 ? qsTr("no matching notes") : (pane.count === 1 ? qsTr("1 match") : qsTr("%1 matches").arg(pane.count)))
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
}
