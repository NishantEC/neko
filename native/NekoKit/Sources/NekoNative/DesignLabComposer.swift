import SwiftUI

struct DesignLabComposer: View {
    @Bindable var preview: DesignLabModel

    var body: some View {
        GlassGroup(spacing: 0) {
            VStack(alignment: .leading, spacing: 0) {
                if preview.layout == .context {
                    DesignLabControls(preview: preview, expanded: true)
                        .padding(.horizontal, 22).padding(.vertical, 16)
                    Divider().overlay { Color.primary.opacity(0.025) }
                }
                VStack(alignment: .leading, spacing: 12) {
                    if let name = preview.attachmentName {
                        HStack(spacing: 10) {
                            Image(systemName: "doc.richtext")
                                .font(.system(size: 17)).frame(width: 32, height: 34)
                                .background(.purple.opacity(0.12), in: .rect(cornerRadius: 8))
                            VStack(alignment: .leading, spacing: 3) {
                                Text(name).font(.system(size: 12, weight: .semibold)).lineLimit(1)
                                Text("Reference · preview").font(.system(size: 11)).foregroundStyle(.secondary)
                            }
                            Button("Remove attachment", systemImage: "xmark") { preview.attachmentName = nil }
                                .labelStyle(.iconOnly).buttonStyle(.plain)
                                .frame(width: 28, height: 28).contentShape(Rectangle())
                        }
                        .padding(8).padding(.trailing, 4)
                        .background(.purple.opacity(0.09), in: .rect(cornerRadius: 16))
                        .fixedSize(horizontal: false, vertical: true)
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
                }.padding(.horizontal, 22).padding(.top, 20).padding(.bottom, 12)
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
                    Button(action: preview.send) {
                        ZStack {
                            DesignLabOrb(size: 42)
                            Image(systemName: "arrow.up").font(.system(size: 18, weight: .semibold)).foregroundStyle(.white)
                        }.contentShape(Circle())
                    }
                    .buttonStyle(.plain).disabled(!preview.canSend)
                    .accessibilityLabel("Send preview message").help("Send to this local preview")
                }
                .padding(.horizontal, 16).padding(.bottom, 14)
            }
            // One sampling surface. Controls inside it are plain, not glass-on-glass.
            .liquidGlass(radius: 28)
        }
    }
}
