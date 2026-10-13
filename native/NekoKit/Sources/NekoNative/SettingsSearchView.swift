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
