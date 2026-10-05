import NekoKit

/// A daemon-confirmed value and its monotonically increasing conversation revision.
struct RuntimeSelectionConfirmation: Equatable {
    var preferences = RuntimeSelectionPreferences()
    var revision: UInt64 = 0

    static func read(in snapshot: JSONValue, conversationID: String, requireRevision: Bool = false) -> Self? {
        let value = snapshot["conversation_runtime_revisions"][conversationID]
        let revision: UInt64
        if value == .null && !requireRevision {
            revision = 0 // A conversation that has never saved settings.
        } else {
            // JSONValue stores numbers as Double. Reject inexact/invalid counters instead of rounding.
            guard case .number(let number) = value, number <= 9_007_199_254_740_991,
                  let exact = UInt64(exactly: number) else { return nil }
            revision = exact
        }
        return Self(preferences: .saved(in: snapshot, conversationID: conversationID), revision: revision)
    }
}
