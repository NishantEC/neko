import SwiftUI
import NekoKit

private func replacing(_ value: JSONValue, _ fields: [String: JSONValue]) -> JSONValue {
    var result = value.object
    fields.forEach { result[$0.key] = $0.value }
    return .object(result)
}
private func nested(_ family: String, _ name: String, _ fields: [String: JSONValue]) -> JSONValue {
    .object([family: .command(name, fields)])
}
private struct ManagementDraft: Identifiable {
    let id = UUID()
    var value: JSONValue
}
@MainActor private func submit(_ model: AppModel, _ command: JSONValue) {
    Task { await model.workbench(command) }
}
@MainActor private func profileFor(_ model: AppModel) -> String {
    let profiles = model.snapshot["agent_profiles"]
    if let workspace = model.selectedWorkspace {
        return profiles["assignments"].array.first { $0["workspace_id"].string == workspace }?["profile_id"].string ?? "default"
    }
    let active = profiles["active_profile_id"].string
    return active.isEmpty ? "default" : active
}

/// List rows can coalesce several controls into one accessibility row on macOS.
/// Eager stacks preserve each native button and toggle as a separate AX element.
private struct ManagementScroll<Content: View>: View {
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
        ManagementScroll {
            Button("New profile") { draft = ManagementDraft(value: .object([:])) }
            ForEach(model.snapshot["agent_profiles"]["profiles"].array, id: \.self) { profile in
                VStack(alignment: .leading, spacing: 12) {
                    Text(profile["name"].string).font(.headline).accessibilityAddTraits(.isHeader)
                    if !profile["instructions"].string.isEmpty {
                        Text(profile["instructions"].string).textSelection(.enabled)
                    }
                    HStack {
                        Button("Edit") { draft = ManagementDraft(value: profile) }
                        Button(model.snapshot["agent_profiles"]["active_profile_id"] == profile["id"] ? "Active profile" : "Make active") {
                            submit(model, nested("AgentProfiles", "SetActive", ["profile_id": profile["id"]]))
                        }
                        if let workspace = model.selectedWorkspace {
                            Button("Assign to selected workspace") {
                                submit(model, nested("AgentProfiles", "AssignWorkspace", ["workspace_id": .string(workspace), "profile_id": profile["id"]]))
                            }
                        }
                    }
                    ForEach(model.snapshot["agent_profiles"]["profiles"].array.filter { $0["id"] != profile["id"] }, id: \.self) { source in
                        Toggle("Read global memories from \(source["name"].string)", isOn: Binding(get: {
                            model.snapshot["agent_profiles"]["read_grants"].array.contains { $0["reader_id"] == profile["id"] && $0["source_id"] == source["id"] }
                        }, set: { allowed in
                            submit(model, nested("AgentProfiles", "SetReadGrant", ["reader_id": profile["id"], "source_id": source["id"], "allowed": .bool(allowed)]))
                        }))
                    }
                    Text("Read grants share global memories only; they do not grant tools or workspace access.").font(.caption).foregroundStyle(.secondary)
                }.accessibilityElement(children: .contain)
            }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .profile, original: item.value, workspace: model.selectedWorkspace, profileID: profileFor(model))
        }
    }
}

struct MemoryView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    private func inScope(_ item: JSONValue) -> Bool {
        item["agent_profile_id"].string == profileFor(model) && (item["workspace_id"] == .null || item["workspace_id"].string == model.selectedWorkspace)
    }
    var body: some View {
        ManagementScroll {
            Button("Add memory") { draft = ManagementDraft(value: .object([:])) }
            if model.snapshot["memory"].array.filter(inScope).isEmpty { Text("No memories yet. Add a preference or something Neko should remember about your work.").foregroundStyle(.secondary) }
            VStack(alignment: .leading, spacing: 12) {
                Text("Memories").font(.headline).accessibilityAddTraits(.isHeader)
                ForEach(model.snapshot["memory"].array.filter(inScope), id: \.self) { entry in
                    VStack(alignment: .leading) {
                        Text(entry["kind"].string.capitalized).font(.caption).foregroundStyle(.secondary)
                        Text(entry["text"].string).textSelection(.enabled)
                        HStack {
                            Button("Edit") { draft = ManagementDraft(value: entry) }
                            Button("Delete", role: .destructive) { submit(model, .command("DeleteMemory", ["id": entry["id"]])) }
                        }
                    }
                }
            }
            VStack(alignment: .leading, spacing: 12) {
                Text("Suggested memories").font(.headline).accessibilityAddTraits(.isHeader)
                ForEach(model.snapshot["memory_proposals"].array.filter(inScope), id: \.self) { proposal in
                    VStack(alignment: .leading) {
                        Text(proposal["text"].string)
                        Text(proposal["source"].string).font(.caption).foregroundStyle(.secondary)
                        HStack {
                            Button("Accept") { submit(model, .command("DecideMemoryProposal", ["id": proposal["id"], "accept": .bool(true)])) }
                            Button("Reject") { submit(model, .command("DecideMemoryProposal", ["id": proposal["id"], "accept": .bool(false)])) }
                        }
                    }
                }
            }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .memory, original: item.value, workspace: model.selectedWorkspace, profileID: profileFor(model))
        }
    }
}

struct ResponsibilitiesView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    var body: some View {
        ManagementScroll {
            Button("New responsibility") { draft = ManagementDraft(value: .object([:])) }.disabled(model.selectedWorkspace == nil)
            if model.snapshot["mcp"]["responsibilities"].array.filter({ $0["workspace_id"].string == model.selectedWorkspace }).isEmpty { Text(model.selectedWorkspace == nil ? "Choose a workspace to add responsibilities." : "No responsibilities yet. Add something you want Neko to check regularly.").foregroundStyle(.secondary) }
            ForEach(model.snapshot["mcp"]["responsibilities"].array.filter { $0["workspace_id"].string == model.selectedWorkspace }, id: \.self) { item in
                VStack(alignment: .leading, spacing: 8) {
                    Text(item["instruction"].string).textSelection(.enabled)
                    Text(item["last_result"].string).font(.caption).foregroundStyle(.secondary)
                    Toggle("Enabled", isOn: Binding(get: { item["enabled"].bool }, set: { enabled in
                        submit(model, nested("Mcp", "SaveResponsibility", ["responsibility": replacing(item, ["enabled": .bool(enabled)])]))
                    }))
                    HStack {
                        Button("Edit") { draft = ManagementDraft(value: item) }
                        Button("Check now") { submit(model, nested("Mcp", "Wake", ["responsibility_id": item["id"]])) }
                    }
                }
            }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .responsibility, original: item.value, workspace: model.selectedWorkspace, profileID: profileFor(model))
        }
    }
}

struct SchedulesView: View {
    @ObservedObject var model: AppModel
    @State private var draft: ManagementDraft?
    var body: some View {
        ManagementScroll {
            Button("New schedule") { draft = ManagementDraft(value: .object([:])) }.disabled(model.selectedWorkspace == nil)
            if model.snapshot["schedules"].array.filter({ $0["workspace_id"].string == model.selectedWorkspace }).isEmpty { Text(model.selectedWorkspace == nil ? "Choose a workspace to add a schedule." : "No schedules yet. Choose when Neko should prepare a plan for you.").foregroundStyle(.secondary) }
            ForEach(model.snapshot["schedules"].array.filter { $0["workspace_id"].string == model.selectedWorkspace }, id: \.self) { item in
                VStack(alignment: .leading, spacing: 12) {
                    Text(item["name"].string).font(.headline).accessibilityAddTraits(.isHeader)
                    Text(item["prompt"].string).textSelection(.enabled)
                    Text("\(ScheduleRecurrence.summary(item["rule"].string)) · \(item["timezone"].string)").font(.caption)
                    Text(item["last_result"].string).foregroundStyle(.secondary)
                    if item["next_due_ms"] != .null {
                        Text("Next: \(Date(timeIntervalSince1970: Double(item["next_due_ms"].int) / 1000).formatted())").font(.caption)
                    }
                    Toggle("Enabled", isOn: Binding(get: { item["enabled"].bool }, set: { enabled in
                        submit(model, nested("Schedules", "SetEnabled", ["id": item["id"], "enabled": .bool(enabled)]))
                    }))
                    HStack {
                        Button("Edit") { draft = ManagementDraft(value: item) }
                        Button("Run now") { submit(model, nested("Schedules", "RunNow", ["id": item["id"]])) }
                        Button("Delete", role: .destructive) { submit(model, nested("Schedules", "Remove", ["id": item["id"]])) }
                    }
                }
            }
        }.sheet(item: $draft) { item in
            ManagementEditor(model: model, kind: .schedule, original: item.value, workspace: model.selectedWorkspace, profileID: profileFor(model))
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
private struct ManagementEditor: View {
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
                Button(saving ? "Saving…" : "Save") { save() }.keyboardShortcut(.defaultAction).disabled(saving || !kind.hasRequiredContent(name: name, text: text) || (kind == .memory && memoryKind == "workspace" && effectiveWorkspace == nil) || (kind == .responsibility && connections.isEmpty))
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
