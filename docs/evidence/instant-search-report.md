# Two-phase search: results as you type

`fm/neko-instant-search`, against `main` at `f8cbe4c`.

The captain: *"The subsequent gap, lag between the stuff that are typed and
the stuff that actually shows up… It should be instantaneous."*

## Headline

Keystroke to first render, measured on the release binaries, same machine,
same fixture corpus, same queries, back to back:

| | n | min | median | mean | max |
|---|---|---|---|---|---|
| **Before** (`f8cbe4c`) | 24 | 67.82 ms | **108.50 ms** | 185.22 ms | 990.09 ms |
| **After** | 22 | 0.23 ms | **0.36 ms** | 0.46 ms | 1.25 ms |

A ~300x median improvement, and the tail — the 990 ms outlier that is what
the complaint is actually about — is gone entirely from the first render.

The total work is unchanged, and deliberately so. The *complete* frame after
the change still lands at 57.83–822.11 ms (median 92.81 ms, n=22), tracking
the before column's own distribution: Spotlight is exactly as slow as it
was. It simply no longer decides when the captain sees anything.

### What "first render" means here, precisely

Elapsed time from the keystroke's own `run_search` dispatch to the moment
that frame's results are committed to `Root::results` and `cx.notify()`
schedules the repaint. It deliberately excludes GPUI's own frame cadence
between that notify and the pixels changing (~8 ms on this window — see
`AGENTS.md`'s "Summon latency"), because that sits identically on top of
every measurement on both sides of the comparison and would only add noise
to it.

### Methodology

- **Both binaries built `--release`.** The "before" side is a `git worktree`
  at `f8cbe4c` carrying a throwaway backport of the two instrumentation
  pieces only (`evidence::run_search_bench`, the `search-latency` print) —
  no behaviour change, so the old single-response path is measured exactly
  as it shipped.
- **Real keystrokes through the app's own edit path**, never synthetic OS
  input: `NEKO_BENCH_SEARCH=documen` types the query one character at a
  time through `TextField::commit_edit` and the `ContentChanged`
  subscription — the same route a physical keypress takes. Each character
  therefore genuinely dispatches a fresh `Request::Search` and supersedes
  the previous one.
- **Samples**: 4 runs per side, 7 keystrokes each. Only the 2+ character
  queries count — below `files::MIN_QUERY_LEN` the file provider returns
  instantly and there is nothing to be slow about, on either side. One
  "after" run ended early when the summon window lost activation to
  something else on this shared machine (a known, documented hazard here —
  `AGENTS.md`, "Comet craft pass"); its completed samples are included, its
  missing two are simply absent, which is why n differs by two between the
  columns.
- **A private fixture corpus, never the captain's own files**: 28,800
  generated files across 480 nested directories under an isolated `HOME` at
  `/Users/nish/neko-evidence-home`, indexed by Spotlight and queried by a
  real `mdfind`. Raw `mdfind` timings against it: 55–70 ms warm, 384 ms
  cold — real, and squarely in the range `files.rs`'s own module comment
  documents for a real corpus. **The evidence home had to sit outside the
  worktree**: Spotlight does not index any path with a hidden component,
  and this worktree lives under `~/.treehouse`. It was removed afterwards.
- **The real `neko-daemon` was never launched**, per the standing rule —
  `verify_harness` only (real `server` module, real providers, real
  protocol, no pasteboard capture loop). The staged binary directory
  deliberately contains no `neko-daemon` next to `neko`, so
  `daemon_launcher`'s unconditional spawn fails loudly and harmlessly
  (`neko: failed to spawn neko-daemon: No such file or directory`) rather
  than starting one.

### Daemon idle memory: no regression

3 samples each, 20 s after the harness reported listening:

| | samples (KB) | mean |
|---|---|---|
| Before | 12800, 12736, 12928 | 12821 KB |
| After | 12880, 12928, 12864 | 12891 KB |

+70 KB, or +0.5% — smaller than the 192 KB spread within the "before"
samples themselves.

## What changed

### 1. The daemon answers a search in two frames

`neko-daemon`'s `handle_request` partitions its registered providers by a
new defaulted `Provider::defers_for(query)`. Apps, clipboard, settings and
commands all answer from memory or SQLite in microseconds and return
`false`; `FileProvider` returns `true` once the query is long enough that it
will really shell out to `mdfind`. The fast group runs first and its
`allocate` result goes out immediately as
`Response::SearchResults { complete: false }`; the deferred group then runs
and the full, re-allocated set goes out as `complete: true`.

A query nobody defers for — anything shorter than `files::MIN_QUERY_LEN`,
or any build with no deferring provider — is still exactly one frame. That
is not an optimisation detail: answering it in two would cost a wire frame
and a second client render for a byte-identical result.

**Why two frames and not per-provider responses.** Section ordering and
reservation (`search::allocate`) are inherently cross-provider decisions —
"which section leads" and "does every provider with a match get a slot"
cannot be answered one provider at a time. Streaming each provider
separately would have forced that logic into the client, across the crate
boundary `AGENTS.md` exists to protect (`neko` must not depend on
`neko-core`). Two frames keep `allocate` where it belongs and run it twice:
once over the fast providers alone, once over everything. Both frames are
tested to honour the reservation, not assumed to.

### 2. A superseded query is abandoned, not just ignored

Each connection now cancels its own previous in-flight `Search` when the
next one arrives (`supersede_previous_search`). No new wire message was
needed to express this: a client sending its next search on the same socket
*is* the statement that it has moved on, and there is no such thing as a
client wanting two of its own searches answered at once.

`neko_core::Cancel` carries the signal into `FileProvider`, where the single
blocking `recv_timeout(QUERY_TIMEOUT)` — a wait that structurally cannot
notice a flag flipping halfway through it — became a poll loop over the same
deadline. The `mdfind` child is killed, not merely left to finish with its
output discarded. That distinction is the point: an abandoned query that
keeps running competes with the query the captain actually wants, so the
faster they type the slower the current answer gets.

Proven directly rather than inferred from timing:
`files::tests::a_cancelled_query_abandons_the_child_immediately_and_kills_it`
asserts `ExitStatusExt::signal() == Some(SIGKILL)` on the reaped child, and
that the call returned in well under its own `QUERY_TIMEOUT`.

Confirmed live too, in the `NEKO_FILE_SEARCH_DELAY_MS=6000` probe below:
generations 4, 5 and 6 each rendered a partial frame and **never produced a
complete one** — each was superseded by the next keystroke and abandoned,
exactly as intended.

```
neko: search-latency gen=4 phase=partial query="no"    rows=5 elapsed_ms=1.59
neko: search-latency gen=5 phase=partial query="not"   rows=5 elapsed_ms=1.42
neko: search-latency gen=6 phase=partial query="note"  rows=6 elapsed_ms=1.05
neko: search-latency gen=7 phase=partial query="notes" rows=2 elapsed_ms=0.31
```

### 3. Late results append; they never reorder and never displace the selection

This is the part that would feel worse than the lag if it were got wrong,
and it is the only genuinely hard decision in the change.

`search::allocate` orders sections by content strength. Its authoritative
answer can therefore legitimately place a decisive Files match *above* the
Applications section the captain has been reading for most of a second —
correct as a one-shot answer, a visible reshuffle under their eyes as a late
one. So `panel::merge_late_results` keeps what is already on screen in its
exact order and appends only what the deferred provider introduced.
`allocate` still decides *which* items exist and how many slots each
provider gets; the merge decides only where the new ones are drawn relative
to what is already being looked at.

Two consequences, stated rather than left to be discovered:

1. The result can be a different order than a single-shot response for the
   same query would have produced. Deliberate, and it self-corrects on the
   next keystroke, which renders from a fresh partial frame with no anchor
   to preserve.
2. **If making room for the late section would cost the selected row, the
   late section is not shown at all.** A captain who has arrowed down to row
   seven is about to press Enter; moving that row — or dropping it and
   snapping the highlight to the top — is far worse than file results
   waiting one keystroke. `fit_within_budget` has to take a new section's
   header-plus-row from somewhere, and the only place it can take it from is
   the tail of an earlier section.

Tests: `late_results_are_appended_below_what_is_already_on_screen_never_promoted_above_it`,
`a_late_result_that_would_displace_the_selected_row_is_not_shown_at_all`,
`a_late_result_still_lands_when_it_costs_only_rows_below_the_selection`
(the counterpart — declining is the exception, not the rule),
`a_complete_frame_that_adds_nothing_new_leaves_the_list_byte_identical`,
and, through `Root`'s real frame handling including a Down press *between*
the two frames,
`a_late_complete_frame_never_moves_the_row_the_captain_arrowed_down_to`.

### 4. The "still searching" tell reads real pending state

`Root::partial_generation` is set when a partial frame is applied and
cleared when the complete one lands — "results are on screen and a provider
is still running", a fact rather than an inference. The 150 ms delay before
revealing the tell stays, now as the anti-flicker threshold it actually is
rather than as a stand-in for pending state.

## Visual evidence

Window-scoped captures (`screencapture -l<windowID>`, the only capture form
permitted on this machine), same window, same query, 14 seconds apart, with
`NEKO_FILE_SEARCH_DELAY_MS=6000` stretching the deferred provider out so the
intermediate state is capturable at all:

- `two-phase-search-partial-frame.png` — the partial frame. **Applications →
  Notes** (selected) and **Clipboard** are already rendered; "Searching…"
  shows in the input row; no Files section yet.
- `two-phase-search-complete-frame.png` — the complete frame, 6.09 s later.
  The Applications and Clipboard rows are in **identical positions**, the
  selection is still on Notes, "Searching…" has cleared, and the **Files**
  section has appeared *beneath* them with three real matches from the
  fixture corpus.

The client's own log for that run records which phase each capture shows:

```
neko: search-latency gen=3 phase=partial  query="notes" rows=2 elapsed_ms=35.15
neko: search-latency gen=3 phase=complete query="notes" rows=5 elapsed_ms=6089.78
```

`NEKO_FILE_SEARCH_DELAY_MS` is verification-only and unset by default — the
same pattern as `NEKO_ICON_EXTRACT_DELAY_MS`. Real hardware answers this
query in tens of milliseconds, which is genuinely too fast to land a
screenshot between the two frames even though that state is precisely what
the change exists to produce.

## Not verified

- **The captain's own corpus.** Every number here is against a synthetic
  28,800-file corpus, chosen specifically so no real filename of his was
  ever rendered or captured. His real `~/Documents` is larger and messier;
  expect his own before-numbers to be worse than the ones in the table, not
  better, and the after-numbers to be unchanged (the fast phase does not
  touch the filesystem at all).
- **Warm summon latency and real-path summon** were not re-measured. Nothing
  on the summon path changed, and real-path summon (~92 ms) is separately
  owned per this task's own brief.
- **A live count of leftover `mdfind` processes** during fast typing. The
  SIGKILL assertion and the missing complete-frames above both demonstrate
  abandonment; a process-table census during a realistic typing burst would
  be a third, independent confirmation and was not taken.
