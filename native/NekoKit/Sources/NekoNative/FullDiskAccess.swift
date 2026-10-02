import SwiftUI
import AppKit

/// Full Disk Access is the one grant that covers every folder Neko, its search
/// and its agents might open: Documents, Desktop, Downloads, iCloud Drive,
/// external drives and other apps' data. Without it macOS asks folder by folder.
enum FullDiskAccess {
    static let settings = URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles")!

    /// Reads a file only Full Disk Access can open. A refusal here is silent:
    /// checking never shows a macOS prompt.
    static var granted: Bool {
        let probe = FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/Application Support/com.apple.TCC/TCC.db").path
        guard let handle = FileHandle(forReadingAtPath: probe) else { return false }
        try? handle.close()
        return true
    }

    static func openSettings() { NSWorkspace.shared.open(settings) }
}

/// Shown once, above every page, until Full Disk Access is on or the person
/// says no. Afterwards the Permissions page is the only place it appears.
struct FullDiskAccessBanner: View {
    @AppStorage("neko.fullDiskAccess.dismissed") private var dismissed = false
    @State private var granted = FullDiskAccess.granted
    @State private var opened = false
    var body: some View {
        Group {
            if !granted && !dismissed {
                HStack(spacing: 12) {
                    Image(systemName: "folder.badge.gearshape").font(.system(size: 18)).foregroundStyle(NekoStyle.amber)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(opened ? "Turn on Neko in the list, then reopen Neko" : "Allow Neko once, instead of folder by folder").font(NekoFont.body.weight(.semibold))
                        Text(opened ? "System Settings → Privacy & Security → Full Disk Access. macOS offers to reopen Neko when you switch it on."
                                    : "Full Disk Access lets search and agents open any folder without asking again.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                    Spacer()
                    Button(opened ? "Open Settings again" : "Allow access…") { FullDiskAccess.openSettings(); opened = true }.nekoGlassButton()
                    Button("Not now") { dismissed = true }.buttonStyle(.plain).foregroundStyle(.secondary)
                }
                .nekoCard(padding: 12, radius: 12).padding(.horizontal, 16).padding(.top, 8)
            }
        }
        .task {
            while !Task.isCancelled && !granted && !dismissed {
                try? await Task.sleep(for: .seconds(2))
                granted = FullDiskAccess.granted
            }
        }
    }
}
