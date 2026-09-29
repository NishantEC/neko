import Foundation

public enum JSONValue: Codable, Sendable, Hashable {
    case object([String: JSONValue]), array([JSONValue]), string(String), number(Double), bool(Bool), null

    public subscript(_ key: String) -> JSONValue { object[key] ?? .null }
    public var string: String { if case .string(let value) = self { value } else { "" } }
    public var bool: Bool { if case .bool(let value) = self { value } else { false } }
    public var array: [JSONValue] { if case .array(let value) = self { value } else { [] } }
    public var object: [String: JSONValue] { if case .object(let value) = self { value } else { [:] } }
    public var int: Int {
        guard case .number(let value) = self, value.isFinite,
              value >= Double(Int.min), value < Double(Int.max) else { return 0 }
        return Int(value)
    }
    public static func command(_ name: String, _ fields: [String: JSONValue] = [:]) -> JSONValue {
        .object([name: .object(fields)])
    }
    public init(from decoder: any Decoder) throws {
        let container = try decoder.singleValueContainer()
        if container.decodeNil() { self = .null }
        else if let value = try? container.decode(Bool.self) { self = .bool(value) }
        else if let value = try? container.decode(String.self) { self = .string(value) }
        else if let value = try? container.decode(Double.self) { self = .number(value) }
        else if let value = try? container.decode([JSONValue].self) { self = .array(value) }
        else { self = .object(try container.decode([String: JSONValue].self)) }
    }
    public func encode(to encoder: any Encoder) throws {
        var container = encoder.singleValueContainer()
        switch self {
        case .null: try container.encodeNil()
        case .bool(let value): try container.encode(value)
        case .string(let value): try container.encode(value)
        case .number(let value): try container.encode(value)
        case .array(let value): try container.encode(value)
        case .object(let value): try container.encode(value)
        }
    }
}
