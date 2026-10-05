import Foundation
import NekoKit

/// Null axes are independent automatic choices. The model/provider pair is indivisible.
struct RuntimeSelectionPreferences: Equatable {
    private(set) var provider: String?
    private(set) var model: String?
    var reasoningEffort: String?
    var serviceTier: String?
    var allowPaidSpeed = false

    init(provider: String? = nil, model: String? = nil, reasoningEffort: String? = nil,
         serviceTier: String? = nil, allowPaidSpeed: Bool = false) {
        if let provider, !provider.isEmpty, let model, !model.isEmpty {
            self.provider = provider
            self.model = model
        }
        self.reasoningEffort = reasoningEffort
        self.serviceTier = serviceTier
        self.allowPaidSpeed = allowPaidSpeed
    }

    var isAutomatic: Bool { model == nil && reasoningEffort == nil && serviceTier == nil }
    var requestsExtraSpeed: Bool { serviceTier != nil && serviceTier != "default" }
    var wire: JSONValue {
        .object([
            "provider": provider.map(JSONValue.string) ?? .null,
            "model": model.map(JSONValue.string) ?? .null,
            "reasoning_effort": reasoningEffort.map(JSONValue.string) ?? .null,
            "service_tier": serviceTier.map(JSONValue.string) ?? .null,
            "allow_paid_speed": .bool(allowPaidSpeed),
        ])
    }
    static func parse(_ value: JSONValue) -> Self {
        func optional(_ key: String) -> String? { value[key].string.isEmpty ? nil : value[key].string }
        return Self(provider: optional("provider"), model: optional("model"), reasoningEffort: optional("reasoning_effort"),
                    serviceTier: optional("service_tier"), allowPaidSpeed: value["allow_paid_speed"].bool)
    }
    static func saved(in snapshot: JSONValue, conversationID: String) -> Self {
        parse(snapshot["conversation_runtime"][conversationID])
    }
    mutating func automaticModel() { provider = nil; model = nil }

    /// A model change drops only incompatible pins, with an explanation for each change.
    mutating func selectModel(provider: String, model: CatalogModel) -> [String] {
        self.provider = provider
        self.model = model.id
        var changes: [String] = []
        if let effort = reasoningEffort, !(model.effortOptions ?? []).contains(where: { $0.id == effort }) {
            reasoningEffort = nil
            changes.append("Effort reset to Auto: \(model.label) does not advertise \(effort).")
        }
        if let speed = serviceTier, speed != "default", !(model.speedOptions ?? []).contains(where: { $0.id == speed }) {
            serviceTier = nil
            changes.append("Speed reset to Auto: \(model.label) does not advertise \(speed).")
        }
        return changes
    }
}
