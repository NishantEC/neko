import Foundation

/// A chat message stores AI SDK `UIMessage` parts (text, reasoning,
/// `dynamic-tool`, `data-neko-*`; see docs/design/chat-on-ai-sdk.md). The native
/// app draws text and short tool summaries from them; the web UI draws the rest.
public extension JSONValue {
    /// The reply as plain text: the text parts after the last tool, or all of
    /// them when no tool follows prose.
    var chatText: String {
        let parts = self["parts"].array
        let lastTool = parts.lastIndex { $0["type"].string == "dynamic-tool" }
        func join(_ list: ArraySlice<JSONValue>) -> String {
            list.filter { $0["type"].string == "text" }.map { $0["text"].string }
                .joined(separator: "\n\n").trimmingCharacters(in: .whitespacesAndNewlines)
        }
        let tail = join(parts[(lastTool.map { $0 + 1 } ?? 0)...])
        return tail.isEmpty ? join(parts[...]) : tail
    }

    /// One short line per tool the reply used ("Ran `git status`"), in order.
    var chatActivity: [String] {
        self["parts"].array.filter { $0["type"].string == "dynamic-tool" && $0["toolName"].string != "neko_task_action" }.map { part in
            let input = part["input"]
            let failed = part["state"].string == "output-error" || (part["output"]["exit_code"].int != 0 && part["output"]["exit_code"] != .null)
            let name = part["toolName"].string
            let line: String
            switch name {
            case "Bash": line = "Ran `\(input["command"].string)`"
            case "Edit", "Write", "MultiEdit": line = "Edited \((input["file_path"].string as NSString).lastPathComponent)"
            case "Read": line = "Read \((input["file_path"].string as NSString).lastPathComponent)"
            default: line = name
            }
            return failed ? line + " · failed" : line
        }
    }

    /// What the reply remembered (the `data-neko-memory` part).
    var chatRemembered: [JSONValue] {
        self["parts"].array.filter { $0["type"].string == "data-neko-memory" }.flatMap { $0["data"]["items"].array }
    }
}
