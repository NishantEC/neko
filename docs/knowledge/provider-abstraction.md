## Provider abstraction

`neko_core::provider::Provider` is the seam every result type in the search
list is built on — app search, clipboard history, and file search (this
task's own proof) all `impl Provider`; agent capability, whenever it's
built, is a fourth. Object-safe by construction (no generics, no `Self`
return types), so the daemon holds a plain `Vec<Box<dyn Provider>>`:

```rust
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;
    fn section_label(&self) -> &'static str;
    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate>;
    fn activate(&self, id: &str) -> Result<(), ProviderError>;
}
```

**This supersedes the old `neko_core::agent::AgentProvider` seam** (an
earlier task's placeholder trait — `id`/`query` only, no score, no
activation, registered nowhere) rather than living alongside it.
`agent.rs` is deleted. The two overlapping provider-shaped traits would
have left a future agent-capability task choosing between them, which
defeats the point of having one seam — and once a second provider
(clipboard) and a third (files) existed to design against, the shape that
actually falls out needs a normalized `score` (for cross-provider ranking)
and an `activate` method (to *perform* a result's action, not just produce
it), neither of which the placeholder had. A future agent-capability
provider is just another `impl Provider`, exactly like the three below.

**Registering a fourth provider** is `Box::new(FourthProvider::new(...))`
added to the `Vec` in `AppState::new` (`neko-daemon/src/server.rs`) plus
the `impl Provider` itself — nothing else. Concretely, it does *not* need
to touch: `neko-protocol` (no new `Request`/`Response`/enum variant —
`Request::Activate { kind, id }` is generic, routed by `kind` at runtime,
not matched per provider), `panel.rs` (no `match` on provider identity
anywhere — see "the wire protocol is now provider-agnostic" below), or
`search::allocate` (every provider is already treated symmetrically). It
*does* need its own `impl Provider` (matching/scoring against whatever its
own index is) and, if its rows want a genuinely new painted shape instead
of a cached raster, one `Glyph` variant plus one case in `panel::
glyph_element` — a data addition and a paint function, not a rendering
`match` keyed on which provider produced the row.

**The wire protocol is now provider-agnostic — `ResultKind` (a closed
two-variant enum) is gone.** `SearchItem::kind` is a plain `String` (the
producing provider's own `id()`), used for exactly two things: grouping a
contiguous run of results under one section header, and routing
`Request::Activate` back to the right provider. Every actual rendering
decision that used to be a client-side `match` on `ResultKind` is now data
the provider sets directly on the `SearchItem`: `section_label` (the header
text), `action_label` (the footer verb, e.g. `"Open  ↵"`/`"Paste  ↵"`), and
`icon: Icon` (`Image(path)` for a cached raster, `Glyph(Glyph)` for a
painted built-in shape, or `Placeholder`). `Icon`/`Glyph` stay a closed enum
on purpose — they're a bounded rendering vocabulary (what the client
actually knows how to paint), not a provider-identity dispatch; the
distinction that matters is "does the panel need to know *which provider*
produced this row to render it" (no, not anymore) vs. "does the panel need
a small closed set of paintable primitives" (yes, and always will, short of
the WASM host's own declarative UI vocabulary, explicitly out of scope
here). `panel.rs` itself has zero knowledge of "apps", "clipboard", or
"files" as concepts — `render_row`, `render_footer`, and the section-header
loop all just read data off `SearchItem`.

**`Request::Launch`/`Request::Paste` are gone too, replaced by one generic
`Request::Activate { kind: String, id: String }`.** The daemon routes it to
`state.providers.iter().find(|p| p.id() == kind)` and calls that provider's
`activate`. This is what actually makes "adding a provider" cheap in the
sense the brief means: if activation still needed a new per-kind `Request`
variant, a fourth provider would touch the protocol every time.

### Cross-provider ranking (`neko_core::search::allocate`)

Each provider's `search()` returns `Candidate { score, item }` — a
provider owns its own matching and scoring entirely; the only
cross-provider contract is "higher score is better, on roughly the scale
`fuzzy_score` produces" (every built-in provider's score is ultimately
built from it, so this held without extra normalization work — see "If you
have to choose" in the launch brief: the smaller abstraction that genuinely
works beats a general normalization scheme for imagined future providers).
`allocate` merges every provider's candidates into one bounded response in
two passes, replacing the old apps-vs-clipboard-only "give apps the full
limit, cap it to `limit - 1` if clipboard matched" special case:

1. **Reservation.** Every provider with at least one candidate reserves one
   slot before anything else is allocated.
2. **Greedy interleave.** Whatever's left of `limit` is spent one slot at a
   time on the single highest-scoring not-yet-taken candidate across every
   provider — a provider whose matches are more relevant to this query
   earns more of the shared budget than one that only barely cleared its
   floor.

**The reservation pass has to include the first-registered provider too,
not just the ones after it — caught live, not by a unit test.** The first
version only reserved a floor for providers after the first ("apps get
whatever's left, clipboard/files each get a floor"), reasoning that the
first-registered provider would naturally win most of the greedy phase
anyway. A window-scoped verification screenshot (`docs/evidence/
provider-search-three-result-types.png`'s first draft, not the committed
one) proved that reasoning wrong the moment a provider whose matches score
*consistently* higher was registered: file search, on a clean prefix match,
easily outscores an app's scattered subsequence match, and it won every
single contested slot — the **Applications** section rendered with zero
rows for a query that had real app matches. `search::allocate`'s reservation
loop now runs over every provider, first included; `search.rs`'s
`the_first_provider_cannot_be_crowded_out_by_a_consistently_higher_scoring_one_either`
test pins the exact shape of this bug so it can't come back silently. The
general lesson: a "primary provider always wins ties" assumption is itself
a form of hard-coding one provider's identity into the ranking, exactly
what this task was asked to remove — symmetry (every provider gets a floor)
is what "no provider crowds another out" has to mean once there's more than
two.

`panel::fit_within_budget`/`fit_section` do the equivalent reservation for
*pixels*, not slots, generalized the same way: `group_into_sections` splits
`results` into contiguous same-`kind` runs (order-preserving), the first
section gets whatever's left of the fixed content-area budget after every
*other* section reserves one header-plus-one-row, and an earlier section
using less than its share rolls the difference forward to later ones — the
exact 2-provider math this function has always done, now provider-count-
agnostic. Both layers (`allocate` for slots, `fit_within_budget` for pixels)
exist because a response with room for a provider's match is necessary but
not sufficient for a screen with room to render it.

### File search (`neko_core::files::FileProvider`)

The proof provider. Source of truth is the same one `apps.rs` established
for application discovery: `mdfind`, not a hand-rolled directory walk —
see `files.rs`'s own module doc comment for the full reasoning, which
includes two real, measured findings this task turned up (not
assumptions):

- **Prefix matching (`kMDItemFSName == 'query*'cd`), not the app
  provider's fuzzy subsequence matching.** A leading-wildcard query
  (`'*query*'cd`, matching anywhere in the filename) was the natural first
  attempt; a single-character version of it matched 55,191 files under a
  real, repository-heavy `~/Documents` and took **~13 seconds** to
  complete, because a leading wildcard forces Spotlight off its index onto
  a slow per-item scan. The identical query as a prefix (no leading
  wildcard) returned in well under a second — Spotlight can serve a prefix
  query from its index directly. This means file search only matches names
  that *start with* the query, not names that merely contain it (Spotlight's
  own live-typing UI makes the same trade). Below `files::MIN_QUERY_LEN`
  (2 characters), `FileProvider::search` returns without querying at all —
  a 1-character prefix is both too broad to be useful and too slow even
  with the bound below.
- **`mdfind` needs `NSUnbufferedIO=YES` here too, not just in `apps.rs`'s
  `-live` watcher.** Without it, a fast, large result set can sit fully
  buffered on the read end well past this query's own deadline even though
  `mdfind` itself would have finished quickly — confirmed the same way
  `apps.rs` confirmed it originally: a query that completed in well under a
  second at the shell produced zero results through this module's reader
  before the fix.

**Bounded regardless of query breadth.** `query_spotlight_paths` reads at
most `files::MAX_RAW_RESULTS` (40) lines through a background reader
thread (the same spawn-plus-channel shape `apps.rs`'s `run_live_watch`
uses), and the caller gives that thread at most `files::QUERY_TIMEOUT`
wall-clock time via `recv_timeout` either way, killing the `mdfind` child
the instant either limit is hit. **`QUERY_TIMEOUT` is 1.5s, measured, not
guessed**: even with the buffering fix above, *time to first streamed
result* for a broad 2–4 character prefix against this machine's real
`~/Documents`/`~/Desktop`/`~/Downloads` varied from well under 100ms to a
little over 1.2s across repeated runs of the identical query — real
variance in Spotlight's own query-planning time for a broad predicate that
this module can't smooth out. A tighter 900ms bound (the first value tried)
cut off real, useful results often enough in that range to be a regression,
not a rare tail.

**Scope: `~/Documents`, `~/Desktop`, `~/Downloads` only** (`files::
default_scope_dirs`, each checked with `.is_dir()` so a missing one is
silently skipped) — "the captain's own documents and common working
locations," per the brief, not the whole disk. Not configurable in this
task; the seam for a follow-up mirrors `clipboard_history_enabled`'s
existing daemon-setting pattern (a new `Request::{Get,Set}FileSearchScope`
pair, or a list-of-paths setting) rather than anything structural. Noise
inside those directories — `node_modules`, `target`, `.git`, build output,
anything hidden, and `.app` bundles (already covered by `AppsProvider`, so
showing the same bundle twice would be confusing, not additive) — is
filtered by `files::is_noisy` *after* the query, since `mdfind` has no
directory-exclusion flag to push it into the query itself.

**Icon: a painted `Glyph::File`/`Glyph::Folder`, not a real per-file
icon, in this task.** `icons.rs`'s `ensure_cached_icon` is generic enough
to work on any file path, not just app bundles, and reusing it was the
obvious next step — but real icon extraction is genuinely expensive
AppKit work (documented in `icons.rs`'s own module comment as "tens of ms
each"), which is why `apps.rs` only ever does it in a background pass
*after* the index is searchable, for a small, bounded (~150) app list. A
file provider has no equivalent bounded set to pre-warm — the set of
possibly-matched files is unbounded — so extracting synchronously on the
search path would reintroduce exactly the latency problem the rest of this
module works to avoid, and a background lazy-extraction pass is real,
separate design work this task didn't do. It's also **exactly the surface
a parallel task (`neko-icon-cache`) owns** (`icons.rs` and the panel's icon
slot, fixing cold-cache icons and oversized files) — inventing a second,
divergent file-icon-extraction path here risked conflicting with whatever
that task lands. The painted glyph is the honest, zero-risk choice for
this task, in the same spirit clipboard rows already use one (no per-entry
icon, "no favicon fetching in this slice" predates this task). Wiring real
file icons through `icons::ensure_cached_icon` in a background pass, once
the icon-cache task's own fixes are in, is the seam.

**Enter opens the item — same "Open  ↵" verb apps use, not a separate
"reveal in Finder" action.** `/usr/bin/open` (already `neko_core::launch::
launch_app`, reused as-is — `FileProvider::activate` is a one-line call
into it) launches a file in its default app and opens a folder as a Finder
window, matching what Enter does in Spotlight's own UI. A dedicated
"reveal in Finder" would be a real secondary action (Raycast's own
convention is ⌘-Enter for exactly this), but there is no secondary-action
affordance anywhere in this panel yet — adding one speculatively, for one
provider, ahead of any real need, is exactly the kind of imagined-future
generality the brief's "prefer the smaller abstraction" guidance rules
out.

### System Settings pane search (`neko_core::settings::SettingsProvider`)

The fourth provider, and the second proof (after file search) that the
seam holds for a genuinely new capability rather than being designed in
the abstract. Full reasoning, with the machine evidence behind every
choice, is in the module's own doc comment — read that before touching
this file again, and `docs/evidence/settings-provider-report.md` for the
discovery narrative, the real-pane-opened proof, and the before/after
latency/memory numbers. Summary:

- **Enumeration source: `/System/Library/ExtensionKit/Extensions/*.appex`,
  not the old `.prefPane` layout**, which is a dead stub on this OS (no
  `Info.plist` at all in e.g. `Displays.prefPane`). A pane is real when its
  `Info.plist` declares `EXAppExtensionAttributes.
  EXExtensionPointIdentifier == "com.apple.Settings.extension.ui"` and
  `SettingsExtensionAttributes.allowsXAppleSystemPreferencesURLScheme ==
  true` (50 of 241 total `.appex` bundles on this machine).
- **The URL target is the extension's own `CFBundleIdentifier`, never
  `legacyBundleIdentifier`** — the legacy field is missing on a third of
  panes and, where present, is sometimes shared by two different panes
  (Siri and Spotlight declare the identical legacy list), so it can't be
  the primary key. The modern bundle identifier is unique and was verified
  live to resolve every pane tested, alone.
- **One-time scan at daemon construction, no live watcher** — the same
  "sealed system volume, an OS update restarts the daemon anyway" argument
  `apps.rs` already established for `/System/Applications`, since
  `/System/Library/ExtensionKit/Extensions` sits on the same sealed
  volume. Enumeration never runs on a client's search path.
- **Display names come from each bundle's localized `InfoPlist.loctable`
  (`"en"` key), not the raw, often-internal-target-named `Info.plist`** —
  falls back to the raw plist, then a two-entry override table for the
  two panes (Battery, Headphones) that have no localized name anywhere in
  their bundle.
- **Icon: the System Settings app's own icon, shared by every pane row**
  — a per-pane icon would need Apple's private iconography stack (each
  pane names an `ISGraphicIconConfiguration.ISTypeIdentifier`, not
  resolvable through the public `NSWorkspace.iconForFile` this daemon's
  icon cache already uses everywhere else). Extracted once, in the same
  background pass `main.rs` already runs for every app icon, through the
  same `icons::ensure_cached_icon` — no second icon pipeline.
- **No settings-specific ranking bonus.** Unlike `AppsProvider`'s gated
  `app_category_score`, this provider's candidates are plain
  `fuzzy_score` — the launch brief's own requirement ("must not crowd out
  genuine application matches for app-shaped queries") is already
  satisfied without one, verified against `fe4e4e9`'s own ranking test
  queries ("code", "terminal", "chrome") producing identical top results.
- **The seam held.** Touched: one new file (`settings.rs`), one new
  function in `launch.rs` (`open_url`, `launch_app`'s identical shape
  typed on `&str` for a URL rather than a `Path`), one `pub mod` line,
  and one provider-registration line in `server.rs`. Untouched:
  `neko-protocol` (no new `Request`/`Response` variant), `panel.rs` (no
  `match` on provider identity — section label, icon, and action verb are
  already data on `SearchItem`), and `search::allocate` (already
  provider-count-agnostic).

### Daemon concurrency (why `handle_connection` now spawns a thread per request)

`FileProvider`'s `mdfind` round-trip (up to 1.5s on a broad query) exposed
a real risk in the daemon's original request-handling shape: one
persistent connection per client, one thread reading frames in a loop and
handling each synchronously before reading the next. A slow `Search` would
have queued every subsequent request on that connection — every future
keystroke, a hotkey change, an onboarding check — behind it, visibly
degrading the whole app's responsiveness rather than just that one search.
`neko-daemon/src/server.rs`'s `handle_connection` now spawns one thread per
*request*, not per connection; `handle_request` (`Search` specifically)
also fans its providers out across `std::thread::scope` so the daemon
round-trip is `max(provider times)`, not their sum. **`max(provider times)`
is no longer what a client waits to see anything, though** — see "Two-phase
search: results as you type" above: a search is answered in two frames, the
first at `max(fast provider times)`, and one thread per request is what
lets a superseded search be cancelled independently of the one replacing
it. Responses can complete
out of order relative to requests as a result — safe by construction,
since `neko-client`'s `Shared::pending` map (`neko-client/src/lib.rs`)
already matched a response to its caller by the request's own `id`, never
by arrival order. The other necessary half: `AppState`'s per-connection
writer moved from a bare `UnixStream` to a shared `Arc<Mutex<UnixStream>>`
— two request-handling threads on the same connection can now genuinely
write concurrently, and since a `UnixStream::try_clone()` shares the
underlying socket fd, two unsynchronized `write_frame` calls (each two
separate `write_all`s: a length prefix, then the payload) could interleave
mid-frame and corrupt the stream without it. Every writer for a connection
— a request's own response, and any `Event` broadcast to it — now goes
through that one lock.

### Evidence-capture hook: `NEKO_SHOW_QUERY`

`crates/neko/src/evidence.rs` gained one more env-gated hook, same pattern
as the existing `NEKO_SHOW_ON_LAUNCH`/`NEKO_BENCH`: `NEKO_SHOW_QUERY=<text>`
types `<text>` into the search field before the screenshot (via
`TextField::set_content_for_evidence`, a real edit through the same
`ContentChanged` path a keystroke takes — not a rendering shortcut), for
capturing real, non-empty search results without synthetic OS keystrokes.
**Synthetic keystrokes turned out unreliable here beyond just the hotkey
case `AGENTS.md`'s "Testing caveat" already documented** — a `System
Events` `keystroke` sent to the frontmost process right after this
window's own `activate_window()` call landed on the wrong process in
practice (confirmed live: the query field stayed empty).

Timing, precisely: when a query is set, `show_once` prints `neko: ready
for evidence setup` immediately (a deterministic "go" signal an outside
script can watch for — the pasteboard is systemwide, not scoped to any
per-run isolated `HOME`, so this is the moment to seed a real clipboard
entry), waits 2s, then applies the query; because that re-runs search the
same way a keystroke does, a second 3.5s wait follows before the
window-number line prints (the actual "now capture" signal), covering
`files::QUERY_TIMEOUT`'s worst case plus real scheduling variance.

**The recipe above is proven deterministic, not just plausible — a
five-run repeat, each in a fresh isolated `HOME`, each copying a unique
token to the clipboard the instant `ready for evidence setup` appears in
the log, passed 5/5**, all three sections (Applications, Files, Clipboard)
rendering correctly every time. The investigation that produced this
proof is itself worth recording: an earlier round of ad-hoc verification
runs saw the Clipboard section intermittently missing, and two rounds of
debugging chased it as a possible timing race before the actual cause
surfaced — the *verification token itself* (`"racetok1_...", "diagtok_...`)
didn't contain the query's characters in order, so `fuzzy_score` correctly
never matched it; a harness bug, not a product one. The lesson generalizes
beyond this one hook: when a search-driven test fails intermittently,
verify the query is capable of matching the fixture *before* suspecting
the ranking or timing code — a `python3 -c` one-liner reimplementing
`fuzzy_score`'s subsequence check against the exact fixture string is
faster than another round of live capture, and would have caught this
immediately.

### Evidence-capture hook: `NEKO_SHOW_CONFIRM`, and a live-hotkey collision it exposed

`neko-p0-fixes`'s own evidence hook, for capturing the inline activation-
error footer (see "Click-outside dismissal and inline activation errors"
above) without synthetic OS keystrokes — same reasoning as `NEKO_SHOW_QUERY`
above. Read alongside it: once the query's results render,
`panel::Root::confirm_for_evidence` drives the exact same `confirm()` path a
real Enter keystroke takes (`self.confirm(&Confirm, window, cx)` —
`Confirm` is a plain constructible unit struct, GPUI's `actions!` macro
output), then `show_once` waits for the real `Request::Activate` round-trip
before printing the window-number "now capture" line.

**Real finding while using this hook, worth recording for any future
evidence run on this machine, not just this one hook**: `main.rs` registers
a live OS hotkey unconditionally at startup whenever `AXIsProcessTrusted()`
is already true for the binary being run — `NEKO_SHOW_ON_LAUNCH` bypasses
the onboarding *screen* but not this registration. If the captain's real
daemon/client doesn't currently hold the default `⌥Space` combo (e.g. he's
rebound it), an isolated-`HOME` evidence client's registration attempt can
*succeed* — and then genuinely receive real physical `⌥Space` presses meant
for whatever the captain was actually doing, visibly resetting the evidence
run's own query field via `reset_for_summon` mid-capture (confirmed live:
three unexplained `neko: summon latency …` / `lost activation` cycles during
one capture attempt turned out to be three real hotkey presses landing on
the evidence process instead of the captain's). **Mitigation, now routine
for any evidence run that leaves `NEKO_SHOW_ON_LAUNCH`'s hotkey path live**:
before starting the client, send the isolated daemon a `Request::CommitHotkey`
for an obscure combo (e.g. all four modifiers plus `F13`) so nothing anyone
is realistically pressing can land on it. This is a persisted daemon-side
setting (`neko_core::hotkey::set_hotkey`, no live registration attempt of
its own — see "The hotkey is a runtime-configurable setting" above), so it
can be set with one request against the isolated socket before the client
process even starts. **Superseded structurally as of
`fm/neko-evidence-no-activate`**: `main.rs` now skips hotkey registration
entirely for any evidence run (`evidence::evidence_run_active`), so this
mitigation is belt-and-braces rather than the thing standing between an
evidence client and the captain's real keypresses. See "Standing safety
rule: an evidence window must never become the key window" above — that
section's own incident is what proved a remember-to-do-it mitigation
insufficient.
