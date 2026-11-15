// Lists the on-screen windows of a process by name (CGWindowList): layer, alpha, bounds, on-screen.
// Usage: winlist <owner-name>. Read-only; it never touches a window.
import CoreGraphics
import Foundation

let owner = CommandLine.arguments.dropFirst().first ?? "rune-overlay"
let opts: CGWindowListOption = [.optionAll]
let list = (CGWindowListCopyWindowInfo(opts, kCGNullWindowID) as? [[String: Any]]) ?? []
for w in list where (w[kCGWindowOwnerName as String] as? String) == owner {
    let b = w[kCGWindowBounds as String] as? [String: Any] ?? [:]
    print("id=\(w[kCGWindowNumber as String] ?? "?") layer=\(w[kCGWindowLayer as String] ?? "?") alpha=\(w[kCGWindowAlpha as String] ?? "?") onscreen=\(w[kCGWindowIsOnscreen as String] ?? false) sharing=\(w[kCGWindowSharingState as String] ?? "?") bounds=\(b["X"] ?? "?"),\(b["Y"] ?? "?") \(b["Width"] ?? "?")x\(b["Height"] ?? "?") name=\(w[kCGWindowName as String] ?? "")")
}
