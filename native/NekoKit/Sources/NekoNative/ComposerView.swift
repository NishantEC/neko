import AppKit
import CryptoKit
import SwiftUI

/// Plain text remains the daemon contract. Images become durable file references
/// in that text, matching crates/neko/src/attachments.rs.
struct ComposerView: NSViewRepresentable {
    @Binding var text: String
    let onSubmit: () -> Void
    let onError: (String) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.borderType = .noBorder

        let editor = ComposerTextView(frame: .zero)
        editor.delegate = context.coordinator
        editor.isRichText = false
        editor.importsGraphics = false
        editor.allowsUndo = true
        editor.isEditable = true
        editor.isSelectable = true
        editor.drawsBackground = false
        editor.font = .systemFont(ofSize: 15)
        editor.textColor = .labelColor
        editor.insertionPointColor = .labelColor
        editor.textContainerInset = NSSize(width: 0, height: 6)
        editor.textContainer?.lineFragmentPadding = 0
        editor.isVerticallyResizable = true
        editor.isHorizontallyResizable = false
        editor.autoresizingMask = [.width]
        editor.textContainer?.widthTracksTextView = true
        editor.textContainer?.containerSize = NSSize(width: 400, height: CGFloat.greatestFiniteMagnitude)
        editor.minSize = NSSize(width: 0, height: 40)
        editor.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        editor.setAccessibilityLabel("Message to Neko")
        editor.setAccessibilityHelp("Write a message. Paste an image to attach it. Command Return sends; Return adds a line.")
        editor.string = text
        editor.submit = onSubmit
        editor.reportError = onError
        scroll.documentView = editor
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let editor = scroll.documentView as? ComposerTextView else { return }
        editor.submit = onSubmit
        editor.reportError = onError
        // Never interrupt an input method's marked composition. Delegate writes
        // normally keep this equal; external clears apply after composition ends.
        if editor.string != text, !editor.hasMarkedText() {
            let selection = editor.selectedRange()
            editor.string = text
            let length = (text as NSString).length
            editor.setSelectedRange(NSRange(location: min(selection.location, length), length: 0))
            editor.needsDisplay = true
        }
    }

    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSScrollView, context: Context) -> CGSize? {
        guard let editor = nsView.documentView as? ComposerTextView,
              let container = editor.textContainer, let layout = editor.layoutManager else { return nil }
        let width = max(100, proposal.width ?? 400)
        container.containerSize = NSSize(width: width, height: CGFloat.greatestFiniteMagnitude)
        layout.ensureLayout(for: container)
        let height = layout.usedRect(for: container).height + editor.textContainerInset.height * 2 + 6
        return CGSize(width: width, height: min(180, max(42, height)))
    }

    @MainActor final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: ComposerView
        init(parent: ComposerView) { self.parent = parent }
        func textDidChange(_ notification: Notification) {
            guard let editor = notification.object as? NSTextView else { return }
            parent.text = editor.string
            editor.needsDisplay = true
        }
    }
}

@MainActor private final class ComposerTextView: NSTextView {
    var submit: (() -> Void)?
    var reportError: ((String) -> Void)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.type == .keyDown, [36, 76].contains(event.keyCode),
           event.modifierFlags.contains(.command), !hasMarkedText() {
            submit?()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func paste(_ sender: Any?) {
        let board = NSPasteboard.general
        guard let type = board.availableType(from: [.png, .tiff]), let source = board.data(forType: type) else {
            // NSTextView's plain-text path retains selection, undo and IME rules.
            super.paste(sender)
            return
        }
        do {
            let reference = try ComposerAttachmentStore.saveImage(source)
            let range = selectedRange()
            let leading = range.location == 0 ? "" : " "
            insertText(leading + reference + " ", replacementRange: range)
        } catch { reportError?(error.localizedDescription) }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        if string.isEmpty, !hasMarkedText() {
            let attributes: [NSAttributedString.Key: Any] = [
                .font: font ?? NSFont.systemFont(ofSize: 15),
                .foregroundColor: NSColor.placeholderTextColor
            ]
            ("Ask Neko, or tell it what to look after…" as NSString).draw(
                at: NSPoint(x: textContainerInset.width, y: textContainerInset.height), withAttributes: attributes)
        }
    }
}

enum ComposerAttachmentStore {
    static func saveImage(_ source: Data, root: URL? = nil) throws -> String {
        guard source.count <= 32 * 1024 * 1024,
              let representation = NSBitmapImageRep(data: source),
              representation.pixelsWide > 0, representation.pixelsHigh > 0,
              Double(representation.pixelsWide) * Double(representation.pixelsHigh) <= 64_000_000,
              let png = representation.representation(using: .png, properties: [:]),
              png.count <= 32 * 1024 * 1024 else {
            throw NSError(domain: "NekoComposer", code: 1, userInfo: [NSLocalizedDescriptionKey: "This image could not be attached. Use a PNG or TIFF under 32 MB and 64 megapixels."])
        }
        let environment = ProcessInfo.processInfo.environment
        let dataDirectory: URL
        if let root { dataDirectory = root }
        else if let override = environment["NEKO_DATA_DIR"], override.hasPrefix("/") {
            dataDirectory = URL(fileURLWithPath: override, isDirectory: true)
        } else {
            dataDirectory = URL(fileURLWithPath: environment["HOME"] ?? "/tmp", isDirectory: true)
                .appendingPathComponent("Library/Application Support/neko", isDirectory: true)
        }
        let directory = dataDirectory.appendingPathComponent("attachments", isDirectory: true)
        let hash = SHA256.hash(data: png).map { String(format: "%02x", $0) }.joined()
        let path = directory.appendingPathComponent(hash + ".png")
        let manager = FileManager.default
        try manager.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        if !manager.fileExists(atPath: path.path) {
            try png.write(to: path, options: .atomic)
            try manager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path.path)
        }
        // Deliberately retain referenced images; automatic pruning would break
        // durable conversation history. Same markdown shape as the Rust client.
        return "![Image](file://\(path.path))"
    }
}
