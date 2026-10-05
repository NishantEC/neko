# Chat continuity and saved command evidence

Date: 2026-10-05. Baseline: `dfc2116`.

The approved reference remains **20A · Complete chat — conversation first** in
[Paper](https://app.paper.design/file/01M3YBQPKND2A0GY02E3Z13H1G/p-1-0).
This slice repairs the history/evidence foundation beneath that presentation.

## Behavior

Home previously chose the latest 12 messages across all conversations, then
filtered workspace and profile. Activity elsewhere could displace the entire
relevant history. Filtering now happens first, including completed/nonempty
eligibility. The newest eligible suffix stays chronological. A 12 KiB transcript
budget includes role labels and omission notices, and normally retains whole
messages instead of cutting each message at 1,000 bytes. An individually oversized
newest message retains its UTF-8-safe tail and marks its omitted beginning. The
current user request remains outside this history budget.

Command receipts previously truncated serialized JSON at 2,048 bytes. Long output
or escaped content could leave invalid JSON, losing command labels in the native
history. Receipts now bound string fields before serialization and preserve
explicit `command_truncated`, `output_truncated`, `receipt_incomplete` and
`receipt_malformed` flags. A damaged legacy record retains a bounded raw preview.
The live verification path keeps complete command identity; its saved event copy
can be shorter. Known failure exit codes remain failures. Incomplete or shortened
command identity cannot satisfy verification, and a later malformed record cannot
be ignored in favor of an earlier success. Shortened output remains a labelled
preview, not a claim that the entire execution log is retained.

The native result has an expandable **Saved command evidence** group. Work history
and activity reuse the same command detail view. Each record exposes the command,
recorded exit status, retained output and **Copy saved record**. Missing/malformed
records explicitly say incomplete; no command is inferred from cut-off JSON or
terminal prose. Retained records can include earlier attempts and cannot replace
the current independent review verdict.

## Verification

Baseline verification: clean `main` at `dfc2116`; 94 XCTest and 11 Swift Testing
tests passed. The optional daemon contract test was skipped in that baseline.

Final source verification: 100 XCTest tests and 11 Swift Testing tests passed;
`neko-daemon` passed all 158 tests. The default-parallel core run passed 642 tests
but its process-cleanup fixture
`memory_extraction_disables_tools_and_rejects_tool_receipts` exceeded its five-second
timing assertion (11 tests ignored). After the final role/prose regression tests
were added, the full core suite with `--test-threads=2` passed 646 tests, zero
failures and 11 ignored. The timing failure is retained here rather than hidden by
the passing constrained run.

Independent source reviews covered exact scope/profile boundaries, byte accounting,
Unicode, malformed/shortened command proof, latest-receipt handling and prose-prefix
collisions. Review found two presentation gaps: human notes could be interpreted as
receipts, and incomplete records hid known failure codes. Both were fixed and
covered by regressions. Only actual worker roles enter receipt presentation;
user/system text remains text. Assistant prose with the reserved receipt prefix
is explicitly labelled before it enters the progress channel.

A read-only snapshot of the installed StreamChat ticket found 30 saved command
records, including 10 with invalid JSON and one known nonzero exit code among the
valid records. This establishes the legacy-data reproduction without sending a
new task reply.

Local logs: `/tmp/neko-continuity-swift-final.log`,
`/tmp/neko-continuity-core-tests.log`, `/tmp/neko-continuity-core-final.log`, and
`/tmp/neko-continuity-daemon-final.log`.

## Installed verification

Source commit `e86b236` was built and signed with the existing development identity.
The Rust release build completed in 36.93 seconds; the Swift production build in
38.32 seconds. The optional native protocol contract test then passed against
that exact bundled daemon in an isolated temporary data directory, including its
restart/persistence checks (`/tmp/neko-continuity-protocol.log`). It did not use the
normal Neko database or send a model request.

Immediately before installation, the live gate reported 69 tickets, zero
Planning/Building/Reviewing, zero Queued tickets, and zero pending/queued Home
messages. The visible composer was empty. Installation succeeded, preserving
data and credentials, and only `/Applications/Neko.app` was launched.

The installed signature verifies. Installed and built executable hashes match:

- Client: `b9ab839bd27ac5c39a0cd75dbab21aa7af2aee199d1578f6e84291f293ef09cb`
- Daemon: `f3434948838b38973fda5b034e7f2a1d1249f704608a8c1a0b1000a5f92fb039`

CUA screenshots and accessibility readbacks verified the real NEK-7E33 chat:

- A compact evidence group beneath the independent review reports 30 saved
  commands, 10 incomplete records and two shortened outputs.
- Expanding the group exposes roles, recorded exit codes and command labels.
  The known failed command reads “Failed · exit 1”.
- Expanding damaged legacy records shows an explicit incomplete warning, a raw
  record disclosure and Copy saved record; it never displays a blank command.
- Copy saved record changed to Copied after accessibility activation. Unit tests
  cover exact retained-text preservation; the clipboard payload was not reread.
- A shortened reviewer output expands into its complete retained command and a
  bounded scrolling output panel with an explicit shortening warning. A native
  accessibility scroll action brought it onscreen for screenshot inspection.

Screenshots confirmed readable spacing, aligned rows, visible warning text, and
the preserved native sidebar, toolbar and composer. Home navigation worked and
its composer remained empty. The coordinate mouse/scroll attempts on the long,
quoted ticket window title returned the CUA `noWindowsAvailable` error; successful
receipt interaction checks used accessibility actions. This does not establish
physical mouse or drag behavior for the new controls.

No task reply, acceptance, cancellation or runtime change was issued by these
checks. The final snapshot had no active/pending work, and process inspection
found exactly one installed app and one installed daemon. Local build/install
logs: `/tmp/neko-continuity-build.log`, `/tmp/neko-continuity-install.log`.

## Boundaries

This does not add provider compaction, durable Home sessions, streaming Markdown,
stable transcript block identities, restart-persistent drafts, or a full execution
journal. It cannot recover output already discarded by older versions. Stored
conversation/event retention limits remain. Source and fixture tests do not prove
a new live model conversation, resumed build, or actual provider compaction.
