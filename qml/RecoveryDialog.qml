import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

// Trash and checkpoints of the open notebook. Entries are addressed by their
// position in the lists the model exposes (newest first), never by node id.
ThemedDialog {
    id: dialog

    required property var notebook

    // Restoring failed; `message` is ready to show the user.
    signal failed(string message)
    // A Trash entry or checkpoint was restored.
    signal restored

    // Reading this inside a binding makes it re-run when Trash or the
    // checkpoints change.
    readonly property int revision: notebook.recoveryRevision
    readonly property int trashCount: {
        revision;
        return notebook.trashCount();
    }
    readonly property int checkpointCount: {
        revision;
        return notebook.checkpointCount();
    }

    function when(seconds) {
        return new Date(seconds * 1000).toLocaleString(Qt.locale(), Locale.ShortFormat);
    }

    function finish(error) {
        if (error !== "") {
            failed(error);
        } else {
            close();
            restored();
        }
    }

    parent: Overlay.overlay
    x: Math.round((parent.width - width) / 2)
    y: Math.round((parent.height - height) / 2)
    width: Math.min(480, parent.width - 48)
    height: Math.min(380, parent.height - 48)
    modal: true
    title: qsTr("Recovery")
    standardButtons: Dialog.Close
    // Always start on Trash, with the first row current (none if empty).
    onAboutToShow: {
        tabs.currentIndex = 0;
        trashList.reset();
        checkpointList.reset();
    }
    // The list has the keyboard on open when it has entries; otherwise the
    // safe default (Close) does.
    initialFocusItem: trashCount > 0 ? trashList : null

    function currentList() {
        return tabs.currentIndex === 0 ? trashList : checkpointList;
    }

    contentItem: ColumnLayout {
        spacing: 8

        TabBar {
            id: tabs

            Layout.fillWidth: true
            // Left / Right on the header switch tab; the list never does.
            // The destination list starts on its first row.
            onCurrentIndexChanged: {
                // A key on the header keeps the keyboard on the header, on the
                // tab it has just chosen.
                const onHeader = tabs.activeFocus || tabs.contentChildren.some(t => t.activeFocus);
                dialog.currentList().reset();
                if (onHeader && tabs.currentItem)
                    Qt.callLater(() => tabs.currentItem.forceActiveFocus(Qt.TabFocusReason));
            }
            background: Rectangle {
                color: "transparent"
                Rectangle {
                    anchors.bottom: parent.bottom
                    width: parent.width
                    height: 1
                    color: Theme.border
                }
            }

            RecoveryTab {
                text: qsTr("Trash (%1)").arg(dialog.trashCount)
            }
            RecoveryTab {
                text: qsTr("Checkpoints (%1)").arg(dialog.checkpointCount)
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: tabs.currentIndex

            // Trash
            Item {
                RecoveryList {
                    id: trashList
                    objectName: "trashList"

                    model: dialog.trashCount
                    onRestoreCurrent: dialog.finish(dialog.notebook.restoreTrash(currentIndex))

                    delegate: RecoveryRow {
                        required property int index

                        width: trashList.width
                        title: {
                            dialog.revision;
                            return dialog.notebook.trashTitle(index);
                        }
                        detail: {
                            dialog.revision;
                            const size = dialog.notebook.trashSize(index);
                            const date = dialog.when(dialog.notebook.trashDeletedAt(index));
                            return size === 1 ? qsTr("Deleted %1 · 1 note").arg(date) : qsTr("Deleted %1 · %2 notes").arg(date).arg(size);
                        }
                        onRestoreClicked: dialog.finish(dialog.notebook.restoreTrash(index))
                    }
                }
                Label {
                    anchors.centerIn: parent
                    visible: dialog.trashCount === 0
                    text: qsTr("Trash is empty")
                    color: Theme.mutedForeground
                }
            }

            // Checkpoints
            Item {
                RecoveryList {
                    id: checkpointList
                    objectName: "checkpointList"

                    model: dialog.checkpointCount
                    onRestoreCurrent: dialog.finish(dialog.notebook.restoreCheckpoint(currentIndex))

                    delegate: RecoveryRow {
                        required property int index

                        width: checkpointList.width
                        title: {
                            dialog.revision;
                            return dialog.notebook.checkpointReason(index);
                        }
                        detail: {
                            dialog.revision;
                            return dialog.when(dialog.notebook.checkpointCreatedAt(index));
                        }
                        onRestoreClicked: dialog.finish(dialog.notebook.restoreCheckpoint(index))
                    }
                }
                Label {
                    anchors.centerIn: parent
                    visible: dialog.checkpointCount === 0
                    text: qsTr("No checkpoints yet")
                    color: Theme.mutedForeground
                }
            }
        }
    }

    // A tab: muted text, with the accent underline marking the current one.
    component RecoveryTab: TabButton {
        id: tab

        // The header is one stop for Tab (on the current tab); Left / Right
        // choose the other.
        focusPolicy: checked ? Qt.StrongFocus : Qt.NoFocus
        contentItem: Label {
            text: tab.text
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            // The same look as the Edit / Preview switch.
            font.family: Ui.monoFamily
            font.pixelSize: Ui.commandPixelSize
            font.letterSpacing: 0.6
            font.capitalization: Font.AllUppercase
            color: tab.checked ? Theme.foreground : (tab.hovered ? Theme.accent : Theme.mutedForeground)
        }
        background: Rectangle {
            color: "transparent"
            Rectangle {
                anchors.bottom: parent.bottom
                width: parent.width
                height: Ui.bar
                color: tab.checked ? Theme.accent : "transparent"
            }
        }
    }

    // A list of either kind. The ListView's own currentIndex is the one
    // selection: Up / Down move it (bounded, no wrap), Enter / Space restore
    // that row through the same path as its RESTORE command, and the row it
    // marks is the row they restore. The mark shows only while the list has
    // the keyboard, so it never competes with another focus mark.
    component RecoveryList: ListView {
        id: list

        signal restoreCurrent

        // First row current, or nothing when empty.
        // (The view gives the keyboard focus to its current row whenever that
        // changes; where the focus was is put back, so that choosing a tab
        // leaves it on the tab header.)
        function reset() {
            const window = Window.window;
            const was = window ? window.activeFocusItem : null;
            currentIndex = count > 0 ? 0 : -1;
            positionViewAtBeginning();
            if (was && window.activeFocusItem !== was)
                was.forceActiveFocus(Qt.TabFocusReason);
        }

        anchors.fill: parent
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        activeFocusOnTab: true
        keyNavigationEnabled: true
        keyNavigationWraps: false
        highlightMoveDuration: 0
        highlightFollowsCurrentItem: true
        ScrollBar.vertical: ScrollBar {}
        // Entries come and go while open: never leave the selection dangling.
        onCountChanged: {
            if (count === 0)
                currentIndex = -1;
            else if (currentIndex < 0)
                currentIndex = 0;
            else if (currentIndex >= count)
                currentIndex = count - 1;
        }
        highlight: Rectangle {
            visible: list.activeFocus && list.currentIndex >= 0
            color: Qt.alpha(Theme.accent, 0.12)
            Rectangle {
                width: Ui.bar
                height: parent.height
                color: Theme.accent
            }
        }
        Keys.onReturnPressed: event => activate(event)
        Keys.onEnterPressed: event => activate(event)
        Keys.onSpacePressed: event => activate(event)
        function activate(event) {
            event.accepted = true;
            if (currentIndex >= 0 && currentIndex < count)
                restoreCurrent();
        }
    }

    // One row of either list: a title, a smaller detail line, and Restore.
    component RecoveryRow: RowLayout {
        id: row

        property string title
        property string detail

        signal restoreClicked

        spacing: 8
        height: 48

        ColumnLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Ui.medium
            spacing: 0

            Label {
                Layout.fillWidth: true
                text: row.title
                elide: Text.ElideRight
            }
            Label {
                Layout.fillWidth: true
                text: row.detail
                elide: Text.ElideRight
                color: Theme.mutedForeground
                font.pixelSize: 12
            }
        }
        Command {
            // Clickable; the keyboard restores through the list instead.
            focusPolicy: Qt.NoFocus
            text: qsTr("restore")
            onClicked: row.restoreClicked()
        }
    }
}
