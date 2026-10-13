import SwiftUI
import AppKit
import NekoKit

/// The Skills tab: the selected workspace's instruction files, enabling them,
/// and reviewing suggested ones.
struct ToolsSkillsList: View {
    @ObservedObject var model: AppModel
    let workspace: String
    @Binding var search: String
    @Binding var repositoryURL: String
    @Binding var showFindSkill: Bool
    @Binding var expandedSkills: Set<String>
    @Binding var openedAudits: Set<String>
    let send: ToolsCommand
    /// Shows a problem in the page's error line.
    let report: (String) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: NekoLayout.sectionGap) {
            HStack {
                VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                    Text("Instructions from this workspace's folders. A skill cannot access tools on its own.")
                        .font(NekoFont.body).foregroundStyle(N.text3)
                }
                Spacer(minLength: 12)
                Button("Refresh", systemImage: "arrow.clockwise") { send("Skills", "Refresh") }.controlSize(.regular)
            }
            NekoSearchField(title: "Search skills", text: $search)
            if !visibleSkills.isEmpty {
                VStack(spacing: 0) {
                    ForEach(Array(visibleSkills.enumerated()), id: \.offset) { index, skill in
                        skillRow(skill)
                        if index < visibleSkills.count - 1 { Divider().padding(.leading, 40) }
                    }
                }
            }
            ForEach(SkillPresentation.unavailable(enabled: model.snapshot["skills"]["enabled"].array, available: model.snapshot["skills"]["available"].array, workspace: workspace), id: \.self) { record in
                NekoSection(title: "Unavailable enabled skill", marker: NekoStyle.amber, grouped: true) { EmptyView() } content: {
                    VStack(alignment: .leading, spacing: NekoSpace.sm) {
                        Text(record["path"].string).textSelection(.enabled)
                        Text("Runs stop until these instructions are restored or disabled.").foregroundStyle(.secondary)
                        Button("Disable unavailable skill") {
                            send("Skills", "SetEnabled", ["workspace_id": record["workspace_id"], "path": record["path"], "content_hash": record["content_hash"], "enabled": .bool(false)])
                        }
                    }.frame(maxWidth: .infinity, alignment: .leading)
                }
            }
            if visibleSkills.isEmpty {
                if search.isEmpty {
                    NekoEmptyState("No local skills available yet. Refresh local skills or fetch an instruction file below.")
                } else {
                    NekoEmptyState(reason: .noMatches(search))
                }
            }
            DisclosureGroup(isExpanded: $showFindSkill) {
                VStack(alignment: .leading, spacing: 10) {
                    Link("Browse skills.sh", destination: URL(string: "https://skills.sh")!)
                    LabeledContent("GitHub SKILL.md URL") { TextField("https://github.com/…/SKILL.md", text: $repositoryURL).labelsHidden() }
                    Text("Only the instruction file is previewed. Scripts and references are not installed.")
                        .font(NekoFont.meta).foregroundStyle(N.text3)
                    Button("Fetch for review") { send("Skills", "PreviewRepository", ["workspace_id": .string(workspace), "url": .string(repositoryURL)]) }.disabled(repositoryURL.isEmpty)
                }.padding(.top, NekoSpace.md)
            } label: {
                Label("Find a skill", systemImage: "plus")
                    .font(NekoFont.heading)
            }
            .padding(NekoLayout.rowInset)
            .background(N.card, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
            ForEach(model.snapshot["skills"]["proposals"].array.filter { $0["workspace_id"].string == workspace }, id: \.recordID) { proposal in
                proposalCard(proposal)
            }
        }
    }

    private var visibleSkills: [JSONValue] {
        model.snapshot["skills"]["available"].array.filter {
            ($0["workspace_id"].string.isEmpty || $0["workspace_id"].string == workspace) && (search.isEmpty || ($0["name"].string + " " + $0["description"].string).localizedCaseInsensitiveContains(search))
        }
    }

    private func skillRow(_ skill: JSONValue) -> some View {
        let record = SkillPresentation.enabledRecord(for: skill, enabled: model.snapshot["skills"]["enabled"].array, workspace: workspace)
        let changed = record != nil && record?["content_hash"] != skill["content_hash"]
        let key = skill["path"].string
        return VStack(alignment: .leading, spacing: 0) {
            Button {
                if expandedSkills.contains(key) { expandedSkills.remove(key) }
                else { expandedSkills.insert(key) }
            } label: {
                HStack(spacing: NekoSpace.md) {
                    Image(systemName: "text.book.closed").foregroundStyle(N.text3).frame(width: 28)
                    VStack(alignment: .leading, spacing: NekoSpace.xs) {
                        Text(skill["name"].string).font(NekoFont.heading).foregroundStyle(N.text)
                        Text(skill["description"].string).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2)
                    }
                    Spacer()
                    Text(record == nil ? "Available" : changed ? "Changed" : "Enabled")
                        .font(NekoFont.meta).foregroundStyle(changed ? NekoStyle.amber : N.text3)
                    Image(systemName: expandedSkills.contains(key) ? "chevron.up" : "chevron.down").font(NekoFont.meta).foregroundStyle(N.text4)
                }.contentShape(Rectangle()).padding(.vertical, NekoLayout.rowInset)
            }.buttonStyle(.plain)
            if expandedSkills.contains(key) {
                VStack(alignment: .leading, spacing: NekoSpace.sm) {
                Text(skill["description"].string).font(NekoFont.body).foregroundStyle(N.text3)
                Text(skill["path"].string).font(NekoFont.meta).foregroundStyle(N.text4).textSelection(.enabled)
                if changed { Text("Instructions changed. Review them before enabling the updated version.").foregroundStyle(.orange) }
                Button("Open instructions to review") {
                    if !NSWorkspace.shared.open(URL(fileURLWithPath: skill["path"].string)) { report("Could not open this instruction file.") }
                }
                if record == nil || changed {
                    Button(changed ? "Enable reviewed changes" : "Enable current instructions") {
                        send("Skills", "SetEnabled", ["workspace_id": .string(workspace), "path": skill["path"], "content_hash": skill["content_hash"], "enabled": .bool(true)])
                    }
                }
                if let record {
                    Button("Disable in this workspace") {
                        send("Skills", "SetEnabled", ["workspace_id": record["workspace_id"], "path": record["path"], "content_hash": record["content_hash"], "enabled": .bool(false)])
                    }
                }
                }.frame(maxWidth: .infinity, alignment: .leading).padding(.leading, 40).padding(.bottom, NekoLayout.rowInset)
            }
        }
    }

    private func proposalCard(_ proposal: JSONValue) -> some View {
        let key = proposal["id"].string + ":" + proposal["content_hash"].string
        let auditURL = proposal["audit_url"].string
        let reviewed = auditURL.isEmpty || proposal["audit_reviewed_hash"].string == proposal["content_hash"].string
        return DisclosureGroup {
            VStack(alignment: .leading, spacing: 10) {
                Text(proposal["source"].string).font(NekoFont.meta).foregroundStyle(N.text3).textSelection(.enabled)
                Text(proposal["audit_status"].string).font(NekoFont.meta).foregroundStyle(N.text3)
                if let url = URL(string: auditURL), !auditURL.isEmpty {
                    Button("View published source and audit links") {
                        if NSWorkspace.shared.open(url) { openedAudits.insert(key) }
                    }
                    Text("Read the published audit results. If none are available, do not confirm or install. Confirmation records your review, not a passing audit.").font(NekoFont.meta)
                    Button("I reviewed the linked published audit results") {
                        send("Skills", "ConfirmAuditReview", ["id": proposal["id"], "content_hash": proposal["content_hash"], "audit_url": proposal["audit_url"]])
                    }.disabled(!openedAudits.contains(key))
                }
                ScrollView {
                    Text(verbatim: proposal["body"].string)
                        .font(NekoFont.mono).textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading).padding(NekoSpace.md)
                }
                .frame(height: 240)
                .background(N.canvas, in: RoundedRectangle(cornerRadius: NekoRadius.md))
                .accessibilityLabel("Proposed skill instructions")
                Text("Accept saves these exact instructions. Enable separately after installation.").font(NekoFont.meta).foregroundStyle(N.text3)
                HStack {
                    Button("Accept & save reviewed content") { decide(proposal, accept: true) }.disabled(!reviewed)
                    Button("Reject") { decide(proposal, accept: false) }
                }
            }.frame(maxWidth: .infinity, alignment: .leading).padding(.top, NekoSpace.md)
        } label: {
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                Text(proposal["name"].string).font(NekoFont.heading).foregroundStyle(N.text).lineLimit(2)
                Text("Suggested skill · Review instructions").font(NekoFont.meta).foregroundStyle(N.text3)
            }
        }
        .padding(NekoLayout.rowInset)
        .background(N.card, in: RoundedRectangle(cornerRadius: NekoRadius.xl, style: .continuous))
    }

    private func decide(_ proposal: JSONValue, accept: Bool) {
        send("Skills", "DecideProposal", ["id": proposal["id"], "content_hash": proposal["content_hash"], "accept": .bool(accept)])
    }
}
