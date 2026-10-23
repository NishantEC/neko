## Standing safety rule: an evidence window must never become the key window

**It recurred on 2026-09-02, and the guard caught it rather than a person
doing.** A `NEKO_SHOW_SKELETON` capture logged `key window false (after show)`
and then `key window true (at capture)` with the `SAFETY WARNING` line — the
window took key status *between* being shown and being photographed, with no
`NEKO_EVIDENCE_ACTIVATE` set and nothing in `show_once` asking for it. The
client was killed immediately and the capture checked before being kept: it
contains only placeholder plates, and the query field shows its own greyed
placeholder rather than typed text, so nothing had landed in it. The cause
was not established — the run also had a `caffeinate -u` user-activity
assertion alive, which is the only unusual thing about it, and that is a
hypothesis rather than a finding. **What is worth keeping: the readback is
what made this observable at all.** A capture that silently took focus would
have looked identical.



Binding on every future task in this repo, alongside the no-synthetic-input
rule and the window-scoped-capture-only rule (see "Window material" below).
Full incident account, the per-hook audit table, and the live readbacks:
`docs/evidence/evidence-key-window-safety-report.md`.

**The incident.** `evidence.rs::show_once` used to show its window with
`window.activate_window()` + `cx.activate(true)`, making a throwaway
evidence panel the system's real **key** window. On a machine the captain is
actively working on, his next keystrokes then land in that panel's search
field instead of wherever he is typing. Not hypothetical: a design-review
worker captured a screenshot and found the words `fix it` already in the
query field — the captain's own typing, on a run with no query hook set.
That capture was deleted immediately, nothing was persisted, and the worker
reported it rather than quietly continuing.

**Why the pre-existing mitigation didn't help.** This file already
prescribed committing an obscure hotkey to an isolated instance (see
"Evidence-capture hook: `NEKO_SHOW_CONFIRM`"), after an earlier incident
where a real `⌥Space` press landed on an evidence client. That was in force
here and did nothing, because the hazard is **key-window focus**, not
hotkey collision. This repo's standing rules covered synthetic input going
*out* of the app and screen content going *out* of the machine; nothing
covered the app **taking real input**, which is the same hazard from the
other side.

**The rule, and how it's enforced structurally rather than by memory.**

- **Every hook in `evidence.rs` is non-activating by default.** The window
  is shown with `material::order_front_regardless`, which "structurally
  cannot reach `windowDidBecomeKey:` at all" — it paints normally and is
  fully `screencapture -l<windowID>`-able while never taking keyboard
  focus. Forgetting a flag must fail *safe*; an opt-in safe path is exactly
  the mitigation shape that already failed once here.
- **Activation is one explicit opt-in, `NEKO_EVIDENCE_ACTIVATE=1`**
  (`evidence::activation_opt_in`), and it warns loudly on stderr when set.
  Only two hooks consult it. **`NEKO_BENCH_REAL` is the one legitimate
  exception** — measuring the real summon path *is* measuring
  `activate_window()`, and the non-activating stand-in already exists and
  is `NEKO_BENCH` — so it keeps its activation but **refuses to start**
  without the opt-in rather than being reachable by typing one env var
  that doesn't say what it does. Do not run it on a machine someone is
  using.
- **Focus state is read back, not asserted.** `material::is_key_window`
  (`-[NSWindow isKeyWindow]`, same "verified, not trusted" pattern as
  `verify_installed`/`verify_shadow_disabled`) drives a `neko: key window
  <bool>` line at every point that matters, so a capture is
  self-evidencing about focus. A run that ends up key without opting in
  prints a `SAFETY WARNING`.
- **An evidence run never registers a live OS hotkey.** `main.rs` skips
  `apply_initial` whenever `evidence::evidence_run_active()` — closing the
  other half of the same "an evidence process must not intercept real
  input" hazard structurally, instead of relying on each agent remembering
  to commit an obscure combo to the isolated daemon first.
- `window.focus(&root.focus_handle(cx), cx)` stays on both paths and is
  safe: it's GPUI-internal focus only (the caret, this process's own
  action routing) and cannot pull real OS keystrokes into a window that
  isn't key.

**If you add a hook to `evidence.rs`, add its row to that file's own
per-hook focus table** (can it take key focus, does it, why) — the table is
the audit, and an unlisted hook is an unaudited one.
