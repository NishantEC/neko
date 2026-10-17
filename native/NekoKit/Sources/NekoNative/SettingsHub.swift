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
        case .search: "Quick panel search"
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
    /// Groups shown as headings in the section list.
    static let groups: [(title: String, sections: [SettingsSection])] = [
        ("", [.general, .ai, .rulebook]),
        ("What Neko watches and works on", [.workspaces, .watches, .schedules]),
        ("What Neko knows and can use", [.tools, .memory, .profiles]),
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

struct SettingsHub: View {
    @ObservedObject var model: AppModel
    @State var section: SettingsSection
    @AppStorage("neko.settings.tab") private var settingsTab: SettingsTab = .general
    var body: some View {
        HStack(spacing: 0) {
            List(selection: Binding(get: { section }, set: { if let value = $0 { section = value } })) {
                ForEach(SettingsSection.groups, id: \.title) { group in
                    Section(group.title) {
                        ForEach(group.sections) { item in
                            Label(item.title, systemImage: item.symbol).tag(item)
                        }
                    }
                }
            }
            .listStyle(.sidebar)
            .frame(width: 220)
            Divider()
            Group {
                switch section {
                case .rulebook: RulebookView(model: model)
                case .workspaces: WorkspacesView(model: model)
                case .watches: ResponsibilitiesView(model: model)
                case .schedules: SchedulesView(model: model)
                case .tools: ToolsView(model: model)
                case .memory: MemoryView(model: model)
                case .profiles: ProfilesView(model: model)
                default: PreferencesView(inHub: true, model: model)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        }
        .navigationTitle("Settings")
        .onAppear { if let tab = section.tab { settingsTab = tab } }
        .onChange(of: section) { _, value in if let tab = value.tab { settingsTab = tab } }
        .onChange(of: settingsTab) { _, tab in
            // A search result in Preferences can switch tabs; follow it here.
            if let match = SettingsSection.allCases.first(where: { $0.tab == tab }), section.tab != nil, match != section { section = match }
        }
    }
}
