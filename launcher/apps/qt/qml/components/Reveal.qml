import QtQuick
import OpenClient

// Entrada de una vista: `fadeTarget` aparece con un fundido y los bloques principales de la vista
// suben escalonados. Solo añade un Translate temporal a los bloques sin transform propio, así no
// rompe enlaces de opacidad ni animaciones que ya tengan; al terminar lo retira.
Item {
    id: reveal
    property Item fadeTarget: null
    property int stagger: 60
    property int duration: 620
    property int fadeDuration: 340
    property real distance: 26
    property int maxSlots: 7
    property real progress: 1
    property var entries: []
    visible: false

    function ease(t) { return 1 - Math.pow(1 - t, 4) }

    // Baja por envoltorios de un solo hijo hasta encontrar los bloques reales de la vista.
    function blocks(root) {
        let level = root
        for (let depth = 0; depth < 4 && level; ++depth) {
            const shown = []
            for (let i = 0; i < level.children.length; ++i) {
                const child = level.children[i]
                if (child.visible && child.width > 0 && child.height > 0) shown.push(child)
            }
            if (shown.length !== 1) return shown
            level = shown[0]
        }
        return level ? [level] : []
    }

    // Si la vista cambió mientras animaba, su bloque (y el Translate, que es su hijo) ya no existe.
    function alive(entry) { return !!entry.item && entry.item.transform !== undefined }

    function clear() {
        for (const entry of entries) {
            if (!alive(entry)) continue
            if (entry.item.transform.length === 1 && entry.item.transform[0] === entry.shift)
                entry.item.transform = []
            entry.shift.destroy()
        }
        entries = []
    }

    function play(root) {
        driver.stop()
        clear()
        if (!root || root.children === undefined || !Theme.motion) {
            progress = 1
            if (fadeTarget) fadeTarget.opacity = 1
            return
        }
        const targets = blocks(root).filter(item => item.transform.length === 0)
        targets.sort((a, b) => a.mapToItem(root, 0, 0).y - b.mapToItem(root, 0, 0).y)
        const list = []
        for (const item of targets) {
            const shift = shiftComponent.createObject(item, { y: distance })
            item.transform = [shift]
            list.push({ item: item, shift: shift, slot: Math.min(list.length, maxSlots - 1) })
        }
        entries = list
        driver.duration = duration + stagger * Math.max(0, Math.min(list.length, maxSlots) - 1)
        progress = 0
        apply()
        driver.restart()
    }

    function apply() {
        const elapsed = progress * driver.duration
        for (const entry of entries) {
            const local = Math.max(0, Math.min(1, (elapsed - entry.slot * stagger) / duration))
            if (alive(entry)) entry.shift.y = (1 - ease(local)) * distance
        }
        if (fadeTarget) fadeTarget.opacity = ease(Math.min(1, elapsed / fadeDuration))
    }

    onProgressChanged: apply()
    Component { id: shiftComponent; Translate {} }
    NumberAnimation {
        id: driver
        target: reveal
        property: "progress"
        from: 0; to: 1
        onFinished: reveal.clear()
    }
}
