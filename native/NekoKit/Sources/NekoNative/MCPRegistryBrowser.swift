import Foundation
import SwiftUI
import NekoKit

struct RegistryServer: Identifiable, Hashable {
    let name: String
    let description: String
    let url: URL
    var id: String { name + "\u{1f}" + url.absoluteString }
    var shortName: String { String(name.split(separator: "/").last ?? Substring(name)) }
}

enum RegistryCatalog {
    static let endpoint = URL(string: "https://registry.modelcontextprotocol.io/v0.1/servers")!
    static let sentry = RegistryServer(name: "io.github.getsentry/sentry-mcp", description: "Investigate errors, traces, and issues in Sentry.", url: URL(string: "https://mcp.sentry.dev/mcp")!)

    static func parse(_ data: Data) throws -> [RegistryServer] {
        let response = try JSONDecoder().decode(JSONValue.self, from: data)
        return response["servers"].array.compactMap { entry in
            let server = entry["server"]
            let name = server["name"].string
            guard !name.isEmpty else { return nil }
            guard let remote = server["remotes"].array.first(where: { $0["type"].string == "streamable-http" && validURL($0["url"].string) != nil }),
                  let url = validURL(remote["url"].string) else { return nil }
            return RegistryServer(name: name, description: server["description"].string, url: url)
        }
    }

    static func search(_ query: String) async throws -> [RegistryServer] {
        var components = URLComponents(url: endpoint, resolvingAgainstBaseURL: false)!
        components.queryItems = [URLQueryItem(name: "search", value: query), URLQueryItem(name: "version", value: "latest"), URLQueryItem(name: "limit", value: "30")]
        var request = URLRequest(url: components.url!)
        request.timeoutInterval = 12
        let (data, response) = try await URLSession.shared.data(for: request)
        guard let response = response as? HTTPURLResponse, response.statusCode == 200 else {
            throw NSError(domain: "MCP Registry", code: 1, userInfo: [NSLocalizedDescriptionKey: "The registry is unavailable. Try again or add a server URL manually."])
        }
        return try parse(data)
    }

    private static func validURL(_ value: String) -> URL? {
        guard let url = URL(string: value), url.scheme == "https", url.host != nil,
              url.user == nil, url.password == nil, url.fragment == nil else { return nil }
        return url
    }
}

@MainActor
struct MCPRegistryBrowser: View {
    @ObservedObject var model: AppModel
    @Environment(\.dismiss) private var dismiss
    @State private var query = ""
    @State private var results: [RegistryServer] = []
    @State private var selected: RegistryServer?
    @State private var addedID: String?
    @State private var loading = false
    @State private var connecting = false
    @State private var error: String?
    @State private var searchError: String?
    @State private var scope: String = ""
    private var visibleResults: [RegistryServer] { results.filter { $0.url != RegistryCatalog.sentry.url } }

    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack {
                Text("Browse MCP servers").font(NekoFont.title)
                Spacer()
                Button("Done") { dismiss() }.keyboardShortcut(.cancelAction)
            }
            HStack(spacing: 14) {
                NekoSearchField(title: "Search the public registry", text: $query)
                Picker("Available in", selection: $scope) {
                    Text("All workspaces").tag("")
                    ForEach(model.workspaces, id: \.recordID) { item in Text(item["name"].string).tag(item.recordID) }
                }
                .frame(width: 220).disabled(connecting || model.busy)
            }
            if let searchError {
                Label(searchError, systemImage: "exclamationmark.triangle")
                    .font(NekoFont.meta).foregroundStyle(NekoStyle.amber).fixedSize(horizontal: false, vertical: true)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Text("Featured").font(NekoFont.heading).foregroundStyle(N.text3).padding(.bottom, 8)
                    serverRow(RegistryCatalog.sentry, subtitle: "Official Sentry server")
                    if !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        HStack {
                            Text("Registry results").font(NekoFont.heading)
                            Spacer()
                            if loading { ProgressView().controlSize(.small).accessibilityLabel("Searching registry") }
                            else { Text("\(visibleResults.count)").font(NekoFont.meta).monospacedDigit() }
                        }
                        .foregroundStyle(N.text3).padding(.top, 20).padding(.bottom, 8)
                        if loading {
                            Text("Searching hosted servers…").font(NekoFont.meta).foregroundStyle(N.text3).padding(.vertical, 12)
                        } else if visibleResults.isEmpty, searchError == nil {
                            Text(results.isEmpty ? "No hosted servers found. Try another name, or add a URL manually from Tools & skills." : "Sentry is shown in Featured above.")
                                .font(NekoFont.body).foregroundStyle(N.text3).padding(.vertical, 12)
                        }
                        ForEach(visibleResults) { server in serverRow(server, subtitle: "Community listing · verify publisher") }
                    }
                }.frame(maxWidth: .infinity, alignment: .leading)
            }
            Divider()
            connectionReview
        }
        .font(NekoFont.body).controlSize(.regular)
        .padding(NekoLayout.pageInset).frame(width: 680, height: 600)
        .onAppear { scope = model.selectedWorkspace ?? "" }
        .onChange(of: scope) { _, _ in addedID = nil }
        .task(id: query) { await searchRegistry() }
    }

    @ViewBuilder private var connectionReview: some View {
        if let error {
            DisclosureGroup {
                Text(error).font(NekoFont.meta).textSelection(.enabled)
            } label: {
                Label(error, systemImage: "exclamationmark.triangle").lineLimit(2)
            }.foregroundStyle(NekoStyle.amber)
        }
        if let addedID, let connection = model.snapshot["mcp"]["connections"].array.first(where: { $0.recordID == addedID }) {
            VStack(alignment: .leading, spacing: 10) {
                Label("\(connection["label"].string) added", systemImage: "checkmark.circle").font(NekoFont.heading)
                Text("\(ToolsPresentation.connectionStatus(connection)) · \(ToolsPresentation.toolSummary(connection))")
                    .font(NekoFont.meta).foregroundStyle(N.text3)
                if !connection["error"].string.isEmpty {
                    DisclosureGroup("Connection needs attention") {
                        Text(connection["error"].string).font(NekoFont.meta).textSelection(.enabled)
                    }.foregroundStyle(NekoStyle.amber)
                }
                HStack(spacing: 8) {
                    Button("Sign in") { Task { await run("Authenticate", id: addedID) } }
                    Button("Refresh tools") { Task { await run("Discover", id: addedID) } }
                    if connecting { ProgressView().controlSize(.small) }
                    Spacer()
                    Button("Done") { dismiss() }
                }.disabled(connecting || model.busy)
            }
        } else if let selected {
            VStack(alignment: .leading, spacing: 10) {
                Text(selected.name).font(NekoFont.heading).lineLimit(2)
                ScrollView {
                    VStack(alignment: .leading, spacing: 8) {
                        Text(selected.url.absoluteString).font(NekoFont.mono).textSelection(.enabled).fixedSize(horizontal: false, vertical: true)
                        Text("Connect only a provider you trust. Its tools may read or change data in the selected workspace. Read-only work sources can start a watch; manage these in Watching.")
                            .font(NekoFont.meta).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }.frame(height: 88)
                HStack(spacing: 8) {
                    Text(scope.isEmpty ? "All workspaces" : (model.workspaces.first { $0.recordID == scope }?["name"].string ?? "Workspace"))
                        .font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(1)
                    Spacer()
                    Button("Cancel") { self.selected = nil; error = nil }.disabled(connecting)
                    Button(connecting ? "Adding…" : "Add connection") { Task { await add(selected) } }
                        .nekoPrimaryButton().disabled(connecting || model.busy)
                }
            }
        } else {
            Text("Select a hosted server to review its publisher, address and workspace access.")
                .font(NekoFont.meta).foregroundStyle(N.text3)
        }
    }

    private func serverRow(_ server: RegistryServer, subtitle: String) -> some View {
        Button {
            selected = server
            addedID = nil
            error = nil
        } label: {
            HStack(spacing: 12) {
                Image(systemName: "network").font(NekoFont.body).foregroundStyle(N.text3).frame(width: 18).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 3) {
                    Text(server.shortName).font(NekoFont.heading).foregroundStyle(N.text).lineLimit(1)
                    Text(subtitle).font(NekoFont.meta).foregroundStyle(N.text3)
                    if !server.description.isEmpty { Text(server.description).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(1) }
                }.frame(maxWidth: .infinity, alignment: .leading)
                Image(systemName: selected == server ? "checkmark" : "chevron.right")
                    .font(NekoFont.meta).foregroundStyle(N.text3).accessibilityHidden(true)
            }
            .padding(NekoLayout.rowInset).contentShape(Rectangle())
        }
        .buttonStyle(.plain).disabled(connecting || model.busy)
        .background(selected == server ? N.selected : N.card, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(selected == server ? N.lineStrong : N.line))
        .padding(.bottom, 6)
        .accessibilityLabel("\(server.name), \(subtitle)")
        .accessibilityValue(selected == server ? "Selected" : "")
    }

    private func searchRegistry() async {
        let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
        results = []
        searchError = nil
        guard !term.isEmpty else { loading = false; return }
        loading = true
        do {
            try await Task.sleep(for: .milliseconds(300))
            let found = try await RegistryCatalog.search(term)
            guard !Task.isCancelled else { return }
            results = found
        } catch is CancellationError {
            return
        } catch {
            guard !Task.isCancelled else { return }
            searchError = error.localizedDescription
        }
        loading = false
    }

    private func add(_ server: RegistryServer) async {
        connecting = true
        defer { connecting = false }
        if let existing = model.snapshot["mcp"]["connections"].array.first(where: {
            $0["workspace_id"].string == scope && $0["config"]["url"].string == server.url.absoluteString
        }) {
            addedID = existing.recordID
            return
        }
        let before = Set(model.snapshot["mcp"]["connections"].array.map(\.recordID))
        let succeeded = await model.workbench(.object(["Mcp": .command("AddConnection", [
            "workspace_id": .string(scope), "label": .string(server.shortName),
            "config": .object(["transport": .string("http"), "url": .string(server.url.absoluteString)]),
            "trust_local_process": .bool(false), "credentials": .null
        ])]))
        if succeeded {
            addedID = model.snapshot["mcp"]["connections"].array.first(where: {
                !before.contains($0.recordID) && $0["config"]["url"].string == server.url.absoluteString
            })?.recordID
            if addedID == nil { error = "Connection saved, but its details did not appear. Reopen Tools & skills." }
        }
        else { error = model.error ?? "Could not add the connection." }
    }

    private func run(_ command: String, id: String) async {
        connecting = true
        defer { connecting = false }
        let fields: [String: JSONValue] = command == "Authenticate"
            ? ["connection_id": .string(id), "client_id": .null]
            : ["connection_id": .string(id)]
        let succeeded = await model.workbench(.object(["Mcp": .command(command, fields)]))
        if !succeeded { error = model.error ?? "Could not \(command.lowercased()) this server." }
        else { error = nil }
    }
}
