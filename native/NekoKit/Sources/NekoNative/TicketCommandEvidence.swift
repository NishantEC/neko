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
