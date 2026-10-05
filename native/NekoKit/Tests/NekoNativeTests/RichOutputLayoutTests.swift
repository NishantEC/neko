import AppKit
import SwiftUI
import XCTest
@testable import NekoNative

@MainActor final class RichOutputLayoutTests: XCTestCase {
    func testLongTableCellsGrowVerticallyAtBothChatWidths() {
        for width: CGFloat in [360, 700] {
            let short = tableHeight("Passed", width: width)
            let long = tableHeight(String(repeating: "The selected slot remains available after changing the address. ", count: 8), width: width)
            XCTAssertGreaterThan(long, short + 40, "Long cells must wrap instead of being clipped to one line at \(width)pt")
        }
    }

    private func tableHeight(_ value: String, width: CGFloat) -> CGFloat {
        let host = NSHostingView(rootView: NativeTableBlock(headers: ["Check", "Outcome"], rows: [["Regression", value]])
            .frame(width: width).preferredColorScheme(.dark))
        host.frame = NSRect(x: 0, y: 0, width: width, height: 1000)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.03))
        host.layoutSubtreeIfNeeded()
        return host.fittingSize.height
    }
}
