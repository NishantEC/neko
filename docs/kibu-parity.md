# Kibu → Neko gap list

Source: `madhurjyadc/kibu` at `4397da4` (read 2026-10-02). Neko: `codex/kibu-parity` from `7000f1f`.

Status: **Done** = built on this branch · **Have** = Neko already does it · **Partial** · **Gap** = Neko lacks it.
Fit: **Core** = belongs in Neko's job (workspaces, agents, tasks, launcher) · **Maybe** · **Skip** = Kibu's product, not Neko's.

## Done on this branch

| Kibu | Neko now |
|---|---|
| Model list read from each coding app (account, plan, default, hidden, efforts) | Daemon `agent_catalog`: Codex `app-server` (`account/read`, `model/list`), Ollama `/api/tags`, LM Studio `/v1/models`; no `ocx` |
| "Check & use model": one short reply, saved only on success | `CheckAgentModel { save }` runs the real no-tool runner; failure keeps the previous model |
| Credential-free, actionable failure text | `friendly_failure`: quota, sign-in, plan/unavailable, newer-Codex, timeout; ignores CLI log noise |
| Picks a working CLI | `resolve_codex` picks the newest installed Codex (was: first on PATH, which was stale here) |
| Memory refuses passwords, codes, keys, card and ID numbers | `neko_memory::sensitive` on every write, learning proposals and prompt assembly |
| One permissions list; reading never prompts | Settings → Permissions: Accessibility, clipboard, protected workspace folders, what Neko never asks for |

## Agents and models

| Kibu | Neko | Fit | Note |
|---|---|---|---|
| Claude Code as a worker, one persistent `stream-json` process per task, tools off | Gap | Core | Replaces `ocx` for Claude. Needs its own sandbox story; see open decision |
| OpenCode as a worker (`permission: deny`, scratch dir) | Gap | Core | Same as above, for Grok/others |
| Claude Code / OpenCode model catalogs | Gap | Core | Discovery code is easy once they can run tasks |
| OpenCode "free models only" filter (reported zero price) | Gap | Maybe | |
| Per-app saved model choice | Partial | Core | Neko stores one runtime (+ per-profile) |
| Planner swap behind one interface (`PlannerLike`) | Have | — | `AgentRuntime` + runner |
| Narrow the tool menu per request, widen on trouble | Gap | Maybe | Faster turns; Neko uses MCP bridge `neko_list_tools` |
| Pre-fetch "this" context (selection, front tab, Finder selection) | Gap | Core | Composer could attach the previous app's selection |
| Cheap router model for yes/no/choice decisions (Jev) | Gap | Skip | Paid third-party key |

## Task loop and safety

| Kibu | Neko | Fit | Note |
|---|---|---|---|
| Per-task step, wall-clock and **dollar** limits enforced before each step | Partial | Core | Neko has timeouts and worker caps; no spend limit |
| "Wrap up" note at 75% of the step budget | Gap | Maybe | |
| Every tool has a verifier; unverified success counts as failure | Partial | Have | Reviewer requires command receipts + diff coverage |
| Protected paths refused outright (`~/.ssh`, Keychains, `/System`…) | Gap | Core | Cheap guard for read-only scouts and MCP file tools |
| Folder-level grant ("allow this folder", never `~` or `/`) | Partial | Core | Workspace folders are the grant; no mid-task escalation |
| Shell with no shell: argv allowlist, `git` subcommand list, code runners ask every time | Gap | Maybe | Neko relies on Codex sandbox; useful for the MCP bridge |
| Stale UI references fail with "re-observe first" | Skip | — | Neko doesn't drive app UI |
| ⌘⇧Esc stops everything immediately | Gap | Core | Neko has per-task cancel; no global kill shortcut |
| Crash recovery: interrupted tasks marked on start | Have | — | Durable claims, restart preserves worktrees |
| Untrusted file/page text marked as data in prompts | Partial | Core | Check MCP tool output framing |

## Undo, history and evidence

| Kibu | Neko | Fit | Note |
|---|---|---|---|
| Undo newest-first, refuse when the world changed | Partial | Core | Worktrees isolate; no undo for integrated child patches or MCP side effects |
| `/steps`: every tool call and whether it was verified | Partial | Core | Ticket events exist; no compact "steps + verified" view |
| Delete a task / clear finished history (files untouched) | Gap | Core | No `DeleteTask` command |
| Result view: open files, reveal in Finder, open links | Partial | Core | |
| `TESTED.md`: what's live-proven vs faked vs untested | Partial | Core | `docs/evidence/` per change; no single ledger |
| `/bench`: time every route on this Mac | Gap | Maybe | Would have caught the stale-Codex issue |

## Memory

| Kibu | Neko | Fit | Note |
|---|---|---|---|
| "Remember that…" / "forget that…" / "what do you remember" in chat, handled by code | Partial | Core | Memory page + proposals; no chat verbs |
| Learn only choices the user made, never guesses | Have | — | Proposals require approval |
| Relevant-only recall (shared word/topic) instead of all memory | Gap | Core | Neko sends up to 4 KB of recent memory |
| Answer cites the memory it used ("From memory") | Gap | Core | Lets a wrong memory be caught |
| Turn learning or memory off | Partial | Core | |

## Interface

| Kibu | Neko | Fit | Note |
|---|---|---|---|
| Quick actions fill an editable draft (Find / Organize / Rename) | Gap | Maybe | Neko equivalents: Plan, Review, Fix |
| Slash commands in the composer (`/undo`, `/steps`, `/stop`, `/setup`…) | Gap | Core | |
| Drop files/folders to scope a task | Have | — | Composer attachments |
| Include the previous app (⌥Enter) | Partial | Core | Palette tracks the previous app; composer doesn't attach it |
| Answer questions with buttons and number keys | Partial | Core | Approvals have buttons; no number-key answers |
| Presence modes: on demand / only while working / always / menu bar | Partial | Maybe | Activity capsule; no "linger 6 s then hide" |
| Capability help answered locally ("what can you do") | Gap | Maybe | Built from real registered tools, so it can't drift |
| One-minute tour with permissions on each card, `/setup` to rerun | Partial | Core | Onboarding exists; no rerun |
| Uninstall from the menu (stops shortcut, login item, data choice) | Gap | Core | |
| Auto-update | Gap | Maybe | Kibu uses electron-updater; Neko would need Sparkle |
| Hotkey conflict picks the next free chord | Partial | Core | Neko detects and refuses; doesn't suggest |

## Mac work (Kibu's product, mostly not Neko's)

| Kibu | Neko | Fit |
|---|---|---|
| Calendar, Reminders, Notes, Mail draft via scripting, each change read back and undoable | Gap | Skip (or as an MCP server) |
| Timers and due reminders that bring the pet back | Gap | Skip |
| System appearance, volume, Shortcuts runner | Gap | Maybe (palette commands) |
| Your browser via Apple Events; Playwright browser | Gap | Skip |
| Folder organise / rename / natural document search (Aadhaar, CV…) without a planner | Partial | Maybe (Neko has file search) |
| Desktop pet sprite, personality lines | Gap | Skip |

## Open decision

How Neko runs non-OpenAI models now that `ocx` is out of discovery. The runner still accepts an existing `opencodex` runtime so saved choices don't break, but the picker no longer offers it.

1. Claude Code / OpenCode as workers, using their own logins (Kibu's way)
2. A Neko proxy in the daemon calling provider APIs with Keychain keys
3. Codex account + local models only

