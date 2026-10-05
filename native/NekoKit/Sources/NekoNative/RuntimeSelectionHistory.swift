import NekoKit

struct RuntimeSelectionHistory: Equatable {
    let conversationID: String
    let runID: String
    let runtime: RuntimeSelectionPreferences
    let preferences: RuntimeSelectionPreferences
    let reason: String
    let automatic: Bool
    let selectedAtMS: Int
    let catalogReadAtMS: Int

    static func latest(in snapshot: JSONValue, conversationID: String) -> Self? {
        guard let row = snapshot["runtime_selections"].array.last(where: { $0["conversation_id"].string == conversationID }) else { return nil }
        return Self(conversationID: conversationID, runID: row["run_id"].string,
                    runtime: .parse(row["runtime"]), preferences: .parse(row["preferences"]),
                    reason: row["reason"].string, automatic: row["automatic"].bool,
                    selectedAtMS: row["selected_at_ms"].int, catalogReadAtMS: row["catalog_read_at_ms"].int)
    }
    func differs(from preferences: RuntimeSelectionPreferences) -> Bool { self.preferences != preferences }
}
