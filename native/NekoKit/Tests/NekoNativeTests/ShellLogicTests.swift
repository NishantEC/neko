import XCTest
@testable import NekoNative
import NekoKit

/// Presentation logic behind the shell: typed pages, Settings search, quick
/// panel presentation tokens and the menu bar state. Pure, so no window.
final class ShellLogicTests: XCTestCase {
    func testPagesKeepTheirHistoricalKeys() {
        XCTAssertEqual(Page(key: "Today"), .home)
        XCTAssertEqual(Page(key: "Tools & skills"), .tools)
        XCTAssertEqual(Page(key: "Responsibilities"), .watching)
        XCTAssertNil(Page(key: "Nowhere"))
        XCTAssertEqual(Page.watching.title, "Watching")
        XCTAssertEqual(Page.tickets.title, "All tasks")
        XCTAssertEqual(Page.settings.symbol, "gearshape")
    }

    func testOnlyHomeAndTicketsOwnTheToolbarRow() {
        XCTAssertEqual(Page.allCases.filter(\.ownsToolbarRow), [.home, .tickets])
    }

    func testEverySetupPageOpensItsSettingsSection() {
        XCTAssertEqual(SettingsSection(page: .watching), .watches)
        XCTAssertEqual(SettingsSection(page: .tools), .tools)
        XCTAssertNil(SettingsSection(page: .home))
        let listed = SettingsSection.groups.flatMap(\.sections)
        XCTAssertEqual(Set(listed), Set(SettingsSection.allCases), "every section appears once in the list")
        XCTAssertEqual(listed.count, SettingsSection.allCases.count)
    }

    func testStartPageReadsTheEnvironment() {
        XCTAssertEqual(Page.start(environment: [:]), .home)
        XCTAssertEqual(Page.start(environment: ["NEKO_START_PAGE": "Design lab"]), .designLab)
        XCTAssertEqual(Page.start(environment: ["NEKO_START_PAGE": "bogus"]), .home)
    }

    func testSettingsSearchRanksTitlesBeforeKeywordsAndTabs() {
        XCTAssertEqual(SettingsSearch.results(for: "budget").first, .budget)
        XCTAssertEqual(SettingsSearch.results(for: "  Dark ").first, .appearance)
        XCTAssertEqual(SettingsSearch.results(for: "claude").first, .runtime)
        XCTAssertEqual(SettingsSearch.results(for: "full disk").first, .permissions)
        XCTAssertTrue(SettingsSearch.results(for: "").isEmpty)
        XCTAssertTrue(SettingsSearch.results(for: "zzzz").isEmpty)
        // Title prefix beats a keyword that merely contains the text.
        XCTAssertEqual(SettingsSearch.score(.model, "model"), 0)
        XCTAssertEqual(SettingsSearch.score(.budget, "cost"), 2)
    }

    func testEverySettingBelongsToARealTab() {
        for target in SettingsTarget.allCases {
            XCTAssertTrue(SettingsTab.allCases.contains(target.tab), "\(target)")
            XCTAssertFalse(target.keywords.isEmpty, "\(target) needs search words")
        }
    }

    func testAStaleDismissCannotCloseTheNextShow() {
        var presentation = PaletteGeneration()
        let firstShow = presentation.advance()
        let dismissal = presentation.advance()
        XCTAssertFalse(presentation.isCurrent(firstShow))
        let reopened = presentation.advance()
        XCTAssertFalse(presentation.isCurrent(dismissal), "the old fade must not order out the reopened panel")
        XCTAssertTrue(presentation.isCurrent(reopened))
    }

    func testMenuBarStatePrefersWhatNeedsTheUser() {
        func summary(_ statuses: [String]) -> StatusSummary {
            StatusSummary(snapshot: .object(["tasks": .array(statuses.map { .object(["status": .string($0)]) })]))
        }
        XCTAssertEqual(MenuBarState(summary([])), .idle)
        XCTAssertEqual(MenuBarState(summary(["Building"])), .working)
        XCTAssertEqual(MenuBarState(summary(["Building", "AwaitingApproval", "Failed"])), .needsYou(2))
        XCTAssertEqual(MenuBarState.needsYou(1).accessibilityLabel, "Neko, 1 task needs you")
    }

    func testCommandsDropTheWorktreeCdButKeepAnyOtherDirectory() {
        let wt = "/Users/x/Library/Application Support/neko/task-worktrees/abc"
        XCTAssertEqual(TicketThread.shortCommand("cd \"\(wt)\" && yarn test src"), "yarn test src")
        XCTAssertEqual(TicketThread.shortCommand("cd /tmp/other && yarn test"), "cd /tmp/other && yarn test")
        XCTAssertEqual(TicketThread.shortCommand("/bin/zsh -lc 'cd \(wt) && git diff'"), "git diff")
    }

    @MainActor func testMenuBarIconIsATemplate() {
        let image = MenuBarIcon.image(for: .needsYou(3))
        XCTAssertTrue(image.isTemplate)
        XCTAssertEqual(image.accessibilityDescription, "Neko, 3 tasks need you")
    }
}

/// The single status model and how tasks read.
final class TaskStageTests: XCTestCase {
    private func task(_ status: String, id: String = "t", title: String = "TypeError: x is undefined", display: String? = nil) -> JSONValue {
        var fields: [String: JSONValue] = ["id": .string(id), "status": .string(status), "title": .string(title)]
        if let display { fields["display_title"] = .string(display) }
        return .object(fields)
    }

    func testEveryStatusHasExactlyOneStageAndQueuedIsNotRunning() {
        XCTAssertEqual(TaskStage(status: "Queued"), .upNext)
        XCTAssertEqual(TaskStage(status: "Building"), .running)
        XCTAssertEqual(TaskStage(status: "AwaitingApproval"), .needsYou)
        XCTAssertEqual(TaskStage(status: "Failed"), .needsYou)
        XCTAssertEqual(TaskStage(status: "ReadyForReview"), .review)
        XCTAssertEqual(TaskStage(status: "Completed"), .done)
    }

    func testCountsComeFromOnePlace() {
        let counts = TaskCounts(tasks: ["Queued", "Queued", "Building", "Failed", "ReadyForReview", "Completed"].map { task($0) })
        XCTAssertEqual(counts[.upNext], 2)
        XCTAssertEqual(counts[.running], 1)
        XCTAssertEqual(counts.waitingOnYou, 2)
    }

    func testATaskReadsByItsHumanTitleWithSourceRepoAndPullRequest() {
        let t: JSONValue = .object(["id": .string("t1"), "status": .string("ReadyForReview"), "title": .string("TypeError: x"), "display_title": .string("Fix crash when saving basic info"),
                                    "pull_request": .object(["url": .string("https://github.com/acme/athena/pull/4616")])])
        let snapshot: JSONValue = .object([
            "task_roots": .object(["t1": .string("/Users/x/hme/athena")]),
            "mcp": .object([
                "connections": .array([.object(["id": .string("c"), "label": .string("sentry")])]),
                "sources": .array([.object(["task_id": .string("t1"), "connection_id": .string("c"), "external_id": .string("CONNECT-PROD-P5E")])]),
            ]),
        ])
        let text = TaskText(task: t, snapshot: snapshot)
        XCTAssertEqual(text.title, "Fix crash when saving basic info")
        XCTAssertEqual(text.sourceTitle, "TypeError: x")
        XCTAssertEqual(text.detail, "athena · #4616 · Sentry", "a long machine id is left out")
        let linear: JSONValue = .object(["mcp": .object([
            "connections": .array([.object(["id": .string("c"), "label": .string("linear-personal")])]),
            "sources": .array([.object(["task_id": .string("t1"), "connection_id": .string("c"), "external_id": .string("HEA-1427")])]),
        ])])
        XCTAssertEqual(TaskText(task: t, snapshot: linear).source, "Linear · HEA-1427", "a short id a person quotes is kept")
        XCTAssertEqual(TaskText(task: task("Queued"), snapshot: .object([:])).title, "TypeError: x is undefined", "falls back to the source title")
        XCTAssertEqual(TaskText.sourceName("linear-personal"), "Linear")
    }

    func testAConnectionSaysWhereItRunsOrWhatIsWrong() {
        func connection(_ config: [String: JSONValue], error: String = "") -> JSONValue {
            .object(["config": .object(config), "error": .string(error)])
        }
        XCTAssertEqual(ToolsPresentation.connectionDetail(connection(["transport": .string("http"), "url": .string("https://mcp.linear.app/mcp")])), "mcp.linear.app")
        XCTAssertEqual(ToolsPresentation.connectionDetail(connection(["transport": .string("http"), "url": .string("http://127.0.0.1:29979/mcp")])), "A server on this Mac")
        XCTAssertEqual(ToolsPresentation.connectionDetail(connection(["transport": .string("stdio"), "command": .string("/usr/bin/python3"), "args": .array([.string("/x/admin_mcp_shim.py")])])), "Runs on this Mac · admin_mcp_shim.py")
        XCTAssertEqual(ToolsPresentation.connectionDetail(connection([:], error: "Source MCP definition changed; review it again before using tools")), "Changed since you approved it. Open to review.")
        XCTAssertEqual(ToolsPresentation.connectionDetail(connection([:], error: "MCP authentication failed. Check the server.")), "Signed out. Open to sign in again.")
    }
}

final class StopReasonTests: XCTestCase {
    func testParkingAndTriageNotesAreNeverTheStopReason() {
        let task: JSONValue = .object(["status": .string("Failed"), "events": .array([
            .object(["role": .string("supervisor"), "message": .string("Run tool scope is paused, unavailable, or changed")]),
            .object(["role": .string("host"), "message": .string("Parked: its unfinished work is saved on branch codex/neko-x")]),
        ])])
        XCTAssertEqual(TicketPresentation.stopReason(task), "A tool connection it relied on was paused or changed while it worked. Start it again to retry.")
    }
}

final class MachineStatusTests: XCTestCase {
    func testTheStripSaysWhyWorkIsHeldOrLimited() {
        XCTAssertEqual(MachineStatus(health: .null).title, "Reading the Mac…")
        XCTAssertEqual(MachineStatus(health: .object(["load": .number(6.4)])).title, "Mac OK · load 6")
        XCTAssertEqual(MachineStatus(health: .object(["cap": .number(1), "night": .bool(true)])).title, "Night: one task at a time")
        let held = MachineStatus(health: .object(["holding": .string("Only 9 GB of disk is free."), "cap": .number(1)]))
        XCTAssertEqual(held.level, .holding)
        XCTAssertEqual(held.title, "New work is on hold")
    }
}
