import SwiftUI
import NekoKit

/// One connection: its status, sign-in and pause controls, and its tools.
struct ToolsConnectionDetail: View {
    @ObservedObject var model: AppModel
    let connection: JSONValue
    @Binding var toolSearch: String
    @Binding var clientID: String
    let busy: Bool
    let feedback: String?
    let send: ToolsCommand
    let close: () -> Void

    var body: some View {
        let count = connection["tools"].array.count
        let filtered = connection["tools"].array.filter {
            toolSearch.isEmpty || ($0["name"].string + " " + $0["description"].string).localizedCaseInsensitiveContains(toolSearch)
        }
        NekoSheet(title: connection["label"].string,
                  message: "\(ToolsPresentation.connectionStatus(connection)) · \(ToolsPresentation.toolSummary(connection))",
                  width: .wide, height: 640, cancelTitle: "Done", onCancel: close) {
            ScrollView {
                VStack(alignment: .leading, spacing: NekoSpace.xxl) {
                    ToolsOperationFeedback(message: feedback)
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
                                send("Mcp", "Authenticate", ["connection_id": connection["id"], "client_id": ConnectionForm.clientIDValue(clientID)])
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
                            toolRow(tool)
                            if index < filtered.count - 1 { Divider() }
                        }
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
        } actions: {
            if busy { ProgressView().controlSize(.small) }
        }
    }

    private func toolRow(_ tool: JSONValue) -> some View {
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
}
