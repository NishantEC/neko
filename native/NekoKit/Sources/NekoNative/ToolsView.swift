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
    @State private var showManual = false
    @State private var repositoryURL = ""
    @State private var search = ""
    @State private var validation: String?
    @State private var busy = false
    @State private var openedAudits: Set<String> = []
    @State private var expandedConnections: Set<String> = []
    @State private var expandedSkills: Set<String> = []
    @State private var showImports = false

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
        VStack(alignment: .leading, spacing: 12) {
            Text("Tools & skills").font(.system(size: 20, weight: .semibold)).foregroundStyle(N.text)
            Text("Connect the services Neko can use. You decide which tools each workspace may access.").font(.system(size: 13)).foregroundStyle(N.text3)
            if let error = validation ?? model.error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
                Picker("Show", selection: $tab) {
                    Text("Connections").tag(0)
                    Text("Skills").tag(1)
                }.pickerStyle(.segmented).labelsHidden().frame(maxWidth: 260)
                ScrollView {
                    VStack(alignment: .leading, spacing: 20) {
                        if tab == 0 { connectionsBody }
                        else if workspace.isEmpty {
                            ContentUnavailableView("Choose a workspace for skills", systemImage: "folder", description: Text("Pick a workspace in the toolbar to see the skills its folders already contain."))
                        } else { skillsBody }
                    }.frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 8)
                }
                .disabled(busy || model.busy)
            if busy { ProgressView("Updating…").controlSize(.small) }
        }.padding(24)
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
        VStack(alignment: .leading, spacing: 20) {
            if workspace.isEmpty {
                Label("Viewing all connections. Choose a workspace in the sidebar to set permissions.", systemImage: "square.stack.3d.up").font(.callout).foregroundStyle(.secondary)
            }
            HStack {
                Text("Connected tools").font(.system(size: 15, weight: .semibold)).foregroundStyle(N.text)
                Text("\(connections.count)").font(.caption).foregroundStyle(N.text4)
                Spacer()
                if !workspace.isEmpty {
                    Text("\(model.snapshot["mcp"]["grants"].array.filter { $0["workspace_id"].string == workspace }.count) allowed in this workspace")
                        .font(.caption).foregroundStyle(N.text3)
                }
            }
            ForEach(connections, id: \.recordID) { connection in connectionCard(connection) }
            if connections.isEmpty {
                ContentUnavailableView("No tools connected", systemImage: "shippingbox", description: Text("Import a tool from this Mac or add an MCP server below."))
            }
            DisclosureGroup(isExpanded: $showImports) {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Add a tool already configured in Codex, Claude, or a project folder. Existing connections stay unchanged.")
                        .font(.callout).foregroundStyle(N.text3)
                    Button("Scan this Mac again") {
                        send("SetupImport", "Discover", ["repositories": .array(folders), "source_id": .null])
                    }.controlSize(.small)
                    ForEach(unlinkedCandidates, id: \.recordID) { candidate in candidateRow(candidate) }
                    if unlinkedCandidates.isEmpty {
                        Text("No new connections found.").font(.callout).foregroundStyle(N.text4)
                    }
                }.padding(.top, 12)
            } label: {
                Label("Import from this Mac", systemImage: "square.and.arrow.down")
                    .font(.system(size: 14, weight: .medium))
            }
            .padding(16)
            .background(N.card.opacity(0.6), in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(N.line))
            Button(showManual ? "Hide manual setup" : "Add a tool manually…", systemImage: showManual ? "chevron.down" : "chevron.right") { showManual.toggle() }
                .buttonStyle(.borderless)
            if showManual {
                addConnection
                LabeledContent("OAuth client ID (advanced)") { TextField("Only if the tool asks for one", text: $clientID).labelsHidden() }
                Text("Signing in never gives a workspace access by itself; you still choose per workspace.").font(.caption).foregroundStyle(.secondary)
                Link("Find tools in the MCP Registry", destination: URL(string: "https://registry.modelcontextprotocol.io")!)
            }
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
        return VStack(alignment: .leading, spacing: 6) {
            Text(candidate["name"].string).font(.headline)
            Text(candidate["source"].string).font(.caption).foregroundStyle(N.text3)
            DisclosureGroup("Connection details") {
                Text(candidate["metadata"]["config_summary"].string).textSelection(.enabled)
                Text("Configuration: " + candidate["metadata"]["config_fingerprint"].string).font(.caption).textSelection(.enabled)
            }.font(.caption)
            if !problem.isEmpty { Text(problem).foregroundStyle(.red) }
            if localProcess { Text("This executable runs on your Mac outside the agent sandbox. Review the command before trusting it.").font(.caption) }
            if workspace.isEmpty { Text("Select a workspace before linking this source configuration.").font(.caption).foregroundStyle(.secondary) }
            Button(localProcess ? "Trust and connect local tool" : "Connect this tool") {
                send("Mcp", "LinkSource", ["workspace_id": .string(workspace), "candidate_id": candidate["id"], "trust_local_process": .bool(localProcess)])
            }.disabled(workspace.isEmpty || !problem.isEmpty || candidate["metadata"]["enabled_at_source"].string == "false")
            Divider()
        }
    }

    private func connectionCard(_ connection: JSONValue) -> some View {
        let count = connection["tools"].array.count
        let allowed = model.snapshot["mcp"]["grants"].array.filter { $0["workspace_id"].string == workspace && $0["connection_id"].string == connection.recordID }.count
        return VStack(alignment: .leading, spacing: 0) {
            Button {
                if expandedConnections.contains(connection.recordID) { expandedConnections.remove(connection.recordID) }
                else { expandedConnections.insert(connection.recordID) }
            } label: {
                HStack(spacing: 12) {
                    Image(systemName: connection["enabled"].bool ? "checkmark.circle.fill" : "pause.circle")
                        .foregroundStyle(connection["enabled"].bool ? NekoStyle.mint : N.text4)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(connection["label"].string).font(.system(size: 14, weight: .medium)).foregroundStyle(N.text)
                        Text(connection["workspace_id"].string.isEmpty ? "Available to all workspaces" : (model.workspaces.first { $0.recordID == connection["workspace_id"].string }?["name"].string ?? "Workspace tool"))
                            .font(.caption).foregroundStyle(N.text4)
                    }
                    Spacer()
                    Text(workspace.isEmpty ? "\(count) tools" : "\(allowed) of \(count) allowed")
                        .font(.caption).foregroundStyle(N.text3)
                    Image(systemName: expandedConnections.contains(connection.recordID) ? "chevron.up" : "chevron.down")
                        .font(.caption).foregroundStyle(N.text4)
                }.contentShape(Rectangle()).padding(15)
            }.buttonStyle(.plain)
            if expandedConnections.contains(connection.recordID) {
                VStack(alignment: .leading, spacing: 10) {
                Text(connection["config"]["url"].string.isEmpty ? connection["config"]["command"].string : connection["config"]["url"].string).font(.caption).foregroundStyle(N.text4).textSelection(.enabled)
                if !connection["error"].string.isEmpty { Text(connection["error"].string).foregroundStyle(.red) }
                HStack {
                    Button("Discover tools") { send("Mcp", "Discover", ["connection_id": connection["id"]]) }.disabled(!connection["enabled"].bool)
                    Button(connection["enabled"].bool ? "Pause & revoke grants" : "Enable") {
                        send("Mcp", "SetEnabled", ["connection_id": connection["id"], "enabled": .bool(!connection["enabled"].bool)])
                    }
                    if connection["config"]["transport"].string == "http" {
                        Button("Sign in through browser") {
                            send("Mcp", "Authenticate", ["connection_id": connection["id"], "client_id": clientID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? .null : .string(clientID.trimmingCharacters(in: .whitespacesAndNewlines))])
                        }.disabled(!connection["enabled"].bool)
                    }
                }
                if count == 0 { Text("No tools discovered yet. Discover tools, then choose what Neko may use.").font(.callout).foregroundStyle(N.text3) }
                ForEach(Array(connection["tools"].array.enumerated()), id: \.offset) { _, tool in toolRow(tool, connection: connection) }
                }.padding(.horizontal, 15).padding(.bottom, 15)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(N.card.opacity(0.65), in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(N.line))
    }

    private func toolRow(_ tool: JSONValue, connection: JSONValue) -> some View {
        let allowed = model.snapshot["mcp"]["grants"].array.contains {
            $0["workspace_id"].string == workspace && $0["connection_id"].string == connection["id"].string && $0["tool_name"].string == tool["name"].string && $0["schema_hash"].string == tool["schema_hash"].string
        }
        return VStack(alignment: .leading, spacing: 6) {
            Divider()
            Text(tool["name"].string).font(.headline)
            Text(tool["description"].string).foregroundStyle(.secondary)
            Text(tool["read_only"].bool ? "Server declares read-only: granted chat lookups run without another prompt." : "Action or unspecified effect: chat requires approval per call; unattended responsibilities use the grant directly.").font(.caption)
            DisclosureGroup("Input schema") { Text(tool["input_schema"].string).font(.system(.caption, design: .monospaced)).textSelection(.enabled) }
            Button(allowed ? "Revoke tool access" : (tool["read_only"].bool ? "Allow Neko to read with this tool" : "Allow Neko to request this tool")) {
                send("Mcp", "SetWorkspaceToolGrant", ["workspace_id": .string(workspace), "connection_id": connection["id"], "tool_name": tool["name"], "schema_hash": tool["schema_hash"], "allowed": .bool(!allowed)])
            }.disabled(workspace.isEmpty || !connection["enabled"].bool || !connection["error"].string.isEmpty)
        }
    }

    private var addConnection: some View {
        GroupBox("Tool server") {
            VStack(alignment: .leading, spacing: 12) {
                LabeledContent("Connection name") { TextField("Name", text: $label).labelsHidden() }
                if workspace.isEmpty {
                    Text("Scope: Global definition. Grant tools separately in each workspace.").font(.callout)
                } else {
                    Toggle("Global definition (grant separately in each workspace)", isOn: $global)
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
        VStack(alignment: .leading, spacing: 14) {
            Text("Skills teach Neko how to work. They do not give it access to tools.").font(.callout).foregroundStyle(N.text3)
            HStack(spacing: 12) {
                TextField("Search skills", text: $search).textFieldStyle(.roundedBorder)
                Button("Refresh", systemImage: "arrow.clockwise") { send("Skills", "Refresh") }.controlSize(.small)
            }
            ForEach(Array(visibleSkills.enumerated()), id: \.offset) { _, skill in skillRow(skill) }
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
            GroupBox("Find a skill") {
                VStack(alignment: .leading, spacing: 10) {
                    Link("Browse skills.sh", destination: URL(string: "https://skills.sh")!)
                    LabeledContent("GitHub SKILL.md URL") { TextField("https://github.com/…/SKILL.md", text: $repositoryURL).labelsHidden() }
                    Text("Preview fetches only the instruction file (64 KB maximum). Scripts and references are not installed. Choose a self-contained skill.").font(.caption)
                    Button("Fetch for review") { send("Skills", "PreviewRepository", ["workspace_id": .string(workspace), "url": .string(repositoryURL)]) }.disabled(repositoryURL.isEmpty)
                }.padding(8)
            }
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
                    Image(systemName: "text.book.closed").foregroundStyle(N.text3).frame(width: 18)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(skill["name"].string).font(.system(size: 14, weight: .medium)).foregroundStyle(N.text)
                        Text(skill["description"].string).font(.caption).foregroundStyle(N.text4).lineLimit(1)
                    }
                    Spacer()
                    Text(record == nil ? "Available" : changed ? "Changed" : "Enabled")
                        .font(.caption).foregroundStyle(changed ? NekoStyle.amber : N.text3)
                    Image(systemName: expandedSkills.contains(key) ? "chevron.up" : "chevron.down").font(.caption).foregroundStyle(N.text4)
                }.contentShape(Rectangle()).padding(14)
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
                }.frame(maxWidth: .infinity, alignment: .leading).padding(.horizontal, 14).padding(.bottom, 14)
            }
        }
        .background(N.card.opacity(0.65), in: RoundedRectangle(cornerRadius: 12))
        .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(N.line))
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
