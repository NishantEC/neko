# System Settings pane search — discovery, verification, and measurements

Item 11 of the neko audit's prioritised plan. This provider (`neko_core::settings`)
is the second proof of the `Provider` seam (`provider.rs`), after file search.
Full design rationale lives in `settings.rs`'s own module doc comment; this
file is the evidence: what was actually found on disk, what was actually
opened, and the before/after latency and memory numbers the brief asked for.

## Discovery: how panes are enumerated on this OS (macOS 26.5.1 "Tahoe")

The old `.prefPane` bundle layout (`/System/Library/PreferencePanes`,
`/Library/PreferencePanes`, `~/Library/PreferencePanes`) still exists on
disk but is a dead end: `Displays.prefPane`, for instance, has **no
`Contents/Info.plist` at all** — just a leftover `Resources/MirrorDisplays.app`
helper bundle. Confirmed directly:

```
$ find /System/Library/PreferencePanes/Displays.prefPane -maxdepth 5
/System/Library/PreferencePanes/Displays.prefPane
/System/Library/PreferencePanes/Displays.prefPane/Contents
/System/Library/PreferencePanes/Displays.prefPane/Contents/Resources
/System/Library/PreferencePanes/Displays.prefPane/Contents/Resources/MirrorDisplays.app
...
$ plutil -p /System/Library/PreferencePanes/Displays.prefPane/Contents/Info.plist
Info.plist couldn't be opened because there is no such file.
```

The real mechanism: System Settings was rewritten on ExtensionKit. Every
real pane is a `.appex` bundle under
`/System/Library/ExtensionKit/Extensions/` (241 total on this machine, most
unrelated to Settings at all — Siri metrics collectors, Mail search
indexing, widget extensions, ...). A pane declares itself with:

```
EXAppExtensionAttributes.EXExtensionPointIdentifier == "com.apple.Settings.extension.ui"
EXAppExtensionAttributes.SettingsExtensionAttributes.allowsXAppleSystemPreferencesURLScheme == true
```

51 of the 241 bundles match the extension point; 50 also set the URL-scheme
flag (the one exception, `HomePrivacySettingsExtension.appex`, is excluded).

**The URL target is the extension's own `CFBundleIdentifier`, not
`legacyBundleIdentifier`.** The legacy field looked like the obvious choice
(it's what the URL scheme has accepted since much older macOS versions) and
it does work, but it's the wrong primary key: only 33 of the 50 panes have
one at all, and where it exists it isn't always unique —
`SiriPreferenceExtension.appex` and `SpotlightPreferenceExtension.appex`
both declare the identical two-element legacy list
(`["com.apple.preference.speech", "com.apple.preference.spotlight"]`), so
picking "the" legacy id for either is ambiguous by construction. The modern
`CFBundleIdentifier` has neither problem: every pane has one, it's unique,
and it was verified live to resolve correctly on its own (see below).
`settings.rs` never reads `legacyBundleIdentifier`.

**Display names mostly come from `Contents/Resources/InfoPlist.loctable`,
not the raw `Info.plist`.** A `.appex`'s unlocalized `CFBundleDisplayName`
is frequently just its Xcode target name (`MouseExtension`,
`TrackpadExtension`, `AccessibilitySettingsExtension`); the real name
("Mouse", "Trackpad", "Accessibility") lives in the loctable's `"en"` entry.
Two of the 50 panes (`com.apple.Battery-Settings.extension`,
`com.apple.HeadphoneSettings`) have no localized name anywhere in the
bundle — checked directly, not assumed — and get a two-entry override table
("Battery", "Headphones") rather than shipping their raw target names.

## Verification: real panes opened through the real `Activate` request

Proof method: after calling `open "x-apple.systempreferences:<id>"` (or,
for the daemon-routed cases below, the real `Request::Activate { kind:
"settings", id }` path), watch for that pane's own `.appex` process to
appear in `ps` — each pane is its own XPC-hosted process, so this
identifies exactly which pane opened, not just "System Settings came to
the front."

Four panes confirmed via bundle-id URL resolution directly:

```
$ open "x-apple.systempreferences:com.apple.Displays-Settings.extension"
→ /System/Library/ExtensionKit/Extensions/DisplaysExt.appex/Contents/MacOS/DisplaysExt

$ open "x-apple.systempreferences:com.apple.settings.Storage"
→ /System/Library/ExtensionKit/Extensions/Storage.appex/Contents/MacOS/Storage

$ open "x-apple.systempreferences:com.apple.preferences.Bluetooth"
→ /System/Library/ExtensionKit/Extensions/Bluetooth.appex/Contents/MacOS/Bluetooth

$ open "x-apple.systempreferences:com.apple.Siri-Settings.extension"
→ /System/Library/ExtensionKit/Extensions/SiriPreferenceExtension.appex/Contents/MacOS/SiriPreferenceExtension
```

Three more confirmed through the **real daemon**, running the release
binary against an isolated `HOME`, sending literal `Request::Activate {
kind: "settings", id }` frames over the Unix socket (the identical code
path the panel's Enter key drives):

```
$ python3 neko_activate.py .../neko.sock com.apple.Displays-Settings.extension
{"Response": {"id": 1, "response": "Activated"}}
→ ps shows DisplaysExt.appex running

$ python3 neko_activate.py .../neko.sock com.apple.settings.Storage
{"Response": {"id": 1, "response": "Activated"}}
→ ps shows Storage.appex running

$ python3 neko_activate.py .../neko.sock com.apple.BluetoothSettings
{"Response": {"id": 1, "response": "Activated"}}
→ ps shows Bluetooth.appex running
```

Each `open`/`Activate` was followed by verifying the right `.appex` process
appeared, then quitting System Settings (`tell application "System
Settings" to quit`) — no setting was ever changed, only navigated to and
closed.

## Search behavior

```
displays   → settings: Displays
bluetooth  → app: Bluetooth File Exchange, settings: Bluetooth   (both — no crowd-out)
sound      → settings: Sound
battery    → settings: Battery          (override table)
headphones → settings: Headphones       (override table)
storage    → settings: Storage
keyboard   → settings: Keyboard
network    → settings: Network
chrome     → app: Google Chrome          (settings unaffected — no title contains "chrome")
terminal   → app: Terminal               (settings unaffected)
code       → app: Code, app: T3 Code (Alpha), app: Cloudflare WARP, app: Cloudless
             Voice, app: Xcode, settings: Background Security Improvements
             (matches the pre-existing app-ranking behavior from fe4e4e9 exactly —
             "Code" still wins the top slot; "settings" only appears in the
             unboosted tail, same as the file-search "Xcode" case already
             documented in ranking-before-after.md)
general    → (no results — see "Known limitations" below)
```

Screenshots (window-scoped, `screencapture -l<windowID>`, isolated `HOME`,
release binary, real daemon):

- `settings-provider-displays.png` — query "displays", one "System
  Settings" section, the Displays pane row with the System Settings app
  icon (not an empty socket).
- `settings-provider-bluetooth-mixed.png` — query "bluetooth", both
  "Applications" (Bluetooth File Exchange, its own real extracted icon)
  and "System Settings" (Bluetooth pane) sections rendering in the same
  list — direct proof neither provider crowds the other out for a query
  that genuinely matches both.

## Known limitations, reported rather than silently absorbed

- **Multi-word queries don't match.** `fuzzy_score` requires every query
  character to appear as an in-order subsequence of the title; "keyboard
  shortcuts" (with the space) never matches "Keyboard" because after
  consuming "keyboard" there's nothing left in the title to match the
  space against. This is not specific to this provider — the identical
  limitation already applies to every other provider's title matching
  (e.g. it would equally fail to match an app literally named "Keyboard").
  A single word or prefix ("keyboard", "displays") matches correctly; a
  multi-word phrase does not. Out of scope to fix here — it would mean
  changing the shared `fuzzy_score`, which the provider contract
  (`provider.rs`'s own doc comment) deliberately keeps out of any single
  provider's hands.
- **"General" no longer exists as a pane name.** Apple renamed the old
  "General" pane to "Appearance" pane in a recent macOS revision (confirmed
  via this bundle's own `legacyBundleIdentifier`: `Appearance.appex` is
  `com.apple.preference.general`'s modern successor) — querying "general"
  correctly returns nothing, since no pane is actually named that on this
  OS. Not a bug in enumeration; a real naming change upstream.
- **No per-pane icon.** Each pane's `Info.plist` names an
  `ISGraphicIconConfiguration.ISTypeIdentifier` (the colored glyph System
  Settings' own sidebar renders per row), but resolving that identifier to
  pixels goes through Apple's private iconography stack, not the public
  `NSWorkspace.iconForFile` this daemon's icon cache already uses for
  everything else. Per the launch brief's own escape hatch, every pane row
  shares one cached extraction of System Settings.app's own icon instead of
  an empty socket — see `settings.rs`'s doc comment, "Icon" section.

## What adding this provider required touching

The seam held. Summary, full detail in `settings.rs`'s own module doc
comment and this repo's `AGENTS.md`:

- **New**: `crates/neko-core/src/settings.rs` (the provider itself).
- **`crates/neko-core/src/launch.rs`**: one new function, `open_url`, the
  identical shape as the existing `launch_app` but typed on `&str` since a
  `systempreferences:` URL is never a valid `Path`. No existing function
  changed.
- **`crates/neko-core/src/lib.rs`**: one `pub mod settings;` line.
- **`crates/neko-daemon/src/server.rs`**: one `Box::new(SettingsProvider::new())`
  line in the provider list, plus renaming the existing test-only
  `with_file_provider` constructor to `with_test_providers` so it can also
  take a hermetic, empty-panes `SettingsProvider` — the same reason
  `FileProvider::empty()` already existed for the file provider's own
  tests. No production code path in `server.rs` changed beyond that one
  registration line.
- **`crates/neko-daemon/src/main.rs`**: one line in the existing
  background icon-extraction thread, extracting the one System Settings
  icon before the per-app loop.
- **Untouched**: `neko-protocol` (no new `Request`/`Response` variant —
  `Request::Activate { kind, id }` already routes generically),
  `crates/neko/src/panel.rs` (no `match` on provider identity to edit —
  section header, icon, and action label all came from data already on
  `SearchItem`), `crates/neko-core/src/search.rs` (`allocate`'s reservation
  and greedy-interleave passes are already provider-count-agnostic; no
  settings-specific ranking bonus was added, and none of `fe4e4e9`'s
  existing `app_category_score` tests needed to change).

This matches exactly what `AGENTS.md`'s "Provider abstraction" section
predicted a fourth provider would cost: one `impl Provider`, one
registration line, nothing in the wire protocol or the panel.

## Measurements

**Daemon idle memory** — release binaries, isolated `HOME`, measured after
RSS stabilized (no client connected, no search yet, ~15s after the startup
icon-extraction/app-scan passes finished):

```
baseline (fe4e4e9, before this provider):  13,424 KB  (≈ 13.1 MB)
with settings provider:                    13,728 KB  (≈ 13.4 MB)
```

+304 KB (~2.3%) for 50 cached `SettingsPane { bundle_id, title }` structs
and the provider itself — not a meaningful regression against the ~13 MB
baseline the brief cites. (An earlier, discarded measurement of ~36 MB
was a methodology mistake, not a real number: it measured the daemon
*after* several `Search`/`Activate` round trips instead of at idle — noted
here so the mistake doesn't get quietly repeated.)

**Warm summon latency** — `NEKO_BENCH=20`, release binaries, isolated
`HOME`, real daemon resident, same machine and session for both runs so
ambient load is comparable:

```
baseline (fe4e4e9): 0.71ms – 9.27ms across 19 warm samples (sample 0 is cold, excluded)
with settings provider: 1.41ms – 8.85ms across 19 warm samples
```

Same range, same order of magnitude as the brief's own stated 4–6ms
budget — expected, since summon is purely client-side window activation
(`window.activate_window()` → first frame) and never touches the daemon's
provider list at all (`AGENTS.md`'s "Summon latency" section: "a slow or
even completely unreachable daemon cannot make summon itself slower").
Enumeration happens once, at daemon construction (`SettingsProvider::new`),
never on any request a client makes.
