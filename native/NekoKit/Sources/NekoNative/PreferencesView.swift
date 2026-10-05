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
        combo == .null ? "Loading…" : Self.label(combo)
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
                    try await apply(candidate, previous: previous)
                    model.error = nil
                } catch {
                    // Taken by the system or another app: use the next free chord and say so.
                    var replaced = false
                    for fallback in HotkeyFallbacks.candidates(after: candidate) where fallback != previous {
                        if (try? await apply(fallback, previous: previous)) != nil {
                            model.error = nil
                            model.notice = "\(Self.label(candidate)) is already in use, so Neko uses \(Self.label(fallback)) instead. Press another chord to change it."
                            replaced = true
                            break
                        }
                    }
                    if !replaced { model.error = error.localizedDescription }
                }
                pending = false
            }
            return nil
        }
    }
    /// Checks known conflicts, registers live, then persists; restores the old chord on failure.
    private func apply(_ candidate: JSONValue, previous: JSONValue) async throws {
        let conflict = try await model.request(.command("CheckHotkeyConflict", ["candidate": candidate]))
        if conflict["HotkeyConflict"]["reason"] != .null { throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: conflict["HotkeyConflict"]["reason"].string]) }
        guard let register = NativeHotkeySettings.register else { throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "Shortcut registration is not available yet."]) }
        try register(candidate)
        do {
            _ = try await model.request(.command("CommitHotkey", ["candidate": candidate]))
        } catch { try? register(previous); throw error }
        combo = candidate
    }
    static func label(_ combo: JSONValue) -> String {
        let symbols = ["Cmd": "⌘", "Alt": "⌥", "Ctrl": "⌃", "Shift": "⇧"]
        return combo["modifiers"].array.map { symbols[$0.string] ?? $0.string }.joined() + combo["key"].string.replacingOccurrences(of: "Key", with: "")
    }
}

/// Chords tried, in order, when the one pressed is taken.
enum HotkeyFallbacks {
    static func candidates(after pressed: JSONValue) -> [JSONValue] {
        let key = pressed["key"].string.isEmpty ? "Space" : pressed["key"].string
        let sets: [[String]] = [["Alt"], ["Cmd", "Shift"], ["Ctrl", "Alt"], ["Alt", "Shift"], ["Ctrl", "Shift"], ["Ctrl"]]
        var out: [JSONValue] = []
        for keyName in [key, "Space"] {
            for modifiers in sets {
                let candidate: JSONValue = .object(["modifiers": .array(modifiers.map(JSONValue.string)), "key": .string(keyName)])
                if candidate != pressed, !out.contains(candidate) { out.append(candidate) }
            }
        }
        return out
    }
}

struct PreferencesView: View {
    @AppStorage("neko.settings.tab") private var settingsTab = "General"
    @AppStorage("nekoAppearance") private var appearance = NekoAppearance.system.rawValue
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
    @State private var budget = ""
    @State private var presence = PresenceController.enabled
    @State private var checkingUpdates = false
    @State private var updateStatus: Updates.Status?
    @State private var updateNote: String?
    private var tabDescription: String {
        switch settingsTab {
        case "AI": "Choose the runtime and model for new work."
        case "Search": "Choose which folders appear in the quick panel."
        case "Permissions": "Review what Neko can access on this Mac."
        case "Diagnostics": "Check local runtimes and the connection to Neko."
        case "Agents": "Manage the optional legacy agent directory."
        case "About": "Your workspace, native on your Mac."
        default: "Make Neko fit the way you use your Mac."
        }
    }
    private func checkUpdates() {
        checkingUpdates = true
        Task { updateStatus = await Updates.check(); checkingUpdates = false }
    }
    private func runUpdate() {
        switch Updates.update() {
        case .success(let log): updateNote = "Updating in the background. Neko will quit, rebuild and reopen. Log: \(log.path)"
        case .failure(let reason): updateNote = reason.message
        }
    }
    private enum BudgetValue: Equatable { case none, cents(Int), invalid }
    private var budgetCents: BudgetValue {
        let text = budget.trimmingCharacters(in: .whitespaces).replacingOccurrences(of: "$", with: "")
        if text.isEmpty { return .none }
        guard let dollars = Double(text), dollars >= 0.01, dollars <= 1000 else { return .invalid }
        return .cents(Int((dollars * 100).rounded()))
    }
    private func saveBudget() {
        let value: JSONValue
        switch budgetCents { case .none: value = .null; case .cents(let c): value = .number(Double(c)); case .invalid: return }
        Task { _ = await model.workbench(.command("SetTaskBudget", ["cents": value])) }
    }
    private var sectionPicker: some View {
        Picker("Settings section", selection: $settingsTab) {
            ForEach(["General", "AI", "Search", "Permissions", "Diagnostics", "Agents", "About"], id: \.self) {
                Text($0).tag($0)
            }
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
            ViewThatFits(in: .horizontal) {
                sectionPicker.pickerStyle(.segmented).labelsHidden().fixedSize(horizontal: true, vertical: false)
                sectionPicker.pickerStyle(.menu).fixedSize()
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            VStack(alignment: .leading, spacing: 10) {
                Text(settingsTab).font(NekoFont.display).foregroundStyle(N.text)
                Text(tabDescription).font(NekoFont.body).foregroundStyle(N.text3)
                    .fixedSize(horizontal: false, vertical: true)
            }.accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
            Group {
                switch settingsTab {
                case "General":
                Form {
                    Section {
                        Picker("Appearance", selection: $appearance) {
                            ForEach(NekoAppearance.allCases) { option in
                                Text(option.title).tag(option.rawValue)
                            }
                        }.pickerStyle(.segmented)
                    } header: { Text("Appearance") } footer: {
                        Text("System follows your Mac’s light or dark appearance.")
                    }
                    Section("Quick access") {
                        HotkeySettingsView(model: model)
                        preferenceToggle("Launch at login", "launch-at-login")
                        Toggle(isOn: Binding(get: { presence }, set: { presence = $0; PresenceController.enabled = $0; PresenceController.shared.refresh() })) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("Show activity while Neko works")
                                Text("A small status in the corner. Click it to return to Neko.")
                                    .font(NekoFont.meta).foregroundStyle(.secondary)
                            }
                        }
                        .accessibilityLabel("Show activity while Neko works")
                        .accessibilityHint("A small status in the corner. Click it to return to Neko.")
                    }
                    Section("Clipboard") {
                        Toggle(isOn: Binding(get: { clipboard.enabled ?? false }, set: { value in setClipboard(value) })) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text("Save clipboard history")
                                Text("Stored on this Mac. Turning it off stops capture and keeps existing history.")
                                    .font(NekoFont.meta).foregroundStyle(.secondary)
                            }
                        }
                        .accessibilityLabel("Save clipboard history")
                        .accessibilityHint("Stored on this Mac. Turning it off stops capture and keeps existing history.")
                        .disabled(clipboard.enabled == nil)
                        if clipboard.enabled == nil { Button("Read clipboard setting") { Task { await loadClipboard() } } }
                    }
                }.formStyle(.grouped)
                case "AI":

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
                        if catalog.sources.isEmpty {
                            HStack(spacing: 8) { ProgressView().controlSize(.small); Text("Reading models from your runtimes…").foregroundStyle(.secondary) }
                        }
                        if let source = catalog.source(agentProvider) {
                            LabeledContent("Connection") {
                                Label(source.connection, systemImage: source.ready ? "checkmark.circle" : "exclamationmark.circle")
                                    .foregroundStyle(source.ready ? Color.secondary : NekoStyle.amber)
                            }
                            if let note = source.note { Text(note).font(NekoFont.meta).foregroundStyle(.secondary) }
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
                                if let reason = selected.reason ?? selected.description { Text(reason).font(NekoFont.meta).foregroundStyle(.secondary) }
                            }
                        } else if agentProvider == "opencodex" {
                            Text("This model runs through an external proxy. Choose Claude Code or OpenCode above to run Claude and other models with their own logins instead.")
                                .font(NekoFont.meta).foregroundStyle(.secondary)
                        }
                        DisclosureGroup("Use an exact model ID") {
                        HStack {
                            TextField("Or enter an exact model ID", text: $customModel)
                                .textFieldStyle(.roundedBorder)
                            Button("Use ID") { agentModel = customModel.trimmingCharacters(in: .whitespacesAndNewlines) }
                                .disabled(!validModelID(customModel.trimmingCharacters(in: .whitespacesAndNewlines)))
                        }
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
                                .font(NekoFont.body)
                        }
                        Text("Checking sends one short reply through the same runner tasks use, so it may use quota. A failed check keeps your current model. New conversations, tasks and background checks use the saved model; work already running is not interrupted.")
                            .font(NekoFont.meta).foregroundStyle(.secondary)
                    }
                    Section("Budget") {
                        LabeledContent("Stop a ticket after") {
                            HStack {
                                TextField("Budget", text: $budget, prompt: Text("No limit")).labelsHidden().frame(width: 90).textFieldStyle(.roundedBorder)
                                Text("US$").foregroundStyle(.secondary)
                                Spacer()
                                Button("Save budget") { saveBudget() }.disabled(pending || budgetCents == .invalid)
                            }
                        }
                        if budgetCents == .invalid { Text("Enter an amount between 0.01 and 1000, or leave it empty.").font(NekoFont.meta).foregroundStyle(NekoStyle.amber) }
                        Text("Counts the cost Claude Code and OpenCode report for each ticket and stops the ticket when it passes this amount. Codex subscriptions don’t report a price, so they aren’t limited here. Leave empty for no limit.")
                            .font(NekoFont.meta).foregroundStyle(.secondary)
                    }
                }.formStyle(.grouped)
                case "Search":

                Form {
                    Section("Included folders") {
                        if folders.isEmpty {
                            Label("No folders added", systemImage: "folder")
                                .foregroundStyle(.secondary).padding(.vertical, 8)
                        }
                        ForEach(folders, id: \.self) { folder in
                            HStack(spacing: 12) {
                                Image(systemName: "folder").foregroundStyle(.secondary).accessibilityHidden(true)
                                Text(folder["title"].string).textSelection(.enabled)
                                Spacer()
                                Button("Remove") { activate("folder-scope", folder["id"].string) }
                                    .accessibilityLabel("Remove \(folder["title"].string) from search")
                            }.padding(.vertical, 4)
                        }
                    }
                    Section("Add a folder") {
                        TextField("Folder path", text: $path, prompt: Text("/Users/you/Documents"))
                            .textFieldStyle(.roundedBorder)
                        HStack {
                            Button("Choose folder…") { choose() }
                            Spacer()
                            Button("Add folder") { activate("folder-scope", path, action: "add") }
                                .buttonStyle(.borderedProminent)
                                .disabled(path.trimmingCharacters(in: .whitespaces).isEmpty)
                        }
                    }
                }.formStyle(.grouped)
                case "Permissions":

                PermissionsView(model: model)
                case "Diagnostics":

                DiagnosticsView(model: model)
                case "Agents":

                Form {
                    Section("Legacy providers") {
                    Text(agents.isEmpty ? "No agent provider is turned on." : "\(agents.count) agents visible from the configured provider.")
                    preferenceToggle("Show agents", "agents-enabled")
                    preferenceToggle("Include idle agents", "agents-include-idle")
                    Text("Legacy agent providers are opt-in. These settings do not enable a provider.").font(NekoFont.meta).foregroundStyle(.secondary)
                    }
                }.formStyle(.grouped)
                case "About":

                VStack(spacing: 16) {
                    BrandMark(size: 72)
                    Text("Neko").font(NekoFont.display)
                    Text("Version \(Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String ?? "Development")")
                    Text("Your workspace, native on your Mac.").foregroundStyle(.secondary)
                    if let build = Updates.Build.current { Text("Build \(String(build.commit.prefix(7)))").font(NekoFont.mono).foregroundStyle(.secondary) }
                    HStack {
                        Button(checkingUpdates ? "Checking…" : "Check for updates") { checkUpdates() }.disabled(checkingUpdates)
                        if case .behind = updateStatus { Button("Update now") { runUpdate() }.buttonStyle(.borderedProminent) }
                    }.padding(.top, 8)
                    if let updateStatus { Text(updateStatus.message).font(NekoFont.body).foregroundStyle(.secondary).multilineTextAlignment(.center) }
                    if let updateNote { Text(updateNote).font(NekoFont.meta).foregroundStyle(.secondary).multilineTextAlignment(.center).textSelection(.enabled) }
                }.frame(maxWidth: .infinity).padding(.top, 28)
                default: EmptyView()
                }
            }
            .scrollContentBackground(.hidden)
            .contentMargins(.horizontal, 0, for: .scrollContent)
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
            .disabled(pending)
        }
        .font(NekoFont.body)
        .controlSize(.regular)
        .frame(maxWidth: NekoLayout.pageWidth, maxHeight: .infinity)
        .padding(NekoLayout.pageInset)
        .navigationTitle("Settings")
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
        let cents = model.snapshot["task_budget_cents"]
        budget = cents == .null ? "" : String(format: "%.2f", Double(cents.int) / 100)
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
