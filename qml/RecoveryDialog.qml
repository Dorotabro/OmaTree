import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Trash and checkpoints of the open notebook. Entries are addressed by their
// position in the lists the model exposes (newest first), never by node id.
Dialog {
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
    // Always start on Trash.
    onAboutToShow: tabs.currentIndex = 0

    contentItem: ColumnLayout {
        spacing: 8

        TabBar {
            id: tabs

            Layout.fillWidth: true

            TabButton {
                text: qsTr("Trash (%1)").arg(dialog.trashCount)
            }
            TabButton {
                text: qsTr("Checkpoints (%1)").arg(dialog.checkpointCount)
            }
        }

        StackLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            currentIndex: tabs.currentIndex

            // Trash
            Item {
                ListView {
                    id: trashList

                    anchors.fill: parent
                    clip: true
                    model: dialog.trashCount
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}

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
                    opacity: 0.6
                }
            }

            // Checkpoints
            Item {
                ListView {
                    id: checkpointList

                    anchors.fill: parent
                    clip: true
                    model: dialog.checkpointCount
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar {}

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
                    opacity: 0.6
                }
            }
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
                opacity: 0.6
                font.pixelSize: 12
            }
        }
        Button {
            text: qsTr("Restore")
            flat: true
            onClicked: row.restoreClicked()
        }
    }
}
