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
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 5) {
                    Text("Tools & skills").font(.system(size: 25, weight: .semibold)).foregroundStyle(N.text)
                    Text("Connections provide access. Skills provide instructions.")
                        .font(.system(size: 13)).foregroundStyle(N.text3)
                }
                Spacer(minLength: 12)
                if busy { ProgressView().controlSize(.small).accessibilityLabel("Updating tools and skills") }
            }
            .padding(.bottom, 24)

            HStack {
                GlassSegmented(selection: $tab, options: [
                    .init(value: 0, title: "Connections"),
                    .init(value: 1, title: "Skills")
                ])
                Spacer()
                if tab == 0 {
                    Menu {
                        Button("Browse MCP servers", systemImage: "square.grid.2x2") { showRegistrySheet = true }
                        Button("Import from this Mac", systemImage: "square.and.arrow.down") { showImportSheet = true }
                        Button("Add connection manually", systemImage: "plus") { showManualSheet = true }
                    } label: {
                        HStack(spacing: 8) {
                            Label("Add connection", systemImage: "plus")
                            Image(systemName: "chevron.down").font(.system(size: 9, weight: .semibold))
                        }
                        .font(.system(size: 13))
                        .padding(.horizontal, 10)
                        .frame(height: NekoControlMetrics.height())
                        .background(Color.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 8))
                        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(Color.primary.opacity(0.12)))
                        .contentShape(Rectangle())
                    }
                    .menuStyle(.borderlessButton).menuIndicator(.hidden).fixedSize()
                }
            }
            .padding(.bottom, 16)

            if let error = validation ?? model.error {
                Label(error, systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange)
                    .textSelection(.enabled)
                    .padding(.bottom, 12)
            }
            ScrollView {
                Group {
                    if tab == 0 { connectionsBody }
                    else if workspace.isEmpty {
                        ContentUnavailableView("Choose a workspace for skills", systemImage: "folder", description: Text("Choose a workspace in the sidebar to see skills in its folders."))
                    } else { skillsBody }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.bottom, 28)
            }
            .disabled(busy || model.busy)
        }
        .frame(maxWidth: 940, maxHeight: .infinity, alignment: .topLeading)
        .padding(.horizontal, 32)
        .padding(.top, 28)
        .frame(maxWidth: .infinity, alignment: .top)
        .sheet(item: $selectedConnection) { selection in
            if let connection = connections.first(where: { $0.recordID == selection.id }) {
                connectionDetail(connection)
            } else {
                ContentUnavailableView("Connection unavailable", systemImage: "link", description: Text("Close this window and choose a connection again."))
                    .frame(width: 560, height: 300)
            }
        }
        .sheet(isPresented: $showImportSheet) { importSheet }
        .sheet(isPresented: $showManualSheet) { manualSheet }
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

    private var connectionsBody: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .firstTextBaseline) {
                Text("Connections").font(.system(size: 14, weight: .semibold)).foregroundStyle(N.text)
                Text("\(connections.count)").font(.caption).foregroundStyle(N.text4)
                Spacer()
                if workspace.isEmpty {
                    Text("All workspaces").font(.caption).foregroundStyle(N.text4)
                } else {
                    Text(model.workspaces.first { $0.recordID == workspace }?["name"].string ?? "Selected workspace")
                        .font(.caption).foregroundStyle(N.text4)
                }
            }
            .padding(.bottom, 10)
            if workspace.isEmpty {
                    Text("Choose a workspace in the sidebar to see the connections it can use.")
                    .font(.callout).foregroundStyle(N.text3)
                    .padding(.bottom, 14)
            }
            if connections.isEmpty {
                ContentUnavailableView("No connections yet", systemImage: "link", description: Text("Browse MCP servers, import from this Mac, or add a URL manually."))
                    .frame(maxWidth: .infinity, minHeight: 240)
            } else {
                VStack(spacing: 0) {
                    ForEach(Array(connections.enumerated()), id: \.element.recordID) { index, connection in
                        connectionRow(connection)
                        if index < connections.count - 1 { Divider().padding(.leading, 48) }
                    }
                }
                .background(N.card.opacity(0.45), in: RoundedRectangle(cornerRadius: 13))
                .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(N.line.opacity(0.7)))
            }
        }
    }

    private var importSheet: some View {
        let items = unlinkedCandidates
        return VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text("Import from this Mac").font(.title2.weight(.semibold))
                Spacer()
                Button("Done") { showImportSheet = false }
            }
            .padding(.bottom, 6)
            Text("Review connections already configured in other apps. Nothing is linked until you choose it.")
                .font(.callout).foregroundStyle(.secondary)
                .padding(.bottom, 18)
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
                }
            }
            .controlSize(.small)
            .padding(.bottom, 10)
            if workspace.isEmpty {
                Text("Choose a workspace above to link a connection.")
                    .font(.callout).foregroundStyle(N.text3)
                    .padding(.bottom, 12)
            }
            ScrollView {
                if items.isEmpty {
                    VStack {
                        ContentUnavailableView("No new connections found", systemImage: "magnifyingglass", description: Text("Scan again after adding a tool in Codex, Claude, or a project folder."))
                    }.frame(maxWidth: .infinity, minHeight: 260)
                } else {
                    VStack(spacing: 0) {
                        ForEach(Array(items.enumerated()), id: \.element.recordID) { index, candidate in
                            candidateRow(candidate)
                            if index < items.count - 1 { Divider().padding(.leading, 16) }
                        }
                    }
                    .background(N.card.opacity(0.4), in: RoundedRectangle(cornerRadius: 12))
                    .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(N.line.opacity(0.65)))
                }
            }
        }
        .padding(24)
        .frame(width: 670, height: 570)
    }

    private var manualSheet: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack {
                Text("Add connection").font(.title2.weight(.semibold))
                Spacer()
                Button("Done") { showManualSheet = false }
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 14) {
                    addConnection
                    LabeledContent("OAuth client ID (advanced)") { TextField("Only if the tool asks for one", text: $clientID).labelsHidden() }
                    Text("Connecting makes discovered tools available here. Neko asks before chat actions that may change data.")
                        .font(.caption).foregroundStyle(.secondary)
                    Button("Browse hosted MCP servers") { showManualSheet = false; showRegistrySheet = true }
                }
            }
        }
        .padding(24)
        .frame(width: 650, height: 620)
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
                HStack(spacing: 12) {
                    Image(systemName: localProcess ? "terminal" : "link")
                        .font(.system(size: 16, weight: .regular))
                        .foregroundStyle(N.text3)
                        .frame(width: 24)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(candidate["name"].string)
                            .font(.system(size: 14, weight: .medium)).foregroundStyle(N.text)
                        Text(source.isEmpty ? "Local configuration" : source)
                            .font(.caption).foregroundStyle(N.text4)
                    }
                    Spacer(minLength: 10)
                    if !problem.isEmpty {
                        Text("Needs setup").font(.caption).foregroundStyle(NekoStyle.amber)
                    } else if candidate["metadata"]["enabled_at_source"].string == "false" {
                        Text("Paused at source").font(.caption).foregroundStyle(N.text4)
                    } else {
                        Text(localProcess ? "Local" : "Remote").font(.caption).foregroundStyle(N.text4)
                    }
                    Image(systemName: expanded ? "chevron.up" : "chevron.down")
                        .font(.system(size: 10, weight: .semibold)).foregroundStyle(N.text4)
                }
                .padding(.horizontal, 16)
                .frame(minHeight: 64)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if expanded {
                VStack(alignment: .leading, spacing: 10) {
                    if !problem.isEmpty {
                        Label(problem, systemImage: "exclamationmark.triangle")
                            .font(.callout).foregroundStyle(NekoStyle.amber)
                    }
                    Text(candidate["metadata"]["config_summary"].string)
                        .font(.system(.caption, design: .monospaced))
                        .foregroundStyle(N.text3)
                        .textSelection(.enabled)
                    if localProcess {
                        Text("This executable runs on your Mac outside the agent sandbox. Review its command before trusting it.")
                            .font(.caption).foregroundStyle(N.text4)
                    }
                    Button(localProcess ? "Trust and connect" : "Connect") {
                        send("Mcp", "LinkSource", ["workspace_id": .string(workspace), "candidate_id": candidate["id"], "trust_local_process": .bool(localProcess)])
                    }
                    .disabled(workspace.isEmpty || !problem.isEmpty || candidate["metadata"]["enabled_at_source"].string == "false")
                    .controlSize(.small)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, 52).padding(.trailing, 16).padding(.bottom, 16)
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
            HStack(spacing: 13) {
                Image(systemName: "point.3.connected.trianglepath.dotted")
                    .font(.system(size: 17, weight: .regular))
                    .foregroundStyle(N.text3)
                    .frame(width: 32)
                VStack(alignment: .leading, spacing: 4) {
                    Text(connection["label"].string).font(.system(size: 14, weight: .medium)).foregroundStyle(N.text)
                    Text("\(scope)  ·  \(status)").font(.caption).foregroundStyle(N.text4)
                }
                Spacer(minLength: 10)
                Text(summary)
                    .font(.caption).foregroundStyle(N.text3)
                Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(N.text4)
            }
            .padding(.horizontal, 16)
            .frame(minHeight: 64)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(connection["label"].string), \(status), \(summary)")
    }

    private func connectionDetail(_ connection: JSONValue) -> some View {
        let count = connection["tools"].array.count
        let filtered = connection["tools"].array.filter {
            toolSearch.isEmpty || ($0["name"].string + " " + $0["description"].string).localizedCaseInsensitiveContains(toolSearch)
        }
        return VStack(alignment: .leading, spacing: 0) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(connection["label"].string).font(.title2.weight(.semibold))
                    Text(ToolsPresentation.toolSummary(connection))
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Done") { selectedConnection = nil }
            }
            .padding(.bottom, 18)
            if !connection["error"].string.isEmpty {
                Label(connection["error"].string, systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange).textSelection(.enabled).padding(.bottom, 14)
            }
            HStack {
                Button("Refresh tools") { send("Mcp", "Discover", ["connection_id": connection["id"]]) }
                    .disabled(!connection["enabled"].bool)
                Button(connection["enabled"].bool ? "Pause connection" : "Enable connection") {
                    send("Mcp", "SetEnabled", ["connection_id": connection["id"], "enabled": .bool(!connection["enabled"].bool)])
                }
                if connection["config"]["transport"].string == "http" {
                    Button(ToolsPresentation.connectionStatus(connection) == "Sign in needed" ? "Retry sign-in" : "Sign in") {
                        send("Mcp", "Authenticate", ["connection_id": connection["id"], "client_id": clientID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? .null : .string(clientID.trimmingCharacters(in: .whitespacesAndNewlines))])
                    }.disabled(!connection["enabled"].bool)
                }
                Spacer()
            }
            .controlSize(.small)
            .padding(.bottom, 18)
            DisclosureGroup("Connection details") {
                Text(connection["config"]["url"].string.isEmpty ? connection["config"]["command"].string : connection["config"]["url"].string)
                    .font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            .font(.callout)
            .padding(.bottom, 18)
            HStack {
                Text("Tools").font(.headline)
                Spacer()
                if count > 0 { NekoSearchField(title: "Search tools", text: $toolSearch).frame(width: 220) }
            }
            .padding(.bottom, 10)
            Divider()
            ScrollView {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if count == 0 {
                        Text(ToolsPresentation.connectionStatus(connection) == "Sign in needed" ? "Sign in again. Neko will discover its tools automatically, then watch for relevant work when the server offers a read-only source." : connection["discovered_ms"] == .null ? "Neko is discovering this connection's tools automatically. You can refresh if it takes too long." : "This connection did not expose any tools. Use Refresh tools to check again.")
                            .font(.callout).foregroundStyle(.secondary).padding(.vertical, 22)
                    } else if filtered.isEmpty {
                        Text("No tools match your search.").font(.callout).foregroundStyle(.secondary).padding(.vertical, 22)
                    }
                    ForEach(Array(filtered.enumerated()), id: \.offset) { index, tool in
                        toolRow(tool, connection: connection)
                        if index < filtered.count - 1 { Divider() }
                    }
                }
            }
        }
        .padding(24)
        .frame(width: 700, height: 620)
    }

    private func toolRow(_ tool: JSONValue, connection: JSONValue) -> some View {
        return VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline) {
                Text(tool["name"].string).font(.system(size: 14, weight: .medium))
                Spacer(minLength: 12)
                Text(!connection["enabled"].bool ? "Paused" : (connection["error"].string.isEmpty ? "Available" : "Needs attention"))
                    .font(.caption).foregroundStyle(N.text4)
            }
            if !tool["description"].string.isEmpty {
                Text(tool["description"].string)
                    .font(.callout).foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            HStack(spacing: 10) {
                Text(tool["read_only"].bool ? "Declared read-only" : "May change data")
                    .font(.caption).foregroundStyle(N.text4)
                DisclosureGroup("Details") {
                    VStack(alignment: .leading, spacing: 10) {
                        Text(tool["description"].string).textSelection(.enabled)
                        Text(tool["read_only"].bool ? "The server declares this tool read-only. Chat lookups can run without another prompt." : "Chat asks before each call. Enabled responsibilities may use their selected connections unattended.")
                        Text("Input schema").fontWeight(.medium)
                        Text(tool["input_schema"].string)
                            .font(.system(.caption, design: .monospaced))
                            .textSelection(.enabled)
                    }
                    .font(.caption)
                    .foregroundStyle(N.text3)
                    .padding(.top, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .fixedSize(horizontal: true, vertical: false)
            }
            .font(.caption)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(.vertical, 14)
    }

    private var addConnection: some View {
        GroupBox("Tool server") {
            VStack(alignment: .leading, spacing: 12) {
                LabeledContent("Connection name") { TextField("Name", text: $label).labelsHidden() }
                if workspace.isEmpty {
                    Text("Scope: Available in every workspace when connected.").font(.callout)
                } else {
                    Toggle("Available in all workspaces", isOn: $global)
                }
                Toggle("Local executable", isOn: $local).onChange(of: local) { _, _ in trust = false; target = "" }
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
                    Text("Local servers execute code outside worker sandboxes. Nothing is downloaded automatically.").font(.caption)
                    Toggle("I trust this executable and its arguments", isOn: $trust)
                }
                LabeledContent("Credentials (optional JSON)") { SecureField("Credentials", text: $credentials).labelsHidden() }
                Text("Use {\"bearer\":\"…\"} for HTTP or {\"environment\":{\"TOKEN\":\"…\"}} for local servers. Stored in Keychain. Keep secrets out of URLs and arguments.").font(.caption).foregroundStyle(.secondary)
                Button("Add server", action: addServer).disabled(label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || target.isEmpty || (local && !trust))
            }.padding(8)
                .onChange(of: target) { _, _ in trust = false }
                .onChange(of: arguments) { _, _ in trust = false }
                .onChange(of: workingDirectory) { _, _ in trust = false }
        }
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
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Skills").font(.system(size: 14, weight: .semibold)).foregroundStyle(N.text)
                    Text("Instructions from this workspace's folders. A skill cannot access tools on its own.")
                        .font(.callout).foregroundStyle(N.text3)
                }
                Spacer(minLength: 12)
                Button("Refresh", systemImage: "arrow.clockwise") { send("Skills", "Refresh") }.controlSize(.small)
            }
            NekoSearchField(title: "Search skills", text: $search)
            if !visibleSkills.isEmpty {
                VStack(spacing: 0) {
                    ForEach(Array(visibleSkills.enumerated()), id: \.offset) { index, skill in
                        skillRow(skill)
                        if index < visibleSkills.count - 1 { Divider().padding(.leading, 46) }
                    }
                }
                .background(N.card.opacity(0.45), in: RoundedRectangle(cornerRadius: 13))
                .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(N.line.opacity(0.7)))
            }
            ForEach(SkillPresentation.unavailable(enabled: model.snapshot["skills"]["enabled"].array, available: model.snapshot["skills"]["available"].array, workspace: workspace), id: \.self) { record in
                GroupBox("Unavailable enabled skill") {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(record["path"].string).textSelection(.enabled)
                        Text("Runs stop until these instructions are restored or disabled.").foregroundStyle(.secondary)
                        Button("Disable unavailable skill") {
                            send("Skills", "SetEnabled", ["workspace_id": record["workspace_id"], "path": record["path"], "content_hash": record["content_hash"], "enabled": .bool(false)])
                        }
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(8)
                }
            }
            if visibleSkills.isEmpty {
                Text(search.isEmpty ? "No local skills available yet. Refresh local skills or fetch an instruction file below." : "No skills match this search.").foregroundStyle(.secondary)
            }
            DisclosureGroup(isExpanded: $showFindSkill) {
                VStack(alignment: .leading, spacing: 10) {
                    Link("Browse skills.sh", destination: URL(string: "https://skills.sh")!)
                    LabeledContent("GitHub SKILL.md URL") { TextField("https://github.com/…/SKILL.md", text: $repositoryURL).labelsHidden() }
                    Text("Only the instruction file is previewed. Scripts and references are not installed.")
                        .font(.caption).foregroundStyle(N.text3)
                    Button("Fetch for review") { send("Skills", "PreviewRepository", ["workspace_id": .string(workspace), "url": .string(repositoryURL)]) }.disabled(repositoryURL.isEmpty)
                }.padding(.top, 12)
            } label: {
                Label("Find a skill", systemImage: "plus")
                    .font(.system(size: 14, weight: .medium))
            }
            .padding(16)
            .background(N.card.opacity(0.45), in: RoundedRectangle(cornerRadius: 13))
            .overlay(RoundedRectangle(cornerRadius: 13).strokeBorder(N.line.opacity(0.7)))
            ForEach(Array(model.snapshot["skills"]["proposals"].array.filter { $0["workspace_id"].string == workspace }.enumerated()), id: \.offset) { _, proposal in proposalCard(proposal) }
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
                HStack(spacing: 12) {
                    Image(systemName: "text.book.closed").foregroundStyle(N.text3).frame(width: 28)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(skill["name"].string).font(.system(size: 14, weight: .medium)).foregroundStyle(N.text)
                        Text(skill["description"].string).font(.caption).foregroundStyle(N.text4).lineLimit(1)
                    }
                    Spacer()
                    Text(record == nil ? "Available" : changed ? "Changed" : "Enabled")
                        .font(.caption).foregroundStyle(changed ? NekoStyle.amber : N.text3)
                    Image(systemName: expandedSkills.contains(key) ? "chevron.up" : "chevron.down").font(.caption).foregroundStyle(N.text4)
                }.contentShape(Rectangle()).padding(.horizontal, 16).frame(minHeight: 64)
            }.buttonStyle(.plain)
            if expandedSkills.contains(key) {
                VStack(alignment: .leading, spacing: 8) {
                Text(skill["description"].string).font(.callout).foregroundStyle(N.text3)
                Text(skill["path"].string).font(.caption).foregroundStyle(N.text4).textSelection(.enabled)
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
                }.frame(maxWidth: .infinity, alignment: .leading).padding(.leading, 56).padding(.trailing, 16).padding(.bottom, 16)
            }
        }
    }

    private func proposalCard(_ proposal: JSONValue) -> some View {
        let key = proposal["id"].string + ":" + proposal["content_hash"].string
        let auditURL = proposal["audit_url"].string
        let reviewed = auditURL.isEmpty || proposal["audit_reviewed_hash"].string == proposal["content_hash"].string
        return GroupBox("Review: " + proposal["name"].string) {
            VStack(alignment: .leading, spacing: 10) {
                Text(proposal["source"].string).textSelection(.enabled)
                Text(proposal["audit_status"].string).font(.caption)
                if let url = URL(string: auditURL), !auditURL.isEmpty {
                    Button("View published source and audit links") {
                        if NSWorkspace.shared.open(url) { openedAudits.insert(key) }
                    }
                    Text("Read the published audit results. If none are available, do not confirm or install. Confirmation records your review, not a passing audit.").font(.caption)
                    Button("I reviewed the linked published audit results") {
                        send("Skills", "ConfirmAuditReview", ["id": proposal["id"], "content_hash": proposal["content_hash"], "audit_url": proposal["audit_url"]])
                    }.disabled(!openedAudits.contains(key))
                }
                ScrollView { Text(proposal["body"].string).textSelection(.enabled).frame(maxWidth: .infinity, alignment: .leading) }.frame(maxHeight: 300)
                Text("Accept saves these exact instructions. Enable separately after installation.").font(.caption)
                HStack {
                    Button("Accept & save reviewed content") { decide(proposal, accept: true) }.disabled(!reviewed)
                    Button("Reject") { decide(proposal, accept: false) }
                }
            }.frame(maxWidth: .infinity, alignment: .leading).padding(8)
        }
    }

    private func decide(_ proposal: JSONValue, accept: Bool) {
        send("Skills", "DecideProposal", ["id": proposal["id"], "content_hash": proposal["content_hash"], "accept": .bool(accept)])
    }
}
