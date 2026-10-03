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
    @State private var scope: String = ""
    private var visibleResults: [RegistryServer] { results.filter { $0.url != RegistryCatalog.sentry.url } }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Find a connection").font(.title2.weight(.semibold))
                    Text("Browse hosted MCP servers. Review the destination before connecting.")
                        .font(.callout).foregroundStyle(.secondary)
                }
                Spacer()
                Button("Done") { dismiss() }
            }
            .padding(.bottom, 20)
            NekoSearchField(title: "Search the MCP Registry", text: $query)
                .padding(.bottom, 14)
            HStack(spacing: 8) {
                Image(systemName: "folder").foregroundStyle(.secondary)
                Picker("Available in", selection: $scope) {
                    Text("All workspaces").tag("")
                    ForEach(model.workspaces, id: \.recordID) { item in Text(item["name"].string).tag(item.recordID) }
                }
                .labelsHidden().frame(maxWidth: 220)
                Spacer()
                if loading { ProgressView().controlSize(.small) }
                Text("Public registry · hosted servers only").font(.caption).foregroundStyle(.tertiary)
            }
            .padding(.bottom, 12)
            if let error {
                Label(error, systemImage: "exclamationmark.triangle")
                    .font(.callout).foregroundStyle(.orange).padding(.bottom, 10)
            }
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    Text("FEATURED").font(.caption2.weight(.semibold)).foregroundStyle(.secondary).padding(.bottom, 8)
                    serverRow(RegistryCatalog.sentry, subtitle: "Sentry · official hosted server")
                    if !query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        Text("REGISTRY RESULTS").font(.caption2.weight(.semibold)).foregroundStyle(.secondary)
                            .padding(.top, 22).padding(.bottom, 8)
                        if !loading && visibleResults.isEmpty {
                            Text(results.isEmpty ? "No hosted servers found. Try another name or add a URL manually." : "Sentry is shown in Featured above.")
                                .font(.callout).foregroundStyle(.secondary).padding(.vertical, 14)
                        }
                        ForEach(visibleResults) { server in
                            serverRow(server, subtitle: "Community listing · verify its publisher")
                        }
                    }
                }
            }
            Divider().padding(.vertical, 14)
            if let addedID, let connection = model.snapshot["mcp"]["connections"].array.first(where: { $0.recordID == addedID }) {
                VStack(alignment: .leading, spacing: 8) {
                    Label("\(connection["label"].string) added", systemImage: "checkmark.circle.fill")
                        .font(.headline).foregroundStyle(.green)
                    Text(connection["oauth"].bool ? "Signed in. Neko is discovering its tools automatically." : "Neko discovers tools automatically. Sign in if this server requires an account; a watch starts only when it offers a read-only work source.")
                        .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    HStack {
                        Button("Sign in") { Task { await run("Authenticate", id: addedID) } }
                        Button("Refresh tools") { Task { await run("Discover", id: addedID) } }
                        Spacer()
                        Button("Done") { dismiss() }
                    }
                    .disabled(connecting || model.busy)
                }
            } else if let selected {
                VStack(alignment: .leading, spacing: 7) {
                    Text(selected.shortName).font(.headline)
                    Text(selected.url.absoluteString).font(.system(.caption, design: .monospaced)).textSelection(.enabled)
                    Text("The server can expose tools that read or change data. Only connect a provider you trust. Signing in and background responsibilities are separate steps.")
                        .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                    HStack {
                        Spacer()
                        Button("Cancel") { self.selected = nil }
                        Button(connecting ? "Adding…" : "Add connection") { Task { await add(selected) } }
                            .buttonStyle(.borderedProminent).disabled(connecting || model.busy)
                    }.padding(.top, 6)
                }
            } else {
                Text("Select a server to inspect its URL before adding it.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .padding(24)
        .frame(width: 660, height: 500)
        .onAppear { scope = model.selectedWorkspace ?? "" }
        .task(id: query) {
            let term = query.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !term.isEmpty else { results = []; loading = false; error = nil; return }
            loading = true
            do {
                try await Task.sleep(for: .milliseconds(300))
                let found = try await RegistryCatalog.search(term)
                guard !Task.isCancelled else { return }
                results = found
                error = nil
            } catch is CancellationError {
                return
            } catch {
                guard !Task.isCancelled else { return }
                results = []
                self.error = error.localizedDescription
            }
            loading = false
        }
    }

    private func serverRow(_ server: RegistryServer, subtitle: String) -> some View {
        Button { selected = server; error = nil } label: {
            HStack(spacing: 12) {
                Image(systemName: "network").font(.system(size: 16)).foregroundStyle(.secondary).frame(width: 24)
                VStack(alignment: .leading, spacing: 3) {
                    Text(subtitle.hasPrefix("Community") ? server.name : server.shortName)
                        .font(.system(size: 14, weight: .medium))
                    Text(subtitle).font(.caption).foregroundStyle(.secondary)
                    if !server.description.isEmpty { Text(server.description).font(.caption).foregroundStyle(.secondary).lineLimit(2) }
                }
                Spacer()
                Image(systemName: selected == server ? "checkmark.circle.fill" : "chevron.right")
                    .foregroundStyle(selected == server ? Color.accentColor : .secondary)
            }
            .padding(13).frame(maxWidth: .infinity, alignment: .leading)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(selected == server ? Color.accentColor.opacity(0.1) : Color.primary.opacity(0.035), in: RoundedRectangle(cornerRadius: 10))
        .padding(.bottom, 6)
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
