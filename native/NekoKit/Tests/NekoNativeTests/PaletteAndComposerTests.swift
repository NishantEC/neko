import AppKit
import SwiftUI
import XCTest
import NekoKit

@MainActor final class NativeNavigationParityTests: XCTestCase {
    func testPhysicalRecorderMapsExtendedKeysWithoutLayoutTranslation() {
        for key in ["KeyA", "F12", "PageUp", "BracketLeft", "NumpadEnter", "IntlYen"] {
            XCTAssertEqual(NativeHotkeyCodes.name(for: UInt16(NativeHotkeyCodes.code(key)!)), key)
        }
        XCTAssertNil(NativeHotkeyCodes.name(for: 65535))
    }
    func testSlashScopesOnlyRootSearch() {
        XCTAssertEqual(PaletteSearchScope.request(query: "/theme", mode: nil)["Search"]["provider"], .string("command"))
        XCTAssertEqual(PaletteSearchScope.request(query: "/", mode: nil)["Search"]["query"], .string(""))
        XCTAssertEqual(PaletteSearchScope.request(query: "/path", mode: "clipboard")["Search"]["query"], .string("/path"))
        XCTAssertEqual(PaletteSearchScope.request(query: "/path", mode: "clipboard")["Search"]["provider"], .string("clipboard"))
    }
    func testPageNavigationPreservesModifiedEditingAndComposition() {
        XCTAssertEqual(PaletteKeyboardRouting.action(key: 116, modifiers: [], composing: false, presentationOpen: false), .pageUp)
        XCTAssertEqual(PaletteKeyboardRouting.action(key: 119, modifiers: [], composing: false, presentationOpen: false), .last)
        XCTAssertNil(PaletteKeyboardRouting.action(key: 115, modifiers: .shift, composing: false, presentationOpen: false))
        XCTAssertNil(PaletteKeyboardRouting.action(key: 121, modifiers: [], composing: true, presentationOpen: false))
    }
}
@testable import NekoNative

final class PaletteAndComposerTests: XCTestCase {
    func testSettingsIsAvailableAsMainWorkspacePage() {
        XCTAssertTrue(PageInfo.groups.contains { $0.keys.contains("Settings") })
        XCTAssertEqual(PageInfo.icon("Settings"), "gearshape")
    }
    @MainActor func testComposerTextContainerUsesAvailableWidth() throws {
        let model = AppModel { _ in .null }
        model.loadingSetup = false
        let host = NSHostingView(rootView: WorkspaceView(model: model).frame(width: 1400, height: 800))
        host.frame = NSRect(x: 0, y: 0, width: 1400, height: 800)
        host.layoutSubtreeIfNeeded()
        func textView(in root: NSView) -> NSTextView? {
            if let root = root as? NSTextView { return root }
            return root.subviews.compactMap(textView).first
        }
        let editor = try XCTUnwrap(textView(in: host))
        XCTAssertGreaterThan(editor.enclosingScrollView?.frame.width ?? 0, 650, "The scroll view must fill the composer")
        editor.insertText("hasdghjsaghjdgsjahgddhjsag", replacementRange: NSRange(location: 0, length: 0))
        host.layoutSubtreeIfNeeded()
        RunLoop.main.run(until: Date().addingTimeInterval(0.05))
        host.layoutSubtreeIfNeeded()
        XCTAssertGreaterThan(editor.frame.width, 650, "The NSTextView itself must fill the composer")
        XCTAssertGreaterThan(editor.textContainer?.containerSize.width ?? 0, 650)
        let range = try XCTUnwrap(editor.layoutManager?.lineFragmentRect(forGlyphAt: 0, effectiveRange: nil))
        XCTAssertGreaterThan(range.width, 650)
    }
    @MainActor func testActionsKeyboardMovesInsideMenuAndKeepsDestructiveConfirmation() {
        let model = AppModel { _ in .string("Activated") }
        let state = PaletteState(model: model, search: { _ in AsyncThrowingStream { $0.finish() } })
        state.rows = [.object(["kind": .string("clipboard"), "id": .string("entry"), "actions": .array([
            .object(["id": .string("copy"), "destructive": .bool(false)]),
            .object(["id": .string("delete"), "destructive": .bool(true)])
        ])])]
        state.showActions = true
        state.moveKeyboardSelection(1)
        XCTAssertEqual(state.selectedAction, 1)
        XCTAssertEqual(state.selected, 0)
        state.activateKeyboardSelection()
        XCTAssertEqual(state.pendingAction?["id"], .string("delete"))
        XCTAssertEqual(PaletteKeyboardRouting.action(key: 125, modifiers: [], composing: false, presentationOpen: true, actionsOpen: true), .down)
        XCTAssertNil(PaletteKeyboardRouting.action(key: 125, modifiers: [], composing: true, presentationOpen: true, actionsOpen: true))
    }
    @MainActor func testQueryChangeRemovesStaleActionsAndModeEscapeRestoresQuery() {
        let model = AppModel { _ in .string("Activated") }
        let state = PaletteState(model: model, search: { _ in AsyncThrowingStream { $0.finish() } })
        state.rows = [.object(["id": .string("old"), "kind": .string("app")])]
        state.query = "clipboard history"
        XCTAssertEqual(state.selection, .null)
        XCTAssertTrue(state.rows.isEmpty)
        state.enterMode("clipboard")
        state.query = "invoice"
        state.escape()
        XCTAssertNil(state.mode)
        XCTAssertEqual(state.query, "clipboard history")
        state.cancelSearch()
    }
    @MainActor func testOwnedChildWindowsRemainInPaletteFamily() {
        _ = NSApplication.shared
        let root = NSPanel(contentRect: .zero, styleMask: .borderless, backing: .buffered, defer: true)
        let child = NSPanel(contentRect: .zero, styleMask: .borderless, backing: .buffered, defer: true)
        let unrelated = NSPanel(contentRect: .zero, styleMask: .borderless, backing: .buffered, defer: true)
        root.addChildWindow(child, ordered: .above)
        defer { root.removeChildWindow(child) }
        XCTAssertTrue(PaletteWindowOwnership.contains(child, root: root))
        XCTAssertTrue(PaletteWindowOwnership.contains(root, root: root))
        XCTAssertFalse(PaletteWindowOwnership.contains(unrelated, root: root))
    }
    func testDaemonRecoveryBackoffIsBoundedAndResetsAfterSuccess() {
        var backoff = DaemonRecoveryBackoff()
        XCTAssertEqual((0..<8).map { _ in backoff.failed() }, [2, 4, 8, 16, 32, 60, 60, 60])
        backoff.recovered()
        XCTAssertEqual(backoff.failed(), 2)
    }
    func testPaletteLeavesCompositionAndModifiedEditingKeysToAppKit() {
        for key: UInt16 in [36, 53, 125, 126] {
            XCTAssertNil(PaletteKeyboardRouting.action(key: key, modifiers: [], composing: true, presentationOpen: false))
            XCTAssertNil(PaletteKeyboardRouting.action(key: key, modifiers: .shift, composing: false, presentationOpen: false))
            XCTAssertNil(PaletteKeyboardRouting.action(key: key, modifiers: .option, composing: false, presentationOpen: false))
            XCTAssertNil(PaletteKeyboardRouting.action(key: key, modifiers: [], composing: false, presentationOpen: true))
        }
        XCTAssertEqual(PaletteKeyboardRouting.action(key: 125, modifiers: [], composing: false, presentationOpen: false), .down)
        XCTAssertEqual(PaletteKeyboardRouting.action(key: 36, modifiers: [], composing: false, presentationOpen: false), .activate)
        XCTAssertEqual(PaletteKeyboardRouting.action(key: 40, modifiers: .command, composing: false, presentationOpen: false), .actions)
        XCTAssertNil(PaletteKeyboardRouting.action(key: 40, modifiers: [.command, .shift], composing: false, presentationOpen: false))
    }

    func testExtendedPhysicalHotkeyVocabulary() {
        XCTAssertEqual(NativeHotkeyCodes.code("F20"), 90)
        XCTAssertEqual(NativeHotkeyCodes.code("ArrowLeft"), 123)
        XCTAssertEqual(NativeHotkeyCodes.code("NumpadEnter"), 76)
        XCTAssertEqual(NativeHotkeyCodes.code("BracketLeft"), 33)
        XCTAssertEqual(NativeHotkeyCodes.code("Backspace"), 51)
        XCTAssertNil(NativeHotkeyCodes.code("Unknown"))
    }

    func testGenericPreviewAndMeterStayWithinBounds() {
        let row: JSONValue = .object(["kind": .string("terminal"), "id": .string("opaque"), "preview": .string("Actual terminal output"), "meter": .object(["fraction": .number(2)])])
        XCTAssertEqual(PalettePresentation.preview(row), "Actual terminal output")
        XCTAssertEqual(PalettePresentation.meterFraction(row["meter"]), 1)
        XCTAssertEqual(PalettePresentation.meterFraction(.object(["fraction": .number(-1)])), 0)
        XCTAssertEqual(PalettePresentation.preview(.object(["kind": .string("clipboard"), "id": .string("Clipboard content")])), "Clipboard content")
        XCTAssertEqual(PalettePresentation.preview(.object(["kind": .string("file"), "id": .string("opaque")])), "")
    }

    func testPastedTiffBecomesOneDurablePngAndRejectsInvalidData() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("neko-composer-test-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        let bitmap = try XCTUnwrap(NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: 2, pixelsHigh: 2, bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false, colorSpaceName: .deviceRGB, bytesPerRow: 8, bitsPerPixel: 32))
        bitmap.bitmapData!.initialize(repeating: 128, count: 16)
        let tiff = try XCTUnwrap(bitmap.tiffRepresentation)
        let first = try ComposerAttachmentStore.saveImage(tiff, root: root)
        XCTAssertEqual(first, try ComposerAttachmentStore.saveImage(tiff, root: root))
        let files = try FileManager.default.contentsOfDirectory(at: root.appendingPathComponent("attachments"), includingPropertiesForKeys: nil)
        XCTAssertEqual(files.count, 1)
        let file = try XCTUnwrap(files.first)
        XCTAssertEqual(first, "![Image](file://\(root.appendingPathComponent("attachments").appendingPathComponent(file.lastPathComponent).path))")
        XCTAssertEqual(file.pathExtension, "png")
        XCTAssertEqual(Array(try Data(contentsOf: file).prefix(8)), [137, 80, 78, 71, 13, 10, 26, 10])
        XCTAssertThrowsError(try ComposerAttachmentStore.saveImage(Data("not an image".utf8), root: root))
    }

    func testFileAttachmentCopiesBytesAndRejectsDirectory() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent("neko-file-test-" + UUID().uuidString)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let source = root.appendingPathComponent("report.txt")
        try Data("hello".utf8).write(to: source)
        let attachment = try ComposerAttachmentStore.saveFile(source, root: root)
        XCTAssertEqual(attachment.name, "report.txt")
        XCTAssertEqual(attachment.isImage, false)
        XCTAssertTrue(attachment.reference.contains("file://"))
        XCTAssertThrowsError(try ComposerAttachmentStore.saveFile(root, root: root))
    }
}
