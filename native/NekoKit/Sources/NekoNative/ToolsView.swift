import SwiftUI
import AppKit
import NekoKit

enum SkillPresentation {
    static func enabledRecord(for skill: JSONValue, enabled: [JSONValue], workspace: String) -> JSONValue? {
        enabled.first { $0["workspace_id"].string == workspace && $0["path"] == skill["path"] }
    }
    static func unavailable(enabled: [JSONValue], available: [JSONValue], workspace: String) -> [JSONValue] {
        enabled.filter { record in
            record["workspace_id"].string == workspace && !available.contains {
                $0["path"] == record["path"] && ($0["workspace_id"].string.isEmpty || $0["workspace_id"].string == workspace)
            }
        }
    }
}

enum ToolsPresentation {
    static func connectionStatus(_ connection: JSONValue) -> String {
        if !connection["enabled"].bool { return "Paused" }
        let error = connection["error"].string.lowercased()
        if error.contains("auth") || error.contains("oauth") || error.contains("sign-in") || error.contains("401") { return "Sign in needed" }
        if !error.isEmpty { return "Needs attention" }
        if connection["tools"].array.isEmpty {
            return connection["discovered_ms"] == .null ? "Discovering" : "No tools"
        }
        return "Available"
    }
    static func toolSummary(_ connection: JSONValue) -> String {
        let count = connection["tools"].array.count
        if count > 0 { return "\(count) \(count == 1 ? "tool" : "tools")" }
        if !connection["enabled"].bool || !connection["error"].string.isEmpty { return "No tools" }
        return connection["discovered_ms"] == .null ? "Discovering" : "No tools"
    }
}

private struct ConnectionSelection: Identifiable {
    let id: String
}

@MainActor
struct ToolsView: View {
    @ObservedObject var model: AppModel
    @State private var tab = 0
    @State private var form = ConnectionForm()
    @State private var repositoryURL = ""
    @State private var search = ""
    @State private var validation: String?
    @State private var busy = false
    @State private var openedAudits: Set<String> = []
    @State private var expandedSkills: Set<String> = []
    @State private var selectedConnection: ConnectionSelection?
    @State private var showImportSheet = false
    @State private var showManualSheet = false
    @State private var showRegistrySheet = false
    @State private var toolSearch = ""
    @State private var showFindSkill = false
    @State private var expandedCandidates: Set<String> = []

    private var workspace: String { model.selectedWorkspace ?? "" }
    private var connections: [JSONValue] {
        model.snapshot["mcp"]["connections"].array.filter {
            workspace.isEmpty || $0["workspace_id"].string.isEmpty || $0["workspace_id"].string == workspace
        }
    }
    private var folders: [JSONValue] {
        let explicit = model.snapshot["workspace_folders"][workspace].array
        if !explicit.isEmpty { return explicit }
        return model.snapshot["workspaces"].array.filter { $0["id"].string == workspace }.map { $0["repository"] }
    }
    private var feedback: String? { validation ?? model.error }
    private var command: ToolsCommand {
        ToolsCommand { family, name, fields, onSuccess in send(family, name, fields, onSuccess: onSuccess) }
    }

    var body: some View {
        NekoPage(title: "Tools & skills", message: "Connections provide access. Skills provide instructions.") {
                if tab == 0 {
                    Menu("Add connection", systemImage: "plus") {
                        Button("Browse MCP servers", systemImage: "square.grid.2x2") { showRegistrySheet = true }
                        Button("Import from this Mac", systemImage: "square.and.arrow.down") { showImportSheet = true }
                        Button("Add connection manually", systemImage: "plus") { showManualSheet = true }
                    }
                    .menuStyle(.button).controlSize(.regular).disabled(busy || model.busy)
                }
            } content: {
            HStack {
                Picker("Tools section", selection: $tab) {
                    Text("Connections").tag(0)
                    Text("Skills").tag(1)
                }
                .pickerStyle(.segmented).labelsHidden().controlSize(.regular).frame(width: 240)
                Spacer()
                if busy { ProgressView().controlSize(.small).accessibilityLabel("Updating tools and skills") }
            }
            ToolsOperationFeedback(message: feedback)
            if tab == 0 {
                ToolsConnectionsList(model: model, workspace: workspace, connections: connections) { connection in
                    toolSearch = ""
                    selectedConnection = ConnectionSelection(id: connection.recordID)
                }
            } else if workspace.isEmpty {
                NekoEmptyState("Choose a workspace in the sidebar to see its skills.")
            } else {
                ToolsSkillsList(model: model, workspace: workspace, search: $search, repositoryURL: $repositoryURL,
                                showFindSkill: $showFindSkill, expandedSkills: $expandedSkills, openedAudits: $openedAudits,
                                send: command, report: { validation = $0 })
                    .disabled(busy || model.busy)
            }
        }
        .font(NekoFont.body)
        .controlSize(.regular)
        .sheet(item: $selectedConnection) { selection in
            if let connection = connections.first(where: { $0.recordID == selection.id }) {
                ToolsConnectionDetail(model: model, connection: connection, toolSearch: $toolSearch, clientID: $form.clientID,
                                      busy: busy, feedback: feedback, send: command, close: { selectedConnection = nil })
            } else {
                NekoSheet(title: "Connection unavailable", message: "This connection may have been removed or its workspace changed.",
                          width: .narrow, cancelTitle: "Done", onCancel: { selectedConnection = nil }) { EmptyView() }
            }
        }
        .sheet(isPresented: $showImportSheet) {
            ToolsImportSheet(model: model, workspace: workspace, folders: folders, connections: connections, busy: busy, feedback: feedback,
                             expandedCandidates: $expandedCandidates, send: command, close: { showImportSheet = false })
        }
        .sheet(isPresented: $showManualSheet) {
            ToolsManualConnectionSheet(model: model, form: $form, workspace: workspace, busy: busy, feedback: feedback, add: addServer,
                                       browse: { showManualSheet = false; showRegistrySheet = true }, close: { showManualSheet = false })
        }
        .sheet(isPresented: $showRegistrySheet) { MCPRegistryBrowser(model: model) }
    }

    private func send(_ family: String, _ name: String, _ fields: [String: JSONValue]? = nil, onSuccess: (() -> Void)? = nil) {
        guard !busy else { return }
        busy = true
        validation = nil
        let payload: JSONValue = fields.map { .object([name: .object($0)]) } ?? .string(name)
        Task {
            let succeeded = await model.workbench(.object([family: payload]))
            if succeeded { onSuccess?() }
            busy = false
        }
    }

    private func addServer() {
        switch form.configuration() {
        case .failure(let invalid):
            validation = invalid.message
        case .success(let config):
            let secret = form.credentials
            send("Mcp", "AddConnection", ["workspace_id": .string(form.scope(in: workspace)), "label": .string(form.label), "config": config, "trust_local_process": .bool(form.local && form.trust), "credentials": secret.isEmpty ? .null : .string(secret)], onSuccess: {
                if form.credentials == secret { form.credentials = "" }
            })
        }
    }
}
