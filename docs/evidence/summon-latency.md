# Summon latency — measured post-restructure

Same methodology as slice 1 (`AGENTS.md`'s original measurement, preserved): the
`window.on_next_frame` hook after `activate_window()` in `crates/neko/src/main.rs`,
hotkey-press to first-frame-after-activation, via the release binary
(`cargo build --release`, run directly as `./target/release/neko` — not
`cargo run`), with the daemon (`./target/release/neko-daemon`) already resident.
`RUST_LOG` unset.

Real hardware wasn't available for scripted, repeatable timing in this pass either
(same caveat as slice 1); measurements below are from the same
`osascript … key code 49 using {option down}` synthetic-hotkey method, with the
same reliability caveat — roughly half the synthetic presses were dropped
somewhere between AppleScript and the OS, never delivered late or corrupted.

## Cold (first summon after process launch)

Two independent cold samples, from two separate fresh `neko` process launches:

```
160.406250ms
 64.778709ms
```

Both are well above slice 1's own cold baseline (~89ms) and each other — the
panel now renders substantially more content on that very first paint (input
row, section header, up to 8 result rows with icons, footer, all against a
real `WindowBackgroundAppearance::Blurred` compositing pass) than slice 1's
single text field, and this is the one frame that pays for GPU pipeline /
text-shaping warmup on top of that extra content. **This is a real cost, not
noise** — see "what the panel's extra content costs" below.

## Warm (subsequent summons; window, renderer, and daemon connection already live)

Six samples from one continuous session:

```
6.360625ms
2.968916ms
4.383041ms
4.009167ms
2.872083ms
4.286042ms
```

Mean ≈ **4.1ms**, all six inside the slice-1 budget (~11–14ms) — in fact faster
than slice 1's own warm number, despite the panel rendering far more per
frame and the app now talking to a separate daemon process for search. Two
reasons this holds:

1. **The daemon round-trip is off the summon path.** Summon is purely
   hotkey-press → `window.activate_window()` → first painted frame — the same
   three steps as slice 1, unchanged. `Request::Search` only fires from
   `Root::run_search`, triggered by `cx.observe` on the text field's content
   changing, which happens *after* the first frame, not before it. A slow or
   even completely unreachable daemon cannot make summon itself slower — it
   would only leave the result list empty until a response arrives.
2. **The extra panel content is still cheap per-frame GPU work.** A handful
   of `div()`s with text runs and up to 8 small images is not enough geometry
   to meaningfully compete with an already-warm Metal pipeline; the fixed
   680×448 window (see `panel.rs`'s own doc comment on why it isn't resized
   per keystroke) means layout doesn't have to recompute window-level bounds
   either.

## What the panel's extra content costs — stated plainly, per the brief's ask

The warm number shows the *steady-state* cost is effectively zero versus
slice 1. The **cold** number is where the added content and the window
material genuinely show up — cold summon roughly doubled (89ms → ~65–160ms
across two samples, noisy but consistently higher). This is a one-time cost
paid once per process lifetime (the whole point of keeping the process
resident, per `AGENTS.md`'s "Current scope" note carried over from slice 1),
not a per-summon cost, so it doesn't threaten the acceptance criterion the
brief measures (the *warm* budget) — but it is a real, measured regression on
the cold path and is reported here rather than absorbed silently.

## Screenshots

- `summon-panel-empty-query.png` — default (no query typed) state: real
  ranked applications from this machine, real extracted icons, `Calculator`
  selected as the top row (recency-boosted from a real launch during this
  verification pass).
- `summon-panel-search-results.png` — after typing `cal`: fuzzy-matched,
  ranked results including apps that don't start with "cal" (`Claude`,
  `Tailscale`) via subsequence matching, `Calculator` still first on the
  combined fuzzy + recency score.
- `hotkey-no-leak-textedit.png` — the hotkey-scoping manual check (see
  `AGENTS.md`'s "Hotkey scoping" section): typed `BASELINE-` into a focused
  TextEdit document, pressed neko's live hotkey, typed `-AFTER`. Result:
  `BASELINE—AFTER` (TextEdit's own Smart Dashes autocorrect fusing the two
  adjacent hyphens) — proof nothing, not even a space, landed in TextEdit's
  document from the hotkey press between the two typed strings.

Both screenshots are direct window-region captures (`screencapture -R` against
the panel's own on-screen bounds), not full-desktop captures — this machine's
desktop has other real, unrelated windows open (other agents' work, personal
apps) that a full-desktop screenshot would have exposed.
