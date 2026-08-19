# Section ordering by content strength (`neko-section-order`)

Fixes the structural defect `AGENTS.md`'s "Search and ranking" section
already flagged as deliberately deferred: `search::allocate`'s output was
always grouped in provider *registration* order (app, file, clipboard,
settings, command), never re-ordered by how well each section actually
matched the query. `search::allocate`'s own doc comment said so explicitly.
Consequence, live on the captain's real daemon before this task: typing
"clipboard history" — a phrase the `Clipboard History` command matches
almost perfectly via its own alias table (`commands.rs`) — rendered the
**Commands** section beneath the always-reserved **Clipboard** section,
purely because `command` is registered after `clipboard` in
`AppState::with_test_providers`. Same shape for "displays": the **Settings**
section (which has the actual "Displays" pane) rendered beneath **Files**.

## What changed

`neko_core::search::allocate` (`crates/neko-core/src/search.rs`) gained a
third pass, after reservation and greedy interleave: sections are
stable-sorted by their own top (best) surviving candidate's score,
descending — the same score already used for the row-level greedy
interleave, reused rather than re-invented, per the launch brief's own "if
you have to choose, a simple explainable rule beats a clever one" guidance.
Ties resolve to registration order for free, since `sort_by` is stable and
comparing on score alone with no secondary key leaves equal-scoring
providers exactly where `providers`' input order already put them.

`panel::fit_within_budget` (`crates/neko/src/panel.rs`) needed **no
behavioral change** — it already just spends the pixel budget on whatever
order `results` arrives in (`group_into_sections` only groups contiguous
runs, never re-sorts), and "the first section gets the most generous
budget" was already the correct policy once "first" means "most relevant"
instead of "app". Its doc comment was updated to say so explicitly, and one
new test (`the_reservation_holds_regardless_of_which_kind_the_daemon_put_first`)
pins that the reservation guarantee holds when a non-`app` kind leads.

## A second, real defect found during live verification, not assumed

The obvious implementation — order sections by raw top-candidate score,
full stop — was tried first and verified against a synthetic but realistic
isolated-daemon-style probe (see "Verification methodology" below). It
failed the acceptance criteria for exactly the two queries this task
exists to fix: a clipboard entry that happened to be copied at the same
instant as the query, and that merely *starts with* the query text, out-
scored the `Clipboard History` command's own decisive alias match — not
because it was more relevant, but because `ClipboardProvider::search`
(`clipboard.rs`) adds up to `CLIPBOARD_RECENCY_BOOST_CEILING` (8.0) of
"just copied" freshness on top of the match score, and command candidates
get no such boost. Real numbers from the probe, query `"clipboard
manager"`:

```
raw top score [clipboard ] = 107.798 (fuzzy match ~99.8 + recency boost ~8.0)
raw top score [command   ] = 100.160 (fuzzy match only, no recency concept)
```

Section order before the fix below: clipboard first, command last — the
exact defect this task is fixing, reintroduced by a different mechanism.

**Fix**: `allocate`'s section-ordering pass discounts a clipboard
section's top score by `clipboard::CLIPBOARD_RECENCY_BOOST_CEILING`
*only* when comparing sections against each other — `candidate.score`
itself is never mutated, so row-level ranking (recency legitimately
surfacing a fresh copy first within the Clipboard section, and in the
ordinary cross-provider greedy interleave) is completely unaffected. The
constant is `pub(crate)` in `clipboard.rs` and referenced from
`search.rs`, not re-declared, so the two can't silently drift apart.
Verified both directions: a merely-fresh, weakly-relevant clipboard entry
no longer out-ranks a decisive command match (the bug), and a clipboard
entry that's still decisively the strongest match *even after* removing
its entire possible freshness bonus still leads its section correctly
(`search::tests::section_order_still_lets_clipboard_lead_when_it_wins_on_match_strength_alone`)
— the discount is not a blanket "clipboard never leads."

## Verification methodology

**Why not the real running daemon.** The launch brief is explicit and
non-negotiable: never read or capture the captain's real clipboard
history. `neko-daemon`'s clipboard capture loop
(`neko_core::clipboard::run_capture_loop`) starts unconditionally on
daemon startup and polls the *system* `NSPasteboard` — a systemwide
singleton, unrelated to `$HOME` isolation. Launching the real
`neko-daemon` binary under an isolated `HOME`, even briefly, would still
poll and could capture whatever the captain has genuinely on his
clipboard at that moment into a database this task controls — exactly
the outcome the brief forbids, regardless of how quickly it's cleaned up
afterward. This task never launched the real `neko-daemon` binary at all.

**What was used instead**: a standalone Rust harness (built outside this
worktree, under `/tmp`, depending on `neko-core` by path; deleted after
use, never committed) that constructs the exact same five providers in the
exact same order `AppState::with_test_providers`
(`neko-daemon/src/server.rs`) does, and calls the exact same
`neko_core::search::allocate` the daemon's own `Request::Search {
provider: None }` handler calls — the identical production code path,
in-process, with zero socket/daemon plumbing to fake. Compiled and run
against the release profile (`cargo build --release`).

- **Apps**: real `neko_core::apps::scan_applications()` — the real
  installed-application index. No privacy concern; this is public,
  non-personal metadata.
- **Files**: real `mdfind`, scoped to an isolated `$HOME`'s
  `Documents`/`Desktop`/`Downloads` (a throwaway directory tree, not the
  captain's real Documents folder), seeded with synthetic fixture files
  (`DisplaysAdapter.kt`, `ClipboardParser.kt`, etc. — confirmed indexed by
  Spotlight within seconds via a polling `mdfind -onlyin` check before
  relying on it).
- **Settings**: real enumeration of the real machine's System Settings
  panes (`/System/Library/ExtensionKit/Extensions`) — public, non-personal,
  sealed-volume data, identical to what `apps.rs`'s own tests already read
  from the real machine.
- **Clipboard**: a real, isolated SQLite db (under the same isolated
  `$HOME`), seeded directly via `clipboard::record_entry` with invented
  fixture strings (`"clipboard history feature test note one"`, etc.) —
  **never** via the pasteboard-reading capture loop, which was never
  started.
- **Commands**: the fixed, compiled-in table — no I/O either way.

## The four required queries, before and after (real probe output)

Limit 8 shown (limit 10 identical shape in every case).

```
"clipboard"          before: [file, file, file, clipboard x4, command]
                      after:  [command, clipboard x4, file, file, file]

"clipboard history"  before: [clipboard x3, command]
                      after:  [command, clipboard x3]

"clipboard manager"  before: [clipboard, command]
                      after:  [command, clipboard]

"displays"            before: [file x4, settings]
                      after:  [settings, file x4]
```

## App-ranking regression queries — unchanged

`bluetooth`, `terminal`, `chrome`, `code` re-run against the same probe:
`Bluetooth File Exchange` still above the `Bluetooth` pane, `Terminal`
still the sole top result, `Google Chrome` still first, `Code` then `T3
Code (Alpha)` still the top two app rows for `code` — identical shape to
`fe4e4e9`'s own evidence table. All of `search.rs`'s pre-existing 26 tests
pass unchanged (verified — see below), which independently confirms this.

## Automated verification

`cargo build`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
all clean at the workspace root, debug and release profiles. 6 new tests
in `search.rs` (section order follows content strength for both the
command and the settings-pane shape, ties break by registration order and
are stable across repeated runs, the clipboard-recency discount fixes the
adversarial case without becoming a blanket rule) and 1 new test in
`panel.rs` (the reservation guarantee holds when a non-`app` kind leads).

## What this did not (re-)verify, and why

**Warm summon latency / daemon idle memory**: not re-measured with a live
client, per the same reasoning several prior tasks in this file already
used when nothing on the summon path changed — this task touches exactly
one pure function (`search::allocate`), called once per `Request::Search`,
already well off the summon-activation path (`AGENTS.md`'s "Summon
latency" section: search's daemon round-trip has never been on that path).
A microbenchmark of `allocate()` itself, isolated from provider search
time (`mdfind`, SQLite), over the heaviest of the four query shapes
("code", every provider contributing multiple candidates): **100,000
iterations in ~366ms, ~3.7µs/call** — negligible next to `mdfind`'s
documented up-to-1.5s budget, and allocates nothing that outlives one
request. No plausible mechanism for a regression in either metric.

**A window-scoped screenshot of the reordered root list**: not captured.
The only way to produce one is launching the real client against a real,
running daemon — which reopens the exact clipboard-capture risk this
report's "why not the real running daemon" section explains was ruled
out. The in-process probe's own text output (above) is the evidence
instead; it exercises the identical `allocate()` call the real daemon's
`Request::Search` handler makes, just without a socket or a window around
it.
