# The transcript (S1, 2026-11-26)

What a task said and did, stored so a restart loses nothing. `crates/neko-core/src/records.rs` and `conductor.rs`.

## Rows (store version 2)

- `tasks` gains `folder` (chosen), `workdir`, `branch`, `base_commit`, `repo`, `plain`.
- `runs`: one start of an agent: runtime, model, status (`running`, `done`, `failed`, `stopped`, `interrupted`), start, end,
  the CLI's conversation id, folder, branch, reason, and changes (files, lines added, removed).
- `usage`: one line per run, as the CLI reported it: tokens in, out, cached, the cost if it gave one, the model, how to read the
  cost (`charged`, `api_equivalent`, `unknown`), and who reported it.
- `parts`: the transcript in order (`seq` per task). A part is the person's words, the agent's words, thinking, a tool call (its
  state, a short input and output), a permission request, or a notice. The body is JSON in one column.
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

Only what the CLI reported, labelled with who reported it. Under a subscription login the figure is an API-equivalent price, not
money charged, and the app says so. With no cost reported, the run card shows tokens; with neither, nothing. The sidebar's foot
shows today's total.
