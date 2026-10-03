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

    // The button that Enter presses when no button has the focus, and the
    // one that has the focus when the dialog opens: the safe answer. For a
    // destructive question that is No, never Yes. Replaceable per dialog.
    property int defaultButton: destructive ? Dialog.No : (standardButtons & Dialog.Save ? Dialog.Save : (standardButtons & Dialog.Ok ? Dialog.Ok : Dialog.Close))

    // Where the focus goes on open instead of the default button (a list
    // that should scroll with the keys, say).
    property Item initialFocusItem: null

    padding: Ui.large

    // Focus lands on the safe button, so Space and Enter work at once and
    // Tab / Shift+Tab move on from there. Escape closes (rejects) as usual.
    onOpened: {
        const target = initialFocusItem ? initialFocusItem : standardButton(defaultButton);
        if (target)
            target.forceActiveFocus(Qt.TabFocusReason);
    }

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

        // Enter with the focus somewhere other than a button (a button
        // handles its own Enter) presses the safe default.
        Keys.onReturnPressed: pressDefault()
        Keys.onEnterPressed: pressDefault()
        function pressDefault() {
            const button = dialog.standardButton(dialog.defaultButton);
            if (button)
                button.click();
        }

        delegate: Command {
            id: button

            readonly property bool affirmative: DialogButtonBox.buttonRole === DialogButtonBox.AcceptRole || DialogButtonBox.buttonRole === DialogButtonBox.YesRole

            // The default answer is the lit one.
            active: dialog.opened && button === dialog.standardButton(dialog.defaultButton)
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
