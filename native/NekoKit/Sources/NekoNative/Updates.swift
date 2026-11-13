import Foundation
import NekoKit

/// Neko is built from source, so an update is newer commits on GitHub's
/// main branch. The build stamps its commit into the app; checking reads
/// GitHub's public API. Applying an update is the daemon's job: it keeps its
/// own clean clone of the published repository and rebuilds from that
/// (Settings → Neko updates in the web UI), so this file never touches a
/// checkout.
enum Updates {
    struct UpdateError: Error, Equatable { let message: String }
    /// Where "newer commits" are looked for: the project's public repository
    /// unless the person points Neko at their own fork (Settings, About).
    static let defaultRepository = "NishantEC/neko" // privacy-ok: the project's own public repository
    static let repositoryKey = "neko.updates.repository"
    static var repository: String {
        normalizedRepository(UserDefaults.standard.string(forKey: repositoryKey) ?? "") ?? defaultRepository
    }
    /// `owner/name` from `owner/name`, a github.com URL (with or without
    /// `.git`) or an ssh remote; nil for anything else, so a typo falls back to
    /// the default rather than querying a strange URL.
    static func normalizedRepository(_ text: String) -> String? {
        var value = text.trimmingCharacters(in: .whitespacesAndNewlines)
        for prefix in ["https://github.com/", "http://github.com/", "git@github.com:", "github.com/"] where value.hasPrefix(prefix) { value = String(value.dropFirst(prefix.count)) }
        while value.hasSuffix("/") { value = String(value.dropLast()) }
        if value.hasSuffix(".git") { value = String(value.dropLast(4)) }
        let parts = value.split(separator: "/", omittingEmptySubsequences: false)
        let allowed = CharacterSet(charactersIn: "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789-_.")
        guard parts.count == 2, parts.allSatisfy({ !$0.isEmpty && $0.unicodeScalars.allSatisfy(allowed.contains) }) else { return nil }
        return value
    }
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
}
