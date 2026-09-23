# Worktree audit and cleanup

Audited against main at `af1603e` before the personal-agent implementation.

## Findings

All nine non-main local branches and the detached Treehouse checkout were
ancestors of main (or equal to it). There were no unmerged source commits.
Fetching origin found no additional remote branch work to integrate.

| Checkout | Commit | Local material |
| --- | --- | --- |
| Treehouse | `4beced9` | 106 staged evidence screenshots, build cache |
| Claude icons | `4c264fe` | 110 staged evidence screenshots, build cache |
| Claude drag/snap | `bbaf561` | 112 staged evidence screenshots, build cache |
| Claude new-agent | `09ad6b8` | 106 staged evidence screenshots, build cache |
| Codex personal-agent preparation | `af1603e` | Build cache and patched-GPUI symlink; no implementation changes |

There were no unstaged source edits or untracked non-ignored files in those
checkouts. Ignored material was build output, plus the new checkout's symlink
to the main checkout's patched dependency. Process working-directory and
executable-path checks found no users of the worktrees before removal.

## Preservation and cleanup

Recovered 112 distinct screenshots to their original paths under the main
checkout's `docs/evidence/`. Every staged screenshot was byte-compared against
the recovered copy; duplicate paths had identical content. Images remain local
only, consistent with this directory's privacy policy. The ignore rule now
covers nested evidence directories, including theme captures.

Moved the five checkout directories into the macOS Trash folder
`neko-worktrees-20260923.A3W1dT`, then pruned their obsolete Git registrations.
These directories are recoverable until Trash is emptied; their old `.git`
pointers no longer register working checkouts. All committed source remains
reachable through main.

Removed nine merged local branch labels. Retained the fresh roughly 1 GB Cargo
build cache in the main checkout's ignored `target/` for subsequent work.
The roughly 32 GB of old checkout/build material remains in Trash, so this
cleanup has not yet reclaimed that disk space.

## Verification

After cleanup, `git worktree list` contains only the main checkout and
`git branch` contains only main. `git diff --check` passes. The pre-build
baseline had passed 424 tests across core, client, protocol, daemon and its
harness (five ignored); this cleanup changes no application code.

Scope: locally registered Neko worktrees, local branch references, and branches
available from the configured Git remote. Unrelated repository worktrees and
cloud task histories were not deleted.
