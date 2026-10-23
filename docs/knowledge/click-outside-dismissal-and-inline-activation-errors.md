## Click-outside dismissal and inline activation errors

Two more `neko-p0-fixes` defects, both in `main.rs`/`panel.rs`.

**Click-outside dismissal.** `main.rs` registers
`cx.observe_window_activation` once, on the one resident summon window,
right after `open_window` — `window.is_window_active()` false calls
`cx.hide()`, exactly the same hide `confirm()` already used for the
launched-a-result case. Registered only on that window, never on the
separate onboarding window entity, so onboarding is unaffected by
construction. **Fixing this also fixed a latent bug it would otherwise have
made worse**: the hotkey-press handler used to toggle an `Rc<Cell<bool>>`
`visible` flag rather than checking real window state, which desynced the
moment the window was hidden by anything other than that exact branch (a
`confirm()` hide, or this new click-outside hide) — the *next* hotkey press
would then silently no-op instead of re-summoning. `main.rs` now checks
`window.is_window_active()` live at the point of each hotkey press instead
of trusting a cached flag.

**Inline activation errors.** `panel::Root::confirm()` used to discard
`client.request(Request::Activate{..})`'s result and unconditionally
`cx.hide()` — an app moved/deleted since it was indexed, or the daemon
unreachable, closed the panel exactly as if the launch had worked, with no
indication anything failed. `confirm()` now matches the real outcome:
`Response::Error{message}` or a transport `Err` both set a new
`Root::activation_error: Option<String>` field and **do not** hide the
panel; only a real non-error response hides it. `render_footer()` swaps its
normal title/verb content for `"Couldn't open — {message}"` in
`theme::STATE_DANGER` (the same danger token `render_accessibility_banner`
already established — no new toast surface, no geometry change), and
`run_search()` clears `activation_error` as its first line so it never
outlives the query that produced it. Window-scoped GUI evidence (a real
file, indexed, then deleted before Enter, captured via the
`NEKO_SHOW_CONFIRM` evidence hook below):
`docs/evidence/p0-3-activation-error.png`.
