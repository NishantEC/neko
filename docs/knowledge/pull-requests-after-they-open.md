## Pull requests after they open (2026-10-21)

Learned from a Paseo agent that walked Neko's PRs to merge by hand
(Oct 13–19): Neko stopped at "Opened a pull request", so 20 of 42 tasks still
said Ready to review while GitHub showed them merged (12) or closed (8).
`neko_core::pull_requests` decides; `workbench/pull_requests.rs` runs one
GraphQL query per open PR every 3 minutes (`CheckPullRequests` wakes it).
**The rules are taken from established tools' source, not invented** — the
first home-grown version misfired twice on its first night (below). Read the
module doc before changing them:

- `mergeStateStatus`, not `mergeable` (Kodiak): DIRTY = conflict, BEHIND,
  BLOCKED, UNSTABLE, UNKNOWN = decide nothing. `mergeable` is UNKNOWN right
  after a push; the first version's baseline missed two existing navihealth
  conflicts that way and then started agents on them as "new".
- Only `isRequired` checks count when a branch has any (Kodiak); cancelled,
  timed-out or stale runs are pending, not failures (Renovate, Mergify).
- A failure the base branch shares is not handed back (Sweep).
- Feedback = unresolved, non-outdated `reviewThreads` + top-level comments
  + review bodies, from people with a repo role, not bots (GraphQL types
  them `Bot`, with no `[bot]` suffix) and not the PR's author (Neko and other
  agents comment as that account) — OpenHands resolver, claude-code-action.
  `gh pr view --json reviews` has no inline text; the first version missed it.
- A conflict once per head (main moving again is not news; keying by base
  too re-sent athena #4632 each time main moved), a failure once per head and
  failing checks, at most `MAX_FOLLOW_UPS` (4) per
  PR (Sweep stops at 2); the first read is a baseline (keyed by head and
  base, so GitHub finishing a computation is not news).
- Hand-back: a note plus `start_when_planned`, reply marked handled (no
  intent model); the task runs in its own folder, review + publish push to
  the same PR. Neko never force-pushes; the note fast-forwards from the PR
  branch first, since others push to it.
- Merge is by click (`SetMergeWhenReady`): `gh pr merge --squash --body ""
  --match-head-commit <judged head> --delete-branch` (Kodiak; the empty body
  is the user's own choice from the Paseo session), `--admin` only for
  repositories in `Snapshot::merge_without_review`; BEHIND uses GitHub's
  update-branch API with `expected_head_sha`; a waiting reason is said once.
- `ReviewPullRequests` (Re-check all) merges the base only when DIRTY
  (Renovate `rebaseWhen=auto`: updating otherwise re-runs CI for nothing);
  agents end with `SCRAP:` (sets `close_suggestion`), `QUESTION:` or nothing.
  `NO_CHANGE:`/`SCRAP:` on a task with an open PR return it to
  ReadyForReview. `ClosePullRequest` needs your click and a comment.
- `FOLLOW_UP: <repo> | <title> | <detail>` in a result proposes a task in
  that repository (folder, remote or package name), AwaitingApproval,
  recorded in `Snapshot::follow_up_of` and drawn under its parent in the tree.
- A PR task's run that merges newer base-branch work (a conflict, or
  fast-forwarding to a branch updated with main) is reviewed and described
  from `native_runner::pull_request_base` (`merge-base HEAD origin/<base>`,
  only when newer than the run's start), which also becomes `base_commit`:
  otherwise a 7-file fix read as 914 files (navihealth #14671).
- Not adopted yet: in-thread replies/resolution after a fix (OpenHands), one
  batched query per repository and idle backoff, Renovate's "edited by
  someone else, stop pushing" block, and duplicate-title suffixes.
