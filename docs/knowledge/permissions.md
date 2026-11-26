# Permissions, asked in the task (S1, 2026-11-26)

Neko never skips permissions. Claude Code runs with `--permission-mode default --permission-prompt-tool stdio`, so every step it
would ask about comes to Neko as a `can_use_tool` request. Neko's policy (`crates/neko-core/src/policy.rs`) decides, or asks the
person in the task.

## The two modes (Settings)

- **Ask before risky steps** (the default). Go ahead: reading inside the task's folder, and editing there when the folder is the
  task's own (a scratch folder or a git worktree). Ask: running a command, reading or writing outside the folder, the network, a
  tool Neko does not know, and the first edit in a chosen folder that is not a repository.
- **Full access**. Everything goes ahead. Settings says so plainly. Neko still uses the same route; it just answers yes.

Paths are resolved against the folder, with `..` and links followed, so a link that points out of the folder is outside.

## A request

A stored row (`permissions`): what it wants (title), why Neko asks, the exact command or path, whether it is destructive, its
state, when it was asked and answered. It is also a part of the transcript, so it shows where it happened.

States: `pending` → `allowed`, `always_allowed`, `denied`, or `expired`. Only a pending request can be answered; the store refuses
a second answer (`already_answered`). When a run ends, its pending requests expire. When the CLI takes a request back, it
expires.

## Always allow

Scoped to the task, never wider: the same program for a command (`cargo` in `cd x && cargo test`), the same folder outside the
task's, the same host on the network, the same unknown tool, edits in the same plain folder.

A destructive step (`rm`, `git push --force`, `git reset --hard`, `sudo` and the like) has no default answer and no "Always"
button; an "always" answer to one counts once.

## In the app

A pending request is rune-ui's Confirmation (Allow once in ink, first; Always allow; Deny; Escape denies), with a quiet line under
it saying why Neko asks. A destructive one is drawn by Neko in the same look with two equal answers, Allow once and Deny. Once
answered it becomes one quiet line. The task's sidebar row shows a warning mark while it waits, and the window announces it.
