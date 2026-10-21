import SwiftUI

/// Everything a reply did, as one block ABOVE its text. Finished: a muted
/// "Worked for 11s ⌄" header with the reply's time at the right and a hairline
/// under it, then one summary row ("Read 2 files and ran 1 command"). While
/// Neko works the header reads "Working for 4s" and the row under the rule is
/// the newest step, shimmering. Either opens in place to the steps and tool
/// calls behind it, and stays as the reader left it.
struct ChatWorkLog: View {
    let id: String
    let summary: ChatWorkSummary
    /// Tool calls that have an outcome; one waiting on approval is answered in the dock.
    let tools: [ChatToolCallValue]
    let running: Bool
    let startMs: Int
    let completedMs: Int?
    /// When the reply arrived, shown at the header's trailing edge.
    var at: Int = 0
    let live: (activity: NekoActivity, label: String)
    let ledger: ChatWorkLedger
    @State private var userOpened: Bool

    init(id: String, summary: ChatWorkSummary, tools: [ChatToolCallValue], running: Bool, startMs: Int, completedMs: Int?, at: Int = 0,
         live: (activity: NekoActivity, label: String), ledger: ChatWorkLedger) {
        self.id = id; self.summary = summary; self.tools = tools; self.running = running
        self.startMs = startMs; self.completedMs = completedMs; self.at = at; self.live = live; self.ledger = ledger
        _userOpened = State(initialValue: ledger.isOpen(id))
    }

    private var canExpand: Bool { !summary.steps.isEmpty || !tools.isEmpty }
    private var open: Bool { ChatWorkFoldPolicy.isOpen(userOpened: userOpened, canExpand: canExpand) }
    private func toggle() {
        withNekoAnimation(.reveal) {
            userOpened = !open
            if ledger.isOpen(id) != userOpened { ledger.toggle(id) }
        }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: .zero) {
            header
            Hairline()
            if running {
                HStack(spacing: NekoSpace.xs) {
                    ActivityCapsule(activity: live.activity, label: live.label, plain: true, font: NekoFont.of(.callout))
                        .lineLimit(1).truncationMode(.middle)
                    Spacer(minLength: 0)
                }
                .frame(minHeight: NekoChat.workHeader)
            } else if let sentence = summary.sentence {
                HStack(spacing: .zero) {
                    Image(systemName: summary.failed > 0 ? "exclamationmark.circle" : "wrench.and.screwdriver")
                        .font(NekoFont.meta)
                        .foregroundStyle(summary.failed > 0 ? NekoStyle.amber : N.text4)
                        .frame(width: NekoChat.workIndent, alignment: .leading)
                        .accessibilityHidden(true)
                    Text(sentence).monospacedDigit().lineLimit(1).truncationMode(.tail)
                        .font(NekoFont.chat).foregroundStyle(N.text3)
                    Spacer(minLength: 0)
                }
                .frame(minHeight: NekoChat.workHeader)
                .accessibilityElement(children: .combine)
            }
            if open { details.padding(.bottom, NekoSpace.xxs) }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var header: some View {
        let worked = ChatDuration.workedFor(startMs: startMs, endMs: completedMs) ?? "Worked"
        return Button(action: toggle) {
            HStack(spacing: NekoSpace.xs) {
                if running { ChatWorkingFor(since: startMs) } else { Text(worked).monospacedDigit() }
                if canExpand {
                    Image(systemName: "chevron.down").font(NekoFont.of(.small, weight: .semibold))
                        .rotationEffect(.degrees(open ? 180 : 0)).accessibilityHidden(true)
                }
                Spacer(minLength: 0)
                if !running, at > 0 {
                    Text(Date(timeIntervalSince1970: Double(at) / 1000), format: .dateTime.hour().minute())
                        .font(NekoFont.caption).monospacedDigit().foregroundStyle(N.text4)
                }
            }
            .font(NekoFont.chat).foregroundStyle(N.text3)
            .frame(minHeight: NekoChat.workHeader)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain).disabled(!canExpand)
        .accessibilityLabel(running ? "Working" : worked)
        .accessibilityValue(canExpand ? (open ? "Expanded" : "Collapsed") : "")
        .accessibilityHint(canExpand ? (open ? "Hide the steps" : "Show the steps") : "")
    }

    private var details: some View {
        VStack(alignment: .leading, spacing: .zero) {
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
        .padding(.leading, NekoChat.workIndent)
        .transition(.opacity)
    }
}
