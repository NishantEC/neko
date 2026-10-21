import SwiftUI

/// One place for everything that configures Neko, laid out like System
/// Settings: sections on the left, the section on the right. Daily work
/// (tasks) stays in the main sidebar; setup lives here.
enum SettingsSection: String, CaseIterable, Identifiable {
    case general, ai, rulebook, workspaces, watches, schedules, tools, memory, profiles, search, permissions, diagnostics, about
    var id: String { rawValue }
    var title: String {
        switch self {
        case .general: "General"
        case .ai: "AI & agents"
        case .rulebook: "Rulebook"
        case .workspaces: "Workspaces"
        case .watches: "Watches"
        case .schedules: "Schedules"
        case .tools: "Tools & skills"
        case .memory: "Memory"
        case .profiles: "Profiles"
        case .search: "Quick panel"
        case .permissions: "Permissions"
        case .diagnostics: "Diagnostics"
        case .about: "About"
        }
    }
    var symbol: String {
        switch self {
        case .general: "gearshape"
        case .ai: "sparkles"
        case .rulebook: "book.closed"
        case .workspaces: "square.stack.3d.up"
        case .watches: "eye"
        case .schedules: "clock"
        case .tools: "wrench.and.screwdriver"
        case .memory: "brain"
        case .profiles: "person.2"
        case .search: "magnifyingglass"
        case .permissions: "lock.shield"
        case .diagnostics: "stethoscope"
        case .about: "info.circle"
        }
    }
    /// Other words people use for the section, so search finds it.
    var keywords: [String] {
        switch self {
        case .general: ["preferences", "appearance", "shortcut", "hotkey", "login"]
        case .ai: ["model", "runtime", "codex", "claude", "budget", "agents"]
        case .rulebook: ["rules", "standards", "engineering", "review", "instructions", "policy"]
        case .workspaces: ["repository", "repositories", "folders", "projects", "git"]
        case .watches: ["watching", "monitor", "sentry", "linear", "issues", "responsibilities", "sources"]
        case .schedules: ["recurring", "brief", "cron", "daily", "weekly", "routine"]
        case .tools: ["mcp", "connections", "servers", "skills", "integrations"]
        case .memory: ["remember", "working style", "guidance", "context"]
        case .profiles: ["agent profiles", "people", "persona"]
        case .search: ["quick panel", "palette", "folders", "files", "spotlight"]
        case .permissions: ["privacy", "full disk access", "accessibility", "clipboard"]
        case .diagnostics: ["health", "debug", "logs", "troubleshoot", "connection"]
        case .about: ["version", "update", "build"]
        }
    }
    /// Groups shown as headings in the section list. Short, so they fit the column.
    static let groups: [(title: String, sections: [SettingsSection])] = [
        ("Neko", [.general, .ai, .rulebook]),
        ("Work", [.workspaces, .watches, .schedules]),
        ("Knowledge & tools", [.tools, .memory, .profiles]),
        ("This Mac", [.search, .permissions, .diagnostics, .about]),
    ]
    /// The section a historical page now lives in.
    init?(page: Page) {
        switch page {
        case .workspaces: self = .workspaces
        case .watching: self = .watches
        case .schedules: self = .schedules
        case .tools: self = .tools
        case .memory: self = .memory
        case .profiles: self = .profiles
        case .settings: self = .general
        default: return nil
        }
    }
    /// The section that renders a Preferences tab. The legacy Agents tab
    /// has no section of its own and opens beside AI & agents.
    init(tab: SettingsTab) {
        self = SettingsSection.allCases.first { $0.tab == tab } ?? .ai
    }
    /// The Preferences tab that renders this section, for the ones it owns.
    var tab: SettingsTab? {
        switch self {
        case .general: .general
        case .ai: .ai
        case .search: .search
        case .permissions: .permissions
        case .diagnostics: .diagnostics
        case .about: .about
        default: nil
        }
    }
}

/// A request to scroll to and highlight one setting. A fresh value each
/// time, so jumping to the same setting twice still moves.
struct SettingsJump: Equatable {
    let target: SettingsTarget
    let id = UUID()
    let at = Date()
    /// A page that appears just after the jump (the section changed) still
    /// honours it; one that reappears later does not.
    var isFresh: Bool { Date().timeIntervalSince(at) < 1 }
}

struct SettingsHub: View {
    @ObservedObject var model: AppModel
    @State var section: SettingsSection
    @AppStorage("neko.settings.tab") private var settingsTab: SettingsTab = .general
    @State private var query = ""
    @State private var searchFocus = 0
    @State private var jump: SettingsJump?
    var body: some View {
        HStack(spacing: 0) {
            VStack(spacing: 0) {
                NekoSearchField(title: "Find a setting", text: $query, size: .small, focusRequest: searchFocus)
                    .onSubmit { if let first = SettingsSearch.hubResults(for: query).first { open(first) } }
                    .padding(.horizontal, NekoSpace.md).padding(.vertical, NekoSpace.sm)
                List(selection: Binding(get: { query.isEmpty ? section : nil }, set: { if let value = $0 { query = ""; section = value } })) {
                    ForEach(SettingsSection.groups, id: \.title) { group in
                        Section(group.title) {
                            ForEach(group.sections) { item in
                                Label(item.title, systemImage: item.symbol).tag(item)
                            }
                        }
                    }
                }
                .listStyle(.sidebar)
            }
            .frame(width: NekoLayout.settingsSidebarWidth)
            Divider()
            Group {
                if !query.isEmpty {
                    ScrollView {
                        VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
                            NekoPageIntro(title: "Results", message: "Choose a section or setting to open it.")
                            SettingsHubSearchResults(query: query, open: open)
                        }
                        .frame(maxWidth: NekoLayout.pageWidth, alignment: .leading)
                        .padding(NekoLayout.pageInset)
                        .frame(maxWidth: .infinity, alignment: .top)
                    }
                } else {
                    switch section {
                    case .rulebook: RulebookView(model: model)
                    case .workspaces: WorkspacesView(model: model)
                    case .watches: ResponsibilitiesView(model: model)
                    case .schedules: SchedulesView(model: model)
                    case .tools: ToolsView(model: model)
                    case .memory: MemoryView(model: model)
                    case .profiles: ProfilesView(model: model)
                    default: PreferencesView(inHub: true, jump: jump, model: model)
                    }
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .navigationTitle("Settings")
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button("Find a setting", systemImage: "magnifyingglass") { searchFocus += 1 }
                    .keyboardShortcut("f", modifiers: .command).help("Find a setting (⌘F)")
            }
        }
        .onAppear { if let tab = section.tab { settingsTab = tab } }
        .onChange(of: section) { _, value in if let tab = value.tab { settingsTab = tab } }
        .onChange(of: settingsTab) { _, tab in
            // A Preferences page can send you to another tab ("Show" in Permissions); follow it here.
            let match = SettingsSection(tab: tab)
            if section.tab != nil, match.tab == tab, match != section { section = match }
        }
    }

    /// Opens a search hit: a section shows itself; a setting switches to its
    /// section and tab, then scrolls to and highlights the control.
    private func open(_ hit: SettingsHit) {
        query = ""
        switch hit {
        case .section(let value):
            section = value
        case .setting(let target):
            section = SettingsSection(tab: target.tab)
            // After the section change has set its own tab.
            DispatchQueue.main.async {
                settingsTab = target.tab
                jump = SettingsJump(target: target)
            }
        }
    }
}
