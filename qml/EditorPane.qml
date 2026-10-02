import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

// Right pane: title and plain-text body of the selected note.
Item {
    id: pane

    required property var notebook
    required property ItemSelectionModel selection

    signal deleteRequested

    readonly property bool hasNote: selection.currentIndex.valid

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
    }

    function commitTitle() {
        if (loading || !hasNote)
            return;
        const index = selection.currentIndex;
        const current = notebook.data(index, Qt.DisplayRole);
        const wanted = title.text.trim();
        if (wanted === "" || wanted === current)
            title.text = current;
        else
            notebook.rename(index, wanted);
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

        RowLayout {
            Layout.fillWidth: true
            Layout.margins: 12
            Layout.bottomMargin: 0

            TextField {
                id: title

                Layout.fillWidth: true
                font.pixelSize: 20
                font.bold: true
                background: null
                placeholderText: qsTr("Title")
                onEditingFinished: pane.commitTitle()
                Keys.onReturnPressed: body.forceActiveFocus()
                Keys.onEnterPressed: body.forceActiveFocus()
            }
            Button {
                text: qsTr("Delete")
                flat: true
                onClicked: pane.deleteRequested()
            }
        }

        ScrollView {
            Layout.fillWidth: true
            Layout.fillHeight: true

            TextArea {
                id: body

                wrapMode: TextEdit.Wrap
                leftPadding: 12
                rightPadding: 12
                topPadding: 8
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
        opacity: 0.6
    }
}
