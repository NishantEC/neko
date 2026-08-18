# Settings-pane and clipboard ranking: two defects, real numbers

Firstmate reproduced both live against the captain's own running daemon
(branch `fm/neko-ranking-2`, `main` at `8da1238`):

```
'sound':         [file, file, clipboard x7, settings]      settings = ["Sound"]
'storage':       [file x8, clipboard, settings]            settings = ["Storage"]
'appearance':    [file x8, clipboard, settings]            settings = ["Appearance"]
'wallpaper':     [clipboard x9, settings]                  settings = ["Wallpaper"]
'accessibility': [file, clipboard x8, settings]            settings = ["Accessibility"]
'displays':      [file x4, clipboard x2]                   settings = []      <- fell off entirely
'bluetooth':     [app, file x2, clipboard x3]              settings = []      <- fell off entirely
```

This verifies the two defects, implements the fix, and re-verifies against a
real, isolated daemon — never the captain's own machine or clipboard history
(see "Method" below).

## Method

Every daemon run in this investigation used an isolated `$HOME`
(`/tmp/neko-ranking2-home`), never the captain's real
`~/Library/Application Support/neko/`. Clipboard fixtures were inserted
directly into that isolated SQLite database (`sqlite3`, matching
`db.rs`'s own `clipboard_entries` schema) rather than by writing to the
real, systemwide macOS pasteboard — this keeps the captain's real clipboard
untouched entirely, not just unread, and made fixture content trivially
reviewable (nine synthetic paragraphs, each a `Fixture entry N: ...`
placeholder, never anything resembling his real invoices/correspondence).
`crates/neko-client/examples/query_probe.rs` (new, committed alongside the
fix) is a small diagnostic client: it connects to whatever daemon `$HOME`
points at and prints the `kind` of every row returned for the seven queries
above, at `limit: 10` — matching firstmate's own repro methodology exactly,
so before/after numbers are directly comparable. Every daemon process
started for this investigation was killed by its own exact PID; the
captain's real daemon (a different PID, a different `$HOME`) was never
touched.

**Per this task's own hard constraint, added after this evidence was first
drafted: no client (`neko`, the GPUI app) was ever launched, no window was
opened, `NEKO_BENCH` was never run, and no screenshot was taken at any
point in this investigation** — the captain was actively working on this
machine and had ruled out running the app. Every verification here is
against the headless `neko-daemon` binary only, probed over its Unix
socket. See "What wasn't measured" below for what that costs.

## Defect 1 — an exact settings-pane match scored too low

`settings.rs::search`'s own comment explained the (correct) goal — a pane
must never crowd out a genuine application match — but implemented it by
leaving pane candidates completely unboosted. Real `fuzzy_score` numbers
this was tuned against:

```
fuzzy_score("sound", "Sound")                      = 17.4
fuzzy_score("sound", "background_sound.log")        = 14.1   (a boundary match, not even a scattered one)
fuzzy_score("bluetooth", "Bluetooth")                = 36.32
fuzzy_score("bluetooth", "Bluetooth File Exchange")  = 36.04
fuzzy_score("wallpaper", "Wallpaper")                = 36.32
fuzzy_score("storage", "Storage")                    = 25.86
fuzzy_score("appearance", "Appearance")              = 42.3
fuzzy_score("accessibility", "Accessibility")        = 63.24
fuzzy_score("displays", "Displays")                  = 30.84
```

An exact pane title only barely cleared even a loosely-related file
("Sound" beats "background_sound.log" by 3.3 points), and had nothing at
all to clear a clipboard entry whose recency boost (up to +8.0, before this
task's fix) stacks on top of a match anywhere inside a long paragraph — see
Defect 2.

### The fix: `search::settings_category_score`

The same idea `fe4e4e9` already used for the `"app"` provider
(`search::app_category_score`) — a per-word rescore plus a flat,
prefix-gated bonus — extended to a fourth provider, `"settings"`, wired
into `allocate()` exactly where the app bonus already sits. The prefix-gate
and per-word rescore logic is shared (factored into `category_score`,
`search.rs`) so `app_category_score`'s own behavior, and `fe4e4e9`'s
tests against it, are byte-for-byte unchanged — verified below.

`SETTINGS_CATEGORY_BONUS = 1.5` (half of `APP_CATEGORY_BONUS`'s `3.0`), not
picked for a round number: for a single-word title an app and a pane both
match equally well against (`"bluetooth"` vs. both "Bluetooth" the pane and
"Bluetooth File Exchange" the app), `category_score`'s per-word rescore
gives both candidates the *identical* base score before either bonus is
added, so the final gap between the two is **exactly**
`APP_CATEGORY_BONUS − SETTINGS_CATEGORY_BONUS = 1.5`. That keeps the launch
brief's own explicit requirement — "bluetooth must keep Bluetooth File
Exchange above the Bluetooth pane" — true by a full, decisive 1.5-point
margin, while `SETTINGS_CATEGORY_BONUS` alone is still large enough to beat
an unrelated file by a wide margin for a real pane query (18.9 vs. 14.1 for
"sound", a 4.8-point margin).

### What this fix does and does not change — a real finding, not assumed

`search::allocate`'s final output is **grouped by provider registration
order** (`app`, `file`, `clipboard`, `settings` — `AppState::new`'s own
registration order in `server.rs`) and stays that way regardless of score:
the reservation pass and the greedy interleave both only decide *how many*
rows each provider contributes, never *where* that provider's contiguous
block sits in the output (`allocate`'s own doc comment: "render order is
purely about where each section's header lands on screen ... every
provider is treated symmetrically by the allocation itself"). Traced and
confirmed against the actual code, not assumed from the doc comment alone.

Two real consequences that shaped this fix, worth recording so a future
reader doesn't re-derive them from scratch:

1. **A settings pane's score cannot move its section earlier in the
   list.** Since `settings` is registered last, its section is always the
   *last* to render, however high its score — the "puts it tenth" framing
   in the original report is literally true, but it's a structural
   consequence of registration order, not a score competition
   `settings_category_score` can win. What the bonus *does* affect: which
   candidates win the shared greedy budget when a provider (rarely
   settings itself, which almost always has exactly one real match per
   query) has more than one real candidate contesting a slot — pinned by
   `allocate_lets_the_settings_bonus_win_a_contested_slot_end_to_end`
   (`search.rs`).
2. **A settings pane's *presence* was already guaranteed, independent of
   score, before this fix.** `allocate`'s reservation pass gives every
   provider with ≥1 real candidate one slot unconditionally, and
   `panel::fit_within_budget`'s own pixel-budget reservation (client-side,
   untouched by this task) mirrors the identical guarantee for screen
   space. Reproduced against a real, isolated daemon with realistic
   candidate volume in every other provider (files, clipboard, occasionally
   apps) for all seven queries above: settings' one pane always appeared,
   both before and after this fix (see "Before/after" below) — its
   position was always last, its presence was never actually in question
   on this machine.

**This means the original report's "displays"/"bluetooth" total absence
(`settings = []`) could not be reproduced here** — mathematically,
`allocate`'s reservation pass cannot produce that outcome for a provider
that returned ≥1 real candidate, at any `limit ≥ 4` (the real client
requests 8; firstmate's own repro used 10; both are ≥ the provider count).
The only way `settings = []` is possible is if `SettingsProvider::search`
itself returned zero candidates for those two specific queries on the
captain's specific machine — a data/environment question (a different OS
build's exact pane titles, a possibly-missing extension, or a transient
state), not a ranking-algorithm bug, and outside what this fix (or any
scoring change) can address. Flagged here rather than silently dropped,
per this task's own "report anything found but not fixed" instruction.

## Defect 2 — clipboard flooding the list

`ClipboardProvider::search` scored a whole pasted paragraph with the same
unmodified `fuzzy_score` call used for a short title, plus a recency boost
of up to `+8.0`. `fuzzy_score`'s own length penalty (`title.len() * 0.02`)
is sized for title-length strings and barely dents a match found once
inside hundreds of characters of surrounding text — so a long, largely
irrelevant paste that merely *contains* the query routinely outscored
everything else, and `allocate`'s greedy phase (correctly, by design) kept
handing it every contested slot once it was the only provider left with
supply. Real numbers from one fixture entry used in this investigation (a
336-character synthetic paragraph, never real captain data — see "Method"):

```
fuzzy_score("wallpaper", <336-char fixture paragraph>) = 11.78   (raw, unnormalized)
fuzzy_score("wallpaper", "Wallpaper")                    = 36.32  (the real pane, for comparison)
```

### The fix: two independent, complementary layers

**1. `clipboard::clipboard_length_normalization`** — a candidate's raw
`fuzzy_score` is scaled by `min(1.0, CLIPBOARD_TITLE_LIKE_CHARS /
content_len)` (`CLIPBOARD_TITLE_LIKE_CHARS = 60`) before the recency boost
is added. A short paste (a URL, a line, a snippet — the common case) is
untouched; a 336-character fixture paragraph like the one above is scaled
to ~18% of its raw match score (`11.78 → ~2.10`). The recency boost is
deliberately *not* scaled — how recently something was copied is a real
signal independent of how long the copied text happens to be.

**2. `search::clipboard_max_slots`** — normalization alone cannot bound
clipboard's row count when it's the *only* other provider with remaining
supply after reservations (the exact "wallpaper" shape: clipboard has nine
real candidates, settings has exactly one, nothing else matched — no
amount of relative rescoring changes that clipboard is the only thing left
to fill the budget). `allocate` now caps clipboard's greedy-phase intake at
`limit.div_ceil(2)` — half of `limit`, rounded up — whenever at least one
*other* provider also has ≥1 candidate for the query. When the cap leaves
budget unclaimed because nothing else has more supply either, those slots
are simply left unused (a shorter list), never forced back onto clipboard.
Scoped to `"clipboard"` specifically (the same per-provider gating
`app_category_score`/`settings_category_score` already use) — a provider
whose *individual* candidates are all genuinely relevant winning most of
the shared budget (many real file matches, say) is the intended behavior of
the greedy phase, not a bug; clipboard's problem is specifically that a
high score there doesn't reliably mean high relevance for free-form pasted
text.

**The existing "clipboard always gets at least one slot" guarantee is
untouched** — the cap only bounds the *greedy* phase; the *reservation*
pass (which produces that guarantee) runs first and is unconditional.
`search.rs`'s `clipboard_is_not_capped_when_it_is_the_only_provider_with_candidates`
and `clipboard_cap_still_leaves_room_for_a_secondary_match_with_few_candidates`
tests pin both halves of this directly.

## Before/after — the seven queries, re-run against a real, isolated daemon

Same isolated `$HOME`, same nine synthetic clipboard fixtures, same real
System Settings pane data (this machine's own installed panes), same
`query_probe`, `limit: 10` — captured via `git stash`/`git checkout
8da1238 -- <files>` to get a clean, code-only A/B on identical fixture
data (not two different runs days apart):

**Before** (pristine `main`, `8da1238`):

```
sound           kinds=[clipboard x9, settings]  settings_titles=["Sound"]
storage         kinds=[clipboard x9, settings]  settings_titles=["Storage"]
appearance      kinds=[clipboard x9, settings]  settings_titles=["Appearance"]
wallpaper       kinds=[clipboard x9, settings]  settings_titles=["Wallpaper"]
accessibility   kinds=[clipboard x4, settings]  settings_titles=["Accessibility"]
displays        kinds=[clipboard x9, settings]  settings_titles=["Displays"]
bluetooth       kinds=[app, clipboard x8, settings]  settings_titles=["Bluetooth"]
```

**After** (this fix, same fixtures, same daemon binary rebuilt from the
same tree plus the commit on this branch):

```
sound           kinds=[clipboard x5, settings]  settings_titles=["Sound"]
storage         kinds=[clipboard x5, settings]  settings_titles=["Storage"]
appearance      kinds=[clipboard x5, settings]  settings_titles=["Appearance"]
wallpaper       kinds=[clipboard x5, settings]  settings_titles=["Wallpaper"]
accessibility   kinds=[clipboard x4, settings]  settings_titles=["Accessibility"]
displays        kinds=[clipboard x5, settings]  settings_titles=["Displays"]
bluetooth       kinds=[app, clipboard x5, settings]  settings_titles=["Bluetooth"]
```

Clipboard's row count is capped from 9 → 5 (limit 10 → `clipboard_max_slots
= 5`) everywhere it was previously flooding the list (the "accessibility"
case shows only 4 originally, under the cap already, so it's unaffected —
consistent with the cap only ever *reducing*, never inflating, a
provider's count). Settings' one real pane appears in every case, both
before and after — its position (always last) is unchanged, per "What this
fix does and does not change" above. This isolated fixture set has no
files/apps under Spotlight's index (an isolated `$HOME`'s fresh `Documents`
directory isn't Spotlight-indexed), so `file` rows don't appear here the
way they do in the captain's own repro against his real corpus — the
mechanism verified (settings' presence, clipboard's cap) is unaffected by
that difference; a `file`-heavy scenario is additionally covered by the
`allocate`-level unit tests (`three_providers_each_keep_their_reservation`,
`the_first_provider_cannot_be_crowded_out_by_a_consistently_higher_scoring_one_either`,
unchanged) plus the new
`allocate_lets_the_settings_bonus_win_a_contested_slot_end_to_end` and
`a_single_provider_cannot_take_the_whole_list_while_others_have_candidates`
tests.

## Regression check against `fe4e4e9`

`fe4e4e9`'s own app-ranking tests (`app_category_score_*`,
`allocate_ranks_*_end_to_end`, `allocate_does_not_let_a_scattered_app_match_crowd_out_strong_file_matches`)
pass unchanged — `category_score`'s extraction preserves
`app_category_score`'s exact behavior byte-for-byte, confirmed by running
the full `search::tests` module (26 tests, all passing) after the change.
`fe4e4e9`'s own documented queries ("code", "terminal", "chrome") were not
re-run against a live daemon in this task (no client was launched — see
"What wasn't measured"), but the exact `fuzzy_score`/`app_category_score`
numbers those tests assert on are untouched by this task's diff — this
task's changes only add a new `else if *id == "settings"` branch and a new
`clipboard_cap` check inside `allocate`, neither of which touches the
`"app"` branch or its data at all.

## Verification

- `cargo build`, `cargo test`, `cargo clippy --all-targets` all clean at
  the workspace root (99 tests in `neko-core` alone, up from 91 before this
  task; every pre-existing test, including every `fe4e4e9` test, unchanged
  and passing).
- New tests: `settings_category_score_beats_an_unrelated_file_that_merely_contains_the_query`,
  `settings_category_score_does_not_let_a_pane_beat_a_genuine_app_match`,
  `settings_category_score_does_not_boost_a_non_prefix_scattered_match`,
  `allocate_lets_the_settings_bonus_win_a_contested_slot_end_to_end`
  (Defect 1); `clipboard_max_slots_is_half_the_limit_rounded_up`,
  `a_single_provider_cannot_take_the_whole_list_while_others_have_candidates`,
  `clipboard_is_not_capped_when_it_is_the_only_provider_with_candidates`,
  `clipboard_cap_still_leaves_room_for_a_secondary_match_with_few_candidates`
  (Defect 2, `search.rs`); `short_entries_are_left_exactly_as_fuzzy_score_scored_them`,
  `long_entries_are_scaled_down_proportionally_to_their_length`,
  `a_short_exact_match_outscores_a_long_paragraph_that_merely_contains_the_query_at_the_same_age`
  (Defect 2, `clipboard.rs`).
- Verified on release binaries (`cargo build --release -p neko-daemon`),
  headless, under an isolated `$HOME`, probed over the real Unix socket via
  `neko-client` — never a hand-rolled protocol reimplementation.

### Daemon idle/loaded RSS — flat, no regression

Measured with `ps -o pid,rss,vsz`, isolated `$HOME`, both a clean `git
checkout 8da1238 -- <the three changed files>` build ("before") and this
branch's own build ("after"), same machine, back to back:

```
                          before (8da1238)   after (this fix)
idle, 15s after startup   13.8 MB            13.6 MB
after 35 Search requests  18.4 MB            16.8 MB (then flat for 5s)
```

Both flat at idle and under load, no growth trend in either — consistent
with this fix changing only per-request scoring arithmetic (a few extra
`f32` multiplications/comparisons per candidate), not allocation shape,
capture loops, or anything long-lived. The "after" number under load is if
anything lower, plausibly because the cap means fewer total `SearchItem`s
are constructed and serialized per capped query — not verified further
since neither number is a regression either way.

### What wasn't measured, and why

**Warm summon latency was not measured in this task, deliberately, and
should not be assumed unchanged from the last documented figure
(`AGENTS.md`, "Summon latency", ~2.9–6.4 ms).** This task's fix is
entirely server-side (`neko-core`'s `search.rs`/`clipboard.rs`/
`settings.rs`) and summon itself is documented as purely client-side,
off the daemon round-trip entirely (`AGENTS.md`: "summon is hotkey-press →
first-frame-after-activation, purely client-side ... a slow or unreachable
daemon cannot make summon itself slower") — so there is no code-path reason
to expect a change. But per this task's own hard constraint, added
mid-task after the captain reported he was actively working on this
machine: **the client (`neko`) was never launched, no window was opened,
`NEKO_BENCH` was never run, and no screenshot was taken at any point in
this investigation's second pass.** Only the headless `neko-daemon` binary
was exercised, probed over its socket. This is a real, stated gap, not an
oversight — re-measuring warm summon latency (via `NEKO_BENCH`, same
methodology as `docs/evidence/summon-latency.md`) is the one piece of the
original acceptance criteria this evidence does not cover, and should be
picked up in a follow-up pass once the captain isn't actively using the
machine.
