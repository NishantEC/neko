import Foundation

/// Counts quick panel presentations so work scheduled by an earlier show or
/// hide (a resign-key check, an outside-click task, a fade's completion)
/// can tell it is stale and do nothing. Without it, a dismiss queued while
/// the panel was closing can close the panel the user just reopened.
struct PaletteGeneration: Equatable {
    private(set) var generation = 0
    /// Starts a new show or hide and returns its token.
    mutating func advance() -> Int { generation += 1; return generation }
    /// True while nothing newer has started since `token` was issued.
    func isCurrent(_ token: Int) -> Bool { token == generation }
}
