# Evidence captures must never take the key window

`fm/neko-evidence-no-activate`. This documents a real incident, the fix, the
per-hook audit behind it, and the live readbacks that prove it.

## The incident

`evidence.rs::show_once` showed its window with `window.activate_window()` +
`cx.activate(true)` — making a throwaway evidence panel the system's real
**key** window. On a machine the captain is actively working on, his next
keystrokes land in that panel's search field instead of wherever he is
typing.

A worker doing a design review captured a screenshot and found the words
`fix it` already typed into the query field — the captain's own keystrokes,
on a run that set no query hook at all. That capture was deleted
immediately, nothing was persisted, and the worker reported it rather than
quietly continuing.

**Why the existing mitigation did not help.** `AGENTS.md` already prescribed
committing an obscure hotkey to an isolated instance, after an earlier
incident where a real `⌥Space` press landed on an evidence client. That was
in force here. It did not help, because the hazard is **key-window focus**,
not hotkey collision — two different mechanisms for the same outcome.

`AGENTS.md`'s standing rules covered synthetic input going *out* of the app
(no synthetic keystrokes, no `System Events`, no `CGEventPost`) and screen
content coming *out* of the machine (window-scoped capture only). Nothing
covered the app **taking real input**, which is the same hazard from the
other side.

## The fix

Non-activating is now the **default**, not an opt-in flag. An agent who
forgets a flag is precisely the failure mode above, so forgetting has to
fail safe.

- `show_once` shows the window with `material::order_front_regardless`,
  which "structurally cannot reach `windowDidBecomeKey:` at all"
  (`run_bench`'s own long-standing doc comment). The window paints
  normally, is fully `screencapture -l<windowID>`-able, and never takes
  keyboard focus from anything real.
- `window.focus(&root.focus_handle(cx), cx)` is kept on both paths. It is
  GPUI-internal focus only — it renders the caret and routes this process's
  own actions, and cannot pull real OS keystrokes into a window that is not
  key.
- Activation is a single, explicit opt-in: `NEKO_EVIDENCE_ACTIVATE=1`
  (`evidence::activation_opt_in`). Setting it prints a loud stderr warning
  saying the window *will* take real keystrokes.
- Every hook prints `neko: key window <bool>`, read back off the live
  `NSWindow` via the new `material::is_key_window` (`-[NSWindow
  isKeyWindow]`) — the same "verified, not trusted" pattern
  `verify_installed`/`verify_shadow_disabled` already establish. A run that
  ends up key without having opted in prints a `SAFETY WARNING` line.
- `main.rs` additionally suppresses the live OS hotkey registration for any
  evidence run (`evidence::evidence_run_active`). That closes the *other*
  half of the same hazard structurally, rather than relying on each agent
  remembering to commit an obscure combo to the isolated daemon first.

## Per-hook focus audit

| hook | could it take key focus before | can it now | why |
|---|---|---|---|
| `NEKO_SHOW_ON_LAUNCH` | **yes — this was the incident** | only with `NEKO_EVIDENCE_ACTIVATE=1` | now `order_front_regardless` by default |
| `NEKO_SHOW_QUERY` | via `show_once` | no | a modifier on `NEKO_SHOW_ON_LAUNCH`; drives `set_query_for_evidence` in-process, never focus |
| `NEKO_SHOW_CONFIRM` | via `show_once` | no | same — `confirm_for_evidence` |
| `NEKO_CYCLE_MODE_ONCE` | via `show_once` | no | same — `dismiss_for_evidence`/`confirm_for_evidence` |
| `NEKO_SHOW_ACTIONS_MENU` | via `show_once` | no | same — `open_actions_menu_for_evidence` |
| `NEKO_SCROLL_MODE_LIST_TO_BOTTOM` | via `show_once` | no | same — a `ScrollHandle` mutation |
| `NEKO_SHOW_SELECTION` | via `show_once` | no | same — `select_query_for_evidence` |
| `NEKO_REAL_CYCLES_BEFORE_SHOW` | no | no | already `order_front_regardless`/`order_out` only, by its own deliberate design |
| `NEKO_BENCH` | no | no | `order_front_regardless`/`order_out` only; now prints the readback that proves it |
| `NEKO_BENCH_REAL` | yes | **yes, and must** | it exists to measure the real `activate_window` path; now refuses to start without `NEKO_EVIDENCE_ACTIVATE=1` |
| `NEKO_BACKDROP_IMAGE` | no | no | opened with `focus: false`, below the panel's own window level |

`NEKO_BENCH_REAL` is the one legitimate exception. A non-activating
stand-in would measure a structurally different path — that stand-in
already exists and is `NEKO_BENCH`. Its activation is now explicit and
documented rather than incidental, and it cannot be reached by typing one
env var that does not obviously say what it does.

## Verification

Release binaries, isolated `HOME`, `verify_harness` (never the real
`neko-daemon` — its capture loop polls the systemwide pasteboard regardless
of `HOME`), client launched from a directory with no `neko-daemon` sibling
and an empty `PATH` so `daemon_launcher`'s spawn attempt fails cleanly. No
synthetic input of any kind. `caffeinate -u` first, per the standing
display-sleep note.

**Default path — `NEKO_SHOW_ON_LAUNCH=1`, nothing else:**

```
neko: evidence run — skipping live hotkey registration
neko: key window false (after show)
neko: window number 4868
neko: key window false (at capture)
neko: window rect 580px 255px 760px 420px
```

`docs/evidence/evidence-non-activating-capture.png` is the
`screencapture -l4868` taken at that moment: the panel fully painted — real
Applications rows with real cached icons, the Commands section, the seeded
Clipboard fixture, the footer — with an empty query field, captured while
`isKeyWindow` read `false` on the live `NSWindow`. The key-window state is
a native readback printed at capture time, not an assertion made elsewhere.

**`NEKO_BENCH_REAL=2` with no opt-in:**

```
neko: NEKO_BENCH_REAL measures the real activate_window() summon path, which
makes this window the system key window and captures real keystrokes typed
on this machine. Refusing to run without an explicit NEKO_EVIDENCE_ACTIVATE=1.
Use NEKO_BENCH for a non-activating latency bench.
```

**`NEKO_BENCH=6` — still measures, still never key:**

```
neko: key window false (bench summon 0)
neko: bench summon 0 latency 24.8255ms
neko: bench summon 1 latency 16.638042ms
neko: bench summon 2 latency 11.290208ms
neko: bench summon 3 latency 4.908875ms
neko: bench summon 4 latency 16.098833ms
neko: bench summon 5 latency 7.571417ms
neko: bench complete (6 summons)
```

(These figures are from a build without `scripts/setup-gpui-patch.sh`
applied in this worktree — see `AGENTS.md`, "Summon latency". The point
here is that the bench still runs and still reports per-summon latency, not
the absolute numbers.)

**Not run, deliberately:** `NEKO_BENCH_REAL` with the opt-in actually set.
Running it would take real keyboard focus for several seconds — exactly the
hazard this task exists to close — and the captain was at his machine
throughout. Its gate was verified to refuse; the measurement path itself is
unchanged apart from that gate and one added readback, and the code is
unchanged from the version last exercised in
`docs/evidence/gpui-fork-migration-report.md` §4.

## The rule

**An evidence window must never become the key window.** Non-activating is
the default and must stay the default. If a future capture genuinely needs
a real activation, say so in the run — `NEKO_EVIDENCE_ACTIVATE=1` — and do
not run it on a machine someone is using.
