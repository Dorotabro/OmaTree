import QtQuick
import QtQuick.Controls
import org.omatree

// An OmaTree-owned dialog drawn from the semantic palette. All of the
// application's own dialogs are this; native file dialogs stay native.
Dialog {
    id: dialog

    padding: 16

    background: Rectangle {
        color: Theme.surfaceRaised
        border.width: 1
        border.color: Theme.border
        radius: 4
    }

    // A quiet button row: no dark band, compact outlined buttons.
    footer: DialogButtonBox {
        standardButtons: dialog.standardButtons
        alignment: Qt.AlignRight
        spacing: 8
        leftPadding: 16
        rightPadding: 16
        bottomPadding: 12
        topPadding: 4
        background: null

        delegate: Button {
            id: button

            horizontalPadding: 14
            background: Rectangle {
                implicitHeight: 28
                radius: 3
                color: button.down ? Theme.border : (button.hovered ? Theme.surface : "transparent")
                border.width: 1
                border.color: button.activeFocus ? Theme.accent : Theme.border
            }
        }
    }

    header: Label {
        visible: dialog.title !== ""
        text: dialog.title
        font.bold: true
        color: Theme.foreground
        leftPadding: 16
        rightPadding: 16
        topPadding: 14
        bottomPadding: 0
    }
}
