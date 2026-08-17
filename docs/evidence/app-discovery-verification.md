# Application discovery — verified on the release binary

Methodology: `cargo build --release --workspace`, daemon and app run directly
from `target/release/`, not `cargo run`. Verified twice — once against the
captain's real `~/Library/Application Support/neko/` (read-only queries plus
one self-cleaned-up copy/remove of a test bundle in `~/Applications`, no
settings written), and once against an isolated `HOME` override so later,
more invasive checks (seeding `onboarding_completed`, killing/restarting the
daemon repeatedly) couldn't touch the captain's real daemon or the material-
task worker's concurrent one — both were confirmed live on this machine via
`pgrep -fl neko-daemon` during this session. Screenshots via
`screencapture -l<CGWindowID> -o`, window IDs resolved with a throwaway Swift
snippet over `CGWindowListCopyWindowInfo` (JXA's bridging of that call didn't
work reliably on this OS build); the panel summoned via a real click on its
Dock tile (`System Events`, targeting the Dock's own accessibility element)
and typed into via `keystroke` sent to whatever already has key focus after
that click — never a synthetic global hotkey, never full-screen capture.

## Coverage: 124 → 147 apps indexed

Before (old hard-coded 5-directory, depth-1 scan, replicated exactly via
`find` for comparison): 124. After (this change, real daemon startup log):
147. Newly found and confirmed present via a live `Search` request against
the running daemon:

- `~/Applications/CrossOver/Steam/Steam.app` and `.../Steam Support
  Center.app` — the brief's own named example, missed before by the old
  scan's one-subfolder depth cap.
- Every built-in system app under `/System/Applications` and its two
  siblings continues to resolve (these were already found by the old scan's
  hard-coded roots; confirmed they still all resolve under the new sealed-
  directory union — see `apps.rs`'s module doc comment for why that union
  exists at all).

## Filtering: confirmed against real bundles on this machine

- `~/Applications/Claude Code URL Handler.app` (the brief's own named
  example): absent from search results. Real cause, verified via `plutil`:
  `LSBackgroundOnly = true`.
- Raycast, Rectangle, Tailscale, Docker, Amphetamine: all present. All five
  set `LSUIElement = true` and none set `LSBackgroundOnly` (verified via
  `plutil` against each real installed bundle) — confirms the filter rule
  keys on the right flag.
- Script Editor's template stubs (`CocoaApplet`, the `Droplet` family under
  `/Library/Application Support/Script Editor/Templates/`): found in the raw
  `mdfind` output before this filter existed, absent after — a real, named
  brief category, not caught by the nesting rule (see `apps.rs`).

## Live update: proved without a restart

With the daemon already running, copied a real bundle
(`/System/Applications/Utilities/Console.app`) into `~/Applications` under a
new name/bundle id and confirmed it appeared in a live `Search` response
within 2 seconds — then removed it and confirmed it disappeared, again
within 2 seconds, same running daemon process throughout (no restart). See
`apps.rs`'s `watch_applications` doc comment for the mechanism
(`mdfind -live` as a debounced change signal, not a data source).

## A false alarm, corrected: icon rendering was never actually broken

The first round of search-results evidence
(`app-discovery-search-results-raycast.png`, since replaced) showed blank
grey squares for Raycast, Rectangle, RemotePlay, Folder Actions Setup,
Terax, and Telegram. This looked like a regression — and was reported as one
— but wasn't: it was an artifact of the verification method, not the code.

Icon extraction (`neko_core::icons::ensure_cached_icon`) runs once per app,
sequentially, in a background thread, and is a real per-app AppKit cost
(confirmed timing this pass: **147 apps took ~57 seconds to fully cache from
a cold, empty cache**, ~85 of them already done by 15s but the rest trickling
in well after). On the captain's real machine this is invisible — the cache
at `~/Library/Caches/neko/icons/` has been accumulating since the very first
"Icons" task and persists across every daemon restart, so in steady state
every app he's ever searched for already has a cached icon before he opens
the panel. The flawed evidence was captured against a deliberately isolated,
freshly-created `HOME` (see "Methodology" above) with an empty icon cache,
and the screenshot was taken only a few seconds after that daemon's first
start — well before extraction reached the apps that happen to sort later in
`scan_applications()`'s output (the Spotlight-sourced ones, appended after
the sealed-system-directory ones).

Confirmed, not assumed: polled the isolated cache directory every 3 seconds
from a cold start and watched `com.raycast.macos.png`, `com.knollsoft.
Rectangle.png`, and the others each appear over the following ~30-50
seconds, each a valid non-empty PNG (`file` reports real `1024x1024 PNG
image data` for both `Raycast` and `Rectangle`'s cached icons, checked
directly). Re-captured the search-results screenshot against the same
isolated setup once its cache had finished warming — every one of the
previously-blank rows now renders its real icon (`Raycast`, `Rectangle`,
`RemotePlay`, `Folder Actions Setup`, and `Terax` all visible in the
current `app-discovery-search-results-raycast.png`). No code changed as a
result of this investigation — `apps.rs` and `icons.rs` were already
correct; the bug was in how the first screenshot was gathered, not in what
it was screenshotting.
