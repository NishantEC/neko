# neko

A native personal-agent workspace and quick command center for macOS, written
in Rust on [GPUI](https://gpui.rs). Launch Neko for the full app; press ⌥Space
for quick search, clipboard history, and task attention.

## Neko-owned work

Create a workspace from any existing folder, then add your own
MCP servers in **Tools & skills → Connections**. No Linear, Slack, or other service is bundled
or required. Multiple connections, including instances of the same server,
have separate workspace scope, credentials, and permissions.

Code tasks need a Git repository because Neko builds in an isolated worktree;
non-Git folders can still be saved as workspaces. During setup, discovered
project folders appear first. Generated and worktree folders stay selectable
under the collapsed **Other folders** section.

1. Add a remote Streamable HTTP URL or an explicitly trusted local executable
   and JSON argument array. Neko does not download or install server packages.
2. Sign in through the browser if the server supports OAuth, or supply optional
   credential JSON in the masked field. Secrets are stored in macOS Keychain.
3. Discover tools, inspect their schemas, and grant only those you trust to run
   unattended. New or changed tool schemas need fresh permission.
4. Add a responsibility, select its connections, and write what to watch.
   Checks run every ten minutes while the daemon is running; failures back off.
5. Keep **Plan only**, or explicitly allow low-risk local fixes. A read-only
   supervisor investigates, a builder uses an isolated Git worktree, and an
   independent read-only reviewer checks the actual diff and executes checks.
   Findings or missing evidence block the result from becoming ready for review.

Open the full app from the Dock, menu bar **Open Neko Workspace**, or the
palette's **Open Neko Workspace** command. **Today** is a conversation with
Neko: its brief lists tickets that need you, and asking for work opens a
ticket that is planned read-only and waits for your approval. **Tickets**
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
missing schedule metadata must be filled in rather than guessed. **Browse** links
the MCP Registry and skills.sh; skill installation previews standalone SKILL.md
content and requires source/audit review before saving and separate activation.

Automatic local preparation requires successful scoped tool receipts no older
than fifteen minutes, unchanged source revision/content, current responsibility
and tool permission, and a bounded low-risk bug assessment with evidence,
files and tests. Sensitive or uncertain work needs a decision. Manual tasks
still require approval. Model assessments and source interpretation are
judgment, not proof. **Ready for review** does not mean tests passed or merged.

Pausing a connection revokes its grants; resuming does not restore them.
Pausing a responsibility prevents future wakes and cancels its active watch.
Revocation cannot undo a remote action already sent. User-granted MCP tools
can themselves mutate external systems: a server's read-only annotation is
not a security guarantee. Permission to prepare a local fix does not grant
push, PR creation, messages, or deployment.

The local Codex CLI must be installed and authenticated. `NEKO_CODEX_PATH`
can select an absolute executable path. Runs ignore global user config/rules
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

First-run Import performs one bounded, read-only local scan for Codex, Claude,
portable Paseo-compatible project metadata, global/workspace skills, MCP
definitions, and documented schedules. The review ledger records source and
scope without secrets or skill bodies. Applying selected items creates
workspaces idempotently, stores skills as disabled review proposals, keeps MCP
definitions disabled with no grants/tools, and saves schedules paused. It does
not silently widen a workspace-scoped MCP: if its workspace is unavailable,
the review can explicitly keep that definition global and paused for later
workspace-specific grants. It still does
not launch local tools, read Keychain credentials, import transcripts, or
enable provider/live authority. Unsupported metadata is shown as a warning or
problem for review. Verify the credential-free path with:

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

## Historical agent browsers (opt-in only)

The following older surfaces are disabled by default. They are retained behind
`NEKO_LEGACY_AGENTS=1` for compatibility, not dependencies of Neko-owned work.

neko drives the agents [Paseo](https://paseo.sh) supervises, over its daemon's
own MCP endpoint, so the things you would switch apps for are a keypress away:

- **An agent blocked on a permission** leads the root list, and the Dock icon
  badges the count even while the panel is hidden — so you find out without
  looking.
- **Agents** lists every session; Enter sends a follow-up prompt, ⌘K changes
  its session mode or cancels the run.
- **Codex tasks** share the agent tile strip, but keep a separate local
  app-server control path. A task opens a compact, read-only activity view;
  when Codex indexes a task without exposing its turns, the view shows its
  real opening request and says that the conversation is unavailable instead
  of pretending the task title is its content.
  An approval presents a readable summary when Codex supplies one, plus
  explicit Approve/Decline actions. A decline uses the same confirmation guard
  as other destructive menu actions.
- **Schedules** shows what runs on a cron and when it next fires. Enter pauses
  or resumes; running one now is behind ⌘K, because Enter is what a finger
  presses on the way past a list.
- **Terminals** shows what is open and what each last printed.
- **Usage** reads your quota straight from each provider's own API — Claude
  Code, Codex and Grok.
- **Ask neko** takes a sentence. It proposes exactly one tool call, shows you
  the call, and runs nothing until you press Enter again.

The Paseo-backed agent-control surface degrades to nothing if Paseo is not
running; none of it is required for the launcher half to work.

Codex is separately optional. neko supervises one local `codex app-server
--stdio` child and renders only its compact in-memory projection. If Codex is
missing, signed out, or stops, retained task tiles say they are unavailable and
approvals are not actionable; applications, files, clipboard history and the
rest of the launcher continue to work. Opening a task can request up to 40
visible activity summaries only when Codex accepts its experimental history
capability. neko never reads Codex session/rollout files.

## What it is not

- **Not cross-platform.** It links AppKit directly for window material,
  pasteboard access, hotkeys and icon extraction.
- **No launcher plugin runtime.** Search result types are compiled in; MCP
  tool servers are separate user-trusted processes or services. The search seam is
  the `Provider` trait, not WASM — see
  [docs/adding-a-provider.md](docs/adding-a-provider.md).
- **Not a cloud app.** Normal search is local; user-connected MCP tools and
  explicitly created/authorized agent tasks use their respective services.
  The following outbound behavior applies only to the opt-in legacy surfaces:
  SQLite and Spotlight's own index, with nothing leaving the machine. Three
  features do make outbound requests, all of them to somewhere you are already
  signed in, and none of them running unless you use it: **Usage** reads quota
  from Anthropic, OpenAI and xAI; **Ask neko** sends your sentence and the
  names of your agents to Anthropic's Messages API to plan a tool call; and
  everything under **Agents** talks to Paseo's daemon on `127.0.0.1`. The
  optional Codex projection talks only to one local child over stdio.
  Credentials are read from the Keychain and from the CLIs' own config files,
  are never persisted by neko, and are passed to `curl` on stdin so they
  cannot appear in `ps` output.
- **Not distributable.** See the licence section below.

## Build and run

Requires macOS and Rust 1.97.1 (`rustc --version`).

```sh
./scripts/setup-gpui-patch.sh   # once, before the first build
cargo build --release
./target/release/neko
```

The first step is not optional. neko depends on a pinned fork of gpui with one
local patch applied on top, and the workspace `[patch]` section points at a
checkout that this script populates at `.gpui-fork-patched/` inside the repo
(gitignored — it is a whole third-party monorepo). Without it `cargo build` fails
with a missing-path error. Run it again whenever the pinned rev changes. The
patch itself fixes a ~30ms warm-summon regression in the fork; see
[docs/architecture.md](docs/architecture.md#the-gpui-dependency).

Run it once more from a clean shell to confirm — it is idempotent and prints
`nothing to do` when the checkout is already current.

The first launch has six setup steps: welcome, optional Mac permissions and
shortcut, selected import, workspace-scoped tools and skills, a responsibility,
and a real read-only first brief. Imported schedules are paused; credentials
are copied only with explicit consent. The workspace remains a normal app
window independently of the shortcut. Setup can be skipped without granting
tools or running a sweep. To replay onboarding:

```sh
NEKO_RESET_ONBOARDING=1 ./target/release/neko
```

If you decline Accessibility, the hotkey cannot register. neko opens its panel
on launch so it stays reachable; after dismissing it, use the menu-bar item to
summon it again.

For development without a global hotkey, `cargo run --release` works. If you
need `⌥Space`, use a stable Apple Development signature instead — ad-hoc
debug signatures change on every rebuild, so macOS treats each one as a new
Accessibility client:

```sh
NEKO_CODESIGN_IDENTITY='Apple Development: Your Name (TEAMID)' scripts/run-dev.sh
```

To replace the installed app with a **fully clean** local install (the bundle,
Neko database/configuration, and Neko-owned Keychain credentials are removed
before replacement), run:

```sh
NEKO_CODESIGN_IDENTITY='Apple Development: Your Name (TEAMID)' scripts/install-clean.sh
```

This intentionally preserves the macOS Accessibility approval for the stable
`dev.neko.launcher` identity, so the configured summon shortcut can keep working.

Find the exact identity on this Mac with `security find-identity -v -p
codesigning`. The script builds `target/debug/Neko.app`; add that app once in
System Settings → Privacy & Security → Accessibility. Subsequent runs retain
the same permission because the signed bundle identifier remains stable. The
latency numbers in `AGENTS.md` are measured against the release binary run
directly.

## How it is put together

Five crates: a pure wire protocol, a resident daemon that owns SQLite and every
index, a thin client SDK, and the GPUI app. Read
[docs/architecture.md](docs/architecture.md) before the code.

| Document | What it covers |
| --- | --- |
| [docs/architecture.md](docs/architecture.md) | Crates, the daemon/client split, the wire protocol, providers, ranking, modes, the window |
| [docs/adding-a-provider.md](docs/adding-a-provider.md) | Adding a result type through the `Provider` seam |
| [docs/plan-agent-control-plane.md](docs/plan-agent-control-plane.md) | How the agent half was surveyed and built, including the two layers that turned out differently |
| [docs/adding-a-theme.md](docs/adding-a-theme.md) | Adding an eighteenth theme |
| `AGENTS.md` | The full engineering record: every decision, what was measured, and what was ruled out |
| `docs/evidence/` | Reports and window-scoped screenshots behind each of those decisions |

`AGENTS.md` is long because it is the project's memory. When a document above
and `AGENTS.md` disagree, `AGENTS.md` is the record and the code is the truth.

## Development

```sh
cargo test --workspace
cargo clippy --all-targets
```

Tests are headless. The GPUI ones run against `TestAppContext`, so no window
opens. Two suites take a process-wide lock because they touch shared OS state:
theme tests (`theme::test_lock`, the active palette is a process global) and
pasteboard tests (`text_field::tests::pasteboard_test_lock`, there is one
systemwide `NSPasteboard`).

`crates/neko/src/evidence.rs` holds env-gated hooks for capturing screenshots
without synthetic input — `NEKO_SHOW_ON_LAUNCH=1`, `NEKO_SHOW_QUERY=<text>`,
`NEKO_SHOW_THEME=<id>`, `NEKO_BENCH=<n>` and others. Every one of them shows
its window **without taking keyboard focus**. That is a deliberate default: an
evidence window that took focus once captured real keystrokes meant for another
app. Activation is a separate opt-in, `NEKO_EVIDENCE_ACTIVATE=1`, and it warns
on stderr when set.

## Licence

Every line of source in this repository is MIT — see `LICENSE`. One vendored
file is Apache-2.0 and attributed in `NOTICE`. Seventeen theme palettes vendor
colour *values* from eight upstream projects, every one MIT and verified from
its own source; the table is in `docs/evidence/themes-report.md`.

**A built binary is a different matter, and it is GPL, not MIT.**
`crates/neko` links the `wingleeio/zed` fork of gpui, which unconditionally
pulls in GPL-3.0-or-later code through `gpui → sum_tree → ztracing`, with no
feature that avoids it. This is not particular to that fork — vanilla Zed's
gpui carries the same chain (`zed-industries/zed#55470`, open).

So, taking the cautious reading:

- **Source**: redistribute freely. MIT is GPL-compatible; nothing conflicts.
- **Binaries**: you may ship them, under **GPL-3.0-or-later** terms — complete
  corresponding source to recipients, same freedoms, no added restrictions.
- **Using it yourself**: nothing attaches. GPL obligations arrive on
  conveyance, not on use.

`NOTICE` states this in full, including the two questions that are genuinely
unsettled rather than answered. None of it is legal advice; if you intend to
ship this commercially, ask a lawyer. The record of why the fork was adopted
with the exposure understood is in `AGENTS.md`, "The GPUI dependency decision".
