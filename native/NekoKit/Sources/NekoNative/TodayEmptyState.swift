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
        let counts = workSummary.counts
        let waiting = counts.waitingOnYou
        VStack(alignment: .leading, spacing: NekoSpace.md) {
            Text(Date.now.formatted(.dateTime.weekday(.wide).day().month(.wide)))
                .font(NekoFont.meta).foregroundStyle(N.text3)
            Text(waiting == 0 ? "\(part). Nothing is waiting on you." : "\(part). \(waiting) \(waiting == 1 ? "task is" : "tasks are") waiting on you.")
                .font(NekoFont.display).foregroundStyle(N.text)
                .fixedSize(horizontal: false, vertical: true)
            Text(Self.subline(counts: counts, hasWorkspace: !model.workspaces.isEmpty))
                .font(NekoFont.body).lineSpacing(NekoLeading.body).foregroundStyle(N.text3)
                .fixedSize(horizontal: false, vertical: true)
            if !model.workspaces.isEmpty {
                HStack(spacing: NekoSpace.sm) {
                    ForEach(TaskStage.allCases.filter { $0 != .done && counts[$0] > 0 }) { stage in
                        Button { model.showAgents(stage.rawValue) } label: {
                            HStack(spacing: NekoSpace.xs) {
                                Circle().fill(stage.color).frame(width: 7, height: 7)
                                Text("\(counts[stage]) \(stage.title.lowercased())")
                            }
                        }
                        .nekoButton(.secondary).controlSize(.small)
                        .help("Show these on the board")
                    }
                    Button(watched.isEmpty ? "Start watching" : "Check sources now") {
                        if watched.isEmpty { model.requestedPage = .watching }
                        else { for item in watched { Task { await model.workbench(.object(["Mcp": .command("Wake", ["responsibility_id": item["id"]])])) } } }
                    }
                    .nekoButton(watched.isEmpty ? .primary : .quiet).controlSize(.small)
                    .help(watched.isEmpty ? "Choose sources for Neko to keep an eye on" : "Look at all \(watched.count) watched sources now instead of waiting")
                }
            }
        }
        .padding(.top, NekoSpace.page * 2)
    }

    /// One plain sentence about what Neko is doing, never a status code.
    static func subline(counts: TaskCounts, hasWorkspace: Bool) -> String {
        guard hasWorkspace else { return "Add a folder you work in to give Neko somewhere to start." }
        let running = counts[.running], next = counts[.upNext]
        switch (running, next) {
        case (0, 0): return "Ask Neko anything below, or let it watch your sources and bring work here."
        case (0, _): return "\(next) \(next == 1 ? "task is" : "tasks are") lined up. Neko starts the next one when there is room."
        case (_, 0): return "Neko is working on \(running) \(running == 1 ? "task" : "tasks") now. It stops and asks when it needs you."
        default: return "Neko is working on \(running) now, with \(next) up next. It stops and asks when it needs you."
        }
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
            LoopStep(number: 3, title: "Ask you", detail: "Nothing changes without your approval. Plans and finished fixes wait for you in All tasks.", done: model.tasks.contains { $0["status"].string == "Completed" }, action: "Open All tasks") {
                model.requestedPage = .tickets
            }
        }
    }
}

/// What is waiting on you, then what is moving, or a few prompts to try.
struct TodayBrief: View {
    @ObservedObject var model: AppModel
    let workSummary: TodayWorkSummary
    /// Replaces the composer's draft.
    let fill: (String) -> Void
    private let shown = 5
    var body: some View {
        VStack(alignment: .leading, spacing: NekoChat.turnGap) {
            if !workSummary.needsYou.isEmpty {
                group("Waiting on you", tasks: workSummary.needsYou, total: workSummary.counts.waitingOnYou, filter: "all")
            }
            let running = workSummary.working.filter { TaskStage(status: $0["status"].string) == .running }
            if !running.isEmpty {
                group("Running now", tasks: running, total: running.count, filter: TaskStage.running.rawValue)
            }
            if workSummary.activeCount == 0 {
                VStack(alignment: .leading, spacing: NekoSpace.sm) {
                    heading("Try asking")
                    HStack(spacing: NekoSpace.sm) {
                        TryAsking(title: "Plan my next step", help: "Neko looks around and suggests one useful move. Changes nothing.") { fill("Help me decide the next small, useful step in this workspace. Read only and explain your reasoning.") }
                        TryAsking(title: "Review my changes", help: "Risks and missing tests. Changes nothing.") { fill("Review the current changes in this workspace. Do not modify files; explain risks and missing tests.") }
                        TryAsking(title: "Teach Neko how I work", help: "A preference it will remember next time.") { fill("Remember this preference: ") }
                    }
                }
            }
        }
    }

    private func heading(_ title: String) -> some View {
        Text(title).font(NekoFont.heading).foregroundStyle(N.text2).padding(.bottom, NekoSpace.md).accessibilityAddTraits(.isHeader)
    }

    private func group(_ title: String, tasks: [JSONValue], total: Int, filter: String) -> some View {
        VStack(alignment: .leading, spacing: NekoSpace.xxs) {
            heading(title)
            ForEach(Array(tasks.prefix(shown)), id: \.recordID) { task in
                let text = TaskText(task: task, snapshot: model.snapshot)
                let status = task["status"].string
                let reason = status == "Failed" ? TicketPresentation.stopReason(task) : TicketPresentation.waitingReason(task)
                BriefRow(dot: text.stage.color, title: text.title,
                         meta: [reason ?? friendlyTaskStatus(status), text.detail].filter { !$0.isEmpty }.joined(separator: " · "),
                         action: Self.verb(status), status: status) { model.openAgent(task.recordID, from: .home) }
            }
            if total > shown {
                Button("Show all \(total)") { model.showAgents(filter) }.nekoButton(.quiet).font(NekoFont.meta).padding(.top, NekoSpace.sm)
            }
        }
    }

    /// What pressing the row does, in the person's words.
    static func verb(_ status: String) -> String {
        switch TaskStage(status: status) {
        case .needsYou: status == "Failed" ? "See why" : "Decide"
        case .review: "Review"
        default: "Open"
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
                Spacer(minLength: NekoSpace.md)
                HStack(spacing: NekoSpace.xxs) {
                    Text(action)
                    Image(systemName: "chevron.right").font(NekoFont.of(.micro, weight: .semibold))
                }
                .font(NekoFont.meta).foregroundStyle(hover ? N.text : N.text2)
                .padding(.horizontal, NekoSpace.xs).frame(height: NekoSpace.xxl + NekoSpace.hair)
                .fixedSize()
            }
            .padding(.vertical, NekoSpace.md).padding(.horizontal, NekoSpace.sm)
            .background(hover ? N.selected : .clear, in: RoundedRectangle(cornerRadius: NekoRadius.lg, style: .continuous))
            .contentShape(Rectangle())
        }.buttonStyle(.plain).onHover { h in withNekoAnimation(.hover) { hover = h } }
    }
}

/// A suggestion capsule: clicking fills the composer, never sends.
struct TryAsking: View {
    let title: String
    let help: String
    let fill: () -> Void
    var body: some View {
        Button(action: fill) {
            Text(title).font(NekoFont.meta).foregroundStyle(N.text2)
                .padding(.horizontal, NekoSpace.md).padding(.vertical, NekoSpace.xs + 1)
                .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .liquidGlassCapsule(interactive: true)
        .help(help)
        .accessibilityHint(help)
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
                Text(detail).font(NekoFont.body).lineSpacing(NekoLeading.body).foregroundStyle(N.text3).fixedSize(horizontal: false, vertical: true)
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
