# Cats (2026-11-26)

Named agents who do the person's work. A cat has standing rules, and they reach the agent on every run of its tasks. Protocol 8,
store 6. Since protocol 14 / store 11 a thread has no cat: see "Threads have no cat".

## The model

`crates/neko-core/src/cats.rs`, `crates/neko-protocol/src/cats.rs`.

- A cat has an id, a name, a face, an optional purpose line, standing rules, an optional default workspace (kept for a Cattain's optional project later; it assigns nothing), and when it was made
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

## Threads have no cat

Nothing assigns a cat. A thread is just the agent; `Task.cat` is `Option<String>` and is absent on the wire when there is none.
`create_task` takes no `cat`. The old rules (the cat picked in the composer, the project's default cat, the least busy cat) and their
tests are removed.

Why: cats are to become Cattains, a cat that runs other threads. Assigning by load made every ordinary thread wear a cat's face and
rules, which made faces noise; a face should mean something.

What still has a cat:

- **A round's task** has its round's cat (`rounds.md`).
- **Tasks made before** keep their cat as history, and their runs still carry that cat's rules block.
- **A subagent** inherits its parent's cat, or none.

A thread with no cat is handed no `<neko-cat>` block and its words are untouched (daemon test
`a_thread_with_no_cat_is_handed_no_cat_block_and_its_words_untouched`); every run still gets the subagent paragraph in
`--append-system-prompt`. `set_task_cat` is removed.

**Migration (store 6, history).** When a version-5 store had tasks, the starter cats were made and each task was given one, oldest
first, by the least-busy rule then in force. Store 11 changes nothing for cats: `tasks.cat_id` was already nullable and old tasks
keep theirs.

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

A retired cat takes no new work after this, and its rounds stop running. Its tasks keep it and stay readable. Follow-ups on its tasks still run
with its rules. It comes back from Settings ▸ Cats, unless 16 are active; a default workspace removed meanwhile is dropped.

## The protocol

Requests: `list_cats`, `create_cat`, `rename_cat`, `set_cat_face`, `set_cat_purpose`, `set_cat_workspace`, `save_cat_rules`,
`restore_cat_rules`, `retire_cat`, `bring_back_cat`. Replies `cats`, `cat`; errors
`invalid_name`, `refused`, `not_found`. Events `cat_added`, `cat_updated` (the whole cat). A task carries `cat` when it has one; a run carries `cat`
and `rules_hash`; a Needs-you item carries `cat` and `cat_name` (empty strings for a thread with no cat).

## The app

How it looks is in `design.md`.

- Sidebar: **Cattains**, directly under New thread, Briefing, Needs you and Rounds and above the workspaces. It lists only the cats
  at work now (a task working, a request or a question waiting for the person); only tasks a cat was given (rounds, older tasks) put a cat at work, so the section is usually absent (no header, no empty line); a napping cat is not listed. A row is the cat's portrait in its state (a turning ring while it works, an amber
  dot when it needs the person), its name and one line of what it is doing: the working task's title, `needs you: Run a command` (the
  request's short title; `a question`, `could not go on`), or `working on 2` / `needs you · 2 tasks` with its active tasks nested under
  it (the same task row, stepped in 12 points, as Capy nests a Captain's crew). A press opens the cat's page. There is no New cat… and
  no list of idle cats in the sidebar: cats are made, renamed, given a face, rules and a default workspace, retired and brought back
  in **Settings ▸ Cats** (New cat… is its first button, Open on each cat reaches the page), and a task's band opens its cat's page.
- New thread has no cat picker, no house of cat tiles and no "Any cat" (`threads.md`).
- A cat's page: its portrait large on a quiet wash of its own ground (every wash the same step from the page, `faces::wash`), its name, what it is doing, Rename… and Retire… (asks first); the
  faces as a row of portraits to choose from, the default workspace picker, purpose, the rules editor (rune-ui's Textarea) with
  Save, Revert, "Unsaved changes" and Restore previous rules, then its tasks: working, needs you, recent.
- New cat… (Settings ▸ Cats): the dialog shows the face it will get; its page opens when it is made.
- A thread has no head: a turn has no portrait, name or "done" row, a request is led by a small amber dot (red if destructive) and
  says "The agent asks", the receipt says "Changed 3 files" with no cat name, and the composer says "Message the agent". The band
  has no portrait; a task a cat was given shows the cat's name as quiet text (`task-cat`) that opens its page.
- Needs you items are cards led by a small amber or red dot: "The agent asks", "The agent has a question", "The agent could not go
  on". Notifications carry the thread's title and no cat name (`needs-you.md`).

## Not here yet, on purpose

Per-cat permissions, per-cat agent or model, budgets, generated faces, Cattains (a cat that runs other threads).

## Tests

- `crates/neko-core/tests/cats.rs`: starters, name rules, the 16 limit and the last active cat, that a thread has no cat, rules and restore, retire and bring back, the rules version and the changed line,
  the task's cat before and after a run, the migration from a version-5 store made in the test. Daemon tests that need a task with a cat make it from a round run (`support::task_of_cat`).
- `crates/neko-core/src/claude/mod.rs`: the block is one argument and changes no other.
- `crates/neko-daemon/tests/cats.rs`: the requests and their events, the block reaching the fake CLI once per run and verbatim with
  the person's words untouched, edited rules reaching a resumed run with the changed line, Needs you naming the cat, rules that
  cannot skip the check. The fake CLI records its arguments and stdin (`NEKO_FAKE_RECORD`).
- `apps/neko/tests/cats.rs`: end to end against a real daemon and the fake CLI.
- `crates/neko-protocol/tests/round_trip.rs` pins the shapes.
