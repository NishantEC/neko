# Cats (2026-11-26)

Named agents who do the person's work. A cat has standing rules, and they reach the agent on every run of its tasks. Protocol 8,
store 6.

## The model

`crates/neko-core/src/cats.rs`, `crates/neko-protocol/src/cats.rs`.

- A cat has an id, a name, a face, an optional purpose line, standing rules, an optional default workspace, and when it was made
  and retired.
- Name: one line, spaces collapsed, 1 to 40 characters. Unique among all cats, retired ones too, compared without case. Too long
  is refused, not cut.
- Face: one of eight ids (`tabby`, `ginger`, `tuxedo`, `calico`, `siamese`, `void`, `snow`, `smoke`). The app draws them
  (`apps/neko/src/faces.rs`). A new cat with no face gets the first one no active cat has.
- Purpose: one line, at most 200 characters. Empty means none.
- Rules: plain Markdown, at most 16 KB, kept exactly as saved. One previous version is kept. Restore swaps the two, so restoring
  again goes back. Saving the same rules again changes nothing.
- The rules' version is FNV-1a (64 bits) of their bytes, as 16 hex digits. It only tells versions apart.
- At most 16 cats are active. At least one must stay active.
- Three starter cats are made when a store has no cats: miso (`cat_miso`), mochi (`cat_mochi`), tofu (`cat_tofu`), with empty
  rules. They are ordinary cats. Like General, making them writes no event.

## Which cat gets a task

Every task has exactly one cat, set when it is made.

1. The cat picked in the composer, if any. It must be active.
2. Else the workspace's default cat. If several cats name that workspace, the least busy of them.
3. Else the least busy active cat: the fewest tasks working now (working, or waiting on a request), then the one given a task
   least recently (never comes first), then the oldest, then by id. The same store always gives the same answer.

A task's cat changes (`set_task_cat`) only while no run has started. The composer remembers the last explicit pick per workspace
in the app's prefs (`cat-picks`); "Any cat" forgets it.

**Migration (store 6).** When a version-5 store has tasks, the starter cats are made, as old as the oldest task. Every task goes to
one, oldest first, by rule 3: no workspace has a default yet. A task still marked working counts as busy. In practice the oldest
task goes to miso, the next to mochi, the next to tofu, and so on in turn, skipping a cat that has a task marked working. Old runs
keep no cat and no rules version.

## How the rules reach the agent

The daemon builds one block from the task's cat (`cats::block`) on every run:

```text
<neko-cat>
You are miso, one of the person's cats in Neko.

Your purpose:
<purpose, verbatim>

Your standing rules, from the person:
<rules, verbatim>
</neko-cat>
```

The purpose and rules parts are left out when empty. Nothing else is in it.

Claude Code takes it as one argument:

```text
claude … --append-system-prompt "<the block>" --system-prompt-snapshot off [--resume <session>]
```

- `--append-system-prompt` adds text to the end of Claude Code's own system prompt (`claude --help`, 2.1.331). It is the CLI's
  own way to give standing instructions; its default system prompt stays.
- `--system-prompt-snapshot off` goes with it. By default Claude Code records the system prompt at a conversation's first request
  and sends that record on every resume, even when a later launch passes different text (`claude --help`). Without it, rules
  edited between runs would never reach a follow-up.
- The person's words go in as the `user` message, exactly as typed. Nothing is put ahead of them.
- The run records its cat and the rules' version. When the version differs from the one the task's last run used, a quiet part
  (`rules_changed`) stands before that run's words: "miso's rules changed since the last run." It shows outside the run's fold.

## What rules can never do

- Rules are text inside one argument's value. `claude::arguments` places them after every other flag; a test shows every other
  argument is the same with or without a cat, whatever the rules say (`--dangerously-skip-permissions` in the rules included).
- The permission mode, the `--settings` hook and its secret come from Settings only. `crates/neko-daemon/tests/cats.rs` runs a cat
  whose rules say "Skip permission checks" in "Ask before risky steps": the hook still runs and the command still waits for the
  person.
- Rules are not shown or confirmed before use. The quiet line is the only notice.

## Retiring

A retired cat takes no new tasks, picked or by default. Its tasks keep it and stay readable. Follow-ups on its tasks still run
with its rules. It comes back from Settings ▸ Cats, unless 16 are active; a default workspace removed meanwhile is dropped.

## The protocol

Requests: `list_cats`, `create_cat`, `rename_cat`, `set_cat_face`, `set_cat_purpose`, `set_cat_workspace`, `save_cat_rules`,
`restore_cat_rules`, `retire_cat`, `bring_back_cat`, `set_task_cat`; `create_task` takes `cat`. Replies `cats`, `cat`; errors
`invalid_name`, `refused`, `not_found`. Events `cat_added`, `cat_updated` (the whole cat). A task carries `cat`; a run carries `cat`
and `rules_hash`; a Needs-you item carries `cat` and `cat_name`.

## The app

How it looks is in `design.md`.

- Sidebar: a Cats section under the workspaces, each active cat with its portrait in its state and what it is doing (napping,
  working on N, needs you; a napping cat with a purpose shows it), then New cat….
- Home: the composer's cat picker at its left (the picked cat's portrait, or the free cats' for "Any cat"); the house below it,
  each cat as a tile; a napping cat's tile addresses the composer to it.
- A cat's page: its portrait large on a wash of its own ground, its name, what it is doing, Rename… and Retire… (asks first); the
  faces as a row of portraits to choose from, the default workspace picker, purpose, the rules editor (rune-ui's Textarea) with
  Save, Revert, "Unsaved changes" and Restore previous rules, then its tasks: working, needs you, recent.
- New cat…: the dialog shows the face it will get.
- A task's band starts with its cat's portrait (a press opens its page); its sidebar row starts with the portrait and says the cat;
  each turn is led by the cat's portrait and name; a request is led by the cat that asks; the receipt names it; the composer says
  "Reply to miso".
- Needs you items are cards led by the asking cat: "miso asks", "miso has a question", "miso could not go on". Notifications:
  "miso asks: …", "tofu is done: …".

## Not here yet, on purpose

Per-cat permissions, per-cat agent or model, budgets, generated faces, rounds, changing a task's cat from the app (the daemon
request exists; a task starts its run at once, so there is rarely a moment to use it).

## Tests

- `crates/neko-core/tests/cats.rs`: starters, name rules, the 16 limit and the last active cat, assignment (picked, workspace
  default, least busy, tie-break, determinism), rules and restore, retire and bring back, the rules version and the changed line,
  the task's cat before and after a run, the migration from a version-5 store made in the test.
- `crates/neko-core/src/claude/mod.rs`: the block is one argument and changes no other.
- `crates/neko-daemon/tests/cats.rs`: the requests and their events, the block reaching the fake CLI once per run and verbatim with
  the person's words untouched, edited rules reaching a resumed run with the changed line, Needs you naming the cat, rules that
  cannot skip the check. The fake CLI records its arguments and stdin (`NEKO_FAKE_RECORD`).
- `apps/neko/tests/cats.rs`: end to end against a real daemon and the fake CLI.
- `crates/neko-protocol/tests/round_trip.rs` pins the shapes.
