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
        XCTAssertEqual(Page.tickets.title, "All agents")
        XCTAssertEqual(Page.settings.symbol, "gearshape")
    }

    func testOnlyHomeAndTicketsOwnTheToolbarRow() {
        XCTAssertEqual(Page.allCases.filter(\.ownsToolbarRow), [.home, .tickets])
    }

    func testEverySidebarPageIsListedOnceAndGalleriesAreHidden() {
        let listed = Page.sidebarGroups.flatMap(\.pages)
        XCTAssertEqual(listed.count, Set(listed).count)
        XCTAssertFalse(listed.contains(.activity) || listed.contains(.replyViews) || listed.contains(.home) || listed.contains(.tickets))
        XCTAssertEqual(Set(listed + [.home, .tickets, .activity, .replyViews]), Set(Page.allCases))
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
        XCTAssertEqual(MenuBarState.needsYou(1).accessibilityLabel, "Neko, 1 ticket needs you")
    }

    @MainActor func testMenuBarIconIsATemplate() {
        let image = MenuBarIcon.image(for: .needsYou(3))
        XCTAssertTrue(image.isTemplate)
        XCTAssertEqual(image.accessibilityDescription, "Neko, 3 tickets need you")
    }
}
