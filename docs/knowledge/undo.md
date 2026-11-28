# Undo: checkpoints, review, undo and redo (2026-11-26; the right pane 2026-11-27)

A run's change line has Review and Undo (or Redo), and the right pane's Overview and Changes tab have Undo and Redo too. Review shows what that run changed as a diff in the Changes tab. Undo puts the files back as they were
before the run; Redo puts its changes back. Nothing is ever merged, pushed, committed on a branch or lost. Protocol 7, store 5.

## How other tools do it, and where Neko differs

Claude Code rewinds files it edited itself. T3 Code reverts a thread to a turn. Both keep pictures of the folder per turn. Neko
does the same per run, with git, so it also sees what a shell command changed. It does not move a branch or write a commit on
one: undo writes working files only.

## A checkpoint

At each run's start (before the agent starts) and at its end (before the folder may be tidied), the daemon takes a snapshot of
the task's folder. It is a commit under Neko's own hidden refs:

```text
refs/neko/checkpoints/<task id>/<run id>/start
refs/neko/checkpoints/<task id>/<run id>/end
refs/neko/checkpoints/<task id>/<run id>/before_undo-<n>   (the folder just before an "Undo anyway")
refs/neko/checkpoints/<task id>/<run id>/before_redo-<n>   (the folder just before a "Redo anyway")
```

Where it is kept:

- **A task's worktree** (a git workspace): in the person's repository, under those refs. Worktrees share refs and objects.
- **Any other folder** (a plain workspace, General's scratch folder): in a shadow repository Neko owns,
  `<data>/undo/<short id>.git`, a bare repository with the folder as its work tree. Nothing is written into the folder.
- **A plain folder that is a git repository with no commits yet**: refused plainly ("this folder is a git repository with no
  commits yet"). The run goes ahead without undo.

## The exact git commands

All in `crates/neko-core/src/checkpoint.rs`. Each runs with `GIT_OPTIONAL_LOCKS=0`, `-c core.fsmonitor=false -c
core.untrackedCache=false -c gc.auto=0 -c maintenance.auto=false -c core.quotePath=false`, no prompt, and without any `GIT_*`
variable the daemon inherited. In a shadow repository each also has `--git-dir <shadow> --work-tree <folder>`; in a worktree,
`-C <worktree>`. `$TMP` is a temporary index file in `<data>/undo/tmp/`, removed afterwards.

A checkpoint:

```text
git rev-parse --git-path MERGE_HEAD | rebase-merge | rebase-apply | CHERRY_PICK_HEAD | REVERT_HEAD   (refuse when one exists)
git rev-parse --verify --quiet HEAD^{commit}                     (the parent; none in a repository: refuse)
cp "$(git rev-parse --git-path index)" $TMP                      (a copy, for its stat data; else: GIT_INDEX_FILE=$TMP git read-tree HEAD)
GIT_INDEX_FILE=$TMP git status --porcelain=v1 -z --untracked-files=all --no-renames --ignore-submodules=all   (the size guard)
GIT_INDEX_FILE=$TMP git add -A
GIT_INDEX_FILE=$TMP git write-tree
GIT_INDEX_FILE=$TMP git ls-files -z                              (how many files)
git commit-tree <tree> -p <HEAD> -m "Neko checkpoint: start of run_…" --no-gpg-sign   (author and committer "Neko")
git update-ref refs/neko/checkpoints/<task>/<run>/<kind> <commit>
```

A shadow repository is made once with `git init --quiet --bare <shadow>` and `git --git-dir <shadow> config core.autocrlf false`
(also `core.safecrlf false`, `gc.auto 0`), its `info/exclude` holding `.DS_Store`. It keeps its own index (`neko-index`) between
snapshots so the next one reads only what changed.

A diff (the Changes tab; Uncommitted compares `HEAD^{tree}` with the folder's tree now):

```text
git diff-tree -r -z -M --raw <start tree> <end tree>         (statuses, renames, modes, blob ids)
git diff-tree -r -z -M --numstat <start tree> <end tree>     (lines added and removed; "-" for a binary file)
git cat-file -s <blob>                                       (a binary file's sizes)
git diff-tree -p -M --no-color --no-ext-diff -U3 <start tree> <end tree> -- <path> [<old path>]   (one file's lines)
```

Undo (Redo is the same with the trees swapped):

```text
(the checkpoint commands above, without commit-tree and update-ref: the folder's tree now)
git diff-tree -r -z --name-only --no-renames <expected tree> <tree now>   (the files changed since, when they differ)
GIT_INDEX_FILE=$TMP git read-tree <tree now>
GIT_INDEX_FILE=$TMP git update-index -q --refresh
GIT_INDEX_FILE=$TMP git read-tree -m -u --no-recurse-submodules <tree now> <target tree>
```

The last command writes the working files from one tree to the other: edits, new files, deleted files (and folders left empty),
renames, mode changes, binary files, links, in every subfolder. Git checks every file first; if one changed in between it
refuses and writes nothing.

Housekeeping: `git for-each-ref --format=%(refname) refs/neko/checkpoints/<task>/`, `git update-ref -d <ref>` for each, and in a
shadow repository only, `git gc --quiet --prune=now`.

## Kittens

A kitten's run has checkpoints like any run, kept under its own task id. A kitten in a copy of its own (`subagents.md`) is checkpointed in its own worktree. A kitten in the shared folder is checkpointed in the folder it shares, the nearest task above it that has a folder of its own (`subagents::folder_task`), and its refs are kept in the same repository or the same shadow repository. A copy's first snapshot (`workplace::copy_base`) is the same `git add -A` into a temporary index and `write-tree` as a checkpoint, and keeps no ref of its own: the new branch holds it. A merge the thread makes belongs to the thread's run: its start and end checkpoints see the merged files, and Undo of that run writes the thread's working files only (the kittens' branches and copies stay).

## What is never touched

The person's index (and the worktree's), HEAD, every branch, the stash, remotes, config, hooks and their working files. Neko
never runs `git add` on a real index, `git stash`, `git commit`, `git checkout`, `git reset` or `git config` on their repository.
The tests compare the index file, HEAD, config, `packed-refs`, the reflog, every branch, the stash list, `git status`, and every
file's mode, size and modification time before and after a checkpoint, byte for byte. What a checkpoint does add: loose objects,
and Neko's refs. `git log --all` shows those refs; a normal `git push` does not send them.

Undo writes working files only. After an undo the index of the task's worktree is as it was (anything the agent staged stays
staged), so `git status` there shows the difference.

## Limits

- **Size guard:** a snapshot that would store more than **10,000** new or changed files, or more than **100 MB** of them
  (ignored files aside; a shadow repository's first snapshot counts every file) is not made. The run card says "Undo is not
  available for this run: the folder has 12,480 new or changed files (310.2 MB), more than undo keeps (10,000 files or 100.0
  MB)." The run itself works. Tests change the limits with `NEKO_UNDO_MAX_FILES` and `NEKO_UNDO_MAX_BYTES`.
- **Time:** one snapshot may take **10 seconds**; past that it is abandoned and undo is not available for that run.
- **The bound on starting a run:** the start checkpoint is taken on the run's own thread, never on the request path (the reply
  to the person comes at once). The agent starts after it, so the snapshot is provably of the folder before the run. The run waits
  at most 10 seconds; measured below, it waits about 0.3 seconds on a 5,000-file repository.

## Refusals

| Case | What Neko says |
| --- | --- |
| A repository mid-merge, mid-rebase, mid-cherry-pick or mid-revert | no checkpoint: "the repository is in the middle of a merge"; undo refused the same way |
| A locked index (`index.lock`) | a checkpoint is fine (it reads a copy); undo and redo are refused: "git is busy in this folder (its index is locked)" |
| No commits yet | "this folder is a git repository with no commits yet" |
| A detached HEAD | works: the checkpoint's parent is the commit HEAD names |
| A submodule | works: its commit is recorded; its files are never written |
| The run committed | "This run committed its work; revert the commit instead." Review still works |
| New commits on the branch since the run | refused: "The task's branch has new commits since this run, so Neko did not touch it." |
| Files changed since the run | nothing is written; the app names them and offers "Undo anyway" |

## Which run, and in what order

`neko_protocol::undo::offers` decides, for the app and the daemon alike (the daemon refuses anything it does not offer):

- Undo goes newest first: only the newest run that changed something and is not undone. An older run's Undo is off: "Undo the
  newer run first: runs are undone newest first."
- A newer run with no checkpoint blocks it: "A newer run has no checkpoint, so this one cannot be undone safely."
- Redo goes the other way: the oldest undone run first.
- Nothing can be pressed while a run works: "Undo waits until the run in progress ends."
- A new run after an undo starts from the undone files. It drops the redo, says so in a notice ("This message starts from the
  undone files, so the undone run can no longer be redone."), and its words to the agent start with one plain line:
  "(Neko: the person undid the changes of your last run, so the files are as they were before it. Do not assume those edits are
  still there.)" The person's words are stored as they typed them.

## Files changed since, and "Undo anyway"

Before restoring, the daemon takes the folder's tree now. It must equal the run's end tree (for Redo, its start tree). If not,
nothing is written and the reply is `changed_since` with the paths. "Undo anyway" (behind a confirmation that names them) first
keeps the folder as it is as a `before_undo-<n>` checkpoint, then undoes from there. Redo after that puts back exactly the folder
as it was before the undo, the person's later edits included. A "Redo anyway" keeps a `before_redo-<n>` checkpoint the same way;
its ref keeps those files, but nothing on screen brings them back yet.

## Housekeeping

- **Removing a task** removes its refs in the repository and its shadow repository.
- **A tidied worktree** (`workspaces.md`, the tidy rule) takes its checkpoints with it: its refs go, and its runs say "The undo
  history of this run was removed." (A run that changed nothing says nothing.)
- **Older than 30 days:** when a task finishes a run, its checkpoints older than 30 days go, never its newest run's pair. Tests
  shorten it with `NEKO_UNDO_KEEP_MS`.
- **Settings ▸ Workspaces** says "Storage used by undo: 18.4 MB in Neko's folder, 38 checkpoints in 6 tasks. Checkpoints of a git
  repository are kept in the repository itself." "Clear undo history…" asks first, then removes every ref, every shadow
  repository and marks every run's history removed. It is refused while a task works.
- The objects of a removed ref stay in the person's repository until their own `git gc` prunes them (two weeks by git's default).

## Measured

`crates/neko-core/tests/checkpoints.rs` (`a_checkpoint_of_5000_files_is_quick`), on this Mac with tests running beside it, git
2.50: a repository of 5,000 files of about 2 KB each, all different, in 50 folders.

| Snapshot | Time | Bytes |
| --- | --- | --- |
| Repository, nothing changed | 0.30 s | a commit and nothing else |
| Repository, 50 files changed and 50 new | 0.36 s | 132 KB of new objects |
| Plain folder, its first (every file stored) | 2.5 s | 960 KB in the shadow repository |
| Plain folder, the next one | 0.16 s | |

## Review

The Changes tab of the right pane (the change line's Review, ⌘⇧G, ⌘D, the thread menu's All changes) shows the changes of one of three
sources, a switch above the files (protocol 15: `get_diff` and `get_diff_file` take `source`):

- **This run:** the run's start checkpoint against its end one.
- **All runs:** from the first run's start to where the newest run left the folder (`source: thread`; a request with no run and no
  source still means this, as before).
- **Uncommitted:** the folder as it is now against the newest commit of its branch (`source: uncommitted`). It compares the same way
  a checkpoint is made (`checkpoint::tree_now`: a copy of the index, `git add -A`, `git write-tree`, nothing in the person's
  repository changes) with `HEAD^{tree}`, then the usual `git diff-tree` commands. Only a task's own checkout or a chosen folder that is a
  git repository with commits has one; a plain folder, a repository with no commits and a folder that is gone are refused in words
  ("This folder is not a git repository with commits, so there is nothing to compare with.").

The conversation stays beside the tab. `get_overview` (the Overview tab) returns the thread's folder and branch, the All runs files and
the uncommitted figures in one reply. Each file is rune-ui's Diff, folded until opened; its header has the path (or "old → new") and its + and − counts. Its
lines come from the daemon the first time it opens (`get_diff_file`), cut at 1,000 lines with "Show all" (up to 50,000). A
binary file is named with its sizes, never shown. An open file's foot has a note (moved from, now executable, "Showing 1,000 of
4,210 lines"), Show all, Copy diff, Copy path and Reveal in Finder.

## Not here yet

- Copy diff copies a whole file's lines, signs included, in one press; the lines can also be selected (rune-ui's Diff, 0.10).
- No way on screen to bring back a `before_redo` checkpoint.
- Undo of one file, or of part of a run.

## Tests

`crates/neko-core/tests/checkpoints.rs` (real temporary repositories: what a snapshot holds, nothing touched, exact restores of
every kind of change, files changed since, the size guard, the shadow repository, mid-merge, no commits, a locked index, a
detached HEAD, a submodule, refs), `crates/neko-protocol/src/undo.rs` (which run is offered), `crates/neko-daemon/tests/undo.rs`
(the real daemon with the fake CLI editing files: review, undo, redo, the agent told, "Undo anyway", a run that committed, the size
guard, a plain folder, housekeeping) and `apps/neko/tests/undo.rs` (the app against a real daemon: Review in the Changes tab and its three sources, Undo, Redo, the
follow-up).
