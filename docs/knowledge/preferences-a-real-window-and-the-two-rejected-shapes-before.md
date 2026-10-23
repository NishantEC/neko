## Preferences: a real window, and the two rejected shapes before it

`fm/neko-preferences`, from the captain's own *"it's better to have a
settings or preference as well... we haven't built the settings panel yet,
right?"*. Three settings, chosen by him: **the summon hotkey, launch at
login, and the file-search folders**. Clipboard-history on/off was
deliberately left out even though it is the cheapest possible row.

**Read this section before proposing a surface for anything settings-like.**
This shipped in three shapes in one sitting, and the two that were discarded
are the useful part of the record:

1. **In-panel mode with drill-in sub-screens.** A `preference` mode plus
   `preference.hotkey` and `preference.folders` as modes of their own. Forced
   `active_mode: Option<ActiveMode>` to become a `mode_stack: Vec<ActiveMode>`
   so Escape could pop one level.
2. **In-panel sidebar with in-pane editing.** `has_detail: true` like
   clipboard history, and the captain's own follow-up — *"they don't have to
   open their own window as well, right?"* — replaced the drill-ins with a
   `PaneFocus` model: Enter moved the keyboard into the detail pane, Escape
   brought it back. **The mode stack was reverted here**, because with
   nothing nesting it was generality nothing asked for.
3. **A real window** (shipped), after the captain compared it against
   Raycast's own settings window.

**Why the window is right, stated as a rule rather than a preference.** A
launcher panel is a *transient* surface: it hides on click-outside, its field
is a query, its Enter means "do the thing and get out of the way", and — the
load-bearing one — it is a **non-activating** `NSPopUpWindowLevel` panel.
Settings are the opposite of transient. They want a surface you can leave
open beside the thing you are configuring, and they need controls (a path
you type, a control that records raw key presses) that only make sense in a
window that can genuinely become key. Both earlier shapes were fighting that
one fact.

**Nothing daemon-side changed across all three shapes.** The settings live in
`neko_core::preferences` behind the ordinary `Provider` seam, and the window
reads and writes them through exactly the requests the panel used: a scoped
`Request::Search` per list, `Request::Activate` per change. That is the seam
working — the surface is the client's own business. `neko-protocol` gained
exactly one thing across the whole task: a `Glyph::Sliders` variant plus its
paint case, the documented cost of a new painted shape.

**The window** is `crate::preferences`, split `state.rs` (pure, unit-tested:
which tab, and what a recorded key press means) / `view.rs` (GPUI and all
I/O), the same split `crate::onboarding` uses. Chrome matches onboarding's
exactly — frameless inset title bar, native traffic lights repositioned into
it, `WindowKind::Normal`, fixed 720×520, opaque (it is read for minutes at a
time, so legibility beats the vibrancy that makes a transient overlay feel
light). Three tabs: **General** (summon hotkey, launch at login), **Search**
(the folder list), **About**. **There is no AI tab** — a tab bar padded out
with tabs that say "nothing here yet" is worse than a small one.

- **It is opened through an injected closure**, `panel::PreferencesOpener =
  Rc<dyn Fn(&Window, &mut App)>`, for the same two reasons `AppearanceSetter`
  is: the pieces it needs (client, live registrar, single-window slot) belong
  to `main.rs` rather than to a list of results, and **GPUI's test platform
  `unimplemented!()`s — panics — on `open_window`, `App::hide`, and every
  native window call**, so an unconditional call from `confirm` takes every
  panel test down with it. Dismissing the panel lives *inside* that closure:
  it is part of "open Preferences", not a second thing a caller might forget.

- **Dismissing the panel here is `material::order_out`, never `cx.hide()` —
  a second window changes what "hide" has to mean.** `App::hide` is
  `[NSApp hide:]`: it hides *every* window this app owns. That was
  indistinguishable from "hide the panel" for as long as the panel was the
  only window that could be active, and stopped being so the moment
  Preferences became a real one — the first version of this genuinely called
  `cx.hide()` right after opening the window, i.e. asked macOS to hide the
  window it had just opened. **The same trap is in
  `main.rs`'s click-outside observer**, which hides on the panel losing
  activation: opening Preferences *is* the panel losing activation. It now
  asks `cx.active_window()` first — "did focus go to another neko window, or
  out of neko entirely?" — and only hides the app in the second case, which
  is also the only case where macOS has a previous app to restore focus to.
  That question needs no state of our own, which is why it is preferred over
  tracking whether a Preferences window happens to be open.
- **`SharedPreferencesSlot` keeps it singular.** Confirming the row again
  focuses the live window; two settings windows could disagree on screen
  about what a setting currently is.
- **Every value is re-read from the daemon after every change**, never
  mutated optimistically — installing a LaunchAgent can genuinely fail, and a
  settings window showing a value it merely *hopes* is true is the exact
  defect `set_launch_at_login` is written to avoid.
- **`Response::Error`'s message is shown verbatim.** The daemon already
  phrases these for a person ("not a folder: /nope"); rewording loses detail.

**The rebind goes through the *same* `HotkeyController` the summon loop
uses**, exactly as onboarding step 09 does — a combination proven live in
Preferences is the registration that summons afterwards, not a second one.
`hotkey_client::HotkeyRebinder` is an injected trait held in a **deferred
slot** (`SharedRebinder = Rc<RefCell<Option<...>>>`), because the controller
cannot exist until the daemon has answered with the current combo, which is
after the window and `panel::Root` are created; `main.rs` fills it then.
`None` renders as "neko is still starting up" rather than silently ignoring a
press. Order matters and matches onboarding: **register live first, persist
only on success**, so a combination the OS refuses never reaches storage and
a working hotkey is never lost. The daemon's `CheckHotkeyConflict` heuristic
supplies the human-readable *reason*, since a Carbon failure is an opaque
status code. `onboarding::state::canonicalize_key_name` moved from `view.rs`
to `state.rs` and is shared by both callers, so the two can never disagree
about what a physical key is called. `state::candidate_from_press` **ignores
a bare modifier press rather than rejecting it** — gpui reports those as
ordinary key presses, and treating one as a candidate would flash "no key"
every time somebody started holding ⌘.

**Two providers, one of them mode-only — a new registry in `AppState`.**
`neko_core::preferences::PreferencesProvider` (id `"preference"`, section
"Preferences") is registered in the root list on purpose: typing "hotkey"
reaches the setting itself. It **cannot** be called `"settings"` —
`neko_core::settings::SettingsProvider` already owns that id for macOS System
Settings panes, which is also why the command is titled "Preferences" with
`"Settings"` only as an alias (verified live: a root query for "settings"
returns macOS's own **System Settings** app *and* neko's **Preferences**
command, unambiguously). Its sibling `FolderScopeProvider` (id
`"folder-scope"`) is registered in a **second** list,
`AppState::mode_providers`: reachable by an explicitly scoped search and by
`Request::Activate`, never by a root-list query, because a configured
search-folder path is not a result anybody wants back from the root list.
`AppState::all_providers()` is the union, used by the scoped-search and
activate paths only. **This is the seam for any future list that only means
something inside its own surface.**

**Launch at login writes a `~/Library/LaunchAgents` plist, not
`SMAppService`.** `SMAppService::mainApp` registers *the calling app's
bundle*, and neko is a bare Mach-O (`target/release/neko`), not a `.app` —
there is no bundle to register. The plist names the **client** binary
(resolved as the sibling `neko` next to the daemon's own `current_exe()`),
never the daemon: the client spawns the daemon itself, while starting the
daemon alone would leave no window to summon. `launchctl load -w`/`unload -w`
rather than `bootstrap`/`bootout` purely to avoid a `libc` dependency for
`getuid()`. **If neko is ever packaged as a `.app`, move this to
`SMAppService`** — the plist route needs the binary to stay at one path,
which a real install guarantees and a `cargo build` output directory does
not. The flag and the agent on disk are always written together, so "On" can
never mean "persisted true, nothing installed".

**File-search scope is now live-configurable.** `files::FileProvider` holds a
`Scope` (`Fixed` for tests, `Configured(Arc<Mutex<Db>>)` for the real daemon)
and re-reads `preferences::get_search_folders` on every query rather than
caching, so a folder added in the window takes effect on the next keystroke
with no restart and no invalidation message — one KV point query per search,
far below the `mdfind` round-trip the same call is about to make. An unset
key falls back to `files::default_scope_dirs()`, so an untouched install
behaves exactly as before; a persisted **empty** list is honoured as a real
choice ("search nothing"), and a corrupted value degrades to the default, the
same rule `themes.rs` applies.

**An empty root query must not reach every provider —
`Provider::answers_empty_root_query`, caught live from a captain
screenshot.** With nothing typed, the root list was returning seventeen
**Themes** rows ahead of everything else and a **Preferences → Launch at
Login** row underneath them. Neither is a useful answer to "I have not asked
for anything yet", while apps (top apps) and clipboard (recent entries)
genuinely are.

**Clipboard opted out of the empty root query later, on captain
instruction, and the reason generalises.** Apps are a useful thing to be
shown unprompted; the most recently copied thing very often is not. A
password manager's payload is already filtered (`is_privacy_marked`), but
an invoice, a client email or a chunk of somebody's source is not, and
rendering it the instant the panel opens puts it on screen in front of
whoever happens to be standing there. Typing is the signal that it was
actually wanted — and clipboard search itself is untouched, as is the
`Clipboard History` mode, which scopes to the provider explicitly and so
never passes through the root-list guard at all.

The fix is a defaulted trait method, not a guard inside `search()`, and the
reason is worth keeping: **`search("")` serves two opposite questions**. It
is both the root-list search with nothing typed *and* the call a surface
makes when it deliberately scopes to one provider and wants its whole list —
the `Themes` mode's browsable palette list, and the **Preferences window
loading its own values**. A guard inside `search()` would have silently
broken the window, which is exactly what a first attempt at this did before
that call site was noticed. `themes.rs`'s own doc comment had named this
distinction for a long time without anything being able to act on it; there
was no way for a provider to express it until this method existed. The
daemon applies it in `handle_request`'s **root-list branch only** — the
scoped branch returns earlier and never sees the filter.
Pinned by `server.rs`'s own
`an_empty_root_query_never_returns_settings_or_theme_rows_but_a_scoped_one_still_does`,
which asserts both halves.

**Verified over the real wire protocol against a real daemon**, not just by
unit test: an empty root query returns Applications / Commands / Clipboard
and nothing else; "hotkey" in the root list leads with **Preferences → Summon
Hotkey ⌥Space**; the scoped `preference` search returns all three rows with
live accessories; the scoped `folder-scope` search returns the configured
folders; `folder-scope` is absent from a root query for "documents". Both
mutations were exercised and then **reverted, leaving no residue** — the
launch-at-login toggle really did write and then remove
`~/Library/LaunchAgents/com.neko.launcher.plist` with the correct client path
inside it, and a folder add/remove round-tripped with a bogus path correctly
rejected inline (`not a folder: /nope/not/real`).

### Three real defects in one control, none visible from reading the code

The Summon Hotkey recorder said "Listening…" and recorded nothing. Three
separate causes, found one at a time, each only by a live readback — two
wrong hypotheses were killed by logs before the third stuck. **The general
lesson: a control that takes keyboard input has three independent
preconditions, and failing any one of them looks identical from the
outside.**

1. **The window has to be key, and one opened from the summon panel is
   not.** The panel is `NSNonactivatingPanelMask` by design (style-mask
   readback `0x8080`, bit 7) — clicking the launcher must not steal
   activation from whatever you were working in — so a window opened from it
   inherits an inactive app: visible, clickable, never key. **AppKit
   delivers clicks to a non-key window but not key events**, which is
   exactly the "the button responds, typing does nothing" shape.
   `open_window` now calls `cx.activate(true)` (make *neko* frontmost) *and*
   `window.activate_window()` (make *this window* key), with
   `material::is_key_window` read back and logged — same "verified, not
   trusted" discipline as `verify_installed`.
2. **Being key is not being focused.** `window.focus(...)` is GPUI's own
   internal focus: it draws the caret and routes actions, and **cannot pull
   real OS keystrokes into a window the OS does not consider key**. Both are
   required, and they fail independently. (This one turned out *not* to be
   the remaining bug — the readback said `focused before=true` — but the
   distinction is what the first two rounds of debugging got wrong.)
3. **A key recorder must listen in the capture phase.** GPUI matches key
   bindings and dispatches actions **between** the capture and bubble
   phases, so a bubble-phase `on_key_down` only ever sees keystrokes nothing
   else wanted. Measured live: an unmodified **Escape reached a bubble
   handler while ⌃⇧K never did** — and a hotkey is made entirely of modified
   keystrokes. `capture_key_down` is the fix, and this repo already knew it:
   the earlier in-panel version of this same control used capture phase
   deliberately and said why. The knowledge was lost moving the control into
   a window, because onboarding's structure was copied instead — and
   onboarding binds almost nothing, so the phase never mattered there.

**Not verified: anything rendered on screen.** No window-scoped screenshot
was taken, and the recorder has never been driven by a real keypress —
synthesizing one is forbidden here. The window's layout, the tab bar's
traffic-light clearance, the painted toggle switch and the `Glyph::Sliders`
mark are all unproven visually. **The traffic-light clearance is the most
likely thing to be wrong**, since it reuses onboarding's constant against a
different header layout.
