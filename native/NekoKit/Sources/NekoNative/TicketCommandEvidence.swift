import SwiftUI
import NekoKit

/// A saved host receipt, not an interpretation of terminal text. Older stores
/// may contain a JSON prefix cut off at the event limit; keep that distinguishable
/// from both a successful command and a command that returned a failure code.
struct TicketCommandReceipt: Equatable {
    static let prefix = "VERIFICATION_COMMAND "
    static func from(event: JSONValue) -> Self? {
        // A human can paste anything into a reply, including this internal
        // prefix. Its content remains a message, never execution evidence.
        guard ["scout", "supervisor", "builder", "reviewer", "coordinator"].contains(event["role"].string) else { return nil }
        return Self(message: event["message"].string)
    }
    let rawRecord: String
    let command: String?
    let output: String?
    let exitCode: Int?
    let commandTruncated: Bool
    let outputTruncated: Bool
    let receiptIncomplete: Bool

    init?(message: String) {
        guard message.hasPrefix(Self.prefix) else { return nil }
        rawRecord = message
        let value = (try? JSONDecoder().decode(JSONValue.self, from: Data(message.dropFirst(Self.prefix.count).utf8))) ?? .null
        if case .string(let text) = value["command"], !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { command = text }
        else { command = nil }
        if case .string(let text) = value["output"] { output = text }
        else { output = nil }
        if case .number(let code) = value["exit_code"], code.isFinite,
           code >= Double(Int32.min), code <= Double(Int32.max), code.rounded() == code { exitCode = Int(code) }
        else { exitCode = nil }
        commandTruncated = value["command_truncated"].bool
        outputTruncated = value["output_truncated"].bool
        // Unknown flag types are malformed, rather than equivalent to false.
        receiptIncomplete = value["receipt_incomplete"].bool || value["receipt_malformed"].bool || value["raw"] != .null
            || ["command_truncated", "output_truncated", "receipt_incomplete", "receipt_malformed", "raw_truncated"].contains {
            value.object[$0] != nil && value[$0] != .bool(true) && value[$0] != .bool(false)
        }
    }

    var isIncomplete: Bool { receiptIncomplete || command == nil || output == nil || exitCode == nil || commandTruncated }
    var failed: Bool { exitCode.map { $0 != 0 } ?? false }
    var title: String { command.map(TicketThread.shortCommand) ?? "Command details unavailable" }
    var status: String {
        if isIncomplete { return failed ? "Failed · exit \(exitCode!) · incomplete record" : "Incomplete record" }
        return failed ? "Failed · exit \(exitCode!)" : "Completed · exit 0"
    }
    var notices: [String] {
        var messages: [String] = []
        if isIncomplete { messages.append("This saved record is incomplete. It does not provide full command evidence.") }
        if commandTruncated { messages.append("The command was shortened before it was saved.") }
        if outputTruncated { messages.append("Output was shortened before it was saved. Only the retained output is available here.") }
        return messages
    }
}

struct TicketCommandReceiptView: View {
    let receipt: TicketCommandReceipt
    var role: String? = nil
    @State private var copied = false

    var body: some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                ForEach(receipt.notices, id: \.self) { notice in
                    Label(notice, systemImage: "exclamationmark.triangle")
                        .font(.caption).foregroundStyle(NekoStyle.amber)
                        .fixedSize(horizontal: false, vertical: true)
                }
                if let command = receipt.command {
                    Text(command).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                if let output = receipt.output {
                    Text(receipt.outputTruncated ? "Saved output · shortened" : "Saved output").font(.caption).foregroundStyle(.secondary)
                    if output.isEmpty {
                        Text("No output was recorded.").font(.caption).foregroundStyle(.secondary)
                    } else {
                        ScrollView([.horizontal, .vertical]) {
                            Text(output).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                                .fixedSize(horizontal: true, vertical: false).padding(10)
                        }.frame(maxHeight: 220)
                            .background(Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 8))
                    }
                }
                HStack {
                    Button(copied ? "Copied" : "Copy saved record", systemImage: copied ? "checkmark" : "doc.on.doc") {
                        NSPasteboard.general.clearContents()
                        copied = NSPasteboard.general.setString(receipt.rawRecord, forType: .string)
                    }.controlSize(.small).help("Copy only the record retained by Neko, including its truncation flags")
                    Spacer()
                }
                if receipt.isIncomplete {
                    DisclosureGroup("Raw saved record") {
                        Text(receipt.rawRecord).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }.font(.caption)
                }
            }.padding(.top, 8)
        } label: {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 6) {
                    if let role { Text(TicketThread.agentName(role)); Text("·") }
                    Text(receipt.status)
                    if receipt.outputTruncated { Text("· Shortened output") }
                }.font(.caption).foregroundStyle(receipt.isIncomplete || receipt.failed ? NekoStyle.amber : .secondary)
                Text(receipt.title).font(.system(size: 12, design: .monospaced)).lineLimit(2)
            }
        }
        .onChange(of: receipt) { _, _ in copied = false }
    }
}

/// Kept separate from the review verdict: retained activity can span several
/// attempts and is not a complete journal of the current review.
struct TicketCommandEvidenceView: View {
    let events: [JSONValue]
    var body: some View {
        let records = events.compactMap { event -> (role: String, receipt: TicketCommandReceipt)? in
            guard let receipt = TicketCommandReceipt.from(event: event) else { return nil }
            return (event["role"].string, receipt)
        }
        if !records.isEmpty {
            DisclosureGroup {
                VStack(alignment: .leading, spacing: 14) {
                    Text("Retained commands across this agent’s history. These may include earlier attempts; older activity may no longer be saved.")
                        .font(.caption).foregroundStyle(.secondary)
                    ForEach(Array(records.enumerated()), id: \.offset) { _, record in
                        TicketCommandReceiptView(receipt: record.receipt, role: record.role)
                    }
                }.padding(.top, 10)
            } label: {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Saved command evidence · \(records.count)").font(.subheadline)
                    let incomplete = records.filter { $0.receipt.isIncomplete }.count
                    let shortened = records.filter { $0.receipt.outputTruncated }.count
                    if incomplete > 0 || shortened > 0 {
                        Text([incomplete > 0 ? "\(incomplete) incomplete records" : nil,
                              shortened > 0 ? "\(shortened) shortened outputs" : nil].compactMap { $0 }.joined(separator: " · "))
                            .font(.caption).foregroundStyle(NekoStyle.amber)
                    }
                }
            }
            .padding(14)
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(Color.primary.opacity(0.12)))
        }
    }
}
