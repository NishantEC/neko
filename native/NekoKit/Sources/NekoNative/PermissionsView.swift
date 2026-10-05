import SwiftUI
import AppKit
@preconcurrency import ApplicationServices
import NekoKit

/// Everything Neko can be allowed to do on this Mac, in one list. Reading a
/// status never triggers a macOS prompt; only an explicit button does, and
/// only for the one item chosen.
enum PermissionCatalog {
    static let accessibilitySettings = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
    static let filesSettings = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_FilesAndFolders")!

    /// macOS protects these folders per app, and offers no way to read the
    /// decision without asking. Workspaces inside them prompt on first use.
    static func protectedFolders(_ paths: [String], home: String) -> [(folder: String, workspaces: [String])] {
        let names = ["Desktop", "Documents", "Downloads", "Library/Mobile Documents"]
        return names.compactMap { name in
            let root = (home as NSString).appendingPathComponent(name)
            let inside = paths.filter { $0 == root || $0.hasPrefix(root + "/") }
            return inside.isEmpty ? nil : (name == "Library/Mobile Documents" ? "iCloud Drive" : name, inside)
        }
    }
}

struct PermissionsView: View {
    @ObservedObject var model: AppModel
    @State private var trusted = AXIsProcessTrusted()
    @State private var fullDisk = FullDiskAccess.granted
    @State private var clipboard = ClipboardConsentState()
    @State private var pending = false
    private var workspacePaths: [String] {
        model.workspaces.flatMap { ws -> [String] in
            let folders = model.snapshot["workspace_folders"][ws.recordID].array.map(\.string)
            return folders.isEmpty ? [ws["repository"].string] : folders
        }.filter { !$0.isEmpty }
    }
    var body: some View {
        Form {
            Section {
                row("Full Disk Access", "Allows access to protected folders, including Documents, Desktop and iCloud Drive.",
                    status: fullDisk ? "Allowed" : "Not allowed", ok: fullDisk) {
                    Button(fullDisk ? "Open Settings" : "Allow…") { FullDiskAccess.openSettings() }
                }
                row("Accessibility", "Lets Neko paste for you and read the text you have selected. Nothing else on screen is read.",
                    status: trusted ? "Allowed" : "Not allowed", ok: trusted) {
                    if trusted {
                        Button("Open Settings") { NSWorkspace.shared.open(PermissionCatalog.accessibilitySettings) }
                    } else {
                        Button("Allow…") {
                            let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
                            trusted = AXIsProcessTrustedWithOptions(options)
                        }
                        Button("Open Settings") { NSWorkspace.shared.open(PermissionCatalog.accessibilitySettings) }
                    }
                }
                row("Clipboard history", "Saves what you copy, only on this Mac. Turning it off stops new capture; existing history stays until you clear it.",
                    status: clipboard.enabled.map { $0 ? "On" : "Off" } ?? "Unknown", ok: clipboard.enabled != nil) {
                    Toggle("Clipboard history", isOn: Binding(get: { clipboard.enabled ?? false }, set: { value in setClipboard(value) }))
                        .labelsHidden().toggleStyle(.switch).disabled(clipboard.enabled == nil || pending)
                }
            } header: { Text("This Mac") }
            Section {
                let protected = PermissionCatalog.protectedFolders(workspacePaths, home: FileManager.default.homeDirectoryForCurrentUser.path)
                if fullDisk {
                    Text("Full Disk Access is on, so tasks open every workspace without asking.")
                        .font(.callout).foregroundStyle(.secondary)
                } else if protected.isEmpty {
                    Text("None of your workspaces are in a folder macOS protects, so tasks open them without asking.")
                        .font(.callout).foregroundStyle(.secondary)
                } else {
                    ForEach(protected, id: \.folder) { entry in
                        row(entry.folder, "\(entry.workspaces.count == 1 ? "1 workspace is" : "\(entry.workspaces.count) workspaces are") here. macOS asks once, the first time a task opens it, and there is no way to check the answer without asking.",
                            status: "Asked on first use", ok: true) {
                            Button("Allow all folders…") { FullDiskAccess.openSettings() }
                        }
                    }
                }
            } header: { Text("Workspace folders") }
            Section {
                Text("Neko does not record your screen or control your mouse. Authorized agents are local processes with full filesystem access; their worktrees are separate copies, not a security boundary. Publishing or changing external systems needs separate authority.")
                    .font(.callout).foregroundStyle(.secondary)
            } header: { Text("Agent access") }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .task {
            if let reply = try? await model.request(.string("GetClipboardHistoryEnabled")) { try? clipboard.load(reply) }
            while !Task.isCancelled {
                trusted = AXIsProcessTrusted()
                fullDisk = FullDiskAccess.granted
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }
    private func row<Action: View>(_ title: String, _ purpose: String, status: String, ok: Bool, @ViewBuilder action: () -> Action) -> some View {
        HStack(alignment: .top, spacing: 12) {
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 6) {
                    Text(title).font(NekoFont.heading)
                    Text(status).font(.caption.weight(.medium))
                        .foregroundStyle(ok ? Color.secondary : NekoStyle.amber)
                }
                Text(purpose).font(NekoFont.meta).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            HStack(spacing: 6) { action() }
        }.padding(.vertical, 6)
    }
    private func setClipboard(_ enabled: Bool) {
        guard !pending, clipboard.begin(enabled) else { return }
        pending = true
        Task {
            defer { pending = false }
            do {
                try clipboard.finish(try await model.request(.command("SetClipboardHistoryEnabled", ["enabled": .bool(enabled)])))
                model.error = nil
            } catch {
                clipboard.failed()
                model.error = error.localizedDescription
            }
        }
    }
}
