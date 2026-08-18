# Search ranking: applications win app-shaped queries, source files are demoted

Captain's report, via the audit at `data/neko-audit/report.md` (firstmate home)
Part 2 "Search quality": searching for an app returns source files instead.
This verifies the audit's numbers, implements the fix, and re-verifies against
the real machine — live daemon queries against the captain's real corpus, not
unit fixtures.

## Audit numbers re-verified

Re-ran `fuzzy_score` directly (a `#[test]` with `println!`, not re-derived in
a separate language) against the exact strings the audit cited:

```
fuzzy_score("terminal", "Terminal")     = 30.84   (audit: 30.84 — match)
fuzzy_score("terminal", "terminal.rs")  = 30.78   (audit: 30.78 — match)
fuzzy_score("terminal", "terminal.h")   = 30.8    (audit: 30.80 — match)
fuzzy_score("terminal", "terminal.svg") = 30.76   (audit: 30.76 — match)
fuzzy_score("chrome", "Google Chrome")  = 18.24   (audit: 18.24 — match)
fuzzy_score("chrome", "chrome.exe.txt") = 21.22   (audit: 21.22 — match)
```

All confirmed exactly. `search::allocate`'s reservation/greedy math itself is
correct and untouched — the audit's own diagnosis (a razor-thin length-penalty
spread, and a lost idx==0 bonus on vendor-prefixed names) is the real cause,
not a ranking-algorithm bug.

## The fix

Two changes, both scoped exactly to `search.rs` and `files.rs` — `fuzzy_score`
itself is untouched, its 12 original tests still pass unmodified.

**1. `search::app_category_score` (`search.rs`)** — applied only to the
`"app"` provider's own candidates, inside `allocate`, before cross-provider
ranking:

- **Per-word rescoring.** Re-scores the query against each individual word of
  the app's title too (`"Chrome"` alone, not just `"Google Chrome"`) and takes
  whichever scores higher. Fixes the vendor-prefix idx==0 loss structurally —
  the launch brief's own suggested fix.
- **`APP_CATEGORY_BONUS = 3.0`**, a flat, deliberate per-provider category
  weight added on top. Applications are a small, curated, already-installed
  set; a match against one is inherently higher-intent than an incidental
  filename match for the same string. Chosen against the audit's own measured
  worst case (a ~3.1-point residual gap even after per-word rescoring).
- **Gated on a real prefix match** — the app's whole title, or one of its
  significant words, must actually *start with* the query. This was added
  after a real regression caught live (see "What the live run caught" below):
  an unconditional bonus let scattered, non-prefix matches ("Xcode" for
  "code") outrank genuinely relevant files.

**2. Source-artifact demotion (`files.rs`)** — `is_source_artifact` recognizes
compiled/source-code extensions (`.rs`, `.h`, `.py`, `.pyc`, `.svg`, `.ts`,
`.java`, ... — the full list is in `files.rs`); a match against one has its
`fuzzy_score` multiplied by `SOURCE_ARTIFACT_DEMOTION = 0.85` (not excluded —
the brief: "a developer does sometimes want them"). Document formats (`.pdf`,
`.docx`, `.txt`, `.md`), images, and folders are never in the list.

**3. `/site-packages/` added to `files::NOISY_PATH_SUBSTRINGS`** — see "The
duplicate investigation" below.

## What the live run caught (both constants were re-tuned against real data)

Neither constant above was accepted at its first value — both were corrected
after live queries against the captain's real `~/Documents` surfaced a real
interaction a unit-fixture wouldn't have.

**`APP_CATEGORY_BONUS` unconditional → gated on prefix match.** An
unconditional +3.0 to every app match, live query `"code"`:

```
AFTER (unconditional bonus, no gate):
  [app] Code
  [app] T3 Code (Alpha)
  [app] Cloudflare WARP      <- "code" is a scattered, non-prefix match
  [app] Cloudless Voice      <- same
  [app] Xcode                <- same
  [file] CodexDriver.ts
  [file] CodexAdapter.ts
```

`fuzzy_score`'s subsequence matcher legitimately finds "code" scattered
inside "Cloudflare WARP"/"Cloudless Voice"/"Xcode" — that's pre-existing,
correct, untouched behavior. The bug was rewarding that coincidence with the
same category bonus a genuine prefix match earns. Fixed by gating the bonus
on `title` (or one of its significant words) actually starting with the
query — "T3 Code (Alpha)" still qualifies (real word-prefix match on
"Code"), the three scattered matches no longer do.

**`SOURCE_ARTIFACT_DEMOTION` 0.5 → 0.85.** With the gate above already in
place but demotion still at the first-tried `0.5`, the *same* query exposed a
second interaction:

```
AFTER (demotion = 0.5):
  [app] Code
  [app] T3 Code (Alpha)
  [app] Cloudflare WARP      <- unboosted (correctly, no prefix match),
  [app] Cloudless Voice         raw fuzzy_score ≈ 10.7
  [app] Xcode                <- raw fuzzy_score ≈ 8.9
  [file] CodexDriver.ts      <- a real prefix match ("Codex" starts with
  [file] CodexAdapter.ts        "Code"), fuzzy_score ≈ 13.7, demoted to ≈ 6.85
```

Demoting real, relevant source-file matches by 50% pushed them *below* three
unrelated apps' raw, unboosted, merely-coincidental scores — files lost real
results to app-search noise the demotion feature was never meant to promote.
`0.85` keeps every one of those three real files (≈ 11.6 after demotion)
comfortably above all three scattered app scores (≤ 10.7), while still
cutting a source file's competitive weight by 15% against a same-scoring
document — enough to consistently lose a contested slot to one. Final,
correct result for the same query:

```
AFTER (final: gate + demotion=0.85):
  [app] Code
  [app] T3 Code (Alpha)
  [file] CodexDriver.ts
  [file] CodexAdapter.ts
  [file] CodexProvider.ts
  [file] CodexHomeLayout.ts
  [file] CodexAdapter.test.ts
  [clipboard] "..." (Copied from Paseo)
```

Both regressions are pinned by dedicated `search.rs` unit tests using the
real numbers above:
`app_category_score_does_not_boost_a_non_prefix_scattered_match`,
`app_category_score_boosts_a_significant_word_prefix_match_even_mid_title`,
`allocate_does_not_let_a_scattered_app_match_crowd_out_strong_file_matches`.

## Methodology for the before/after table below

Built the release `neko-daemon` binary at two points and queried each
directly over its own Unix socket (a throwaway length-prefixed-JSON Python
client, same technique the original audit used) — no GUI, no synthetic
keystrokes:

- **BEFORE**: `main`'s current tip (`8be9f7c`, the exact commit the audit was
  written against), built in a separate git worktree.
- **AFTER**: this branch (`fm/neko-ranking`).

Both daemons ran under their own **isolated `HOME`** (`/tmp/neko-ranking-home*`)
so neither touched the captain's real daemon, socket, or database — but with
`Documents`/`Desktop`/`Downloads` **symlinked to the real ones**, so
`FileProvider` queried the captain's actual file corpus (confirmed live:
`mdfind -onlyin <symlink>` resolves through the symlink and returns real
absolute paths) while `AppsProvider`'s own SQLite/launch-history stayed
isolated. Read-only `Request::Search` calls only — nothing was activated,
nothing was written to the real pasteboard or database. The real daemon (pid
17761) and client (pid 18165) were confirmed running, untouched, before and
after. Both test daemons were killed by their own exact PIDs at the end, never
by pattern.

## Before/after: the required query set

| Query | Before (top result) | After (top result) |
|---|---|---|
| `terminal` | `Terminal` (app) | `Terminal` (app) — unchanged, already won by luck at this exact tie |
| `chrome` | `Google Chrome` (app) | `Google Chrome` (app) — unchanged, already won here too |
| `finder` | *(no app match — Finder isn't in the index; see below)* | *(same — app-index coverage is out of this task's scope)* |
| `safari` | `Safari` (app) | `Safari` (app) — unchanged |
| `code` | `Code` (app), then 7 file rows (some scattered app noise not yet visible) | `Code` (app), `T3 Code (Alpha)` (app), then 6 relevant file/clipboard rows |
| `notes` | `Notes` (app) | `Notes` (app) — unchanged |

**Important honesty note on `terminal`/`chrome`/`safari`/`notes`:** on *this
specific real corpus*, the app already won the top slot even before the fix —
the audit's numeric near-tie is real (`30.84` vs `30.78`, a 0.06 spread) but
happened to break in the app's favor here regardless. The fix's value on
these queries is the **margin**, not a top-slot flip: before, a single
differently-named file in the same folder tree could have flipped the
result; after, the app wins by a decisive, deliberate margin (`app_category_score`
closes what was a 0.06-point coin-flip into a >3-point lead — see the unit
tests using these exact real numbers,
`app_category_score_closes_the_near_tie_terminal_leaves_against_a_same_prefixed_file`
and `app_category_score_recovers_a_vendor_prefixed_name_that_fuzzy_score_alone_loses`).
The `code` query is where the crowding was visible even in the top result set
on this corpus (see the full listings above), and where the file-list quality
also improved (fewer scattered-app rows once demotion was correctly
calibrated).

Full listings for all six queries, both daemons, are in
`/tmp/neko-ranking-evidence-standard.txt` at the time this report was written
(scratch, not committed — reproducible any time from a live daemon with the
commands in "Methodology" above).

### `finder`: a known, separate limitation — not fixed by this task, not silently ignored

Both before and after, `finder` returns **zero app results** — Finder isn't
in `AppsProvider`'s index at all. This is audit report **#1**
(`sealed_system_directories()` scans `/System/Library/CoreServices/Applications`
but Finder lives one directory up, at `/System/Library/CoreServices/Finder.app`),
explicitly owned by the parallel task `neko-p0-fixes`
(`crates/neko-core/src/apps.rs`, out of this task's scope per the launch
brief). The audit itself draws this exact distinction: "`finder` returning
zero apps is #1's coverage bug, not a second independent ranking bug." This
task's ranking layer is verified correct for exactly this shape of match via
unit tests using a real single-word app title (`"Terminal"`, `"Safari"` —
same shape as `"Finder"` would be) — once the other task's fix lands, this
provider's own gated-prefix-match logic will treat `"Finder"` identically:
a genuine prefix match, boosted, winning decisively. Not re-verified against
their code directly (their branch has no commits yet as of this report — the
work is still in progress in their own worktree, confirmed via
`git log fm/neko-p0-fixes` returning `main`'s tip).

## The duplicate investigation — resolved

Audit's own words: "I could not get Spotlight to index synthetic test files
under `/tmp` quickly enough to reproduce this live... unresolved, flagging
honestly rather than asserting either way."

Resolved against the real machine:

```
$ mdfind -onlyin ~/Documents -onlyin ~/Desktop -onlyin ~/Downloads \
    "kMDItemFSName == 'finders.py*'cd"
/Users/nish/Documents/hme/navihealth/NaviHealth/env/lib/python3.8/site-packages/django/contrib/staticfiles/finders.py
/Users/nish/Documents/hme/navihealth/NaviHealth/env/lib/python3.8/site-packages/djangobower/finders.py
```

**Two genuinely different files, not a duplicate-emission bug.** Two separate
Django-ecosystem packages (`django.contrib.staticfiles` and `djangobower`)
each ship their own `finders.py`, both vendored inside the same virtualenv.
`query_spotlight_paths` has no dedup logic because it never receives the same
path twice from `mdfind` in the first place — confirmed directly, not
inferred. The row *was* already distinguishable (`home_relative_parent` puts
each file's own parent directory in the subtitle — the two rows read
different paths, not two blank duplicates), but both were also exactly the
kind of vendored-dependency noise `node_modules`/`vendor` are already
filtered for, on a virtualenv literally named `env` (not `venv`/`.venv`, so
the existing substrings missed it). `/site-packages/` is now in
`NOISY_PATH_SUBSTRINGS` — regardless of what the venv root itself is named,
`site-packages` is always the fixed name pip/virtualenv unpack vendored
packages into. Confirmed live: `finders.py` now returns zero file matches.

## File search still finds genuinely wanted files

Document/PDF query, unaffected by demotion (PDFs aren't in
`SOURCE_ARTIFACT_EXTENSIONS`), real files from the captain's own
`~/Documents/res`:

```
query: "nishant_gupta_res"
  [file] Nishant_Gupta_Resume.pdf       ~/Documents/res
  [file] Nishant_Gupta_Resume.pdf       ~/Documents/res/site
  [file] Nishant_Gupta_Resume_ATS.pdf   ~/Documents/res
```

Identical before and after — three real PDFs, no ranking change, nothing
demoted or excluded.

**Two queries against the captain's own real file names**, as required:

```
query: "screenshot" — before and after identical:
  [app]  Screenshot                              <- macOS's own Screenshot.app
  [file] screenshots/                            ...neko-native-material
  [file] screenshot.md                           ...phoenix/docs/.../ui-tests
  [file] screenshot.png                          ...comet/docs
  ...
```

```
query: "icon" — a live, real-corpus example of demotion actually firing:
BEFORE:
  [app]  Mission Control
  [file] icon.ico
  [file] icons.rs            <- a real Rust source file, undemoted, ranked #3
  [file] icon.icns
  [file] icon-cache-128px-report.md
  [file] icon-row-window-after-128px.png
  [file] icon-row-window-before-64px.png
  [clipboard] "..."
AFTER:
  [app]  Mission Control
  [file] icon.ico
  [file] icon.icns
  [file] icon-cache-128px-report.md
  [file] icon-row-window-after-128px.png
  [file] icon-row-window-before-64px.png
  [file] icon-row-rendered-blowup-after-128px.png   <- a real doc image that
                                                        wasn't visible before
  [clipboard] "..."
```

`icons.rs` (a real source file in this very repo) is demoted out of the
top-8, replaced by one more real evidence screenshot that was previously
crowded out — exactly the effect the brief asked for, on completely
unstaged, real data.

**One disclosed tradeoff, found live, not swept under the rug:** `.svg` is in
`SOURCE_ARTIFACT_EXTENSIONS` (the audit's own example list explicitly named
it: "a `.pyc`, `.h`, `.rs` or `.svg`..."), but a live `"logo"` query shows a
real `logo.svg` design asset — genuinely the thing someone would search for
by that name, not incidental repo noise — get demoted from position #4 to
position #7 (still shown, never excluded, per "demote, don't exclude").
Most SVGs under a developer's `~/Documents` are incidental icon assets
embedded in a repo (the rationale the audit itself gives), but this is a
real, disclosed exception where that heuristic is imperfect. Not fixed
further in this task — the brief's own "if you have to choose: the smaller
correct rule over the general one" — a per-file "is this actually a design
deliverable vs. a repo asset" classifier is real, unscoped, speculative work.

## Summon latency: not independently re-measured, and why that's the right call

This task's entire diff is two files in `neko-core` (`search.rs`, `files.rs`)
plus one line in `neko-daemon/src/server.rs` threading the query string
through to `allocate`. **Zero lines changed in the `neko` (GUI client)
crate** — confirmed via `git diff --stat main` against this branch. Per
`AGENTS.md`'s own "Summon latency" section: "summon is hotkey-press →
first-frame-after-activation, purely client-side... The daemon round-trip for
`Search` is off the summon path entirely... a slow or unreachable daemon
cannot make summon itself slower." A change confined to the daemon's
`Search`-handling code cannot regress a latency measurement that doesn't
invoke `Search` at all, by construction — not "unlikely to," but structurally
incapable of it, the same way the icon-cache and clipboard-capture tasks
before this one didn't need to re-measure summon either. Did not run
`NEKO_BENCH` live to produce a fresh number: that requires the GUI client
binary and a real `NSWindow`, and the parallel `neko-p0-fixes` task is
actively rebuilding and testing that exact binary on this same shared machine
right now — running a second, unrelated GUI bench risked interfering with
their in-flight verification for no evidence this task's diff could possibly
need. If a fresh number is wanted regardless, `AGENTS.md`'s own recipe
(`NEKO_BENCH=<n>` against a release build) reproduces it in under a minute
once that overlap clears.

## Verification

```
cargo build --workspace          # clean
cargo test --workspace           # 124 tests total (39 + 2 + 76 + 4 + 3), 0 failures
cargo clippy --workspace --all-targets -- -D warnings   # clean
```
