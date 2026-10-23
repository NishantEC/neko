## A chat is scrolled, not stepped through — and what you can paste into one

**The arrow keys moved a selection in the transcript, because it was built out
of the row list.** That was wrong for the surface: a chat has no cursor. Enter
belongs to the composer, `⌘↵` targets the *session* rather than any one turn,
and a tool chip folds on click — so the highlight pointed at a thing nothing
acted on. Up/Down now move the **view**, by `TRANSCRIPT_SCROLL_STEP_PX`
(60pt, about three lines) through the same `ScrollGlide` a selection move
uses, clamped at both ends. The selection wash is gone with it; hover stays,
because the pointer genuinely can act.

**That forced the stick-to-bottom rule to be re-keyed**, since it read
`selected == len - 1` and `selected` no longer means anything here.
`transcript_at_bottom(scrolled, max)` is pure and separate from the handle for
the same reason `edge_fade_visibility` is: a live `ScrollHandle` reports
`max_offset` of zero until something has been laid out, so a test driving it
through the handle would be testing the harness. `max <= 0` counts as *at the
end* — a short conversation that could not scroll would otherwise never follow
a reply — and there is one step of slack, because a glide lands on a
fractional offset.

**Pasting an image into the composer works, and the mechanism is Paseo's own,
read off a real transcript rather than guessed.** `send_agent_prompt` takes
exactly `prompt: String` — **no attachment field anywhere in its schema** — so
an image cannot be handed to an agent through the tool call at all. What Paseo
does instead is visible in what it writes:
`![Image](file:///var/folders/.../paseo-attachments-XXXX/<hash>.png)` in the
prompt text, which the agent's harness resolves. `crate::attachments` does the
same, so an image pasted in neko reaches an agent exactly as one pasted in
Paseo does — that format is a contract with something else, which is why it is
pinned by a test.

Two details: **an image on the pasteboard beats text on it**, because a copied
screenshot carries both flavours and the text one is usually a file URL, so
checking text first would silently paste a path where a picture was meant; and
the file is written **beside neko's other caches, not to `/tmp`**, because a
prompt referencing a swept file is worse than one that never had the image —
it looks like it worked. Keyed by content hash, so pasting the same screenshot
twice writes one file, and pruned oldest-first past `MAX_ATTACHMENTS`.
