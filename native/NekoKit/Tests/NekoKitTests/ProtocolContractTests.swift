import Foundation
import Darwin
import Testing
@testable import NekoKit

@Test(.enabled(if: ProcessInfo.processInfo.environment["NEKO_TEST_DAEMON"] != nil,
               "Set NEKO_TEST_DAEMON to an explicitly selected daemon executable."))
func realDaemonPersistsNativeProtocolCommands() async throws {
    let executable = try #require(ProcessInfo.processInfo.environment["NEKO_TEST_DAEMON"])
    var template = Array("/tmp/neko-native-contract-XXXXXX".utf8CString)
    let root = try #require(template.withUnsafeMutableBufferPointer { pointer in
        mkdtemp(pointer.baseAddress!).map { String(cString: $0) }
    })
    let rootURL = URL(fileURLWithPath: root)
    defer { try? FileManager.default.removeItem(at: rootURL) }
    let folders = [rootURL.appendingPathComponent("notes"), rootURL.appendingPathComponent("design")]
    for folder in folders { try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true) }
    let client = DaemonClient(socketPath: root + "/neko.sock", timeout: 3)
    var process = try launchContractDaemon(executable, root: root)
    defer { stopContractDaemon(process) }
    try await waitForContractDaemon(client, process: process)
    #expect(try await client.request(.string("Ping")) == .string("Pong"))
    let empty = try await contractCommand(client, .string("Snapshot"))
    #expect(empty["workspaces"].array.isEmpty)

    // Disabling only: never read or enable the user's clipboard in this test.
    let clipboard = try await client.request(.command("SetClipboardHistoryEnabled", ["enabled": .bool(false)]))
    #expect(clipboard["ClipboardHistoryEnabled"]["enabled"] == .bool(false))
    let onboarding = try await client.request(.command("SetOnboardingComplete", ["completed": .bool(true)]))
    #expect(onboarding["OnboardingState"]["completed"] == .bool(true))

    let workspace: JSONValue = .object(["id": .string(""), "name": .string("Native contract"), "repository": .string(folders[0].path), "instructions": .string("Read-only integration fixture"), "away_enabled": .bool(false)])
    let saved = try await contractCommand(client, .command("SaveWorkspaceWithFolders", ["workspace": workspace, "folders": .array(folders.map { .string($0.path) })]))
    let workspaceID = try #require(saved["workspaces"].array.first?["id"].string)
    #expect(!workspaceID.isEmpty)
    let canonicalFolders = try folders.map { folder in
        let canonical = try #require(realpath(folder.path, nil))
        defer { free(canonical) }
        return String(cString: canonical)
    }
    #expect(saved["workspace_folders"][workspaceID].array.map(\.string) == canonicalFolders)
    #expect(!FileManager.default.fileExists(atPath: folders[0].appendingPathComponent(".git").path))

    let profiled = try await contractCommand(client, .object(["AgentProfiles": .command("Save", ["profile": .object(["id": .string(""), "name": .string("Research"), "instructions": .string("Summarize only")])])]))
    let profileID = try #require(profiled["agent_profiles"]["profiles"].array.first { $0["name"].string == "Research" }?["id"].string)
    _ = try await contractCommand(client, .object(["AgentProfiles": .command("AssignWorkspace", ["workspace_id": .string(workspaceID), "profile_id": .string(profileID)])]))
    let assigned = try await contractCommand(client, .object(["AgentProfiles": .command("SetReadGrant", ["reader_id": .string(profileID), "source_id": .string("default"), "allowed": .bool(true)])]))
    #expect(assigned["agent_profiles"]["assignments"].array.contains { $0["workspace_id"].string == workspaceID && $0["profile_id"].string == profileID })
    #expect(assigned["agent_profiles"]["read_grants"].array.contains { $0["reader_id"].string == profileID && $0["source_id"].string == "default" })

    let proposed = try await contractCommand(client, .object(["DecisionContext": .command("SavePreference", [
        "workspace_id": .string(workspaceID), "id": .string(""), "expected_version": .null,
        "applicability": .object(["terms": .array([.string("native navigation")]), "task_ids": .array([])]),
        "instruction": .string("Use full-page agent conversations for native navigation."),
        "supporting_record_ids": .array([]), "exceptions": .array([.string("quick panel")])
    ])]))
    let preference = try #require(proposed["working_preferences"].array.first)
    #expect(preference["state"] == .string("proposed"))
    let confirmed = try await contractCommand(client, .object(["DecisionContext": .command("KeepPreference", [
        "workspace_id": .string(workspaceID), "id": preference["id"], "expected_version": preference["version"]
    ])]))
    #expect(confirmed["working_preferences"].array.first?["state"] == .string("confirmed"))
    let stale = try await client.request(.object(["Workbench": .object(["DecisionContext": .command("DismissPreference", [
        "workspace_id": .string(workspaceID), "id": preference["id"], "expected_version": preference["version"]
    ])])]))
    #expect(stale["Error"] != .null)

    let now = JSONValue.number(Double(Int64(Date().timeIntervalSince1970 * 1000)))
    let memory: JSONValue = .object(["id": .string(""), "agent_profile_id": .string(profileID), "workspace_id": .string(workspaceID), "kind": .string("workspace"), "text": .string("Native protocol persistence fixture"), "source": .string("user"), "created_at_ms": now, "updated_at_ms": now])
    let remembered = try await contractCommand(client, .command("SaveMemory", ["entry": memory]))
    #expect(remembered["memory"].array.contains { $0["text"].string == "Native protocol persistence fixture" })

    let schedule: JSONValue = .object(["id": .string(""), "name": .string("Paused contract schedule"), "prompt": .string("Read-only test; never run"), "workspace_id": .string(workspaceID), "rule": .string("FREQ=DAILY;BYHOUR=9;BYMINUTE=0"), "timezone": .string("UTC"), "anchor_ms": now, "enabled": .bool(true)])
    let scheduled = try await contractCommand(client, .object(["Schedules": .command("Save", ["schedule": schedule])]))
    let savedSchedule = try #require(scheduled["schedules"].array.first { $0["name"].string == "Paused contract schedule" })
    #expect(savedSchedule["enabled"] == .bool(false))
    #expect(savedSchedule["anchor_ms"] == now)

    stopContractDaemon(process)
    process = try launchContractDaemon(executable, root: root)
    try await waitForContractDaemon(client, process: process)
    let restored = try await contractCommand(client, .string("Snapshot"))
    #expect(restored["workspace_folders"][workspaceID] == saved["workspace_folders"][workspaceID])
    #expect(restored["agent_profiles"] == scheduled["agent_profiles"])
    #expect(restored["memory"] == scheduled["memory"])
    #expect(restored["working_preferences"] == confirmed["working_preferences"])
    #expect(restored["schedules"] == scheduled["schedules"])
    #expect((try await client.request(.string("GetOnboardingState")))["OnboardingState"]["completed"] == .bool(true))
    #expect((try await client.request(.string("GetClipboardHistoryEnabled")))["ClipboardHistoryEnabled"]["enabled"] == .bool(false))
}

private func contractCommand(_ client: DaemonClient, _ command: JSONValue) async throws -> JSONValue {
    let reply = try await client.request(.object(["Workbench": command]))
    if reply["Error"] != .null {
        throw NSError(domain: "NativeProtocolContract", code: 1, userInfo: [NSLocalizedDescriptionKey: reply["Error"]["message"].string])
    }
    return try #require(reply.object["Workbench"])
}

private func launchContractDaemon(_ executable: String, root: String) throws -> Process {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    var environment = ProcessInfo.processInfo.environment
    environment["NEKO_DATA_DIR"] = root
    environment["NEKO_LEGACY_AGENTS"] = "0"
    process.environment = environment
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    try process.run()
    return process
}

private func stopContractDaemon(_ process: Process) {
    guard process.isRunning else { return }
    process.terminate()
    for _ in 0..<100 {
        if !process.isRunning { return }
        usleep(10_000)
    }
    // Exact child PID owned by this fixture; never signal another daemon.
    if process.isRunning { kill(process.processIdentifier, SIGKILL); process.waitUntilExit() }
}

private func waitForContractDaemon(_ client: DaemonClient, process: Process) async throws {
    for _ in 0..<100 {
        guard process.isRunning else { throw NSError(domain: "NativeProtocolContract", code: 2, userInfo: [NSLocalizedDescriptionKey: "Fixture daemon exited during startup."]) }
        if let reply = try? await client.request(.string("Ping")), reply == .string("Pong") { return }
        try await Task.sleep(for: .milliseconds(100))
    }
    throw NSError(domain: "NativeProtocolContract", code: 3, userInfo: [NSLocalizedDescriptionKey: "Fixture daemon never became ready."])
}
