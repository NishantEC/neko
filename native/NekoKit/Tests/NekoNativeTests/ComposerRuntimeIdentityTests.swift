import AppKit
import SwiftUI
import XCTest
import NekoKit
@testable import NekoNative

@MainActor private final class RuntimeIdentityRecorder {
    var selections: [RuntimeSelectionState] = []
}

/// Records the actual pickers constructed by both SharedComposer layout candidates.
private struct RecordedRuntimePicker: View {
    let picker: ComposerRuntimePicker
    init(model: AppModel, selection: RuntimeSelectionState, recorder: RuntimeIdentityRecorder) {
        picker = ComposerRuntimePicker(model: model, selection: selection)
        recorder.selections.append(picker.selection)
    }
    var body: some View { picker.frame(width: 180) }
}

private struct RuntimeIdentityHost: View {
    let model: AppModel
    let conversationID: String
    let width: CGFloat
    let recorder: RuntimeIdentityRecorder
    var body: some View {
        ComposerRuntimeScope(model: model, conversationID: conversationID) { selection in
            SharedComposer(
                text: .constant(""), attachments: [], placeholder: "Message",
                accessibilityLabel: "Message", accessibilityHelp: "Test composer",
                state: .init(running: false, hasContent: false, submitting: false, blocked: false, queues: false),
                validationError: nil, onSend: {}, onStop: {}, onAttach: { _ in }, onRemove: { _ in },
                onChooseAttachments: {}, onError: { _ in }
            ) {
                Text("Ask / Plan").frame(width: 160, height: 34)
                RecordedRuntimePicker(model: model, selection: selection, recorder: recorder)
            }
        }
        .id(conversationID)
        .frame(width: width)
        .fixedSize(horizontal: false, vertical: true)
    }
}

private actor RuntimeIdentitySaveGate {
    private var continuation: CheckedContinuation<RuntimeSelectionConfirmation, any Error>?
    func save() async throws -> RuntimeSelectionConfirmation {
        try await withCheckedThrowingContinuation { continuation = $0 }
    }
    func waitUntilPending() async -> Bool {
        for _ in 0..<10000 {
            if continuation != nil { return true }
            await Task.yield()
        }
        return false
    }
    func reject() {
        continuation?.resume(throwing: NSError(domain: "Test", code: 1, userInfo: [NSLocalizedDescriptionKey: "Check failed"]))
        continuation = nil
    }
}

@MainActor final class ComposerRuntimeIdentityTests: XCTestCase {
    private func resize(_ host: NSHostingView<RuntimeIdentityHost>, model: AppModel, scope: String,
                        width: CGFloat, recorder: RuntimeIdentityRecorder) {
        host.rootView = RuntimeIdentityHost(model: model, conversationID: scope, width: width, recorder: recorder)
        host.frame = NSRect(x: 0, y: 0, width: width, height: 400)
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.03))
        host.layoutSubtreeIfNeeded()
    }

    func testResponsiveFooterSharesDraftPendingCheckAndErrorThenCleansUpScope() async throws {
        let catalog: JSONValue = .object(["AgentModels": .object(["sources": .array([
            .object(["provider": .string("sample"), "status": .string("ready"), "models": .array([
                .object(["id": .string("sample-model"), "effort_options": .array([
                    .object(["id": .string("high"), "label": .string("High")])
                ]), "speed_options": .array([])])
            ])])
        ])])])
        let model = AppModel { _ in catalog }
        let recorder = RuntimeIdentityRecorder()
        let scope = "task:responsive"
        let host = NSHostingView(rootView: RuntimeIdentityHost(model: model, conversationID: scope, width: 900, recorder: recorder))
        resize(host, model: model, scope: scope, width: 900, recorder: recorder)
        let selection = try XCTUnwrap(recorder.selections.last)
        XCTAssertGreaterThanOrEqual(recorder.selections.count, 2, "ViewThatFits constructs multiple picker candidates")
        XCTAssertTrue(recorder.selections.allSatisfy { $0 === selection })
        let wideHeight = host.fittingSize.height
        for _ in 0..<10000 {
            if selection.loaded && !selection.loading { break }
            await Task.yield()
        }
        selection.draft.reasoningEffort = "high"
        XCTAssertTrue(selection.canApply, "Catalog loading must settle before starting the test check")

        let gate = RuntimeIdentitySaveGate()
        let pending = Task { await selection.apply { _, _ in try await gate.save() } }
        let checking = await gate.waitUntilPending()
        XCTAssertTrue(checking)
        resize(host, model: model, scope: scope, width: 420, recorder: recorder)
        let narrowSelection = try XCTUnwrap(recorder.selections.last)
        XCTAssertGreaterThan(host.fittingSize.height, wideHeight, "Narrow width must select the stacked footer")
        XCTAssertTrue(narrowSelection === selection)
        XCTAssertTrue(recorder.selections.allSatisfy { $0 === selection })
        XCTAssertEqual(narrowSelection.draft.reasoningEffort, "high")
        XCTAssertTrue(narrowSelection.saving)
        XCTAssertFalse(narrowSelection.canApply)
        await narrowSelection.apply { _, _ in XCTFail("Resize must not permit a duplicate check"); return .init() }

        await gate.reject()
        await pending.value
        resize(host, model: model, scope: scope, width: 900, recorder: recorder)
        let restored = try XCTUnwrap(recorder.selections.last)
        XCTAssertTrue(restored === selection)
        XCTAssertEqual(restored.applyError, "Check failed")
        XCTAssertEqual(restored.draft.reasoningEffort, "high")
        XCTAssertFalse(restored.saving)

        resize(host, model: model, scope: "task:other", width: 900, recorder: recorder)
        let other = try XCTUnwrap(recorder.selections.last)
        XCTAssertFalse(other === selection)
        XCTAssertEqual(other.conversationID, "task:other")
        XCTAssertEqual(other.draft, .init())
        XCTAssertNil(other.applyError)
        XCTAssertFalse(other.saving)
    }
}
