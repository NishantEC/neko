import SwiftUI
import NekoKit

/// What a pending chat turn is doing, in words taken from its tool calls.
enum ChatActivity {
    @MainActor static func describe(_ message: JSONValue, model: AppModel) -> (activity: NekoActivity, label: String) {
        let calls = message["tool_calls"].array
        if calls.contains(where: { $0["status"].string == "awaiting_approval" }) {
            return (.needsYou, "Waiting for your OK")
        }
        if let call = calls.last(where: { ["running", "approved"].contains($0["status"].string) }) {
            let source = Watching.label(model, connection: call["connection_id"].string)
            return (.reading, "Reading \(source)…")
        }
        return calls.isEmpty ? (.thinking, "Thinking…") : (.analyzing, "Putting it together…")
    }
}
