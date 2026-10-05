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
            VStack(alignment: .leading, spacing: NekoLayout.sectionGap, content: content)
                .frame(maxWidth: NekoLayout.pageWidth, alignment: .leading)
                .padding(NekoLayout.pageInset)
                .frame(maxWidth: .infinity, alignment: .top)
        }
        .font(NekoFont.body)
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
            PageIntro(title: "Profiles", message: "Instructions and memories for each kind of work.") {
                Button("New profile", systemImage: "plus") { draft = ManagementDraft(value: .object([:])) }
            }
            ForEach(profiles, id: \.recordID) { profile in
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
                    ManagementDetailText(text: profile["instructions"].string.isEmpty ? "No special instructions. Edit this profile to add a voice or working rules." : profile["instructions"].string, disclosure: "Full instructions")
                    Text(assigned.isEmpty ? "Not used by a specific workspace." : "Used in " + assigned.joined(separator: ", "))
                        .font(NekoFont.meta).foregroundStyle(N.text3)
                    if profiles.count > 1 {
                        DisclosureGroup("Share memories from other profiles") {
                            ForEach(profiles.filter { $0["id"] != profile["id"] }, id: \.self) { source in
                                Toggle("Can read \(source["name"].string)'s general memories", isOn: Binding(get: {
                                    model.snapshot["agent_profiles"]["read_grants"].array.contains { $0["reader_id"] == profile["id"] && $0["source_id"] == source["id"] }
                                }, set: { allowed in
                                    submit(model, nested("AgentProfiles", "SetReadGrant", ["reader_id": profile["id"], "source_id": source["id"], "allowed": .bool(allowed)]))
                                }))
                            }
                            Text("This only shares memories. It never shares tools or workspace access.").font(NekoFont.meta).foregroundStyle(N.text3)
                        }.font(NekoFont.body).toggleStyle(.checkbox)
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
        return Toggle(isOn: Binding(get: { on }, set: { value in
            var next: [String: JSONValue] = ["learning": options["learning"] == .null ? .bool(true) : options["learning"], "use_memory": options["use_memory"] == .null ? .bool(true) : options["use_memory"]]
            next[key] = .bool(value)
            Task { await model.workbench(.command("SetMemoryOptions", ["options": .object(next)])) }
        })) {
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(NekoFont.body).foregroundStyle(N.text)
                Text(help).font(NekoFont.meta).foregroundStyle(N.text3)
                    .fixedSize(horizontal: false, vertical: true)
            }.frame(maxWidth: .infinity, alignment: .leading)
        }.toggleStyle(.switch).controlSize(.small).disabled(model.busy)
            .accessibilityLabel(title).accessibilityHint(help)
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
            PageIntro(title: "Memory", message: "Saved context and working guidance. Suggestions wait for your approval.") {
                if tab == "memories" { Button("Add memory", systemImage: "plus") { draft = ManagementDraft(value: .object([:])) } }
            }
            Picker("Memory view", selection: $tab) {
                Text("Saved memories").tag("memories")
                Text("Working style").tag("style")
            }.pickerStyle(.segmented).labelsHidden().frame(maxWidth: 300)
            if tab == "style" { WorkingStyleView(model: model) } else {
            WorkspaceSection(name: "Memory settings", color: N.text4) { EmptyView() } content: {
                memoryToggle("Suggest new memories", key: "learning", help: "Propose context from chats and agents. Direct requests to remember something still work when this is off.")
                Divider()
                memoryToggle("Use saved memory", key: "use_memory", help: "Include memory in replies and tasks. Turning this off keeps saved memories here.")
            }
            if !proposals.isEmpty {
                WorkspaceSection(name: "Suggested", color: NekoStyle.amber, detail: "\(proposals.count) waiting") { EmptyView() } content: {
                    ForEach(proposals, id: \.recordID) { proposal in
                        VStack(alignment: .leading, spacing: 10) {
                            ManagementDetailText(text: proposal["text"].string, disclosure: "Full suggestion")
                            HStack(spacing: 8) {
                                Text(scopeLabel(proposal)).font(NekoFont.meta).foregroundStyle(N.text3)
                                Spacer(minLength: 8)
                                Button("Keep memory") { submit(model, .command("DecideMemoryProposal", ["id": proposal["id"], "accept": .bool(true)])) }
                                Button("Dismiss") { submit(model, .command("DecideMemoryProposal", ["id": proposal["id"], "accept": .bool(false)])) }
                            }
                            MemorySourceDetails(model: model, source: proposal["source"].string)
                        }.controlSize(.small).padding(.vertical, 4)
                        if proposal != proposals.last { Divider() }
                    }
                }
            }
            WorkspaceSection(name: "Remembered", color: N.text4, detail: memories.isEmpty ? nil : "\(memories.count)") { EmptyView() } content: {
                if memories.isEmpty { EmptyRow(text: "Nothing yet. Tell Neko \"remember that…\" in Home, or add one here.") }
                ForEach(memories, id: \.recordID) { entry in
                    VStack(alignment: .leading, spacing: 10) {
                        ManagementDetailText(text: entry["text"].string, disclosure: "Full memory")
                        HStack(spacing: 8) {
                            Text("\(entry["kind"].string.capitalized) · \(scopeLabel(entry))").font(NekoFont.meta).foregroundStyle(N.text3)
                            Spacer(minLength: 8)
                            Button("Edit") { draft = ManagementDraft(value: entry) }
                            Menu("Memory actions", systemImage: "ellipsis") {
                                Button("Forget memory", role: .destructive) { submit(model, .command("DeleteMemory", ["id": entry["id"]])) }
                            }.labelStyle(.iconOnly).fixedSize()
                        }
                        MemorySourceDetails(model: model, source: entry["source"].string)
                    }.controlSize(.small).padding(.vertical, 4)
                    if entry != memories.last { Divider() }
                }
            }
        }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .memory, original: item.value, workspace: model.selectedWorkspace, profileID: profileFor(model))
        }
    }
}

private struct MemorySourceDetails: View {
    @ObservedObject var model: AppModel
    let source: String
    private var task: JSONValue? {
        guard source.hasPrefix("ticket:") else { return nil }
        let id = source.dropFirst("ticket:".count).split(separator: ":").first.map(String.init) ?? ""
        return model.tasks.first { $0.recordID == id }
    }
    private var label: String {
        if source == "user" { return "Added by you" }
        if source == "chat" || source.hasPrefix("chat:") { return "From a Home conversation" }
        if source.hasPrefix("ticket:") { return task.map { "From agent: " + $0["title"].string } ?? "From an agent conversation" }
        return "Source details"
    }
    var body: some View {
        if !source.isEmpty {
            DisclosureGroup(label) {
                VStack(alignment: .leading, spacing: 8) {
                    Text(source).font(NekoFont.meta.monospaced()).textSelection(.enabled)
                    if let task { Button("Open source agent") { model.openAgent(task.recordID, from: "Memory") } }
                }.padding(.top, 6)
            }.font(NekoFont.meta).foregroundStyle(N.text3)
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
        VStack(alignment: .leading, spacing: 6) {
            Text(title).font(NekoFont.title).foregroundStyle(N.text).accessibilityAddTraits(.isHeader)
            Text(message).font(NekoFont.body).foregroundStyle(N.text3)
                .fixedSize(horizontal: false, vertical: true)
        }
            .frame(maxWidth: NekoLayout.readingWidth, alignment: .leading)
            .navigationTitle(title)
            .toolbar { ToolbarItemGroup(placement: .primaryAction) { action() } }
    }
}

struct WorkspaceSection<Trailing: View, Content: View>: View {
    let name: String
    let color: Color
    var detail: String? = nil
    @ViewBuilder var trailing: () -> Trailing
    @ViewBuilder var content: () -> Content
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ViewThatFits(in: .horizontal) {
                HStack(spacing: 12) {
                    heading.fixedSize(horizontal: true, vertical: false)
                    Spacer(minLength: 12)
                    trailing().fixedSize()
                }
                VStack(alignment: .leading, spacing: 10) {
                    heading
                    trailing()
                }
            }
            .controlSize(.small)
            VStack(alignment: .leading, spacing: 12, content: content)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(NekoLayout.rowInset)
                .background(N.card, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(N.line))
        }
        .accessibilityElement(children: .contain)
    }
    private var heading: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            Circle().fill(color).frame(width: 6, height: 6).accessibilityHidden(true)
            Text(name).font(NekoFont.heading).foregroundStyle(N.text)
            if let detail { Text(detail).font(NekoFont.meta).foregroundStyle(N.text3) }
        }
        .accessibilityElement(children: .combine).accessibilityAddTraits(.isHeader)
    }
}

struct EmptyRow: View {
    let text: String
    var body: some View {
        Text(text).font(NekoFont.body).foregroundStyle(N.text3)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading).padding(.vertical, 4)
    }
}

/// Saved instructions and evidence are inert text, never executable chat blocks.
/// Verbatim rendering also preserves file references whose targets no longer exist.
struct ManagementSavedText: View {
    let text: String
    var body: some View {
        Text(verbatim: text)
            .font(NekoFont.body).textSelection(.enabled).lineLimit(nil)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// A brief readback with the complete saved content available on demand.
struct ManagementDetailText: View {
    let text: String
    var disclosure = "Read more"
    @State private var expanded = false
    private var needsDisclosure: Bool { text.count > 240 || text.components(separatedBy: .newlines).count > 3 }
    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            if needsDisclosure {
                if !expanded {
                    Text(verbatim: String(text.prefix(240)))
                        .lineLimit(2).foregroundStyle(N.text2)
                }
                DisclosureGroup(disclosure, isExpanded: $expanded) {
                    ManagementSavedText(text: text).padding(.top, 6)
                }.font(NekoFont.meta)
            } else {
                ManagementSavedText(text: text)
            }
        }.font(NekoFont.body).textSelection(.enabled)
    }
}

func relativeTime(_ ms: Int) -> String {
    ms <= 0 ? "Never" : RelativeDateTimeFormatter().localizedString(for: Date(timeIntervalSince1970: Double(ms) / 1000), relativeTo: .now)
}

struct SchedulesView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    var body: some View {
        ManagementScroll {
            PageIntro(title: "Schedules", message: "Recurring briefs and reviews, delivered to All agents.") { EmptyView() }
            if model.workspaces.isEmpty { EmptyRow(text: "Add a workspace in Workspaces, then create its first schedule here.") }
            ForEach(scopedWorkspaces(model), id: \.element.recordID) { index, workspace in
                let items = model.snapshot["schedules"].array.filter { $0["workspace_id"].string == workspace.recordID }
                WorkspaceSection(name: workspace["name"].string, color: workspaceColor(index), detail: items.isEmpty ? nil : "\(items.count) scheduled") {
                    Button("New schedule", systemImage: "plus") { draft = ManagementDraft(value: .object([:]), workspace: workspace.recordID) }.controlSize(.small)
                } content: {
                    if items.isEmpty { EmptyRow(text: "Create a schedule for a morning brief, a weekly review, or another recurring task.") }
                    ForEach(items, id: \.recordID) { item in
                        VStack(alignment: .leading, spacing: 10) {
                            Toggle(isOn: Binding(get: { item["enabled"].bool }, set: { enabled in
                                submit(model, nested("Schedules", "SetEnabled", ["id": item["id"], "enabled": .bool(enabled)]))
                            })) {
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(item["name"].string).font(NekoFont.heading).foregroundStyle(N.text)
                                    Text(item["enabled"].bool ? "Enabled" : "Paused").font(NekoFont.meta).foregroundStyle(N.text3)
                                }
                            }.toggleStyle(.switch).controlSize(.small)
                            Text("\(ScheduleRecurrence.summary(item["rule"].string)) · \(item["timezone"].string)" + (item["next_due_ms"] == .null ? "" : " · next \(relativeTime(item["next_due_ms"].int))"))
                                .font(NekoFont.meta).foregroundStyle(N.text3)
                            ManagementDetailText(text: item["prompt"].string, disclosure: "Full instructions")
                            HStack(spacing: 8) {
                                Button("Run now") { submit(model, nested("Schedules", "RunNow", ["id": item["id"]])) }
                                Button("Edit schedule") { draft = ManagementDraft(value: item, workspace: workspace.recordID) }
                                Spacer()
                                Menu("Schedule actions", systemImage: "ellipsis") {
                                    Button("Delete schedule", role: .destructive) { submit(model, nested("Schedules", "Remove", ["id": item["id"]])) }
                                }.labelStyle(.iconOnly).fixedSize()
                            }
                        }.controlSize(.small).padding(.vertical, 4)
                        if item != items.last { Divider() }
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
            PageIntro(title: "Workspaces", message: "Project folders, connected tools, and the work Neko watches.") {
                Button("Add workspace", systemImage: "plus") { model.selectedWorkspace = nil; editing = true }
            }
            if model.homeWorkspaceID == nil {
                HStack(spacing: 12) {
                    Image(systemName: "house").font(.system(size: 16)).foregroundStyle(N.text3).frame(width: 24)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("A workspace for everyday work").font(NekoFont.heading).foregroundStyle(N.text)
                        Text("Use your home folder for work outside a specific project.").font(NekoFont.meta).foregroundStyle(N.text3)
                    }
                    Spacer()
                    Button("Add home folder") { Task { await model.addHomeWorkspace() } }.nekoGlassButton().controlSize(.small)
                }
                .padding(NekoLayout.rowInset)
                .background(N.card, in: RoundedRectangle(cornerRadius: 8))
                .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(N.line))
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
                fact("Watching", responsibilities.isEmpty ? "Nothing yet. Add a watch in Tools & skills." : "\(responsibilities.count) \(responsibilities.count == 1 ? "item" : "items")")
                if !responsibilities.isEmpty {
                    fact("Last sync", relativeTime(lastChecked) + (failing ? " · needs attention" : ""), warn: failing)
                }
            }
            DisclosureGroup("Folders · \(folders.count)  /  Tools · \(tools.count)") {
                VStack(alignment: .leading, spacing: 12) {
                    Text("Folders").font(NekoFont.heading)
                    if folders.isEmpty { EmptyRow(text: "No folders saved. Open Settings to add one.") }
                    ForEach(folders, id: \.self) { path in
                        Label((path as NSString).abbreviatingWithTildeInPath, systemImage: "folder")
                            .textSelection(.enabled)
                    }
                    Text("Connected tools").font(NekoFont.heading)
                    Text(tools.isEmpty ? "Connect tools in Tools & skills to make them available here." : tools.joined(separator: ", "))
                        .textSelection(.enabled)
                    if !workspace["instructions"].string.isEmpty {
                        Text("Workspace instructions").font(NekoFont.heading)
                        ManagementSavedText(text: workspace["instructions"].string)
                    }
                }.font(NekoFont.body).padding(.top, 8)
            }
            .font(NekoFont.meta)
        }
    }
    private func fact(_ label: String, _ value: String, warn: Bool = false) -> some View {
        GridRow {
            Text(label).font(NekoFont.meta).foregroundStyle(N.text3).gridColumnAlignment(.leading)
            Text(value).font(NekoFont.body).foregroundStyle(warn ? NekoStyle.coral : N.text2).textSelection(.enabled)
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
            Text("\(original["id"].string.isEmpty ? "New" : "Edit") \(kind.rawValue)").font(NekoFont.title)
            Form {
                Section {
                    if kind == .profile || kind == .schedule { TextField("Name", text: $name) }
                    if kind == .memory {
                        Picker("Kind", selection: $memoryKind) {
                            Text("Profile").tag("profile")
                            Text("Workspace").tag("workspace")
                            Text("Decision").tag("decision")
                        }.disabled(!original["id"].string.isEmpty)
                    }
                    VStack(alignment: .leading, spacing: 8) {
                        Text(contentLabel).font(NekoFont.heading)
                        TextEditor(text: $text).font(NekoFont.body)
                            .scrollContentBackground(.hidden).padding(8)
                            .frame(height: 112).background(N.panel, in: RoundedRectangle(cornerRadius: 6))
                            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(N.line))
                            .accessibilityLabel(contentLabel)
                    }
                } header: { Text(kind.rawValue) }
                if kind == .schedule {
                    Section {
                        Picker("Repeat", selection: $recurrence) { ForEach(ScheduleRecurrence.allCases, id: \.self) { Text($0.rawValue).tag($0) } }
                        if recurrence == .hourly { Stepper("Every \(interval) hours", value: $interval, in: 1...168) }
                        else if recurrence != .custom {
                            if recurrence == .weekly { Picker("Day", selection: $weekday) { ForEach(ScheduleRecurrence.days, id: \.0) { Text($0.1).tag($0.0) } } }
                            DatePicker("At", selection: $time, displayedComponents: .hourAndMinute)
                        }
                        LabeledContent("Time zone", value: timezone).font(NekoFont.meta)
                        DisclosureGroup("Advanced timing") {
                            TextField("Time zone", text: $timezone)
                            if recurrence == .custom {
                                TextField("Custom recurrence rule", text: $rule)
                                Text("Your saved custom rule stays unchanged until you edit it or choose another repeat option.").font(NekoFont.meta).foregroundStyle(N.text3)
                            }
                        }
                    } header: { Text("Timing") } footer: {
                        Text("Saving pauses this schedule. Enable it after reviewing the saved settings.")
                    }
                }
                if kind == .responsibility {
                    Section {
                        Toggle("Enabled", isOn: $enabled)
                        if enabled && !missingToolAccess.isEmpty {
                            Label("Choose access for \(missingToolAccess.joined(separator: ", ")) in Tools & skills before turning this on.", systemImage: "exclamationmark.circle")
                                .font(NekoFont.meta).foregroundStyle(NekoStyle.amber)
                        }
                        Toggle("Allow preparation of low-risk local fixes", isOn: $prepare)
                    } header: { Text("Authority") } footer: {
                        Text("Remote tool permission and publication authority are separate.")
                    }
                    Section("Connected tools") {
                        ForEach(model.snapshot["mcp"]["connections"].array.filter { $0["workspace_id"].string.isEmpty || $0["workspace_id"].string == effectiveWorkspace }, id: \.self) { connection in
                            Toggle(connection["label"].string, isOn: Binding(get: { connections.contains(connection["id"].string) }, set: { selected in
                                if selected { connections.insert(connection["id"].string) } else { connections.remove(connection["id"].string) }
                            }))
                        }
                    }
                }
            }
            .formStyle(.grouped).scrollContentBackground(.hidden)
            .frame(height: kind == .schedule || kind == .responsibility ? 430 : 270)
            if let error = model.error { Text(error).foregroundStyle(.red).textSelection(.enabled) }
            HStack {
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction)
                Spacer()
                Button(saving ? "Saving…" : "Save") { save() }.keyboardShortcut(.defaultAction).disabled(saving || !kind.hasRequiredContent(name: name, text: text) || (kind == .memory && memoryKind == "workspace" && effectiveWorkspace == nil) || (kind == .responsibility && (connections.isEmpty || (enabled && !missingToolAccess.isEmpty))))
            }
        }.font(NekoFont.body).padding(NekoLayout.pageInset).frame(width: 560).onAppear {
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
    private var contentLabel: String {
        switch kind {
        case .profile: "Instructions"
        case .memory: "What to remember"
        case .schedule: "What to prepare"
        case .responsibility: "What to watch"
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
