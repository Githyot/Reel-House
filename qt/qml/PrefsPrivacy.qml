import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import Reelhouse 1.0

// Preferences > General > IP protection (the core: src/privacy.rs, src/controller_privacy.rs). The app's traffic
// (torrent peers, addons, pictures, the player, the research browser) leaves through a tunnel instead of straight
// from this device: Tor (built in, free), a WireGuard file of any VPN (run inside the app), a SOCKS5/HTTP proxy of
// any VPN, or a VPN app of the system. While it is on, nothing falls back to a direct connection (kill switch).
PrefGroup {
    id: pp
    readonly property var p: Core.privacy || ({})
    readonly property string mode: p.mode || "off"
    readonly property string phase: p.phase || "off"
    readonly property var ids: ["off", "tor", "wireguard", "proxy", "system"]
    function tr(t) { return Core.tr(Core.uiLang, t) }
    function phaseText() {
        if (pp.mode === "off") return pp.tr("Off: the app connects straight, peers and sites see your address")
        if (pp.phase === "connecting") return pp.mode === "tor" && (pp.p.progress || 0) > 0 ? pp.tr("Connecting to Tor…") + " " + pp.p.progress + "%" : pp.tr("Connecting…")
        if (pp.phase === "ready") return pp.mode === "system" ? pp.tr("Protected by the system VPN") : pp.tr("Protected: the app's traffic goes through the tunnel")
        if (pp.phase === "down") return pp.tr("The tunnel dropped: nothing goes out until it is back") + (pp.p.message ? " (" + pp.p.message + ")" : "")
        return pp.tr("Not working: nothing goes out without it") + (pp.p.message ? " (" + pp.p.message + ")" : "")
    }

    title: pp.tr("IP protection (VPN)")
    description: pp.tr("Hides your IP address from torrent peers, addons and sites: the app's traffic goes through a tunnel. While it is on, nothing goes out without it; if the tunnel drops, the torrent stops (kill switch).")

    PrefCombo {
        first: true
        title: pp.tr("Protection")
        subtitle: pp.mode === "tor" ? pp.tr("Free, built in. Slow; good for addons and sites, not for torrents")
                : pp.mode === "wireguard" ? pp.tr("A WireGuard file (.conf) from any VPN, run inside the app")
                : pp.mode === "proxy" ? pp.tr("The SOCKS5 or HTTP proxy of any VPN or server")
                : pp.mode === "system" ? pp.tr("A VPN app of this device does the work; the app stops when it is off")
                : ""
        options: [pp.tr("Off"), pp.tr("Tor (free, built in)"), pp.tr("WireGuard (any VPN)"), pp.tr("Proxy SOCKS5 / HTTP (any VPN)"), pp.tr("System VPN")]
        current: Math.max(0, pp.ids.indexOf(pp.mode))
        onPicked: function(i) {
            var m = pp.ids[i]
            // WireGuard without a file yet: ask for one first
            if (m === "wireguard" && pp.p.wg_has !== true) { wgPick.open(); return }
            if (m === "proxy" && !(pp.p.proxy_url || "")) { proxyEntry.forceFocus(); Core.toast(pp.tr("Type the proxy address, then press ✓")); return }
            Core.cmd({cmd: "privacy_set", mode: m})
        }
    }

    PrefRow {
        visible: pp.mode !== "off"
        title: pp.phaseText()
        subtitle: pp.p.exit_ip ? pp.tr("Address the internet sees:") + " " + pp.p.exit_ip
                : pp.p.check_error ? pp.tr("Could not check the address:") + " " + pp.p.check_error : ""
        Rectangle {
            width: 10; height: 10; radius: 5
            anchors.verticalCenter: parent.verticalCenter
            color: pp.phase === "ready" ? Theme.c.success : pp.phase === "connecting" ? Theme.c.warning : Theme.c.error
        }
        FlatButton {
            anchors.verticalCenter: parent.verticalCenter
            text: pp.tr("Check my IP")
            enabled: pp.phase === "ready"
            onClicked: Core.cmd({cmd: "privacy_check"})
        }
    }

    // ---- Tor ----
    PrefSwitch {
        visible: pp.mode === "tor"
        title: pp.tr("Allow torrents over Tor")
        subtitle: pp.tr("Very slow, and the Tor Project asks people not to. Off: torrents are refused while on Tor (Real-Debrid still works)")
        checked: pp.p.torrent_over_tor === true
        onToggled: function(on) { Core.cmd({cmd: "privacy_set", torrent_over_tor: on}) }
    }

    // ---- WireGuard ----
    PrefButton {
        visible: pp.mode === "wireguard" || pp.mode === "off"
        title: pp.p.wg_has === true ? pp.tr("WireGuard file:") + " " + (pp.p.wg_name || "wg.conf") : pp.tr("WireGuard file")
        subtitle: pp.p.wg_has === true ? (pp.p.wg_summary || "") : pp.tr("Download a .conf from your VPN's site (several have a free plan) and choose it here")
        label: pp.p.wg_has === true ? pp.tr("Change…") : pp.tr("Choose…")
        onActivated: wgPick.open()
    }

    // Cloudflare WARP: free, no account; made only when pressed (Cloudflare is a company and sees the traffic)
    PrefButton {
        visible: pp.mode === "wireguard" || pp.mode === "off"
        title: pp.tr("Free tunnel: Cloudflare WARP")
        subtitle: pp.tr("One tap, no account. Fast and fine for torrents: peers see Cloudflare's address, not yours. Cloudflare sees your real address and answers court orders, so it is not anonymity")
        label: pp.p.wg_name === "Cloudflare WARP" ? pp.tr("Make a new one") : pp.tr("Use WARP")
        onActivated: Core.cmd({cmd: "privacy_warp"})
    }

    // ---- Proxy ----
    PrefEntry {
        id: proxyEntry
        visible: pp.mode === "proxy" || pp.mode === "off"
        apply: true
        title: pp.tr("Proxy address")
        placeholder: "socks5://user:password@server:1080"
        text: pp.p.proxy_url || ""
        function forceFocus() { proxyEntry.forceActiveFocus() }
        onApplied: function(t) { Core.cmd({cmd: "privacy_set", mode: "proxy", proxy_url: t.trim()}) }
    }

    // ---- System VPN ----
    PrefRow {
        visible: pp.mode === "system"
        title: pp.p.system_up === true ? pp.tr("A VPN tunnel is on in the system") : pp.tr("No VPN tunnel is on in the system")
        subtitle: pp.tr("Turn on your VPN app (WireGuard, OpenVPN, Proton VPN…). The app looks for its tunnel every few seconds")
    }

    PrefRow {
        visible: pp.mode !== "off"
        title: pp.tr("What goes through it")
        subtitle: pp.tr("Torrent peers and trackers (no DHT, no incoming connections), addons and sites, covers, the player, Suwayomi and the helper programs. Your home network (Suwayomi on another PC) stays direct.")
    }

    FileDialog {
        id: wgPick
        title: pp.tr("Choose a WireGuard file")
        nameFilters: [pp.tr("WireGuard files") + " (*.conf)", pp.tr("All files") + " (*)"]
        onAccepted: {
            // Android gives a content:// document: copied into the app's data first (the core removes the copy)
            var u = selectedFile.toString()
            var f = Core.importFile(u, "vpn")
            if (f === "") { Core.toast(pp.tr("Could not read that file.")); return }
            Core.cmd({cmd: "privacy_wg_file", path: f, copied: u.indexOf("content:") === 0})
        }
    }
}
