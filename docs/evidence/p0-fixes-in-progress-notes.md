# neko-p0-fixes — in-progress evidence notes

**Status: work stopped mid-verification per captain's stop-all-agents order.**
This file records what was measured before the stop so it isn't lost. See
the final status line in the task's status file for the authoritative
picture of what's done vs. outstanding.

## 1. Finder / sealed_system_directories

Fix: added `/System/Library/CoreServices` (the loose top-level directory,
not just its `Applications` subdirectory) to `sealed_system_directories()`
in `crates/neko-core/src/apps.rs`, **and** a second, audit-unanticipated fix
— `read_app_bundle`'s `CFBundlePackageType` check rejected Finder outright
even after the directory was added: `/System/Library/CoreServices/
Finder.app/Contents/Info.plist` carries the legacy `"FNDR"` four-char OSType
value, not `"APPL"`, on this real machine (verified with `PlistBuddy`;
Installer/Siri/Game Center/Screen Time in the same directory all use
`"APPL"` — only Finder uses the legacy value). `read_app_bundle` now accepts
both `"APPL"` and `"FNDR"`.

Live daemon query (isolated `HOME`, real Spotlight + sealed-dir scan,
release binary):
```
$ python3 neko_query.py <sock> '{"Request":{"id":1,"request":{"Search":{"query":"finder","limit":8}}}}'
{"Response":{"id":1,"response":{"SearchResults":{"items":[{"id":"com.apple.finder","kind":"app","title":"Finder", ...}]}}}}
```
Finder is the only/top result. Window-scoped GUI screenshot:
`p0-1-finder-search-result.png`.

### App-index count, before/after (real machine, release binary, `scan_applications()`)

- Before (pre-fix, `git stash`): **147** apps.
- After (post-fix): **259** apps. Net **+112**.

Of the 112 new entries, 5 are the real, desirable ones the audit named:
**Finder, Installer, Siri, Game Center, Screen Time**.

**The other ~107 are macOS background agents/daemons/helpers that leaked
through** — the audit's assumption that `LSBackgroundOnly`/nesting filters
"should already drop" them does **not** hold on this OS build (checked with
`PlistBuddy` against several: `Dock.app`, `ControlCenter.app`,
`SystemUIServer.app`, `loginwindow.app`, `NotificationCenter.app`,
`WindowManager.app`, `Spotlight.app`, `System Events.app`, `WiFiAgent.app`,
`OBEXAgent.app`, `iCloud.app`, `BluetoothUIServer.app`,
`CoreServicesUIAgent.app`, `rcd.app` — **none of these set
`LSBackgroundOnly`**, so the existing filter doesn't touch them). Diffed
`CFBundlePackageType`/`LSUIElement`/`LSApplicationCategoryType`/
`NSPrincipalClass` between known-junk and known-good entries in the same
directory (e.g. `Siri.app` vs `Dock.app`) and found **no reliable static
`Info.plist` signal** that distinguishes "real, user-launchable app" from
"background daemon" among the loose `/System/Library/CoreServices/*.app`
bundles on this OS build — both classes mix `LSUIElement=true`/absent
inconsistently.

**Not fixed** — deliberately, per the brief's own "verify + report, prefer
the smaller fix" framing, and because no static signal was found to build a
non-fragile filter on (a hardcoded ~107-name denylist would be fragile
across OS versions and out of scope for this task). Flagged as a follow-up:
demote/exclude by name or by a `launchd`-registration check, owned by a
future search-quality pass, not this one.

Full comm-diff of new entries (all 112 names) was captured during the
session but not yet copied into this file before the stop — regenerate with
`git stash` (old apps.rs) vs current, each piped through a small
`neko_core::apps::scan_applications()`-calling example binary, `comm -13`
on sorted name lists.

## 2. Click-outside dismissal

Fix: `main.rs` now registers `cx.observe_window_activation` on the one
resident summon window (inside the `window.update` block right after
`open_window`, using `Window::is_window_active()` to decide), calling
`cx.hide()` on deactivation, with an `eprintln!("neko: summon window lost
activation, hiding")` alongside it (same permanent-diagnostic convention as
`material.rs`'s own verification logging). Also replaced the old
`visible: Rc<Cell<bool>>` hotkey-toggle flag with a live
`window.is_window_active()` check at the point of the next hotkey press —
the flag would otherwise desync the moment the window was hidden by
anything other than the hotkey-press branch itself (a `confirm()`-triggered
hide, or this new click-outside hide), making the *next* hotkey press
silently no-op instead of re-summoning. This was reasoned through, not yet
independently live-verified as its own scenario (only the direct
click-outside path was captured live before the stop).

Live repro (isolated HOME, release binary, `NEKO_SHOW_ON_LAUNCH=1
NEKO_SHOW_QUERY=finder`): window opened and rendered normally
(`p0-2-click-outside-before.png`), then:
```
$ osascript -e 'tell application "Finder" to activate'
```
produced, in the client's own stderr:
```
neko: summon window lost activation, hiding
```
**Note on screenshot evidence for the "after" state**: `screencapture
-l<windowID>` still renders a hidden/deactivated window's last content (the
same limitation `AGENTS.md`'s Window material section already documents for
proving live compositing) — so a post-hide screenshot looks identical to
before and isn't meaningful proof by itself. The stderr log line above is
the real evidence; an "after" screenshot was captured but discarded as
misleading rather than committed.

**Not yet verified**: Escape still works (no regression expected — untouched
code path, but not re-confirmed live this session), and onboarding is
unaffected (the observer is registered only on the summon `Root` window,
never the separate onboarding window entity, by construction — reasoned,
not yet live-screenshotted).

## 3. Activation failure surfaced inline

Fix: `panel.rs::confirm()` now matches on the real `client.request(...)`
outcome — `Response::Error{message}` or a transport `Err` both set
`self.activation_error`, notify, and **do not** hide the panel; only a
non-error response hides it (the original `confirm()`-hides-launched-app
case, unaffected). `render_footer()` swaps its normal title/verb content for
`"Couldn't open — {message}"` in `theme::STATE_DANGER`, inside the same
fixed-height footer strip (no geometry change) — reusing the danger tokens
`render_accessibility_banner` already established rather than inventing a
new color or a toast surface. Cleared on the next `run_search` (query
change or fresh summon), so it never outlives the state that produced it.

**Live daemon-level repro completed** (isolated HOME, real file-search
provider, real deleted file):
```
$ python3 neko_query.py <sock> '{"Request":{"id":4,"request":{"Activate":{"kind":"file","id":"/Users/Shared/neko-p0-evidence-home/Documents/nekoActivationFailureDemo.txt"}}}}'
{"Response":{"id":4,"response":{"Error":{"message":"/usr/bin/open exited with exit status: 1"}}}}
```
Confirms the daemon really returns `Response::Error` for a deleted file, the
exact "moved/deleted since indexed" scenario in the brief.

**Not yet done before the stop**: the full GUI-level live capture (search
for the file in the real panel, delete it, send a real Enter keystroke to
the confirmed-frontmost test window, window-scoped screenshot of the
rendered `"Couldn't open — …"` footer). The daemon-level proof above and the
code path itself (read again before writing this note) are sound, but the
visual, panel-level screenshot this criterion asks for (`Demonstrate it`)
was not captured. **This is the top item for whoever picks this up next.**

## 4. Text field shortcuts

Fully implemented and unit-tested — 8 new `TextField` actions
(`DeleteLineStart`/`DeleteLineEnd`/`DeleteWordBackward`/`DeleteWordForward`/
`LineStart`/`LineEnd`/`WordBackward`/`WordForward`), bound in `main.rs`
(`cmd-backspace`, `cmd-delete`, `alt-backspace`, `alt-delete`, `cmd-left`,
`cmd-right`, `alt-left`, `alt-right`), word-boundary logic via
`unicode_segmentation::UnicodeWordIndices` (already a transitive dependency
via `global-hotkey` -> `keyboard-types`, MIT/Apache-2.0, zero new crate
versions in the tree — added as a direct `neko` dependency). One unit test
per shortcut plus a dedicated Unicode-awareness test (`café`/CJK), all
passing (`cargo test -p neko` — 48 passed). This is the one item fully done
including its own verification; no GUI screenshot was planned for this one
since the acceptance criterion is "a unit test over the editing model," not
a visual capture.

**Selection/paste seam, as required by the brief**: deliberately not
touched. `TextField` still has exactly one `cursor: usize`, no range concept
— every new shortcut here is either a cursor jump or a delete of
`[start, cursor)`/`[cursor, end)`, never a highlighted range. Real selection
(⇧-arrows, ⌘A) and paste (⌘V/⌘C/⌘X) need a selection-range field added to
`TextField` first, plus real pasteboard reads — a bigger, separate piece of
work, exactly as the brief anticipated. The seam is `TextField`'s own struct
(add a `selection: Option<Range<usize>>` alongside `cursor`) plus new
`EntityInputHandler`/action wiring; nothing in this task's changes makes
that harder to add later.

## Outstanding when the stop order landed

- Full `cargo build`/`test`/`clippy` at the workspace root: **clean**,
  re-verified immediately before the stop.
- GUI-level screenshot for the activation-failure inline message (#3) — not
  captured.
- Escape-still-works and onboarding-unaffected live re-verification (#2) —
  reasoned/code-level only, not freshly screenshotted this session.
- The full 112-name diff list for the app-count report (#1) — was on screen
  during the session but not copied into a durable file before the stop.
- `AGENTS.md` not yet updated with this task's durable knowledge (the
  `sealed_system_directories`/`FNDR` finding, the click-outside/visible-flag
  fix, the inline-error footer convention, the text-field shortcut seam).
- No commit yet as of writing this note — see the task status file for
  whether one landed after this note was written.
