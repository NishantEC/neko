import SwiftUI
import AppKit
import NekoKit

struct ClipboardConsentState {
    private(set) var enabled: Bool?
    private(set) var requested: Bool?
    mutating func load(_ reply: JSONValue) throws {
        guard case .bool(let value) = reply["ClipboardHistoryEnabled"]["enabled"] else { throw invalidReply() }
        enabled = value
    }
    mutating func begin(_ value: Bool) -> Bool {
        guard enabled != nil, requested == nil else { return false }
        requested = value
        return true
    }
    mutating func finish(_ reply: JSONValue) throws {
        defer { requested = nil }
        guard let requested, reply["ClipboardHistoryEnabled"]["enabled"] == .bool(requested) else { throw invalidReply() }
        enabled = requested
    }
    mutating func failed() { requested = nil }
    private func invalidReply() -> NSError { NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "The clipboard setting was not acknowledged. Please try again."]) }
}

/// The palette owner installs an atomic registration operation before settings are used.
/// It must register the candidate before releasing the old shortcut, or throw.
@MainActor enum NativeHotkeySettings {
    static var register: ((JSONValue) throws -> Void)?
}

struct HotkeySettingsView: View {
    @ObservedObject var model: AppModel
    @State private var combo: JSONValue = .null
    @State private var recording = false
    @State private var monitor: Any?
    @State private var pending = false
    var body: some View {
        HStack {
            Text("Summon shortcut")
            Spacer()
            Button(recording ? "Press shortcut… (Esc cancels)" : label) { recording.toggle(); updateMonitor() }.disabled(pending)
        }.task {
            do { combo = (try await model.request(.string("GetHotkey")))["Hotkey"]["config"]["combo"] }
            catch { model.error = error.localizedDescription }
        }.onDisappear { recording = false; updateMonitor() }
    }
    private var label: String {
        let symbols = ["Cmd": "⌘", "Alt": "⌥", "Ctrl": "⌃", "Shift": "⇧"]
        return combo == .null ? "Loading…" : combo["modifiers"].array.map { symbols[$0.string] ?? $0.string }.joined() + combo["key"].string.replacingOccurrences(of: "Key", with: "")
    }
    private func updateMonitor() {
        if let monitor { NSEvent.removeMonitor(monitor); self.monitor = nil }
        guard recording else { return }
        monitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { event in
            if event.keyCode == 53 { recording = false; updateMonitor(); return nil }
            var modifiers: [JSONValue] = []
            for (flag, name): (NSEvent.ModifierFlags, String) in [(.command, "Cmd"), (.option, "Alt"), (.control, "Ctrl"), (.shift, "Shift")] where event.modifierFlags.contains(flag) { modifiers.append(.string(name)) }
            guard modifiers.contains(.string("Cmd")) || modifiers.contains(.string("Alt")) || modifiers.contains(.string("Ctrl")) else { model.error = "Use Command, Option or Control with the shortcut."; return nil }
            guard let key = NativeHotkeyCodes.name(for: event.keyCode) else {
                model.error = "This physical key cannot be used as a shortcut. Choose another key."; return nil
            }
            let candidate: JSONValue = .object(["modifiers": .array(modifiers), "key": .string(key)])
            recording = false; updateMonitor(); pending = true
            Task {
                let previous = combo
                do {
                    let conflict = try await model.request(.command("CheckHotkeyConflict", ["candidate": candidate]))
                    if conflict["HotkeyConflict"]["reason"] != .null { throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: conflict["HotkeyConflict"]["reason"].string]) }
                    guard let register = NativeHotkeySettings.register else { throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "Shortcut registration is not available yet."]) }
                    try register(candidate)
                    do {
                        _ = try await model.request(.command("CommitHotkey", ["candidate": candidate]))
                    } catch { try? register(previous); throw error }
                    combo = candidate; model.error = nil
                } catch { model.error = error.localizedDescription }
                pending = false
            }
            return nil
        }
    }
}

struct PreferencesView: View {
    @ObservedObject var model: AppModel
    @State private var settings: [JSONValue] = []
    @State private var folders: [JSONValue] = []
    @State private var agents: [JSONValue] = []
    @State private var path = ""
    @State private var pending = false
    @State private var clipboard = ClipboardConsentState()
    @State private var agentProvider = "codex"
    @State private var agentModel = ""
    @State private var catalog = ModelCatalog()
    @State private var customModel = ""
    @State private var checking = false
    @State private var check: ModelCheckResult?
    @State private var refreshingModels = false
    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Settings").font(.system(size: 24, weight: .semibold))
                .padding(.horizontal, 12)
            TabView {
                Form {
                    HotkeySettingsView(model: model)
                    preferenceToggle("Launch at login", "launch-at-login")
                    Toggle("Clipboard history", isOn: Binding(get: { clipboard.enabled ?? false }, set: { value in setClipboard(value) })).disabled(clipboard.enabled == nil)
                    Text("Saves copied content locally. Turning this off stops new capture; existing history remains.").font(.caption).foregroundStyle(.secondary)
                    if clipboard.enabled == nil { Button("Read clipboard setting") { Task { await loadClipboard() } } }
                }.padding().tabItem { Label("General", systemImage: "gearshape") }
                Form {
                    Section("Agent runtime") {
                        Picker("Runs with", selection: $agentProvider) {
                            ForEach(catalog.sources.isEmpty ? [] : catalog.sources) { source in
                                Text(source.label).tag(source.provider)
                            }
                            if catalog.sources.isEmpty { Text("Codex").tag("codex") }
                            if agentProvider == "opencodex" { Text("Connected model (legacy)").tag("opencodex") }
                        }
                        .onChange(of: agentProvider) { _, _ in check = nil; if model.snapshot["agent_runtime"]["provider"].string != agentProvider { agentModel = ""; customModel = "" } }
                        if let source = catalog.source(agentProvider) {
                            LabeledContent("Connection") {
                                Label(source.connection, systemImage: source.ready ? "checkmark.circle" : "exclamationmark.circle")
                                    .foregroundStyle(source.ready ? Color.secondary : NekoStyle.amber)
                            }
                            if let note = source.note { Text(note).font(.caption).foregroundStyle(.secondary) }
                            Picker("Model", selection: $agentModel) {
                                Text(source.defaultTitle).tag("")
                                ForEach(source.models) { entry in
                                    Text(pickerTitle(entry)).tag(entry.id)
                                }
                                if !agentModel.isEmpty, !source.models.contains(where: { $0.id == agentModel }) {
                                    Text(agentModel).tag(agentModel)
                                }
                            }
                            .onChange(of: agentModel) { _, _ in check = nil }
                            if let selected = catalog.model(agentProvider, agentModel) {
                                if let reason = selected.reason ?? selected.description { Text(reason).font(.caption).foregroundStyle(.secondary) }
                            }
                        } else if agentProvider == "opencodex" {
                            Text("This model was set up through an external proxy. Choose a provider above to move to Neko’s own runtime list.")
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        HStack {
                            TextField("Or enter an exact model ID", text: $customModel)
                                .textFieldStyle(.roundedBorder)
                            Button("Use ID") { agentModel = customModel.trimmingCharacters(in: .whitespacesAndNewlines) }
                                .disabled(!validModelID(customModel.trimmingCharacters(in: .whitespacesAndNewlines)))
                        }
                        HStack {
                            Button(checking ? "Checking…" : "Check & use model") { checkAndUse() }
                                .buttonStyle(.borderedProminent)
                                .disabled(checking || pending || selectedUnavailable)
                            Button("Use without checking") { saveAgentRuntime() }
                                .disabled(checking || pending || selectedUnavailable)
                            Spacer()
                            Button(refreshingModels ? "Refreshing…" : "Refresh models") { refreshModels() }
                                .disabled(refreshingModels)
                        }
                        if let check {
                            Label(check.message, systemImage: check.ok ? "checkmark.circle.fill" : "xmark.circle")
                                .foregroundStyle(check.ok ? Color.green : (check.unavailable ? NekoStyle.amber : Color.red))
                                .font(.callout)
                        }
                        Text("Checking sends one short reply through the same runner tasks use, so it may use quota. A failed check keeps your current model. New conversations, tasks and background checks use the saved model; work already running is not interrupted.")
                            .font(.caption).foregroundStyle(.secondary)
                    }
                }.padding().tabItem { Label("AI", systemImage: "cpu") }
                VStack(alignment: .leading) {
                    Text("Search folders").font(.headline)
                    List(folders, id: \.self) { folder in
                        HStack { Text(folder["title"].string); Spacer(); Button("Remove") { activate("folder-scope", folder["id"].string) } }
                    }
                    HStack { TextField("Folder path", text: $path); Button("Choose…") { choose() }; Button("Add") { activate("folder-scope", path, action: "add") }.disabled(path.trimmingCharacters(in: .whitespaces).isEmpty) }
                }.padding().tabItem { Label("Search", systemImage: "magnifyingglass") }
                PermissionsView(model: model)
                    .tabItem { Label("Permissions", systemImage: "lock.shield") }
                Form {
                    Text(agents.isEmpty ? "No agent provider is turned on." : "\(agents.count) agents visible from the configured provider.")
                    preferenceToggle("Show agents", "agents-enabled")
                    preferenceToggle("Include idle agents", "agents-include-idle")
                    Text("Legacy agent providers are opt-in. These settings do not enable a provider.").font(.caption).foregroundStyle(.secondary)
                }.padding().tabItem { Label("Agents", systemImage: "person.2") }
                VStack(spacing: 16) {
                    BrandMark(size: 72)
                    Text("Neko").font(.title.bold())
                    Text("Version \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "Development")")
                    Text("Your workspace, native on your Mac.").foregroundStyle(.secondary)
                }.padding().tabItem { Label("About", systemImage: "info.circle") }
            }.disabled(pending)
            if let error = model.error { Text(error).foregroundStyle(.red).textSelection(.enabled).padding() }
        }
        .frame(maxWidth: 860, maxHeight: .infinity)
        .padding(.horizontal, 28).padding(.bottom, 20)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .task {
            await load()
            catalog = await AgentModelCatalog.load(model)
        }
    }
    private func preferenceToggle(_ title: String, _ id: String) -> some View {
        Toggle(title, isOn: Binding(get: { settings.first { $0["id"].string == id }?["accessory"].string == "On" }, set: { _ in activate("preference", id) }))
    }
    private func load() async {
        let runtime = model.snapshot["agent_runtime"]
        agentProvider = runtime["provider"].string.isEmpty ? "codex" : runtime["provider"].string
        agentModel = runtime["model"].string
        await loadClipboard()
        do {
            settings = try await search("preference")
            folders = try await search("folder-scope")
        } catch { model.error = error.localizedDescription }
        // Legacy agent providers are opt-in (NEKO_LEGACY_AGENTS=1). When the
        // provider is not registered, that simply means "no agents", not an error.
        agents = (try? await search("agent")) ?? []
    }
    private func saveAgentRuntime() {
        pending = true
        Task {
            _ = await model.workbench(.command("SetAgentRuntime", ["runtime": .object([
                "provider": .string(agentProvider), "model": .string(agentModel.trimmingCharacters(in: .whitespacesAndNewlines))
            ])]))
            pending = false
        }
    }
    private var selectedUnavailable: Bool {
        catalog.source(agentProvider).map { !$0.ready } ?? false
            || catalog.model(agentProvider, agentModel).map { !$0.usable } ?? false
    }
    private func pickerTitle(_ entry: CatalogModel) -> String {
        var title = entry.label
        if entry.recommended { title += " · Recommended" }
        if entry.access == .checked { title += " · Checked" }
        if !entry.usable { title += " · Unavailable" }
        return title
    }
    private func validModelID(_ id: String) -> Bool {
        !id.isEmpty && id.count <= 120 && !id.hasPrefix("-")
            && id.unicodeScalars.allSatisfy { ($0.isASCII && CharacterSet.alphanumerics.contains($0)) || ".-_:/@[]".unicodeScalars.contains($0) }
    }
    private func checkAndUse() {
        checking = true
        check = nil
        let provider = agentProvider, id = agentModel
        Task {
            check = await AgentModelCatalog.check(model, provider: provider, id: id, save: true)
            catalog = await AgentModelCatalog.load(model)
            checking = false
        }
    }
    private func refreshModels() {
        refreshingModels = true
        Task {
            catalog = await AgentModelCatalog.load(model, refresh: true)
            refreshingModels = false
        }
    }
    private func search(_ provider: String) async throws -> [JSONValue] {
        (try await model.request(.command("Search", ["query": .string(""), "limit": .number(200), "provider": .string(provider)])))["SearchResults"]["items"].array
    }
    private func loadClipboard() async {
        guard !pending else { return }
        do { try clipboard.load(await model.request(.string("GetClipboardHistoryEnabled"))) }
        catch { model.error = error.localizedDescription }
    }
    private func setClipboard(_ enabled: Bool) {
        guard !pending, clipboard.begin(enabled) else { return }
        pending = true
        Task {
            defer { pending = false }
            do {
                let reply = try await model.request(.command("SetClipboardHistoryEnabled", ["enabled": .bool(enabled)]))
                try clipboard.finish(reply)
                model.error = nil
            } catch { clipboard.failed(); model.error = error.localizedDescription }
        }
    }
    private func activate(_ kind: String, _ id: String, action: String? = nil) {
        guard !pending else { return }
        pending = true
        Task {
            do {
                _ = try await model.request(.command("Activate", ["kind": .string(kind), "id": .string(id), "action": action.map(JSONValue.string) ?? .null, "query": .string("")]))
                model.error = nil; await load(); if kind == "folder-scope" && action == "add" { path = "" }
            } catch { model.error = error.localizedDescription }
            pending = false
        }
    }
    private func choose() {
        FolderPicker.choose { selected in path = selected.first?.path ?? "" }
    }
}
