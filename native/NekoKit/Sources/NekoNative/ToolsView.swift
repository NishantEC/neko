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
    @State private var label = ""
    @State private var local = false
    @State private var global = false
    @State private var target = ""
    @State private var arguments = "[]"
    @State private var workingDirectory = ""
    @State private var credentials = ""
    @State private var trust = false
    @State private var clientID = ""
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
            operationFeedback
            if tab == 0 { connectionsBody }
            else if workspace.isEmpty {
                NekoEmptyState("Choose a workspace in the sidebar to see its skills.")
            } else { skillsBody.disabled(busy || model.busy) }
        }
        .font(NekoFont.body)
        .controlSize(.regular)
        .sheet(item: $selectedConnection) { selection in
            if let connection = connections.first(where: { $0.recordID == selection.id }) {
                connectionDetail(connection)
            } else {
                NekoSheet(title: "Connection unavailable", message: "This connection may have been removed or its workspace changed.",
                          width: .narrow, cancelTitle: "Done", onCancel: { selectedConnection = nil }) { EmptyView() }
            }
        }
        .sheet(isPresented: $showImportSheet) { importSheet }
        .sheet(isPresented: $showManualSheet) { manualSheet }
        .sheet(isPresented: $showRegistrySheet) { MCPRegistryBrowser(model: model) }
    }

    @ViewBuilder private var operationFeedback: some View {
        if let error = validation ?? model.error {
            DisclosureGroup {
                Text(error).font(NekoFont.meta).textSelection(.enabled).padding(.top, NekoSpace.xxs)
            } label: {
                Label(error, systemImage: "exclamationmark.triangle").font(NekoFont.body).lineLimit(2)
            }.foregroundStyle(NekoStyle.amber)
        }
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

    private var connectionsBody: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Text("Connections").font(NekoFont.heading).foregroundStyle(N.text)
                Text("\(connections.count)").monospacedDigit()
                Spacer()
                Text(workspace.isEmpty ? "All workspaces" : (model.workspaces.first { $0.recordID == workspace }?["name"].string ?? "Selected workspace"))
                    .lineLimit(1)
            }.font(NekoFont.meta).foregroundStyle(N.text3)
            if connections.isEmpty {
                NekoEmptyState("Browse MCP servers, import from this Mac, or add a URL manually.",
                               title: model.connected ? "No connections yet" : "Waiting for connections…", symbol: "link", block: true)
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(connections.enumerated()), id: \.element.recordID) { index, connection in
                        connectionRow(connection)
                        if index < connections.count - 1 { Divider().padding(.leading, 42) }
                    }
                }
            }
        }
    }

    private var importSheet: some View {
        let items = unlinkedCandidates
        return NekoSheet(title: "Import from this Mac",
                         message: "Review connections already configured in other apps. Nothing is linked until you choose it.",
                         width: .wide, height: 620, cancelTitle: "Done", onCancel: { showImportSheet = false }) {
            VStack(alignment: .leading, spacing: 0) {
                HStack {
                    Menu {
                        ForEach(model.workspaces, id: \.recordID) { item in
                            Button(item["name"].string) { model.selectedWorkspace = item.recordID }
                        }
                    } label: {
                        Label(model.workspaces.first { $0.recordID == workspace }?["name"].string ?? "Choose workspace", systemImage: "folder")
                    }
                    .accessibilityLabel("Workspace for imported connection")
                    Spacer()
                    Button("Scan again", systemImage: "arrow.clockwise") {
                        send("SetupImport", "Discover", ["repositories": .array(folders), "source_id": .null])
                    }.disabled(busy || model.busy)
                }
                .controlSize(.regular)
                .padding(.bottom, 18)
                operationFeedback
                if busy { ProgressView("Updating connections…").controlSize(.small).padding(.bottom, 10) }
                if workspace.isEmpty {
                    Text("Choose a workspace above to link a connection.")
                        .font(NekoFont.body).foregroundStyle(N.text3)
                        .padding(.bottom, NekoSpace.md)
                }
                ScrollView {
                    if items.isEmpty {
                        NekoEmptyState("Scan again after adding a tool in Codex, Claude, or a project folder.",
                                       title: "No new connections found", symbol: "magnifyingglass", block: true)
                            .frame(maxWidth: .infinity, minHeight: 160)
                    } else {
                        VStack(spacing: 0) {
                            ForEach(Array(items.enumerated()), id: \.element.recordID) { index, candidate in
                                candidateRow(candidate)
                                if index < items.count - 1 { Divider() }
                            }
                        }
                    }
                }
            }
        }
    }

    private var manualSheet: some View {
        NekoSheet(title: "Add connection", message: "Connect a hosted server or a trusted executable on this Mac.",
                  width: .wide, height: 660, cancelTitle: "Done", onCancel: { showManualSheet = false }) {
            operationFeedback
            if busy { ProgressView("Adding connection…").controlSize(.small) }
            Form {
                addConnection
                Section("Advanced sign-in") {
                    LabeledContent("OAuth client ID (advanced)") { TextField("Only if the tool asks for one", text: $clientID).labelsHidden() }
                }
            }
            .formStyle(.grouped).scrollContentBackground(.hidden)
            .disabled(busy || model.busy)
            Text("Connecting makes discovered tools available here. Neko asks before chat actions that may change data.")
                .font(NekoFont.meta).foregroundStyle(.secondary)
        } actions: {
            Button("Browse hosted MCP servers") { showManualSheet = false; showRegistrySheet = true }
            Button("Add connection", action: addServer).nekoButton(.primary)
                .disabled(busy || model.busy || label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || target.isEmpty || (local && !trust))
        }
    }

    private var candidates: [JSONValue] {
        model.snapshot["import_preview"]["candidates"].array.filter { candidate in
            guard candidate["kind"].string == "connection" else { return false }
            let path = candidate["workspace"].string
            return path.isEmpty || folders.contains { folder in
                URL(fileURLWithPath: folder.string).resolvingSymlinksInPath() == URL(fileURLWithPath: path).resolvingSymlinksInPath()
            }
        }
    }

    private var unlinkedCandidates: [JSONValue] {
        candidates.filter { candidate in
            candidate["metadata"]["already_in_neko"].string != "true" &&
            !connections.contains { $0["source_link"]["candidate_id"].string == candidate["id"].string }
        }
    }

    private func candidateRow(_ candidate: JSONValue) -> some View {
        let localProcess = candidate["metadata"]["transport"].string != "http"
        let problem = candidate["problem"].string
        let key = candidate.recordID
        let expanded = expandedCandidates.contains(key)
        let source = candidate["source"].string
        return VStack(alignment: .leading, spacing: 0) {
            Button {
                if expanded { expandedCandidates.remove(key) }
                else { expandedCandidates.insert(key) }
            } label: {
                HStack(spacing: NekoSpace.md) {
                    Image(systemName: localProcess ? "terminal" : "link")
                        .font(NekoFont.of(.body, weight: .regular))
                        .foregroundStyle(N.text3)
                        .frame(width: 24)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(candidate["name"].string)
                            .font(NekoFont.heading).foregroundStyle(N.text)
                        Text(source.isEmpty ? "Local configuration" : source)
                            .font(NekoFont.meta).foregroundStyle(N.text4)
                    }
                    Spacer(minLength: 10)
                    if !problem.isEmpty {
                        Text("Needs setup").font(NekoFont.meta).foregroundStyle(NekoStyle.amber)
                    } else if candidate["metadata"]["enabled_at_source"].string == "false" {
                        Text("Paused at source").font(NekoFont.meta).foregroundStyle(N.text4)
                    } else {
                        Text(localProcess ? "Local" : "Remote").font(NekoFont.meta).foregroundStyle(N.text4)
                    }
                    Image(systemName: expanded ? "chevron.up" : "chevron.down")
                        .font(NekoFont.of(.micro, weight: .semibold)).foregroundStyle(N.text4)
                }
                .padding(.vertical, NekoLayout.rowInset)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if expanded {
                VStack(alignment: .leading, spacing: 10) {
                    if !problem.isEmpty {
                        Label(problem, systemImage: "exclamationmark.triangle")
                            .font(NekoFont.body).foregroundStyle(NekoStyle.amber)
                    }
                    Text(candidate["metadata"]["config_summary"].string)
                        .font(NekoFont.mono)
                        .foregroundStyle(N.text3)
                        .textSelection(.enabled)
                    if localProcess {
                        Text("This executable runs on your Mac outside the agent sandbox. Review its command before trusting it.")
                            .font(NekoFont.meta).foregroundStyle(N.text4)
                    }
                    Button(localProcess ? "Trust and connect" : "Connect") {
                        send("Mcp", "LinkSource", ["workspace_id": .string(workspace), "candidate_id": candidate["id"], "trust_local_process": .bool(localProcess)])
                    }
                    .disabled(busy || model.busy || workspace.isEmpty || !problem.isEmpty || candidate["metadata"]["enabled_at_source"].string == "false")
                    .controlSize(.regular)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, 36).padding(.bottom, NekoLayout.rowInset)
            }
        }
    }

    private func connectionRow(_ connection: JSONValue) -> some View {
        let summary = ToolsPresentation.toolSummary(connection)
        let scope = connection["workspace_id"].string.isEmpty ? "All workspaces" : (model.workspaces.first { $0.recordID == connection["workspace_id"].string }?["name"].string ?? "Workspace")
        let status = ToolsPresentation.connectionStatus(connection)
        return Button {
            toolSearch = ""
            selectedConnection = ConnectionSelection(id: connection.recordID)
        } label: {
            HStack(spacing: NekoSpace.lg) {
                Image(systemName: connection["config"]["transport"].string == "http" ? "link" : "terminal")
                    .font(NekoFont.body).foregroundStyle(N.text3).frame(width: 26).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.xs) {
                    Text(connection["label"].string).font(NekoFont.heading).foregroundStyle(N.text).lineLimit(2)
                    Text(scope).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2)
                }.frame(maxWidth: .infinity, alignment: .leading)
                VStack(alignment: .trailing, spacing: 3) {
                    Text(status).foregroundStyle(status == "Needs attention" || status == "Sign in needed" ? NekoStyle.amber : N.text3)
                    if summary != status { Text(summary).foregroundStyle(N.text3).monospacedDigit() }
                }.font(NekoFont.meta).fixedSize()
                Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold)).foregroundStyle(N.text3).accessibilityHidden(true)
            }
            .padding(.vertical, NekoLayout.rowInset)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(connection["label"].string), \(scope), \(status), \(summary)")
    }

    private func connectionDetail(_ connection: JSONValue) -> some View {
        let count = connection["tools"].array.count
        let filtered = connection["tools"].array.filter {
            toolSearch.isEmpty || ($0["name"].string + " " + $0["description"].string).localizedCaseInsensitiveContains(toolSearch)
        }
        return NekoSheet(title: connection["label"].string,
                         message: "\(ToolsPresentation.connectionStatus(connection)) · \(ToolsPresentation.toolSummary(connection))",
                         width: .wide, height: 640, cancelTitle: "Done", onCancel: { selectedConnection = nil }) {
            ScrollView {
                VStack(alignment: .leading, spacing: NekoSpace.xxl) {
                    operationFeedback
                    if !connection["error"].string.isEmpty {
                        DisclosureGroup {
                            Text(connection["error"].string).font(NekoFont.meta).textSelection(.enabled)
                        } label: {
                            Label(connection["error"].string, systemImage: "exclamationmark.triangle").lineLimit(2)
                        }.foregroundStyle(NekoStyle.amber)
                    }
                    HStack(spacing: NekoSpace.sm) {
                        Button("Refresh tools") { send("Mcp", "Discover", ["connection_id": connection["id"]]) }.disabled(!connection["enabled"].bool)
                        Button(connection["enabled"].bool ? "Pause connection" : "Enable connection") {
                            send("Mcp", "SetEnabled", ["connection_id": connection["id"], "enabled": .bool(!connection["enabled"].bool)])
                        }
                        if connection["config"]["transport"].string == "http" {
                            Button(ToolsPresentation.connectionStatus(connection) == "Sign in needed" ? "Retry sign-in" : "Sign in") {
                                send("Mcp", "Authenticate", ["connection_id": connection["id"], "client_id": clientID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? .null : .string(clientID.trimmingCharacters(in: .whitespacesAndNewlines))])
                            }.disabled(!connection["enabled"].bool)
                        }
                    }.controlSize(.regular).disabled(busy || model.busy)
                    DisclosureGroup("Connection details") {
                        VStack(alignment: .leading, spacing: 10) {
                            LabeledContent("Workspace", value: connection["workspace_id"].string.isEmpty ? "All workspaces" : (model.workspaces.first { $0.recordID == connection["workspace_id"].string }?["name"].string ?? "Unavailable"))
                            Text(connection["config"]["url"].string.isEmpty ? connection["config"]["command"].string : connection["config"]["url"].string)
                                .font(NekoFont.mono).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                            if connection["config"]["transport"].string != "http" {
                                Text("Arguments: \(connection["config"]["args"].array.map(\.string).joined(separator: " "))").textSelection(.enabled)
                                if !connection["config"]["cwd"].string.isEmpty { Text("Working directory: \(connection["config"]["cwd"].string)").textSelection(.enabled) }
                                Text(connection["trusted"].bool ? "Trusted local executable" : "Local executable trust required").foregroundStyle(N.text3)
                            } else {
                                LabeledContent("OAuth client ID") { TextField("Only if the server asks for one", text: $clientID).labelsHidden() }
                            }
                        }.font(NekoFont.meta).padding(.top, NekoSpace.sm)
                    }
                    HStack {
                        Text("Tools").font(NekoFont.heading)
                        Spacer()
                        if count > 0 { NekoSearchField(title: "Search tools", text: $toolSearch).frame(width: 220) }
                    }
                    if count == 0 {
                        NekoEmptyState(!connection["enabled"].bool ? "Enable this connection to discover its tools." : ToolsPresentation.connectionStatus(connection) == "Sign in needed" ? "Sign in to discover this server’s tools." : !connection["error"].string.isEmpty ? "No tools were discovered. Review the connection error above, then refresh." : connection["discovered_ms"] == .null ? "Discovering tools. Refresh if this takes too long." : "This server exposed no tools. Refresh to check again.")
                    } else if filtered.isEmpty {
                        NekoEmptyState(reason: .noMatches(toolSearch))
                    }
                    VStack(spacing: 0) {
                        ForEach(Array(filtered.enumerated()), id: \.offset) { index, tool in
                            toolRow(tool, connection: connection)
                            if index < filtered.count - 1 { Divider() }
                        }
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
        } actions: {
            if busy { ProgressView().controlSize(.small) }
        }
    }

    private func toolRow(_ tool: JSONValue, connection: JSONValue) -> some View {
        DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                if !tool["description"].string.isEmpty { Text(tool["description"].string).textSelection(.enabled) }
                Text(tool["read_only"].bool ? "The server declares this tool read-only. Chat lookups can run without another prompt." : "Chat asks before each call. Enabled responsibilities may use their selected connections unattended.")
                Text("Input schema").font(NekoFont.heading)
                Text(tool["input_schema"].string.isEmpty ? String(data: (try? JSONEncoder().encode(tool["input_schema"])) ?? Data(), encoding: .utf8) ?? "Unavailable" : tool["input_schema"].string)
                    .font(NekoFont.mono).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
            }.font(NekoFont.meta).foregroundStyle(N.text2).padding(.top, NekoSpace.sm)
        } label: {
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                Text(tool["name"].string).font(NekoFont.heading).foregroundStyle(N.text)
                Text(tool["read_only"].bool ? "Declared read-only" : "May change data").font(NekoFont.meta).foregroundStyle(N.text3)
                if !tool["description"].string.isEmpty { Text(tool["description"].string).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2) }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, NekoLayout.rowInset)
    }

    private var addConnection: some View {
        Group {
            Section("Connection") {
                LabeledContent("Connection name") { TextField("Name", text: $label).labelsHidden() }
                if workspace.isEmpty {
                    Text("Scope: Available in every workspace when connected.").font(NekoFont.body)
                } else {
                    Toggle("Available in all workspaces", isOn: $global)
                }
            }
            Section("Server") {
                Picker("Connection type", selection: $local) {
                    Text("Hosted server").tag(false)
                    Text("Local executable").tag(true)
                }
                .pickerStyle(.segmented).controlSize(.regular)
                .onChange(of: local) { _, _ in trust = false; target = "" }
                LabeledContent(local ? "Absolute executable path" : "Server URL") {
                    TextField(local ? "/path/to/executable" : "https://example.com/mcp", text: $target).labelsHidden()
                }
                if local {
                    LabeledContent("Arguments (JSON array)") { TextField("[]", text: $arguments).labelsHidden() }
                    LabeledContent("Working directory (optional)") {
                        HStack {
                            TextField("Default", text: $workingDirectory).labelsHidden()
                            Button("Choose…") {
                                FolderPicker.choose { selected in
                                    workingDirectory = selected.first?.path ?? ""
                                    trust = false
                                }
                            }
                        }
                    }
                    Text("Local servers execute code outside worker sandboxes. Nothing is downloaded automatically.").font(NekoFont.meta)
                    Toggle("I trust this executable and its arguments", isOn: $trust)
                }
            }
            Section {
                LabeledContent("Credentials (optional JSON)") { SecureField("Credentials", text: $credentials).labelsHidden() }
                Text("Use {\"bearer\":\"…\"} for HTTP or {\"environment\":{\"TOKEN\":\"…\"}} for local servers. Stored in Keychain. Keep secrets out of URLs and arguments.").font(NekoFont.meta).foregroundStyle(.secondary)
            } header: { Text("Credentials · optional") }
        }
        .textFieldStyle(.roundedBorder)
        .onChange(of: target) { _, _ in trust = false }
        .onChange(of: arguments) { _, _ in trust = false }
        .onChange(of: workingDirectory) { _, _ in trust = false }
    }

    private func addServer() {
        let destination = target.trimmingCharacters(in: .whitespacesAndNewlines)
        let config: JSONValue
        if local {
            guard destination.hasPrefix("/") else { validation = "Use an absolute executable path."; return }
            guard let data = arguments.data(using: .utf8), let args = try? JSONDecoder().decode([String].self, from: data) else {
                validation = "Arguments must be a JSON array of strings."; return
            }
            let directory = workingDirectory.trimmingCharacters(in: .whitespacesAndNewlines)
            if !directory.isEmpty {
                var isDirectory: ObjCBool = false
                guard directory.hasPrefix("/"), FileManager.default.fileExists(atPath: directory, isDirectory: &isDirectory), isDirectory.boolValue else {
                    validation = "Choose an existing absolute working directory."; return
                }
            }
            config = .object(["transport": .string("stdio"), "command": .string(destination), "args": .array(args.map(JSONValue.string)), "cwd": directory.isEmpty ? .null : .string(directory)])
        } else {
            guard let url = URL(string: destination), ["https", "http"].contains(url.scheme ?? ""), url.host != nil else { validation = "Enter a complete HTTP or HTTPS server URL."; return }
            config = .object(["transport": .string("http"), "url": .string(destination)])
        }
        let secret = credentials
        let scope = global || workspace.isEmpty ? "" : workspace
        send("Mcp", "AddConnection", ["workspace_id": .string(scope), "label": .string(label), "config": config, "trust_local_process": .bool(local && trust), "credentials": secret.isEmpty ? .null : .string(secret)], onSuccess: {
            if credentials == secret { credentials = "" }
        })
    }

    private var skillsBody: some View {
        VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
            HStack {
                VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                    Text("Instructions from this workspace's folders. A skill cannot access tools on its own.")
                        .font(NekoFont.body).foregroundStyle(N.text3)
                }
                Spacer(minLength: 12)
                Button("Refresh", systemImage: "arrow.clockwise") { send("Skills", "Refresh") }.controlSize(.regular)
            }
            NekoSearchField(title: "Search skills", text: $search)
            if !visibleSkills.isEmpty {
                VStack(spacing: 0) {
                    ForEach(Array(visibleSkills.enumerated()), id: \.offset) { index, skill in
                        skillRow(skill)
                        if index < visibleSkills.count - 1 { Divider().padding(.leading, 40) }
                    }
                }
            }
            ForEach(SkillPresentation.unavailable(enabled: model.snapshot["skills"]["enabled"].array, available: model.snapshot["skills"]["available"].array, workspace: workspace), id: \.self) { record in
                NekoSection(title: "Unavailable enabled skill", marker: NekoStyle.amber, grouped: true) { EmptyView() } content: {
                    VStack(alignment: .leading, spacing: NekoSpace.sm) {
                        Text(record["path"].string).textSelection(.enabled)
                        Text("Runs stop until these instructions are restored or disabled.").foregroundStyle(.secondary)
                        Button("Disable unavailable skill") {
                            send("Skills", "SetEnabled", ["workspace_id": record["workspace_id"], "path": record["path"], "content_hash": record["content_hash"], "enabled": .bool(false)])
                        }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            if visibleSkills.isEmpty {
                if search.isEmpty {
                    NekoEmptyState("No local skills available yet. Refresh local skills or fetch an instruction file below.")
                } else {
                    NekoEmptyState(reason: .noMatches(search))
                }
            }
            DisclosureGroup(isExpanded: $showFindSkill) {
                VStack(alignment: .leading, spacing: 10) {
                    Link("Browse skills.sh", destination: URL(string: "https://skills.sh")!)
                    LabeledContent("GitHub SKILL.md URL") { TextField("https://github.com/…/SKILL.md", text: $repositoryURL).labelsHidden() }
                    Text("Only the instruction file is previewed. Scripts and references are not installed.")
                        .font(NekoFont.meta).foregroundStyle(N.text3)
                    Button("Fetch for review") { send("Skills", "PreviewRepository", ["workspace_id": .string(workspace), "url": .string(repositoryURL)]) }.disabled(repositoryURL.isEmpty)
                }.padding(.top, NekoSpace.md)
            } label: {
                Label("Find a skill", systemImage: "plus")
                    .font(NekoFont.heading)
            }
            .padding(NekoLayout.rowInset)
            .background(N.card, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
            ForEach(model.snapshot["skills"]["proposals"].array.filter { $0["workspace_id"].string == workspace }, id: \.recordID) { proposal in
                proposalCard(proposal)
            }
        }
    }

    private var visibleSkills: [JSONValue] {
        model.snapshot["skills"]["available"].array.filter {
            ($0["workspace_id"].string.isEmpty || $0["workspace_id"].string == workspace) && (search.isEmpty || ($0["name"].string + " " + $0["description"].string).localizedCaseInsensitiveContains(search))
        }
    }

    private func skillRow(_ skill: JSONValue) -> some View {
        let record = SkillPresentation.enabledRecord(for: skill, enabled: model.snapshot["skills"]["enabled"].array, workspace: workspace)
        let changed = record != nil && record?["content_hash"] != skill["content_hash"]
        let key = skill["path"].string
        return VStack(alignment: .leading, spacing: 0) {
            Button {
                if expandedSkills.contains(key) { expandedSkills.remove(key) }
                else { expandedSkills.insert(key) }
            } label: {
                HStack(spacing: NekoSpace.md) {
                    Image(systemName: "text.book.closed").foregroundStyle(N.text3).frame(width: 28)
                    VStack(alignment: .leading, spacing: NekoSpace.xs) {
                        Text(skill["name"].string).font(NekoFont.heading).foregroundStyle(N.text)
                        Text(skill["description"].string).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2)
                    }
                    Spacer()
                    Text(record == nil ? "Available" : changed ? "Changed" : "Enabled")
                        .font(NekoFont.meta).foregroundStyle(changed ? NekoStyle.amber : N.text3)
                    Image(systemName: expandedSkills.contains(key) ? "chevron.up" : "chevron.down").font(NekoFont.meta).foregroundStyle(N.text4)
                }.contentShape(Rectangle()).padding(.vertical, NekoLayout.rowInset)
            }.buttonStyle(.plain)
            if expandedSkills.contains(key) {
                VStack(alignment: .leading, spacing: NekoSpace.sm) {
                Text(skill["description"].string).font(NekoFont.body).foregroundStyle(N.text3)
                Text(skill["path"].string).font(NekoFont.meta).foregroundStyle(N.text4).textSelection(.enabled)
                if changed { Text("Instructions changed. Review them before enabling the updated version.").foregroundStyle(.orange) }
                Button("Open instructions to review") {
                    if !NSWorkspace.shared.open(URL(fileURLWithPath: skill["path"].string)) { validation = "Could not open this instruction file." }
                }
                if record == nil || changed {
                    Button(changed ? "Enable reviewed changes" : "Enable current instructions") {
                        send("Skills", "SetEnabled", ["workspace_id": .string(workspace), "path": skill["path"], "content_hash": skill["content_hash"], "enabled": .bool(true)])
                    }
                }
                if let record {
                    Button("Disable in this workspace") {
                        send("Skills", "SetEnabled", ["workspace_id": record["workspace_id"], "path": record["path"], "content_hash": record["content_hash"], "enabled": .bool(false)])
                    }
                }
                }.frame(maxWidth: .infinity, alignment: .leading).padding(.leading, 40).padding(.bottom, NekoLayout.rowInset)
            }
        }
    }

    private func proposalCard(_ proposal: JSONValue) -> some View {
        let key = proposal["id"].string + ":" + proposal["content_hash"].string
        let auditURL = proposal["audit_url"].string
        let reviewed = auditURL.isEmpty || proposal["audit_reviewed_hash"].string == proposal["content_hash"].string
        return DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                Text(proposal["source"].string).font(NekoFont.meta).foregroundStyle(N.text3).textSelection(.enabled)
                Text(proposal["audit_status"].string).font(NekoFont.meta).foregroundStyle(N.text3)
                if let url = URL(string: auditURL), !auditURL.isEmpty {
                    Button("View published source and audit links") {
                        if NSWorkspace.shared.open(url) { openedAudits.insert(key) }
                    }
                    Text("Read the published audit results. If none are available, do not confirm or install. Confirmation records your review, not a passing audit.").font(NekoFont.meta)
                    Button("I reviewed the linked published audit results") {
                        send("Skills", "ConfirmAuditReview", ["id": proposal["id"], "content_hash": proposal["content_hash"], "audit_url": proposal["audit_url"]])
                    }.disabled(!openedAudits.contains(key))
                }
                ScrollView {
                    Text(verbatim: proposal["body"].string)
                        .font(NekoFont.mono).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(NekoSpace.md)
                }
                .frame(height: 240)
                .background(N.canvas, in: RoundedRectangle(cornerRadius: NekoRadius.md))
                .accessibilityLabel("Proposed skill instructions")
                Text("Accept saves these exact instructions. Enable separately after installation.").font(NekoFont.meta).foregroundStyle(N.text3)
                HStack {
                    Button("Accept & save reviewed content") { decide(proposal, accept: true) }.disabled(!reviewed)
                    Button("Reject") { decide(proposal, accept: false) }
                }
            }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, NekoSpace.md)
        } label: {
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                Text(proposal["name"].string).font(NekoFont.heading).foregroundStyle(N.text).lineLimit(2)
                Text("Suggested skill · Review instructions").font(NekoFont.meta).foregroundStyle(N.text3)
            }
        }
        .padding(NekoLayout.rowInset)
        .background(N.card, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
    }

    private func decide(_ proposal: JSONValue, accept: Bool) {
        send("Skills", "DecideProposal", ["id": proposal["id"], "content_hash": proposal["content_hash"], "accept": .bool(accept)])
    }
}
