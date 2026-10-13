import XCTest
import SwiftUI
@testable import NekoNative
import NekoKit

/// Offscreen renders of the shell components in both appearances, for
/// checking a visual change by eye without opening a window (principles §9).
/// The view is hosted in a window that is never ordered on screen, so it can
/// never take keyboard focus. Skipped unless NEKO_RENDER_DIR is set;
/// `scripts/render-previews.sh` sets it and lists the files.
@MainActor final class RenderPreviewsTests: XCTestCase {
    func testRenderShellCatalogue() throws {
        guard let directory = ProcessInfo.processInfo.environment["NEKO_RENDER_DIR"], !directory.isEmpty else {
            throw XCTSkip("Set NEKO_RENDER_DIR to render previews")
        }
        let folder = URL(fileURLWithPath: directory)
        try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        for scheme in [ColorScheme.light, .dark] {
            let suffix = scheme == .dark ? "dark" : "light"
            try render(PreviewCatalogue.page, size: CGSize(width: 900, height: 980), scheme: scheme, to: folder.appendingPathComponent("page-\(suffix).png"))
            try render(PreviewCatalogue.sheet, size: CGSize(width: 620, height: 420), scheme: scheme, to: folder.appendingPathComponent("sheet-\(suffix).png"))
            try render(PreviewCatalogue.controls, size: CGSize(width: 620, height: 360), scheme: scheme, to: folder.appendingPathComponent("controls-\(suffix).png"))
            try render(PreviewCatalogue.settingsSearch, size: CGSize(width: 620, height: 300), scheme: scheme, to: folder.appendingPathComponent("settings-search-\(suffix).png"))
        }
    }

    private func render<V: View>(_ view: V, size: CGSize, scheme: ColorScheme, to url: URL) throws {
        let host = NSHostingView(rootView: view.environment(\.colorScheme, scheme).frame(width: size.width, height: size.height).background(N.canvas))
        host.frame = CGRect(origin: .zero, size: size)
        // An offscreen window gives AppKit-backed controls a real appearance
        // without ever being ordered front or becoming key.
        let window = NSWindow(contentRect: host.frame, styleMask: [.borderless], backing: .buffered, defer: false)
        window.appearance = NSAppearance(named: scheme == .dark ? .darkAqua : .aqua)
        window.contentView = host
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.2))
        guard let bitmap = host.bitmapImageRepForCachingDisplay(in: host.bounds) else { return XCTFail("No bitmap for \(url.lastPathComponent)") }
        host.cacheDisplay(in: host.bounds, to: bitmap)
        guard let png = bitmap.representation(using: .png, properties: [:]) else { return XCTFail("No PNG for \(url.lastPathComponent)") }
        try png.write(to: url)
    }
}

/// Fixed sample content for the renders. Nothing here reaches the daemon.
@MainActor enum PreviewCatalogue {
    static var page: some View {
        NekoPage(title: "Memory", message: "Saved context and working guidance. Suggestions wait for your approval.") {
            NekoSection(title: "Memory settings", grouped: true) {
                NekoToggleRow(title: "Suggest new memories", help: "Propose context from chats and agents.", isOn: .constant(true))
                Divider()
                NekoToggleRow(title: "Use saved memory", help: "Include memory in replies and tasks.", isOn: .constant(false))
            }
            NekoSection(title: "Suggested", marker: NekoStyle.amber, detail: "2 waiting") {
                NekoDividedList(["Prefers small PRs with one concern each.", "Runs the athena tests with yarn, never npm."], id: \.self) { text in
                    VStack(alignment: .leading, spacing: NekoSpace.sm) {
                        NekoReadMore(text: text)
                        NekoItemFooter(meta: "Everywhere") { Button("Keep memory") {}; Button("Dismiss") {} }
                    }
                }
            }
            NekoSection(title: "Remembered", marker: N.text4) {
                NekoEmptyState("Nothing yet. Tell Neko \"remember that…\" in Home, or add one here.")
            }
            NekoSection(title: "Search results") {
                NekoEmptyState(reason: .noMatches("billing"), block: true)
            }
        }
    }

    static var settingsSearch: some View {
        VStack(alignment: .leading, spacing: NekoSpace.lg) {
            NekoSearchField(title: "Find a setting", text: .constant("mo")).frame(width: 220)
            SettingsSearchResults(query: "mo") { _ in }
        }.padding(NekoLayout.pageInset)
    }

    static var sheet: some View {
        NekoSheet(title: "New schedule", message: "Choose what to prepare and when it should run.", error: "A name is required.") {
            Form { Section { TextField("Name", text: .constant("Morning brief")) } }.formStyle(.grouped).frame(height: 120)
        } actions: {
            NekoPrimaryAction(title: "Save") {}
        }
    }

    static var controls: some View {
        VStack(alignment: .leading, spacing: NekoSpace.lg) {
            NekoNotice(kind: .success, text: "Saved your workspace.") {}
            NekoNotice(kind: .warning, text: "Neko isn't connected.") {} actions: { Button("Retry") {}.nekoButton(.secondary) }
            HStack(spacing: NekoSpace.sm) {
                Button("Primary") {}.nekoButton(.primary)
                Button("Secondary") {}.nekoButton(.secondary)
                Button("Quiet") {}.nekoButton(.quiet)
                Button("Delete", role: .destructive) {}.nekoButton(.destructive)
                NekoBadge(count: 3)
            }
            HStack(spacing: NekoSpace.md) {
                ForEach(["Queued", "Building", "AwaitingApproval", "ReadyForReview", "Completed", "Failed"], id: \.self) { StatusGlyph(status: $0, size: 16) }
            }
            NekoRow(symbol: "keyboard", title: "Quick panel shortcut", detail: "Summon Neko from anywhere.") { Text("⌥ Space").font(NekoFont.mono) }
        }.padding(NekoLayout.pageInset)
    }
}
