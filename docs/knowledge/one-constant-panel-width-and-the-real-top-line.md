## One constant panel width, and the real top line

`fm/neko-footer-hairline`. Full before/after screenshots and the pixel-level
diagnosis: `docs/evidence/footer-hairline-and-panel-width-report.md`.

**A wrong inference, corrected.** The captain reported "a top line, maybe we
can remove that" on the current build; this task's first pass inferred that
meant the footer's own horizontal hairline (`render_footer`'s
`.border_t_1().border_color(theme::BORDER_HAIRLINE)` — the only horizontal
rule anywhere in the panel) and removed it. Wrong: his own follow-up
screenshot showed the line he meant sits **outside the panel's top-left
edge, in the transparent margin**, roughly level with the panel's top edge,
ending where the rounded corner begins. **The footer hairline is restored,
unchanged from before this task** — the vertical divider inside the footer
(between `Open ↵`/`Paste ↵` and `Actions ⌘K`, named explicitly in the
frozen design, `data/neko-design/report.md` §2) was never touched either
way.

**The panel is now one constant width, 760px, root list and clipboard mode
alike** — a captain override of the frozen design's original two-width rule
(680px list-only / 760px only with a detail pane; `data/neko-design/
report.md` §2, updated in place to record the override). It has to be 760,
not 680: the real `NSWindow` was already permanently fixed at 760px for the
process's whole lifetime for the unrelated reason "Mode view resize seam"
above documents (gpui's own paint viewport goes stale after a show/hide
cycle, with no public API to resync it), so one constant width has to match
the window, not the other way around. `panel::Root::render` no longer
varies width by `active_mode`.

**What this let the implementation delete, since it was now genuinely
dead**: `render`'s centering stage element and its two margin `div`s
(`render_dismiss_margin`, including their click-to-dismiss handlers — no
margin once the panel always fills the window, so no dead zone to
compensate for); `theme::PANEL_ROOT_INSET_PX`; `Root::
update_background_bounds` and its three call sites
(`reset_for_summon`/`enter_mode`/`exit_mode`) — the native backdrop
`material::install` already puts in place at the window's own full
`contentView` bounds, once, at startup, which now matches the panel in
every mode with nothing left to reposition; `main.rs`'s startup call
narrowing that backdrop to 680px before the first summon; and
`material::set_background_frame` itself (the public wrapper and its macOS
impl), whose only two callers were the two removed just above. **What
stayed, and why**: `theme::PANEL_WIDTH_PX` (680) — `onboarding/view.rs`
still uses it for the onboarding window's own, unrelated size.

**The real top line, diagnosed rather than guessed at.** The leading
hypothesis handed off with the correction was Liquid Glass's own specular
edge rim (`NSGlassEffectView`). Tested directly with
`NEKO_FORCE_MATERIAL=popover` (skips `NSGlassEffectView`, forces the
`NSVisualEffectView` fallback): the same ~2px, fully-opaque, brighter-
than-fill top row exists under the panel with *either* material — so it is
not Liquid Glass–specific, and that line of investigation stops there per
the correction's own instruction. What actually produced a visible line
**outside** the panel, before this task's width fix, was a secondary,
softer artifact: that same top-edge brightening has a low-alpha falloff
past the panel's own rounded top corners, and with the old two-width
layout there was a real 40pt transparent margin between the 680px panel
and the 760px window for that soft falloff to become visible in —
confirmed by direct alpha-channel sampling of a pre-width-fix build
(`ccdbc64`, via a temporary `git worktree`, never disturbing this branch):
`rgba(248,248,248,38)` at the margin's top edge vs. `rgba(0,0,0,6)` a few
rows down, present nowhere else in the margin's height. **The width fix
above is also the fix for this**: with no margin left for the age to fall
into, a same-settings before/after top-left-corner capture confirms the
line is simply gone — no separate code change was needed or made.
`screencapture -l<windowID>`'s own documented limitation (real alpha, but
no live compositing with what's behind the window — "Window material"
above) is why this used the window's own alpha channel directly rather
than a synthetic backdrop image.

**Correction to this section's own claim, found by `fm/neko-top-line-titled`
— this is the second wrong attribution for the same symptom, after the
window shadow one two sections up.** "The width fix above is also the fix
for this … the line is simply gone" is wrong. The width unification removed
a *margin-side* artifact (the soft falloff of the panel's own top-edge
brightening, past its rounded corner, into the 40pt margin), which was real
and is genuinely gone — but the line the captain kept reporting is a
separate, harder one **on the panel's own top edge**, and it survived this
task untouched. Measured on `cda3765` (the build after this fix landed):
the top 2 device px read `(66,66,66)` and `(43,43,43)` against a
`(19,19,19)` fill, with every other edge flat `(19,19,19)`. The line-of-
investigation this section did correctly close stays closed — it is not
Liquid Glass's specular rim, confirmed here twice — but the reason it isn't
material-specific is that **it isn't painted by neko at all**. See "The top
line — AppKit's titled-window rim" immediately below.
