# neko-p0-fixes — evidence notes

**Status: complete.** Both items flagged outstanding in the first pass
(the app-index regression, and fix #3's GUI screenshot) are done — see
`followup.md`'s own two-item list (in the firstmate home,
`data/neko-p0-fixes/followup.md`) for what was asked. This file keeps the
original per-fix write-ups plus what changed in the follow-up pass.

## 1. Finder / sealed_system_directories — index regression fixed

Original fix: added `/System/Library/CoreServices` (the loose top-level
directory, not just its `Applications` subdirectory) to
`sealed_system_directories()`, plus a second, audit-unanticipated fix —
`read_app_bundle`'s `CFBundlePackageType` check rejected Finder outright
even after the directory was added: `Finder.app`'s own `Info.plist` carries
the legacy `"FNDR"` four-char OSType value, not `"APPL"` (verified with
`PlistBuddy`; every other loose app in that directory uses `"APPL"`).
`read_app_bundle` now accepts both.

**That first pass took the index from 147 to 259 apps (+112) — only 5 of
those wanted.** The other ~107 were macOS background agents/daemons
(`Dock.app`, `ControlCenter.app`, `PowerChime.app`, `CoreLocationAgent.app`,
`SystemUIServer.app`, …) that leaked through because `LSBackgroundOnly`
doesn't cover them on this OS build — flagged in the first pass as
"verify and report, no reliable filter found," which the follow-up
correctly rejected as not actually fixed.

**Follow-up fix**: `/System/Library/CoreServices` no longer gets a
recursive scan at all. `sealed_system_directories()` is back to 3 entries;
a new `CORE_SERVICES_ALLOWED_APPS` allowlist (`apps.rs`) names exactly the
5 verified-wanted bundles (`Finder.app`, `Installer.app`, `Siri.app`,
`Game Center.app`, `Screen Time.app`) and `scan_core_services_allowlist`
reads only those paths directly, still through `read_app_bundle`'s
ordinary checks. This is the brief's own explicit fallback ("a small
explicit allowlist... is an acceptable fallback if you genuinely cannot
find [a general rule]") — chosen after checking, and rejecting, five
separate static signals as the general rule:

- `LSUIElement` — wrong: `Siri.app`/`Game Center.app` (wanted) set it,
  identically to `Dock.app`/`ControlCenter.app`/`WindowManager.app`
  (unwanted).
- `CFBundleIconFile`/`CFBundleIconName` presence — wrong: `Dock.app`,
  `ControlCenter.app`, `iCloud+.app`, and many other unwanted agents all
  carry a real icon asset too.
- `lsregister -dump`'s bundle flags — wrong: `Game Center.app`'s flag set
  is byte-identical to `Dock.app`'s.
- A `launchd` `LaunchAgents` registration — wrong in both directions:
  `Finder.app`/`Installer.app` (wanted) have one; `PowerChime.app`/
  `CoreLocationAgent.app` (unwanted) don't.
- A compiled `.nib`/`.storyboardc` (a real window to show) — wrong:
  `PowerChime.app` ships 3 nibs and is still a background chime player;
  `Screen Time.app` (wanted) ships none.

Full investigation transcript is in `CORE_SERVICES_ALLOWED_APPS`'s own doc
comment in `crates/neko-core/src/apps.rs` — read that before touching this
directory's handling again, especially after any macOS upgrade (a future
OS could rename/add/remove a loose bundle here; see `AGENTS.md`'s "Seams
for follow-up work").

**Verified count, real machine, release binary, `scan_applications()`**:

```
count=152
has Finder: true
has Installer: true
has Siri: true
has Game Center: true
has Screen Time: true
has PowerChime: false
has CoreLocationAgent: false
has Dock: false
```

147 (baseline) + 5 (wanted) = 152, exactly as the follow-up's acceptance
criterion asked. `sealed_system_directories()`/`Applications` subdirectories
were unaffected by this change and were re-checked to have no junk of
their own (46 + 19 + 12 = 77 clean entries, all pre-existing).

New/changed tests in `apps.rs`:
- `sealed_system_directories_are_the_three_recursively_scanned_paths` —
  asserts the bare `CoreServices` directory is *not* in the recursively
  scanned list (renamed/re-asserted from the four-path version).
- `scanning_the_real_machine_finds_finder` — now scans via
  `scan_core_services_allowlist`, still asserts Finder specifically.
- `scanning_the_real_machine_excludes_a_verified_background_agent` (new) —
  asserts `PowerChime` is absent, per the follow-up's own suggested
  acceptance test.
- `core_services_allowlist_yields_exactly_the_five_wanted_apps` (new) —
  asserts the allowlist scan returns exactly
  `["Finder", "Game Center", "Installer", "Screen Time", "Siri"]`.

Live daemon query (isolated `HOME`, real Spotlight + sealed-dir scan,
release binary) from the first pass still holds — Finder is the sole
top result for `"finder"`. Window-scoped GUI screenshot:
`p0-1-finder-search-result.png`.

## 2. Click-outside dismissal

Unchanged from the first pass — see `main.rs`'s `cx.observe_window_activation`
registration and the `visible`-flag fix it also resolved (documented fully
in `AGENTS.md`, "Click-outside dismissal and inline activation errors").
Live repro captured in the first pass (`p0-2-click-outside-before.png` plus
the `neko: summon window lost activation, hiding` stderr transcript) still
stands; not re-captured in the follow-up pass since the follow-up's own
task list only asked for items 1 and 2 above (the index regression and
fix #3's screenshot), not a re-verification of this fix.

## 3. Activation failure surfaced inline — GUI evidence captured

Code unchanged from the first pass (`panel::Root::confirm()`,
`render_footer()` — see `AGENTS.md` for the full writeup). What was
missing was the GUI-level screenshot; captured this pass.

**New evidence tooling, added specifically to capture this without
synthetic OS keystrokes** (ruled out generally in this codebase — see
`evidence.rs`'s own doc comment): `panel::Root::confirm_for_evidence`
drives the real `confirm()` path directly (`self.confirm(&Confirm, ...)`),
wired to a new `NEKO_SHOW_CONFIRM=1` env hook in `evidence.rs`, read
alongside the existing `NEKO_SHOW_QUERY`. Full doc comment in `AGENTS.md`,
"Evidence-capture hook: `NEKO_SHOW_CONFIRM`."

**A real, previously-unnoticed risk surfaced while capturing this**: the
first capture attempt showed three unexplained `neko: summon latency …` /
`summon window lost activation, hiding` cycles, and the resulting
screenshot showed the *empty-query default view* (query wiped) — plus, a
second time, a real live clipboard entry ("Summer 💋", not mine) visible in
that default view, the same privacy issue flagged once already in the
first pass. Root cause: `main.rs` registers a live OS hotkey unconditionally
whenever accessibility is already trusted for the binary, regardless of
`NEKO_SHOW_ON_LAUNCH`; since the captain's real daemon apparently isn't
currently holding the default `⌥Space` combo, my isolated-`HOME` evidence
client's own registration attempt *succeeded* and received real physical
keypresses meant for someone else's session, resetting the query mid-capture
via `reset_for_summon`. Both bad screenshots were deleted immediately, never
committed. Fixed for this and future evidence runs: `Request::CommitHotkey`
against the isolated daemon, set to an obscure combo (all four modifiers +
`F13`) *before* starting the client — a pure daemon-side persisted setting,
no live registration attempt of its own, so it's safe to set before the
client process exists. Documented as a standing evidence-capture practice in
`AGENTS.md`, same section.

With that fixed, a clean, deterministic repro: a real file
(`nekoActivationFailureDemoP0d.txt` under an isolated `~/Documents`)
indexed via the file-search provider, deleted from disk ~1.8s after the
query was typed (well after the daemon's own `mdfind` search had already
found it, well before `confirm_for_evidence` fired), then confirmed.
Screenshot: `docs/evidence/p0-3-activation-error.png` — shows the file
still listed as the selected result, and the footer reading `"Couldn't
open — /usr/bin/open exited with exit status: 1"` in the danger color,
panel still open. No privacy-sensitive content visible (a targeted query,
not the empty-query default view).

All isolated-`HOME` daemon/client processes used for this capture were
killed by exact PID immediately after; the captain's real daemon/client
(verified live: bound to `~/Library/Application Support/neko/neko.sock`,
not the isolated one) were confirmed untouched throughout, both before and
after. The isolated `HOME` directory (`/Users/Shared/neko-p0-evidence-home`)
and all scratch files under `/tmp/neko-p0-evidence` were deleted after use.

## 4. Text field shortcuts

Unchanged from the first pass — fully implemented, unit-tested, no GUI
screenshot needed (acceptance criterion was a unit test over the editing
model). See `AGENTS.md`, "Text field editing shortcuts."

## Final verification

`cargo build`/`test`/`clippy --all-targets` at the workspace root, release
binaries: all clean. 124 tests passing (up from 122 in the first pass — the
two new `apps.rs` tests above).
