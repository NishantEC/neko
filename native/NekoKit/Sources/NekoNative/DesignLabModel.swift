import Observation
import SwiftUI

/// Deliberately has no AppModel, IPC or provider dependency. This page is a local design study.
@MainActor @Observable final class DesignLabModel {
    enum Layout: String, CaseIterable, Identifiable {
        case inline = "Inline", context = "Context"
        var id: Self { self }
    }

    enum Agent: String, CaseIterable, Identifiable {
        case colours = "Colours", tokens = "Tokens", settings = "Settings"
        var id: Self { self }
        var color: Color {
            switch self {
            case .colours: Color(red: 0.73, green: 0.40, blue: 0.65)
            case .tokens: Color(red: 0.78, green: 0.57, blue: 0.27)
            case .settings: Color(red: 0.29, green: 0.65, blue: 0.47)
            }
        }
        var status: String { self == .colours ? "Auditing" : "Building" }
        var activity: String {
            switch self {
            case .colours: "Checking text and control contrast in both appearances."
            case .tokens: "Replacing fixed colours with shared appearance tokens."
            case .settings: "Adding the appearance control to Settings."
            }
        }
    }

    enum Plugin: String, CaseIterable, Identifiable {
        case repository = "Repository tools", swift = "SwiftUI craft"
        var id: Self { self }
        var symbol: String { self == .repository ? "arrow.triangle.branch" : "swift" }
        var detail: String {
            self == .repository ? "Issues, pull requests and repository context" : "Native layouts and accessibility patterns"
        }
    }

    var layout = Layout.inline
    var draft = "Keep the dark background neutral. No blue wash."
    var plugins: Set<Plugin> = Set(Plugin.allCases)
    var messages: [String] = []
    var attachmentName: String? = "appearance-reference.png"
    var error: String?
    var selectedAgent: Agent?
    var showingLibrary = false

    var canSend: Bool { !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty }

    func send() {
        guard canSend else { return }
        messages.append(draft)
        draft = ""
        attachmentName = nil
    }

    func toggle(_ plugin: Plugin) {
        if !plugins.insert(plugin).inserted { plugins.remove(plugin) }
    }

    func reset() {
        draft = "Keep the dark background neutral. No blue wash."
        plugins = Set(Plugin.allCases)
        messages = []
        attachmentName = "appearance-reference.png"
        error = nil
        selectedAgent = nil
        showingLibrary = false
    }
}
