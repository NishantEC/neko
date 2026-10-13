import SwiftUI
import NekoKit

// What Home shows before the first message: a greeting with the day's
// numbers, then either how Neko works (no workspaces yet) or a short brief.

struct TodayHero: View {
    @ObservedObject var model: AppModel
    let workSummary: TodayWorkSummary
    @Binding var addingWorkspace: Bool
    var body: some View {
        let h = Calendar.current.component(.hour, from: Date())
        let part = h < 12 ? "Good morning" : h < 18 ? "Good afternoon" : "Good evening"
        let watched = model.snapshot["mcp"]["responsibilities"].array.filter {
            $0["enabled"].bool && (model.selectedWorkspace == nil || $0["workspace_id"].string == model.selectedWorkspace)
        }
        VStack(alignment: .leading, spacing: 18) {
            Text(Date.now.formatted(.dateTime.weekday(.wide).day().month(.wide)))
                .font(NekoFont.meta).foregroundStyle(N.text3)
            Text(workSummary.needsYou.isEmpty ? "\(part). All quiet for now." : "\(part). There's work to review.")
                .font(NekoFont.display).foregroundStyle(N.text)
                .fixedSize(horizontal: false, vertical: true)
            Text(model.workspaces.isEmpty ? "Add a workspace to give Neko somewhere to start." : "Ask about your work, or let Neko watch for changes. You'll review anything it wants to do.")
                .font(NekoFont.body).lineSpacing(4).foregroundStyle(N.text3)
                .fixedSize(horizontal: false, vertical: true)
            LazyVGrid(columns: [GridItem(.adaptive(minimum: 140), alignment: .leading)], alignment: .leading, spacing: NekoSpace.sm) {
                Label("\(workSummary.needsYou.count) needs you", systemImage: "hand.raised")
                Label("\(workSummary.working.count) in progress", systemImage: "circle.dotted")
                Label("\(watched.count) watching", systemImage: "eye")
            }
            .font(NekoFont.meta).foregroundStyle(N.text3)
            .padding(.vertical, NekoSpace.xxs)
            Button(watched.isEmpty ? "Start watching" : "Check now") {
                if watched.isEmpty {
                    if model.workspaces.isEmpty { addingWorkspace = true } else { model.requestedPage = .watching }
                } else {
                    for item in watched { Task { await model.workbench(.object(["Mcp": .command("Wake", ["responsibility_id": item["id"]])])) } }
                }
            }
            .nekoButton(.primary)
            .accessibilityLabel(watched.isEmpty ? "Start watching" : "Check everything now")
        }
        .padding(.top, NekoLayout.sectionGap)
    }
}

/// The three-step loop, shown until the first workspace exists.
struct HowNekoWorks: View {
    @ObservedObject var model: AppModel
    @Binding var addingWorkspace: Bool
    /// Replaces the composer's draft.
    let fill: (String) -> Void
    var body: some View {
        let watching = model.snapshot["mcp"]["responsibilities"].array.contains { $0["enabled"].bool }
        VStack(alignment: .leading, spacing: 0) {
            Text("How Neko works").font(NekoFont.heading).foregroundStyle(N.text2).padding(.bottom, NekoSpace.md).accessibilityAddTraits(.isHeader)
            LoopStep(number: 1, title: "Watch", detail: "Add a folder you work in, then the sources to keep an eye on: Linear, Sentry, GitHub, Slack.", done: !model.workspaces.isEmpty && watching,
                     action: model.workspaces.isEmpty ? "Add a workspace" : "Choose what to watch") {
                if model.workspaces.isEmpty { addingWorkspace = true } else { model.requestedPage = .watching }
            }
            LoopStep(number: 2, title: "Plan", detail: "When something changes, Neko reads it and drafts the next step the way you would. Read only.", done: !model.tasks.isEmpty, action: "Try a plan") {
                fill("Look at this workspace and plan the most useful next step. Read only; explain your reasoning.")
            }
            LoopStep(number: 3, title: "Ask you", detail: "Nothing changes without your approval. Plans and finished fixes wait for you in All agents.", done: model.tasks.contains { $0["status"].string == "Completed" }, action: "Open All agents") {
                model.requestedPage = .tickets
            }
        }
    }
}

/// Work that moved while you were away, or a few prompts to try when none did.
struct TodayBrief: View {
    @ObservedObject var model: AppModel
    let workSummary: TodayWorkSummary
    /// Replaces the composer's draft.
    let fill: (String) -> Void
    var body: some View {
        let recent = Array((workSummary.needsYou + workSummary.working).prefix(5))
        VStack(alignment: .leading, spacing: 0) {
            Text(recent.isEmpty ? "Try asking" : "While you were away").font(NekoFont.heading).foregroundStyle(N.text2).padding(.bottom, NekoSpace.md).accessibilityAddTraits(.isHeader)
            if recent.isEmpty {
                BriefRow(dot: N.text4, title: "Plan my next step", meta: "Decide the next small, useful move · read only", action: "Ask") { fill("Help me decide the next small, useful step in this workspace. Read only and explain your reasoning.") }
                BriefRow(dot: N.text4, title: "Review a change", meta: "Risks and missing tests · no edits", action: "Ask") { fill("Review the current changes in this workspace. Do not modify files; explain risks and missing tests.") }
                BriefRow(dot: N.text4, title: "Remember a preference", meta: "Teach Neko how you work", action: "Ask") { fill("Remember this preference: ") }
            } else {
                ForEach(Array(recent), id: \.recordID) { task in
                    let status = task["status"].string
                    BriefRow(dot: taskColor(status), title: task["title"].string, meta: friendlyTaskStatus(status) + (task["goal"].string.isEmpty ? "" : " · " + task["goal"].string), action: status == "AwaitingApproval" ? "Review plan" : status == "ReadyForReview" ? "Review" : "Open", status: status) { model.openAgent(task.recordID, from: .home) }
                }
            }
        }
    }
}



struct BriefRow: View {
    let dot: Color
    let title: String
    let meta: String
    let action: String
    var status: String? = nil
    let perform: () -> Void
    @State private var hover = false
    var body: some View {
        Button(action: perform) {
            HStack(alignment: .top, spacing: NekoSpace.md) {
                Group {
                    if let status { StatusGlyph(status: status, size: 14) }
                    else { Image(systemName: "arrow.up.right").font(NekoFont.meta).foregroundStyle(dot) }
                }.frame(width: 16, height: 20).accessibilityHidden(true)
                VStack(alignment: .leading, spacing: NekoSpace.xs) {
                    Text(title).font(NekoFont.body).foregroundStyle(N.text).lineLimit(2)
                    Text(meta).font(NekoFont.meta).foregroundStyle(N.text3).lineLimit(2)
                }
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                Spacer(minLength: 12)
                Text(action).font(NekoFont.meta).foregroundStyle(hover ? N.text : N.text2)
                    .padding(.horizontal, NekoSpace.xs).frame(height: 26)
                    .fixedSize()
            }
            .padding(.vertical, NekoLayout.rowInset).padding(.horizontal, NekoSpace.sm)
            .background(hover ? N.selected : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
            .overlay(alignment: .top) { N.line.frame(height: 1).padding(.horizontal, NekoSpace.sm) }
            .padding(.horizontal, -8)
            .contentShape(Rectangle())
        }.buttonStyle(.plain).onHover { h in withNekoAnimation(.hover) { hover = h } }
    }
}



struct LoopStep: View {
    let number: Int
    let title: String
    let detail: String
    let done: Bool
    let action: String
    let perform: () -> Void
    var body: some View {
        HStack(alignment: .top, spacing: 14) {
            ZStack {
                Circle().strokeBorder(done ? NekoStyle.mint : N.lineStrong, lineWidth: 1.5)
                if done { Image(systemName: "checkmark").font(NekoFont.of(.micro, weight: .bold)).foregroundStyle(NekoStyle.mint) }
                else { Text("\(number)").font(NekoFont.of(.small, weight: .semibold).monospacedDigit()).foregroundStyle(N.text3) }
            }.frame(width: 22, height: 22).padding(.top, 1)
            VStack(alignment: .leading, spacing: NekoSpace.xs) {
                Text(title).font(NekoFont.heading).foregroundStyle(N.text)
                Text(detail).font(NekoFont.body).lineSpacing(3).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 16)
            Button(action, action: perform).nekoButton(.secondary).controlSize(.small).padding(.top, NekoSpace.hair)
        }
        .padding(.vertical, NekoLayout.rowInset)
        .overlay(alignment: .top) { N.line.frame(height: 1) }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Step \(number), \(title)\(done ? ", done" : ""). \(detail)")
    }
}
