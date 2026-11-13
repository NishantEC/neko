## Checkpoints and undo (2026-11-14)

Research idea 6 in `docs/research/t3code-and-zeron.md`: per-run checkpoints in
hidden git refs, a per-run diff and undo, without ever putting a commit on the
person's branch. Code: `neko_core::checkpoints` (git plumbing, rows, undo/redo),
`workbench/run_checkpoints.rs` (the run hooks, task view reads, the command,
crew tools), `web/src/work/Runs.tsx` (the run lines).

### What is snapshotted, and when

- **When.** Every *writable* ticket run (`execute_with_worktree`, not read-only
  planning): one snapshot at its start (`<n>-start`) and one when the execution
  ends however it ends (`<n>-end`; cancelled and failed runs get one too).
  `RunSlot` ends the run after `execute_with_worktree_in` returns; a daemon that
  stopped mid-run leaves a run with no end, which the next run's start closes as
  "stopped". A failure to snapshot is logged and never fails a run.
- **Where.** `refs/neko/checkpoints/<task>/<run>-{start,end}`, plus `safeN` (taken
  before every undo/redo) and `manualN` (edits outside any run, kept on request).
  Each ref is a commit of the folder's working state whose parent is the folder's
  HEAD at that moment (so the objects stay reachable and `git gc` keeps them).
- **How.** `checkpoints::capture`: a scratch index in the git dir (`GIT_INDEX_FILE`,
  seeded with a copy of the real index so only changed files are re-read), `git add
  -A` (tracked, modified and untracked files, honouring `.gitignore`), `write-tree`,
  `commit-tree`, `update-ref`. The person's index, HEAD, branches and files are only
  read; a test compares the index file byte for byte. Repository clean/smudge
  filters are switched off (`git_without_filters`), as everywhere in the runner.
- **Size guard.** Files over 5 MB (`MAX_FILE_BYTES`) are excluded from the snapshot
  and recorded (`Point::skipped`, up to 20 names, the full count); the run line says
  "N large files not saved". Undo never touches them.
- **Both cases.** A ticket's own worktree is what runs use today (including
  `work_directly` tickets, which build in their worktree without a plan). The
  functions take any directory in any checkout, so a run that works directly in a
  person's checkout uses the same calls; nothing in Neko starts such a run yet.
- **Rows.** One settings row per ticket (`neko_checkpoints_v1:<task>`) and an index
  (`neko_checkpoints_index_v1`), never the workbench snapshot. Runs carry counts
  (files, +, -) computed once at the end.

### Undo, redo, refusals

`Command::Checkpoints(Undo{task_id, run, through_later, snapshot_unsaved, revert} |
Redo | SnapshotUnsaved)`. All refusals are decided before anything changes
(`check_undo`) and travel as `checkpoint:<code>: <message>`:

| Code | When | The offer |
| --- | --- | --- |
| `later_runs` | later runs exist that are not undone | "Undo back to here" (`through_later`) |
| `unsaved_changes` | the folder's tree is not the one the last run, undo or redo left (edits no checkpoint holds) | "Snapshot them first, then undo" (`snapshot_unsaved`) |
| `pushed` | a commit made since the run started is on a remote-tracking branch, or the ticket has an open pull request and commits were made | "Add a revert commit instead" (`revert`: files restored, history kept, one new commit) |
| `foreign_commits` | commits on the branch after the run's start that no Neko run made (not between a recorded run's start and end HEAD, not a revert commit Neko added: `Row::own_commits`), the person's own for instance | no rewind. Only "Add a revert commit instead": a three-way merge-tree reverses just this run's changes (end checkpoint as base) as a new commit, and refuses if it conflicts. The refusal names the commits |
| `busy` | a worker holds the ticket, or the run has no end yet | stop it first |

The ticket's folder must be one of Neko's task worktrees and exist (a parked
ticket refuses; reads still work, they use the source checkout's objects). Before
any restore a `safeN` snapshot is taken; `restore_tree` deletes files only the
current state has, writes files the target differs on, and leaves ignored files and
oversize files alone; the person's index is reset to the run's start HEAD (a plain
undo also moves the branch back when the run committed, so the branch ref is the
one thing an undo rewrites, and only for commits not pushed). **Redo** refuses to move the branch back over a commit that is on a remote (`pushed`) or that no run made (`foreign_commits`); Neko's own unpushed revert commit may be dropped. **Redo** restores the
safety snapshot (and moves the branch forward again); one redo is kept, a new run
ends it. The undo is a `checkpoint:` event on the ticket, and the next agent turn
gets a sentence saying which runs were undone and not to assume the old files
(`take_notice`, once, added to the run's direction).

### Agents

Crew tools (`the-chat-directs-the-crew.md`): `checkpoint_diff` (lists runs; `run`
or `whole` adds a diff, 48 KB) for the full chat and a ticket's own run (own ticket
only); `undo_run` for the full chat only. `undo_run` changes nothing: it runs the
same checks and returns the `neko-action` block (`kind: undo_run`, `text: "3"`,
optionally `back`, `snapshot`, `revert`) the model puts after its reply, so the
person confirms with the existing one-click chat action. Read-only chat has neither.

Safety: snapshots, undo, redo and expiry take turns (one process-wide lock), an
undo is refused while a worker or a publish holds the ticket, a restore that fails
half way puts the files back, and a file over the size guard or a folder holding
ignored files is never overwritten (the undo refuses instead). The task view does
not check for unsaved edits (that runs `git add -A`); the undo and the crew tools do.

### Housekeeping

`parking_tick` expires checkpoints older than 14 days and a Completed ticket's
checkpoints are deleted when its folder is parked (`delete_task`, then `git gc
--auto`). At most 50 refs per ticket: extras (not the redo's) go first, then the
oldest runs after the first, which stays for the "Whole task" diff. **Never
pushed**: git pushes only `refs/heads` and `refs/tags` by default; a test pushes
with `--all` and `--tags` to a local bare remote and finds no `refs/neko`. Neko's
publishing pushes a named branch.

### Web

Changes tab: "Runs" above "Files changed", one line per run (glyph, "Run 3", `N
files · +a −b`, "Undo this run"), a line expands to that run's diff (the same
`FileRow` renderer), "Whole task" spans first start to last end, undo asks first,
a refusal appears under the list with its offer, "Redo" appears after an undo.
`scripts/verify/checkpoints.mjs` drives it against a seeded task in a real temp
repository (`web_harness::seed_checkpoints`, one run committed and pushed to a
local bare remote).

### Not done

- The native (Swift) app does not show runs or undo.
- A goal's journal does not get the undo as its own entry; the ticket's events do.
- No run starts directly in a person's own checkout yet, so that path is covered by
  tests of the functions only.
- Planning (read-only) runs are not checkpointed; reviews are part of the build's run.
