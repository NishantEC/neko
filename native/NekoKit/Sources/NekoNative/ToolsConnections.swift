import SwiftUI
import AppKit
import NekoKit

/// Sends one Tools command through the page's shared busy and error state.
struct ToolsCommand {
    let run: (_ family: String, _ name: String, _ fields: [String: JSONValue]?, _ onSuccess: (() -> Void)?) -> Void
    func callAsFunction(_ family: String, _ name: String, _ fields: [String: JSONValue]? = nil, onSuccess: (() -> Void)? = nil) {
        run(family, name, fields, onSuccess)
    }
}

/// The last error from a Tools command, folded to two lines.
struct ToolsOperationFeedback: View {
    let message: String?
    var body: some View {
        if let message {
            DisclosureGroup {
                Text(message).font(NekoFont.meta).textSelection(.enabled).padding(.top, NekoSpace.xxs)
            } label: {
                Label(message, systemImage: "exclamationmark.triangle").font(NekoFont.body).lineLimit(2)
            }.foregroundStyle(NekoStyle.amber)
        }
    }
}

// MARK: - Connections tab

/// Every connection in scope; each row opens its detail sheet.
struct ToolsConnectionsList: View {
    @ObservedObject var model: AppModel
    let workspace: String
    let connections: [JSONValue]
    let open: (JSONValue) -> Void
    var body: some View {
        VStack(alignment: .leading, spacing: NekoSpace.xs) {
            NekoListHeader(title: "Connections", tone: .neutral, count: connections.count) {
                Text(workspace.isEmpty ? "All workspaces" : (model.workspaces.first { $0.recordID == workspace }?["name"].string ?? "Selected workspace"))
                    .lineLimit(1)
            }
            if connections.isEmpty {
                NekoEmptyState("Browse MCP servers, import from this Mac, or add a URL manually.",
                               title: model.connected ? "No connections yet" : "Waiting for connections…", symbol: "link", block: true)
            } else {
                let sorted = ToolsPresentation.sorted(connections)
                VStack(spacing: 0) {
                    ForEach(Array(sorted.enumerated()), id: \.element.recordID) { index, connection in
                        connectionRow(connection)
                        // Starts at the title's edge, past the glyph slot.
                        if index < sorted.count - 1 { Hairline().padding(.leading, NekoLayout.listGlyphSlot + NekoSpace.md + NekoSpace.md) }
                    }
                }
            }
        }
    }

    private func connectionRow(_ connection: JSONValue) -> some View {
        let summary = ToolsPresentation.toolSummary(connection)
        let scope = connection["workspace_id"].string.isEmpty ? "All workspaces" : (model.workspaces.first { $0.recordID == connection["workspace_id"].string }?["name"].string ?? "Workspace")
        let status = ToolsPresentation.connectionStatus(connection)
        return Button {
            open(connection)
        } label: {
            NekoListRow(title: connection["label"].string, note: ToolsPresentation.connectionDetail(connection)) {
                Image(systemName: connection["config"]["transport"].string == "http" ? "link" : "terminal")
                    .font(NekoFont.body).accessibilityHidden(true)
            } trailing: {
                NekoStatusLabel(tone: ToolsPresentation.connectionTone(connection), symbol: ToolsPresentation.connectionSymbol(connection), text: status)
                if summary != status { Text(summary).font(NekoFont.caption.monospacedDigit()).foregroundStyle(N.text3).fixedSize() }
                Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold)).foregroundStyle(N.text3).accessibilityHidden(true)
            }
            .padding(.vertical, NekoSpace.xxs)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(connection["label"].string), \(scope), \(status), \(summary)")
    }
}

// MARK: - Import

/// Connections already configured in other apps on this Mac, to link one by one.
struct ToolsImportSheet: View {
    @ObservedObject var model: AppModel
    let workspace: String
    let folders: [JSONValue]
    let connections: [JSONValue]
    let busy: Bool
    let feedback: String?
    @Binding var expandedCandidates: Set<String>
    let send: ToolsCommand
    let close: () -> Void

    var body: some View {
        let items = unlinkedCandidates
        NekoSheet(title: "Import from this Mac",
                  message: "Review connections already configured in other apps. Nothing is linked until you choose it.",
                  width: .wide, height: 620, cancelTitle: "Done", onCancel: close) {
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
                ToolsOperationFeedback(message: feedback)
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
}

// MARK: - Add manually

/// A hosted server URL or a trusted local executable, typed by hand.
struct ToolsManualConnectionSheet: View {
    @ObservedObject var model: AppModel
    @Binding var form: ConnectionForm
    let workspace: String
    let busy: Bool
    let feedback: String?
    let add: () -> Void
    /// Closes this sheet and opens the MCP server browser.
    let browse: () -> Void
    let close: () -> Void

    var body: some View {
        NekoSheet(title: "Add connection", message: "Connect a hosted server or a trusted executable on this Mac.",
                  width: .wide, height: 660, cancelTitle: "Done", onCancel: close) {
            ToolsOperationFeedback(message: feedback)
            if busy { ProgressView("Adding connection…").controlSize(.small) }
            Form {
                fields
                Section("Advanced sign-in") {
                    LabeledContent("OAuth client ID (advanced)") { TextField("Only if the tool asks for one", text: $form.clientID).labelsHidden() }
                }
            }
            .formStyle(.grouped).scrollContentBackground(.hidden)
            .disabled(busy || model.busy)
            Text("Connecting makes discovered tools available here. Neko asks before chat actions that may change data.")
                .font(NekoFont.meta).foregroundStyle(.secondary)
        } actions: {
            Button("Browse hosted MCP servers", action: browse)
            Button("Add connection", action: add).nekoButton(.primary)
                .disabled(busy || model.busy || !form.canSubmit)
        }
    }

    private var fields: some View {
        Group {
            Section("Connection") {
                LabeledContent("Connection name") { TextField("Name", text: $form.label).labelsHidden() }
                if workspace.isEmpty {
                    Text("Scope: Available in every workspace when connected.").font(NekoFont.body)
                } else {
                    Toggle("Available in all workspaces", isOn: $form.global)
                }
            }
            Section("Server") {
                Picker("Connection type", selection: $form.local) {
                    Text("Hosted server").tag(false)
                    Text("Local executable").tag(true)
                }
                .pickerStyle(.segmented).controlSize(.regular)
                .onChange(of: form.local) { _, _ in form.trust = false; form.target = "" }
                LabeledContent(form.local ? "Absolute executable path" : "Server URL") {
                    TextField(form.local ? "/path/to/executable" : "https://example.com/mcp", text: $form.target).labelsHidden()
                }
                if form.local {
                    LabeledContent("Arguments (JSON array)") { TextField("[]", text: $form.arguments).labelsHidden() }
                    LabeledContent("Working directory (optional)") {
                        HStack {
                            TextField("Default", text: $form.workingDirectory).labelsHidden()
                            Button("Choose…") {
                                FolderPicker.choose { selected in
                                    form.workingDirectory = selected.first?.path ?? ""
                                    form.trust = false
                                }
                            }
                        }
                    }
                    Text("Local servers execute code outside worker sandboxes. Nothing is downloaded automatically.").font(NekoFont.meta)
                    Toggle("I trust this executable and its arguments", isOn: $form.trust)
                }
            }
            Section {
                LabeledContent("Credentials (optional JSON)") { SecureField("Credentials", text: $form.credentials).labelsHidden() }
                Text("Use {\"bearer\":\"…\"} for HTTP or {\"environment\":{\"TOKEN\":\"…\"}} for local servers. Stored in Keychain. Keep secrets out of URLs and arguments.").font(NekoFont.meta).foregroundStyle(.secondary)
            } header: { Text("Credentials · optional") }
        }
        .textFieldStyle(.roundedBorder)
        .onChange(of: form.target) { _, _ in form.trust = false }
        .onChange(of: form.arguments) { _, _ in form.trust = false }
        .onChange(of: form.workingDirectory) { _, _ in form.trust = false }
    }
}
