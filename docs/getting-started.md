# Getting started

From a fresh Mac to a working Neko. The five steps are in the
[README](../README.md#install-on-your-mac); this page says what each one does
and what to do when it does not go to plan.

## What you need

| Need | Check | If missing |
| --- | --- | --- |
| macOS 14 or newer | `sw_vers -productVersion` | Software Update |
| Swift (Xcode or its Command Line Tools) | `swift --version` | `xcode-select --install` |
| Rust 1.88 or newer | `rustc --version` | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh`, then a new Terminal window |
| Node 20 or newer, and pnpm | `node --version`, `pnpm --version` | `brew install node && corepack enable pnpm` |
| One agent CLI, signed in | the first-run setup shows them | see below |
| `gh` (optional) | `gh auth status` | `brew install gh && gh auth login`; only for pull requests |

`bash scripts/install.sh --check` runs every check above and prints the exact
command for whatever is missing. It changes nothing.

Full Xcode is not required. Without it the build skips one decorative shader.

## Agents

Neko does not bring a model. It runs the agent CLIs on your Mac, with your own
sign-in and plan. The setup lists which it found and how to sign in:

| Agent | Install | Sign in |
| --- | --- | --- |
| Claude Code | `npm install -g @anthropic-ai/claude-code` | `claude /login` |
| Codex | `npm install -g @openai/codex` (the Codex inside ChatGPT.app is found too) | `codex login` |
| OpenCode | `npm install -g opencode-ai` | `opencode auth login` |

Run the sign-in in Terminal, then press **Check again** in the setup.

## The first run

`scripts/install.sh` opens Neko and the setup follows. Every step is optional;
**Skip setup** is on every screen, and nothing runs on its own until you switch
it on.

1. **Welcome.** Greets you by the first name on this Mac's account.
2. **Agents.** Which CLIs were found, signed in or not, with the command to fix it.
3. **Workspaces.** Folders Neko found (code folders under `~/Documents`, `~/code`,
   `~/Developer` and similar) with the MCP servers each one has, your home folder
   as Personal, and a box to add any other folder.
4. **Routines.** Off. Turn them on for background checks of messages, pull
   requests and your day. They use your agents and each scope's own tools.
5. **This Mac.** Calendar (your meetings beside Neko's work), phone reminders
   (an Apple Reminders item on your iPhone when something needs you), clipboard
   history, and the Accessibility pane. macOS asks once, only for what you
   switch on.
6. **Done.** What you chose, at a glance. It never opens again by itself.

Open it again any time from **Settings → General → Show again**, from the ⌘K
palette (Show the first-run setup), or by typing `/setup` in the composer.

## Your data

Everything lives in `~/Library/Application Support/neko` (the database, the
web token, task worktrees). Reinstalling never touches it. To try Neko without
touching your real data, point the daemon somewhere else:
`NEKO_DATA_DIR=/absolute/path`.

## When something goes wrong

- **`install.sh` stops with MISSING.** Run the command it prints and run the
  script again; it picks up where the checks left off.
- **The first build is slow.** Compiling Rust and Swift from nothing takes
  5 to 15 minutes. Later builds reuse the work.
- **macOS asks for permissions again after an update.** Builds are signed
  ad hoc unless you set `NEKO_CODESIGN_IDENTITY` to an Apple Development
  identity (`security find-identity -v -p codesigning`), and macOS ties
  permissions to the signature.
- **A routine says Claude Code is signed out.** Run `claude /login`; routines
  fall back to Codex meanwhile when it is signed in.
- **Pull requests do not open.** Install `gh` and run `gh auth login`.
- **Start over.** `bash scripts/install-native.sh --clean-data` moves your Neko
  data into a recoverable backup folder next to it.
- **Updates.** `git pull && bash scripts/install.sh`. Settings → About checks
  GitHub for newer commits and can take them from your own fork: set
  "Updates come from" to `owner/name`.

For contributors: `bash scripts/check.sh` is the test gate,
[docs/architecture.md](architecture.md) the map, and `scripts/verify/` the
browser checks for the web UI (they need a Chrome started with
`--remote-debugging-port=9333`).
