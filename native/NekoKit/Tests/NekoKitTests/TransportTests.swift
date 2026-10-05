import Foundation
import Darwin
import Testing
@testable import NekoKit

@Test func authenticationDeadlineCoversDaemonOAuthWindowOnly() {
    let authentication: JSONValue = .object(["Workbench": .object(["Mcp": .command("Authenticate", ["connection_id": .string("server")])])])
    #expect(DaemonClient.requestTimeout(authentication) == 150)
    #expect(DaemonClient.requestTimeout(.string("Ping")) == 30)
    #expect(DaemonClient.requestTimeout(.command("Workbench", ["SetConversationRuntime": .object([:])] )) == 120)
    #expect(DaemonClient.requestTimeout(.command("Search", ["query": .string("Authenticate")])) == 30)
}

@Test func jsonPreservesTypesAndSerdeEnvelope() throws {
    let input: JSONValue = .command("Search", ["query": .string("猫"), "limit": .number(10), "flag": .bool(true), "optional": .null])
    let decoded = try JSONDecoder().decode(JSONValue.self, from: JSONEncoder().encode(input))
    #expect(decoded == input)
    #expect(decoded["Search"]["flag"] == .bool(true))
    #expect(decoded["missing"] == .null)
    #expect(JSONValue.command("Empty") == .object(["Empty": .object([:])]))
    #expect(JSONValue.number(.infinity).int == 0)
}

@Test func fragmentedFramesSkipEventsAndPartialSearch() async throws {
    let fixture = try SocketFixture()
    defer { fixture.close() }
    let server = Task.detached {
        try fixture.serve { fd, request in
            #expect(request["Request"]["request"] == .string("Ping"))
            #expect(request["Request"]["id"] == .number(1))
            try fixture.write(fd, .command("Event", ["ignored": .bool(true)]))
            try fixture.write(fd, .command("Response", ["id": .number(1), "response": .command("SearchResults", ["complete": .bool(false), "items": .array([])])]))
            try fixture.write(fd, .command("Response", ["id": .number(1), "response": .string("Pong")]))
        }
    }
    let result = try await DaemonClient(socketPath: fixture.path).request(.string("Ping"))
    #expect(result == .string("Pong"))
    try await server.value
}

@Test func rejectsOversizedFrameBeforeReadingPayload() async throws {
    let fixture = try SocketFixture()
    defer { fixture.close() }
    let server = Task.detached {
        try fixture.serve { fd, _ in
            let header: [UInt8] = [1, 0, 0, 1] // 16 MiB + 1
            _ = header.withUnsafeBytes { Darwin.write(fd, $0.baseAddress, 4) }
        }
    }
    do {
        _ = try await DaemonClient(socketPath: fixture.path).request(.string("Ping"))
        Issue.record("Expected frame bound rejection")
    } catch DaemonClientError.oversizedFrame { }
    try await server.value
}

@Test func streamYieldsFastAndCompleteSearchResponses() async throws {
    let fixture = try SocketFixture()
    defer { fixture.close() }
    let server = Task.detached {
        try fixture.serve { fd, _ in
            for complete in [false, true] {
                try fixture.write(fd, .command("Response", ["id": .number(1), "response": .command("SearchResults", ["complete": .bool(complete), "items": .array([])])]))
            }
        }
    }
    let client = DaemonClient(socketPath: fixture.path)
    var completions: [Bool] = []
    for try await response in await client.responses(.command("Search", ["query": .string("x"), "limit": .number(10), "provider": .null])) {
        completions.append(response["SearchResults"]["complete"].bool)
    }
    #expect(completions == [false, true])
    try await server.value
}

@Test func silentPeerTimesOutAndCancellationInterruptsRead() async throws {
    for cancel in [false, true] {
        let fixture = try SocketFixture()
        defer { fixture.close() }
        let server = Task.detached {
            try fixture.serve { fd, _ in
                var byte: UInt8 = 0
                _ = Darwin.read(fd, &byte, 1) // Wait for client to close.
            }
        }
        let task = Task { try await DaemonClient(socketPath: fixture.path, timeout: cancel ? 5 : 0.1).request(.string("Ping")) }
        if cancel {
            try await Task.sleep(for: .milliseconds(50))
            task.cancel()
        }
        do { _ = try await task.value; Issue.record("Expected bounded failure") }
        catch is CancellationError { #expect(cancel) }
        catch DaemonClientError.timedOut { #expect(!cancel) }
        try await server.value
    }
}

private final class SocketFixture: Sendable {
    let path: String
    let fd: Int32
    init() throws {
        path = "/tmp/neko-swift-\(UUID().uuidString).sock"
        fd = socket(AF_UNIX, SOCK_STREAM, 0)
        guard fd >= 0 else { throw DaemonClientError.system(errno) }
        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        address.sun_len = UInt8(MemoryLayout<sockaddr_un>.size)
        withUnsafeMutableBytes(of: &address.sun_path) { $0.copyBytes(from: Array(path.utf8) + [0]) }
        let result = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { Darwin.bind(fd, $0, socklen_t(MemoryLayout<sockaddr_un>.size)) }
        }
        guard result == 0, listen(fd, 4) == 0 else { Darwin.close(fd); throw DaemonClientError.system(errno) }
    }
    func close() { Darwin.close(fd); unlink(path) }
    func serve(_ body: (Int32, JSONValue) throws -> Void) throws {
        var descriptor = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
        guard poll(&descriptor, 1, 2_000) > 0 else { throw DaemonClientError.timedOut }
        let client = accept(fd, nil, nil)
        guard client >= 0 else { throw DaemonClientError.system(errno) }
        defer { Darwin.close(client) }
        var timeout = timeval(tv_sec: 2, tv_usec: 0)
        _ = setsockopt(client, SOL_SOCKET, SO_RCVTIMEO, &timeout, socklen_t(MemoryLayout<timeval>.size))
        let header = try read(client, count: 4)
        let size = header.enumerated().reduce(UInt32(0)) { $0 | UInt32($1.element) << ($1.offset * 8) }
        guard size < 1_000_000 else { throw DaemonClientError.oversizedFrame }
        let request = try JSONDecoder().decode(JSONValue.self, from: read(client, count: Int(size)))
        try body(client, request)
    }
    func read(_ client: Int32, count: Int) throws -> Data {
        var data = Data(count: count)
        var offset = 0
        while offset < count {
            let bytes = data.withUnsafeMutableBytes { Darwin.read(client, $0.baseAddress!.advanced(by: offset), count - offset) }
            guard bytes > 0 else { throw DaemonClientError.disconnected }
            offset += bytes
        }
        return data
    }
    func write(_ client: Int32, _ frame: JSONValue) throws {
        let payload = try JSONEncoder().encode(frame)
        let size = UInt32(payload.count)
        let bytes = (0..<4).map { UInt8(truncatingIfNeeded: size >> ($0 * 8)) } + Array(payload)
        for var byte in bytes {
            guard Darwin.write(client, &byte, 1) == 1 else { throw DaemonClientError.system(errno) }
        }
    }
}
