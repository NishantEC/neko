# Agent profiles: backend verification

Profiles are durable Neko identities with names and instructions. Unassigned
workspaces belong to the default Neko profile, preserving existing behavior.
An explicit assignment changes the owner of a workspace's tasks, tools, skills,
and responsibilities. Existing memories and conversations retain their original
profile identity. Global chat follows the selected profile; scoped chat follows
the workspace owner. Profile history and unrelated workspace metadata are not
injected across owners.

Directional read grants share only the source profile's global memories, within
a separate 4 KB prompt budget. They do not share workspace notes, conversations,
instructions, credentials, or action tools. Revocation stops future injection;
it cannot undo information already read. The existing Codex filesystem sandbox
does not guarantee cross-profile filesystem confidentiality.

Assignment is rejected while the workspace has unfinished tickets, pending chat,
enabled schedules, or enabled responsibilities. Configuration changes increment
an authority revision: previous MCP capabilities fail closed, running task
watchdogs cancel, and late chat/responsibility/skill-proposal results are rejected.
Selecting an active profile does not redirect an already-started chat. Profile
configuration edits conservatively invalidate all existing run capabilities.

Validation performed on the local source:

- Core profile and conversation tests: durable defaults/configuration, bounded
  fields, directionality/revocation, memory ownership/deduplication, workspace
  assignment gating, prompt/history isolation and ticket destination.
- Daemon workbench and MCP tests: active-selection race, stale reply mutation
  rejection, stale capability rejection, existing responsibility/approval paths.
- `node scripts/smoke-profiles.mjs`: real isolated daemon, SQLite, framed IPC and
  deterministic child execution prove profile instructions/global memory,
  workspace memory, denied cross-profile context, explicit grant and revocation,
  restart persistence, and absence of implicit tool grants.

The default smoke is deterministic, not live-model proof.

## Integrated UI and live-model proof

The native Agent profiles page creates/edits identities, selects an unscoped
chat identity, assigns the selected workspace and manages directional read
grants. Today filters history by both profile and exact workspace. Memory is
profile-filtered and supports editing without moving the original entry's scope.

`NEKO_SMOKE_LIVE=1 node scripts/smoke-profiles.mjs` passed against the real Codex
CLI in `neko-profiles-smoke-u9lDQY`. The diagnostic user message does not contain
the expected tokens; replies reflect injected instructions, own memories,
workspace memories, explicit sharing and revocation after restart. This proves
the tested model's use of those contexts, not filesystem confidentiality or the
ability to erase information already seen in past conversations.

A later real-model rerun with the explicit ambient-tool restrictions passed in
`neko-profiles-smoke-QVJk4U`.

A native profile window was launched with isolated fixture data and `key=false`.
`/tmp/neko-profiles-native.png` shows the initial profile form, but still shows
pre-connection chrome despite the later `loaded=true connected=true` log. This
capture is not accepted as proof of loaded profile data or switching. No focus
was taken, no installed app was replaced, and no external provider was mutated.
