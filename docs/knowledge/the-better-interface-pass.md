## The better-* interface pass

A `better-interface`-orchestrated review of every surface (`better-accessibility`,
`better-layout`, `better-writing`, `better-typography`, `better-colors`,
`better-ui`), then all eight findings fixed. The two that matter:

**The WCAG gate was measuring a surface that never renders.**
`themes_all_pass_wcag_aa` checks text against the opaque `surface_panel`,
but the panel paints `surface_panel_translucent` over a live
`NSGlassEffectView` — so what is actually read is the fill composited
against the desktop. Neutral measured **15.86:1** in that test and
**2.18:1** on a white wallpaper. `every_theme_stays_legible_over_the_worst_wallpaper_it_can_sit_on`
composites each theme's fill over whichever extreme is further from it
(white for a dark panel, black for a light one) and gates `text_primary` at
AA; `neutral`'s `panel_alpha` went 0.40 → **0.64**, and it was the only
theme below its own floor — the other sixteen already sit at 0.84–0.90.
**Gated on `text_primary` only**: every theme's `text_secondary` would need
0.62–0.91 to clear AA the same way, which would crush the translucency the
app is built on across all seventeen palettes; that limit is recorded in
the test rather than enforced.

**The Preferences window had no keyboard path at all.** `on_key_down`
returned early unless the hotkey recorder was listening, so four tabs, three
switches and every folder's Remove answered only the mouse — in an app whose
panel is otherwise keyboard-first. `preferences::view::Control` is now an
ordered ring per tab with Tab/Shift-Tab, Left/Right across the tab bar (the
ARIA tabs pattern, wrapping), Enter/Space to activate, and **Escape to
close** — the window has no menu bar, so that was the only key that could.
A hand-rolled `focused: usize` rather than gpui's `tab_stop`/`focus_next`,
for the same reason `panel::Root::selected` is: the control list genuinely
changes under it (adding a folder adds a stop) and it is testable without a
live window, which matters because a real keypress into this window cannot
be synthesised under this repo's own rules. The focus ring is a 2px
`text_primary` outline, never a fill — several of these controls already use
fill to mean *selected* or *on*. **Onboarding was already keyboard-complete**
(`enter`→`Primary`, `escape`→`Secondary`, `main.rs:132`); the first draft of
this review claimed otherwise and was wrong.

The rest, briefly:

- **The root list now shows what it dropped** — as a count, not a fade.
  `Root::hidden_rows` renders `"+3 more — keep typing to narrow"`. A
  gradient was the first attempt and it was **wrong**, which only a real
  capture showed (`docs/evidence/interface-pass-truncation-fade-invisible.png`):
  a scroll fade works because content sits *behind* it, and here there is
  none — the list stops, and the fade painted panel-colour over
  panel-colour in whatever slack was left below the last row. Invisible, in
  the exact case it existed for. A count also names the way out, which a
  fade cannot: this list does not scroll, so "there is more" with no means
  of reaching it is worse than silence. **The cue pays for itself** —
  `run_search` refits against `budget - TRUNCATION_CUE_HEIGHT_PX` whenever
  anything was dropped, or the cue would push out the last row that
  `fit_within_budget` exists to protect.
- **One string per idea.** `panel::NO_MATCHES` replaces `"No matching
  results"` / `"No matching entries."` (two nouns, one of them punctuated)
  and names the way out; `panel::DAEMON_UNREACHABLE` replaces `"Can't
  reach"` / `"Couldn't reach"`.
- **Sentence case on settings rows and onboarding buttons**; command names
  and section headers stay Title Case, which is a different element type
  applied consistently. Onboarding's three ways to decline became one —
  `"Skip setup"` stays, because skipping the whole flow is not declining one
  permission.
- **An agent tile's truncation is reachable.** Both its lines ellipsize at
  150px, and since `agents::subtitle` began handing the line to the prompt,
  the hidden part is what tells two sessions apart. `panel::TextTooltip` is
  the smallest view that renders a string in the app's tokens.

**Verified on screen, under the standing evidence discipline** (isolated
`HOME`, `verify_harness` rather than the real daemon, no `neko-daemon`
sibling on `PATH` so the spawn fails cleanly, `key window false` at every
capture, `screencapture -l<windowid>` only): the focus ring on the hotkey
keycaps and on the launch-at-login switch
(`docs/evidence/interface-pass-focus-ring-{hotkey,toggle}.png` — the second
is why the ring is an outline and not a fill; that switch is already
filled), sentence-case labels live, and the truncation cue
(`interface-pass-truncation-cue.png`, logged `results truncated true`
beside the window number so the capture states its own state).
`NEKO_SHOW_PREFERENCES=<index>` is the permanent hook that made the ring
photographable: it parks the ring on one control and, unlike the ordinary
Preferences path, never calls `cx.activate`/`activate_window` — moving the
ring for real needs a Tab press, which this repo does not synthesise.
**One environment note**: `cp` over a *running* binary leaves it unusable
(the client exits instantly with an empty log); replace it with `rm` then
`cp`.

**Rejected, with the reason, so nobody re-runs them**: letter-spacing on the
uppercase badges (**gpui exposes no letter-spacing API** — checked in
`styled.rs`, `style.rs`, `text_system.rs`); colour-alone status on the meter
and the LIVE badge (both carry redundant text); raising
`RESULT_ROW_HEIGHT_PX` (40px already clears WCAG 2.5.8's 24px baseline and
hits the 40px desktop target); a reduced-motion guard (`motion.rs` already
reads `NSWorkspace.accessibilityDisplayShouldReduceMotion` live).
