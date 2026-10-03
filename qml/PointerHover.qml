import QtQuick
import org.omatree

// Hover that cannot go stale behind a popup.
//
// A hover handler only learns about the pointer from the events delivered to
// it. While a menu or dialog is open the pointer's events go to the popup, so
// a control behind it never hears that the pointer left. Worse, when the
// popup closes Qt replays the last position the control knew, and the
// control believes the pointer is back where it was. (Switching the handler
// off and on does not help either: it takes the old position up again.)
//
// So after a popup has opened or closed (`Ui.popupSerial` changed), `over`
// stays false until the pointer has really moved: to a position other than
// the one it was last known at. Until then the control looks un-hovered, as
// it should if the pointer is somewhere else; one small movement over it
// brings the hover back.
HoverHandler {
    property int seenSerial: -1
    property bool moved: false
    property point lastPosition: Qt.point(-1, -1)
    property point anchor: Qt.point(-1, -1)

    readonly property bool over: hovered && moved && seenSerial === Ui.popupSerial

    onPointChanged: {
        const position = point.position;
        if (seenSerial !== Ui.popupSerial) {
            seenSerial = Ui.popupSerial;
            anchor = lastPosition;
            moved = false;
        }
        if (position.x !== anchor.x || position.y !== anchor.y)
            moved = true;
        lastPosition = position;
    }
}
