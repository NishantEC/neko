import SwiftUI

/// A reply that is still being written, drawn as Markdown as it arrives.
/// Blocks that are finished are drawn once and skipped on every later token;
/// only the live tail redraws, and its newest text fades in.
struct ChatStreamingText: View {
    let text: String

    var body: some View {
        let parts = StreamingMarkdown.split(text)
        // The same gap `ReadableText` leaves between two of its own blocks, so
        // the reply does not shift when it finishes and is drawn as one.
        let gap = MarkdownCache.parsed(parts.stable).last.map(ReadableText.blockSpacing) ?? 0
        VStack(alignment: .leading, spacing: 0) {
            if !parts.stable.isEmpty {
                ReadableText(text: parts.stable).equatable()
                    .padding(.bottom, parts.tail.isEmpty ? 0 : gap)
            }
            if !parts.tail.isEmpty {
                ChatFadingTail(text: StreamingMarkdown.mended(parts.tail))
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

/// The unfinished end of a reply. Text that was already there stays solid
/// underneath while the whole tail fades up over it, so only the characters
/// that just arrived change alpha. Under Reduce Motion it is drawn once.
struct ChatFadingTail: View {
    let text: String
    @State private var settled = ""
    @State private var fresh: Double = 1
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        ZStack(alignment: .topLeading) {
            if !reduceMotion, !settled.isEmpty {
                ReadableText(text: settled).equatable()
                    .allowsHitTesting(false).accessibilityHidden(true)
            }
            ReadableText(text: text).equatable()
                .opacity(reduceMotion ? 1 : fresh)
        }
        .onAppear { if !reduceMotion { arrive(carrying: "") } }
        .onChange(of: text) { old, new in
            if !reduceMotion { arrive(carrying: StreamingMarkdown.settled(old: old, new: new)) }
        }
    }

    /// Restarts the fade: the part carried over stays opaque, the rest rises.
    private func arrive(carrying carried: String) {
        var instant = Transaction(animation: nil)
        instant.disablesAnimations = true
        withTransaction(instant) { settled = carried; fresh = 0 }
        DispatchQueue.main.async { withNekoAnimation(.stream) { fresh = 1 } }
    }
}
