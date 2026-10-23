## What three reviewers found, and the two defects worse than the one reported

The captain reported "mouse clicking doesn't work" and asked for designers.
Three read-only reviewers went over pointer interaction, the eight modes, and
the two windows. **They cannot click** — this repo forbids synthetic input —
so they read, drove the daemon over the wire, and used the evidence hooks,
which is how every finding tonight was found anyway.

**The reported bug was real and simple.** `render_row` had an id, a hover
tint and a pointer cursor's worth of styling and **no `on_click` at all**;
the `⌘K` menu rows were the same; `render_meter` was a third. Only agent
tiles, the mode back arrow and two banner controls had ever been wired. A row
now selects and confirms on one click, and a menu row runs its action — with
a destructive one still needing two clicks, because a control that deletes on
one click and not on one Enter is the same control behaving differently by
input device.

**Two findings were worse than the reported one.**

**`activation_error` was set on every failed Enter and rendered nowhere.**
Its only paint site was `render_footer`, deleted when the footer went, and
nothing replaced it. So *every failure in the app was completely silent*:
"that schedule is gone", "type what to send first", "that plan is stale",
the `paseo` CLI's own JSON errors, a 20-second timeout — the panel does not
hide on failure, so pressing Enter looked exactly like pressing nothing.
`AGENTS.md` records this same defect as fixed once before; removing the
footer reintroduced it worse. It renders in the input row now, in
`state_danger`, taking the `esc` hint's place — an error outranks a reminder
of a key that works either way. **The lesson is not about this field**: when
a container is deleted, what it was rendering has to be re-homed or the data
becomes invisible while still being maintained.

**Adding a search folder was impossible.** Preferences' Enter handler guarded
with `if self.focused_control().is_none() { return; }`, meaning "Enter belongs
to the field when the ring is on nothing" — but `controls()` always begins
with the four tabs, so the ring is never on nothing. Arriving at the Search
tab always parks it on `Tab(Search)`, so typing a path and pressing Enter
re-selected the already-selected tab and `stop_propagation`'d in the capture
phase; the keystroke never reached the field. "Press ↵ to add" could not add,
ever. The real question was never "is the ring on something" but "is it on
something *other than a tab*".

**Two more false statements in the copy**, the same class as the README
correction earlier: onboarding's permissions screen promised "Nothing you
copy or open ever leaves this Mac" — written when neko only searched apps and
the clipboard, and untrue since `ask.rs`, `usage.rs` and everything under
Agents — and "Change retention anytime in Settings", a control that does not
exist (`HISTORY_LIMIT` is a hard 200 and Preferences has five rows, none of
them this). Both fixed; both had been true when written, which is exactly why
nobody noticed.

**Also fixed**: `⌘K` had no on-screen affordance at all after the footer went,
so Kill terminal / Delete schedule / Archive were mouse-unreachable *and*
undiscoverable — the hint is back in the input row, shown only when the
selected row actually has actions. The mode back arrow was a 15pt target that
started a window drag, because the input row begins a drag on mouse-*down*
and `on_click` is mouse-*up*. `keeps_open` was `false` on Usage's "Refresh"
(which closed the panel instead of refreshing, since the refetch *is* the
re-search) and on a schedule's pause toggle (whose badge is the only thing
that says what it did). The Agents mode's first Enter always failed, because
entering clears the field and the row still said "Send ↵"; it says "Type the
task first" until there is one.

**A guard that is never read is not a guard.** `menu_open_before_this_press`
was written on every mouse-down and read by nothing, so a click meant to
dismiss the `⌘K` menu also fired the row underneath it — `on_mouse_down_out`
closes in the capture phase, and by the time a bubble-phase click handler
runs there is nothing left to tell "dismiss" from "activate".
