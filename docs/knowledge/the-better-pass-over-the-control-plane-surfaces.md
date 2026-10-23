## The better-* pass over the control-plane surfaces

Same six domain skills, run over what L2–L6 added (permission rows, Schedules,
Ask neko, Terminals). Five findings, all fixed.

**The one that needed a photograph: `font_family("SF Mono")` silently did
nothing.** `terminals::capture_text` substitutes a space for each
unrenderable private-use glyph rather than deleting it, specifically so
column alignment survives — and then the preview rendered in the panel's
proportional font, throwing that away at the last step. The fix looked
correct and was not: **SF Mono ships with Xcode and Terminal.app, not with
macOS**, and is genuinely absent here (`/System/Library/Fonts` has
`Menlo.ttc` and `Monaco.ttf`; no SF Mono anywhere). **gpui resolves an
unknown family by falling back to the system default with no error**, so a
wrong name is indistinguishable from the feature not working. `Menlo` is a
core system font and is what ships. Before/after:
`docs/evidence/terminals-preview-{proportional-before,monospace-after}.png` —
the second has `ls -la`'s size column right-aligned, the first does not.
**Any future `font_family` call needs a picture, not a build.**

The other four:

- **`Glyph::AgentLive` on a permission row said the opposite of the row.**
  A blocked agent is *stopped* — that is the entire reason the row exists —
  and the live variant carries a presence dot meaning a session is running.
  Now `Glyph::Agent`. Same fix on the Ask rows, where nothing is a live
  agent at all.
- **`"Paseo isn't running any more"` was a second wording** of what
  `McpError::NotRunning` already says. One condition, one string — the
  `"Can't reach"` / `"Couldn't reach"` split from the first interface pass,
  caught earlier this time.
- **The Ask mode's empty row repeated its own placeholder verbatim.** The
  field already said "Say what you want done"; the only row on screen now
  says what the field cannot — that neko will propose one call and run
  nothing until you confirm.
- **`render_mode_detail` labelled every mode's fields as the clipboard's.**
  A terminal's working directory under a heading reading "Application" is a
  wrong label on a right value, which is worse than no label. The clipboard's
  three fields are now scoped to clipboard rows; a terminal gets "Directory".
