## Two evidence-safety defects, found by a capture that reported `key window true`

Both were introduced by work earlier the same night, and both are the same
shape: **an evidence process must not be able to take real input.**

- **An evidence run installed a menu bar item.** `menu_bar::install` ran
  unconditionally, so every throwaway client added a live, clickable ⌘ to the
  captain's real menu bar — and clicking one calls `summon_from_outside`,
  which activates the panel. Now skipped on any evidence run, beside the
  hotkey guard that was already there for exactly this reason.
- **The launch log claimed a hotkey registration that had just been skipped.**
  It printed `registered hotkey ⌥Space -> id None` two lines after "evidence
  run — skipping live hotkey registration". The `None` was the truth and the
  sentence was not. It reports the outcome now. **A log that contradicts
  itself costs more than one that says less** — it sent this investigation
  after a hotkey that had never been registered.

**And a capture had to be destroyed.** A macOS notification carrying a private
code floated over the panel during a window-scoped `screencapture -l`, which
is supposed to be immune to what is in front of it and is not, for anything
in the notification layer. Deleted immediately, never committed. **A
window-scoped capture is not automatically private** — check what is in the
frame before saving one, the same way a region capture would demand.
