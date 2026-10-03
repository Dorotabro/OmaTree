import QtQuick
import QtQuick.Controls
import org.omatree

// An OmaTree-owned dialog drawn from the semantic palette. All of the
// application's own dialogs are this; native file dialogs stay native.
Dialog {
    id: dialog

    // Set on a dialog whose affirmative answer destroys something: that
    // button, and only that one, turns red when it is hovered or focused.
    property bool destructive: false

    padding: Ui.large

    background: Rectangle {
        color: Theme.surfaceRaised
        border.width: Ui.hairline
        border.color: Theme.border
        radius: Ui.radius
    }

    // The button row: quiet commands, with the affirmative choice lit.
    footer: DialogButtonBox {
        standardButtons: dialog.standardButtons
        alignment: Qt.AlignRight
        spacing: Ui.tiny
        leftPadding: Ui.medium
        rightPadding: Ui.medium
        bottomPadding: Ui.small
        topPadding: 0
        background: null

        delegate: Command {
            id: button

            readonly property bool affirmative: DialogButtonBox.buttonRole === DialogButtonBox.AcceptRole || DialogButtonBox.buttonRole === DialogButtonBox.YesRole

            active: affirmative
            danger: DialogButtonBox.buttonRole === DialogButtonBox.DestructiveRole || (dialog.destructive && affirmative)
            focusPolicy: Qt.StrongFocus
        }
    }

    header: Label {
        visible: dialog.title !== ""
        text: dialog.title
        font.weight: Font.DemiBold
        font.pixelSize: 15
        color: Theme.foreground
        leftPadding: Ui.large
        rightPadding: Ui.large
        topPadding: Ui.large
        bottomPadding: 0
    }
}
