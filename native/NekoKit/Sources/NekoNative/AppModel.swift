import SwiftUI
import NekoKit

@MainActor final class AppModel: ObservableObject {
    @Published var snapshot: JSONValue = .object([:])
    /// The Mac's health, polled on its own: it changes every reading, and
    /// inside the snapshot it made every poll re-send and redraw everything.
    @Published var machine: JSONValue = .null
    /// `Snapshot::digest` of what `snapshot` holds; sent back so the daemon
    /// can answer "unchanged" instead of the whole workbench.
    private var digest = ""
    @Published var selectedWorkspace: String?
    @Published var requestedPage: Page?
    @Published var agentID: String?
    @Published var agentReturnPage: Page = .home
    @Published var agentFilter = "all"
    @Published var agentSearch = ""
    @Published var includeStoppedAgents = true
    // Drafts notify only views that read them, not every observer of AppModel.
    let ticketDrafts = ComposerDraftStore<String>()
    @Published var sendingTicketIDs: Set<String> = []

    func openAgent(_ id: String, from page: Page? = nil) {
        if agentID == nil { agentReturnPage = page ?? .home }
        agentID = id
    }
    func closeAgent() { agentID = nil; requestedPage = agentReturnPage }
    func showAgents(_ filter: String = "all") {
        agentFilter = filter
        agentID = nil
        requestedPage = .tickets
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
    let chatDrafts = ScopedChatDrafts()
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
            var tick = 0
            while !Task.isCancelled {
                try? await Task.sleep(for: .seconds(2))
                guard !Task.isCancelled else { return }
                await self?.refresh()
                tick += 1
                if tick % 7 == 1 { await self?.refreshMachine() }
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
            // With nothing held yet (or a daemon that sends no digest) ask for
            // the whole workbench; otherwise let the daemon say "unchanged".
            let ask: JSONValue = digest.isEmpty
                ? .object(["Workbench": .string("Snapshot")])
                : .object(["WorkbenchIfChanged": .object(["digest": .string(digest)])])
            let result = try await request(ask)
            guard started == generation, sequence == refreshSequence else { return }
            if case .object = result["WorkbenchUnchanged"] {
                // Nothing new: no copy was sent, nothing is decoded or redrawn.
            } else {
                guard case .object = result["Workbench"] else { throw invalidResponse() }
                acceptSnapshot(result["Workbench"])
            }
            if !connected { connected = true }
            if connectionError != nil { connectionError = nil }
            // A connection failure from before the daemon was up is no longer true.
            if let error, AppModel.isConnectionError(error) { self.error = nil }
        } catch {
            guard started == generation, sequence == refreshSequence else { return }
            if connected { connected = false }
            if connectionError != error.localizedDescription { connectionError = error.localizedDescription }
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
            acceptSnapshot(result["Workbench"])
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

    func refreshMachine() async {
        guard let result = try? await request(.string("MachineHealth")) else { return }
        let health = result["MachineHealth"]
        if health != machine { machine = health }
    }

    private func acceptSnapshot(_ value: JSONValue) {
        // The daemon's digest says whether anything a view shows changed
        // (heartbeat and machine health excluded). Comparing the
        // multi-megabyte trees field by field instead ran on the main
        // thread on every poll, a stall every two seconds.
        let incoming = value["digest"].string
        if incoming.isEmpty {
            // No digest (an older daemon): compare, ignoring the heartbeat.
            var previous = snapshot.object
            var next = value.object
            previous.removeValue(forKey: "heartbeat_ms")
            next.removeValue(forKey: "heartbeat_ms")
            digest = ""
            if previous != next { snapshot = value }
        } else if incoming != digest {
            digest = incoming
            snapshot = value
        }
        if machine == .null, value["machine"] != .null { machine = value["machine"] }
    }
}

extension JSONValue {
    var recordID: String { self["id"].string }
}

