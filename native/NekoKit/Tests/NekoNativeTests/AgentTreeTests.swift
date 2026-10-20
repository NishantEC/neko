import XCTest
@testable import NekoNative
import NekoKit

/// Who started whom, built from the snapshot alone. Pure, so no window.
final class AgentTreeTests: XCTestCase {
    private func json(_ text: String) throws -> JSONValue {
        try JSONDecoder().decode(JSONValue.self, from: Data(text.utf8))
    }

    private func event(_ at: Int, _ role: String, _ message: String) -> String {
        let quoted = String(data: try! JSONEncoder().encode(message), encoding: .utf8)!
        return #"{"at_ms":\#(at),"role":"\#(role)","message":\#(quoted)}"#
    }

    private func task(_ id: String, _ status: String, events: [String] = [], created: Int = 1_000, updated: Int = 9_000) -> String {
        #"{"id":"\#(id)","workspace_id":"w","title":"Task \#(id)","status":"\#(status)","created_at_ms":\#(created),"updated_at_ms":\#(updated),"events":[\#(events.joined(separator: ","))]}"#
    }

    private func snapshot(tasks: [String], extra: String = "") throws -> JSONValue {
        try json(#"{"tasks":[\#(tasks.joined(separator: ","))]\#(extra.isEmpty ? "" : "," + extra)}"#)
    }

    func testRunsAreSplitByRoleAndClosedByTheirReportedCost() throws {
        let t = try json(task("a", "Building", events: [
            event(100, "system", "Queued for planning."),
            event(110, "builder", "Agent started in an isolated task worktree"),
            event(150, "builder", "Command completed"),
            event(200, "builder", #"USAGE {"cost_usd":1.5,"input_tokens":3}"#),
            event(210, "reviewer", "Command running"),
            event(260, "reviewer", #"{"passed":false,"findings":["x"]}"#),
            event(270, "reviewer", #"USAGE {"cost_usd":0.5}"#),
            event(280, "coordinator", "Investigating the blocker"),
            event(300, "coordinator", #"USAGE {"cost_usd":0.25}"#),
            // The same agent directed again: a new run after the last one closed.
            event(310, "builder", "Command completed"),
        ]))
        let runs = AgentTree.runs(of: t, selections: [], now: 500)
        XCTAssertEqual(runs.map(\.name), ["Builder", "Reviewer", "Recovery", "Builder"])
        XCTAssertEqual(runs.map(\.costUSD), [1.5, 0.5, 0.25, nil])
        XCTAssertEqual(runs.map(\.state), [.done, .changesAsked, .done, .running])
        XCTAssertEqual(runs[0].durationMs, 90)
        XCTAssertEqual(runs[3].durationMs, 190, "a live run's time runs to now")
        XCTAssertEqual(runs[3].subtitle, AgentTree.unrecordedModel)
    }

    func testARunWithoutACostThatTheSupervisorStoppedReadsAsStopped() throws {
        let t = try json(task("a", "ReadyForReview", events: [
            event(110, "builder", "Command completed"),
            event(120, "supervisor", "No successful independent execution evidence for: yarn test"),
            event(130, "builder", "Agent started in an isolated task worktree"),
            event(140, "builder", #"USAGE {"cost_usd":1}"#),
            event(150, "supervisor", "Result ready for your review."),
        ]))
        XCTAssertEqual(AgentTree.runs(of: t, selections: [], now: 500).map(\.state), [.stopped, .done])
    }

    func testTheLastRunOfAFailedTaskIsStopped() throws {
        let t = try json(task("a", "Failed", events: [event(110, "builder", "Command completed")]))
        XCTAssertEqual(AgentTree.runs(of: t, selections: [], now: 500).map(\.state), [.stopped])
    }

    func testEachRunNamesTheModelThatWasChosenWhenItStarted() throws {
        let selections = try json(#"""
        [{"conversation_id":"task:a","selected_at_ms":100,"runtime":{"provider":"claude","model":"opus"}},
         {"conversation_id":"task:a","selected_at_ms":300,"runtime":{"provider":"codex","model":"gpt-5.5"}},
         {"conversation_id":"task:b","selected_at_ms":50,"runtime":{"provider":"claude","model":"sonnet"}}]
        """#).array
        let t = try json(task("a", "Completed", events: [
            event(110, "builder", #"USAGE {"cost_usd":1}"#),
            event(310, "reviewer", "Command running"),
        ]))
        XCTAssertEqual(AgentTree.runs(of: t, selections: selections, now: 500).map(\.subtitle), ["Claude Opus", "Codex gpt-5.5"])
        XCTAssertEqual(AgentTree.modelLabel(try json(#"{"provider":"","model":""}"#)), "Codex")
    }

    func testTasksSitUnderWhoeverStartedThem() throws {
        let snap = try snapshot(tasks: [
            task("parent", "Building", events: [event(110, "builder", #"USAGE {"cost_usd":2}"#), event(120, "builder", "Agent started")]),
            task("child", "Queued", events: [event(130, "scout", #"USAGE {"cost_usd":0.5}"#)]),
            task("filed", "AwaitingApproval"),
            task("chat", "Completed"),
            task("mine", "ReadyForReview"),
        ], extra: #"""
        "splits":[{"parent_id":"parent","subtasks":[{"task_id":"child"}],"approved":true,"integrated":false}],
        "conversation":[{"ticket_ids":["chat"]}],
        "mcp":{"connections":[{"id":"c1","label":"sentry"}],
               "sources":[{"task_id":"filed","responsibility_id":"r1","connection_id":"c1","external_id":"X-1"}],
               "responsibilities":[{"id":"r1","workspace_id":"w","name":"Sentry errors","instruction":"Watch errors","connection_ids":["c1"],"enabled":true,"failures":0},
                                   {"id":"r2","workspace_id":"w","name":"Idle","instruction":"Nothing yet","connection_ids":[],"enabled":false,"failures":0}]}
        """#)
        let tasks = snap["tasks"].array
        let tree = AgentTree.build(snapshot: snap, tasks: tasks, workspace: "w", now: 500)

        XCTAssertEqual(tree.root.kind, .neko)
        XCTAssertEqual(tree.root.children.map(\.id), ["watch:r1", "parent", "mine", "chat"],
                       "watches first, then tasks by stage; the split child and the filed task are not at the top")
        let watch = tree.root.children[0]
        XCTAssertEqual(watch.children.map(\.id), ["filed"])
        XCTAssertEqual(watch.tools, ["Sentry"])
        XCTAssertEqual(watch.state, .watching)
        XCTAssertEqual(watch.children[0].tools, ["Sentry"])
        XCTAssertFalse(tree.root.children.contains { $0.id == "watch:r2" }, "a switched-off watch with no tasks is left out")

        let parent = tree.root.children[1]
        XCTAssertEqual(parent.children.map(\.kind), [.run, .run, .task], "its runs, then the task it split off")
        XCTAssertEqual(parent.children.last?.id, "child")
        XCTAssertEqual(parent.children.last?.origin, "Split from “Task parent”")
        XCTAssertEqual(parent.costUSD, 2.5, "a task's cost includes what it started")
        XCTAssertEqual(tree.root.children.first { $0.id == "chat" }?.origin, "From your chat with Neko")
        XCTAssertEqual(tree.root.children.first { $0.id == "mine" }?.origin, "You asked")

        XCTAssertEqual(tree.tasks, 5)
        XCTAssertEqual(tree.running, 1)
        XCTAssertEqual(tree.costUSD, 2.5)
        XCTAssertEqual(tree.root.costUSD, 2.5)
        XCTAssertEqual(tree.root.state, .running)
        XCTAssertEqual(tree.root.work, "Looking after 5 tasks, 2 waiting on you")
    }

    func testAFilteredOutParentLeavesItsChildAtTheTop() throws {
        let snap = try snapshot(tasks: [task("child", "Queued")],
                                extra: #""splits":[{"parent_id":"gone","subtasks":[{"task_id":"child"}]}]"#)
        let tree = AgentTree.build(snapshot: snap, tasks: snap["tasks"].array, workspace: nil, now: 0)
        XCTAssertEqual(tree.root.children.map(\.id), ["child"])
        XCTAssertNil(tree.root.costUSD, "nothing reported a cost, so none is claimed")
    }

    func testOnlyLiveWorkIsOpenByDefault() throws {
        let snap = try snapshot(tasks: [
            task("live", "Building", events: [event(1, "builder", "Command running")]),
            task("idle", "ReadyForReview", events: [event(1, "builder", #"USAGE {"cost_usd":1}"#)]),
        ])
        let tree = AgentTree.build(snapshot: snap, tasks: snap["tasks"].array, workspace: nil, now: 10)
        XCTAssertTrue(AgentTreeView.expandsByDefault(tree.root))
        XCTAssertEqual(tree.root.children.map { AgentTreeView.expandsByDefault($0) }, [true, false])
        XCTAssertEqual(tree.root.children[0].work, "", "no repository recorded, and the source is never repeated here")
    }

    func testFormatsReadAsAPersonWouldSayThem() {
        XCTAssertEqual(AgentTreeFormat.cost(0.27), "$0.27")
        XCTAssertEqual(AgentTreeFormat.cost(0.004), "<$0.01")
        XCTAssertEqual(AgentTreeFormat.cost(0), "$0.00")
        XCTAssertEqual(AgentTreeFormat.cost(140.4), "$140")
        XCTAssertEqual(AgentTreeFormat.duration(18_000), "18s")
        XCTAssertEqual(AgentTreeFormat.duration(90_000), "1m 30s")
        XCTAssertEqual(AgentTreeFormat.duration(7_500_000), "2h 05m")
        XCTAssertEqual(AgentTreeFormat.duration(3 * 86_400_000), "3d")
    }
}
