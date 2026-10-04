# Review repair and the NEK-7E33 stop

NEK-7E33 retained its direct-work grant and saved Codex session, built a local
two-file patch, and reached independent review. It failed because validation
could not run and the planned deferred-response/channel-switch coverage was
missing. The authorization fix was effective; the review handler immediately
propagated every rejected verdict as a terminal worker failure.

## Recovery behavior

A usable rejected review now goes back to the existing builder session with its
findings, original plan and scope. Up to two repair passes occur within the same
worker. Every pass receives a fresh ephemeral read-only review of the full diff
against the original base, including builder commits and untracked files.
An unchanged rejected patch stops early. A passing claim without evidence,
malformed verdict, actual scope escape or split-parent rejection stays failed.
The same captured authority governs runs and result commits; a new human reply,
Stop, grant revocation or budget exhaustion still stops continuation.

This limit is per worker, not a new durable lifetime budget. Existing daemon
restart recovery remains separate. No review result bypasses verification and
no repair grants publication, extra tools or additional filesystem access.

## Actual environment blocker

The ticket's isolated Athena checkout has no `node_modules` or Yarn install
state. Its original checkout has both. The builder is told to investigate local
test setup within its write sandbox, using the original checkout as a read-only
reference. It must not write into shared dependencies.

The reviewer also received EPERM creating Yarn temporary directories. Its
read-only permission profile remains unchanged. This recovery change cannot
make those commands succeed by itself.

## Proposed temp permission change — awaiting user approval

Only reviewer runs would receive a daemon-created, unique temporary directory
outside the task checkout. Keep the read-only profile and add write access only
to that exact directory; set TMPDIR/TMP/TEMP to it. Keep the repository, original
checkout, shared caches, other temporary folders and network under their current
permissions. Clean up the dedicated directory when the run ends.

The host would compare repository HEAD, index and tracked/untracked source
content before and after review, rejecting any mutation as well as missing test
evidence. Verify with a real sandbox probe: a test-created file inside the
dedicated temporary directory succeeds; edits to task source, the original
checkout and another temporary directory fail. Commands requiring writes to the
repository still fail and must be reported honestly. This permission change is
not implemented or authorized by the repair loop.

## Verification

The real-daemon deterministic smoke regression failed before the change with
zero repair passes. The fixture exercises rejection followed by repair in the
same saved session, independent read-only review, and unchanged-output stopping.
Additional unit coverage checks the two-pass limit, malformed/unproven verdicts
and out-of-scope changes. The smoke also checks the original diff base after a
builder commit and cancellation during a repair pass. Independent code review
found a maximum-size verdict truncation edge; a 65,536-byte findings-last
regression reproduced it, and repair prompts now carry the complete raw verdict.
The complete real-daemon deterministic smoke passed after that correction.

Core library: 611 passed, 10 ignored. Daemon: 157 passed; verification harness:
158 passed. Native client: 76 XCTest and 11 Swift Testing tests passed. An initial
concurrent core run hit an existing five-second memory-extraction timing
assertion; the focused test and complete suite with four test threads passed.

This is orchestration proof, not proof that Athena's
tests pass or that the reviewer can now create temporary files.
