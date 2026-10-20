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

    /// A real ticket page, when NEKO_RENDER_SNAPSHOT names a saved workbench
    /// snapshot and NEKO_RENDER_TICKET a ticket in it. NEKO_RENDER_PATCH may
    /// name a diff to serve as its changes. Nothing reaches the daemon.
    func testRenderTicketPage() throws {
        let env = ProcessInfo.processInfo.environment
        guard let directory = env["NEKO_RENDER_DIR"], let snapshotPath = env["NEKO_RENDER_SNAPSHOT"], let ticket = env["NEKO_RENDER_TICKET"] else {
            throw XCTSkip("Set NEKO_RENDER_DIR, NEKO_RENDER_SNAPSHOT and NEKO_RENDER_TICKET to render a ticket page")
        }
        try FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
        let snapshot = try JSONDecoder().decode(JSONValue.self, from: Data(contentsOf: URL(fileURLWithPath: snapshotPath)))
        let patch = env["NEKO_RENDER_PATCH"].flatMap { try? String(contentsOfFile: $0, encoding: .utf8) } ?? ""
        let files = patch.components(separatedBy: "\n").filter { $0.hasPrefix("+++ b/") }.map { JSONValue.string(String($0.dropFirst(6))) }
        let model = AppModel(transport: { _ in .object(["TaskChanges": .object(["files": .array(files), "patch": .string(patch), "truncated": .bool(false)])]) })
        model.snapshot = snapshot
        for scheme in [ColorScheme.dark, .light] {
            try render(TicketDetail(model: model, id: ticket), size: CGSize(width: 900, height: Double(env["NEKO_RENDER_HEIGHT"] ?? "") ?? 3200), scheme: scheme,
                       to: URL(fileURLWithPath: directory).appendingPathComponent("ticket-\(scheme == .dark ? "dark" : "light").png"))
        }
    }

    /// Every page of the real window, when NEKO_RENDER_SNAPSHOT names a saved
    /// workbench snapshot. NEKO_RENDER_TICKET also renders that ticket as an
    /// open agent. Nothing reaches the daemon; requests get an empty answer.
    func testRenderEveryPage() throws {
        let env = ProcessInfo.processInfo.environment
        guard let directory = env["NEKO_RENDER_DIR"], let snapshotPath = env["NEKO_RENDER_SNAPSHOT"], env["NEKO_RENDER_PAGES"] != nil else {
            throw XCTSkip("Set NEKO_RENDER_DIR, NEKO_RENDER_SNAPSHOT and NEKO_RENDER_PAGES=1 to render every page")
        }
        try FileManager.default.createDirectory(atPath: directory, withIntermediateDirectories: true)
        let snapshot = try JSONDecoder().decode(JSONValue.self, from: Data(contentsOf: URL(fileURLWithPath: snapshotPath)))
        let height = Double(env["NEKO_RENDER_HEIGHT"] ?? "") ?? 820
        let pages: [Page] = [.home, .tickets, .pullRequests, .workspaces, .watching, .schedules, .tools, .memory, .profiles, .settings]
        for page in pages {
            let model = AppModel(transport: { _ in .object([:]) })
            model.snapshot = snapshot
            model.loadingSetup = false
            model.onboarding = false
            try render(WorkspaceView(model: model, page: page), size: CGSize(width: 1280, height: height), scheme: .dark,
                       to: URL(fileURLWithPath: directory).appendingPathComponent("page-\(page.rawValue.replacingOccurrences(of: " ", with: "-").replacingOccurrences(of: "&", with: "and")).png"))
        }
        // Surfaces no page reaches on its own: the Rulebook settings page and
        // the chief of staff's action cards in each state.
        let extra = AppModel(transport: { _ in .object([:]) })
        extra.snapshot = snapshot
        extra.loadingSetup = false
        extra.onboarding = false
        try render(SettingsHub(model: extra, section: .rulebook), size: CGSize(width: 1100, height: 820), scheme: .dark,
                   to: URL(fileURLWithPath: directory).appendingPathComponent("page-rulebook.png"))
        if let first = snapshot["tasks"].array.first {
            func action(_ kind: String, _ status: String, _ text: String = "", note: String = "") -> JSONValue {
                .object(["id": .string(kind + status), "task_id": first["id"], "kind": .string(kind), "text": .string(text), "status": .string(status), "note": .string(note)])
            }
            let cards = VStack(alignment: .leading, spacing: NekoSpace.md) {
                ChatActionCard(model: extra, messageID: "m", action: action("message", "Proposed", "Use the 375px designs from HEA-2621 and keep the existing breakpoints."))
                ChatActionCard(model: extra, messageID: "m", action: action("run_next", "Done"))
                ChatActionCard(model: extra, messageID: "m", action: action("merge", "Proposed", snapshot["tasks"].array.last?["id"].string ?? ""))
                ChatActionCard(model: extra, messageID: "m", action: action("stop", "Declined"))
                ChatActionCard(model: extra, messageID: "m", action: action("approve", "Failed", note: "This ticket is already working"))
            }.padding(NekoLayout.pageInset)
            for scheme in [ColorScheme.dark, .light] {
                try render(cards, size: CGSize(width: 760, height: 780), scheme: scheme,
                           to: URL(fileURLWithPath: directory).appendingPathComponent("chat-actions-\(scheme == .dark ? "dark" : "light").png"))
            }
        }
        // The split view's sidebar column is a vibrancy material that
        // cacheDisplay cannot capture, so the sidebar is also drawn alone.
        let sidebarModel = AppModel(transport: { _ in .object([:]) })
        sidebarModel.snapshot = snapshot
        try render(NativeSidebar(model: sidebarModel, page: .constant(.tickets), addingWorkspace: .constant(false)),
                   size: CGSize(width: 280, height: 820), scheme: .dark,
                   to: URL(fileURLWithPath: directory).appendingPathComponent("page-sidebar.png"))
        // All tasks as a tree of who started whom.
        let tree = AgentTree.build(snapshot: snapshot, tasks: snapshot["tasks"].array, workspace: nil,
                                   now: Int(Date().timeIntervalSince1970 * 1000))
        for scheme in [ColorScheme.dark, .light] {
            try render(AgentTreeView(tree: tree, open: { _ in }, openHome: {}, openWatching: {}),
                       size: CGSize(width: 1280, height: height), scheme: scheme,
                       to: URL(fileURLWithPath: directory).appendingPathComponent("agent-tree-\(scheme == .dark ? "dark" : "light").png"))
        }
        if let ticket = env["NEKO_RENDER_TICKET"] {
            let model = AppModel(transport: { _ in .object([:]) })
            model.snapshot = snapshot
            model.loadingSetup = false
            model.onboarding = false
            model.agentID = ticket
            try render(WorkspaceView(model: model, page: .tickets), size: CGSize(width: 1280, height: 820), scheme: .dark,
                       to: URL(fileURLWithPath: directory).appendingPathComponent("page-ticket-open.png"))
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
        RunLoop.main.run(until: Date().addingTimeInterval(1.0))
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
