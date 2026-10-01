import QtQuick
import QtQuick.Window
import org.omatree

Window {
    id: root
    width: 800
    height: 600
    visible: true
    title: qsTr("OmaTree")

    OmaTreeApp {
        id: app
    }

    Text {
        anchors.centerIn: parent
        text: "OmaTree"
        font.pixelSize: 24
    }
}
