import AppKit
import SwiftUI
import NekoKit

/// A ticket opened from Work. The header owns how it is shown, so you change
/// the view from inside the ticket: side peek, centered window, or full page.
struct TicketPeek: View {
    @ObservedObject var model: AppModel
    let id: String
    @Binding var mode: String
    let close: () -> Void
    @State private var copied = false

    static let modes: [(value: String, title: String, symbol: String)] = [
        ("drawer", "Side peek", "sidebar.right"),
        ("modal", "Window", "rectangle.center.inset.filled"),
        ("full", "Full page", "rectangle.inset.filled")
    ]
    private var ticket: JSONValue { model.snapshot["tasks"].array.first { $0.recordID == id } ?? .null }
    private var shortID: String { "NEK-" + String(id.prefix(4)).uppercased() }
    private var current: (value: String, title: String, symbol: String) { Self.modes.first { $0.value == mode } ?? Self.modes[0] }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider().opacity(0.5)
            TicketDetail(model: model, id: id, close: close)
                .frame(maxWidth: mode == "full" ? 860 : .infinity)
                .frame(maxWidth: .infinity)
        }
    }

    private var header: some View {
        HStack(spacing: 4) {
            iconButton("xmark", help: "Close (Esc)", action: close)
            if mode != "full" {
                iconButton("arrow.up.left.and.arrow.down.right", help: "Open as full page") { mode = "full" }
            }
            Menu {
                Picker("Open tickets as", selection: $mode) {
                    ForEach(Self.modes, id: \.value) { option in Label(option.title, systemImage: option.symbol).tag(option.value) }
                }.pickerStyle(.inline)
            } label: { Image(systemName: current.symbol) }
                .menuStyle(.borderlessButton).menuIndicator(.visible).fixedSize()
                .help("Open tickets as: \(current.title)")
            Text(shortID).font(.system(size: 11, weight: .medium, design: .monospaced)).foregroundStyle(.secondary).padding(.leading, 6)
            Spacer(minLength: 8)
            iconButton(copied ? "checkmark" : "link", help: "Copy ticket ID") {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString("\(shortID) \(ticket["title"].string)", forType: .string)
                copied = true
                Task { try? await Task.sleep(for: .seconds(1.2)); copied = false }
            }
            if !ticket["worktree_path"].string.isEmpty {
                iconButton("folder", help: "Show worktree in Finder") {
                    NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: ticket["worktree_path"].string)])
                }
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 38)
    }

    private func iconButton(_ symbol: String, help: String, action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: symbol).frame(width: 24, height: 24).contentShape(Rectangle()) }
            .buttonStyle(.borderless).help(help)
    }
}
