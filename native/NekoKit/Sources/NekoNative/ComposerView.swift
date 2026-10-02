import AppKit
import CryptoKit
import SwiftUI

enum ComposerLayout {
    static func textWidth(for availableWidth: CGFloat) -> CGFloat { max(100, availableWidth) }
    static func height(forTextHeight textHeight: CGFloat) -> CGFloat {
        min(160, max(40, textHeight + 12))
    }
}

struct ComposerAttachment: Identifiable, Equatable {
    let name: String
    let reference: String
    let isImage: Bool
    var id: String { reference }
}

/// Keys the composer offers to an open / or @ menu before handling them itself.
enum ComposerMenuKey { case up, down, accept, dismiss }

/// Plain text remains the daemon contract. Images become durable file references
/// in that text, matching crates/neko/src/attachments.rs.
struct ComposerView: NSViewRepresentable {
    @Binding var text: String
    let onSubmit: () -> Void
    let onInterruptAndSubmit: () -> Void
    let onAttach: (ComposerAttachment) -> Void
    let onError: (String) -> Void
    /// ⌥Return: send with the previous app's selection attached.
    var onSubmitWithContext: (() -> Void)? = nil
    /// Returns true when an open menu used the key.
    var onMenuKey: ((ComposerMenuKey) -> Bool)? = nil

    func makeCoordinator() -> Coordinator { Coordinator(parent: self) }

    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        scroll.scrollerStyle = .overlay
        scroll.borderType = .noBorder

        let editor = ComposerTextView(frame: .zero)
        editor.delegate = context.coordinator
        editor.isRichText = false
        editor.importsGraphics = false
        editor.allowsUndo = true
        editor.isEditable = true
        editor.isSelectable = true
        editor.drawsBackground = false
        editor.font = .systemFont(ofSize: 13)
        editor.textColor = .labelColor
        editor.insertionPointColor = .labelColor
        editor.textContainerInset = NSSize(width: 0, height: 6)
        editor.textContainer?.lineFragmentPadding = 0
        editor.isVerticallyResizable = true
        editor.isHorizontallyResizable = false
        editor.autoresizingMask = [.width]
        editor.textContainer?.widthTracksTextView = true
        editor.textContainer?.containerSize = NSSize(width: 100, height: CGFloat.greatestFiniteMagnitude)
        editor.minSize = NSSize(width: 0, height: 40)
        editor.maxSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        editor.setAccessibilityLabel("Message to Neko")
        editor.setAccessibilityHelp("Return sends or queues a message. Option Return sends it with the selection from the app you were just in. Shift Return adds a line. Command Return interrupts the current reply and sends. Paste or drop images and files to attach them.")
        editor.string = text
        editor.submit = onSubmit
        editor.interruptAndSubmit = onInterruptAndSubmit
        editor.submitWithContext = onSubmitWithContext
        editor.menuKey = onMenuKey
        editor.attach = onAttach
        editor.reportError = onError
        editor.registerForDraggedTypes([.fileURL, .png, .tiff])
        scroll.documentView = editor
        return scroll
    }

    func updateNSView(_ scroll: NSScrollView, context: Context) {
        context.coordinator.parent = self
        guard let editor = scroll.documentView as? ComposerTextView else { return }
        editor.submit = onSubmit
        editor.interruptAndSubmit = onInterruptAndSubmit
        editor.submitWithContext = onSubmitWithContext
        editor.menuKey = onMenuKey
        editor.attach = onAttach
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
        let width = ComposerLayout.textWidth(for: proposal.width ?? nsView.contentSize.width)
        editor.setFrameSize(NSSize(width: width, height: max(40, editor.frame.height)))
        container.containerSize = NSSize(width: width, height: CGFloat.greatestFiniteMagnitude)
        layout.ensureLayout(for: container)
        let textHeight = layout.usedRect(for: container).height
        let documentHeight = max(40, textHeight + editor.textContainerInset.height * 2)
        editor.setFrameSize(NSSize(width: width, height: documentHeight))
        return CGSize(width: width, height: ComposerLayout.height(forTextHeight: textHeight))
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
    var interruptAndSubmit: (() -> Void)?
    var submitWithContext: (() -> Void)?
    var menuKey: ((ComposerMenuKey) -> Bool)?
    var attach: ((ComposerAttachment) -> Void)?
    var reportError: ((String) -> Void)?

    override func performKeyEquivalent(with event: NSEvent) -> Bool {
        if event.type == .keyDown, [36, 76].contains(event.keyCode),
           event.modifierFlags.contains(.command), !hasMarkedText() {
            interruptAndSubmit?()
            return true
        }
        return super.performKeyEquivalent(with: event)
    }

    override func keyDown(with event: NSEvent) {
        if !hasMarkedText(), event.modifierFlags.intersection([.command, .option, .control, .shift]).isEmpty, let menuKey {
            let key: ComposerMenuKey? = switch event.keyCode {
            case 126: .up
            case 125: .down
            case 36, 76, 48: .accept
            case 53: .dismiss
            default: nil
            }
            if let key, menuKey(key) { return }
        }
        guard [36, 76].contains(event.keyCode), !hasMarkedText() else { super.keyDown(with: event); return }
        if event.modifierFlags.contains(.command) { interruptAndSubmit?(); return }
        if event.modifierFlags.contains(.shift) { insertNewline(nil); return }
        if event.modifierFlags.contains(.option), !event.modifierFlags.contains(.control), let submitWithContext { submitWithContext(); return }
        if event.modifierFlags.intersection([.option, .control]).isEmpty { submit?(); return }
        super.keyDown(with: event)
    }

    override func draggingEntered(_ sender: NSDraggingInfo) -> NSDragOperation {
        let board = sender.draggingPasteboard
        if board.canReadObject(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) || board.availableType(from: [.png, .tiff]) != nil {
            return .copy
        }
        return super.draggingEntered(sender)
    }

    override func performDragOperation(_ sender: NSDraggingInfo) -> Bool {
        if importAttachments(from: sender.draggingPasteboard) { return true }
        return super.performDragOperation(sender)
    }

    @discardableResult private func importAttachments(from board: NSPasteboard) -> Bool {
        if let urls = board.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL], !urls.isEmpty {
            for url in urls {
                do { attach?(try ComposerAttachmentStore.saveFile(url)) }
                catch { reportError?(error.localizedDescription) }
            }
            return true
        }
        if let type = board.availableType(from: [.png, .tiff]), let source = board.data(forType: type) {
            do {
                attach?(ComposerAttachment(name: "Pasted image", reference: try ComposerAttachmentStore.saveImage(source), isImage: true))
            } catch { reportError?(error.localizedDescription) }
            return true
        }
        return false
    }

    override func paste(_ sender: Any?) {
        if !importAttachments(from: .general) { super.paste(sender) }
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)
        if string.isEmpty, !hasMarkedText() {
            let attributes: [NSAttributedString.Key: Any] = [
                .font: font ?? NSFont.systemFont(ofSize: 13),
                .foregroundColor: NSColor.placeholderTextColor
            ]
            ("Ask Neko, type / for commands or @ to add context" as NSString).draw(
                at: NSPoint(x: textContainerInset.width, y: textContainerInset.height), withAttributes: attributes)
        }
    }
}

enum ComposerAttachmentStore {
    static func saveFile(_ source: URL, root: URL? = nil) throws -> ComposerAttachment {
        let resolved = source.standardizedFileURL.resolvingSymlinksInPath()
        let values = try resolved.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey])
        guard values.isRegularFile == true, let size = values.fileSize, size <= 32 * 1024 * 1024 else {
            throw NSError(domain: "NekoComposer", code: 2, userInfo: [NSLocalizedDescriptionKey: "Choose a regular file under 32 MB."])
        }
        let data = try Data(contentsOf: resolved, options: .mappedIfSafe)
        guard data.count <= 32 * 1024 * 1024 else {
            throw NSError(domain: "NekoComposer", code: 2, userInfo: [NSLocalizedDescriptionKey: "Choose a file under 32 MB."])
        }
        let name = source.lastPathComponent
        let image = ["png", "jpg", "jpeg", "tiff", "heic", "webp"].contains(source.pathExtension.lowercased())
        if image {
            return ComposerAttachment(name: name, reference: try saveImage(data, root: root), isImage: true)
        }
        let directory = attachmentDirectory(root: root)
        let hash = SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
        let suffix = source.pathExtension.lowercased().filter { $0.isASCII && ($0.isLetter || $0.isNumber) }.prefix(12)
        let path = directory.appendingPathComponent(hash + (suffix.isEmpty ? "" : ".\(suffix)"))
        let manager = FileManager.default
        try manager.createDirectory(at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        if !manager.fileExists(atPath: path.path) {
            try data.write(to: path, options: .atomic)
            try manager.setAttributes([.posixPermissions: 0o600], ofItemAtPath: path.path)
        }
        let label = name.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "[", with: "\\[")
            .replacingOccurrences(of: "]", with: "\\]")
        return ComposerAttachment(name: name, reference: "[\(label)](\(path.absoluteString))", isImage: false)
    }

    static func saveImage(_ source: Data, root: URL? = nil) throws -> String {
        guard source.count <= 32 * 1024 * 1024,
              let representation = NSBitmapImageRep(data: source),
              representation.pixelsWide > 0, representation.pixelsHigh > 0,
              Double(representation.pixelsWide) * Double(representation.pixelsHigh) <= 64_000_000,
              let png = representation.representation(using: .png, properties: [:]),
              png.count <= 32 * 1024 * 1024 else {
            throw NSError(domain: "NekoComposer", code: 1, userInfo: [NSLocalizedDescriptionKey: "This image could not be attached. Use a PNG or TIFF under 32 MB and 64 megapixels."])
        }
        let directory = attachmentDirectory(root: root)
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

    private static func attachmentDirectory(root: URL?) -> URL {
        let environment = ProcessInfo.processInfo.environment
        let dataDirectory: URL
        if let root { dataDirectory = root }
        else if let override = environment["NEKO_DATA_DIR"], override.hasPrefix("/") {
            dataDirectory = URL(fileURLWithPath: override, isDirectory: true)
        } else {
            dataDirectory = URL(fileURLWithPath: environment["HOME"] ?? "/tmp", isDirectory: true)
                .appendingPathComponent("Library/Application Support/neko", isDirectory: true)
        }
        return dataDirectory.appendingPathComponent("attachments", isDirectory: true)
    }
}
