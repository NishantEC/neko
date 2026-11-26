# The transcript (S1, 2026-11-26)

What a task said and did, stored so a restart loses nothing. `crates/neko-core/src/records.rs` and `conductor.rs`.

## Rows (store version 2)

- `tasks` gains `folder` (chosen), `workdir`, `branch`, `base_commit`, `repo`, `plain`.
- `runs`: one start of an agent: runtime, model, status (`running`, `done`, `failed`, `stopped`, `interrupted`), start, end,
  the CLI's conversation id, folder, branch, reason, and changes (files, lines added, removed).
- `usage`: one line per run, as the CLI reported it: tokens in, out, cached, the cost if it gave one, the model, how to read the
  cost (`charged`, `api_equivalent`, `unknown`), who reported it, and (store version 3) the context the run's last model call held
  and the model's window when reported.
- `parts`: the transcript in order (`seq` per task). A part is the person's words, the agent's words, thinking, a tool call (its
  state, a short input and output, and a command's exit code when it ran and exited non-zero), a permission request, or a
  notice. The body is JSON in one column.
- A tool call's state: `running`, `done` (also a command that exited non-zero: it ran), `failed` (it could not run, was cut off or
  timed out), `denied` (the person said no, or no one answered).
- Thinking with no words (all Claude Code sends today) stores no part. Whether the agent is thinking reaches watching screens as
  `activity` events, never stored.
- `permissions`: see `permissions.md`.
- `settings`: key and value (the runtime new tasks use, the permission mode).

The task's own words are its first message; a follow-up is a person part. Every change also appends an event row, as in S0.

## Writing words

Words reach the screens at once, as `part_delta` events to screens watching the task. They are written to disk at most every 3
seconds and when their part ends. A measured 5-minute fake run is in the S1 report.

## A restart

At start the daemon marks every run still `running` as `interrupted`, its task too, expires its pending requests, closes its open
parts (keeping every word written) and adds a notice: "Neko stopped while this was running." A follow-up resumes the CLI's
conversation with `--resume`.

## Cost

Only what the CLI reported, labelled with who reported it. Nothing about money is on screen until the person opens the context
ring in a task's composer: rune-ui's popover shows how full the context is (the newest run's, against the reported window or an
assumed 200K), the task's tokens in, out and cached, the cost when one was reported, and, for a subscription login only, "API-equivalent,
as Claude Code reports it. Not charged on a subscription." The run card and the sidebar show no money.
