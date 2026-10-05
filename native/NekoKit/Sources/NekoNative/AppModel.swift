import SwiftUI
import NekoKit

@MainActor final class AppModel: ObservableObject {
    @Published var snapshot: JSONValue = .object([:])
    @Published var selectedWorkspace: String?
    @Published var requestedPage: String?
    @Published var agentID: String?
    @Published var agentReturnPage = "Home"
    @Published var agentFilter = "all"
    @Published var agentSearch = ""
    @Published var includeStoppedAgents = true
    @Published var ticketDrafts = ComposerDraftStore<String>()
    @Published var sendingTicketIDs: Set<String> = []

    func openAgent(_ id: String, from page: String? = nil) {
        if agentID == nil { agentReturnPage = page ?? "Home" }
        agentID = id
    }
    func closeAgent() { agentID = nil; requestedPage = agentReturnPage }
    func showAgents(_ filter: String = "all") {
        agentFilter = filter
        agentID = nil
        requestedPage = "Tickets"
    }
    /// A short confirmation shown for a few seconds, e.g. after Stop all work.
    @Published var notice: String? {
        didSet {
            guard let notice else { return }
            Task { @MainActor [weak self] in
                try? await Task.sleep(for: .seconds(5))
                if self?.notice == notice { self?.notice = nil }
            }
        }
    }
    @Published private var actionError: String?
    @Published private var connectionError: String?
    /// Explicit action failures survive background reconnects. Dismissing the
    /// banner clears both categories; a successful health refresh clears only
    /// the connection error it has actually resolved.
    var error: String? {
        get { actionError ?? connectionError }
        set {
            actionError = newValue
            if newValue == nil { connectionError = nil }
        }
    }
    @Published var chatDrafts = ScopedChatDrafts()
    @Published var sendingChatScopes: Set<ChatDraftScope> = []
    @Published var connected = false
    @Published var busy = false
    @Published var onboarding = false
    @Published var loadingSetup = true
    let client = DaemonClient()
    private let transport: (@Sendable (JSONValue) async throws -> JSONValue)?
    private var polling: Task<Void, Never>?
    private var generation = 0
    private var refreshSequence = 0

    init(transport: (@Sendable (JSONValue) async throws -> JSONValue)? = nil) { self.transport = transport }

    var workspaces: [JSONValue] { snapshot["workspaces"].array }
    var tasks: [JSONValue] { snapshot["tasks"].array.filter { selectedWorkspace == nil || $0["workspace_id"].string == selectedWorkspace } }
    func request(_ request: JSONValue) async throws -> JSONValue {
        let result: JSONValue
        if let transport { result = try await transport(request) }
        else { result = try await client.request(request) }
        if case .object(let values) = result, let failure = values["Error"] {
            throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: failure["message"].string])
        }
        return result
    }
    func loadSetup() async {
        do {
            let state = try await request(.string("GetOnboardingState"))
            guard case .bool(let completed) = state["OnboardingState"]["completed"] else { throw invalidResponse() }
            onboarding = !completed
            loadingSetup = false
        }
        catch { self.error = error.localizedDescription }
    }
    func start() async {
        await refresh()
        await loadSetup()
        polling?.cancel()
        polling = Task { [weak self] in
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                guard !Task.isCancelled else { return }
                await self?.refresh()
                // Started before the daemon was up: finish setup once it answers.
                if let self, self.loadingSetup, self.connected { await self.loadSetup() }
            }
        }
    }
    func refresh() async {
        guard !busy else { return }
        let started = generation
        refreshSequence += 1
        let sequence = refreshSequence
        do {
            let result = try await request(.object(["Workbench": .string("Snapshot")]))
            guard started == generation, sequence == refreshSequence else { return }
            guard case .object = result["Workbench"] else { throw invalidResponse() }
            snapshot = result["Workbench"]
            connected = true
            connectionError = nil
            // A connection failure from before the daemon was up is no longer true.
            if let error, AppModel.isConnectionError(error) { self.error = nil }
        } catch {
            guard started == generation, sequence == refreshSequence else { return }
            connected = false; connectionError = error.localizedDescription
        }
    }
    @discardableResult func workbench(_ command: JSONValue) async -> Bool {
        guard !busy else { error = "Another change is still saving. Please try again."; return false }
        generation += 1
        busy = true
        defer { busy = false }
        do {
            let result = try await request(.object(["Workbench": command]))
            guard case .object = result["Workbench"] else { throw invalidResponse() }
            snapshot = result["Workbench"]
            connected = true
            error = nil
            return true
        } catch { self.error = error.localizedDescription; return false }
    }
    /// The home folder as the user's catch-all default workspace, named after the Mac user.
    static func isConnectionError(_ message: String) -> Bool {
        message.hasPrefix("Daemon connection failed") || message == DaemonClientError.disconnected.errorDescription || message == DaemonClientError.timedOut.errorDescription
    }
    /// The home folder as the user's catch-all default workspace, named after the Mac user.
    var homeWorkspaceID: String? {
        let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        return workspaces.first { ws in
            ws["repository"].string == home || snapshot["workspace_folders"][ws.recordID].array.contains { $0.string == home }
        }?.recordID
    }
    @discardableResult func addHomeWorkspace() async -> Bool {
        if let existing = homeWorkspaceID { selectedWorkspace = existing; return true }
        let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
        let user = NSFullUserName().split(separator: " ").first.map(String.init) ?? NSUserName()
        let workspace: JSONValue = .object(["id": .string(""), "name": .string(user), "repository": .string(home),
            "instructions": .string("Default workspace for everything under the home folder."), "away_enabled": .bool(false)])
        let saved = await workbench(.command("SaveWorkspaceWithFolders", ["workspace": workspace, "folders": .array([.string(home)])]))
        if saved { selectedWorkspace = homeWorkspaceID }
        return saved
    }
    func completeSetup() async {
        do {
            let reply = try await request(.command("SetOnboardingComplete", ["completed": .bool(true)]))
            guard reply["OnboardingState"]["completed"] == .bool(true) else { throw invalidResponse() }
            onboarding = false
            error = nil
        }
        catch { self.error = error.localizedDescription }
    }
    private func invalidResponse() -> NSError {
        NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "The daemon returned an unexpected response. Please try again."])
    }
}

extension JSONValue {
    var recordID: String { self["id"].string }
}

extension View {
    @ViewBuilder func nekoGlass() -> some View {
        if #available(macOS 26, *) { self.glassEffect(.regular, in: .rect(cornerRadius: 18)) }
        else { self.background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18)) }
    }
}
