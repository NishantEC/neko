import SwiftUI
import NekoKit

/// Episode revisions are displayed once, with host outcomes separate from expectations.
enum DecisionPresentation {
    static func latest(_ records: [JSONValue]) -> [JSONValue] {
        var episodes: [String: JSONValue] = [:]
        for record in records {
            let key = record["episode_id"].string
            if episodes[key] == nil || record["version"].int > episodes[key]!["version"].int { episodes[key] = record }
        }
        return episodes.values.sorted { $0["created_at_ms"].int > $1["created_at_ms"].int }
    }
    static func title(_ action: String) -> String {
        switch action {
        case "prepare_fix": "Prepare a local fix"
        case "ask_user": "Ask for your decision"
        case "skip": "Defer this work"
        case "approve_local_build": "You approved a local build"
        case "accept_local_result": "You accepted the local result"
        case "cancel_task": "You stopped this agent"
        case "report_failure": "Work stopped"
        case "review_local_result": "Local work is ready to review"
        case "add_note": "Your direction was recorded"
        default: "Prepare a plan"
        }
    }
    static func preferenceCommand(_ name: String, _ item: JSONValue) -> JSONValue {
        nested("DecisionContext", name, ["workspace_id": item["workspace_id"], "id": item["id"], "expected_version": item["version"]])
    }
}

struct DecisionCard: View {
    @ObservedObject var model: AppModel
    let record: JSONValue
    @State private var correcting = false
    @State private var correction = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Label(DecisionPresentation.title(record["action"].string), systemImage: "arrow.triangle.branch").font(.system(size: 13, weight: .semibold))
                Spacer()
                if let taskID = record["task_id"].string.nonEmpty, model.agentID != taskID {
                    Button("Open agent") { model.openAgent(taskID, from: "Home") }.controlSize(.small)
                }
            }
            Text(record["rationale"].string).font(.system(size: 13)).textSelection(.enabled)
            if let expected = record["expected_outcome"].string.nonEmpty {
                Text("Expected: " + expected).font(.system(size: 12)).foregroundStyle(.secondary)
            }
            Text("Observed: " + (record["observed_outcome"].string.nonEmpty ?? "Not yet observed")).font(.system(size: 12)).foregroundStyle(.secondary)
            if record["delivery_stage"].string == "local_accepted" {
                Label("Accepted locally", systemImage: "checkmark.circle").font(.system(size: 12)).foregroundStyle(NekoStyle.mint)
            }
            if let text = record["correction"]["text"].string.nonEmpty { Text("Your correction: " + text).font(.system(size: 12)).foregroundStyle(NekoStyle.amber) }
            DisclosureGroup("Why and scope") {
                VStack(alignment: .leading, spacing: 8) {
                    Text("Recorded by " + record["provenance"].string.replacingOccurrences(of: "_", with: " ")).foregroundStyle(.secondary)
                    Text("Working folder: " + (record["scope"]["repository"].string.nonEmpty ?? "Unavailable"))
                    Text("Confirmed guidance used: \(record["preference_versions"].array.count)")
                    ForEach(record["alternatives"].array, id: \.self) { Text("Alternative: " + DecisionPresentation.title($0.string)) }
                    let evidence = record["source"]["evidence"].array
                    Text(evidence.isEmpty ? "No connected source receipt is attached to this decision." : "\(evidence.count) source observations attached.")
                    ForEach(evidence, id: \.self) { source in
                        Text(source["title"].string + " · " + source["revision"].string)
                    }
                    ForEach(record["source"]["declared_evidence"].array, id: \.self) { note in
                        Text("Evidence note: " + note.string)
                    }
                    Text("This records the scope at the time. Current permissions still govern every action.").foregroundStyle(.secondary)
                }.font(.system(size: 12)).textSelection(.enabled).padding(.top, 6)
            }.font(.system(size: 12)).foregroundStyle(.secondary)
            Button("Correct this decision") { correcting = true }.controlSize(.small)
        }.padding(14).frame(maxWidth: .infinity, alignment: .leading)
        .nekoCard(padding: 0, radius: 10)
        .sheet(isPresented: $correcting) {
            VStack(alignment: .leading, spacing: 16) {
                Text("What should Neko do differently here?").font(.headline)
                Text("The original observation stays in history. This correction applies to this decision; broader guidance needs your confirmation in Working style.").font(.callout).foregroundStyle(.secondary)
                TextEditor(text: $correction).frame(height: 110).font(.body)
                HStack { Button("Cancel") { correcting = false }; Spacer(); Button("Save correction") {
                    Task {
                        if await model.workbench(nested("DecisionContext", "CorrectDecision", ["workspace_id": record["workspace_id"], "record_id": record["id"], "expected_version": record["version"], "correction": .string(correction)])) { correcting = false; correction = "" }
                    }
                }.buttonStyle(.borderedProminent).disabled(correction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.busy) }
            }.padding(24).frame(width: 480)
        }
    }
}

struct WorkingStyleView: View {
    @ObservedObject var model: AppModel
    @State private var editor: ManagementDraft?
    private var preferences: [JSONValue] {
        model.snapshot["working_preferences"].array.filter {
            (model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace) && $0["agent_profile_id"].string == profileFor(model)
        }
    }
    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack {
                Text("How Neko makes decisions").font(.system(size: 15, weight: .semibold))
                Spacer()
                Button("Add guidance", systemImage: "plus") { editor = ManagementDraft(value: .object([:])) }.disabled(model.selectedWorkspace == nil)
            }
            Text("Keep guidance that fits the way you work. Each rule has a context and exceptions; it guides choices within the permissions you already gave.").font(.system(size: 13)).foregroundStyle(.secondary)
            if model.selectedWorkspace == nil { Text("Select a workspace to add guidance.").font(.callout).foregroundStyle(.secondary) }
            ForEach(["proposed", "confirmed", "dismissed"], id: \.self) { state in
                let items = preferences.filter { $0["state"].string == state }
                if !items.isEmpty {
                    Text(state == "proposed" ? "Suggested · needs your confirmation" : state == "confirmed" ? "Confirmed" : "Dismissed").font(.system(size: 12, weight: .semibold)).foregroundStyle(.secondary)
                    ForEach(items, id: \.recordID) { item in preferenceRow(item) }
                }
            }
            if preferences.isEmpty { EmptyRow(text: "No working style guidance yet. Add a contextual instruction, or correct an agent decision.") }
            let records = DecisionPresentation.latest(model.snapshot["decision_records"].array.filter { model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace })
            if !records.isEmpty {
                Text("Recent decisions").font(.system(size: 15, weight: .semibold)).padding(.top, 8)
                ForEach(records.prefix(12), id: \.recordID) { DecisionCard(model: model, record: $0) }
            }
        }.frame(maxWidth: 760, alignment: .leading).frame(maxWidth: .infinity)
        .sheet(item: $editor) { WorkingPreferenceEditor(model: model, original: $0.value) }
    }
    private func preferenceRow(_ item: JSONValue) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(item["instruction"].string).font(.system(size: 13)).textSelection(.enabled)
            Text("Applies to: " + (item["applicability"]["terms"].array.map(\.string) + item["applicability"]["task_ids"].array.map { "NEK-" + String($0.string.prefix(4)).uppercased() }).joined(separator: ", ")).font(.system(size: 12)).foregroundStyle(.secondary)
            if !item["exceptions"].array.isEmpty { Text("Except: " + item["exceptions"].array.map(\.string).joined(separator: ", ")).font(.system(size: 12)).foregroundStyle(.secondary) }
            Text((model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "Workspace") + " · version \(item["version"].int)").font(.system(size: 11)).foregroundStyle(.tertiary)
            if !item["supporting_record_ids"].array.isEmpty {
                DisclosureGroup("Supporting decisions · \(item["supporting_record_ids"].array.count)") {
                    ForEach(item["supporting_record_ids"].array, id: \.self) { reference in
                        if let record = model.snapshot["decision_records"].array.first(where: { $0["id"] == reference }) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(record["correction"]["text"].string.isEmpty ? record["rationale"].string : record["correction"]["text"].string).textSelection(.enabled)
                                if !record["task_id"].string.isEmpty { Button("Open agent") { model.openAgent(record["task_id"].string, from: "Memory") } }
                            }.padding(.vertical, 4)
                        } else { Text("This decision is outside the retained history.").foregroundStyle(.secondary) }
                    }
                }.font(.system(size: 12))
            }
            HStack {
                if item["state"].string != "confirmed" { Button("Keep") { submit(model, DecisionPresentation.preferenceCommand("KeepPreference", item)) } }
                Button("Edit") { editor = ManagementDraft(value: item) }
                if item["state"].string != "dismissed" { Button("Dismiss") { submit(model, DecisionPresentation.preferenceCommand("DismissPreference", item)) } }
                Spacer()
                Button("Forget", role: .destructive) { submit(model, DecisionPresentation.preferenceCommand("ForgetPreference", item)) }
            }.controlSize(.small).disabled(model.busy)
        }.padding(14).nekoCard(padding: 0, radius: 10)
    }
}

struct WorkingPreferenceEditor: View {
    @ObservedObject var model: AppModel
    let original: JSONValue
    @Environment(\.dismiss) private var dismiss
    @State private var instruction: String
    @State private var terms: String
    @State private var exceptions: String
    init(model: AppModel, original: JSONValue) {
        self.model = model; self.original = original
        _instruction = State(initialValue: original["instruction"].string)
        _terms = State(initialValue: original["applicability"]["terms"].array.map(\.string).joined(separator: ", "))
        _exceptions = State(initialValue: original["exceptions"].array.map(\.string).joined(separator: ", "))
    }
    private func phrases(_ text: String) -> JSONValue { .array(text.split(separator: ",").map { .string($0.trimmingCharacters(in: .whitespacesAndNewlines)) }.filter { !$0.string.isEmpty }) }
    var body: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(original.recordID.isEmpty ? "Add working guidance" : "Edit working guidance").font(.headline)
            TextField("Instruction", text: $instruction, axis: .vertical).lineLimit(2...5)
            TextField("Context words or phrases, separated by commas", text: $terms)
            TextField("Exceptions, separated by commas", text: $exceptions)
            Text("Match complete words in the task. Changed guidance returns to Suggested until you keep it.").font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Cancel") { dismiss() }; Spacer()
                Button("Save suggestion") {
                    Task {
                        let workspace = original["workspace_id"].string.nonEmpty ?? model.selectedWorkspace ?? ""
                        if await model.workbench(nested("DecisionContext", "SavePreference", ["workspace_id": .string(workspace), "id": .string(original.recordID), "expected_version": original["version"], "applicability": .object(["terms": phrases(terms), "task_ids": original["applicability"]["task_ids"] == .null ? .array([]) : original["applicability"]["task_ids"]]), "instruction": .string(instruction), "supporting_record_ids": original["supporting_record_ids"] == .null ? .array([]) : original["supporting_record_ids"], "exceptions": phrases(exceptions)])) { dismiss() }
                    }
                }.buttonStyle(.borderedProminent).disabled(instruction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || (phrases(terms).array.isEmpty && original["applicability"]["task_ids"].array.isEmpty) || model.busy)
            }
        }.textFieldStyle(.roundedBorder).padding(24).frame(width: 520)
    }
}

private extension String { var nonEmpty: String? { isEmpty ? nil : self } }
