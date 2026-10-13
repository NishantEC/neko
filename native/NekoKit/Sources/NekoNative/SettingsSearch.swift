import Foundation

/// Settings' tabs. The tab owns its name and one-line description.
enum SettingsTab: String, CaseIterable, Identifiable, Sendable {
    case general = "General", ai = "AI", search = "Search", permissions = "Permissions"
    case diagnostics = "Diagnostics", agents = "Agents", about = "About"
    var id: String { rawValue }
    var title: String { rawValue }
    var summary: String {
        switch self {
        case .general: "Make Neko fit the way you use your Mac."
        case .ai: "Choose the runtime and model for new work."
        case .search: "Choose which folders appear in the quick panel."
        case .permissions: "Review what Neko can access on this Mac."
        case .diagnostics: "Check local runtimes and the connection to Neko."
        case .agents: "Manage the optional legacy agent directory."
        case .about: "Your workspace, native on your Mac."
        }
    }
}

/// One findable setting. A control opts in with `.settingsTarget(_:)`, which
/// lets search scroll to it and highlight it.
enum SettingsTarget: String, CaseIterable, Identifiable, Sendable {
    case appearance, launchAtLogin, activityStatus, clipboardHistory
    case runtime, model, customModel, budget
    case includedFolders, addFolder
    case permissions, diagnostics
    case legacyAgents
    case updates

    var id: String { rawValue }

    var tab: SettingsTab {
        switch self {
        case .appearance, .launchAtLogin, .activityStatus, .clipboardHistory: .general
        case .runtime, .model, .customModel, .budget: .ai
        case .includedFolders, .addFolder: .search
        case .permissions: .permissions
        case .diagnostics: .diagnostics
        case .legacyAgents: .agents
        case .updates: .about
        }
    }

    var title: String {
        switch self {
        case .appearance: "Appearance"
        case .launchAtLogin: "Launch at login"
        case .activityStatus: "Show activity while Neko works"
        case .clipboardHistory: "Save clipboard history"
        case .runtime: "Agent runtime"
        case .model: "Model"
        case .customModel: "Exact model ID"
        case .budget: "Ticket budget"
        case .includedFolders: "Included folders"
        case .addFolder: "Add a folder"
        case .permissions: "Permissions"
        case .diagnostics: "Diagnostics"
        case .legacyAgents: "Legacy agent providers"
        case .updates: "Updates and version"
        }
    }

    /// Other words people use for the same thing.
    var keywords: [String] {
        switch self {
        case .appearance: ["dark", "light", "theme", "mode", "colour", "color"]
        case .launchAtLogin: ["startup", "start", "boot", "open at login"]
        case .activityStatus: ["presence", "corner", "status", "indicator"]
        case .clipboardHistory: ["copy", "paste", "pasteboard", "privacy"]
        case .runtime: ["codex", "claude", "opencode", "provider", "runner"]
        case .model: ["gpt", "sonnet", "opus", "llm", "check"]
        case .customModel: ["id", "custom", "manual"]
        case .budget: ["cost", "money", "limit", "spend", "dollars", "price"]
        case .includedFolders: ["files", "quick panel", "scope", "directories"]
        case .addFolder: ["folder", "directory", "path"]
        case .permissions: ["full disk access", "accessibility", "privacy", "access"]
        case .diagnostics: ["health", "connection", "debug", "logs", "troubleshoot"]
        case .legacyAgents: ["paseo", "idle", "agents"]
        case .updates: ["version", "update", "build", "about"]
        }
    }
}

/// Ranks settings for a query: title prefix, then a word in the title, then a
/// keyword, then the tab name. Pure, so it is tested without a window.
enum SettingsSearch {
    static func results(for query: String) -> [SettingsTarget] {
        let needle = query.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        guard !needle.isEmpty else { return [] }
        return SettingsTarget.allCases
            .compactMap { target in score(target, needle).map { (target, $0) } }
            .sorted { $0.1 != $1.1 ? $0.1 < $1.1 : $0.0.title < $1.0.title }
            .map(\.0)
    }

    /// Lower is better; nil means no match.
    static func score(_ target: SettingsTarget, _ needle: String) -> Int? {
        let title = target.title.lowercased()
        if title.hasPrefix(needle) { return 0 }
        if title.split(separator: " ").contains(where: { $0.hasPrefix(needle) }) { return 1 }
        if target.keywords.contains(where: { $0.hasPrefix(needle) }) { return 2 }
        if title.contains(needle) || target.keywords.contains(where: { $0.contains(needle) }) { return 3 }
        if target.tab.title.lowercased().hasPrefix(needle) { return 4 }
        return nil
    }
}
