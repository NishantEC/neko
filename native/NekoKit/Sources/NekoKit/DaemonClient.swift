import Foundation
import Darwin

public enum DaemonClientError: Error, LocalizedError, Sendable {
    case invalidSocketPath, system(Int32), disconnected, timedOut, oversizedFrame, invalidFrame
    public var errorDescription: String? {
        switch self {
        case .invalidSocketPath: "The daemon socket path is invalid."
        case .system(let code): "Daemon connection failed (system error \(code))."
        case .disconnected: "The daemon disconnected before replying."
        case .timedOut: "The daemon did not reply within the request deadline."
        case .oversizedFrame: "The daemon message exceeds the 16 MiB limit."
        case .invalidFrame: "The daemon sent an invalid message."
        }
    }
}

/// One bounded, independent socket per request. Socket work runs off the UI and
/// cooperative executors; polling checks cancellation at least every 50 ms.
public actor DaemonClient {
    public static var defaultSocketPath: String {
        let environment = ProcessInfo.processInfo.environment
        if let override = environment["NEKO_DATA_DIR"], override.hasPrefix("/") {
            return URL(fileURLWithPath: override).appendingPathComponent("neko.sock").path
        }
        return (environment["HOME"] ?? "/tmp") + "/Library/Application Support/neko/neko.sock"
    }
    private let socketPath: String
    private let timeout: TimeInterval
    public init(socketPath: String = DaemonClient.defaultSocketPath) {
        self.socketPath = socketPath
        timeout = 30
    }
    init(socketPath: String, timeout: TimeInterval) {
        self.socketPath = socketPath
        self.timeout = timeout
    }
    public func request(_ request: JSONValue) async throws -> JSONValue {
        let cancellation = RequestCancellation()
        let path = socketPath
        let duration = Self.requestTimeout(request, defaultTimeout: timeout)
        return try await withTaskCancellationHandler {
            try Task.checkCancellation()
            return try await withCheckedThrowingContinuation { continuation in
                DispatchQueue.global(qos: .userInitiated).async {
                    continuation.resume(with: Result {
                        try SocketRequest(path: path, timeout: duration, cancellation: cancellation).run(request)
                    })
                }
            }
        } onCancel: { cancellation.cancel() }
    }
    /// Search consumers receive both fast-provider and complete results.
    public func responses(_ request: JSONValue) -> AsyncThrowingStream<JSONValue, any Error> {
        let cancellation = RequestCancellation()
        let path = socketPath
        let duration = Self.requestTimeout(request, defaultTimeout: timeout)
        return AsyncThrowingStream(bufferingPolicy: .bufferingNewest(8)) { continuation in
            continuation.onTermination = { _ in cancellation.cancel() }
            DispatchQueue.global(qos: .userInitiated).async {
                do {
                    _ = try SocketRequest(path: path, timeout: duration, cancellation: cancellation).run(request) {
                        continuation.yield($0)
                    }
                    continuation.finish()
                } catch { continuation.finish(throwing: error) }
            }
        }
    }
    nonisolated static func requestTimeout(_ request: JSONValue, defaultTimeout: TimeInterval = 30) -> TimeInterval {
        // The daemon's browser OAuth callback accepts up to 120 seconds, plus
        // discovery/token exchange. Keep ordinary IPC bounded at 30 seconds.
        if request["Workbench"]["Mcp"].object["Authenticate"] != nil { return max(150, defaultTimeout) }
        // A model check runs one real, bounded (90 s) runner turn.
        if request.object["CheckAgentModel"] != nil { return max(120, defaultTimeout) }
        return defaultTimeout
    }
}

private final class RequestCancellation: @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false
    func cancel() { lock.withLock { cancelled = true } }
    func check() throws { if lock.withLock({ cancelled }) { throw CancellationError() } }
}

private struct SocketRequest {
    let path: String
    let timeout: TimeInterval
    let cancellation: RequestCancellation
    private static let maxFrame = 16 * 1024 * 1024

    func run(_ request: JSONValue, receive: (@Sendable (JSONValue) -> Void)? = nil) throws -> JSONValue {
        let deadline = ProcessInfo.processInfo.systemUptime + timeout
        try cancellation.check()
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let pathBytes = Array(path.utf8)
        guard !pathBytes.contains(0), pathBytes.count < MemoryLayout.size(ofValue: address.sun_path) else {
            throw DaemonClientError.invalidSocketPath
        }
        withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            buffer.copyBytes(from: pathBytes + [0])
        }
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        let fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw DaemonClientError.system(errno) }
        defer { close(fd) }
        guard fcntl(fd, F_SETFL, O_NONBLOCK) == 0 else { throw DaemonClientError.system(errno) }
        var noSignal: Int32 = 1
        guard setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &noSignal, socklen_t(MemoryLayout<Int32>.size)) == 0 else {
            throw DaemonClientError.system(errno)
        }
        let connected = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { connect(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
        }
        if connected != 0 {
            guard errno == EINPROGRESS else { throw DaemonClientError.system(errno) }
            try wait(fd, event: Int16(POLLOUT), deadline: deadline)
            var error: Int32 = 0
            var length = socklen_t(MemoryLayout<Int32>.size)
            guard getsockopt(fd, SOL_SOCKET, SO_ERROR, &error, &length) == 0 else { throw DaemonClientError.system(errno) }
            if error != 0 { throw DaemonClientError.system(error) }
        }
        let payload = try JSONEncoder().encode(JSONValue.command("Request", ["id": .number(1), "request": request]))
        guard payload.count <= Self.maxFrame else { throw DaemonClientError.oversizedFrame }
        let count = UInt32(payload.count)
        var framed = Data((0..<4).map { UInt8(truncatingIfNeeded: count >> ($0 * 8)) })
        framed.append(payload)
        var offset = 0
        while offset < framed.count {
            try wait(fd, event: Int16(POLLOUT), deadline: deadline)
            let sent = framed.withUnsafeBytes { send(fd, $0.baseAddress!.advanced(by: offset), framed.count - offset, 0) }
            if sent < 0 { if errno == EAGAIN || errno == EINTR { continue }; throw DaemonClientError.system(errno) }
            guard sent > 0 else { throw DaemonClientError.disconnected }
            offset += sent
        }
        while true {
            let header = try read(fd, count: 4, deadline: deadline)
            let length = header.enumerated().reduce(UInt32(0)) { $0 | UInt32($1.element) << ($1.offset * 8) }
            guard length <= Self.maxFrame else { throw DaemonClientError.oversizedFrame }
            let frame = try JSONDecoder().decode(JSONValue.self, from: read(fd, count: Int(length), deadline: deadline))
            if frame.object["Event"] != nil { continue }
            guard let response = frame.object["Response"], response["id"] == .number(1),
                  let value = response.object["response"] else { throw DaemonClientError.invalidFrame }
            try cancellation.check()
            receive?(value)
            if value["SearchResults"].object["complete"] == .bool(false) { continue }
            return value
        }
    }
    private func wait(_ fd: Int32, event: Int16, deadline: TimeInterval) throws {
        while true {
            try cancellation.check()
            let remaining = deadline - ProcessInfo.processInfo.systemUptime
            guard remaining > 0 else { throw DaemonClientError.timedOut }
            var descriptor = pollfd(fd: fd, events: event, revents: 0)
            let result = poll(&descriptor, 1, Int32(min(50, max(1, remaining * 1000))))
            if result < 0 { if errno == EINTR { continue }; throw DaemonClientError.system(errno) }
            if result > 0 {
                if descriptor.revents & Int16(POLLNVAL) != 0 { throw DaemonClientError.disconnected }
                return // recv/send reports EOF or the precise socket error.
            }
        }
    }
    private func read(_ fd: Int32, count: Int, deadline: TimeInterval) throws -> Data {
        var data = Data(count: count)
        var offset = 0
        while offset < count {
            try wait(fd, event: Int16(POLLIN), deadline: deadline)
            let received = data.withUnsafeMutableBytes { recv(fd, $0.baseAddress!.advanced(by: offset), count - offset, 0) }
            if received < 0 { if errno == EAGAIN || errno == EINTR { continue }; throw DaemonClientError.system(errno) }
            guard received > 0 else { throw DaemonClientError.disconnected }
            offset += received
        }
        return data
    }
}
