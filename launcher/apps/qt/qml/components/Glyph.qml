import QtQuick
import QtQuick.Shapes
import OpenClient

Item {
    id: root
    property string name: "grid"
    property color color: Theme.ink
    implicitWidth: 22
    implicitHeight: 22
    readonly property bool brandMark: name === "discord" || name === "github"
    readonly property var paths: ({
        grid: "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z",
        layers: "M12 3 2 8l10 5 10-5Z M2 12l10 5 10-5 M2 16l10 5 10-5",
        list: "M7 5h14 M7 12h14 M7 19h14 M3 5h.1 M3 12h.1 M3 19h.1",
        compass: "M21 12a9 9 0 1 1-18 0a9 9 0 1 1 18 0 M16 8l-2 6-6 2 2-6Z",
        shirt: "M8 3 3 5 1 10l4 2v9h14v-9l4-2-2-5-5-2c0 4-8 4-8 0Z",
        settings: "M3 6h18 M3 12h18 M3 18h18 M7 3v6 M16 9v6 M10 15v6",
        code: "M8 5 1 12l7 7 M16 5l7 7-7 7 M14 3l-4 18",
        user: "M16 7a4 4 0 1 1-8 0a4 4 0 1 1 8 0 M4 21v-2a8 8 0 0 1 16 0v2",
        globe: "M21 12a9 9 0 1 1-18 0a9 9 0 1 1 18 0 M3 12h18 M12 3a4 9 0 1 0 0 18 4 9 0 1 0 0-18",
        storefront: "M3 9l2-5h14l2 5v1a3 3 0 0 1-6 0 3 3 0 0 1-6 0 3 3 0 0 1-6 0V9h18 M5 13v7h14v-7 M10 20v-5h4v5",
        plus: "M12 4v16 M4 12h16",
        star: "m12 3 2.8 5.7 6.2.9-4.5 4.4 1.1 6.2-5.6-3-5.6 3 1.1-6.2L3 9.6l6.2-.9Z",
        puzzle: "M9 3H3v6h2a3 3 0 0 1 0 6H3v6h6v-2a3 3 0 0 1 6 0v2h6v-6h-2a3 3 0 0 1 0-6h2V3h-6v2a3 3 0 0 1-6 0Z",
        image: "M3 3h18v18H3Z M3 17l6-6 4 4 3-3 5 5 M16 6a2 2 0 1 1 0 4 2 2 0 1 1 0-4",
        sun: "M16 12a4 4 0 1 1-8 0 4 4 0 1 1 8 0 M12 2v2 M12 20v2 M2 12h2 M20 12h2 M5 5l1.5 1.5 M17.5 17.5 19 19 M5 19l1.5-1.5 M17.5 6.5 19 5",
        folder: "M3 6h7l2 2h9v12H3Z",
        arrow: "M5 19 19 5 M7 5h12v12",
        chevron: "m5 9 7 7 7-7",
        play: "M7 3l14 9L7 21Z",
        chip: "M6 6h12v12H6Z M9 1v5 M15 1v5 M9 18v5 M15 18v5 M1 9h5 M1 15h5 M18 9h5 M18 15h5",
        check: "M4 12l5 5L20 6",
        refresh: "M20 12a8 8 0 1 1-2.34-5.66 M20 4v5h-5",
        eye: "M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12Z M15 12a3 3 0 1 1-6 0a3 3 0 1 1 6 0",
        edit: "M4 20h4L19 9l-4-4L4 16Z M13 7l4 4",
        copy: "M8 8h12v12H8Z M4 16V4h12",
        trash: "M4 7h16 M10 11v6 M14 11v6 M6 7l1 13h10l1-13 M9 7V4h6v3",
        close: "M5 5l14 14 M5 19 19 5",
        search: "M17 10a7 7 0 1 1-14 0a7 7 0 1 1 14 0 M15 15l7 7",
        news: "M5 3h14v18H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z M7 8h8 M7 12h8 M7 16h5",
        home: "M3 11 12 3l9 8 M5 9.5V21h5v-6h4v6h5V9.5",
        sidebar: "M3 4h18v16H3Z M9 4v16",
        stop: "M6 6h12v12H6Z",
        alert: "M12 3 2 20h20Z M12 10v4 M12 17h.1",
        clock: "M21 12a9 9 0 1 1-18 0a9 9 0 1 1 18 0 M12 7v5l3 2",
        download: "M12 4v12 M7 11l5 5 5-5 M4 20h16",
        shield: "M12 3 4 6v6c0 5 3.5 8 8 9 4.5-1 8-4 8-9V6Z M9 12l2 2 4-4",
        gamepad: "M6 8h12a4 4 0 0 1 4 4v2a3 3 0 0 1-5.5 1.6L15 14H9l-1.5 1.6A3 3 0 0 1 2 14v-2a4 4 0 0 1 4-4Z M7 10.5v3 M5.5 12h3 M16 11h.1 M18 13h.1",
        logout: "M9 4H4v16h5 M16 8l4 4-4 4 M20 12H9",
        arrowright: "M4 12h16 M14 6l6 6-6 6",
        chevronright: "m9 5 7 7-7 7"
    })
    // Discord Brand Assets and GitHub Primer Octicons provide these silhouettes.
    readonly property string discordPath: "M40.9051 0C40.2863 1.09866 39.7306 2.2352 39.2255 3.397C34.4268 2.67719 29.5396 2.67719 24.7283 3.397C24.2358 2.2352 23.6675 1.09866 23.0487 0C18.5404 0.770324 14.1458 2.12155 9.97847 4.02841C1.71959 16.2652 -0.51561 28.1863 0.595677 39.9432C5.4323 43.517 10.8498 46.2447 16.6209 47.9874C17.9216 46.2447 19.0708 44.3883 20.0558 42.4562C18.1868 41.7616 16.381 40.8903 14.6509 39.88C15.1055 39.5517 15.5475 39.2107 15.9769 38.8824C26.1174 43.6559 37.8617 43.6559 48.0148 38.8824C48.4441 39.236 48.8861 39.577 49.3407 39.88C47.6107 40.9029 45.8048 41.7616 43.9232 42.4688C44.9082 44.4009 46.0574 46.2573 47.3581 48C53.1292 46.2573 58.5467 43.5422 63.3834 39.9684C64.6967 26.3299 61.1355 14.5099 53.9753 4.04104C49.8206 2.13418 45.426 0.782952 40.9177 0.0252565L40.9051 0ZM21.4702 32.7072C18.351 32.7072 15.7622 29.8785 15.7622 26.3804C15.7622 22.8824 18.25 20.041 21.4576 20.041C24.6651 20.041 27.216 22.895 27.1655 26.3804C27.115 29.8658 24.6525 32.7072 21.4702 32.7072ZM42.5089 32.7072C39.3771 32.7072 36.8135 29.8785 36.8135 26.3804C36.8135 22.8824 39.3013 20.041 42.5089 20.041C45.7164 20.041 48.2547 22.895 48.2042 26.3804C48.1537 29.8658 45.6912 32.7072 42.5089 32.7072Z"
    readonly property string githubPath: "M10.226 17.284c-2.965-.36-5.054-2.493-5.054-5.256 0-1.123.404-2.336 1.078-3.144-.292-.741-.247-2.314.09-2.965.898-.112 2.111.36 2.83 1.01.853-.269 1.752-.404 2.853-.404 1.1 0 1.999.135 2.807.382.696-.629 1.932-1.1 2.83-.988.315.606.36 2.179.067 2.942.72.854 1.101 2 1.101 3.167 0 2.763-2.089 4.852-5.098 5.234.763.494 1.28 1.572 1.28 2.807v2.336c0 .674.561 1.056 1.235.786 4.066-1.55 7.255-5.615 7.255-10.646C23.5 6.188 18.334 1 11.978 1 5.62 1 .5 6.188.5 12.545c0 4.986 3.167 9.12 7.435 10.669.606.225 1.19-.18 1.19-.786V20.63a2.9 2.9 0 0 1-1.078.224c-1.483 0-2.359-.808-2.987-2.313-.247-.607-.517-.966-1.034-1.033-.27-.023-.359-.135-.359-.27 0-.27.45-.471.898-.471.652 0 1.213.404 1.797 1.235.45.651.921.943 1.483.943.561 0 .92-.202 1.437-.719.382-.381.674-.718.944-.943"
    Shape {
        anchors.centerIn: parent
        width: root.name === "discord" ? 64 : 24
        height: root.name === "discord" ? 48 : 24
        scale: Math.min(root.width / width, root.height / height)
        ShapePath {
            strokeColor: root.brandMark ? "transparent" : root.color
            strokeWidth: root.brandMark ? 0 : 1.6
            fillColor: root.brandMark ? (Theme.light ? "#000000" : "#ffffff") : "transparent"
            fillRule: ShapePath.OddEvenFill
            capStyle: ShapePath.RoundCap
            joinStyle: ShapePath.RoundJoin
            PathSvg {
                path: root.name === "discord" ? root.discordPath : root.name === "github" ? root.githubPath
                    : root.paths[root.name] || root.paths.grid
            }
        }
    }
}
