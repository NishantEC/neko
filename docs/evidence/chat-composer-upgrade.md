# Native chat and composer upgrade

Date: 2026-10-05. Baseline: `6a467cd`, installed client `a22a41f`.

## Scope

The live baseline showed two different input experiences: Home's AppKit editor
supported growth and attachments, while agent chats used a basic multiline field.
The StreamChat agent's latest screen showed an obsolete plan; its useful result
was inside a collapsed Details section above the transcript.

The implementation follows **20A · Complete chat — conversation first** in the
[Paper file](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-1-0).
**20B · Complete chat — work log** preserves the denser alternative. Both use the
existing native sidebar and toolbar. Paper screenshots were checked for spacing,
type, contrast, alignment and clipping. Native implementation uses system colors
and existing Neko materials rather than hardcoding the illustrative mock palette.

## Changes

- Home and agent chats reuse `SharedComposer`, the growing AppKit editor,
  attachment picker/strip, model catalog menu and explicit action state. The
  menu labels its global scope. Ask/Plan remains a Home capability.
- Drafts keep revisions and attachments per conversation scope. Successful
  acknowledgements do not erase newer drafts. Complete payload byte limits are
  checked before sending, including attachment references and Home context.
- Ticket results and independent reviewer readbacks are visible; old plans,
  conversation and decision history fold away on settled work. Retry states
  label preserved output as historical. Copy result preserves the saved text.
- Escaped table pipes no longer become extra columns. Table cells wrap;
  wide tables scroll horizontally. Terminal/diff previews report line counts
  and expose more available lines. Copy uses complete available source.
- Explicit scrolling or clicking the Home transcript cancels its temporary
  post-send follow pin. Jump to latest restores following. The observer passes
  every native event through, so it does not replace scrolling or selection.

## Verification

Baseline passed: 77 XCTest tests and 11 Swift Testing tests. After implementation
and the two final review fixes, 94 XCTest tests passed with zero failures; Swift
Testing reported 11 passing tests. `realDaemonPersistsNativeProtocolCommands`
was explicitly skipped because `NEKO_TEST_DAEMON` was not supplied. No Rust source
changed; the installer build compiled the daemon as part of the signed bundle.

Tests cover newer drafts surviving acknowledgements, attachment scope and
revisions, Unicode/reference payload sizes, Home queue versus scoped Stop,
ticket authority presentation, user scrolling during the post-send pin, composer
margin hit regions, escaped pipes and table growth at 360/700pt. The review found
and corrected two edge cases: global chat queue occupancy must remain separate
from the visible conversation's Stop command, and scroll detection must include
the transcript margins beside the composer.

Test log: `/tmp/neko-chat-final-tests.log`. Signed build log:
`/tmp/neko-chat-final-build.log`. These local logs are not committed artifacts.

The signed release build completed in 36.98 seconds. The idle gate reported 69
tickets, zero Planning/Building/Reviewing and zero pending/queued chats immediately
before installation. `/Applications/Neko.app` matches the built executable's
SHA-256 (`a57402213ed2b68f1134cca3c31262784cdbb5f32d5d04a91da40e85c317409f`).
One canonical app process and one owner of the normal daemon socket were observed.

### Installed screen and interaction checks

CUA screenshots and fresh accessibility readbacks verified:

- Home and agent composers render the same growing editor and footer controls.
  Actual typing plus Shift Return produced two lines in each; no message was sent.
- A local generated text fixture was selected through the agent's native file
  picker, appeared as a removable chip, and preserved the typed draft.
- Navigating agent → Home → agent preserved two distinct drafts and the agent's
  attachment. Both test drafts and the attachment chip were then removed.
- Scrolling Home upward revealed Jump to latest. Activating it returned to the
  latest message and removed the button.
- The waiting agent retained its plan and explicit needed question. The existing
  ReadyForReview StreamChat ticket displayed its result and independent review
  without opening Details, with previous work folded away.
- A coordinate-based mouse click on the top native sidebar toggle collapsed the
  sidebar. Restoring it preserved the empty composer. Other control checks used
  accessibility activation and keyboard input.

The attachment dialog produced a transient AX error while opening; the next
readback showed the working native panel, and selection completed normally.
No approval, task cancellation, runtime change, or real task reply was submitted.
Live image paste/drop, IME composition, long-output expansion, queued-provider
execution and a 1,000-turn performance run were not exercised onscreen. Table
wrapping has native layout-test evidence at two widths, not live conversation
fixture screenshots.

## Boundaries

The daemon/protocol and execution authority are unchanged. Ticket replies still
reach the intent interpreter and saved-session workflow. UI checks do not prove
a new live model turn, resumed builder execution, or provider compaction.

This pass does not add context compaction, measured context occupancy, streaming
Markdown, syntax highlighting, a virtualized transcript, cross-message selection,
caret-aware mentions, ticket slash menus, restart-persistent drafts, or async
attachment conversion. File/image conversion remains synchronous. Home's scoped
history window and the daemon's serialized event truncation need separate fixes.
See the [full audit](../design/chat-rendering-research.md) for the reference evidence
and deeper roadmap. No third-party app was installed or run during this audit.
