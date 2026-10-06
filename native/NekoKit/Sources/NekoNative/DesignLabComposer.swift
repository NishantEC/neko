import SwiftUI

struct DesignLabComposer: View {
    @Bindable var preview: DesignLabModel

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if preview.layout == .separate {
                DesignLabControls(preview: preview, expanded: true)
                    .padding(.horizontal, 22).padding(.vertical, 16)
                Divider().overlay { Color.primary.opacity(0.025) }
            }
            VStack(alignment: .leading, spacing: 16) {
                if let name = preview.attachmentName {
                    ComposerAttachmentChip(name: name, isImage: true, detail: "Reference · preview") {
                        preview.attachmentName = nil
                    }.fixedSize(horizontal: false, vertical: true)
                }
                ComposerView(
                    text: $preview.draft,
                    onSubmit: preview.send, onInterruptAndSubmit: preview.send,
                    onAttach: { preview.attachmentName = $0.name },
                    onError: { preview.error = $0 },
                    placeholder: "Try a message in this preview…",
                    accessibilityLabel: "Design preview message",
                    accessibilityHelp: "Return adds a message to the local preview. Shift Return adds a new line. No agent is started."
                )
                if let error = preview.error {
                    Text(error).font(.caption).foregroundStyle(NekoStyle.amber)
                }
            }.padding(.horizontal, 18).padding(.top, 18).padding(.bottom, 16)
            HStack(spacing: 12) {
                Button("Add sample attachment", systemImage: "paperclip") {
                    preview.attachmentName = "appearance-reference.png"
                }
                .labelStyle(.iconOnly).buttonStyle(.plain)
                .font(.system(size: 17)).foregroundStyle(.secondary)
                .frame(width: 32, height: 36).contentShape(Rectangle())
                if preview.layout == .inline {
                    DesignLabControls(preview: preview)
                }
                Spacer(minLength: 0)
                ViewThatFits(in: .horizontal) {
                    Text("Shift-Return for a new line").fixedSize()
                    Text("Shift-Return\nfor a new line").multilineTextAlignment(.trailing).fixedSize()
                }
                .font(.system(size: 10)).foregroundStyle(.secondary).accessibilityHidden(true)
                ComposerPrimaryButton(label: "Send preview message", enabled: preview.canSend, action: preview.send)
            }
            .padding(.horizontal, 18).padding(.bottom, 18)
        }
        // One sampling surface. Controls inside it are plain, not glass-on-glass.
        .modifier(ComposerSurface())
    }
}
