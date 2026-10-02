import Foundation

/// Composer shortcuts handled by the app, never sent to a model as-is.
enum SlashCommand: Equatable {
    case stop, clearFinished, memory, remember(String), forget(String), recall, models, setup, permissions, tickets, help

    struct Entry: Identifiable { let name: String; let detail: String; var id: String { name } }
    static let all: [Entry] = [
        Entry(name: "/stop", detail: "Stop all work (⌘⇧Esc)"),
        Entry(name: "/remember", detail: "Remember a fact, e.g. /remember tests use pnpm"),
        Entry(name: "/forget", detail: "Forget a memory, e.g. /forget pnpm"),
        Entry(name: "/recall", detail: "Show what Neko remembers"),
        Entry(name: "/memory", detail: "Open the Memory page"),
        Entry(name: "/tickets", detail: "Open your tickets"),
        Entry(name: "/clear", detail: "Clear completed and cancelled tickets"),
        Entry(name: "/models", detail: "Choose and check the agent model"),
        Entry(name: "/permissions", detail: "What Neko is allowed to do on this Mac"),
        Entry(name: "/setup", detail: "Show the first-run tour again"),
        Entry(name: "/help", detail: "List these commands"),
    ]

    /// Commands matching a partial "/re…" draft; empty once a space is typed.
    static func suggestions(for draft: String) -> [Entry] {
        let text = draft.trimmingCharacters(in: .whitespaces).lowercased()
        guard text.hasPrefix("/"), !text.contains(" ") else { return [] }
        return all.filter { $0.name.hasPrefix(text) }
    }

    /// nil for ordinary messages, including ones that merely contain a slash.
    static func parse(_ draft: String) -> SlashCommand? {
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard text.hasPrefix("/") else { return nil }
        let name = String(text.prefix { !$0.isWhitespace }).lowercased()
        let argument = text.dropFirst(name.count).trimmingCharacters(in: .whitespacesAndNewlines)
        switch name {
        case "/stop": return .stop
        case "/clear": return .clearFinished
        case "/memory": return .memory
        case "/remember": return argument.isEmpty ? nil : .remember(argument)
        case "/forget": return argument.isEmpty ? nil : .forget(argument)
        case "/recall": return .recall
        case "/models", "/model": return .models
        case "/setup": return .setup
        case "/permissions": return .permissions
        case "/tickets", "/steps": return .tickets
        case "/help", "/": return .help
        default: return nil
        }
    }

    /// Commands that become a normal chat message so they appear in the
    /// conversation; the daemon answers these in code, without a model.
    var chatText: String? {
        switch self {
        case .remember(let fact): "Remember that \(fact)"
        case .forget(let query): "Forget \(query)"
        case .recall: "What do you remember?"
        default: nil
        }
    }
}

