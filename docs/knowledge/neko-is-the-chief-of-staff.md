## Neko is the chief of staff (2026-10-17)

Every task goes through Neko. **Rulebook** (`neko_core::rulebook`,
Settings → Rulebook): one editable document (engineering and review
standards, this machine's limits, repository policy) that goes into Neko's
chat prompt and every task agent's prompt; empty means the built-in default.
Put standards there, not in per-role prompt strings. **Record**
(`neko_core::chief_of_staff::record`): the chat sees each task's repository,
branch, PR, stage, blocker, queue position and latest events, needs-you first.
**Actions**: the chat may propose `message`/`start`/`stop`/`run_next`/
`approve`/`accept`/`retry`/`rename` on existing tasks (`ChatMessage::actions`);
each waits for one click (`DecideChatAction`) and then runs the exact command
the app's own button sends, so no check is bypassed. `run_next` reorders
admission only. **Front door** (`neko_core::triage`, `workbench/triage.rs`):
watch-filed tasks wait in `Snapshot::untriaged` until one tool-free call
decides work (normal or run next) or duplicate of an open task (merged, the
open task gets a note). There is no "skip"; failure or 10 minutes releases
the task as work.
**Tree** (All tasks → Tree, `AgentTree.swift`): who started whom, derived in
the client from data already in the snapshot, with nothing new stored: split
children under their parent (`splits`), watch-filed tasks under the watch
(`mcp.sources`), chat tasks marked (`conversation[].ticket_ids`), the rest
"You asked". A task's runs are stretches of one agent role's events, closed by
`USAGE` (cost); the model is the `runtime_selections` entry in force when the
run started (the last 200 dispatches only). Codex reports no cost, and agents
use the user's own CLI tools without bridge receipts, so per-run cost is
partial and per-run tools are not known; watches show their connections.
