import SwiftUI

/// Everything a reply did, as one line under its text. While Neko works it is
/// the live status (the newest step, shimmering, with a clock); once the turn
/// ends it becomes a summary sentence ("Ran 3 commands · 1 failed") with how
/// long it worked. Either opens in place to the steps and tool calls behind it,
/// and stays as the reader left it.
struct ChatWorkLog: View {
    let id: String
    let summary: ChatWorkSummary
    /// Tool calls that have an outcome; one waiting on approval is answered in the dock.
    let tools: [ChatToolCallValue]
    let running: Bool
    let startMs: Int
    let completedMs: Int?
    let live: (activity: NekoActivity, label: String)
    let ledger: ChatWorkLedger
    @State private var userOpened: Bool

    init(id: String, summary: ChatWorkSummary, tools: [ChatToolCallValue], running: Bool, startMs: Int, completedMs: Int?,
         live: (activity: NekoActivity, label: String), ledger: ChatWorkLedger) {
        self.id = id; self.summary = summary; self.tools = tools; self.running = running
        self.startMs = startMs; self.completedMs = completedMs; self.live = live; self.ledger = ledger
        _userOpened = State(initialValue: ledger.isOpen(id))
    }

    private var canExpand: Bool { !summary.steps.isEmpty || !tools.isEmpty }
    private var open: Bool { ChatWorkFoldPolicy.isOpen(userOpened: userOpened, canExpand: canExpand) }
    private var toggle: Binding<Bool> {
        Binding(get: { open }, set: { value in
            userOpened = value
            if ledger.isOpen(id) != value { ledger.toggle(id) }
        })
    }

    var body: some View {
        if running {
            VStack(alignment: .leading, spacing: NekoSpace.xxs) {
                Button { withNekoAnimation(.reveal) { toggle.wrappedValue.toggle() } } label: {
                    HStack(spacing: NekoSpace.xs) {
                        if canExpand { ChatWorkChevron(open: open) }
                        ActivityCapsule(activity: live.activity, label: live.label, plain: true, font: NekoFont.meta)
                            .lineLimit(1).truncationMode(.middle)
                        ChatElapsed(since: startMs)
                        Spacer(minLength: 0)
                    }
                    .frame(minHeight: NekoChat.workRow)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain).disabled(!canExpand)
                .accessibilityValue(canExpand ? (open ? "Expanded" : "Collapsed") : "")
                .accessibilityHint(canExpand ? (open ? "Hide the steps" : "Show the steps") : "")
                if open { details }
            }
        } else if let sentence = summary.sentence {
            ChatWorkFold(label: sentence, failed: summary.failed > 0,
                         trailing: ChatDuration.workedFor(startMs: startMs, endMs: completedMs),
                         open: toggle) { details }
        }
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(summary.steps) { step in
                HStack(alignment: .firstTextBaseline, spacing: NekoSpace.xs) {
                    Image(systemName: step.failed ? "xmark.circle" : "checkmark")
                        .foregroundStyle(step.failed ? NekoStyle.coral : N.text4)
                        .frame(width: NekoSpace.md).accessibilityHidden(true)
                    Text(LocalizedStringKey(step.text)).foregroundStyle(step.failed ? N.text : N.text2)
                        .lineLimit(1).truncationMode(.middle)
                    if step.failed { Text("failed").foregroundStyle(NekoStyle.coral) }
                    Spacer(minLength: 0)
                }
                .font(NekoFont.meta)
                .frame(minHeight: NekoChat.commandRow)
                .accessibilityElement(children: .combine)
            }
            ForEach(tools) { ChatToolCallRow(call: $0) }
        }
        .padding(.leading, NekoSpace.lg)
        .transition(.opacity)
    }
}
