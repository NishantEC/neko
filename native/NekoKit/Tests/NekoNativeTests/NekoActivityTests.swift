import Testing
@testable import NekoNative

struct NekoActivityTests {
    private func frame(_ activity: NekoActivity, _ t: Double) -> [Double?] {
        (0..<activity.grid).flatMap { y in (0..<activity.grid).map { x in activity.intensity(x: x, y: y, t: t) } }
    }

    @Test(arguments: NekoActivity.allCases)
    func everyGlyphDrawsCellsWithinRange(_ activity: NekoActivity) {
        for t in stride(from: 0.0, through: 6.0, by: 0.05) {
            let cells = frame(activity, t).compactMap { $0 }
            #expect(!cells.isEmpty)
            #expect(cells.allSatisfy { (0...1).contains($0) })
        }
    }

    @Test(arguments: NekoActivity.allCases)
    func workingStatesMoveAndSettledStatesHoldStill(_ activity: NekoActivity) {
        let frames = stride(from: 0.0, through: 3.0, by: 0.1).map { frame(activity, $0) }
        let moves = Set(frames.map { $0.map { $0 ?? -1 } }).count > 1
        #expect(moves == activity.animates)
    }

    @Test func ticketStatusesMapToActivities() {
        #expect(NekoActivity.forTask("Planning") == .thinking)
        #expect(NekoActivity.forTask("Building") == .creating)
        #expect(NekoActivity.forTask("Reviewing") == .debugging)
        #expect(NekoActivity.forTask("AwaitingApproval") == .needsYou)
        #expect(NekoActivity.forTask("ReadyForReview") == .ready)
        #expect(NekoActivity.forTask("Completed") == .done)
        #expect(NekoActivity.forTask("Failed") == .failed)
        #expect(NekoActivity.forTask("Queued") == .idle)
    }

    @Test func ringVisitsEveryBorderCellOnce() {
        let indices = (0..<5).flatMap { y in (0..<5).compactMap { x in NekoActivity.ringIndex(x, y, size: 5) } }
        #expect(indices.sorted() == Array(0..<16))
    }
}
