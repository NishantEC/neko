import SwiftUI

private struct SettingsHighlightKey: EnvironmentKey { static let defaultValue: SettingsTarget? = nil }
extension EnvironmentValues {
    /// The setting a search just jumped to, highlighted briefly.
    var settingsHighlight: SettingsTarget? {
        get { self[SettingsHighlightKey.self] }
        set { self[SettingsHighlightKey.self] = newValue }
    }
}

/// Makes a control findable by Settings search: search can scroll to it,
/// and it carries a short accent wash when it is the one jumped to.
private struct SettingsTargetModifier: ViewModifier {
    let target: SettingsTarget
    @Environment(\.settingsHighlight) private var highlighted
    func body(content: Content) -> some View {
        content
            .id(target)
            .background {
                RoundedRectangle(cornerRadius: NekoRadius.sm, style: .continuous)
                    .fill(NekoStyle.accent.opacity(highlighted == target ? 0.14 : 0))
                    .padding(-NekoSpace.xxs)
            }
            .nekoAnimation(.reveal, value: highlighted == target)
    }
}

extension View {
    func settingsTarget(_ target: SettingsTarget) -> some View { modifier(SettingsTargetModifier(target: target)) }
}

/// The Settings hub's results: sections and the settings inside them.
/// Return in the search field opens the first.
struct SettingsHubSearchResults: View {
    let query: String
    let open: (SettingsHit) -> Void
    var body: some View {
        let results = SettingsSearch.hubResults(for: query)
        if results.isEmpty {
            NekoEmptyState(reason: .noMatches(query), block: true)
        } else {
            VStack(alignment: .leading, spacing: 0) {
                ForEach(results) { hit in
                    Button { open(hit) } label: {
                        NekoListRow(title: title(hit), detail: nil, meta: nil) {
                            Image(systemName: symbol(hit)).font(NekoFont.body).accessibilityHidden(true)
                        } trailing: {
                            Text(place(hit)).font(NekoFont.meta).foregroundStyle(N.text3)
                            Image(systemName: "chevron.right").font(NekoFont.of(.small, weight: .semibold)).foregroundStyle(N.text4).accessibilityHidden(true)
                        }
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(title(hit)), \(place(hit))")
                }
            }
        }
    }
    private func title(_ hit: SettingsHit) -> String {
        switch hit {
        case .section(let section): section.title
        case .setting(let target): target.title
        }
    }
    private func symbol(_ hit: SettingsHit) -> String {
        switch hit {
        case .section(let section): section.symbol
        case .setting(let target): SettingsSection(tab: target.tab).symbol
        }
    }
    /// Where it lives: a section's group, or a setting's section.
    private func place(_ hit: SettingsHit) -> String {
        switch hit {
        case .section(let section): SettingsSection.groups.first { $0.sections.contains(section) }?.title ?? "Settings"
        case .setting(let target): target.tab == .agents ? "Legacy agents" : SettingsSection(tab: target.tab).title
        }
    }
}

/// Search results in place of the tab's content while a query is typed.
struct SettingsSearchResults: View {
    let query: String
    let open: (SettingsTarget) -> Void
    var body: some View {
        let results = SettingsSearch.results(for: query)
        if results.isEmpty {
            NekoEmptyState(reason: .noMatches(query), block: true)
        } else {
            VStack(alignment: .leading, spacing: 0) {
                NekoDividedList(results, id: \.self) { target in
                    Button { open(target) } label: {
                        HStack(spacing: NekoSpace.md) {
                            Text(target.title).font(NekoFont.body).foregroundStyle(N.text)
                            Spacer(minLength: NekoSpace.sm)
                            Text(target.tab.title).font(NekoFont.meta).foregroundStyle(N.text3)
                            Image(systemName: "chevron.right").font(NekoFont.of(.small, weight: .semibold)).foregroundStyle(N.text4).accessibilityHidden(true)
                        }.contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("\(target.title), in \(target.tab.title)")
                }
            }
        }
    }
}
