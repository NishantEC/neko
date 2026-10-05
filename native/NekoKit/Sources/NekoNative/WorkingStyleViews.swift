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
    var originPage = "Home"
    @State private var correcting = false
    @State private var correction = ""
    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(DecisionPresentation.title(record["action"].string))
                .font(NekoFont.heading).foregroundStyle(N.text).accessibilityAddTraits(.isHeader)
            if let observed = record["observed_outcome"].string.nonEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Observed outcome").font(NekoFont.meta).foregroundStyle(N.text3)
                    ManagementDetailText(text: observed, disclosure: "Full observed outcome")
                }
            } else {
                Text("Outcome not yet observed").font(NekoFont.meta).foregroundStyle(N.text3)
            }
            if record["delivery_stage"].string == "local_accepted" {
                Label("Accepted locally", systemImage: "checkmark.circle").font(NekoFont.meta).foregroundStyle(NekoStyle.mint)
            }
            if let text = record["correction"]["text"].string.nonEmpty {
                VStack(alignment: .leading, spacing: 4) {
                    Text("Your correction").font(NekoFont.heading)
                    ManagementDetailText(text: text, disclosure: "Full correction")
                }
            }
            DisclosureGroup("Reasoning, expectations & scope") {
                VStack(alignment: .leading, spacing: 12) {
                    if !record["rationale"].string.isEmpty {
                        Text("Reasoning").font(NekoFont.heading)
                        ManagementSavedText(text: record["rationale"].string)
                    }
                    if let expected = record["expected_outcome"].string.nonEmpty {
                        Text("Expected outcome").font(NekoFont.heading)
                        ManagementSavedText(text: expected)
                    }
                    Divider()
                    Text("Recorded by " + record["provenance"].string.replacingOccurrences(of: "_", with: " ")).foregroundStyle(N.text3)
                    Text("Working folder: " + (record["scope"]["repository"].string.nonEmpty ?? "Unavailable"))
                    Text("Confirmed guidance used: \(record["preference_versions"].array.count)")
                    ForEach(record["alternatives"].array, id: \.self) { Text("Alternative: " + DecisionPresentation.title($0.string)) }
                    let evidence = record["source"]["evidence"].array
                    Text(evidence.isEmpty ? "No connected source receipt is attached to this decision." : "\(evidence.count) source observations attached.")
                    ForEach(evidence, id: \.self) { source in
                        VStack(alignment: .leading, spacing: 3) {
                            Text(source["title"].string).font(NekoFont.heading)
                            Text(source["revision"].string).font(NekoFont.meta.monospaced()).foregroundStyle(N.text3)
                        }
                    }
                    ForEach(record["source"]["declared_evidence"].array, id: \.self) { note in
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Evidence note").font(NekoFont.heading)
                            ManagementSavedText(text: note.string)
                        }
                    }
                    Text("Scope is recorded at the time of the decision. Current permissions govern every action.").font(NekoFont.meta).foregroundStyle(N.text3)
                }.font(NekoFont.body).textSelection(.enabled).padding(.top, 8)
            }.font(NekoFont.meta)
            HStack(spacing: 8) {
                Button("Correct this decision") { correcting = true }
                if let taskID = record["task_id"].string.nonEmpty, model.agentID != taskID {
                    Button("Open agent") { model.openAgent(taskID, from: originPage) }
                }
            }.controlSize(.small)
        }
        .padding(NekoLayout.rowInset).frame(maxWidth: .infinity, alignment: .leading)
        .background(N.card, in: RoundedRectangle(cornerRadius: 8))
        .overlay(RoundedRectangle(cornerRadius: 8).strokeBorder(N.line))
        .accessibilityElement(children: .contain)
        .sheet(isPresented: $correcting) {
            VStack(alignment: .leading, spacing: 16) {
                Text("Correct this decision").font(NekoFont.title)
                Form {
                    Section {
                        TextEditor(text: $correction).font(NekoFont.body)
                            .scrollContentBackground(.hidden).padding(8).frame(height: 120)
                            .background(N.panel, in: RoundedRectangle(cornerRadius: 6))
                            .overlay(RoundedRectangle(cornerRadius: 6).strokeBorder(N.line))
                            .accessibilityLabel("What should Neko do differently?")
                    } header: { Text("What should Neko do differently?") } footer: {
                        Text("The original observation stays in history. This correction applies to this decision; broader guidance needs your confirmation in Working style.")
                    }
                }.formStyle(.grouped).scrollContentBackground(.hidden).frame(height: 245)
                if let error = model.error { Text(error).font(NekoFont.meta).foregroundStyle(NekoStyle.coral).textSelection(.enabled) }
                HStack {
                    Button("Cancel") { correcting = false }.keyboardShortcut(.cancelAction)
                    Spacer()
                    Button("Save correction") {
                        Task {
                            if await model.workbench(nested("DecisionContext", "CorrectDecision", ["workspace_id": record["workspace_id"], "record_id": record["id"], "expected_version": record["version"], "correction": .string(correction)])) { correcting = false; correction = "" }
                        }
                    }.keyboardShortcut(.defaultAction).disabled(correction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || model.busy)
                }
            }.font(NekoFont.body).padding(NekoLayout.pageInset).frame(width: 560)
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
        VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
            WorkspaceSection(name: "Working guidance", color: N.text4) {
                Button("Add guidance", systemImage: "plus") { editor = ManagementDraft(value: .object([:])) }
                    .disabled(model.selectedWorkspace == nil)
            } content: {
                Text("Contextual rules guide choices within the permissions you already gave.")
                    .font(NekoFont.body).foregroundStyle(N.text3)
                if model.selectedWorkspace == nil {
                    Label("Select a workspace to add guidance.", systemImage: "sidebar.left")
                        .font(NekoFont.meta).foregroundStyle(N.text3)
                }
                if preferences.isEmpty {
                    EmptyRow(text: "Add guidance for a recurring situation, or correct an agent decision to suggest a rule.")
                }
            }
            ForEach(["proposed", "confirmed"], id: \.self) { state in
                let items = preferences.filter { $0["state"].string == state }
                if !items.isEmpty {
                    WorkspaceSection(name: state == "proposed" ? "Needs your confirmation" : "Confirmed guidance", color: state == "proposed" ? NekoStyle.amber : N.text4, detail: "\(items.count)") { EmptyView() } content: {
                        ForEach(items, id: \.recordID) { item in
                            preferenceRow(item)
                            if item != items.last { Divider() }
                        }
                    }
                }
            }
            let dismissed = preferences.filter { $0["state"].string == "dismissed" }
            if !dismissed.isEmpty {
                DisclosureGroup("Dismissed guidance · \(dismissed.count)") {
                    VStack(alignment: .leading, spacing: 14) {
                        ForEach(dismissed, id: \.recordID) { item in
                            preferenceRow(item)
                            if item != dismissed.last { Divider() }
                        }
                    }.padding(.top, 10)
                }.font(NekoFont.heading)
            }
            let records = DecisionPresentation.latest(model.snapshot["decision_records"].array.filter { model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace })
            if !records.isEmpty {
                DisclosureGroup("Recent decisions · \(min(records.count, 12))") {
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(records.prefix(12), id: \.recordID) { DecisionCard(model: model, record: $0, originPage: "Memory") }
                    }.padding(.top, 10)
                }.font(NekoFont.heading)
            }
        }
        .sheet(item: $editor) { WorkingPreferenceEditor(model: model, original: $0.value) }
    }
    private func preferenceRow(_ item: JSONValue) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            ManagementDetailText(text: item["instruction"].string, disclosure: "Full guidance")
            Text("Applies to: " + (item["applicability"]["terms"].array.map(\.string) + item["applicability"]["task_ids"].array.map { "NEK-" + String($0.string.prefix(4)).uppercased() }).joined(separator: ", "))
                .font(NekoFont.meta).foregroundStyle(N.text3)
            if !item["exceptions"].array.isEmpty {
                ManagementDetailText(text: "Except: " + item["exceptions"].array.map(\.string).joined(separator: ", "), disclosure: "All exceptions")
            }
            Text((model.workspaces.first { $0.recordID == item["workspace_id"].string }?["name"].string ?? "Workspace") + " · version \(item["version"].int)")
                .font(NekoFont.meta).foregroundStyle(N.text3)
            if !item["supporting_record_ids"].array.isEmpty {
                DisclosureGroup("Supporting decisions · \(item["supporting_record_ids"].array.count)") {
                    VStack(alignment: .leading, spacing: 12) {
                        ForEach(item["supporting_record_ids"].array, id: \.self) { reference in
                            if let record = model.snapshot["decision_records"].array.first(where: { $0["id"] == reference }) {
                                VStack(alignment: .leading, spacing: 8) {
                                    ManagementSavedText(text: record["correction"]["text"].string.isEmpty ? record["rationale"].string : record["correction"]["text"].string)
                                    if !record["task_id"].string.isEmpty { Button("Open agent") { model.openAgent(record["task_id"].string, from: "Memory") } }
                                }
                            } else { Text("This decision is outside the retained history.").foregroundStyle(N.text3) }
                        }
                    }.font(NekoFont.body).padding(.top, 8)
                }.font(NekoFont.meta)
            }
            HStack(spacing: 8) {
                if item["state"].string != "confirmed" { Button("Keep guidance") { submit(model, DecisionPresentation.preferenceCommand("KeepPreference", item)) } }
                Button("Edit") { editor = ManagementDraft(value: item) }
                if item["state"].string != "dismissed" { Button("Dismiss") { submit(model, DecisionPresentation.preferenceCommand("DismissPreference", item)) } }
                Spacer(minLength: 8)
                Menu("Guidance actions", systemImage: "ellipsis") {
                    Button("Forget guidance", role: .destructive) { submit(model, DecisionPresentation.preferenceCommand("ForgetPreference", item)) }
                }.labelStyle(.iconOnly).fixedSize()
            }.controlSize(.small).disabled(model.busy)
        }.font(NekoFont.body).padding(.vertical, 4)
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
            Text(original.recordID.isEmpty ? "Add working guidance" : "Edit working guidance").font(NekoFont.title)
            Form {
                Section("Guidance") {
                    TextField("Instruction", text: $instruction, axis: .vertical).lineLimit(2...5)
                }
                Section {
                    TextField("Words or phrases", text: $terms, axis: .vertical).lineLimit(1...3)
                    TextField("Exceptions", text: $exceptions, axis: .vertical).lineLimit(1...3)
                } header: { Text("When it applies") } footer: {
                    Text("Separate phrases with commas. Context matches complete words in a task; exceptions narrow when guidance applies.")
                }
            }.formStyle(.grouped).scrollContentBackground(.hidden).frame(height: 290)
            Text("Saved guidance returns to Suggested until you keep it. It does not grant new permissions.")
                .font(NekoFont.meta).foregroundStyle(N.text3)
            if let error = model.error { Text(error).font(NekoFont.meta).foregroundStyle(NekoStyle.coral).textSelection(.enabled) }
            HStack {
                Button("Cancel") { dismiss() }.keyboardShortcut(.cancelAction); Spacer()
                Button("Save suggestion") {
                    Task {
                        let workspace = original["workspace_id"].string.nonEmpty ?? model.selectedWorkspace ?? ""
                        if await model.workbench(nested("DecisionContext", "SavePreference", ["workspace_id": .string(workspace), "id": .string(original.recordID), "expected_version": original["version"], "applicability": .object(["terms": phrases(terms), "task_ids": original["applicability"]["task_ids"] == .null ? .array([]) : original["applicability"]["task_ids"]]), "instruction": .string(instruction), "supporting_record_ids": original["supporting_record_ids"] == .null ? .array([]) : original["supporting_record_ids"], "exceptions": phrases(exceptions)])) { dismiss() }
                    }
                }.keyboardShortcut(.defaultAction).disabled(instruction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || (phrases(terms).array.isEmpty && original["applicability"]["task_ids"].array.isEmpty) || model.busy)
            }
        }.font(NekoFont.body).textFieldStyle(.roundedBorder).padding(NekoLayout.pageInset).frame(width: 560)
    }
}

private extension String { var nonEmpty: String? { isEmpty ? nil : self } }
