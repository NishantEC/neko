import Foundation
import NekoKit

// The pure half of the chat's work log and live reply: what a turn did in one
// sentence, how long it worked, how a streaming reply splits into finished
// blocks and a live tail, and how much room a just-sent message needs under
// it. No views here, so every rule is unit tested.

/// What a turn did, counted from its short activity lines ("Ran `git status`",
/// "Ran `cargo test` · failed") and its tool calls.
struct ChatWorkSummary: Equatable {
    /// Order is priority: what changed things leads what only looked.
    enum Kind: Int, CaseIterable, Comparable {
        case command, edit, read, search, tool, other
        static func < (a: Self, b: Self) -> Bool { a.rawValue < b.rawValue }
    }

    /// One line of the expanded log.
    struct Step: Equatable, Identifiable {
        let id: Int
        let text: String
        let kind: Kind
        let failed: Bool
    }

    private(set) var steps: [Step] = []
    private(set) var counts: [Kind: Int] = [:]
    private(set) var failed = 0

    /// Segments shown in the sentence before the rest are left out.
    static let maxSegments = 3

    init(activity: [String], toolCount: Int = 0, toolFailed: Int = 0) {
        for (index, raw) in activity.enumerated() {
            let line = raw.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !line.isEmpty else { continue }
            let failed = line.hasSuffix("· failed")
            let kind = Self.kind(of: line)
            steps.append(Step(id: index, text: failed ? String(line.dropLast("· failed".count)).trimmingCharacters(in: .whitespaces) : line, kind: kind, failed: failed))
            counts[kind, default: 0] += 1
            if failed { self.failed += 1 }
        }
        if toolCount > 0 { counts[.tool, default: 0] += toolCount }
        failed += toolFailed
    }

    var hasWork: Bool { !counts.isEmpty }

    /// "Ran 3 commands · edited 2 files · 1 failed", or nil when nothing ran.
    var sentence: String? {
        let segments = Kind.allCases.compactMap { kind -> String? in
            guard let count = counts[kind], count > 0 else { return nil }
            return Self.segment(kind, count)
        }
        guard !segments.isEmpty else { return nil }
        var shown = Array(segments.prefix(Self.maxSegments))
        shown[0] = shown[0].prefix(1).uppercased() + shown[0].dropFirst()
        if segments.count > Self.maxSegments { shown.append("+\(segments.count - Self.maxSegments) more") }
        if failed > 0 { shown.append("\(failed) failed") }
        return shown.joined(separator: " · ")
    }

    static func segment(_ kind: Kind, _ count: Int) -> String {
        let plural = count == 1 ? "" : "s"
        switch kind {
        case .command: return "ran \(count) command\(plural)"
        case .edit: return "made \(count) edit\(plural)"
        case .read: return "read \(count) file\(plural)"
        case .search: return count == 1 ? "searched once" : "searched \(count) times"
        case .tool: return "used \(count) tool\(plural)"
        case .other: return "\(count) other action\(plural)"
        }
    }

    static func kind(of line: String) -> Kind {
        let verb = line.prefix { $0.isLetter }.lowercased()
        switch verb {
        case "ran", "run", "running", "executed", "started": return .command
        case "edited", "wrote", "created", "updated", "changed", "patched", "deleted", "moved", "renamed", "saved", "applied": return .edit
        case "read", "opened", "viewed", "listed", "inspected", "loaded": return .read
        case "searched", "searching", "grepped", "looked", "found", "globbed": return .search
        default: return .other
        }
    }
}

/// How long a turn worked, in the words the chat uses.
enum ChatDuration {
    /// "12s", "1m 12s", "1h 5m".
    static func text(seconds: Int) -> String {
        let seconds = max(0, seconds)
        if seconds < 60 { return "\(seconds)s" }
        if seconds < 3600 { return "\(seconds / 60)m \(seconds % 60)s" }
        return "\(seconds / 3600)h \(seconds % 3600 / 60)m"
    }

    /// The moment a reply finished, when the daemon says so.
    static func completion(of message: JSONValue) -> Int? {
        for key in ["completed_ms", "done_ms", "completed_at_ms", "finished_at_ms"] {
            let value = message[key].int
            if value > 0 { return value }
        }
        return nil
    }

    /// "Worked for 1m 12s", or nil when the start or end is unknown or the
    /// turn was too quick to be worth a line.
    static func workedFor(startMs: Int, endMs: Int?) -> String? {
        guard startMs > 0, let endMs, endMs > startMs else { return nil }
        let seconds = (endMs - startMs) / 1000
        return seconds < minimumWorked ? nil : "Worked for \(text(seconds: seconds))"
    }
    static let minimumWorked = 2
}

/// The text of the live status line: the newest thing Neko did, or the
/// generic state when nothing has been reported yet.
enum ChatWorkLive {
    /// `fallback` is the generic label ("Thinking…", "Waiting for your OK").
    /// `keepFallback` holds it when it says something a step cannot: an
    /// approval waiting on the reader, or a tool running right now.
    static func label(steps: [String], fallback: String, keepFallback: Bool) -> String {
        if keepFallback { return fallback }
        guard let last = steps.last?.trimmingCharacters(in: .whitespacesAndNewlines), !last.isEmpty else { return fallback }
        return last.replacingOccurrences(of: "`", with: "")
    }
}

/// Which work logs the reader opened, by message id: kept outside the row so
/// a lazy stack recycling it does not close it. Their choice survives the
/// turn finishing.
@MainActor final class ChatWorkLedger {
    private var open: Set<String> = []
    func isOpen(_ id: String) -> Bool { open.contains(id) }
    @discardableResult func toggle(_ id: String) -> Bool {
        if open.remove(id) == nil { open.insert(id); return true }
        return false
    }
}

/// Whether a work log is expanded: collapsed until the reader opens it, and
/// then it stays as they left it, running or finished.
enum ChatWorkFoldPolicy {
    static func isOpen(userOpened: Bool, canExpand: Bool) -> Bool { canExpand && userOpened }
}

// MARK: - Streaming Markdown

/// A reply that is still being written, as finished blocks and a live tail.
/// Finished blocks never change, so they are parsed once and their views are
/// skipped on every later token; only the tail redraws.
enum StreamingMarkdown {
    /// Splits at the last blank line outside a code fence.
    static func split(_ text: String) -> (stable: String, tail: String) {
        let lines = text.components(separatedBy: "\n")
        var fence: Character?
        var boundary: Int?
        for (index, line) in lines.enumerated() {
            let trimmed = line.trimmingCharacters(in: .whitespaces)
            if let open = fence {
                if trimmed.hasPrefix(String(repeating: open, count: 3)) { fence = nil }
                continue
            }
            if trimmed.hasPrefix("```") { fence = "`"; continue }
            if trimmed.hasPrefix("~~~") { fence = "~"; continue }
            if trimmed.isEmpty, index > 0, index < lines.count - 1 { boundary = index }
        }
        guard let boundary else { return ("", text) }
        let stable = lines[..<boundary].joined(separator: "\n")
        let tail = lines[(boundary + 1)...].joined(separator: "\n")
        // A boundary after nothing but blank lines leaves nothing to keep.
        return stable.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? ("", tail) : (stable, tail)
    }

    /// Closes the bold or code span a token is still in the middle of, so a
    /// late closing marker never reflows the line it ends. Display only.
    static func mended(_ text: String) -> String {
        let fences = text.components(separatedBy: "\n").filter {
            let trimmed = $0.trimmingCharacters(in: .whitespaces)
            return trimmed.hasPrefix("```") || trimmed.hasPrefix("~~~")
        }.count
        guard fences % 2 == 0 else { return text }
        var result = text
        let ticks = result.replacingOccurrences(of: "```", with: "").filter { $0 == "`" }.count
        if ticks % 2 == 1 { result += "`" }
        if (result.components(separatedBy: "**").count - 1) % 2 == 1 { result += "**" }
        return result
    }

    /// The tail's new text is the old text plus more: only then can the old
    /// part stay solid while the rest fades in.
    static func settled(old: String, new: String) -> String {
        !old.isEmpty && new.hasPrefix(old) ? old : ""
    }
}

// MARK: - Sent message anchor

/// The room a message you just sent needs under it to sit at the top of the
/// chat while the reply is written below.
enum ChatRunway {
    /// Extra trailing space, beyond the dock clearance, that lets the newest
    /// exchange scroll to `topMargin` below the top edge. It shrinks as the
    /// reply grows and is zero once the reply fills the screen.
    static func extra(viewport: CGFloat, exchange: CGFloat, clearance: CGFloat, topMargin: CGFloat) -> CGFloat {
        guard viewport > 0, exchange > 0 else { return 0 }
        return max(0, viewport - topMargin - exchange - clearance)
    }
}
