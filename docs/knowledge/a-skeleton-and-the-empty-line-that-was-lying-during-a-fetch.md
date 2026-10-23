## A skeleton, and the empty line that was lying during a fetch

Reported as "usage shows nothing then shows everything — shouldn't there be a
skeleton". There should, and underneath it was a **correctness** bug rather
than a polish one.

**An empty list meant two opposite things and no surface could tell them
apart.** `ModeChrome::empty_line` is a statement of fact about a *finished*
search — "No agents running", "No providers signed in" — and during a fetch
it is simply false. The modes where the wait is real are exactly the ones
backed by a network round trip: Usage fans out to three vendor APIs,
Terminals and Schedules go over MCP. Each of them opened by asserting there
was nothing there, then contradicted itself a moment later.
`Root::awaiting_first_rows` splits the two, and an empty-and-waiting list
renders `components::skeleton` instead.

**Gated on `searching`, not on a request merely being outstanding.** That
flag is already delayed by `SEARCHING_TELL_DELAY_MS` precisely so a fast
answer never flashes a loading state, and a skeleton wants the identical
threshold — a provider answering in microseconds must not blink placeholders
on the way.

**It does not animate itself.** `gpui-component`'s own `skeleton.rs` is 59
lines and all of them are an `Animation::repeat()`, which this project
forbids after comet's measured incident — it was declined for that reason
("Third-party UI") and is rebuilt here on `motion::PulseClock`, the one
sanctioned repeating drive. `sync_pulse` therefore had to learn about it: a
skeleton is often the *only* thing on screen, so it has to hold the clock up
itself or the placeholder freezes at whatever phase it mounted on, which
reads as stuck rather than loading.

**The shape follows the mode, because a placeholder of the wrong shape is a
different jump rather than a fix.** Usage renders *meters* — a title over a
note on the left, a fixed `METER_COLUMN_WIDTH_PX` column of `METER_TICK_COUNT`
ticks with numbers beneath on the right — so standing a generic two-bar row in
front of one only moves the layout shift from "empty to full" to "wrong shape
to right shape". `ModeChrome::skeleton` names it (`Row`, `Meter`, `Bubble`),
the same way the chrome already names `has_detail` and `transcript`: layout
vocabulary is the client's own, never the provider's. Pinned, so a mode that
changes shape and forgets to say so fails a test rather than a screenshot.

Two details that are the difference between a placeholder and a glitch: the
shimmer travels a narrow band around the plate's resting alpha rather than
across the whole range (a hard pulse between invisible and solid reads as
something *failing*), and no two rows share a bar width (identical bars read
as a table with missing data; uneven ones read as text that has not arrived).
Both are pinned. The plates are sized to the real row's own icon socket and
height, so nothing shifts when the answer lands.

**`NEKO_SHOW_SKELETON` is the hook that made it photographable**, and it
exists for the same reason `files.rs` carries `NEKO_FILE_SEARCH_DELAY_MS`:
the wait only happens with real credentials against a real network, and an
isolated evidence `HOME` has neither, so every provider there answers
instantly and the state never occurs. It drives the real fields a pending
fetch sets rather than rendering a shortcut.
`docs/evidence/mode-skeleton-loading.png`.
