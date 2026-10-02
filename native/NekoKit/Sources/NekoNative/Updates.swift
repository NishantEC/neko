import Foundation
import NekoKit

/// Neko is built from source, so an update is newer commits on GitHub's
/// main branch. The build stamps its commit and source checkout into the
/// app; checking reads GitHub's public API; updating fast-forwards that
/// checkout and runs the existing installer, refusing anything that could
/// touch uncommitted work.
enum Updates {
    struct UpdateError: Error, Equatable { let message: String }
    static let repository = "NishantEC/neko"
    static let lastCheckKey = "neko.updates.lastCheck"

    struct Build: Equatable {
        let commit: String
        let source: String?
        static var current: Build? {
            guard let commit = Bundle.main.object(forInfoDictionaryKey: "NekoGitCommit") as? String,
                  commit.count == 40, commit.allSatisfy(\.isHexDigit) else { return nil }
            return Build(commit: commit, source: Bundle.main.object(forInfoDictionaryKey: "NekoSourcePath") as? String)
        }
    }

    enum Status: Equatable {
        case unknownBuild
        case upToDate
        case behind(Int, latest: String)
        case failed(String)

        var message: String {
            switch self {
            case .unknownBuild: "This build doesn’t record its commit, so it can’t compare with GitHub. Rebuild with scripts/install-native.sh."
            case .upToDate: "You’re up to date with GitHub."
            case .behind(let count, let latest): "\(count) \(count == 1 ? "update" : "updates") on GitHub (latest \(String(latest.prefix(7))))."
            case .failed(let reason): reason
            }
        }
    }

    /// GitHub's compare API: how many commits main is ahead of this build.
    static func check(session: URLSession = .shared) async -> Status {
        guard let build = Build.current else { return .unknownBuild }
        guard let url = URL(string: "https://api.github.com/repos/\(repository)/compare/\(build.commit)...main") else { return .failed("Invalid update URL.") }
        var request = URLRequest(url: url, timeoutInterval: 15)
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        do {
            let (data, response) = try await session.data(for: request)
            guard let http = response as? HTTPURLResponse else { return .failed("GitHub didn’t answer.") }
            if http.statusCode == 404 { return .failed("GitHub doesn’t know this build’s commit; it may be local only.") }
            if http.statusCode == 403 { return .failed("GitHub’s rate limit was reached. Try again in an hour.") }
            guard http.statusCode == 200 else { return .failed("GitHub answered \(http.statusCode).") }
            UserDefaults.standard.set(Date(), forKey: lastCheckKey)
            return parse(try JSONDecoder().decode(JSONValue.self, from: data))
        } catch {
            return .failed("Couldn’t reach GitHub. Check your connection.")
        }
    }

    static func parse(_ value: JSONValue) -> Status {
        let ahead = value["ahead_by"].int
        guard ahead > 0 else { return .upToDate }
        let latest = value["commits"].array.last?["sha"].string ?? ""
        return .behind(ahead, latest: latest)
    }

    /// Why an in-place update isn't safe, or nil when it is.
    static func blocker(gitStatus: String, branch: String) -> String? {
        if branch != "main" { return "The source checkout is on \(branch), not main. Switch to main, then update." }
        if !gitStatus.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return "The source checkout has uncommitted changes. Commit or stash them, then update." }
        return nil
    }

    /// Fast-forward the checkout and reinstall in a detached process (the
    /// installer stops this app). Output goes to a log Neko can show.
    static func update() -> Result<URL, UpdateError> {
        guard let build = Build.current, let source = build.source, FileManager.default.fileExists(atPath: source + "/scripts/install-native.sh") else {
            return .failure(UpdateError(message: "This build doesn’t know its source checkout. Update with: git pull && bash scripts/install-native.sh"))
        }
        let git = { (args: [String]) -> String in
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/git")
            process.arguments = ["-C", source] + args
            let pipe = Pipe()
            process.standardOutput = pipe
            process.standardError = FileHandle.nullDevice
            guard (try? process.run()) != nil else { return "" }
            process.waitUntilExit()
            return String(data: pipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8) ?? ""
        }
        if let reason = blocker(gitStatus: git(["status", "--porcelain"]), branch: git(["rev-parse", "--abbrev-ref", "HEAD"]).trimmingCharacters(in: .whitespacesAndNewlines)) {
            return .failure(UpdateError(message: reason))
        }
        let log = FileManager.default.temporaryDirectory.appendingPathComponent("neko-update-\(Int(Date().timeIntervalSince1970)).log")
        let script = "cd \(shellQuote(source)) && git pull --ff-only && bash scripts/install-native.sh && open /Applications/Neko.app"
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/nohup")
        process.arguments = ["/bin/bash", "-c", script + " > \(shellQuote(log.path)) 2>&1"]
        process.standardInput = FileHandle.nullDevice
        process.standardOutput = FileHandle.nullDevice
        process.standardError = FileHandle.nullDevice
        do { try process.run() } catch { return .failure(UpdateError(message: "Couldn’t start the update: \(error.localizedDescription)")) }
        return .success(log)
    }

    static func shellQuote(_ text: String) -> String { "'" + text.replacingOccurrences(of: "'", with: "'\\''") + "'" }
}
