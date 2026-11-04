import QtQuick
import QtQuick.Controls
import Reelhouse 1.0

// The local AI of Conversation (the core: ollama_local.rs, controller_kaiwa.rs). Shows whether Ollama is ready on
// this computer and, the first time, downloads the program (Windows) and the model with a progress bar.
// Self-contained: `OllamaPanel { width: parent.width }`. Reads Core.state("ollama"); calls ollama_setup /
// ollama_status / ollama_stop.
Rectangle {
    id: op
    property var o: ({})
    readonly property string phase: op.o.phase || "idle"
    readonly property bool busy: op.o.busy === true
    readonly property bool isReady: op.phase === "ready"
    readonly property bool failed: op.phase === "error"
    readonly property real pct: Number(op.o.percent || 0)
    readonly property bool hasBytes: Number(op.o.total || 0) > 0
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function refresh() { op.o = Core.state("ollama") || ({}) }
    function gb(b) { return (Number(b || 0) / 1073741824).toFixed(2) + " GB" }
    function phaseText() {
        if (op.failed) return op.o.message || op.tr("The local AI could not be started.")
        if (op.phase === "idle") return op.tr("Runs on this computer, free and private. The first time, Reelhouse downloads it (about 3 GB).")
        return op.tr(op.o.message || "")
    }

    implicitHeight: col.implicitHeight + 28
    radius: Theme.shape.card
    color: Theme.c.card
    border.color: Theme.c.border
    border.width: 1

    Connections { target: Core; function onStudyChanged() { if (op.visible) op.refresh() } }
    // a backup for the progress while a setup runs (the core also sends a change now and then)
    Timer { interval: 1000; repeat: true; running: op.visible && op.busy; onTriggered: op.refresh() }
    onVisibleChanged: if (visible) { Core.cmd({cmd: "ollama_status"}); refresh() }
    Component.onCompleted: { if (visible) Core.cmd({cmd: "ollama_status"}); refresh() }

    Column {
        id: col
        x: 14
        y: 14
        width: op.width - 28
        spacing: 10

        Row {
            width: parent.width
            spacing: 10
            Rectangle {
                width: 10
                height: 10
                radius: 5
                anchors.verticalCenter: parent.verticalCenter
                color: op.isReady ? Theme.c.success : op.failed ? Theme.c.error : op.busy ? Theme.c.warning : Theme.c.textDim
            }
            Lbl {
                text: op.tr("Local AI (Ollama)") + (op.o.model ? "  ·  " + op.o.model : "")
                bold: true
                width: parent.width - 20
                elide: Text.ElideRight
            }
        }

        Lbl {
            width: parent.width
            text: op.phaseText()
            dim: !op.failed
            color: op.failed ? Theme.c.errorText : Theme.c.textDim
            wrapMode: Text.Wrap
        }

        // the progress of the download (the program, then the model)
        Column {
            width: parent.width
            spacing: 4
            visible: op.busy && (op.phase === "downloading_runtime" || op.phase === "pulling_model")
            Rectangle {
                width: parent.width
                height: 8
                radius: 4
                color: Theme.c.fill3
                Rectangle {
                    visible: op.hasBytes
                    width: parent.width * Math.max(0, Math.min(1, op.pct / 100))
                    height: parent.height
                    radius: parent.radius
                    color: Theme.c.accent
                    Behavior on width { NumberAnimation { duration: 250 } }
                }
                // no size known yet: a short bar slides back and forth
                Rectangle {
                    visible: !op.hasBytes
                    width: parent.width * 0.25
                    height: parent.height
                    radius: parent.radius
                    color: Theme.c.accent
                    SequentialAnimation on x {
                        running: op.visible && !op.hasBytes && op.busy
                        loops: Animation.Infinite
                        NumberAnimation { from: 0; to: col.width * 0.75; duration: 900 }
                        NumberAnimation { from: col.width * 0.75; to: 0; duration: 900 }
                    }
                }
            }
            Lbl {
                visible: op.hasBytes
                text: op.pct.toFixed(1) + "%  ·  " + op.gb(op.o.done) + " / " + op.gb(op.o.total)
                dim: true
                size: 12
            }
        }

        Row {
            spacing: 8
            visible: !op.busy
            FlatButton {
                visible: !op.isReady
                kind: "suggested"
                text: op.failed ? op.tr("Try again") : op.tr("Download and turn on the local AI")
                onClicked: { Core.cmd({cmd: "ollama_setup"}); op.refresh() }
            }
            FlatButton {
                visible: op.isReady && op.o.ours === true
                text: op.tr("Stop the local AI")
                onClicked: { Core.cmd({cmd: "ollama_stop"}); op.refresh() }
            }
        }

        Lbl {
            visible: !!op.o.dir && (op.o.canDownload === true || op.o.ours === true)
            width: parent.width
            text: op.tr("Files in %1").replace("%1", op.o.dir || "")
            dim: true
            size: 12
            wrapMode: Text.WrapAnywhere
        }
    }
}
