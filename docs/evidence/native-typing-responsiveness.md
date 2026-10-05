# Native typing responsiveness — 2026-10-05

Baseline: source `6f69992`, installed client `69a5a90`.

## Reproduction and cause

Typing an unsent sentence in NEK-9523 visibly wrapped text into a narrow strip
inside a wide composer. Accessibility contained the full sentence. SwiftUI's
`sizeThatFits` implementation was resizing the live NSTextView and its text
container for speculative widths. A regression test reproduced a 720pt live
editor being left at 350pt after sizing probes. A native process sample also
captured `ComposerView.sizeThatFits` calling `NSTextView.setFrameSize` during
SwiftUI minimum-size calculation. The sample is at
`/tmp/neko-typing-before.sample` (local temporary evidence).

Separate regression tests found 160 app-wide notifications for 160 draft edits
and three notifications for one unchanged background poll. The live snapshot
was approximately 1.3 MB; rebuilding observers unnecessarily also reprocessed
ticket and conversation presentation.

## Changes

- Composer measurement uses a reusable separate TextKit stack. Actual scroll
  view layout owns editor width and document height. Marked text, selection and
  the editor's undo machinery are not modified by sizing probes.
- Draft stores use Observation independently of AppModel's broad publisher.
  Navigation still owns their lifetime and revision-aware send completion.
- Unchanged and heartbeat-only polls do not publish UI updates. Changed content,
  connection failures and recovery still propagate under existing ordering rules.

## Automated verification

105 XCTest cases and 11 Swift Testing tests passed; the optional real-daemon
test was skipped without `NEKO_TEST_DAEMON`. Seven new regression tests cover
draft notifications, composer observation, unchanged/heartbeat-only snapshots,
changed snapshot delivery, sizing purity, actual width/multiline overflow,
marked composition and trailing newlines. Existing draft revision, attachment,
navigation and asynchronous snapshot-ordering tests passed.

Logs: `/tmp/neko-typing-baseline.log`, `/tmp/neko-typing-red.log`,
`/tmp/neko-sizing-red.log`, `/tmp/neko-typing-green.log`. These are temporary
local logs. AppKit tests use in-memory views; they do not replace live checks.

## Installed verification

Installed code: `4aef0ee8f503bd398bd6e25c51f0cc0bebe7b7d1`. The signed release
build completed in 32.37 seconds. Bundle signing passed strict verification;
installed client and daemon binaries matched the build output. Immediately
before replacement there were zero active/queued tickets and zero pending/queued
Home messages. Final inspection found one client and one daemon, both from
`/Applications/Neko.app`. Data and credentials were preserved.

Native UI checks on macOS 26.5.1 confirmed full-width typing in Home and the same
NEK-9523 composer that reproduced the problem. Screenshots showed both typed
lines in the available space after Shift-Return. Home undo and redo restored the
typed content. A twelve-line agent draft grew to the height cap, scrolled to the
caret, and survived navigation to Home and back. Home remained empty while the
agent draft was retained. Both diagnostic drafts were cleared afterward; Send
was disabled, Home was left open, and no message or task action was submitted.
Screenshots were reviewed in the chat rather than saved as repository artifacts.

The notification counts are deterministic test measurements, not a claimed
end-to-end latency benchmark. Physical rapid typing, a live OS input-method
session, older macOS versions and all possible sources of scrolling/frame lag
remain outside this check. Marked text preservation was tested with AppKit views.
No daemon execution behavior changed. Commits remain local to main.
