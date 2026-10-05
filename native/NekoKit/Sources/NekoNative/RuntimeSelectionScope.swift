import NekoKit

enum RuntimeSelectionScope {
    static func home(_ scope: ChatDraftScope) -> String { "home:\(scope.profileID):\(scope.workspaceID ?? "*")" }
    static func task(_ taskID: String) -> String { "task:\(taskID)" }
    static func request(conversationID: String, preferences: RuntimeSelectionPreferences) -> JSONValue {
        .object(["Workbench": .command("SetConversationRuntime", [
            "conversation_id": .string(conversationID), "preferences": preferences.wire,
        ])])
    }
}
