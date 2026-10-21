import XCTest
@testable import NekoNative
import NekoKit

/// Presentation logic behind the management redesign: dense lists, the
/// board's column widths, pull request rows, tool tones, Settings search
/// and the agent tree's keyboard order. Pure, so no window.
@MainActor final class ManagementPresentationTests: XCTestCase {
    // MARK: All tasks

    func testTheSubtitleCountsWhatIsShownWhenFiltersHideSome() {
        XCTAssertEqual(TicketsView.subtitle(workspace: "athena", visible: 12, total: 12), "athena · 12 tasks")
        XCTAssertEqual(TicketsView.subtitle(workspace: "athena", visible: 4, total: 12), "athena · 4 of 12 tasks")
        XCTAssertEqual(TicketsView.taskCount(1), "1 task")
        XCTAssertEqual(TicketsView.taskCount(0), "0 tasks")
    }

    func testBoardColumnsShareTheWidthWithinTheirLimits() {
        // Four columns at the minimum, with their gaps, are 1040pt; insets and the Done rail come on top.
        XCTAssertEqual(4 * NekoLayout.boardColumnMin + 3 * NekoSpace.lg, 1040)
        let used = 2 * NekoLayout.pageInset + 3 * NekoSpace.lg + NekoLayout.boardRail + NekoSpace.lg
        XCTAssertEqual(TicketsView.columnWidth(available: used + 4 * NekoLayout.boardColumnMin, columns: 4, rail: true), NekoLayout.boardColumnMin)
        XCTAssertEqual(TicketsView.columnWidth(available: 600, columns: 4, rail: true), NekoLayout.boardColumnMin, "never narrower than the minimum")
        XCTAssertEqual(TicketsView.columnWidth(available: 4000, columns: 4, rail: true), NekoLayout.boardColumnMax, "never wider than the maximum")
        let middle = TicketsView.columnWidth(available: 1300, columns: 4, rail: true)
        XCTAssertGreaterThan(middle, NekoLayout.boardColumnMin)
        XCTAssertLessThan(middle, NekoLayout.boardColumnMax)
    }

    func testTheBoardStillHasEveryStage() {
        XCTAssertEqual(TicketsView.columns.map(\.id), ["approval", "review", "working", "upnext", "done"])
    }

    // MARK: Pull requests

    private func pullRequest(_ status: [String: JSONValue], taskStatus: String = "ReadyForReview") -> PullRequestRow {
        let task: JSONValue = .object(["id": .string("t"), "status": .string(taskStatus), "title": .string("Fix it"),
                                       "pull_request": .object(["url": .string("https://github.com/acme/athena/pull/4618"), "status": .object(status)])])
        return PullRequestRow(task: task, snapshot: .object([:]))
    }

    func testProblemsGetTheirOwnLineAndTheMetaLineStaysQuiet() {
        let row = pullRequest(["state": .string("OPEN"), "checks": .string("failing"), "failing": .array([.string("Test"), .string("Lint")]),
                               "merge_state": .string("DIRTY"), "base": .string("develop"), "review": .string("APPROVED")])
        XCTAssertEqual(row.problems.map(\.text), ["Checks failing: Test, Lint", "Conflicts with develop"])
        XCTAssertEqual(row.problems.map(\.symbol), ["xmark.octagon", "arrow.triangle.branch"])
        XCTAssertEqual(row.meta, "#4618 · Approved", "a failing check is a problem, not meta")
        let behind = pullRequest(["state": .string("OPEN"), "checks": .string("passing"), "merge_state": .string("BEHIND")])
        XCTAssertEqual(behind.problems.map(\.text), ["Behind main"])
        XCTAssertEqual(behind.meta, "#4618 · Checks passing")
    }

    func testEveryVerdictHasASymbolBesideItsColour() {
        for verdict in PullRequestRow.Verdict.allCases { XCTAssertFalse(verdict.symbol.isEmpty, "\(verdict)") }
        XCTAssertEqual(PullRequestRow.Verdict.ready.tone, .done)
        XCTAssertEqual(PullRequestRow.Verdict.needsYou.tone, .attention)
    }

    func testThePullRequestSubtitleNamesOnlyWhatIsWaiting() {
        let ready = pullRequest(["state": .string("OPEN")])
        XCTAssertEqual(ready.verdict, .ready)
        XCTAssertEqual(PullRequestRow.subtitle([ready, ready]), "2 ready")
        XCTAssertEqual(PullRequestRow.subtitle([]), "Nothing waiting on you")
    }

    // MARK: Tools

    private func connection(label: String, enabled: Bool = true, error: String = "", tools: Int = 1, discovered: Bool = true) -> JSONValue {
        .object(["label": .string(label), "enabled": .bool(enabled), "error": .string(error),
                 "tools": .array(Array(repeating: .object([:]), count: tools)), "discovered_ms": discovered ? .number(1) : .null])
    }

    func testConnectionStatusesHaveATone() {
        XCTAssertEqual(ToolsPresentation.connectionTone(connection(label: "a", enabled: false)), .neutral)
        XCTAssertEqual(ToolsPresentation.connectionTone(connection(label: "a", error: "401 unauthorized")), .attention)
        XCTAssertEqual(ToolsPresentation.connectionTone(connection(label: "a", error: "boom")), .attention)
        XCTAssertEqual(ToolsPresentation.connectionTone(connection(label: "a", tools: 0, discovered: false)), .running)
        XCTAssertEqual(ToolsPresentation.connectionTone(connection(label: "a", tools: 0)), .neutral)
        XCTAssertEqual(ToolsPresentation.connectionTone(connection(label: "a")), .done)
    }

    func testABrokenConnectionSortsFirst() {
        let sorted = ToolsPresentation.sorted([
            connection(label: "Sentry"), connection(label: "Paused", enabled: false),
            connection(label: "Linear", error: "boom"), connection(label: "Github"),
        ])
        XCTAssertEqual(sorted.map { $0["label"].string }, ["Linear", "Github", "Sentry", "Paused"])
    }

    // MARK: Settings

    func testSettingsSearchFindsSectionsAsWellAsSettings() {
        XCTAssertEqual(SettingsSearch.hubResults(for: "watch").first, .section(.watches))
        XCTAssertEqual(SettingsSearch.hubResults(for: "rules").first, .section(.rulebook))
        XCTAssertEqual(SettingsSearch.hubResults(for: "mcp").first, .section(.tools))
        XCTAssertTrue(SettingsSearch.hubResults(for: "budget").contains(.setting(.budget)))
        XCTAssertTrue(SettingsSearch.hubResults(for: "  ").isEmpty)
        XCTAssertTrue(SettingsSearch.hubResults(for: "zzzz").isEmpty)
    }

    func testEverySettingOpensASection() {
        for target in SettingsTarget.allCases {
            let section = SettingsSection(tab: target.tab)
            XCTAssertTrue(section.tab == target.tab || target.tab == .agents, "\(target) opens \(section)")
        }
        XCTAssertEqual(SettingsTarget.clipboardHistory.tab, .permissions, "the one live clipboard switch lives in Permissions")
        for section in SettingsSection.allCases { XCTAssertFalse(section.keywords.isEmpty, "\(section) needs search words") }
    }

    func testSettingsGroupTitlesAreShort() {
        XCTAssertEqual(SettingsSection.groups.map(\.title), ["Neko", "Work", "Knowledge & tools", "This Mac"])
        XCTAssertEqual(SettingsSection.search.title, "Quick panel")
    }

    // MARK: Agent tree

    private func node(_ id: String, _ kind: AgentNode.Kind = .task, state: AgentNode.State = .done, children: [AgentNode] = []) -> AgentNode {
        AgentNode(id: id, kind: kind, name: id, subtitle: "", work: "", state: state, children: children)
    }

    @MainActor func testArrowKeysFollowWhatIsOnScreen() {
        let live = node("run", .run, state: .running)
        let root = node("neko", .neko, children: [node("a", children: [live]), node("b", children: [node("b1", .run)])])
        let order = AgentTreeView.visibleOrder(root, isExpanded: AgentTreeView.expandsByDefault)
        XCTAssertEqual(order, ["neko", "a", "run", "b"], "a folded task hides its runs from the keyboard too")
        XCTAssertEqual(AgentTreeView.visibleOrder(root, isExpanded: { _ in false }), ["neko"])
    }

    func testTheTreeSummaryIsOneLine() {
        let tree = AgentTree(root: node("neko", .neko), tasks: 12, running: 2, costUSD: 3.4)
        XCTAssertEqual(AgentTreeView.summaryLine(tree), "12 tasks · 2 running · $3.40 reported")
        XCTAssertEqual(AgentTreeView.tone(.needsYou), .attention)
        XCTAssertEqual(AgentTreeView.tone(.failed), .failed)
    }

    // MARK: Shells

    func testKeycapsHaveSpokenNames() {
        XCTAssertEqual(NekoKeycap.spokenName("⌘"), "Command")
        XCTAssertEqual(NekoKeycap.spokenName("↵"), "Return")
        XCTAssertEqual(NekoKeycap.spokenName("K"), "K")
    }

    func testStagesMapToTones() {
        XCTAssertEqual(NekoTone(stage: .needsYou), .attention)
        XCTAssertEqual(NekoTone(stage: .review), .review)
        XCTAssertEqual(NekoTone(stage: .upNext), .neutral)
    }
}
