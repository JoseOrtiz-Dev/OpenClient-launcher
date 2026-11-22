import QtQuick
import OpenClient

Item {
    id: block
    property bool fabric: false
    property string loader: fabric ? "fabric" : "vanilla"
    readonly property bool grassStyle: loader === "fabric" || loader === "quilt" || fabric
    readonly property color topTint: loader === "forge" ? "#68d97724"
                                   : loader === "neoforge" ? "#68e05d34"
                                   : loader === "quilt" ? "#688a5cf5"
                                   : grassStyle ? "#70449425" : "#30dbe8f3"
    implicitWidth: 34
    implicitHeight: 34

    // The three texture faces use the same isometric projection.
    Item {
        width: 40
        height: 40
        anchors.centerIn: parent
        scale: Math.min(block.width, block.height) / 40

        Image {
            x: 3; y: 11; width: 17; height: 20; smooth: false
            source: block.grassStyle ? "qrc:/assets/blocks/dirt.png" : "qrc:/assets/blocks/stone.png"
            transform: Matrix4x4 { matrix: Qt.matrix4x4(1,0,0,0, 0.49,1,0,0, 0,0,1,0, 0,0,0,1) }
            Image { visible: block.grassStyle; width: parent.width; height: 5; smooth: false; source: "qrc:/assets/blocks/moss_block.png" }
        }
        Image {
            x: 20; y: 19.3; width: 17; height: 20; smooth: false
            source: block.grassStyle ? "qrc:/assets/blocks/dirt.png" : "qrc:/assets/blocks/stone.png"
            transform: Matrix4x4 { matrix: Qt.matrix4x4(1,0,0,0, -0.49,1,0,0, 0,0,1,0, 0,0,0,1) }
            Image { visible: block.grassStyle; width: parent.width; height: 5; smooth: false; source: "qrc:/assets/blocks/moss_block.png" }
            Rectangle { anchors.fill: parent; color: "#3d000000" }
        }
        Image {
            x: 20; y: 2.7; width: 17; height: 17; smooth: false
            source: block.grassStyle ? "qrc:/assets/blocks/moss_block.png" : "qrc:/assets/blocks/stone.png"
            transform: Matrix4x4 { matrix: Qt.matrix4x4(1,-1,0,0, 0.49,0.49,0,0, 0,0,1,0, 0,0,0,1) }
            Rectangle { anchors.fill: parent; color: block.topTint }
        }
    }
}
