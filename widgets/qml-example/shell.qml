import QtQuick
import Quickshell
import Quickshell.Wayland

ShellRoot {
    PanelWindow {
        id: panel
        property var theme: {
            const raw = Quickshell.env("RS_WWIDGET_THEME");
            return raw ? JSON.parse(raw) : {background:"#282828",foreground:"#d4be98",muted:"#a89984",accent:"#e78a4e",green:"#a9b665",border:"#504945",bar_background:"#3c3836"};
        }
        property bool pinned: Quickshell.env("RS_WWIDGET_LOCKED") === "1"
        property bool spinning: true
        property int clicks: 0
        anchors { left: true; top: true }
        margins.left: Number(Quickshell.env("RS_WWIDGET_X") || 64)
        margins.top: Number(Quickshell.env("RS_WWIDGET_Y") || 96)
        implicitWidth: Number(Quickshell.env("RS_WWIDGET_WIDTH") || 360)
        implicitHeight: Number(Quickshell.env("RS_WWIDGET_HEIGHT") || 300)
        color: "transparent"
        exclusionMode: ExclusionMode.Ignore
        WlrLayershell.namespace: "rs-wwidget-qml-example"
        WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand
        WlrLayershell.layer: {
            const layer = Quickshell.env("RS_WWIDGET_LAYER") || "bottom";
            if (layer === "overlay") return WlrLayer.Overlay;
            if (layer === "top") return WlrLayer.Top;
            if (layer === "background") return WlrLayer.Background;
            return WlrLayer.Bottom;
        }

        Rectangle {
            anchors.fill: parent
            color: panel.theme.background; radius: 18
            border.color: panel.theme.border
            Text { x: 20; y: 16; text: "QML / rs-wwidget"; color: panel.theme.foreground; font.pixelSize: 18 }
            MouseArea {
                x: 0; y: 0; width: parent.width - 84; height: 44
                enabled: !panel.pinned
                cursorShape: pressed ? Qt.ClosedHandCursor : Qt.OpenHandCursor
                property real startX: 0
                property real startY: 0
                onPressed: mouse => { startX = mouse.x; startY = mouse.y; }
                onPositionChanged: mouse => {
                    if (pressed) {
                        panel.margins.left = Math.max(0, panel.margins.left + mouse.x - startX);
                        panel.margins.top = Math.max(0, panel.margins.top + mouse.y - startY);
                    }
                }
            }
            Rectangle {
                x: parent.width - 76; y: 10; width: 64; height: 30; radius: 8
                color: panel.pinned ? panel.theme.green : panel.theme.bar_background
                Text { anchors.centerIn: parent; text: panel.pinned ? "PINNED" : "MOVE"; color: panel.theme.foreground; font.pixelSize: 11 }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: panel.pinned = !panel.pinned }
            }
            Rectangle {
                x: 28; y: 72; width: 56; height: 56; radius: 12; color: panel.theme.accent
                NumberAnimation on rotation { from: 0; to: 360; duration: 4000; loops: Animation.Infinite; running: true; paused: !panel.spinning }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: panel.spinning = !panel.spinning }
            }
            Text { x: 116; y: 82; text: "Click shape to pause"; color: panel.theme.muted; font.pixelSize: 14 }
            Rectangle {
                x: 20; y: 158; width: parent.width - 40; height: 40; radius: 8; color: panel.theme.bar_background
                TextInput {
                    id: editor
                    anchors.fill: parent; anchors.margins: 10; color: panel.theme.foreground
                    font.pixelSize: 16; clip: true; text: "Type anything here"
                    selectByMouse: true; focus: true
                }
            }
            Rectangle {
                x: 20; y: 216; width: parent.width - 40; height: 40; radius: 8; color: panel.theme.green
                Text { anchors.centerIn: parent; text: "Clicks: " + panel.clicks; color: panel.theme.background; font.pixelSize: 16 }
                MouseArea { anchors.fill: parent; cursorShape: Qt.PointingHandCursor; onClicked: panel.clicks += 1 }
            }
        }
    }
}
