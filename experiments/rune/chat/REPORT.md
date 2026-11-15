# Rune chat spike: web view child, Neko-scale chat, daemon, churn

Rune at `4ddf696`. Raw numbers are in `results/*.json` (run `summarize.py` on them), screenshots in `shots/`, and each step can be rerun with the `run-*.sh` scripts.

## 1. WKWebView as a child of a Rune window: works, with costs

- It composites above Rune's surface. Its rect matches the layout rect, including after a resize.
- Clicks, focus and IME go to the right view. Tested with synthetic `NSTextInputClient` calls, since no CJK input source was installed.
- **A Rune dropdown draws under the web view and is cut off** (`shots/q1-child-dropdown-hidden-behind-web.png`). Hiding the web view while an overlay is open works.
- The web view is not clipped by Rune scroll viewports. A clip container fixes that.
- A borderless child panel overflows the window and can't be clipped.
- A transparent window with the web view under Rune showed nothing.

## 2. Neko-scale chat: feasible, load-sensitive

The test used a real window, a perf profile and a 4,000-word reply streamed at 60 tokens/s.

| Phase | Frame interval p50 / p95 | Update p50 / p95 |
|---|---|---|
| Stream, first quarter | 17.6 / 25.6 ms | 3.5 / 4.5 ms |
| Stream, last quarter | 23.2 / 29 ms | 4.9 / 8.1 ms |
| Stream + scrolling the 500-task list | 17.7 / 26 ms | 3.5 / 6.5 ms |
| Scrolling the list alone | 17.8 / 25.5 ms | 2.4 / 7.4 ms |

- Another run used about half the CPU, so read these as a range.
- Isolated ~190 ms stalls appeared 3–4 times per run. Their cause is unknown.
- Memory:
  - Idle footprint is 58.6 MB (97 MB RSS).
  - One long reply peaks near 245 MB and settles at 123 MB.
  - Over 30 cycles memory plateaus near 125 MB, then creeps about 0.18 MB per cycle.
  - Each retained 4,000-word message costs about 7.8 MB.
- Builds: a cold perf build took 70 s and a warm release rebuild 10 s. The binary is 31 MB (26 MB stripped).
- The kit's `TextArea` has no paste hook, so the composer uses core `text_input`. Image paste and IME work in headless scripts.

## 3. Real daemon: works

The HTTP route answers 401, so the spike connects over the Unix socket with `neko_protocol::Frame`. 25 seeded tasks loaded in 6 ms. Decode time for a real-size snapshot was not measured.

## 4. Churn: low day to day, high over a week

- The latest `dev`, one commit ahead, built unchanged. The pin is restored.
- `dev` took 753 commits in 7 days.
- At a week-old revision, none of the five required patch targets existed, and neither did the `tooling` feature. The app's own source compiled unchanged.

## Top problems

1. Native web views always sit on top and are unclipped, so Rune overlays can't draw over them.
2. Update time grows with message length, and a retained long message costs about 8 MB. The stalls are unexplained.
3. The integration contract (the patch table and feature names) changed within a week, and Rune takes no pull requests.
