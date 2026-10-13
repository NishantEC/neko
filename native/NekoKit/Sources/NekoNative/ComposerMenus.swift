import SwiftUI
import AppKit
import NekoKit

/// One row of the composer's / or @ menu.
struct ComposerMenuItem: Identifiable {
    let id: String
    let symbol: String
    let title: String
    let detail: String
    var shortcut = ""
    let apply: () -> Void
}

/// What the menu's rows do to the composer that owns them.
struct ComposerMenuActions {
    /// Replaces the trailing "@word" with the chosen mention.
    let replaceMention: (String) -> Void
    /// Switches the composer to Plan and clears the draft.
    let plan: () -> Void
    /// Replaces the whole draft.
    let fill: (String) -> Void
    let send: () -> Void
}

/// Where the composer's / and @ menu stands: which row is highlighted, the draft
/// the person dismissed it for, and the files the last @ search found.
struct ComposerMenuState {
    var index = 0
    var dismissed: String?
    var mentionFiles: [String] = []

    /// The "@word" being typed at the end of the draft, if any.
    func mentionQuery(in draft: String) -> String? {
        guard let last = draft.split(separator: " ", omittingEmptySubsequences: false).last, last.hasPrefix("@"),
              !draft.hasSuffix(" "), dismissed != draft else { return nil }
        return String(last.dropFirst())
    }

    /// The highlighted row among `count`, clamped to the list.
    func selected(of count: Int) -> Int { min(index, count - 1) }

    /// Moves the highlight or dismisses the menu. Accepting a row is the caller's job.
    mutating func navigate(_ key: ComposerMenuKey, count: Int, draft: String) {
        switch key {
        case .up: index = (selected(of: count) - 1 + count) % count
        case .down: index = (selected(of: count) + 1) % count
        case .accept: break
        case .dismiss: dismissed = draft
        }
    }

    /// The rows for `draft`: workspaces, connections and files after "@", or
    /// slash commands after "/". Empty when neither applies or it was dismissed.
    @MainActor func items(for draft: String, model: AppModel, actions: ComposerMenuActions) -> [ComposerMenuItem] {
        guard dismissed != draft else { return [] }
        if let query = mentionQuery(in: draft) {
            let q = query.lowercased()
            var items: [ComposerMenuItem] = []
            for workspace in model.workspaces where q.isEmpty || workspace["name"].string.lowercased().contains(q) {
                items.append(ComposerMenuItem(id: "w:" + workspace.recordID, symbol: "folder", title: workspace["name"].string, detail: "Workspace") {
                    actions.replaceMention("Workspace: " + workspace["name"].string)
                })
            }
            for connection in model.snapshot["mcp"]["connections"].array where connection["enabled"].bool && (q.isEmpty || connection["label"].string.lowercased().contains(q)) {
                let count = connection["tools"].array.count
                items.append(ComposerMenuItem(id: "c:" + connection.recordID, symbol: "puzzlepiece.extension", title: connection["label"].string, detail: "\(count) \(count == 1 ? "tool" : "tools")") {
                    actions.replaceMention("@\(connection["label"].string) ")
                })
            }
            for path in mentionFiles {
                items.append(ComposerMenuItem(id: "f:" + path, symbol: "doc", title: (path as NSString).lastPathComponent, detail: ReplyFiles.location(path)) {
                    actions.replaceMention("\(path) ")
                })
            }
            return Array(items.prefix(9))
        }
        let text = draft.trimmingCharacters(in: .whitespaces).lowercased()
        guard text.hasPrefix("/"), !text.contains(" ") else { return [] }
        var items: [ComposerMenuItem] = []
        if "/plan".hasPrefix(text) {
            items.append(ComposerMenuItem(id: "/plan", symbol: "list.number", title: "/plan", detail: "Plan first, start after you approve") { actions.plan() })
        }
        for entry in SlashCommand.suggestions(for: draft) {
            items.append(ComposerMenuItem(id: entry.name, symbol: "command", title: entry.name, detail: entry.detail, shortcut: entry.name == "/stop" ? "⌘⇧⎋" : "") {
                actions.fill(["/remember", "/forget"].contains(entry.name) ? entry.name + " " : entry.name)
                if !["/remember", "/forget"].contains(entry.name) { actions.send() }
            })
        }
        return items
    }
}

/// The floating list above the composer while a / or @ menu is open.
struct ComposerMenuList: View {
    let items: [ComposerMenuItem]
    @Binding var index: Int
    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                let selected = index == min(self.index, items.count - 1)
                Button { item.apply() } label: {
                    HStack(spacing: 10) {
                        Image(systemName: item.symbol).font(NekoFont.caption).frame(width: 16)
                            .foregroundStyle(selected ? Color(nsColor: .alternateSelectedControlTextColor) : .secondary)
                        Text(item.title).font(NekoFont.body).foregroundStyle(selected ? Color(nsColor: .alternateSelectedControlTextColor) : .primary).frame(minWidth: 84, alignment: .leading)
                        Text(item.detail).font(NekoFont.meta).foregroundStyle(selected ? Color(nsColor: .alternateSelectedControlTextColor).opacity(0.8) : .secondary).lineLimit(1)
                        Spacer(minLength: 8)
                        if !item.shortcut.isEmpty { Text(item.shortcut).font(NekoFont.meta).foregroundStyle(selected ? Color(nsColor: .alternateSelectedControlTextColor).opacity(0.8) : .secondary) }
                    }
                    .padding(.horizontal, NekoSpace.md).frame(minHeight: 34)
                    .background(selected ? Color(nsColor: .selectedContentBackgroundColor) : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.md, style: .continuous))
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .onHover { if $0 { self.index = index } }
            }
        }
        .padding(NekoSpace.sm)
        .liquidGlass(radius: NekoRadius.xl)
        .shadow(color: .black.opacity(0.12), radius: 16, y: 8)
    }
}

/// Finds files for an "@" mention in the selected workspace's folders.
enum ComposerMentionSearch {
    /// Up to five files whose names start with `query`, or nil when the search
    /// was cancelled and the previous results should stay.
    @MainActor static func files(matching query: String?, model: AppModel) async -> [String]? {
        guard let query, query.count >= 2 else { return [] }
        try? await Task.sleep(for: .milliseconds(180))
        guard !Task.isCancelled else { return nil }
        let safe = query.filter { $0.isLetter || $0.isNumber || "._- ".contains($0) }
        guard !safe.isEmpty else { return [] }
        let ids = model.selectedWorkspace.map { [$0] } ?? model.workspaces.map(\.recordID)
        let folders = ids.flatMap { id -> [String] in
            let listed = model.snapshot["workspace_folders"][id].array.map(\.string)
            return listed.isEmpty ? [model.workspaces.first { $0.recordID == id }?["repository"].string ?? ""] : listed
        }.filter { !$0.isEmpty }
        guard !folders.isEmpty else { return [] }
        let found = await Task.detached { () -> [String] in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/mdfind")
            process.arguments = folders.prefix(6).flatMap { ["-onlyin", $0] } + ["kMDItemFSName == '\(safe)*'cd"]
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = FileHandle.nullDevice
            guard (try? process.run()) != nil else { return [] }
            let timer = DispatchWorkItem { if process.isRunning { process.terminate() } }
            DispatchQueue.global().asyncAfter(deadline: .now() + 1.5, execute: timer)
            let data = pipe.fileHandleForReading.readDataToEndOfFile()
            process.waitUntilExit()
            timer.cancel()
            let noisy = ["/node_modules/", "/.git/", "/target/", "/.build/", "/DerivedData/", "/Pods/", "/Library/", "/."]
            return String(decoding: data, as: UTF8.self).split(separator: "\n").map(String.init)
                .filter { path in !noisy.contains { path.contains($0) } }
                .prefix(5).map { $0 }
        }.value
        return Task.isCancelled ? nil : found
    }
}
