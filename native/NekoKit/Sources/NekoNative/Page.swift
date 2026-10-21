import Foundation

/// Every destination in the workspace window. The page owns its title,
/// symbol, sidebar group and toolbar behaviour, so adding a page is one case
/// here rather than four string lists that have to agree.
///
/// Raw values are the historical keys, kept so `NEKO_START_PAGE` and saved
/// state keep working.
enum Page: String, CaseIterable, Identifiable, Sendable {
    case home = "Home"
    /// The same Home chat, served by the daemon as a web page (trial).
    case homeWeb = "Home (web)"
    case tickets = "Tickets"
    case pullRequests = "Pull requests"
    case workspaces = "Workspaces"
    case watching = "Responsibilities"
    case schedules = "Schedules"
    case tools = "Tools & skills"
    case memory = "Memory"
    case profiles = "Profiles"
    case designLab = "Design lab"
    case settings = "Settings"
    /// Development galleries, reachable with `NEKO_START_PAGE` only.
    case activity = "Activity"
    case replyViews = "Reply views"

    var id: String { rawValue }

    /// Accepts the historical keys, including "Today" for Home.
    init?(key: String) {
        if key == "Today" { self = .home } else { self.init(rawValue: key) }
    }

    /// What the sidebar and window title call it.
    var title: String {
        switch self {
        case .tickets: "All tasks"
        case .watching: "Watching"
        default: rawValue
        }
    }

    var symbol: String {
        switch self {
        case .home: "house"
        case .homeWeb: "globe"
        case .tickets: "checklist"
        case .pullRequests: "arrow.triangle.pull"
        case .workspaces: "square.stack.3d.up"
        case .watching: "eye"
        case .schedules: "clock"
        case .tools: "wrench.and.screwdriver"
        case .memory: "brain"
        case .profiles: "person.2"
        case .designLab: "circle.hexagongrid"
        case .settings: "gearshape"
        case .activity: "sparkles"
        case .replyViews: "text.bubble"
        }
    }

    /// Home and All agents place their own content in the toolbar row; every
    /// other page shares the permission banner above its content.
    var ownsToolbarRow: Bool { self == .home || self == .tickets }

    /// The page to open at launch: `NEKO_START_PAGE` when it names one, else Home.
    static func start(environment: [String: String] = ProcessInfo.processInfo.environment) -> Page {
        environment["NEKO_START_PAGE"].flatMap(Page.init(key:)) ?? .home
    }
}
