import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// Controller navigation like a console store: the D-pad and the left stick move the focus to the nearest control
// in that direction (down from a poster row goes to the nearest card of the next row; left and right stay in the
// row, and left from the first card goes into the sidebar), a held direction repeats (350 ms, then every 90 ms),
// and the page and the row scroll smoothly so the focused control is fully in view (rows keep it near the left).
// The right stick scrolls the page freely (up/down) and the rows and carousels sideways (left/right), in the player it
// seeks (left/right) and sets the volume (up/down), in a reader it goes to the reader's padStick() (scroll and zoom).
// A focused slider takes left/right (Preferences, the volume in the sidebar). L2 / R2 jump about a page.
Item {
    id: nav
    property var window
    // right stick, -1 (down) to 1 (up), and -1 (left) to 1 (right)
    property real rsY: 0
    property real rsX: 0
    // Main.qml: the player (its action(name)) and the reader open now (padStick(x, y, seconds))
    property Item player: null
    property Item reader: null
    // "RS:<y>" / "RSX:<x>" from the core (rh_pad_poll)
    function stick(msg) {
        const v = parseFloat(msg.slice(msg.indexOf(":") + 1)) || 0
        if (msg.indexOf("RSX:") === 0) rsX = v
        else rsY = v
    }
    property string held: ""
    // milliseconds between repeats of a held direction (Preferences > Controller)
    property int repeatMs: 90
    // the moment the controller last moved the focus (Main.qml plays the blip for those changes only)
    property real movedAt: 0

    function isIn(it, anc) {
        for (var p = it; p; p = p.parent) if (p === anc) return true
        return false
    }
    function center(it) { return it.mapToItem(null, it.width / 2, it.height / 2) }

    // the controls the focus can go to: the focus chain, limited to the open popup when there is one
    function candidates(cur) {
        const out = []
        const overlay = nav.Overlay.overlay
        const inPopup = cur && overlay && isIn(cur, overlay)
        var it = cur ? cur.nextItemInFocusChain(true) : window.contentItem.nextItemInFocusChain(true)
        for (var n = 0; it && n < 3000 && it !== cur; ++n) {
            if (it.visible && it.enabled && it.width > 0 && it.height > 0 && (!inPopup || isIn(it, overlay))) out.push(it)
            const next = it.nextItemInFocusChain(true)
            if (next === it || out.indexOf(next) >= 0) break
            it = next
        }
        return out
    }

    function focusItem(it) {
        if (!it) return
        movedAt = Date.now()
        it.forceActiveFocus(Qt.TabFocusReason)
    }

    // a focused horizontal slider or spin box (left / right change its value)
    function sliderOf(it) {
        if (!it || typeof it.increase !== "function" || typeof it.decrease !== "function") return null
        if (it.orientation !== undefined && it.orientation !== Qt.Horizontal) return null
        return it
    }
    // dir: "Left", "Right", "Up", "Down"
    function move(dir) {
        const cur = window.activeFocusItem
        const sl = (dir === "Left" || dir === "Right") ? sliderOf(cur) : null
        if (sl) {
            if (dir === "Right") sl.increase(); else sl.decrease()
            // the same signal a drag sends, so the setting is saved (Slider.moved, SpinBox.valueModified)
            if (typeof sl.moved === "function") sl.moved()
            else if (typeof sl.valueModified === "function") sl.valueModified()
            return
        }
        const list = candidates(cur && cur !== window.contentItem ? cur : null)
        if (!cur || cur === window.contentItem) { focusItem(list.length ? list[0] : null); return }
        const c = center(cur)
        const horiz = dir === "Left" || dir === "Right"
        const sign = dir === "Right" || dir === "Down" ? 1 : -1
        var best = null, bestScore = 1e12
        for (var i = 0; i < list.length; ++i) {
            const p = center(list[i])
            const primary = (horiz ? p.x - c.x : p.y - c.y) * sign
            const orth = Math.abs(horiz ? p.y - c.y : p.x - c.x)
            if (primary <= 4) continue
            // horizontal: stay in the row; vertical: the next row first, then the nearest column in it
            const score = horiz ? primary + orth * 4 : primary + orth * 0.5
            if (score < bestScore) { bestScore = score; best = list[i] }
        }
        focusItem(best)
    }

    // L2 / R2: the control nearest to the same place one page higher or lower
    function page(sign) {
        const cur = window.activeFocusItem
        if (!cur || cur === window.contentItem) { move("Down"); return }
        const f = flickOf(cur, true)
        const h = f ? f.height * 0.85 : window.height * 0.85
        const c = center(cur)
        const list = candidates(cur)
        var best = null, bestD = 1e12
        for (var i = 0; i < list.length; ++i) {
            const p = center(list[i])
            if ((p.y - c.y) * sign <= 4) continue
            const d = Math.abs(p.y - (c.y + sign * h)) + Math.abs(p.x - c.x) * 0.5
            if (d < bestD) { bestD = d; best = list[i] }
        }
        focusItem(best)
    }

    // the nearest scrolling parent of `it` that scrolls vertically (or horizontally)
    function flickOf(it, vertical) {
        for (var p = it ? it.parent : null; p; p = p.parent) {
            if (p.contentY === undefined || p.contentItem === undefined || p.flickableDirection === undefined) continue
            if (vertical ? p.contentHeight > p.height + 1 : p.contentWidth > p.width + 1) return p
        }
        return null
    }

    // scrolls the page (and the row) so `it` is fully visible, with an eased animation
    function ensureVisible(it) {
        if (!it || it === window.contentItem) return
        const fy = flickOf(it, true)
        if (fy) {
            const r = it.mapToItem(fy.contentItem, 0, 0)
            const m = 40
            var ty = fy.contentY
            if (r.y - m < ty) ty = r.y - m
            else if (r.y + it.height + m > ty + fy.height) ty = r.y + it.height + m - fy.height
            ty = Math.max(fy.originY - fy.topMargin, Math.min(ty, fy.originY + fy.contentHeight + fy.bottomMargin - fy.height))
            if (Math.abs(ty - fy.contentY) > 1) { animY.stop(); animY.target = fy; animY.to = ty; animY.start() }
        }
        const fx = flickOf(it, false)
        if (fx) {
            const r2 = it.mapToItem(fx.contentItem, 0, 0)
            // a row keeps the focused card near its left edge (the store's rows)
            var tx = r2.x - 48
            tx = Math.max(fx.originX - fx.leftMargin, Math.min(tx, fx.originX + fx.contentWidth + fx.rightMargin - fx.width))
            if (Math.abs(tx - fx.contentX) > 1) { animX.stop(); animX.target = fx; animX.to = tx; animX.start() }
        }
    }
    NumberAnimation { id: animY; property: "contentY"; duration: 220; easing.type: Easing.OutCubic }
    NumberAnimation { id: animX; property: "contentX"; duration: 220; easing.type: Easing.OutCubic }

    // ---- a held direction repeats ----
    function press(dir) { held = dir; move(dir); repeat.interval = 350; repeat.restart() }
    function release(dir) { if (held === dir) { held = ""; repeat.stop() } }
    Timer {
        id: repeat
        repeat: true
        onTriggered: { if (nav.held === "") { stop(); return } nav.move(nav.held); interval = nav.repeatMs }
    }

    // the deepest scrolling item under a point of the window (the page under the middle of the content when the
    // focus is in no scrolling list, e.g. on the sidebar or nowhere): an open popup first, then the page
    function flickAt(vertical) {
        const roots = [nav.Overlay.overlay, window.contentItem]
        for (var r = 0; r < roots.length; ++r) {
            const root = roots[r]
            if (!root) continue
            var it = root, best = null
            var p = {x: root.width * 0.6, y: root.height * 0.55}
            for (var n = 0; it && n < 64; ++n) {
                if (it !== root && it.contentY !== undefined && it.contentItem !== undefined && it.flickableDirection !== undefined
                        && (vertical ? it.contentHeight > it.height + 1 : it.contentWidth > it.width + 1)) best = it
                // the child under the point, the last drawn first (not the click watcher of Main, objectName "padPassive")
                var next = null
                for (var i = it.children.length - 1; i >= 0; --i) {
                    const c = it.children[i]
                    if (!c.visible || c.objectName === "padPassive") continue
                    const q = it.mapToItem(c, p.x, p.y)
                    if (q.x >= 0 && q.y >= 0 && q.x < c.width && q.y < c.height) { next = c; p = q; break }
                }
                it = next
            }
            if (best) return best
        }
        return null
    }
    function scrollBy(f, vertical, d) {
        if (!f) return
        if (vertical) {
            animY.stop()
            const top = f.originY - f.topMargin
            const bottom = f.originY + f.contentHeight + f.bottomMargin - f.height
            f.contentY = Math.max(top, Math.min(bottom, f.contentY + d))
        } else {
            animX.stop()
            const left = f.originX - f.leftMargin
            const right = f.originX + f.contentWidth + f.rightMargin - f.width
            f.contentX = Math.max(left, Math.min(right, f.contentX + d))
        }
    }

    // ---- D-pad on a page: moves the content (HP: the D-pad scrolls, the left stick moves the focus) ----
    // the open reader scrolls (or turns the page of a paged book) with padStick(0, y); elsewhere the page under the
    // focus (or under the middle of the window) scrolls, and left / right scroll the row of the focused card
    property int dpadX: 0
    property int dpadY: 0
    function contentPress(dir) {
        if (dir === "Up") dpadY = 1
        else if (dir === "Down") dpadY = -1
        else if (dir === "Left") dpadX = -1
        else if (dir === "Right") dpadX = 1
    }
    function contentRelease(dir) {
        if ((dir === "Up" && dpadY > 0) || (dir === "Down" && dpadY < 0)) dpadY = 0
        else if ((dir === "Left" && dpadX < 0) || (dir === "Right" && dpadX > 0)) dpadX = 0
        if (dpadY === 0 && nav.reader && typeof nav.reader.padStick === "function") nav.reader.padStick(0, 0, 0)
    }
    FrameAnimation {
        running: nav.dpadX !== 0 || nav.dpadY !== 0
        onTriggered: {
            const dt = Math.min(0.05, frameTime)
            if (nav.reader && typeof nav.reader.padStick === "function") {
                if (nav.dpadY !== 0) nav.reader.padStick(0, nav.dpadY, dt)
                return
            }
            const fi = nav.window.activeFocusItem
            if (nav.dpadY !== 0) nav.scrollBy(nav.flickOf(fi, true) || nav.flickAt(true), true, -nav.dpadY * 1100 * dt)
            if (nav.dpadX !== 0) nav.scrollBy(nav.flickOf(fi, false) || nav.flickAt(false), false, nav.dpadX * 1100 * dt)
        }
    }

    // ---- right stick ----
    // the player: a seek or volume step now and again while the stick is held (faster the further it goes)
    property real seekWait: 0
    property real volWait: 0
    onRsXChanged: if (rsX === 0) seekWait = 0
    onRsYChanged: if (rsY === 0) volWait = 0
    FrameAnimation {
        running: nav.rsY !== 0 || nav.rsX !== 0
        onTriggered: {
            const dt = Math.min(0.05, frameTime)
            if (nav.player && nav.window.inPlayer) {
                nav.seekWait -= dt
                nav.volWait -= dt
                if (Math.abs(nav.rsX) > 0.3 && nav.seekWait <= 0) {
                    nav.player.action(nav.rsX > 0 ? "seek_forward" : "seek_back")
                    nav.seekWait = 0.5 - 0.35 * Math.abs(nav.rsX)
                }
                if (Math.abs(nav.rsY) > 0.3 && nav.volWait <= 0) {
                    nav.player.action(nav.rsY > 0 ? "volume_up" : "volume_down")
                    nav.volWait = 0.3 - 0.2 * Math.abs(nav.rsY)
                }
                return
            }
            if (nav.reader && typeof nav.reader.padStick === "function" && !nav.isIn(nav.window.activeFocusItem, nav.Overlay.overlay)) {
                nav.reader.padStick(nav.rsX, nav.rsY, dt)
                return
            }
            const fi = nav.window.activeFocusItem
            if (nav.rsY !== 0) nav.scrollBy(nav.flickOf(fi, true) || nav.flickAt(true), true, -nav.rsY * 1400 * dt)
            if (nav.rsX !== 0) nav.scrollBy(nav.flickOf(fi, false) || nav.flickAt(false), false, nav.rsX * 1400 * dt)
        }
    }
}
