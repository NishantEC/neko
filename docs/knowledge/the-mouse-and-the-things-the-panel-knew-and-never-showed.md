## The mouse, and the things the panel knew and never showed

A pass over every finding three reviewers left open, plus what the scrollbar
work surfaced. Each is small; together they are most of what "the mouse
doesn't work" meant.

**The search field had an I-beam cursor and no mouse listeners at all** — the
strongest affordance in the app keeping no promise. Click places the caret,
double-click takes the word, triple-click takes everything, drag extends.
Two details are the difference between working and nearly working:
`byte_index_for_x` **clamps** rather than returning `None` outside the shaped
run, which is where a drag spends most of its time and where `None` would
freeze the selection where the pointer left the text; and the move/up listeners
are registered on the **window from `paint`**, not on the field's div, because
a div's `on_mouse_move` is gated on its hitbox being hovered and this field is
one line tall. Registering in paint also scopes them to the frames that need
them — gpui clears window mouse listeners every frame, so there is nothing to
unregister. **The press calls `stop_propagation`, and that is what stops the
panel dragging out from under it** (`render_input_row` begins a window drag on
mouse-down; the field is a descendant, so its bubble handler runs first) — but
only when the press was genuinely handled, since before the first paint there is
no `ShapedLine` to place a caret against. Only a **single** click arms a drag:
dragging out of a double-click extends by whole words on this platform, a
materially bigger behaviour, left out rather than half-built.
`selection_for_click` is pure and separately tested because the rest needs a
`ShapedLine` that only exists after a real paint. **Not verified under a real
mouse** — synthesising pointer input is forbidden here.

**`⌘K` was a label.** Kill terminal, Delete schedule, Archive and every session
mode sat behind a keystroke rendered as static text with no click handler. It is
a real trigger now, and it brings back the race the footer's own trigger had:
clicking it while the menu is open fires the card's capture-phase
`on_mouse_down_out` *and* the trigger's bubble-phase click from one press, so
reading current state finds `None` and reopens. `menu_open_before_this_press` —
the snapshot taken before any of that — is what it reads. **Both directions are
pinned**, because a guard that suppresses a genuine open is the same bug wearing
the other face.

**`⌘K` also opened the menu for a row nobody could see was chosen.** While the
keyboard is up in the agent grid, `selected` keeps its old value on purpose
(only one thing paints as selected), so `results[selected]` acted on a different
item than the one highlighted — and the hint above it had the same bug
independently. Both go through one `highlighted_item()` now. **Two callers doing
that arithmetic separately is how they came apart.**

**`ModeChrome::title` was live data nothing rendered** — a mode announced itself
only through its placeholder, which disappears the moment anybody types. It is a
chip beside the field: the one place with room the query cannot overwrite.

**The verb went missing exactly where Enter is least guessable.** `render_row`
drops `action_label` on `compact` rows because the 264px column has no space,
which left "Paste" and "Open folder" unlabelled. A compact row always implies a
detail pane (one flag drives both), so the pane is guaranteed to exist wherever
the row gave the verb up.

**A clipped row's hidden half was unreachable.** gpui cannot report whether a
`truncate()` actually clipped, so `row_text_may_be_clipped` is an estimate and
says so — compact rows always, full-width rows once their text passes what the
column holds, with the subtitle spending the same budget because it shares the
line. Erring toward showing it costs a tooltip nobody needed; erring the other
way costs content nobody can reach.

**Enter could block for seconds showing nothing.** `Root::activating` shares the
search tell's slot and outranks it: a search is "the list is about to change",
an activation is "the thing you pressed is happening". No reveal delay, unlike a
search, which usually answers in microseconds and would flicker. Cleared on
summon.

**In Preferences, clicking a control never moved the focus ring** — the ring is
that window's only statement of where the keyboard is, so the next Enter acted
somewhere the eye was not, which is worse than no ring because it is a confident
wrong answer. Pointer and keyboard share one cursor now. **And nothing in that
window hovered**, in an app whose panel tints every row under the pointer; each
control hovers in the channel it is not already using (a tab tints, the switch —
already a fill — moves its border, Remove takes the danger wash). The Remove
chip measured ~21pt against WCAG 2.5.8's 24pt floor, the only control under it
and the one that destroys something.
