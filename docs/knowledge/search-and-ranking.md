## Search and ranking

`neko_core::search::fuzzy_score` is a small, dependency-free subsequence
scorer (fzf-shaped: contiguous-run and word-boundary bonuses, a length
penalty), shared by every provider — see "Provider abstraction" below for
where each provider's own matching/scoring lives now and how their scores
are merged into one ranked response (`neko_core::search::allocate`,
superseding the old `rank_apps`/`rank_clipboard`-plus-hard-coded-reservation
shape this section used to describe). The daemon caps results server-side
(`Request::Search { limit, .. }`, the client asks for 8) — the client never
renders more than that, which is why the result list isn't virtualized (see
"gpui-component" below).

**No provider can crowd another out of a response entirely when there's a
matching entry — a first pass at this (apps vs. clipboard, before the
provider abstraction existed) got that wrong, twice.** The very first
version gave apps the full `limit` and filled whatever was left with
clipboard matches, so a query matching `limit` or more apps returned zero
clipboard results even with a real match — silent, not an error, easy to
miss (see "v1 simplification" below: the client's own first fix hit the
same shape and initially "fixed" it by dropping the whole clipboard section
rather than reserving it room, which is exactly backwards from the brief's
"same list" requirement). That was fixed by reserving clipboard's top match
a slot before apps ever ran. The provider-abstraction task then found a
*third* variant of the same bug, live rather than in a unit test — see
"Provider abstraction" below, `search::allocate`'s doc comment, for the
full account of why a reservation has to apply to *every* provider,
including whichever one is registered first.

**`cx.observe` fires on *any* notification, not just the one you meant —
this made keyboard navigation unusable.** `panel::Root` used to re-run
search via `cx.observe(&text_field, |root, _, cx| root.run_search(cx))`,
and `TextField` notifies on every cursor blink tick (~2/sec, forever —
that's how the cursor's own render updates) as well as on real edits.
`cx.observe` can't tell those apart, so search re-ran and `selected` reset
to `0` roughly twice a second, independent of typing — pressing Down looked
like it "went back to the first selection" because it did, within half a
second, every time. Fixed with GPUI's typed-event path instead of raw
notify: `text_field::ContentChanged` is emitted only from `TextField::
commit_edit` (the one real content-mutation chokepoint; blink's own
`cx.notify()` is untouched, since the cursor still needs to visibly blink),
and `Root` subscribes to that event type (`cx.subscribe`) rather than
observing the entity generically. **This is the general lesson, not just
this bug**: any entity that calls `cx.notify()` for a render-only reason
(a blink timer, a hover-state pulse, anything cosmetic) can silently drive
`cx.observe` listeners elsewhere in the app; if a re-render signal and a
"something meaningful changed" signal need to be distinguished, that's
what a typed event + `cx.subscribe` is for, not `cx.observe`. Selection
across a genuine re-search is preserved by identity (`(kind, id)`, not
index) via `panel::resolve_selection` when the previously-highlighted item
is still present in the new results, and reset to the top only when it
isn't — covered in `text_field.rs`'s and `panel.rs`'s own test suites
(`#[gpui::test]` needs `gpui`'s `test-support` feature, added as a
dev-dependency only).

**An exact app-name match only barely outscored a same-prefixed file — a
razor-thin length-penalty spread, not a reservation bug — so "terminal"
surfaced junk files, and a vendor-prefixed app ("Google Chrome") lost to a
plain file entirely.** Fixed with two scoped, deliberate additions — neither
touches `fuzzy_score` itself (still the same 12 original tests, unmodified):
`search::app_category_score` gives the `"app"` provider's own candidates a
per-word rescore (so "chrome" matches "Chrome" inside "Google Chrome" at
full strength, not just the whole string) plus a flat `APP_CATEGORY_BONUS`,
**gated on a real prefix match** (whole title or a significant word must
*start with* the query) — an unconditional version of this bonus was tried
first and caught live promoting scattered, coincidental app matches
("Xcode" for query "code") above genuinely relevant files, which is exactly
backwards. `files::is_source_artifact` demotes (never excludes) compiled/
source-code extensions' `fuzzy_score` by `SOURCE_ARTIFACT_DEMOTION`; the
first value tried (`0.5`) was too aggressive and, combined with the app
bonus above, let unboosted scattered app matches outrank real, relevant
source files — `0.85` was the value that survived live verification against
the captain's own file corpus. Full before/after numbers, the two live
regressions the first-tried constants caused and why, and the resolution of
a previously-unresolved "duplicate `finders.py`" report (two genuinely
different files vendored inside a Python virtualenv's `site-packages` — now
filtered as noise, the same category `node_modules`/`vendor` already are):
`docs/evidence/ranking-before-after.md`.

**`allocate`'s section order is decided by content strength, not provider
registration order — this was true through `neko-ranking-2` (below) and was
deliberately left for a later task; `neko-section-order` is that task.**
Through `neko-ranking-2`, the final `flat_map` iterated `providers` in
exactly the order `AppState::new` registered them (`app`, `file`,
`clipboard`, `settings`, `command`), always — a provider's score, however
boosted, could change *how many* rows it got but never move its section
earlier or later. That was a real, captain-reported defect on its own:
typing "clipboard history" — a phrase the `command` provider's own alias
table matches almost perfectly — still rendered **Commands** beneath the
always-reserved **Clipboard** section, purely because `command` registers
last. `search::allocate` now adds a third pass after reservation and greedy
interleave: sections are stable-sorted by their own top surviving
candidate's score, descending, reusing the exact score already established
as "comparable enough" for the row-level greedy interleave rather than
inventing a second cross-provider scale — ties resolve to registration
order for free, since a stable sort with no secondary key leaves
equal-scoring providers exactly where `providers`' input order already put
them. `panel::fit_within_budget` needed no code change for this — it
already just spends the pixel budget on whatever order `results` arrives
in, so "the first section gets the most generous budget" was already
correct once "first" means "most relevant" instead of "app" specifically.
**One second-order defect this surfaced, live, not assumed**: reusing the
row-level score as-is let a clipboard entry copied at the same instant as
the query out-rank a decisive command match purely on its recency boost
(up to `clipboard::CLIPBOARD_RECENCY_BOOST_CEILING`, 8.0) — freshness is a
legitimate *row*-ordering signal within Clipboard, not a *section*-strength
signal against other providers. `allocate`'s section-ordering pass
discounts a clipboard section's top score by that same constant (shared
with `clipboard.rs`, not re-declared, so the two can't drift apart) only
for this one comparison — `candidate.score` itself, and therefore every
row-level ranking, is untouched. Any future per-provider category bonus
still changes *which* candidates win the shared greedy budget as before;
it now also feeds section placement, both deliberately. Full before/after
query traces (all four captain-reported shapes, plus the recency-discount
finding) and the verification methodology (why the real daemon was never
launched for this): `docs/evidence/section-order-report.md`.
`docs/evidence/settings-and-clipboard-ranking.md` still has the full trace
for the registration-order-only era described above.

**System Settings pane matches scored too low, and clipboard could flood
the list — two more captain-reported defects, `neko-ranking-2`.**
`settings.rs`'s original choice to leave pane candidates completely
unboosted (reasoning: plain `fuzzy_score` already keeps a pane from
crowding out a genuine app match) overcorrected — an exact pane title also
lost to *everything else*, files and clipboard included. `search::
settings_category_score` extends `app_category_score`'s exact shape (a
shared `category_score` helper factors out the prefix-gate/per-word-rescore
logic both now use) to a fourth provider, `"settings"`, with a smaller,
deliberately-sized bonus (`SETTINGS_CATEGORY_BONUS = 1.5`, half of
`APP_CATEGORY_BONUS`) so a genuine app match for an app-shaped query
("Bluetooth File Exchange" for "bluetooth") still wins by a full 1.5-point
margin. Separately, `ClipboardProvider::search` scored a whole pasted
paragraph with the same unmodified `fuzzy_score` plus up to `+8.0` of
recency boost — `fuzzy_score`'s own length penalty is sized for
title-length strings and barely dents a match found once inside hundreds of
characters of text, so a long, largely-irrelevant paste routinely
outscored everything else and (correctly, by the greedy phase's own design)
won every contested slot once it was the last provider with remaining
supply: nine of ten rows for one real "wallpaper" query, and a privacy
concern given the captain's real clipboard holds invoices and client
correspondence. Fixed with two independent layers: `clipboard::
clipboard_length_normalization` scales a candidate's raw match score down
proportionally once its content passes a title-like length threshold
(`CLIPBOARD_TITLE_LIKE_CHARS = 60`), applied before the recency boost;
`search::clipboard_max_slots` caps clipboard's own greedy-phase intake at
half of `limit` (rounded up) whenever at least one other provider also has
a candidate — scoped to `"clipboard"` specifically, the same per-provider
gating the category bonuses use, since a provider whose *individual*
candidates are all genuinely relevant winning most of the shared budget is
the greedy phase working as intended, not a bug. The pre-existing "clipboard
always gets at least one slot" guarantee (from `allocate`'s unconditional
reservation pass) is untouched — the cap only bounds what the *greedy*
phase can additionally hand it. Full before/after numbers (a clean
`git checkout <prior-commit> -- <files>` A/B on identical fixture data, not
two separate runs), the real `fuzzy_score` values both bonuses were tuned
against, daemon idle/loaded RSS measurements, and an investigated-but-not-
reproduced finding (the original report's "displays"/"bluetooth" pane
*totally* absent, not just last — mathematically impossible from
`allocate`'s reservation pass alone if the provider returned any real
candidate, so likely a `SettingsProvider` data/environment difference on
the captain's own machine, not a ranking bug): `docs/evidence/
settings-and-clipboard-ranking.md`. That task's own hard constraint — no
client launch, no window, no `NEKO_BENCH`, no screenshot, because the
captain was actively using this machine — means warm summon latency was
not re-measured; the evidence file states that gap plainly rather than
assuming the last documented figure still holds.
