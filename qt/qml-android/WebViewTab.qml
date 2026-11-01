import QtQuick
import QtWebView

// One tab of Research mode on Android: a page in the Android System WebView, made and driven by webview_host.cpp.
// It reports to `host` (the engine bridge there), which passes it on to its WebPane as Chromium's events would be.
// Only built into the Android app (this file also tells androiddeployqt to package Qt WebView's QML module).
WebView {
    property int tabId: 0
    property QtObject host: null
    anchors.fill: parent
    function report() { if (host) host.state(tabId, url.toString(), title, loading, canGoBack, canGoForward) }
    onUrlChanged: report()
    onTitleChanged: report()
    onLoadingChanged: function(r) { report(); if (host) host.loaded(tabId, r.status, r.url.toString(), r.errorString || "") }
}
