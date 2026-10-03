import SwiftUI
import NekoKit

func replacing(_ value: JSONValue, _ fields: [String: JSONValue]) -> JSONValue {
    var result = value.object
    fields.forEach { result[$0.key] = $0.value }
    return .object(result)
}
func nested(_ family: String, _ name: String, _ fields: [String: JSONValue]) -> JSONValue {
    .object([family: .command(name, fields)])
}
struct ManagementDraft: Identifiable {
    let id = UUID()
    var value: JSONValue
    var workspace: String? = nil
}
@MainActor func submit(_ model: AppModel, _ command: JSONValue) {
    Task { await model.workbench(command) }
}
@MainActor func profileFor(_ model: AppModel) -> String {
    let profiles = model.snapshot["agent_profiles"]
    if let workspace = model.selectedWorkspace {
        return profiles["assignments"].array.first { $0["workspace_id"].string == workspace }?["profile_id"].string ?? "default"
    }
    let active = profiles["active_profile_id"].string
    return active.isEmpty ? "default" : active
}

/// List rows can coalesce several controls into one accessibility row on macOS.
/// Eager stacks preserve each native button and toggle as a separate AX element.
struct ManagementScroll<Content: View>: View {
    @ViewBuilder var content: () -> Content
    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20, content: content)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(24)
        }
        .buttonStyle(.bordered)
        .accessibilityElement(children: .contain)
    }
}

struct ProfilesView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    var body: some View {
        let profiles = model.snapshot["agent_profiles"]["profiles"].array
        ManagementScroll {
            PageIntro(title: "Profiles", message: "A profile is how Neko thinks and writes for a kind of work: its instructions and its memories. Give a workspace its own profile when it needs a different voice or rules.") {
                Button("New profile", systemImage: "plus") { draft = ManagementDraft(value: .object([:])) }
            }
            ForEach(profiles, id: \.self) { profile in
                let active = model.snapshot["agent_profiles"]["active_profile_id"] == profile["id"]
                let assigned = model.snapshot["agent_profiles"]["assignments"].array.filter { $0["profile_id"] == profile["id"] }.compactMap { a in model.workspaces.first { $0.recordID == a["workspace_id"].string }?["name"].string }
                WorkspaceSection(name: profile["name"].string, color: active ? NekoStyle.accent : N.text4, detail: active ? "Default" : nil) {
                    HStack(spacing: 8) {
                        if !active { Button("Make default") { submit(model, nested("AgentProfiles", "SetActive", ["profile_id": profile["id"]])) }.controlSize(.small) }
                        Menu("Use in workspace") {
                            ForEach(model.workspaces, id: \.recordID) { workspace in
                                Button(workspace["name"].string) { submit(model, nested("AgentProfiles", "AssignWorkspace", ["workspace_id": .string(workspace.recordID), "profile_id": profile["id"]])) }
                            }
                        }.controlSize(.small).fixedSize().disabled(model.workspaces.isEmpty)
                        Button("Edit") { draft = ManagementDraft(value: profile) }.controlSize(.small)
                    }
                } content: {
                    Text(profile["instructions"].string.isEmpty ? "No special instructions." : profile["instructions"].string)
                        .font(.system(size: 12.5)).foregroundStyle(N.text3).lineLimit(3).textSelection(.enabled)
                    Text(assigned.isEmpty ? "Not used by a specific workspace." : "Used in " + assigned.joined(separator: ", "))
                        .font(.system(size: 12)).foregroundStyle(N.text4)
                    if profiles.count > 1 {
                        DisclosureGroup("Share memories from other profiles") {
                            ForEach(profiles.filter { $0["id"] != profile["id"] }, id: \.self) { source in
                                Toggle("Can read \(source["name"].string)'s general memories", isOn: Binding(get: {
                                    model.snapshot["agent_profiles"]["read_grants"].array.contains { $0["reader_id"] == profile["id"] && $0["source_id"] == source["id"] }
                                }, set: { allowed in
                                    submit(model, nested("AgentProfiles", "SetReadGrant", ["reader_id": profile["id"], "source_id": source["id"], "allowed": .bool(allowed)]))
                                }))
                            }
                            Text("This only shares memories. It never shares tools or workspace access.").font(.caption).foregroundStyle(N.text4)
                        }.font(.system(size: 12.5))
                    }
                }
            }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .profile, original: item.value, workspace: model.selectedWorkspace, profileID: profileFor(model))
        }
    }
}

struct MemoryView: View {
    @ObservedObject var model: AppModel
    @State private var tab = "memories"
    @State private var draft: ManagementDraft?
    private func memoryToggle(_ title: String, key: String, help: String) -> some View {
        let options = model.snapshot["memory_options"]
        let on = options[key] == .null ? true : options[key].bool
        return Toggle(title, isOn: Binding(get: { on }, set: { value in
            var next: [String: JSONValue] = ["learning": options["learning"] == .null ? .bool(true) : options["learning"], "use_memory": options["use_memory"] == .null ? .bool(true) : options["use_memory"]]
            next[key] = .bool(value)
            Task { await model.workbench(.command("SetMemoryOptions", ["options": .object(next)])) }
        })).toggleStyle(.switch).controlSize(.small).help(help).disabled(model.busy)
    }
    private func inScope(_ item: JSONValue) -> Bool {
        item["agent_profile_id"].string == profileFor(model) && (item["workspace_id"] == .null || model.selectedWorkspace == nil || item["workspace_id"].string == model.selectedWorkspace)
    }
    private func scopeLabel(_ item: JSONValue) -> String {
        item["workspace_id"] == .null ? "Everywhere" : (model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "Workspace")
    }
    var body: some View {
        let memories = model.snapshot["memory"].array.filter(inScope)
        let proposals = model.snapshot["memory_proposals"].array.filter(inScope)
        ManagementScroll {
            PageIntro(title: "Memory", message: "What Neko has learned about how you work: preferences, decisions and facts about your projects. Neko suggests new memories; nothing is kept until you accept it.") {
                Button("Add memory", systemImage: "plus") { draft = ManagementDraft(value: .object([:])) }
            }
            Picker("Memory view", selection: $tab) {
                Text("Saved memories").tag("memories")
                Text("Working style").tag("style")
            }.pickerStyle(.segmented).frame(maxWidth: 300)
            if tab == "style" { WorkingStyleView(model: model) } else {
            HStack(spacing: 18) {
                memoryToggle("Suggest new memories", key: "learning", help: "When off, Neko stops proposing memories from chats and tickets. Things you tell it to remember are still saved.")
                memoryToggle("Use memory in replies and tasks", key: "use_memory", help: "When off, no memory is sent to any model. Saved memories stay here.")
                Spacer()
            }.font(.system(size: 13))
            if !proposals.isEmpty {
                WorkspaceSection(name: "Suggested", color: NekoStyle.amber, detail: "\(proposals.count) waiting") { EmptyView() } content: {
                    ForEach(proposals, id: \.self) { proposal in
                        HStack(alignment: .top, spacing: 12) {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(proposal["text"].string).font(.system(size: 13)).foregroundStyle(N.text)
                                Text(proposal["source"].string).font(.system(size: 12)).foregroundStyle(N.text4)
                            }
                            Spacer()
                            Button("Keep") { submit(model, .command("DecideMemoryProposal", ["id": proposal["id"], "accept": .bool(true)])) }.controlSize(.small)
                            Button("Dismiss") { submit(model, .command("DecideMemoryProposal", ["id": proposal["id"], "accept": .bool(false)])) }.controlSize(.small)
                        }.padding(.vertical, 6).overlay(alignment: .top) { N.line.frame(height: 1) }
                    }
                }
            }
            WorkspaceSection(name: "Remembered", color: N.text4, detail: memories.isEmpty ? nil : "\(memories.count)") { EmptyView() } content: {
                if memories.isEmpty { EmptyRow(text: "Nothing yet. Tell Neko \"remember that…\" in Home, or add one here.") }
                ForEach(memories, id: \.self) { entry in
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(entry["text"].string).font(.system(size: 13)).foregroundStyle(N.text).textSelection(.enabled)
                            Text("\(entry["kind"].string.capitalized) · \(scopeLabel(entry))").font(.system(size: 12)).foregroundStyle(N.text4)
                        }
                        Spacer()
                        Menu("More") {
                            Button("Edit") { draft = ManagementDraft(value: entry) }
                            Button("Forget", role: .destructive) { submit(model, .command("DeleteMemory", ["id": entry["id"]])) }
                        }.controlSize(.small).fixedSize()
                    }.padding(.vertical, 6).overlay(alignment: .top) { N.line.frame(height: 1) }
                }
            }
        }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .memory, original: item.value, workspace: model.selectedWorkspace, profileID: profileFor(model))
        }
    }
}

/// Workspaces Neko should show in a grouped page: the selected one, or all of them.
@MainActor func scopedWorkspaces(_ model: AppModel) -> [(offset: Int, element: JSONValue)] {
    Array(model.workspaces.enumerated()).filter { model.selectedWorkspace == nil || $0.element.recordID == model.selectedWorkspace }
}

struct PageIntro<Action: View>: View {
    let title: String
    let message: String
    @ViewBuilder var action: () -> Action
    var body: some View {
        HStack(alignment: .top, spacing: 16) {
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.system(size: 20, weight: .semibold)).foregroundStyle(N.text)
                Text(message).font(.system(size: 13)).foregroundStyle(N.text3).lineLimit(4)
            }
            Spacer(minLength: 16)
            action()
        }.padding(.bottom, 4)
    }
}

struct WorkspaceSection<Trailing: View, Content: View>: View {
    let name: String
    let color: Color
    var detail: String? = nil
    @ViewBuilder var trailing: () -> Trailing
    @ViewBuilder var content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack(spacing: 8) {
                RoundedRectangle(cornerRadius: 3, style: .continuous).fill(color).frame(width: 10, height: 10)
                Text(name).font(.system(size: 13, weight: .semibold)).foregroundStyle(N.text)
                if let detail { Text(detail).font(.system(size: 12)).foregroundStyle(N.text4) }
                Spacer()
                trailing()
            }
            .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
            content()
        }
        .padding(16)
        .background(N.card.opacity(0.6), in: RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(N.line))
    }
}

struct EmptyRow: View {
    let text: String
    var body: some View { Text(text).font(.system(size: 12.5)).foregroundStyle(N.text4).padding(.vertical, 6) }
}

func relativeTime(_ ms: Int) -> String {
    ms <= 0 ? "Never" : RelativeDateTimeFormatter().localizedString(for: Date(timeIntervalSince1970: Double(ms) / 1000), relativeTo: .now)
}

struct SchedulesView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    var body: some View {
        ManagementScroll {
            PageIntro(title: "Schedules", message: "Ask Neko to prepare something at a set time, such as a morning brief or a weekly review. Every result waits for you in Work.") { EmptyView() }
            if model.workspaces.isEmpty { EmptyRow(text: "Add a workspace first.") }
            ForEach(scopedWorkspaces(model), id: \.element.recordID) { index, workspace in
                let items = model.snapshot["schedules"].array.filter { $0["workspace_id"].string == workspace.recordID }
                WorkspaceSection(name: workspace["name"].string, color: workspaceColor(index), detail: items.isEmpty ? nil : "\(items.count) scheduled") {
                    Button("New schedule", systemImage: "plus") { draft = ManagementDraft(value: .object([:]), workspace: workspace.recordID) }.controlSize(.small)
                } content: {
                    if items.isEmpty { EmptyRow(text: "No schedules here yet.") }
                    ForEach(items, id: \.self) { item in
                        HStack(alignment: .top, spacing: 12) {
                            Toggle("", isOn: Binding(get: { item["enabled"].bool }, set: { enabled in
                                submit(model, nested("Schedules", "SetEnabled", ["id": item["id"], "enabled": .bool(enabled)]))
                            })).toggleStyle(.switch).controlSize(.mini).labelsHidden().accessibilityLabel("Enabled")
                            VStack(alignment: .leading, spacing: 3) {
                                Text(item["name"].string).font(.system(size: 13, weight: .medium)).foregroundStyle(N.text)
                                Text(item["prompt"].string).font(.system(size: 12.5)).foregroundStyle(N.text3).lineLimit(2).textSelection(.enabled)
                                Text("\(ScheduleRecurrence.summary(item["rule"].string)) · \(item["timezone"].string)" + (item["next_due_ms"] == .null ? "" : " · next \(relativeTime(item["next_due_ms"].int))"))
                                    .font(.system(size: 12)).foregroundStyle(N.text4)
                            }
                            Spacer()
                            Button("Run now") { submit(model, nested("Schedules", "RunNow", ["id": item["id"]])) }.controlSize(.small)
                            Menu("More") {
                                Button("Edit") { draft = ManagementDraft(value: item, workspace: workspace.recordID) }
                                Button("Delete", role: .destructive) { submit(model, nested("Schedules", "Remove", ["id": item["id"]])) }
                            }.controlSize(.small).fixedSize()
                        }.padding(.vertical, 6).overlay(alignment: .top) { N.line.frame(height: 1) }
                    }
                }
            }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .schedule, original: item.value, workspace: item.workspace ?? model.selectedWorkspace, profileID: profileFor(model))
        }
    }
}

/// One place for every workspace: folders, connected tools, what's watched, and sync status.
struct WorkspacesView: View {
    @ObservedObject var model: AppModel
    @State private var editing = false
    var body: some View {
        ManagementScroll {
            PageIntro(title: "Workspaces", message: "Each workspace is a project: its folders, the tools it can use (for example its own Linear team) and what Neko watches for it. Everything comes together under All workspaces.") {
                Button("Add workspace", systemImage: "plus") { model.selectedWorkspace = nil; editing = true }
            }
            if model.homeWorkspaceID == nil {
                HStack(spacing: 12) {
                    Image(systemName: "house").font(.system(size: 16)).foregroundStyle(N.text3).frame(width: 24)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Add your home folder as the default workspace").font(.system(size: 13, weight: .medium)).foregroundStyle(N.text)
                        Text("Covers everything in ~ that isn't part of a specific workspace.").font(.system(size: 12)).foregroundStyle(N.text4)
                    }
                    Spacer()
                    Button("Add home folder") { Task { await model.addHomeWorkspace() } }.nekoGlassButton().controlSize(.small)
                }
                .padding(14)
                .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder(N.line, style: StrokeStyle(lineWidth: 1, dash: [4, 4])))
            }
            ForEach(Array(model.workspaces.enumerated()), id: \.element.recordID) { index, workspace in
                row(workspace, color: workspaceColor(index))
            }
        }.sheet(isPresented: $editing) { WorkspaceEditor(model: model) }
    }
    private func row(_ workspace: JSONValue, color: Color) -> some View {
        let id = workspace.recordID
        let folders = model.snapshot["workspace_folders"][id].array.map(\.string)
        let responsibilities = model.snapshot["mcp"]["responsibilities"].array.filter { $0["workspace_id"].string == id }
        let tools = model.snapshot["mcp"]["connections"].array.filter { $0["workspace_id"].string.isEmpty || $0["workspace_id"].string == id }.map { $0["label"].string }
        let lastChecked = responsibilities.map { $0["last_attempt_ms"].int }.max() ?? 0
        let failing = responsibilities.contains { $0["failures"].int > 0 }
        let tickets = model.tasks.filter { $0["workspace_id"].string == id && !["Completed", "Cancelled"].contains($0["status"].string) }.count
        let isHome = id == model.homeWorkspaceID
        let detail = [isHome ? "Default · home folder" : nil, tickets == 0 ? nil : "\(tickets) open"].compactMap { $0 }.joined(separator: " · ")
        return WorkspaceSection(name: workspace["name"].string, color: color, detail: detail.isEmpty ? nil : detail) {
            HStack(spacing: 8) {
                Button("Sync now", systemImage: "arrow.clockwise") {
                    responsibilities.forEach { submit(model, nested("Mcp", "Wake", ["responsibility_id": $0["id"]])) }
                }.controlSize(.small).disabled(responsibilities.isEmpty).help(responsibilities.isEmpty ? "Add something to watch first" : "Check everything this workspace watches")
                Button("Settings") { model.selectedWorkspace = id; editing = true }.controlSize(.small)
            }
        } content: {
            Grid(alignment: .leadingFirstTextBaseline, horizontalSpacing: 16, verticalSpacing: 6) {
                fact("Folders", folders.isEmpty ? "None" : folders.map { ($0 as NSString).abbreviatingWithTildeInPath }.joined(separator: ", "))
                fact("Tools", tools.isEmpty ? "None connected" : tools.joined(separator: ", "))
                fact("Watching", responsibilities.isEmpty ? "Nothing yet" : "\(responsibilities.count) \(responsibilities.count == 1 ? "item" : "items")")
                fact("Last sync", responsibilities.isEmpty ? "—" : relativeTime(lastChecked) + (failing ? " · needs attention" : ""), warn: failing)
            }
        }
    }
    private func fact(_ label: String, _ value: String, warn: Bool = false) -> some View {
        GridRow {
            Text(label).font(.system(size: 12)).foregroundStyle(N.text4).gridColumnAlignment(.trailing)
            Text(value).font(.system(size: 12.5)).foregroundStyle(warn ? NekoStyle.coral : N.text2).lineLimit(2).textSelection(.enabled)
        }
    }
}

enum ScheduleRecurrence: String, CaseIterable {
    case daily = "Daily", weekdays = "Weekdays", weekly = "Weekly", hourly = "Every N hours", custom = "Custom"
    static let days = [("MO", "Monday"), ("TU", "Tuesday"), ("WE", "Wednesday"), ("TH", "Thursday"), ("FR", "Friday"), ("SA", "Saturday"), ("SU", "Sunday")]
    func rule(hour: Int, minute: Int, weekday: String, interval: Int, custom: String) -> String {
        switch self {
        case .daily: return "FREQ=DAILY;BYHOUR=\(hour);BYMINUTE=\(minute)"
        case .weekdays: return "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=\(hour);BYMINUTE=\(minute)"
        case .weekly: return "FREQ=WEEKLY;BYDAY=\(weekday);BYHOUR=\(hour);BYMINUTE=\(minute)"
        case .hourly: return "FREQ=HOURLY;INTERVAL=\(interval)"
        case .custom: return custom
        }
    }
    static func summary(_ rule: String) -> String {
        let pieces = rule.split(separator: ";").map { $0.split(separator: "=", maxSplits: 1).map(String.init) }
        guard pieces.allSatisfy({ $0.count == 2 }), Set(pieces.map { $0[0] }).count == pieces.count else { return "Custom schedule" }
        let fields = Dictionary(uniqueKeysWithValues: pieces.map { ($0[0], $0[1]) })
        if fields["FREQ"] == "HOURLY", fields.count == 2, let n = fields["INTERVAL"], Int(n) != nil { return "Every \(n) hours" }
        guard let hour = fields["BYHOUR"].flatMap(Int.init), let minute = fields["BYMINUTE"].flatMap(Int.init), (0...23).contains(hour), (0...59).contains(minute) else { return "Custom schedule" }
        let time = String(format: "%02d:%02d", hour, minute)
        if fields["FREQ"] == "DAILY", fields.count == 3 { return "Daily at \(time)" }
        if fields["FREQ"] == "WEEKLY", fields.count == 4 {
            if fields["BYDAY"] == "MO,TU,WE,TH,FR" { return "Weekdays at \(time)" }
            if let day = days.first(where: { $0.0 == fields["BYDAY"] }) { return "Every \(day.1) at \(time)" }
        }
        return "Custom schedule"
    }
}

enum ManagementKind: String {
    case profile = "Profile", memory = "Memory", responsibility = "Responsibility", schedule = "Schedule"
    func hasRequiredContent(name: String, text: String) -> Bool {
        let hasName = !name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        let hasText = !text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        switch self {
        case .profile: return hasName
        case .schedule: return hasName && hasText
        case .memory, .responsibility: return hasText
        }
    }
}
struct ManagementEditor: View {
    @ObservedObject var model: AppModel
    let kind: ManagementKind
    let original: JSONValue
    let workspace: String?
    let profileID: String
    @Environment(\.dismiss) private var dismiss
    @State private var name = ""
    @State private var text = ""
    @State private var memoryKind = "profile"
    @State private var rule = "FREQ=DAILY;BYHOUR=9;BYMINUTE=0"
    @State private var recurrence = ScheduleRecurrence.daily
    @State private var time = Calendar.current.date(from: DateComponents(year: 2001, month: 1, day: 1, hour: 9)) ?? Date()
    @State private var weekday = "MO"
    @State private var interval = 4
    @State private var timezone = TimeZone.current.identifier
    @State private var connections = Set<String>()
    @State private var prepare = false
    @State private var enabled = false
    @State private var saving = false
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("\(original["id"].string.isEmpty ? "New" : "Edit") \(kind.rawValue)").font(.title2)
            Form {
                if kind == .profile || kind == .schedule { TextField("Name", text: $name) }
                if kind == .memory {
                    Picker("Kind", selection: $memoryKind) {
                        Text("Profile").tag("profile")
                        Text("Workspace").tag("workspace")
                        Text("Decision").tag("decision")
                    }.disabled(!original["id"].string.isEmpty)
                }
                TextEditor(text: $text).frame(minHeight: 120).accessibilityLabel(kind == .profile ? "Instructions" : "Content")
                if kind == .schedule {
                    Picker("Repeat", selection: $recurrence) { ForEach(ScheduleRecurrence.allCases, id: \.self) { Text($0.rawValue).tag($0) } }
                    if recurrence == .hourly { Stepper("Every \(interval) hours", value: $interval, in: 1...168) }
                    else if recurrence != .custom {
                        if recurrence == .weekly { Picker("Day", selection: $weekday) { ForEach(ScheduleRecurrence.days, id: \.0) { Text($0.1).tag($0.0) } } }
                        DatePicker("At", selection: $time, displayedComponents: .hourAndMinute)
                    }
                    Text("Time zone: \(timezone)").font(.caption).foregroundStyle(.secondary)
                    DisclosureGroup("Advanced") {
                        TextField("Time zone", text: $timezone)
                        if recurrence == .custom {
                            TextField("Custom recurrence rule", text: $rule)
                            Text("The saved custom rule is preserved unless you edit it or choose a different repeat option.").font(.caption)
                        }
                    }
                    Text("Saving pauses this schedule. Enable it after reviewing the saved settings.").font(.caption)
                }
                if kind == .responsibility {
                    Toggle("Enabled", isOn: $enabled)
                    if enabled && !missingToolAccess.isEmpty {
                        Label("Choose access for \(missingToolAccess.joined(separator: ", ")) in Tools & skills before turning this on.", systemImage: "exclamationmark.circle")
                            .font(.caption).foregroundStyle(NekoStyle.amber)
                    }
                    Toggle("Allow preparation of low-risk local fixes", isOn: $prepare)
                    Text("Remote tool permission and publication authority are separate.").font(.caption)
                    ForEach(model.snapshot["mcp"]["connections"].array.filter { $0["workspace_id"].string.isEmpty || $0["workspace_id"].string == effectiveWorkspace }, id: \.self) { connection in
                        Toggle(connection["label"].string, isOn: Binding(get: { connections.contains(connection["id"].string) }, set: { selected in
                            if selected { connections.insert(connection["id"].string) } else { connections.remove(connection["id"].string) }
                        }))
                    }
                }
            }
            if let error = model.error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
            HStack {
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Spacer()
                Button(saving ? "Saving…" : "Save") { save() }.keyboardShortcut(.defaultAction).disabled(saving || !kind.hasRequiredContent(name: name, text: text) || (kind == .memory && memoryKind == "workspace" && effectiveWorkspace == nil) || (kind == .responsibility && (connections.isEmpty || (enabled && !missingToolAccess.isEmpty))))
            }
        }.padding(24).frame(width: 540).onAppear {
            name = original["name"].string
            text = original[kind == .profile ? "instructions" : kind == .schedule ? "prompt" : kind == .responsibility ? "instruction" : "text"].string
            if !original["kind"].string.isEmpty { memoryKind = original["kind"].string }
            if !original["rule"].string.isEmpty { rule = original["rule"].string; recurrence = .custom }
            if !original["timezone"].string.isEmpty { timezone = original["timezone"].string }
            connections = Set(original["connection_ids"].array.map(\.string))
            enabled = original["enabled"].bool
            prepare = original["prepare_low_risk"].bool
        }
    }
    private var effectiveWorkspace: String? { original["id"].string.isEmpty ? workspace : (original["workspace_id"] == .null ? nil : original["workspace_id"].string) }
    private var missingToolAccess: [String] {
        Watching.ungranted(model, .object(["workspace_id": .string(effectiveWorkspace ?? ""), "connection_ids": .array(connections.sorted().map(JSONValue.string))]))
    }
    private func save() {
        let command: JSONValue
        let now = JSONValue.number((Date().timeIntervalSince1970 * 1000).rounded(.down))
        switch kind {
        case .profile:
            command = nested("AgentProfiles", "Save", ["profile": .object(["id": .string(original["id"].string), "name": .string(name), "instructions": .string(text)])])
        case .memory:
            let entry = replacing(original, ["id": .string(original["id"].string), "agent_profile_id": original["id"].string.isEmpty ? .string(profileID) : original["agent_profile_id"], "kind": .string(memoryKind), "workspace_id": memoryKind == "profile" ? .null : effectiveWorkspace.map(JSONValue.string) ?? .null, "text": .string(text), "source": original["id"].string.isEmpty ? .string("user") : original["source"], "created_at_ms": original["id"].string.isEmpty ? now : original["created_at_ms"], "updated_at_ms": now])
            command = .command("SaveMemory", ["entry": entry])
        case .schedule:
            let components = Calendar.current.dateComponents([.hour, .minute], from: time)
            let savedRule = recurrence.rule(hour: components.hour ?? 9, minute: components.minute ?? 0, weekday: weekday, interval: interval, custom: rule)
            command = nested("Schedules", "Save", ["schedule": replacing(original, ["id": .string(original["id"].string), "name": .string(name), "prompt": .string(text), "workspace_id": effectiveWorkspace.map(JSONValue.string) ?? .null, "rule": .string(savedRule), "timezone": .string(timezone), "anchor_ms": original["id"].string.isEmpty ? now : original["anchor_ms"], "enabled": .bool(false)])])
        case .responsibility:
            command = nested("Mcp", "SaveResponsibility", ["responsibility": replacing(original, ["id": .string(original["id"].string), "workspace_id": .string(effectiveWorkspace ?? ""), "instruction": .string(text), "connection_ids": .array(connections.sorted().map(JSONValue.string)), "enabled": .bool(enabled), "prepare_low_risk": .bool(prepare), "next_due_ms": original["id"].string.isEmpty ? now : original["next_due_ms"], "last_attempt_ms": original["last_attempt_ms"], "last_result": .string(original["last_result"].string), "failures": .number(Double(original["failures"].int))])])
        }
        saving = true
        Task {
            let saved = await model.workbench(command)
            saving = false
            if saved { dismiss() }
        }
    }
}
