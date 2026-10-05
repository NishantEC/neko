import Foundation
import NekoKit

struct CatalogRuntimeOption: Hashable, Identifiable {
    let id: String
    let label: String
    var description: String?

    static func parse(_ value: JSONValue) -> [Self]? {
        guard case .array(let rows) = value else { return nil }
        var seen = Set<String>()
        return rows.compactMap { row in
            let id = row["id"].string
            guard !id.isEmpty, seen.insert(id).inserted else { return nil }
            return Self(id: id, label: row["label"].string.isEmpty ? id : row["label"].string,
                        description: row["description"].string.isEmpty ? nil : row["description"].string)
        }
    }
}

/// One model in Neko's own catalog, read by the daemon from the runtime itself.
struct CatalogModel: Hashable, Identifiable {
    enum Access: String { case listed, checked, unavailable }
    let id: String
    let label: String
    var description: String?
    var recommended = false
    var access: Access = .listed
    var reason: String?
    var reasoningEfforts: [String] = []
    /// nil is unknown; [] explicitly means this axis is unsupported.
    var effortOptions: [CatalogRuntimeOption]?
    var defaultEffort: String?
    /// Additional tiers only; the picker supplies Normal with wire ID `default`.
    var speedOptions: [CatalogRuntimeOption]?
    var defaultSpeed: String?
    var usable: Bool { access != .unavailable }
}

/// A place models come from, keyed by the AgentRuntime provider that runs them.
struct ModelSource: Hashable, Identifiable {
    enum Status: String { case ready, signInRequired = "sign_in_required", notInstalled = "not_installed", notRunning = "not_running", error }
    let provider: String
    let label: String
    let connection: String
    let status: Status
    var note: String?
    var defaultModel: String?
    var models: [CatalogModel]
    var id: String { provider }
    var ready: Bool { status == .ready }
    /// "Default · GPT-6-Astra" when the runtime reports its default.
    var defaultTitle: String {
        guard let id = defaultModel else { return "Provider default" }
        let name = models.first(where: { $0.id == id })?.label ?? id
        return "Default · " + name
    }
}

struct ModelCatalog: Hashable {
    var sources: [ModelSource] = []
    func source(_ provider: String) -> ModelSource? {
        sources.first { $0.provider == (provider.isEmpty ? "codex" : provider) }
    }
    func model(_ provider: String, _ id: String) -> CatalogModel? {
        source(provider)?.models.first { $0.id == id }
    }

    static func parse(_ value: JSONValue) -> ModelCatalog {
        ModelCatalog(sources: value["sources"].array.compactMap { row in
            guard !row["provider"].string.isEmpty, let status = ModelSource.Status(rawValue: row["status"].string) else { return nil }
            return ModelSource(
                provider: row["provider"].string,
                label: row["label"].string,
                connection: row["connection"].string,
                status: status,
                note: row["note"].string.isEmpty ? nil : row["note"].string,
                defaultModel: row["default_model"].string.isEmpty ? nil : row["default_model"].string,
                models: row["models"].array.compactMap { model in
                    guard !model["id"].string.isEmpty else { return nil }
                    return CatalogModel(
                        id: model["id"].string,
                        label: model["label"].string.isEmpty ? model["id"].string : model["label"].string,
                        description: model["description"].string.isEmpty ? nil : model["description"].string,
                        recommended: model["recommended"].bool,
                        access: CatalogModel.Access(rawValue: model["access"].string) ?? .listed,
                        reason: model["reason"].string.isEmpty ? nil : model["reason"].string,
                        reasoningEfforts: model["reasoning_efforts"].array.map(\.string),
                        effortOptions: CatalogRuntimeOption.parse(model["effort_options"]),
                        defaultEffort: model["default_effort"].string.isEmpty ? nil : model["default_effort"].string,
                        speedOptions: CatalogRuntimeOption.parse(model["speed_options"])?.filter { $0.id != "default" },
                        defaultSpeed: model["default_speed"].string.isEmpty ? nil : model["default_speed"].string
                    )
                }
            )
        })
    }
}

struct ModelCheckResult: Equatable {
    let ok: Bool
    let unavailable: Bool
    let message: String
    static func parse(_ value: JSONValue) -> ModelCheckResult {
        ModelCheckResult(ok: value["ok"].bool, unavailable: value["unavailable"].bool, message: value["message"].string)
    }
}

/// Talks to the daemon's catalog. Listing never generates tokens; checking
/// sends one short prompt and may use quota.
enum AgentModelCatalog {
    @MainActor static func fetch(_ model: AppModel, refresh: Bool = false) async throws -> ModelCatalog {
        let reply = try await model.request(.command("AgentModels", ["refresh": .bool(refresh)]))
        guard case .array = reply["AgentModels"]["sources"] else {
            throw NSError(domain: "Neko", code: 1, userInfo: [NSLocalizedDescriptionKey: "The daemon returned an unreadable model catalog. Refresh models to try again."])
        }
        return ModelCatalog.parse(reply["AgentModels"])
    }
    @MainActor static func load(_ model: AppModel, refresh: Bool = false) async -> ModelCatalog {
        (try? await fetch(model, refresh: refresh)) ?? ModelCatalog()
    }

    /// With save, the daemon makes this the default only if the check passes.
    @MainActor static func check(_ model: AppModel, provider: String, id: String, save: Bool) async -> ModelCheckResult {
        do {
            let reply = try await model.request(.command("CheckAgentModel", [
                "runtime": .object(["provider": .string(provider), "model": .string(id)]),
                "save": .bool(save),
            ]))
            if save { await model.refresh() }
            return ModelCheckResult.parse(reply["AgentModelCheck"])
        } catch {
            return ModelCheckResult(ok: false, unavailable: false, message: error.localizedDescription)
        }
    }

    /// The composer's short label for the saved runtime.
    static func label(provider: String, model selected: String, catalog: ModelCatalog) -> String {
        if provider == "opencodex", let slash = selected.firstIndex(of: "/") {
            return "\(selected[..<slash].capitalized) · \(selected[selected.index(after: slash)...])"
        }
        let source = catalog.source(provider)
        let name = source?.label ?? (provider == "ollama" ? "Ollama" : provider == "lmstudio" ? "LM Studio" : "Codex")
        if selected.isEmpty { return name }
        return "\(name) · \(catalog.model(provider, selected)?.label ?? selected)"
    }
}
