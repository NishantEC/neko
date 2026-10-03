# neko

A native personal-agent workspace and quick command center for macOS. Neko's
SwiftUI/AppKit app runs on a local Rust daemon. Open the app for your workspace;
press ⌥Space for quick search, clipboard history, and task attention.

## Neko-owned work

Create a named workspace with one or more existing folders using the native multi-folder picker, then browse hosted servers in the app, link an MCP definition already
in your local Codex/Claude setup, or add your own server in **Tools & skills → Connections**. Sentry's official hosted server is featured; no Linear, Slack, or other service is bundled
or required. Multiple connections, including instances of the same server,
have separate workspace scope, credentials, and permissions.

Code tasks need a Git repository because Neko builds in an isolated worktree;
non-Git folders can still be attached. When a workspace has multiple folders,
Neko's agent can name the folder for a ticket; an ambiguous task is held rather
than silently run in the first folder. Setup never requires Git just to register
a workspace.

1. Choose **Add connection → Browse MCP servers** to search the public MCP Registry
   for hosted HTTPS servers, select and review the destination, or import an existing
   definition from this Mac. You can also add a remote Streamable HTTP URL or an
   explicitly trusted local executable and JSON argument array. Registry listings
   are third-party metadata, not a trust guarantee. Neko does not download or install server packages.
2. Sign in through the browser if the server supports OAuth, or supply optional
   credential JSON in the masked field. Secrets are stored in macOS Keychain.
3. Discover tools and inspect their schemas. Discovered tools are available in
   the connection's workspace without a second per-tool switch.
4. A trusted connected source with a currently granted, read-declared tool for
   listing changing work starts one plan-only watch in each available workspace. An
   untried chat suggestion for that source is started instead of adding a
   duplicate. You can refine or pause the watch; checks run every ten minutes
   while the daemon is running, and failures back off. Sources with no such
   tools need review before unattended checking. Search-only reference catalogs
   stay available on demand rather than becoming empty recurring checks.
5. Keep **Plan only**, or explicitly allow low-risk local fixes. A read-only
   supervisor investigates, a builder uses an isolated Git worktree, and an
   independent read-only reviewer checks the actual diff and executes checks.
   Findings or missing evidence block the result from becoming ready for review.

Open the full app from the Dock, menu bar **Open Neko Workspace**, or the
palette's **Open Neko Workspace** command. **Today** is a conversation with
Neko: its brief lists tickets that need you, and asking for work opens a
ticket that is planned read-only and waits for your approval. Its composer
accepts pasted or dropped images and files as local attachments. Return sends
or queues a message, Shift-Return inserts a line, and Command-Return interrupts
the current reply and sends. **Work**
lists everything by needs you, working and done; open one to approve, retry,
or add a note that steers its next plan or build. **Propose parallel subtasks**
on an awaiting plan generates two or three scoped proposals for your approval.
Approved children run in isolated worktrees (three workers globally, two per
workspace), then integrate into the parent task worktree for a final independent
review. Conflicts, cancellation and restart preserve their evidence.
Closing this window does not
stop the daemon. Dismissing the palette hides only the palette.

**Agent profiles** separates identities, instructions and memories. A workspace
has one owning agent; unscoped chat uses the selected default. Explicit one-way
read grants share profile memories only, never workspace notes or tools. Changing
ownership requires finishing/cancelling work and pausing schedules and watches.
Profile edits invalidate existing worker authority. These are application-level
boundaries, not filesystem privacy guarantees.

**Memory** shows saved preferences, workspace notes and ticket decisions, with
edit/delete controls. A separate bounded learning pass after chats and completed
tickets may suggest memories. Suggestions show their source and remain inactive
until you choose **Remember this**; **Dismiss** rejects them across restarts.

**Responsibilities** also manages scheduled plans: save a paused draft with an
hourly-or-slower recurrence and IANA timezone, then enable it explicitly. Due
runs create ordinary approval-gated tickets, with no catch-up burst after sleep.
Codex and Claude imports preserve available instructions as paused drafts;
missing schedule metadata must be filled in rather than guessed. The in-app
connection browser searches the MCP Registry; **Browse skills.sh** opens the
skill catalog. Skill installation previews standalone SKILL.md
content and requires source/audit review before saving and separate activation.

Automatic local preparation requires successful scoped tool receipts no older
than fifteen minutes, unchanged source revision/content, an enabled responsibility
with selected connections, and a bounded low-risk bug assessment with evidence,
files and tests. Sensitive or uncertain work needs a decision. Manual tasks
still require approval. Model assessments and source interpretation are
judgment, not proof. **Ready for review** does not mean tests passed or merged.

Pausing a connection removes its tool access; resuming restores the currently
discovered tools in its workspace.
After a trusted connection is added or signed in, the daemon discovers its
tools in the background and retries transient failures. A read-declared tool
that lists changing work starts one plan-only watch for its workspace. Other
tools stay available for on-demand use; discovery alone does not authorize
remote writes or publication.
The tool-access migration pauses existing responsibilities once. Untried chat
suggestions for readable sources then start automatically, while an existing
manual pause remains in effect.
Pausing a responsibility prevents future wakes and cancels its active watch.
Revocation cannot undo a remote action already sent. Connected MCP tools
can themselves mutate external systems: a server's read-only annotation is
not a security guarantee. Background watches expose and call only tools
declared read-only; trust the server before connecting it. Permission to prepare a local fix does not grant
push, PR creation, messages, or deployment.

The local Codex CLI must be installed and authenticated. `NEKO_CODEX_PATH`
can select an absolute executable path. You can select a model in the
main-window Settings → AI page or the Today composer. The default uses the
signed-in Codex account; Ollama and LM Studio use the Codex CLI's local-provider
adapter and require their local server to be installed and running. If the
optional OpenCodex CLI is installed, Neko reads its live model catalog and
shows enabled models under their provider names. Routed runs use its running
loopback proxy; Neko does not import its credentials or fetch those providers'
models independently. Without it, the Codex default and local-provider choices
remain available. Model selection changes future runs, not work in progress.
Runs ignore global user config/rules
and receive only a temporary Neko bridge, not upstream server credentials.
Shell network access stays disabled; only the two capability-checked bridge
tools are preapproved. Your global Codex configuration is not modified.
**Filesystem read confidentiality between workspaces is not guaranteed** by
the installed Codex sandbox. Local MCP executables run outside that sandbox
and must be trusted as software.

Historical Linear records, tasks and worktrees are preserved. Old polling and
standing grants are disabled; reconnect with a user-added MCP server.
Legacy Paseo/Codex Desktop imports remain opt-in through `NEKO_LEGACY_AGENTS`.
No marketplace, arbitrary swarm, cross-workspace grants, cloud scheduling,
or automatic publication is included. Checks do not run while the Mac sleeps.

OAuth requires usable server discovery and a public client registration
(server registration or a client ID supplied by you). This implementation
does not promise compatibility with every MCP extension or server. A real
user account must be verified separately before calling monitoring active.

`NEKO_DATA_DIR=/absolute/path` isolates development data, sockets and task
worktrees. Never point a test instance at your daily data. Credential-free
end-to-end fixture:

```sh
cargo build -p neko-daemon --bin neko-daemon
node scripts/smoke-workbench.mjs
```

First run no longer asks you to import another app's setup. Choose an existing
local folder with the native folder picker; Neko registers that folder as a
workspace without copying its files. Codex, Claude, Agents and workspace
SKILL.md files are listed by reference to Neko's agents in that workspace.
Agents open a skill only when relevant; a skill never grants tool authority.
Neko-installed skills still require separate activation. Existing MCP
definitions appear in **Tools & skills → Connections** for the selected
workspace. Linking stores a source reference, not source credentials. Neko
re-reads the source before discovery and dispatch; changed or missing definitions
remove access until reviewed and rediscovered. Local executables require
separate trust. The legacy import protocol remains
available for migration fixtures and existing data; it is no longer a first-run
screen. Verify that legacy protocol with:

```sh
cargo build -p neko-daemon --bin neko-daemon
node scripts/smoke-import.mjs
```

This smoke uses a synthetic home and daemon data directory. It proves IPC,
redaction, source/scope grouping, selected apply, disabled/paused defaults, and
idempotent retry; it does not prove OAuth, Keychain acceptance, provider
accounts, or native onboarding capture.

Optional real-model/local-fixture probe (uses your authenticated CLI quota):
`NEKO_SMOKE_LIVE=1 node scripts/smoke-mcp-live.mjs`. It does not authenticate
an external MCP account or activate personal monitoring.


![The neko panel, showing application, System Settings, command and clipboard
results for one query](docs/screenshot.png)

## What it does

One query searches every source at once, each under its own section header:

- **Applications** — every app Spotlight knows about, plus the ones on the
  sealed system volume that Spotlight does not index. Enter launches.
- **Clipboard history** — the last 200 distinct text and link copies, with the
  app they came from. Enter puts one back on the pasteboard.
- **Files and folders** — name-prefix search across `~/Documents`, `~/Desktop`
  and `~/Downloads`. Enter opens.
- **System Settings panes** — "displays", "bluetooth", "sound". Enter opens
  that pane.
- **Neko tasks** — local task status and plans, with approval waits and failures
  highlighted. Enter opens the exact task in the full workspace.
- **Commands** — rows that open a mode inside the panel instead of launching
  something: Clipboard History, Themes, Preferences, and Open Neko Workspace.

⌘K opens the actions menu for the selected row. Escape leaves a mode, then
hides the panel. Seventeen themes ship built in, with live preview as you
arrow through them.

## What it is not

- **Not cross-platform.** It links AppKit directly for window material,
  pasteboard access, hotkeys and icon extraction.
- **No launcher plugin runtime.** Search result types are compiled in; MCP
  tool servers are separate user-trusted processes or services. The search seam is
  the `Provider` trait, not WASM — see
  [docs/adding-a-provider.md](docs/adding-a-provider.md).
- **Not a cloud app.** Search and workbench state live on your Mac. A model run
  or a tool you connect can contact its provider; Neko does not host your work
  in a Neko cloud service.

## Build and run

The native SwiftUI/AppKit client requires macOS 14+, Xcode's Swift toolchain,
and Rust 1.97.1 for the daemon. Build a signed, self-contained bundle without
changing the installed app:

```sh
bash scripts/build-native.sh
bash scripts/install-native.sh --dry-run
```

When ready to install, `bash scripts/install-native.sh` builds and verifies
the native bundle, stops only processes running from the exact installed
`/Applications/Neko.app` paths, and moves the previous app into a recoverable
backup. User data, Keychain credentials, and preferences remain intact. Set
`NEKO_CODESIGN_IDENTITY` to a stable Apple Development identity if desired;
otherwise the build uses ad-hoc signing. Use `--skip-build` to install an
already-built, verified native bundle.

`--clean-data` explicitly moves the Neko data directory into a recoverable
sibling backup; it does not delete credentials or preferences. The installer
prints recovery paths and does not launch the replacement automatically.
Preview builds (`NEKO_NATIVE_PREVIEW=1`) use a separate bundle identity and
are not accepted by this installer. No installation is performed by a dry run.

On first launch, choose a workspace folder and any Mac permissions you want to
grant. The main window works without the global shortcut; no tool connection or
background watch is activated just by opening the app. If you decline
Accessibility, use the Dock or menu bar to reopen Neko.

## How it is put together

The SwiftUI/AppKit app in `native/NekoKit` talks to a resident Rust daemon over
the typed `neko-protocol` wire format. The daemon owns SQLite, search indexes,
MCP connections, scheduling, and task supervision. See
[docs/architecture.md](docs/architecture.md) for the boundaries.

| Document | What it covers |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | Native app, daemon/client split, wire protocol, workbench and MCP authority |
| [docs/adding-a-provider.md](docs/adding-a-provider.md) | Adding a result type through the `Provider` seam |
| `AGENTS.md` | Detailed engineering decisions and constraints |

## Development

```sh
swift test --package-path native/NekoKit
cargo test -p neko-core -p neko-daemon -p neko-protocol
cargo build -p neko-daemon --bin neko-daemon
node scripts/smoke-workbench.mjs
```

The smoke test uses isolated local data and a deterministic child fixture. It
does not prove a live model, an authenticated MCP account, or production
monitoring.

## Licence

Neko's own source is MIT-licensed; see `LICENSE`. Vendored dependencies have
their own notices in `NOTICE`. Binary obligations depend on the target being
built: the native installer does not link the optional GPUI client, whose
dependency chain includes GPL-3.0-or-later code. Review the dependencies and
`NOTICE` for any binary you plan to distribute. This is not legal advice.
