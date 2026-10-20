import Foundation
import NekoKit

/// Who started whom: Neko at the top, the watches and tasks it answers for
/// below it, and under each task the agent runs that did the work.
///
/// Built entirely from the snapshot the app already has, so every existing
/// task has a place in the tree and the daemon records nothing new:
/// - a split's child tasks sit under the task that split them (`splits`),
///   and a fix proposed in another repository under the task that found it
///   (`follow_up_of`);
/// - a task a watch filed sits under that watch (`mcp.sources`);
/// - a task Neko's chat opened says so (`conversation[].ticket_ids`);
/// - everything else was started by you, and still answers to Neko.
///
/// A task's runs come from its event log: each scout, builder, reviewer or
/// recovery run is one stretch of that role's events, and a `USAGE` event
/// (the run's reported cost) closes it. Codex runs report no cost, so their
/// cost stays unknown rather than reading as free. Event logs are bounded,
/// so a long-lived task can have lost its earliest runs.
struct AgentTree: Equatable {
    var root: AgentNode
    /// Tasks in the tree.
    var tasks: Int
    /// Agent runs working right now.
    var running: Int
    /// Reported spend across every run in the tree.
    var costUSD: Double

    static let nekoID = "neko"
}

struct AgentNode: Identifiable, Equatable {
    enum Kind: Equatable { case neko, watch, task, run }

    enum State: Equatable {
        case running, upNext, needsYou, review, done, passed, changesAsked, stopped, failed, watching, paused, failing

        var title: String {
            switch self {
            case .running: "Running"
            case .upNext: "Up next"
            case .needsYou: "Needs you"
            case .review: "Ready to review"
            case .done: "Done"
            case .passed: "Passed"
            case .changesAsked: "Changes asked"
            case .stopped: "Stopped"
            case .failed: "Failed"
            case .watching: "Watching"
            case .paused: "Paused"
            case .failing: "Failing"
            }
        }
        var symbol: String {
            switch self {
            case .running: "circle.dotted"
            case .upNext: "clock"
            case .needsYou: "hand.raised"
            case .review: "eye"
            case .done, .passed: "checkmark"
            case .changesAsked: "arrow.uturn.backward"
            case .stopped, .paused: "stop.fill"
            case .failed, .failing: "exclamationmark"
            case .watching: "eye"
            }
        }
        var isLive: Bool { self == .running }
    }

    let id: String
    let kind: Kind
    /// "Neko", a watch's name, a task's title, or a run's role ("Builder").
    let name: String
    /// The model it runs on, or what it is ("Chief of staff").
    let subtitle: String
    /// What it is working on.
    let work: String
    /// Where it came from, for a task ("You asked", "From chat").
    var origin: String? = nil
    let state: State
    /// Its own cost plus everything below it; nil when nothing reported one.
    var costUSD: Double?
    /// How long it has run (or ran), in milliseconds; nil when unknown.
    var durationMs: Int?
    /// Tools it works with, as a person would name them ("Sentry").
    var tools: [String] = []
    /// The task to open when this node is chosen.
    var taskID: String?
    var children: [AgentNode] = []

    /// Every node below this one, at any depth.
    var descendants: [AgentNode] { children.flatMap { [$0] + $0.descendants } }
}

extension AgentTree {
    /// Builds the tree for `tasks` (already filtered to what the page shows).
    /// Watches are shown for `workspace` (nil: all) when they have tasks in
    /// the tree, or when `showIdleWatches` and they are switched on.
    static func build(snapshot: JSONValue, tasks: [JSONValue], workspace: String?, showIdleWatches: Bool = true, now: Int) -> AgentTree {
        let selections = snapshot["runtime_selections"].array
        let visible = Set(tasks.map(\.recordID))
        let byID = Dictionary(tasks.map { ($0.recordID, $0) }, uniquingKeysWith: { first, _ in first })

        // Who started whom.
        var parentOf: [String: String] = [:]
        for split in snapshot["splits"].array {
            let parent = split["parent_id"].string
            for sub in split["subtasks"].array where !sub["task_id"].string.isEmpty {
                parentOf[sub["task_id"].string] = parent
            }
        }
        // A fix an agent proposed in another repository sits under the task
        // that found it (`follow_up_of`).
        var followUpOf: [String: String] = [:]
        for (child, parent) in snapshot["follow_up_of"].object { followUpOf[child] = parent.string; parentOf[child] = parent.string }
        var watchOf: [String: String] = [:]
        for source in snapshot["mcp"]["sources"].array where !source["task_id"].string.isEmpty {
            watchOf[source["task_id"].string] = source["responsibility_id"].string
        }
        let fromChat = Set(snapshot["conversation"].array.flatMap { $0["ticket_ids"].array.map(\.string) })
        let connectionNames = Dictionary(snapshot["mcp"]["connections"].array.map { ($0["id"].string, TaskText.sourceName($0["label"].string)) }, uniquingKeysWith: { first, _ in first })

        func taskNode(_ task: JSONValue) -> AgentNode {
            let id = task.recordID
            let runs = Self.runs(of: task, selections: selections, now: now)
            let splitChildren = tasks
                .filter { parentOf[$0.recordID] == id }
                .sorted(by: Self.taskOrder)
                .map(taskNode)
            let children = runs + splitChildren
            let own = runs.compactMap(\.costUSD)
            let below = splitChildren.compactMap(\.costUSD)
            let cost: Double? = own.isEmpty && below.isEmpty ? nil : (own + below).reduce(0, +)
            let status = task["status"].string
            let created = task["created_at_ms"].int
            let ended = TaskStage(status: status) == .running ? now : task["updated_at_ms"].int
            let origin: String
            if let parent = parentOf[id], let parentTask = byID[parent] {
                let parentTitle = TaskText(task: parentTask, snapshot: snapshot).title
                origin = followUpOf[id] != nil ? "Proposed by “\(parentTitle)”" : "Split from “\(parentTitle)”"
            } else if watchOf[id] != nil {
                origin = TaskText(task: task, snapshot: snapshot).source.map { "Filed from \($0)" } ?? "Filed by a watch"
            } else if fromChat.contains(id) {
                origin = "From your chat with Neko"
            } else {
                origin = "You asked"
            }
            let text = TaskText(task: task, snapshot: snapshot)
            return AgentNode(
                id: id, kind: .task, name: text.title,
                subtitle: runs.last(where: { $0.subtitle != Self.unrecordedModel })?.subtitle ?? "",
                // The source is already its tool tag; the line keeps what the tag cannot say.
                work: [text.repository, text.pullRequest].compactMap { $0 }.joined(separator: " · "),
                origin: origin,
                state: Self.state(ofTask: status),
                costUSD: cost,
                durationMs: created > 0 && ended >= created ? ended - created : nil,
                tools: watchOf[id].flatMap { _ in snapshot["mcp"]["sources"].array.first { $0["task_id"].string == id } }
                    .flatMap { connectionNames[$0["connection_id"].string] }.map { [$0] } ?? [],
                taskID: id,
                children: children)
        }

        // Tasks a split started sit under their parent, not at the top.
        let topLevel = tasks.filter { parentOf[$0.recordID].map { !visible.contains($0) } ?? true }

        var watchNodes: [AgentNode] = []
        for watch in snapshot["mcp"]["responsibilities"].array {
            if let workspace, watch["workspace_id"].string != workspace { continue }
            let filed = topLevel.filter { watchOf[$0.recordID] == watch.recordID }.sorted(by: Self.taskOrder).map(taskNode)
            let enabled = watch["enabled"].bool
            if filed.isEmpty && !(showIdleWatches && enabled) { continue }
            let name = watch["name"].string.isEmpty ? String(watch["instruction"].string.prefix(60)) : watch["name"].string
            let costs = filed.compactMap(\.costUSD)
            watchNodes.append(AgentNode(
                id: "watch:\(watch.recordID)", kind: .watch, name: name,
                subtitle: "Watch",
                work: watch["instruction"].string.trimmingCharacters(in: .whitespacesAndNewlines),
                state: !enabled ? .paused : watch["failures"].int > 0 ? .failing : .watching,
                costUSD: costs.isEmpty ? nil : costs.reduce(0, +),
                tools: Self.unique(watch["connection_ids"].array.compactMap { connectionNames[$0.string] }),
                children: filed))
        }
        // Watches with live work first, then the busiest.
        watchNodes.sort { ($0.descendants.contains { $0.state.isLive } ? 0 : 1, -$0.children.count, $0.name) < ($1.descendants.contains { $0.state.isLive } ? 0 : 1, -$1.children.count, $1.name) }

        let direct = topLevel.filter { task in
            guard let watch = watchOf[task.recordID] else { return true }
            return !watchNodes.contains { $0.id == "watch:\(watch)" }
        }.sorted(by: Self.taskOrder).map(taskNode)

        let children = watchNodes + direct
        let everything = children.flatMap { [$0] + $0.descendants }
        let running = everything.filter { $0.kind == .run && $0.state.isLive }.count
        let cost = everything.filter { $0.kind == .run }.compactMap(\.costUSD).reduce(0, +)
        let taskCount = everything.filter { $0.kind == .task }.count
        let waiting = everything.filter { $0.kind == .task && [.needsYou, .review].contains($0.state) }.count
        let work: String
        if taskCount == 0 { work = "Nothing to look after yet" }
        else if waiting > 0 { work = "Looking after \(taskCount) \(taskCount == 1 ? "task" : "tasks"), \(waiting) waiting on you" }
        else { work = "Looking after \(taskCount) \(taskCount == 1 ? "task" : "tasks")" }
        let root = AgentNode(
            id: nekoID, kind: .neko, name: "Neko", subtitle: "Chief of staff", work: work,
            state: running > 0 ? .running : waiting > 0 ? .needsYou : .done,
            costUSD: everything.contains { $0.kind == .run && $0.costUSD != nil } ? cost : nil,
            children: children)
        return AgentTree(root: root, tasks: taskCount, running: running, costUSD: cost)
    }

    /// Live and waiting work first, then most recently touched.
    static func taskOrder(_ a: JSONValue, _ b: JSONValue) -> Bool {
        func rank(_ task: JSONValue) -> Int {
            switch TaskStage(status: task["status"].string) {
            case .running: 0
            case .needsYou: 1
            case .review: 2
            case .upNext: 3
            case .done: 4
            }
        }
        let (ra, rb) = (rank(a), rank(b))
        return ra != rb ? ra < rb : a["updated_at_ms"].int > b["updated_at_ms"].int
    }

    static func state(ofTask status: String) -> AgentNode.State {
        switch status {
        case "Planning", "Building", "Reviewing": .running
        case "Queued": .upNext
        case "AwaitingApproval": .needsYou
        case "ReadyForReview": .review
        case "Completed": .done
        case "Failed": .failed
        default: .stopped
        }
    }

    // MARK: Runs

    /// The roles whose events are an agent at work, and what to call them.
    static let roles: [String: (name: String, work: String)] = [
        "scout": ("Scout", "Read the code and planned the work"),
        "builder": ("Builder", "Made the change and checked it"),
        "reviewer": ("Reviewer", "Checked the change independently"),
        "coordinator": ("Recovery", "Worked out why it stopped and what to try next")
    ]

    /// The agent runs in a task's event log, oldest first.
    static func runs(of task: JSONValue, selections: [JSONValue], now: Int) -> [AgentNode] {
        struct Run { var role: String; var start: Int; var end: Int; var cost: Double?; var closed: Bool; var verdict: Bool?; var stoppedBy: String? }
        var runs: [Run] = []
        for event in task["events"].array {
            let role = event["role"].string
            let message = event["message"].string
            let at = event["at_ms"].int
            guard roles[role] != nil else {
                // Anything else (Neko, the host, you) ends the run in progress.
                if let last = runs.indices.last, !runs[last].closed {
                    runs[last].closed = true
                    if role == "supervisor" { runs[last].stoppedBy = message }
                }
                continue
            }
            let restart = message.hasPrefix("Agent started")
            if let last = runs.indices.last, runs[last].role == role, !runs[last].closed, !restart {
                runs[last].end = at
            } else {
                runs.append(Run(role: role, start: at, end: at, cost: nil, closed: false, verdict: nil, stoppedBy: nil))
            }
            let last = runs.count - 1
            if message.hasPrefix("USAGE "), let data = message.dropFirst(6).data(using: .utf8),
               let usage = try? JSONDecoder().decode(JSONValue.self, from: data), case .number(let cost) = usage["cost_usd"] {
                runs[last].cost = (runs[last].cost ?? 0) + cost
                runs[last].closed = true
            } else if role == "reviewer", message.hasPrefix("{"), let data = message.data(using: .utf8),
                      let verdict = try? JSONDecoder().decode(JSONValue.self, from: data), case .bool(let passed) = verdict["passed"] {
                runs[last].verdict = passed
            }
        }
        let status = task["status"].string
        let live = TaskStage(status: status) == .running
        return runs.enumerated().map { index, run in
            let isLast = index == runs.count - 1
            let state: AgentNode.State
            if isLast && live && !run.closed { state = .running }
            else if let verdict = run.verdict { state = verdict ? .passed : .changesAsked }
            else if isLast && ["Failed", "Cancelled"].contains(status) && run.cost == nil { state = .stopped }
            else if let reason = run.stoppedBy, !reason.hasPrefix("Result ready"), !reason.hasPrefix("Opened"), !reason.hasPrefix("Plan ready"), run.cost == nil { state = .stopped }
            else { state = .done }
            let end = state == .running ? now : run.end
            let label = roles[run.role] ?? (run.role.capitalized, "")
            return AgentNode(
                id: "\(task.recordID)#\(index)", kind: .run, name: label.name,
                subtitle: modelLabel(for: task.recordID, at: run.start, selections: selections) ?? unrecordedModel,
                work: label.work,
                state: state,
                costUSD: run.cost,
                durationMs: end >= run.start ? end - run.start : nil,
                taskID: task.recordID)
        }
    }

    static let unrecordedModel = "Model not recorded"

    /// The model a task's run used: the selection in force when it started,
    /// else the task's earliest one. Selections are kept for the last 200
    /// dispatches, so an old run may have none.
    static func modelLabel(for taskID: String, at ms: Int, selections: [JSONValue]) -> String? {
        let own = selections.filter { $0["conversation_id"].string == "task:\(taskID)" }
        guard let chosen = own.last(where: { $0["selected_at_ms"].int <= ms }) ?? own.first else { return nil }
        return modelLabel(chosen["runtime"])
    }

    /// "Claude Opus", "Codex gpt-5.5", or the provider alone.
    static func modelLabel(_ runtime: JSONValue) -> String {
        let provider: String = switch runtime["provider"].string {
        case "", "codex": "Codex"
        case "claude": "Claude"
        case "opencode": "OpenCode"
        case let other: other.prefix(1).uppercased() + other.dropFirst()
        }
        let model = runtime["model"].string
        guard !model.isEmpty else { return provider }
        let shown = model.allSatisfy { $0.isLetter && $0.isLowercase } ? model.prefix(1).uppercased() + model.dropFirst() : model
        return "\(provider) \(shown)"
    }

    private static func unique(_ names: [String]) -> [String] {
        var seen = Set<String>()
        return names.filter { !$0.isEmpty && seen.insert($0).inserted }
    }
}

enum AgentTreeFormat {
    /// "$0.27", "$12", or "<$0.01".
    static func cost(_ usd: Double) -> String {
        if usd > 0 && usd < 0.01 { return "<$0.01" }
        return usd >= 100 ? String(format: "$%.0f", usd) : String(format: "$%.2f", usd)
    }
    /// "18s", "1m 30s", "2h 05m", "3d".
    static func duration(_ ms: Int) -> String {
        let s = max(0, ms / 1000)
        if s < 60 { return "\(s)s" }
        if s < 3600 { return "\(s / 60)m \(s % 60)s" }
        if s < 86_400 { return String(format: "%dh %02dm", s / 3600, (s % 3600) / 60) }
        return "\(s / 86_400)d"
    }
}
