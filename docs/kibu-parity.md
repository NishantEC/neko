# Kibu → Neko gap list

Source: `madhurjyadc/kibu` at `4397da4`. Neko branch `codex/kibu-parity` (from `7000f1f`). Second sweep 2026-10-02.

**Done** = built and tested on this branch · **Have** = Neko already did it · **Partial** · **Gap** · **Skip** = Kibu's product, not Neko's.

## Built on this branch

| Area | What Neko does now | Proof |
|---|---|---|
| Model catalog | Daemon reads Codex `app-server` (account, plan, default, efforts), Ollama, LM Studio; no `ocx` | live: ChatGPT · Pro, 17 models |
| Check & use model | One minimal reply through the real runner; saved only on success | live: pass 10 s; bad model refused, old choice kept |
| Error messages | Quota, sign-in, plan, newer-Codex, timeout; CLI log noise ignored; no secrets shown | unit + live |
| Codex install | Newest of all installs (found 3 here: nvm 0.146, Homebrew 0.155, ChatGPT.app 0.159) | Diagnostics |
| Diagnostics | Settings → Diagnostics: installs, versions, per-runtime timings, app↔daemon round trip | smoke |
| Secrets in memory | Refused on every write, proposal and prompt; reply says why | unit + smoke |
| Memory chat verbs | "remember that…", "forget…", "what do you remember" answered by code in ~2 ms | smoke |
| Relevant memory | Prompts carry memory sharing a word with the request, plus standing facts about you | unit |
| Memory citations | Lines tagged `[m:…]`; replies end with "From memory: …" | unit |
| Memory switches | "Suggest new memories" and "Use memory" on the Memory page, enforced in the daemon | unit + smoke |
| Ticket history | Delete a finished ticket; Clear finished (failed stay for retry); worktrees and files kept | unit + smoke |
| Stop all work | Work menu, status menu, `/stop`, global ⌘⇧Esc | unit + smoke |
| Slash commands | `/stop /remember /forget /recall /memory /tickets /clear /models /permissions /setup /help` with suggestions | unit |
| Previous app context | ⌥Return attaches the previous app's selection and window title, marked untrusted, 4,000-char cap | unit |
| Keyboard approvals | ⌘1 allow / ⌘2 deny when exactly one tool request waits | build |
| Hotkey fallback | A taken chord falls back to the next free one and says which | unit |
| Permissions page | Accessibility, clipboard, protected workspace folders; reading never prompts | unit |
| Protected folders | System, credential and Neko-data folders refused as workspaces | unit + smoke |
| Uninstall | Moves the app (and optionally data) to the Trash; stops work, login item, daemon; resets Accessibility | unit (plan only) |
| Untrusted tool output | MCP call results described to the model as untrusted data | prompt text |
| IPC robustness | A malformed request gets an error reply instead of a closed connection | unit + smoke |

## Still open

| Kibu | Neko | Fit | What it takes |
|---|---|---|---|
| Claude Code / OpenCode as workers, using their own logins | Gap | Core | **Needs your decision** (below). New runner adapter: stream-json events → receipts, MCP bridge via `--mcp-config`, write limits without Codex's sandbox |
| Their model catalogs | Gap | Core | Small once they can run tasks |
| Search inside documents, synonyms (CV/resume, Aadhaar) | Gap | Core | File search is filename-only; content search must not disturb the tuned ranking |
| Per-task spend limit, "wrap up" at 75% | Partial | Maybe | Neko has timeouts and worker caps; `codex exec` reports usage only at turn end, so a mid-turn budget needs app-server turns |
| Preview of planned file changes before they apply | Partial | Maybe | Neko reviews a diff after the build in its worktree; a pre-build preview would be new |
| Undo | Partial | Maybe | Neko never writes outside task worktrees, so there's little to undo; remote MCP side effects can't be undone by anyone |
| "Only while working" presence (appear, linger 6 s, hide) | Partial | Maybe | Activity capsule exists; no linger/auto-hide policy |
| Auto-update | Gap | Maybe | Sparkle or a GitHub-release check |
| Capability answer built from real registered tools | Partial | Maybe | `/help` lists commands; not connected tools |
| Local arithmetic / time questions without a model | Gap | Maybe | Palette could answer "2+2" or "time in Tokyo" |

## Skip

Calendar, Reminders, Notes and Mail drafts; timers and reminders; system volume and appearance; Shortcuts runner; driving your browser; the pet sprite and personality; the Jev router model (separate paid key). These are Kibu's product. Any of them can come later as a user-added MCP server.

## Found in this sweep (Neko-only)

- Two daemon singleton-socket tests fail when both daemon test binaries run in parallel; they pass alone (3/3). Existing flake.
- Uninstall, ⌥Return context, ⌘1/⌘2 and the Permissions page are built and unit-tested, but not yet clicked through in the installed app.

## Open decision

How Neko runs Claude, Grok and other non-OpenAI models now that `ocx` is out of discovery. A previously saved `opencodex` runtime still runs; the picker no longer offers it.

1. Claude Code / OpenCode as workers, with their own logins (Kibu's way)
2. A Neko proxy in the daemon calling provider APIs with Keychain keys
3. Codex account + local models only

