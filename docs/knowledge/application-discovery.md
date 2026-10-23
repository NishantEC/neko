## Application discovery

`neko_core::apps` (`scan_applications`, `watch_applications`). Replaced the
original hard-coded five-directory scan after the captain reported apps
missing that Spotlight itself finds. Full reasoning, with the machine
evidence behind each choice, is in the module's own doc comment — read that
before touching this file again. Summary:

- **Spotlight's metadata index (`mdfind`) is the primary source**, not a
  directory list — it's what Spotlight's own UI queries, has no depth limit
  (fixes the old `depth < 1` cap that missed nested installs like
  `~/Applications/CrossOver/Steam/*.app`), and isn't tied to any hard-coded
  root. No public LaunchServices call enumerates "every registered app"
  (checked against the SDK header directly), which is why this shells out to
  `mdfind` rather than linking `NSMetadataQuery`/`MDQuery`.
- **Unioned with a plain scan of three sealed-system-volume directories**
  (`/System/Applications` and its two siblings) — verified on the dev
  machine that `mdfind` returns zero results there (`mdutil -s /` reports
  indexing disabled for that whole volume) even though every app is really
  there. That scan runs once, not live — the volume can't change without an
  OS update, which restarts the daemon anyway.
- **A fourth sealed-volume location, `/System/Library/CoreServices` itself
  (not just its `Applications` subdirectory), gets a named allowlist of 5
  bundle names instead of a fourth recursive scan — `neko-p0-fixes`,
  correcting this section's own prior claim.** Finder/Installer/Siri/Game
  Center/Screen Time all live loose in that top-level directory, which is
  why the captain's own "Finder is missing" report traced here — but so do
  ~112 macOS background agents (`Dock.app`, `PowerChime.app`,
  `CoreLocationAgent.app`, `SystemUIServer.app`, …), and a first version of
  this fix scanned the whole directory on the assumption that
  `LSBackgroundOnly` (below) would drop them, landing 259 apps instead of
  ~152. It doesn't: verified against a wide sample that **none of the
  leaked agents set `LSBackgroundOnly`**, and no other static
  `Info.plist`/`lsregister`-flag/launchd-registration/nib-presence signal
  checked separates the 5 wanted bundles from the ~112 agents either (both
  classes mix every candidate property inconsistently) — the full
  investigation, each signal tried and why it failed, is in
  `apps.rs`'s `CORE_SERVICES_ALLOWED_APPS` doc comment. `Finder.app`'s own
  `Info.plist` also needed a second, narrower fix either way:
  `CFBundlePackageType` is the legacy `"FNDR"` value there, not `"APPL"` —
  `read_app_bundle` accepts both now, the one general rule change this task
  made. Regression test:
  `apps::tests::scanning_the_real_machine_excludes_a_verified_background_agent`
  (asserts `PowerChime` absent, `Finder` present).
- **Live updates via `mdfind -live` as a change signal, not a data source**
  — it only ever reports a match *count*, never the updated paths (confirmed
  against its man page and by experiment), so an update triggers a fresh
  one-shot query rather than being parsed directly. Requires
  `NSUnbufferedIO=YES` in the child's environment or its stdout sits fully
  buffered and invisible for a minute-plus even though Spotlight already has
  the change (confirmed live). Debounced 300ms so a burst of filesystem
  churn collapses into one rescan.
- **Filtering rule: `LSBackgroundOnly`, never `LSUIElement`.** `LSUIElement`
  only hides the Dock icon — Raycast, Rectangle, Tailscale, Docker, and
  Amphetamine all set it and are all meant to be launchable (verified: none
  of the five set `LSBackgroundOnly`). `LSBackgroundOnly` means no UI
  surface at all exists to bring forward, which is what actually
  distinguishes a helper (verified against `~/Applications/Claude Code URL
  Handler.app`, a real example from the captain's machine). Also filtered:
  bundles nested inside another `.app`/`.framework` (login items, XPC
  services, framework-embedded helpers), Xcode/CI build products
  (`DerivedData`, `ios/build`, ...), installer staging directories, and
  Script Editor's template stubs (a separate rule from the nesting one —
  they live loose in a shared `Templates` directory on this machine, not
  inside `Script Editor.app` itself).
- **Known limitation, stated rather than engineered around**: if Spotlight
  indexing is disabled machine-wide, `mdfind` still runs successfully but
  returns nothing, and a successful empty result is trusted rather than
  triggering the directory-scan fallback (that fallback only fires if
  `mdfind` can't be run at all) — per "the metadata index must be the
  primary source." The sealed-system-volume apps still show either way.
- **A bundle name resolves to `None`, not an empty string, if every
  candidate is blank.** `bundle_display_name` (`apps.rs`) chains
  `CFBundleDisplayName` → `CFBundleName` → the `.app` filename's own stem,
  but falls through past a candidate that's *present and blank* too, not
  just a missing key — `plist::Value::as_string()` returns `Some("")` for an
  empty `<string></string>`, which the original `.unwrap_or_else(...)` chain
  treated as "found," producing a real row with a cached icon and a
  completely empty title (hit at 147 real Spotlight-indexed apps). A bundle
  where even the filename stem is blank is dropped from the index entirely
  — never render an icon with no title, per the same rule `panel.rs`'s row
  layout assumes throughout.
