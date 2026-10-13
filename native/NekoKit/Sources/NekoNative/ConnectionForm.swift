import Foundation
import NekoKit

/// Everything typed into Tools' "Add connection" sheet, plus the OAuth client
/// ID that sheet and a connection's sign-in share.
struct ConnectionForm: Equatable {
    var label = ""
    /// A local executable rather than a hosted server.
    var local = false
    /// Available in every workspace rather than only the selected one.
    var global = false
    /// The server URL, or the executable's absolute path.
    var target = ""
    /// A JSON array of strings, for a local executable.
    var arguments = "[]"
    var workingDirectory = ""
    /// Optional JSON, stored in Keychain by the daemon.
    var credentials = ""
    /// Confirms a local executable may run. Cleared whenever what runs changes.
    var trust = false
    var clientID = ""

    struct Invalid: Error, Equatable { let message: String }

    /// A name, a destination, and trust for a local executable.
    var canSubmit: Bool {
        !label.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !target.isEmpty && (!local || trust)
    }

    /// The client ID to send with a sign-in, or null when none was given.
    static func clientIDValue(_ clientID: String) -> JSONValue {
        let id = clientID.trimmingCharacters(in: .whitespacesAndNewlines)
        return id.isEmpty ? .null : .string(id)
    }

    /// The workspace a new connection belongs to; empty means every workspace.
    func scope(in workspace: String) -> String { global || workspace.isEmpty ? "" : workspace }

    /// The daemon's connection config, or what to fix first.
    func configuration() -> Result<JSONValue, Invalid> {
        let destination = target.trimmingCharacters(in: .whitespacesAndNewlines)
        if local {
            guard destination.hasPrefix("/") else { return .failure(Invalid(message: "Use an absolute executable path.")) }
            guard let data = arguments.data(using: .utf8), let args = try? JSONDecoder().decode([String].self, from: data) else {
                return .failure(Invalid(message: "Arguments must be a JSON array of strings."))
            }
            let directory = workingDirectory.trimmingCharacters(in: .whitespacesAndNewlines)
            if !directory.isEmpty {
                var isDirectory: ObjCBool = false
                guard directory.hasPrefix("/"), FileManager.default.fileExists(atPath: directory, isDirectory: &isDirectory), isDirectory.boolValue else {
                    return .failure(Invalid(message: "Choose an existing absolute working directory."))
                }
            }
            return .success(.object(["transport": .string("stdio"), "command": .string(destination), "args": .array(args.map(JSONValue.string)), "cwd": directory.isEmpty ? .null : .string(directory)]))
        }
        guard let url = URL(string: destination), ["https", "http"].contains(url.scheme ?? ""), url.host != nil else {
            return .failure(Invalid(message: "Enter a complete HTTP or HTTPS server URL."))
        }
        return .success(.object(["transport": .string("http"), "url": .string(destination)]))
    }
}
