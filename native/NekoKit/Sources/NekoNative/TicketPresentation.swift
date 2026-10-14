import SwiftUI
import NekoKit

enum TicketPresentation {
    /// The planner's question for a ticket waiting on you, if it asked one.
    static func waitingReason(_ task: JSONValue) -> String? {
        guard task["status"].string == "AwaitingApproval" else { return nil }
        let decision = task["supervision"]
        if decision["action"].string == "AskUser" { return "Needs your input: " + decision["reason"].string }
        if decision["action"].string == "Skip" { return "Nothing to build: " + decision["reason"].string }
        if let event = task["events"].array.last(where: { ["supervisor", "coordinator"].contains($0["role"].string) }) {
            let message = event["message"].string
            for prefix in ["Needs your input before building: ", "Nothing to build: "] where message.hasPrefix(prefix) {
                return (prefix.hasPrefix("Needs") ? "Needs your input: " : "Nothing to build: ") + message.dropFirst(prefix.count)
            }
        }
        return nil
    }
    /// The newest daemon message on a stopped ticket, in one readable line.
    static func stopReason(_ task: JSONValue) -> String {
        let message = task["events"].array.reversed().first { !["user", "note"].contains($0["role"].string) }?["message"].string ?? ""
        let line = message.trimmingCharacters(in: .whitespacesAndNewlines)
        if line.contains("not a git repository") {
            return "This task isn’t linked to a Git repository yet, so Neko couldn’t make its working copy."
        }
        if line.contains("daemon restart") { return "Stopped when Neko restarted. Start it again to continue." }
        if line.hasPrefix("Independent verification failed:") {
            return "Review found problems: " + line.dropFirst("Independent verification failed:".count).trimmingCharacters(in: .whitespaces).prefix(380)
        }
        if line.hasPrefix("Verifier returned a malformed verdict") { return "The reviewer’s answer couldn’t be read. Start it again to re-review." }
        return line.isEmpty ? "Neko stopped without a reason. Start it again to retry." : String(line.prefix(400))
    }
    struct Review: Decodable {
        let passed: Bool
        let findings: [String]
        let files: [String]
        let tests: [String]
        let summary: String
        private enum CodingKeys: String, CodingKey { case passed, findings, files, tests, summary }
        /// Findings arrive either as plain strings or as {severity, path, finding}
        /// objects; both render as one readable line.
        struct Finding: Decodable {
            let text: String
            init(from decoder: Decoder) throws {
                if let plain = try? decoder.singleValueContainer().decode(String.self) { text = plain; return }
                let value = try JSONValue(from: decoder)
                let severity = value["severity"].string, path = value["path"].string
                let body = value["finding"].string.isEmpty ? value["message"].string : value["finding"].string
                text = [severity.isEmpty ? nil : severity.capitalized, path.isEmpty ? nil : (path as NSString).lastPathComponent].compactMap { $0 }.joined(separator: " · ") + (severity.isEmpty && path.isEmpty ? "" : ": ") + body
            }
        }
        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            passed = try c.decode(Bool.self, forKey: .passed)
            // Every field is required: a malformed reply must never read as a pass.
            findings = try c.decode([Finding].self, forKey: .findings).map { $0.text }
            files = try c.decode([String].self, forKey: .files)
            tests = try c.decode([String].self, forKey: .tests)
            summary = try c.decode(String.self, forKey: .summary)
        }
    }
    static func review(_ result: String) -> (body: String, verdict: Review?) {
        guard let range = result.range(of: "\n\nIndependent review:\n", options: .backwards),
              let data = String(result[range.upperBound...]).data(using: .utf8),
              let verdict = try? JSONDecoder().decode(Review.self, from: data) else { return (result, nil) }
        return (String(result[..<range.lowerBound]), verdict)
    }
    /// Saved plans and results survive replies/retries. Only the ticket's current
    /// phase decides whether they belong at the latest end or in previous work.
    struct Conversation {
        let foldsHistory: Bool
        let showsPlan: Bool
        let planTitle: String
        let outcome: Outcome
    }
    enum Outcome: Equatable {
        case ready, completed, reviewing, stopped, previous

        var belongsInHistory: Bool { self == .previous }
        var reviewIsCurrent: Bool { self == .ready || self == .completed }
        var title: String {
            switch self {
            case .ready, .completed: "Result"
            case .reviewing: "Result under review"
            case .stopped: "Last recorded result"
            case .previous: "Previous result"
            }
        }
        var note: String {
            switch self {
            case .ready: "Ready for your review"
            case .completed: "Task completed"
            case .reviewing: "Independent review is in progress. This output has not cleared the current review."
            case .stopped: "Work stopped. This saved output may be from an earlier attempt."
            case .previous: "Saved from earlier work; it does not describe the current attempt."
            }
        }
    }
    static func conversation(_ ticket: JSONValue) -> Conversation {
        let status = ticket["status"].string
        let outcome: Outcome
        switch status {
        case "ReadyForReview": outcome = .ready
        case "Completed": outcome = .completed
        case "Reviewing": outcome = .reviewing
        case "Failed", "Cancelled": outcome = .stopped
        default: outcome = .previous
        }
        return Conversation(
            foldsHistory: ["ReadyForReview", "Completed", "Failed", "Cancelled"].contains(status),
            showsPlan: ["Planning", "AwaitingApproval"].contains(status)
                && !ticket["plan"].string.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
            planTitle: status == "Planning" ? "Saved plan · being revised" : "Plan",
            outcome: outcome
        )
    }
    static func canProposeSplit(_ ticket: JSONValue, splits: [JSONValue]) -> Bool {
        ticket["status"].string == "AwaitingApproval" && !splits.contains { split in
            split["parent_id"] == ticket["id"] || split["subtasks"].array.contains { $0["task_id"] == ticket["id"] }
        }
    }
}
