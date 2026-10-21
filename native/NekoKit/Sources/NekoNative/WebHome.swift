import SwiftUI
import WebKit

/// Where the daemon's web interface lives and how to sign in to it. The daemon
/// (crates/neko-daemon/src/web.rs) listens on 127.0.0.1 only and wants the
/// secret it keeps in `<data dir>/web-token`; this reads that file, so the
/// token never has to be typed, stored here or passed around.
enum WebHomeLocation {
    static func port(environment: [String: String] = ProcessInfo.processInfo.environment) -> Int? {
        guard let raw = environment["NEKO_WEB_PORT"] else { return 4747 }
        guard let value = Int(raw.trimmingCharacters(in: .whitespaces)) else { return 4747 }
        return value == 0 ? nil : value
    }

    static func dataDirectory(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        if let override = environment["NEKO_DATA_DIR"], override.hasPrefix("/") { return URL(fileURLWithPath: override) }
        return URL(fileURLWithPath: (environment["HOME"] ?? "/tmp") + "/Library/Application Support/neko")
    }

    /// `http://127.0.0.1:<port>/?token=…`, or nil when the bridge is off or the
    /// daemon has not written its token yet.
    static func url(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL? {
        guard let port = port(environment: environment),
              let token = try? String(contentsOf: dataDirectory(environment: environment).appendingPathComponent("web-token"), encoding: .utf8)
                .trimmingCharacters(in: .whitespacesAndNewlines),
              token.count == 64, token.allSatisfy(\.isHexDigit)
        else { return nil }
        return URL(string: "http://127.0.0.1:\(port)/?token=\(token)")
    }
}

/// The web interface inside the native window. The page background is left
/// transparent (the web app marks itself `data-shell="native"` and drops its own
/// fill), so what shows is the window's own canvas, like every other page.
struct WebHomeView: View {
    @State private var url = WebHomeLocation.url()
    @State private var failure: String?
    @State private var generation = 0

    var body: some View {
        ZStack {
            if let url, failure == nil {
                WebHomeRepresentable(url: url, failure: $failure).id(generation)
            } else {
                NekoEmptyState(title: "The web interface isn’t reachable",
                               message: failure ?? "Neko’s daemon has not published its web address yet. It starts with the daemon; try again in a moment.",
                               symbol: "globe") {
                    Button("Try again") { failure = nil; url = WebHomeLocation.url(); generation += 1 }
                        .nekoButton(.secondary)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .onAppear { if url == nil { url = WebHomeLocation.url() } }
    }
}

private struct WebHomeRepresentable: NSViewRepresentable {
    let url: URL
    @Binding var failure: String?

    func makeCoordinator() -> Coordinator { Coordinator(failure: $failure) }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        // Tells the page it lives inside the app, so it can go transparent.
        let marker = WKUserScript(source: "document.documentElement.dataset.shell = 'native'", injectionTime: .atDocumentStart, forMainFrameOnly: true)
        configuration.userContentController.addUserScript(marker)
        let view = WKWebView(frame: .zero, configuration: configuration)
        view.navigationDelegate = context.coordinator
        view.uiDelegate = context.coordinator
        view.setValue(false, forKey: "drawsBackground")
        view.allowsMagnification = false
        view.load(URLRequest(url: url))
        return view
    }

    func updateNSView(_ view: WKWebView, context: Context) {}

    final class Coordinator: NSObject, WKNavigationDelegate, WKUIDelegate {
        @Binding var failure: String?
        init(failure: Binding<String?>) { _failure = failure }

        /// Only the daemon's own address loads here; everything else opens in the browser.
        func webView(_ webView: WKWebView, decidePolicyFor action: WKNavigationAction) async -> WKNavigationActionPolicy {
            guard let target = action.request.url else { return .cancel }
            if target.host == "127.0.0.1" { return .allow }
            if target.scheme == "http" || target.scheme == "https" { NSWorkspace.shared.open(target) }
            return .cancel
        }

        /// `target="_blank"` links (the web app's own) go to the browser too.
        func webView(_ webView: WKWebView, createWebViewWith configuration: WKWebViewConfiguration, for action: WKNavigationAction, windowFeatures: WKWindowFeatures) -> WKWebView? {
            if let target = action.request.url, target.scheme == "http" || target.scheme == "https" { NSWorkspace.shared.open(target) }
            return nil
        }

        func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
            failure = "Couldn’t reach Neko’s web interface (\(error.localizedDescription))."
        }
    }
}
