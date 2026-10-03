import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import org.omatree

// Right pane: title and plain-text body of the selected note.
Item {
    id: pane

    required property var notebook
    required property ItemSelectionModel selection

    signal deleteRequested
    // A rename was refused (for example, a sibling already has that name).
    signal renameFailed(string message)

    readonly property bool hasNote: selection.currentIndex.valid

    // --- Edit / Preview ----------------------------------------------------
    // Presentation only. The body is the raw text in the note; Preview shows
    // that text rendered as Markdown, read-only, and never writes anything
    // back. The mode is not part of the document: it survives moving between
    // notes, and starts again as Edit whenever a whole document is replaced.

    property bool previewing: false
    // The raw body being previewed, re-read from the notebook whenever the
    // note, the mode or the notebook changes.
    property string previewSource: ""
    // How a link is opened once the user has clicked it. Replaceable.
    property var openExternal: url => Qt.openUrlExternally(url)

    function refreshPreview() {
        previewSource = hasNote ? notebook.body(selection.currentIndex) : "";
    }

    // The Edit / Preview switch: choose a mode explicitly. Nothing to do
    // without a note, or when it is already the current mode.
    function setPreviewing(on) {
        if (hasNote && on !== previewing)
            toggleMode();
    }

    // The theme colours the Markdown renderer draws with, as "role=#rrggbb".
    readonly property var previewColors: ["background=" + Theme.background, "accent=" + Theme.accent, "accentSecondary=" + Theme.accentSecondary, "foreground=" + Theme.foreground, "muted=" + Theme.mutedForeground, "raised=" + Theme.surfaceRaised, "border=" + Theme.border, "warning=" + Theme.warning, "positive=" + Theme.positive]

    // Ctrl+E and the button. Nothing to do without a note.
    function toggleMode() {
        if (!hasNote)
            return;
        if (previewing) {
            previewing = false;
            body.forceActiveFocus();
        } else {
            refreshPreview();
            previewing = true;
        }
    }

    function resetMode() {
        previewing = false;
    }

    // Only http, https and mailto links, and only after a click.
    function openLink(link) {
        if (Markdown.isSafeLink(link))
            openExternal(link);
    }

    // True while a note is being loaded into the fields. Edits made by the
    // load itself must never be written back to the notebook.
    property bool loading: false

    function focusTitle() {
        title.forceActiveFocus();
        title.selectAll();
    }

    function load(index) {
        loading = true;
        title.text = index.valid ? notebook.data(index, Qt.DisplayRole) : "";
        body.text = index.valid ? notebook.body(index) : "";
        loading = false;
        // A different note: show its body now, from the top.
        previewSource = index.valid ? notebook.body(index) : "";
        previewView.contentItem.contentY = 0;
    }

    function commitTitle() {
        if (loading || !hasNote)
            return;
        const index = selection.currentIndex;
        const current = notebook.data(index, Qt.DisplayRole);
        const wanted = title.text.trim();
        if (wanted === "" || wanted === current) {
            title.text = current;
        } else {
            const error = notebook.rename(index, wanted);
            if (error !== "") {
                title.text = current;
                renameFailed(error);
            }
        }
    }

    Connections {
        target: pane.notebook
        // Restores and the like can change the body of the note on show.
        function onDocumentMutated() {
            if (pane.previewing)
                pane.refreshPreview();
        }
    }

    Connections {
        target: pane.selection
        function onCurrentChanged(current, previous) {
            pane.load(current);
        }
    }

    ColumnLayout {
        anchors.fill: parent
        visible: pane.hasNote
        spacing: 0

        // The header: the title is the page, and the rest recedes.
        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: Ui.editorPadding
            Layout.rightMargin: Ui.medium
            Layout.topMargin: Ui.small
            spacing: Ui.tiny

            TextField {
                id: title

                Layout.fillWidth: true
                leftPadding: 0
                font.pixelSize: 22
                font.weight: Font.DemiBold
                color: Theme.foreground
                placeholderTextColor: Theme.mutedForeground
                selectionColor: Theme.selection
                selectedTextColor: Theme.selectionForeground
                // No box: it reads as plain page text until you edit it, when
                // a thin accent line appears under it.
                background: Rectangle {
                    color: "transparent"
                    Rectangle {
                        anchors.bottom: parent.bottom
                        width: parent.width
                        height: Ui.hairline
                        color: title.activeFocus ? Theme.accent : "transparent"
                    }
                }
                placeholderText: qsTr("Title")
                onEditingFinished: pane.commitTitle()
                Keys.onReturnPressed: body.forceActiveFocus()
                Keys.onEnterPressed: body.forceActiveFocus()
            }
            // The two modes of the body, as a pair of commands: the current
            // one is lit and underlined.
            Command {
                text: qsTr("edit")
                hint: qsTr("Edit the note   Ctrl+E")
                active: !pane.previewing
                onClicked: pane.setPreviewing(false)
            }
            Command {
                text: qsTr("preview")
                hint: qsTr("Preview as Markdown   Ctrl+E")
                active: pane.previewing
                onClicked: pane.setPreviewing(true)
            }
            // Always available, never loud: it only turns red under the mouse
            // or the keyboard focus.
            Command {
                id: deleteButton

                text: qsTr("delete")
                hint: qsTr("Move this note to Trash   Delete")
                danger: true
                onClicked: pane.deleteRequested()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.leftMargin: Ui.editorPadding
            Layout.rightMargin: Ui.medium
            implicitHeight: Ui.hairline
            color: Qt.alpha(Theme.border, 0.7)
        }

        // The Markdown view of the same text. Read-only, with nothing wired
        // from it back to the note.
        ScrollView {
            id: previewView

            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: pane.previewing && pane.previewSource.trim() !== ""

            TextEdit {
                id: preview

                width: previewView.availableWidth
                readOnly: true
                selectByMouse: true
                textFormat: TextEdit.RichText
                wrapMode: TextEdit.Wrap
                color: Theme.foreground
                selectionColor: Theme.selection
                selectedTextColor: Theme.selectionForeground
                leftPadding: Ui.editorPadding
                rightPadding: Ui.editorPadding
                topPadding: Ui.large
                bottomPadding: Ui.large
                // Re-rendered when the text, the mode or the theme changes.
                text: pane.previewing ? Markdown.render(pane.previewSource, pane.previewColors) : ""
                onLinkActivated: link => pane.openLink(link)

                HoverHandler {
                    cursorShape: preview.hoveredLink !== "" ? Qt.PointingHandCursor : Qt.IBeamCursor
                }
            }
        }

        Label {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: pane.previewing && pane.previewSource.trim() === ""
            horizontalAlignment: Text.AlignHCenter
            verticalAlignment: Text.AlignVCenter
            text: qsTr("Nothing to preview")
            color: Theme.mutedForeground
        }

        // The plain-text editor. It stays alive while Preview is shown, so
        // the cursor, the selection and the scroll position are still there
        // when you come back.
        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true
            visible: !pane.previewing

            TextArea {
                id: body

                wrapMode: TextEdit.Wrap
                color: Theme.foreground
                selectionColor: Theme.selection
                selectedTextColor: Theme.selectionForeground
                placeholderTextColor: Theme.mutedForeground
                leftPadding: Ui.editorPadding
                rightPadding: Ui.editorPadding
                topPadding: Ui.large
                background: null
                onTextChanged: {
                    if (!pane.loading && pane.hasNote)
                        pane.notebook.setBody(pane.selection.currentIndex, text);
                }
            }
        }
    }

    Label {
        anchors.centerIn: parent
        visible: !pane.hasNote
        text: qsTr("Select or create a note")
        color: Theme.mutedForeground
    }
}
