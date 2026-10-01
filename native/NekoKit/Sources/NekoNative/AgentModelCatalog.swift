import Foundation

struct AgentModel: Hashable, Identifiable {
    let provider: String
    let model: String
    var native = false
    var id: String { native ? model : "\(provider)/\(model)" }
}

enum AgentModelCatalog {
    static func parse(_ data: Data) -> [AgentModel] {
        guard let rows = try? JSONSerialization.jsonObject(with: data) as? [[String: Any]] else { return [] }
        return Array(Set(rows.compactMap { row -> AgentModel? in
            guard row["disabled"] as? Bool == false,
                  let provider = row["provider"] as? String, !provider.isEmpty,
                  let model = row["id"] as? String, !model.isEmpty,
                  let namespaced = row["namespaced"] as? String else { return nil }
            let native = row["native"] as? Bool == true
            guard namespaced == (native ? model : "\(provider)/\(model)") else { return nil }
            return AgentModel(provider: provider, model: model, native: native)
        })).sorted { ($0.provider, $0.model) < ($1.provider, $1.model) }
    }

    static func load() async -> [AgentModel] {
        await Task.detached(priority: .utility) {
            guard let executable = executable() else { return [] }
            let process = Process()
            process.executableURL = executable
            process.arguments = ["models", "live", "--json"]
            let output = Pipe()
            process.standardOutput = output
            process.standardError = FileHandle.nullDevice
            do {
                try process.run()
                let data = output.fileHandleForReading.readDataToEndOfFile()
                process.waitUntilExit()
                return process.terminationStatus == 0 ? parse(data) : []
            } catch { return [] }
        }.value
    }

    private static func executable() -> URL? {
        let files = FileManager.default
        let home = files.homeDirectoryForCurrentUser
        var directories = (ProcessInfo.processInfo.environment["PATH"] ?? "")
            .split(separator: ":").map(String.init)
        directories += [home.appendingPathComponent(".local/bin").path, "/opt/homebrew/bin", "/usr/local/bin"]
        let versions = home.appendingPathComponent(".nvm/versions/node")
        if let nodes = try? files.contentsOfDirectory(atPath: versions.path) {
            directories += nodes.sorted().reversed().map { versions.appendingPathComponent($0).appendingPathComponent("bin").path }
        }
        for directory in directories {
            let path = URL(fileURLWithPath: directory).appendingPathComponent("ocx")
            if files.isExecutableFile(atPath: path.path) { return path }
        }
        return nil
    }
}
