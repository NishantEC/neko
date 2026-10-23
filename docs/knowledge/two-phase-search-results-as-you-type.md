## Two-phase search: results as you type

`fm/neko-instant-search`, fixing the captain's own daily complaint —
*"the lag between the stuff that are typed and the stuff that actually
shows up… it should be instantaneous."* Full before/after numbers,
methodology, and window-scoped evidence:
`docs/evidence/instant-search-report.md`. Measured on release binaries,
keystroke to first render: **median 108.50ms → 0.36ms** (n=24 / n=22).

**The cause, and why it was structural rather than a tuning problem.** One
`Request::Search` ran all five providers and returned one combined
response, so nothing rendered until the slowest finished — and the slowest,
`files::FileProvider`, shells out to `mdfind` (measured 55–990ms on a real
corpus, bounded at `files::QUERY_TIMEOUT` = 1.5s). Apps, clipboard,
settings and commands were all ready in microseconds and waited anyway.

**A search can now be answered by more than one frame.**
`Response::SearchResults` carries `complete: bool`;
`Response::ends_request()` is the one place the "a request may be answered
more than once" rule is written down. `neko-daemon`'s `handle_request`
partitions its providers by `Provider::defers_for(query)` (defaulted
`false`; only `FileProvider` overrides it, and only once the query is long
enough that it will really run `mdfind`), answers the fast group
immediately with `complete: false`, then answers the whole set with
`complete: true`. **A query nobody defers for is still exactly one frame** —
answering a short query in two would cost a wire frame and a second client
render for a byte-identical result, which is why `defers_for` takes the
query rather than being a fixed property of the provider.

**Why two frames rather than one response per provider.** Section ordering
and reservation (`search::allocate`) are inherently cross-provider
decisions — "which section leads", "does every provider with a match get a
slot" — that cannot be answered one provider at a time. Per-provider
responses would have pushed that logic into the client, across the crate
boundary this repo exists to protect (`neko` must never depend on
`neko-core`). Two frames keep `allocate` where it belongs and simply run it
twice: over the fast providers alone, then over everything. Both frames are
tested to honour the reservation
(`the_partial_and_final_frames_both_honour_allocates_own_reservation_rules`),
not assumed to.

**Client side**: `NekoClient::request_streaming` returns a `ResponseStream`
yielding every frame for one request id until `ends_request`. A plain
`request()` entry is *left in place* when a partial arrives and discards
it, so every pre-existing caller still resolves with the complete answer
and never learns the daemon answered in two parts — that includes anything
that reaches for `request(Request::Search { .. })` in future.

**A superseded query is abandoned, not merely ignored.** Each connection
cancels its own previous in-flight `Search` when the next arrives
(`server::supersede_previous_search`) — no new wire message was needed,
since a client sending its next search on the same socket *is* the
statement that it has moved on, and no client ever wants two of its own
searches answered at once. `neko_core::Cancel` carries the signal to
`Provider::search_cancellable` (defaulted to delegate to `search`, so a
provider with nothing interruptible still implements exactly one method).
Inside `files.rs`, the single blocking `recv_timeout(QUERY_TIMEOUT)` — a
wait that structurally cannot notice a flag flipping halfway through it —
is now a poll loop over the same deadline, and **the `mdfind` child is
killed, not left to finish with its output discarded**. That distinction is
the whole point: an abandoned query that keeps running competes with the
one the captain actually wants, so the faster they type the slower the
current answer gets. `run_bounded_child` returns the reaped child's exit
status specifically so the test can assert `SIGKILL` directly rather than
infer the kill from timing.

**Late results append; they never reorder and never displace the
selection — the hard part, and the one most likely to feel worse than the
lag if got wrong.** `search::allocate` orders sections by content strength,
so its authoritative answer can legitimately put a decisive Files match
*above* the Applications section already on screen: correct as a one-shot
answer, a visible reshuffle under the captain's eyes as a late one.
`panel::merge_late_results` therefore keeps what is rendered in its exact
order and appends only what the deferred provider introduced (necessarily
its own rows — a fast provider cannot gain candidates between the two
frames). Two consequences, both deliberate and both stated in that
function's own doc comment: the result can differ in order from what a
single-shot response would have produced (self-correcting on the next
keystroke, which has no anchor to preserve), and **if making room for the
late section would cost the selected row, the late section is not shown at
all** until the next keystroke — a captain who has arrowed down to row
seven is about to press Enter, and `fit_within_budget` can only take a new
section's header-plus-row from the tail of an earlier one.

**The "still searching" tell now reads real pending state**, not elapsed
time: `Root::partial_generation` is set when a partial frame lands and
cleared when the complete one does. The 150ms delay before revealing it
stays, now as the anti-flicker threshold it actually is.

**Verification hooks this added, all unset by default**: `NEKO_BENCH_SEARCH=<query>`
(types the query one character at a time through the panel's own real edit
path — never synthetic input — for one keystroke-to-first-render sample per
character), `NEKO_LOG_SEARCH_LATENCY=1` (implied by the former; prints one
`neko: search-latency` line per applied frame, which is what tells you which
phase a captured screenshot actually shows), and
`NEKO_FILE_SEARCH_DELAY_MS=<ms>` in `neko-core/src/files.rs` (same pattern as
`NEKO_ICON_EXTRACT_DELAY_MS` — real hardware answers a prefix query in tens
of milliseconds, genuinely too fast to screenshot the intermediate state
that the whole change exists to produce; it is itself cancellable, which
makes the abandon path live-verifiable too).

**One environment fact worth keeping for any future evidence run here:
Spotlight does not index a path with a hidden component**, so a fixture
corpus placed anywhere under this repo's own `~/.treehouse` worktree is
invisible to `mdfind` no matter how long you wait or how you `mdimport` it.
An evidence `HOME` that needs real file-search results has to live outside
it.
