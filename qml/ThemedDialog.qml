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

    // A plain Yes / No question: N and Y answer it at once, in either case.
    readonly property bool binary: (standardButtons & Dialog.Yes) !== 0 && (standardButtons & Dialog.No) !== 0

    // The standard buttons' own text is Qt's, translated into the user's
    // system language (on macOS, "Ano" / "Ne"), while the rest of OmaTree is
    // English. So every standard button is labelled here, explicitly, and the
    // process locale is left alone. The Save button may read otherwise (Save
    // As…) for a dialog whose Save does that.
    property string saveText: qsTr("Save")
    readonly property var buttonTexts: ({
            [Dialog.Yes]: qsTr("Yes"),
            [Dialog.No]: qsTr("No"),
            [Dialog.Save]: saveText,
            [Dialog.Discard]: qsTr("Discard"),
            [Dialog.Cancel]: qsTr("Cancel"),
            [Dialog.Ok]: qsTr("OK"),
            [Dialog.Close]: qsTr("Close")
        })
    function labelButtons() {
        for (const which in buttonTexts) {
            const button = standardButton(Number(which));
            if (button)
                button.text = buttonTexts[which];
        }
    }

    padding: Ui.large

    // Focus lands on the safe button, so Space and Enter work at once and
    // Tab / Shift+Tab (and the arrow keys) move on from there. Escape closes
    // (rejects) as usual: that is No, or Cancel.
    property Item previousFocus: null
    onAboutToShow: {
        labelButtons();
        previousFocus = (parent && parent.Window.window) ? parent.Window.window.activeFocusItem : null;
        Ui.dialogsOpen++;
    }
    // (A beat later, because Qt itself puts the focus on an Accept-role
    // button as the dialog opens, and without the keyboard focus mark.)
    onOpened: Qt.callLater(focusInitial)
    function focusInitial() {
        if (!opened)
            return;
        const target = initialFocusItem ? initialFocusItem : standardButton(defaultButton);
        if (!target)
            return;
        target.forceActiveFocus(Qt.TabFocusReason);
    }
    // Whatever had the keyboard when the dialog opened gets it back, unless
    // another dialog has opened in the meantime and owns it now. (A beat
    // later, when this dialog has stopped counting as open.)
    onClosed: {
        Ui.dialogsOpen--;
        Qt.callLater(restoreFocus);
    }
    function restoreFocus() {
        const was = previousFocus;
        previousFocus = null;
        if (was && was.visible && Ui.dialogsOpen === 0)
            was.forceActiveFocus();
    }

    // N and Y, only while this dialog is open: shortcuts take precedence over
    // everything beneath, so the keystroke reaches nothing else.
    readonly property Shortcut noShortcut: noKey
    readonly property Shortcut yesShortcut: yesKey
    Shortcut {
        id: noKey

        sequences: ["N", "Shift+N"]
        enabled: dialog.opened && dialog.binary
        onActivated: dialog.standardButton(Dialog.No).click()
    }
    Shortcut {
        id: yesKey

        sequences: ["Y", "Shift+Y"]
        enabled: dialog.opened && dialog.binary
        onActivated: dialog.standardButton(Dialog.Yes).click()
    }

    background: Rectangle {
        color: Theme.surfaceRaised
        border.width: Ui.hairline
        border.color: Theme.border
        radius: Ui.radius
    }

    // The button row: quiet commands, with the affirmative choice lit.
    footer: DialogButtonBox {
        id: buttons

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
        // Left and Right (handled on each button below) move the real
        // keyboard focus along the buttons, and around: past the last is the
        // first. What shows the focus is the button's own focus state,
        // nothing else. They are handled on the buttons because the box's own
        // list view would otherwise take the arrow keys first and move the
        // focus without the visible focus mark.
        function moveFocus(step) {
            if (count < 2)
                return;
            let at = -1;
            for (let i = 0; i < count; ++i) {
                if (itemAt(i).activeFocus)
                    at = i;
            }
            const next = at < 0 ? 0 : (at + step + count) % count;
            itemAt(next).forceActiveFocus(Qt.TabFocusReason);
        }
        function pressDefault() {
            const button = dialog.standardButton(dialog.defaultButton);
            if (button)
                button.click();
        }

        delegate: Command {
            id: button

            readonly property bool affirmative: DialogButtonBox.buttonRole === DialogButtonBox.AcceptRole || DialogButtonBox.buttonRole === DialogButtonBox.YesRole

            // (No "lit default": the only mark is the focus, and it is the
            // button that Enter and Space will press.)
            danger: DialogButtonBox.buttonRole === DialogButtonBox.DestructiveRole || (dialog.destructive && affirmative)
            focusPolicy: Qt.StrongFocus
            Keys.onLeftPressed: event => {
                buttons.moveFocus(-1);
                event.accepted = true;
            }
            Keys.onRightPressed: event => {
                buttons.moveFocus(1);
                event.accepted = true;
            }
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
