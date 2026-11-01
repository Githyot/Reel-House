.pragma library
// Key presses as GTK accelerator names ("space", "<Shift>Left", "<Control>s"), the form the shortcuts are saved in
// (GTK shortcuts.rs), so the Qt and GTK versions share the same settings file.

var names = {}
function k(code, name) { names[code] = name }
k(0x20, "space")                // Qt.Key_Space
k(0x01000012, "Left"); k(0x01000014, "Right"); k(0x01000013, "Up"); k(0x01000015, "Down")
k(0x01000000, "Escape"); k(0x01000004, "Return"); k(0x01000005, "KP_Enter"); k(0x01000001, "Tab")
k(0x01000003, "BackSpace"); k(0x01000007, "Delete"); k(0x01000006, "Insert")
k(0x01000010, "Home"); k(0x01000011, "End"); k(0x01000016, "Page_Up"); k(0x01000017, "Page_Down")
k(0x5b, "bracketleft"); k(0x5d, "bracketright"); k(0x2c, "comma"); k(0x2e, "period"); k(0x2d, "minus")
k(0x3d, "equal"); k(0x2f, "slash"); k(0x5c, "backslash"); k(0x3b, "semicolon"); k(0x27, "apostrophe")
k(0x60, "grave"); k(0x2b, "plus"); k(0x2a, "asterisk")
k(0x01000080, "MediaPlay"); k(0x010000a1, "MediaPause")
for (var f = 1; f <= 12; ++f) k(0x01000030 + f - 1, "F" + f)

var MODS = [[0x02000000, "Shift"], [0x04000000, "Control"], [0x08000000, "Alt"], [0x10000000, "Super"]]

// The key name of a Qt key code, or "" for a lone modifier and unknown keys.
function keyName(key) {
    if (names[key] !== undefined) return names[key]
    if (key >= 0x41 && key <= 0x5a) return String.fromCharCode(key + 32)     // letters, lower case like GTK to_lower()
    if (key >= 0x30 && key <= 0x39) return String.fromCharCode(key)
    return ""
}

function isModifier(key) { return key >= 0x01000020 && key <= 0x01000026 }

// "<Shift><Control>x" from a QML KeyEvent.
function fromEvent(e) {
    var name = keyName(e.key)
    if (name === "") return ""
    var out = ""
    for (var i = 0; i < MODS.length; ++i) if (e.modifiers & MODS[i][0]) out += "<" + MODS[i][1] + ">"
    return out + name
}

// The same key whatever the order of the modifiers ("<Control><Shift>a" = "<Shift><Control>a").
function canonical(accel) {
    var mods = []
    var rest = accel
    while (rest.charAt(0) === "<") {
        var i = rest.indexOf(">")
        if (i < 0) break
        var m = rest.substring(1, i)
        if (m === "Primary" || m === "Ctrl") m = "Control"
        mods.push(m)
        rest = rest.substring(i + 1)
    }
    mods.sort()
    if (rest.length === 1) rest = rest.toLowerCase()
    return mods.join("+") + "|" + rest
}

function matches(e, accel) {
    var n = fromEvent(e)
    return n !== "" && canonical(n) === canonical(accel)
}

var SEQ = {space: "Space", BackSpace: "Backspace", Escape: "Esc", Return: "Return", KP_Enter: "Enter", Page_Up: "PgUp", Page_Down: "PgDown",
           bracketleft: "[", bracketright: "]", comma: ",", period: ".", minus: "-", equal: "=", slash: "/", backslash: "\\",
           semicolon: ";", apostrophe: "'", grave: "`", plus: "+", asterisk: "*", MediaPlay: "Media Play", MediaPause: "Media Pause"}

// A Qt key sequence ("Shift+Left") from a GTK accelerator name ("<Shift>Left").
function toSequence(accel) {
    var out = []
    var rest = accel
    while (rest.charAt(0) === "<") {
        var i = rest.indexOf(">")
        if (i < 0) break
        var m = rest.substring(1, i)
        out.push(m === "Control" || m === "Primary" ? "Ctrl" : (m === "Super" ? "Meta" : m))
        rest = rest.substring(i + 1)
    }
    out.push(SEQ[rest] !== undefined ? SEQ[rest] : (rest.length === 1 ? rest.toUpperCase() : rest))
    return out.join("+")
}
