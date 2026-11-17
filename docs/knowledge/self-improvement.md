## Neko improves itself, one suggestion at a time (2026-11-17)

Neko notices trouble that repeats, asks the cheapest model for one fix, hands it to the person as one Inbox item
with a default, applies it only on their click, then watches whether it helped and offers Undo if not.
Code: `crates/neko-core/src/improve.rs` (+ `improve/signals.rs`, `prompt.rs`, `apply.rs`), `skills/revise.rs`,
`crates/neko-daemon/src/improve.rs`, `crates/neko-protocol/src/improve.rs`, web `web/src/setup/improve.ts`,
`web/src/pages/setup/Improvements.tsx`, verify area `scripts/verify/improve.mjs`. **Rune does not have the section yet**
(owner, 2026-11-17: the Rune identity pass ports it from the web page; the protocol needs nothing more).

### Signals (own rows, never the snapshot)

Observations live in `improve_signals_v1` (quiet), each with its time, so a signal can be counted again a week later
after the source data was trimmed. Gathered when the Mac is idle (same gate as the curator: no hold, no goal, no worker),
at most every 6 h, and on events (a ticket fails, a routine run fails; at most every 10 min, `improve::note_event`).
**Off means nothing is read or written** (`Suggestions`, Settings → AI, `improve_off_v1`, on by default).

| Key | Fires when |
| --- | --- |
| `missing_tool:<name>` | a failed ticket, routine run or a chat turn's text says "tool not found", "no such tool", "unknown tool", "MCP server … (not available / not connected / failed)", "command not found: x", "not installed" (case-insensitive; `signals::missing_tools`, a hand-written matcher, no regex crate). Fires on one sighting in 7 days |
| `correction:<topic>` | 3+ corrections on one topic in 14 days |
| `ask:<title key>` | the same `inbox::title_key` raised 3+ times in 7 days (a new item or an open one refreshed, `note_ask` in `absorb_repeats`); our own and goal items are not asks |

**Deviation: there were no topic keys to reuse.** Memory proposals and lessons have none, so `signals::topic_words`
makes them: content words, a topic key of the four longest, and a new correction joins an existing topic when it shares
at least 60% of that topic's words (and 2+). Sources: pending memory proposals, learned (non-`user`) memory entries, and
ticket-lesson skill proposals (dated when first seen).

### Proposals and limits

`improve::run_pass`: for each due signal one tool-free call on the judge's runtime (`workbench::ask_cheap_model`, Haiku
else a Codex mini), prompt `improve::prompt::prompt` (golden `tests/prompts/improve-proposal*.md`: signal, evidence ≤4 KB
with the spill rule from `spill::bounded_with`, the catalog's packs, the related skills, the scope's standing orders).
JSON `{kind, title, why, change, target, default_label}` (**`target` is an addition to the brief's shape**: the pack or
skill the change is for). `prompt::parse` validates: `install_pack` names a catalog pack, `skill_patch` an existing skill
and a complete SKILL.md the curator's `consolidate` accepts, `new_skill` a fresh name, `standing_order` one sentence,
`code_task` only with a Development checkout. A failure drops it, logs once and the key waits a day; no model stops the pass.
At most 3 a day (rolling), one open (open, sent, applied) per key, a rejected or settled key quiet for 30 days.

### Delivery and apply

Each suggestion is one Inbox item in a hidden routine `improve:<workspace>` (like goals): "Neko suggests: <title>",
why plus the change, default = `default_label`, link `improve:<id>`. `inbox_reply` maps the default to `improve::apply`
(phone push and replies work unchanged; typed words are refused, never read as instructions). Apply is only reached by
`Improve(Apply)` or that default, both the person's act.

| Kind | Apply |
| --- | --- |
| `skill_patch` | `skills::curator::submit` (the curator's own rules, extracted from `finish`): applied by itself only for a Neko-written, unedited, unlocked skill and a selection-only revision (`auto_safe`); otherwise a Patch proposal in Skills (state **Sent**). Lines the revision drops are archived in the file by `consolidate` |
| `new_skill` | a skill proposal in Skills (Sent); never installed by apply |
| `standing_order` | appends the line to the workspace's text (default text if none); always needs the click |
| `install_pack` | returns `open_pack: catalog:<name>`; the UI opens the preview; **nothing installs** (the row stays Open until the pack is installed through the digest flow, which `verify` notices) |
| `code_task` | `GoalCommand::Create` (by the person) in the workspace of the Development checkout, `done_check` = `bash scripts/check.sh` when it exists; refused with no checkout or no such workspace |

A Sent suggestion becomes Applied once the person approves it in Skills (file or folder is as proposed) and Rejected if
they turn it down there; rejecting a Sent one withdraws the Skills proposal.

### Verify and undo

Applying records the signal's count in the 7 days before (`before`). `verify` (every idle tick and on every List) waits 7
days from `applied_ms`: fired at least as often (min 1) means **Didn't help** and a second item "That didn't help: <title>"
(default Undo, link `improve-undo:<id>`); fewer or none means **Helped** (fewer-but-some counts as Helped; the brief
only named "stops firing"). Undo state is `improve_undo_v1:<id>` and an undone suggestion reads Rejected (30-day quiet).

- skill: restores the exact prior bytes through the normal patch path, only while the file is exactly what the suggestion
  wrote (an edited file refuses, nothing written); the file replaced is copied to `<data>/cleanup-archive/improve/`.
  A new skill's folder is moved there, never deleted.
- standing order: if the text is exactly what apply wrote, the earlier text (or "no entry") comes back exactly; otherwise
  only that line is removed and later edits stay.
- pack: `packs::remove` (which archives). Confirms in the UI.
- code task: checkpoints `Undo` (all runs, snapshotting unsaved edits first); if pushed, foreign commits or history moved,
  a revert task is made instead; a task that never ran is cancelled. Confirms in the UI.

### Tests

Stub model and fake signals in `signals.rs`, `apply.rs` (every kind, the pack apply never installs, no checkout refuses
code tasks, Helped vs didn't help raising the Undo item, each undo, the off switch, the limits) and the daemon's
`improve.rs` (a pass from a failed routine run). Web: `scripts/verify/improve.mjs` over `seed_improve` in the harness.
Tests never call a model.

### Not done

Rune section (see top). Chat turns are scanned as plain text, so a reply that merely discusses a missing tool can count
as a sighting. Improvement history is capped at 200 rows.
