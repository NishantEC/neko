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
new task reply. Installation and screen-check evidence follows separately.

Local logs: `/tmp/neko-continuity-swift-final.log`,
`/tmp/neko-continuity-core-tests.log`, `/tmp/neko-continuity-core-final.log`, and
`/tmp/neko-continuity-daemon-final.log`.

## Boundaries

This does not add provider compaction, durable Home sessions, streaming Markdown,
stable transcript block identities, restart-persistent drafts, or a full execution
journal. It cannot recover output already discarded by older versions. Stored
conversation/event retention limits remain. Source and fixture tests do not prove
a new live model conversation, resumed build, or actual provider compaction.
