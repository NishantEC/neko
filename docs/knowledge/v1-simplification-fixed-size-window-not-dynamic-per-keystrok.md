## v1 simplification: fixed-size window, not dynamic per-keystroke resize

`gpui::Window::resize` exists, but this task had no safe way to verify its
resize-anchor behavior on a borderless always-on-top popup window before
committing to it on the hotkey-latency-critical path. `neko::panel::Root`
instead renders at one fixed size (680×448) with the footer pinned to the true
bottom via a flex-grow content area, so short result lists leave quiet space
above the footer rather than the window growing or shrinking. Documented in
`panel.rs`'s own module doc comment. Real dynamic resizing is a follow-up, not
attempted here specifically because it was untested territory on the one path
this task's acceptance criteria measures a hard number against. The
*height* (448) is still exactly this: fixed, never resized at runtime. The
*width* no longer varies at the real `NSWindow` level at all either, since
"Mode view resize seam" below — the window is a fixed
`PANEL_WIDTH_WITH_DETAIL_PX`, and only the panel `div`'s own centering
changes for a mode transition.

**A fixed content area with two possible section headers (apps, clipboard)
means the row count that fits isn't always `RESULT_LIMIT` (8) — a header eats
real space nothing budgeted for before clipboard existed.** Rendering
whatever the daemon returned and relying on `overflow_hidden()` to clip the
excess produced a real defect: the last row would render half-visible,
jammed against the footer, instead of being fully shown or fully absent.
`panel::fit_within_budget` (called on every search response, before results
are stored in `Root`, so a truncated-but-out-of-sync `selected` index can
never point at a row that isn't rendered) fixes that — but an earlier
version fixed it by dropping the *entire* clipboard section whenever apps
alone filled the budget, which just moved the "Search and ranking" crowd-out
bug into the client instead of removing it (six or more app matches made
clipboard invisible again, silently). The corrected version reserves one
section header plus one row for clipboard *before* deciding how many apps
fit — `fit_section` spends whatever budget is actually left, in order, so
apps only give up rows they don't strictly need, and clipboard always gets
at least its reservation plus any unused app budget. Still never a partial
row, never a header with zero rows under it. Covered by `panel.rs`'s own
tests, including the exact 6-apps-plus-1-clipboard-row shape that exposed
both bugs in turn, and a 20-apps case proving the reservation holds well
past the original repro's app count. **Generalized from exactly two
sections to however many providers a response actually contains** by the
provider-abstraction task — see "Provider abstraction" above, "Cross-
provider ranking" subsection, for the current N-section algorithm; the
reservation-and-rollover behavior described above is still exactly what it
does for the 2-section case.
