## Commands and modes

The audit's top-priority finding: the captain expects to search for a
*thing to do*, not just a row — "search for Clipboard History and enter
it," the way Raycast's own root list mixes commands in with applications.
Built as two small, orthogonal pieces on top of the existing seams rather
than a new abstraction layer: **a command** is a fifth `Provider`
(`neko_core::commands::CommandsProvider`) whose rows carry a new
`SearchItem::enters_mode` field instead of doing a daemon-side action;
**a mode** is `neko`'s own client-side UI state (`crate::modes`, `panel::
Root`) that a confirmed command transitions the panel into. Full concept
writeup, including the exact "what does a second command have to
implement" accounting the launch brief asked for (the same shape
`settings.rs`'s own doc comment gives for a fourth provider), lives in
`crates/neko/src/modes.rs`'s module doc comment — read that before adding a
second command or touching this area again.

**The wire protocol touches, and why each one is additive, not a
workaround.** Three changes, all backward-compatible field additions, no
new `Request`/`Response` variant beyond what's noted:
- `SearchItem` gained four fields: `enters_mode: Option<String>` (the mode
  id a confirmed row enters, checked by `panel::Root::confirm` *before*
  ever building a `Request::Activate` — a command's own `activate` is never
  actually called, and errors if it somehow is), `group_label: Option<String>`
  (a mode's own "Today"/"Yesterday" time-grouping, orthogonal to
  `section_label`'s provider grouping — the root list never reads it),
  `actions: Vec<ItemAction>` (the `⌘K` menu's own secondary actions, empty
  for the three providers that have nothing beyond their one primary
  action), and `source: Option<String>` (a bare "where this came from"
  value for a detail pane's own labeled field, distinct from `subtitle`'s
  already-composed sentence). All four are exactly the same "provider
  describes it, client just renders it" pattern `action_label`/`badge`/
  `icon` already established — no new per-kind `match` anywhere.
- `Request::Search` gained `provider: Option<String>` — `Some(id)` scopes
  the search to exactly that one provider's own `search()`, sorted by
  score, no cross-provider `allocate()` at all. This is the actual mode
  mechanism: entering a mode means every keystroke searches with this field
  set to the mode's own provider id instead of `None`. The daemon-side
  branch is a straight `if let`/`else` in `handle_request`
  (`neko-daemon/src/server.rs`) before the existing multi-provider path,
  which is completely untouched.
- `Request::Activate` gained `query: String` much later
  (`fm/neko-new-agent`) — the search field's contents at the moment Enter was
  pressed, for the one kind of row whose action takes an argument rather than
  naming a thing to open. Received through a defaulted
  `Provider::activate_with_query`, so every other provider is untouched. See
  "Starting an agent: the New Agent command" for the two silently-wrong
  alternatives it replaces.
- `Request::Activate` gained `action: Option<String>` — `None` is the
  existing single primary action (`Provider::activate`, unchanged);
  `Some(action_id)` routes to a new, defaulted `Provider::perform_action`
  trait method (`provider.rs`) instead. The default errors "no such
  action," so the three providers with nothing beyond their primary action
  (apps, files, settings, commands) need zero code changes at all;
  `ClipboardProvider` is the one override, for its three menu actions
  below.

**Commands (`neko_core::commands`).** One built-in command today,
`CommandsProvider::search` (id `"command"`, section label "Commands"),
returning `"Clipboard History"` with `badge: Some("COMMAND")` (pre-
uppercased on the wire, same convention `ClipboardProvider`'s `"TEXT"`/
`"LINK"` badges already use — this client never CSS-transforms text) and
`enters_mode: Some("clipboard")`. **The multi-word matching limit
documented in `docs/evidence/settings-provider-report.md`** ("keyboard
shortcuts" never matches a pane titled "Keyboard" — `fuzzy_score` needs
every query character, including the space, to appear as an in-order
subsequence of *one* title) would otherwise block "clipboard manager" (no
`m`/`a`/`n`/`g`/`e`/`r` exist anywhere in "Clipboard History" after
"Clipboard "). Fixed inside this provider, per the launch brief's own
instruction not to touch the shared `fuzzy_score` (owned by a parallel
task's `search.rs`): each command carries a short list of alias phrases
(`"Clipboard History"`, `"Clipboard Manager"`, `"Clipboard"`), each scored
independently, best score wins. "clipboard" and "clipboard history" already
match the literal title as a plain subsequence with no alias needed —
verified in `commands.rs`'s own test that pins exactly this distinction.

**Modes (`crate::modes`, `neko`).** Split the same way `onboarding/state.rs`
splits from `onboarding/view.rs`: `modes.rs` is pure, GPUI-free chrome data
(`ModeChrome`: id, scoped provider id, footer title, placeholder text,
whether it has a detail pane) plus a lookup function, unit-tested with
plain `#[test]`s; `panel::Root` is the view layer. Confirming a command row
(`Root::confirm`) calls `enter_mode`, which saves the current query, clears
the field to start the mode's list fresh (a real edit through
`TextField::set_content`, which re-runs search — by then `active_mode` is
already `Some`, so `run_search` takes the provider-scoped branch
immediately), swaps the placeholder, and — if `ModeChrome::has_detail` —
widens the *visible* panel. `Escape` (a new `Root::handle_dismiss`, registered
as a window-level `on_action` listener on `Root`'s own div so it intercepts
`DismissWindow` before `main.rs`'s global `cx.hide()` fallback ever sees it
— GPUI's own action dispatch runs window listeners in the bubble phase
before global ones, and a handled action stops propagating there by
default) or the input row's own back-arrow glyph both call `exit_mode`,
which restores the saved query verbatim and narrows the panel back.
`reset_for_summon` also force-exits any active mode — a mode is
per-summon-session state, not something that survives the panel being
hidden and re-shown.

**The real window resize this originally described (`display_placement::
resize_and_recenter`, a raw, synchronous `NSWindow` `setContentSize:`/
`setFrameTopLeftPoint:` call pair) was removed by `neko-mode-resize` and
does not exist any more — see "Mode view resize seam" below.** It resized
correctly by every measurement taken at the time; the real window, its
rendering surface, and its backing drawable all tracked the new size. What
didn't reliably track it, discovered only once a session that had summoned
and dismissed the panel a few times first was reproduced, was `gpui::
Window`'s own private `viewport_size` — the size its root element is
actually laid out and painted against every frame. A mode transition now
resizes nothing: the real `NSWindow` is a fixed `PANEL_WIDTH_WITH_DETAIL_PX`
for the process's entire lifetime, and `enter_mode`/`exit_mode` instead call
`panel::Root::update_background_bounds` (a direct native-view frame set,
`material::set_background_frame`) to move/resize the material backdrop
*within* that fixed window, in lockstep with `Render::render`'s own
`justify_center()` centering of the panel `div`.

**The two-column clipboard-history view**
(`data/neko-design/mockups/12-first-clipboard-use.html`, the frozen mockup
— not just the launch brief's prose description of the captain's Raycast
screenshots, which mentions Dimensions/Image size fields this app has no
data for and this mockup deliberately doesn't include) — left column fixed
at `theme::MODE_LIST_COLUMN_WIDTH_PX` (264px, a new geometry token, the
mockup's own `.panel-list.with-detail` width — not a change to any
*existing* frozen token), right column the preview (the entry's raw stored
content, `SearchItem::id`, not the list row's own truncated/quoted `title`)
plus three info fields: Application (`source`), Content Type (`badge`,
title-cased for display), Copied. **One disclosed deviation from the
mockup's literal copy**: "Copied" shows this app's existing relative-time
label ("12m", "3h" — `clipboard::relative_time`, already used for the row's
own accessory) rather than an absolute local timestamp like the mockup's
"Today, 9:50 AM" — no date/time-formatting dependency exists anywhere in
this codebase, and adding one (`chrono` or equivalent) for one label wasn't
judged worth it. The "Today"/"Yesterday" list-section grouping
(`clipboard::day_bucket_label`) is real but **UTC-calendar-day
arithmetic, not local-timezone-aware**, for the identical reason — a copy
made shortly before/after local midnight can land in the "wrong" UTC-day
bucket; disclosed in that function's own doc comment rather than silently
assumed correct. **One new color token**, `theme::SURFACE_INPUT` (the
preview box's recessed background) — derived by the exact same
warm-to-cool hue-swap rule every existing base-neutral token in `theme.rs`
already used for the palette re-tone (same L/C, hue swapped from ~70° to
255°), not a fresh color pick, so it doesn't reopen the closed
colour-identity question. **Not built**: the mockup's "All Types" filter
control on the right of the mode's input row — there is no real type-filter
logic behind it (clipboard has exactly two content kinds today, already
visible per-row via the badge), and a control with nothing real to do would
be a fake affordance; the back-arrow half of that row is real and built.

**The `⌘K` actions menu** (no mockup exists for this — the launch brief's
own note) is `panel::Root`'s own `ActionsMenuState`, populated from the
selected row's `SearchItem::actions` (no daemon round-trip to open it — the
data already arrived with the last search response, the same "provider
describes it" pattern the rest of this file uses). `ClipboardProvider`
offers three: "Paste" and "Copy" both call the identical `activate`
(writing to the pasteboard — see "Clipboard history" above on why neko
never simulates a keystroke, which makes these two genuinely the same
operation today, not just similarly labeled), "Delete"
(`Db::delete_clipboard_entry`) is `destructive: true`. **A destructive
action needs a second, explicit Enter to actually run** — the first only
"arms" it (`ActionsMenuState::confirm_armed`, rendered as "Confirm Delete —
↵ again"); moving the menu selection at all disarms it again, so a
confirmation can't survive being scrolled past and back. This satisfies the
brief's "a mis-keyed action cannot silently destroy an entry" requirement
without a separate modal dialog. A primary-row Enter still hides the panel
on success (matching every pre-existing `Activate` call site, mode or not);
the menu deliberately never does, on either action — reviewing/managing
entries is exactly what opening the menu is for.

**Verification: headless only, by explicit instruction — the captain was
using this machine.** No client launch, no real window, no screenshots this
pass. Covered instead: `commands.rs`/`clipboard.rs`/`db.rs`/`provider.rs`
plain `#[test]`s (command matching and aliasing, `perform_action` routing,
day-bucket labeling, delete), `server.rs` integration tests against
`handle_request` directly (provider-scoped search, the new `action` field
routing), and — the load-bearing ones for the client side —
`panel.rs`'s own `#[gpui::test]`s using `TestAppContext`/`FakeAccessibilityChecker`/a
real `NekoClient::connect` against a socket nobody's listening on (the same
headless-GPUI, no-OS-window mechanism `row_icon_cache.rs`'s own tests
already established for this crate): confirming a command row really does
enter the mode and save the prior query, `Escape` exiting it and restoring
that query, opening the actions menu populated vs. inert, and the
destructive-delete double-confirm/disarm-on-navigate behavior — all
directly on `Root`'s real methods, not reimplemented test doubles. **What
this could not verify at the time**: the real `NSWindow` resize/recenter
call, the two-column layout's actual on-screen appearance, warm summon
latency, and daemon idle memory — all needed a live client/window/daemon
this task was explicitly told not to launch. **Since closed by
`neko-mode-visual`** (see "Mode-view row anatomy and the neutral re-tone"
below): `resize_and_recenter` was verified live, twice (a fresh mode entry
and an exit/re-enter cycle), through the real `Root::confirm` path — it
correctly resizes to 760px both times. Warm summon latency and daemon idle
memory were still not re-measured by that task either, since neither the
resize path nor the daemon changed — only a client-side row-rendering
conditional and a palette constant table did.
