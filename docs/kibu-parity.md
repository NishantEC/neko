# Kibu → Neko gap list

Source: `madhurjyadc/kibu` at `4397da4`. Neko branch `codex/kibu-parity` (from `7000f1f`). Third sweep 2026-10-02.

**Done** = built and tested on this branch · **Have** = Neko already did it · **Skip** = Kibu's product, not Neko's.

## Agents and models

| Area | What Neko does now | Proof |
|---|---|---|
| Model catalog | Daemon reads Codex `app-server`, Claude Code `initialize`, `opencode models`, Ollama, LM Studio; no `ocx` | live, including a bare GUI-style PATH |
| Claude Code and OpenCode workers | Run with their own logins under a Neko `sandbox-exec` profile: writes only to the worktree, the CLI's state folders and a private temp dir | live: edit in worktree, receipts, write to `~` blocked |
| Role tool limits | Read-only roles get no edit tools; extraction gets none; web tools never; OpenCode ignores repository config | unit |
| Check & use model | One reply through the real runner; saved only on success | live for Codex, Claude Code, OpenCode |
| Error messages | Quota, sign-in, plan, newer Codex, not responding, timeout; log noise ignored; nothing secret shown | unit + live |
| Codex install | Newest of every install (nvm 0.146, Homebrew 0.155, ChatGPT.app 0.159 here) | Diagnostics |
| Per-ticket budget | Stops a ticket at a dollar amount, before each phase and mid-run, from reported cost | unit |
| Diagnostics | Installs, versions, per-runtime timings, app ↔ daemon round trip | in-app |

## Safety and history

| Area | What Neko does now | Proof |
|---|---|---|
| Stop all work | ⌘⇧Esc anywhere, Work menu, status menu, `/stop` | unit + smoke |
| Ticket history | Delete a finished ticket; Clear finished (failed ones stay) | unit + smoke |
| Ticket changes | Read-only file list and patch from the ticket's worktree | build + smoke (no-worktree path) |
| Protected folders | System, credential and Neko-data folders refused as workspaces | unit + smoke |
| Untrusted content | MCP results and ⌥Return context are marked as data | prompt text + unit |
| IPC | A malformed request gets an error reply | unit + smoke |
| Undo | Not needed: workers can only write inside their own worktree, and nothing reaches your repo until you take it | design |

## Memory

| Area | What Neko does now | Proof |
|---|---|---|
| Secrets | Keys, tokens, card/ID numbers and "password is …" refused everywhere | unit + smoke |
| Chat verbs | "remember that…", "forget…", "what do you remember" answered by code | smoke |
| Relevance | Prompts carry memory that shares a word with the request, plus standing facts about you | unit |
| Citations | Replies end with "From memory: …" | unit |
| Switches | Suggest new memories / use memory | unit + smoke |

## Interface

| Area | What Neko does now | Proof |
|---|---|---|
| Slash commands | `/stop /remember /forget /recall /memory /tickets /clear /models /permissions /setup /help` with suggestions | unit + in-app |
| "What can you do?" | Answered from the real setup | smoke + in-app |
| Previous app context | ⌥Return attaches the previous app's selection and window title | unit |
| Approvals | ⌘1 / ⌘2 for the one waiting request | build |
| Hotkey | A taken chord falls back to the next free one | unit |
| Permissions page | Accessibility, clipboard, protected workspace folders; no prompts on read | in-app |
| Presence | Optional floating capsule while work runs; shows the outcome 6 s; click opens Neko | unit |
| Quick panel answers | Arithmetic and "time in <city>", locally; Enter copies | live IPC |
| Document search | "Search inside documents" mode: Spotlight text + names, filler dropped, synonyms | live IPC |
| Updates | About compares the build's commit with GitHub main; Update now refuses a dirty or non-main checkout | in-app + unit |
| Uninstall | Moves the app (and optionally data) to the Trash | unit (plan) |

## Skip

Calendar, Reminders, Notes and Mail; timers; system volume and appearance; Shortcuts; driving your browser; the pet and personality; the Jev router model. Any of them can come later as a user-added MCP server.

## Found while building

- A GUI-launched daemon has a bare PATH; npm-installed CLIs need `node`. Discovery and runners now use the full agent search path.
- OpenCode reads its project from `PWD`/`--dir`, not the process cwd; without this it worked in the wrong folder (the sandbox blocked the writes).
- OpenCode's free tier rejects requests that offer no tools at all; no-tool runs use shell "ask", which auto-rejects.
- Startup could hang on "Connecting to Neko…" if the first request beat the daemon; setup now finishes once the daemon answers.
- Two daemon singleton-socket tests are flaky when both daemon test binaries run in parallel (pass alone).

## Known limits

- Shell commands run by Claude Code and OpenCode workers can reach the network; Codex disables it. File writes are confined for all three.
- Uninstall, ⌥Return, ⌘1/⌘2 and the floating capsule are unit-tested but weren't exercised in the running app.
- The budget only counts runtimes that report cost; Codex subscriptions report none.

