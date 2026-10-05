# neko architecture

The map, not the territory. Every section points at the code that owns the
detail. `AGENTS.md` has the full decision record for anything below that looks
arbitrary; several of these decisions cost days to learn.

## Current default: standalone Neko work

`neko-protocol::workbench` supplies typed commands and snapshots. The installed
client is `native/NekoKit` (SwiftUI/AppKit); the earlier GPUI `workspace.rs`
window remains a separate legacy client. Both are clients of the same daemon.
The native bundle prohibits multiple macOS app instances. Its installer stops
the installed client/daemon and any copies running from installer backups,
unregisters the build output and recovery bundles from Launch Services, and registers
`/Applications/Neko.app` as the current launch target. The client keeps checking
daemon health and relaunches its bundled daemon after a connection failure;
the daemon owns socket singleton detection.
`neko-core::workbench` stores bounded,
validated snapshots atomically in the daemon-owned SQLite settings table.
Storage accounting uses actual persisted bytes. Waiting tickets do not reserve
hypothetical maximum-size future output or impose an indirect worker cap.
The current snapshot still has an 8 MiB actual-content limit and 1,000-task
record limit; this is bounded storage, not unlimited archival history. Old events
compact under pressure, while plans/results are retained and an oversized write
fails atomically without replacing saved data.
Workspace identity is a canonical existing folder, including a non-Git folder.
Creating or planning a code task separately validates that the folder belongs
to a Git repository, because execution requires an isolated worktree. Import
review places dated Codex output, Neko smoke fixtures and linked Git worktrees
under an optional **Other folders** disclosure; classification does not remove
their selection capability.
Before creating a task worktree, repository discovery compares source paths in
the title and goal with bounded, cancellable `git ls-files` reads from checkouts
under the workspace folders. Path suffixes accommodate omitted monorepo prefixes
and stack line/column annotations. Bare filenames, untracked files and traversal
paths do not identify a checkout. A unique file match precedes name/model routing;
matching clones with an identical origin may reuse the single checkout already
recorded for other tasks in this workspace. Different origins or multiple known
copies remain ambiguous. The host saves the root and file evidence on the ticket;
this discovery does not run repository code or change approval/sandbox policy.
`neko-daemon::workbench` claims every eligible ticket on each tick, without a
fixed global or per-workspace worker cap. An active claim prevents duplicate
workers for one ticket; dependency, approval and scope checks still apply.
Checkout creation serializes only Git metadata setup per shared repository
(including linked worktrees), with cancellable waiting. Model execution remains
concurrent. Restarted split parents wait until every child has passed review.
Model/network calls run outside the database mutex. A two-second supervisor heartbeat
is separate from task progress; it does not claim the model made progress.

The native main window (`native/NekoKit/Sources/NekoNative/NekoApp.swift`) uses a
system source-list sidebar: Home and All agents, then the five newest Needs you,
Working and Recent agents in the selected workspace, with Show all for each group.
Home is the main Neko conversation. Ticket entry points route to full-page agent
chats; their drafts, originating page and board filter persist in `AppModel`.
All agents retains the List/Board switch and task move commands. Agent headers
show status and approval/stop actions. An unanswered question or a no-work decision
cannot be approved into a build. Accept locally records human acceptance of an
isolated result; it does not apply changes to the project or publish them.
Page titles, subtitles and primary actions use the native window toolbar, keeping
its top row useful and mouse-accessible with the sidebar expanded or collapsed.
Page bodies and side panels respect the native top safe area. Hiding the toolbar
background does not remove its mouse-hit region; content must not draw controls
underneath it.
The native workspace uses a shared light/dark surface ramp and bounded page
widths: 880pt for management and 760pt for conversations, with 32pt outer insets.
Appearance follows macOS by default and can be set to Light or Dark in Settings
or the sidebar. The floating composer uses Liquid Glass on macOS 26, with a
material fallback on older supported systems; content cards remain opaque.
The content canvas is solid neutral charcoal in dark mode, with no decorative
gradient wash. Native sidebar and control materials retain their own appearance.
Design lab is a separate sidebar page for a local, interactive composer study.
It offers Inline (selected direction) and Separate model/effort/speed layouts with
real Liquid Glass, the Metal pearl, native popovers and the existing AppKit editor.
Both use the production selector components against an explicitly illustrative
catalog; model/speed names there do not establish live runtime availability.
The observation model has no AppModel/IPC/provider dependency. Send appends only
to the sample conversation and Apply sample changes only the local preview.
Plugin switches do not install or grant anything. Task membership stays with the
conversation, derived from distinct child ticket IDs in approved sample splits.
One task, Split task and Review demonstrate task states, not runtime actions.
Home and agent composers use the same selector with real daemon catalog/settings.
Pearl lighting follows pointer events and has no idle render timer; Reduce Motion
holds the light/scale still, and transparency/contrast preferences select a solid
fallback. The shader applies only to beads, not the system glass composer.
Native segmented
pickers own focus and keyboard selection. Management headers expose their
actions separately to accessibility; long source/check details use explicit
disclosures. Active watches appear before suggestions. Settings and editors
use grouped native forms. See `docs/design/native-polish.md`.
Home and ticket chats share a native `NSTextView` composer and attachment strip.
`ComposerDraftStore` keeps text, durable attachment references and revisions per
workspace/profile or ticket ID in `AppModel`; navigation preserves them, app quit
does not. Successful sends clear only the submitted revision. The two adapters
retain different commands: Home sends/queues/cancels chat, while ticket replies
use `ReplyToTask` and can redirect active work. Return sends content and never
activates Stop; Shift Return inserts a line. Complete serialized payloads are
checked against the daemon's 4 KiB Home and 2 KiB ticket limits.
Draft stores use scoped Observation rather than publishing each keystroke through
the app model. Composer sizing uses a separate TextKit measurement stack; only
actual scroll-view layout sets the live editor's width. Background snapshots
publish only when displayed content changes, ignoring the unused heartbeat field;
IPC success/failure still updates connection health.
Settled agent chats show their saved result and independent reviewer readback
openly, with prior conversation, plans and decisions folded into history. Saved
results during retries are labelled historical; display text does not determine
approval or verification status. Rich tables wrap, and long terminal/diff previews
have explicit line counts and incremental expansion of available output. These
presentation changes are separate from runtime context compaction.
Saved command evidence is directly expandable beneath the reviewer readback and
inside work history/activity. It shows recorded exit codes and saved output, with
explicit warnings for shortened output or incomplete legacy receipts. Copy saved
record copies exactly the retained event, not an unavailable full execution log.
These records can span multiple attempts; they are not the current review verdict.
`neko-core::neko_chat` stores the conversation in its own bounded setting,
outside the task store's content limit. Each message is one Codex turn with
read-only filesystem access (`neko-daemon::workbench::converse`). A selected
workspace receives its connected MCP tools through a temporary lease;
an unscoped chat receives no bridge. Chat history injected into the prompt is
also scoped: exact workspace/profile filtering precedes the newest-12 window.
A 12 KiB budget includes role labels and omission notices. Whole recent messages
are preferred; an oversized newest message retains a UTF-8-safe tail with a notice.
This is bounded history selection, without a generated summary or provider context
compaction. Task command receipts bound fields before JSON serialization rather
than cutting serialized JSON. Missing or shortened command identity cannot prove a
successful check; shortened output is an explicitly labelled preview of the host
execution receipt. A reply may propose up to three
tickets, which become ordinary queued tasks, so planning and approval gates are
unchanged. Ticket notes (event role `note`) reach planner and builder prompts
as direction inside the approved scope and never grant tools or publication.
`ReplyToTask` also interprets unread human messages in order in an empty scratch
directory with tool-free `native_runner::extract`. Only one human message reaches each
strict Work/ReadOnly/Context classifier, never issue text or repository content.
Work sets the existing local start permission; ReadOnly (also the failure default)
persists a restriction overriding global autostart and standing preparation.
Context preserves that restriction until a new work request or explicit Start/
Approve supersedes it. Durable reply revisions and handled markers order these
actions, survive restart and prevent reinterpreting old replies. A new reply queues
the ticket and cancels the previous worker; run leases and result commits reject
its stale revision. An ordered, bounded unread queue prevents a rapid clarification
from erasing an uninterpreted no-change instruction. Only human work requests or
explicit Start/Approve set local start authority; a running status alone never does.
That authority lasts through building and is removed when the result is ready.
Approved split identities and child scopes survive follow-up replies.
Previously written changes remain in the task worktree.
Direct work uses the scout to plan investigation and fixes, including reproduction
and regression tests, instead of the unattended supervisor's missing-evidence
hold. Only genuine unavailable access or user-owned decisions need a question.

Chat tool cards persist the exact arguments and workspace. A tool whose server
declares `readOnlyHint=true` can run without another prompt; the declaration participates
in the schema hash. Unknown/action
tools need an inline approval for each exact call. Denial never dispatches.
Approvals expire after 120 seconds; upstream execution remains bounded to 20
seconds. Stop, runner timeout, lease drop, or daemon restart fail closed.
Successful and failed dispatched calls have inline receipts. These guarantees
depend on the explicitly trusted server's effect declaration; Neko cannot prove
that arbitrary remote code is read-only, or undo an action already dispatched.

The daemon-owned `neko-core::mcp_host` hosts user-added and source-linked stdio
and Streamable HTTP servers through pinned rmcp 3.4.1. A linked connection
stores its source identity, reviewed transport snapshot, and fingerprint, not
source credentials. The daemon resolves the current source definition and
credentials before discovery and dispatch, and removes access when the
definition or trusted local executable changes or disappears. Connections,
discovered schema hashes, workspace-scoped tool records, responsibilities, receipts and
source evidence live in the versioned workbench snapshot. Neko-owned
credentials live in per-connection Keychain entries; source credentials remain
in their original setup. OAuth uses discovered metadata, PKCE/state/issuer checks and bounded
browser callbacks. Legacy Linear data remains historical; old polling and
permissions are disabled, not converted into grants.

The native client can search the public MCP Registry for hosted HTTPS
Streamable HTTP endpoints and offers Sentry's official hosted endpoint as a
featured entry. Registry metadata stays client-side until the user reviews a
server URL and adds it through the existing `AddConnection` command. Browser
sign-in remains user-driven. The daemon automatically discovers tools on new,
trusted connections and after sign-in, retrying failures without blocking the
client. The responsibility scheduler then evaluates eligibility for a
plan-only source watch. Package listings are not installed automatically.

`native_runner` starts Neko-owned CLI processes in task worktrees. Settings → AI
sets the global default; each Home conversation (profile + workspace) and each
ticket has independent model, effort and speed pins in `conversation_runtime`.
The daemon's `agent_catalog` reads provider-native metadata, including Codex
app-server effort descriptions, defaults and service tiers. Unknown capability
metadata is distinct from an explicitly unsupported control. Fast aliases are
normalized to the adapter's wire ID; no tier is invented from another account.

Before a Home reply or task worker, `runtime_selection` asks a bounded tool-free
supervisor to choose among connected, advertised candidates. Manual pins win.
A failed or malformed selection uses a validated connected default with those
pins. No viable pinned candidate stops explicitly rather than silently changing
it. The selector has no quota feed and does not claim quota-aware failover. A legacy
OpenCodex setting absent from discovery requires an explicit connected-model
selection; it is never silently reinterpreted as a Codex provider pin.
Automatic paid acceleration requires a saved conversation preference; a manual
speed pin is an explicit choice. Settings are checked with one minimal inference
before manual pins are saved; failure preserves the prior preferences.

The resolved request and reason are persisted before execution in bounded
`runtime_selections` history (200 dispatches). A worker captures that request
through planning/building/review recovery; edits apply to the next worker or
Home turn. Execution passes model, effort and service tier to fresh and resumed
Codex processes using per-process configuration. If an older catalog cannot
resolve a default effort for resume, the worker explicitly starts fresh from
saved ticket history instead of retaining an earlier effort pin. This is requested runtime
configuration, not confirmation of the tier actually served. Other adapters keep
model selection but reject effort/speed overrides until their mapping is supported.
Settings never change execution, tool, workspace or publication authority.
Worker concurrency is independent of account usage limits; another session does
not reset those limits.
A scoped, expiring capability in environment variables connects a temporary
stdio bridge back to the daemon. Only `neko_list_tools` and `neko_call_tool`
are enabled and preapproved. Upstream configurations and credentials are not
passed to workers. Global user config/rules are ignored. Shell networking stays
disabled for restricted Codex runs; full-access task phases can use it for setup. Current workspace scope, connection state and schema are checked before
each call, including after upstream rediscovery. Revocation prevents subsequent
dispatch; cancellation cannot undo a request already delivered upstream.

The scheduler seeds one plan-only watch for each trusted, scoped source with a
granted read-declared tool that can enumerate changing work, or activates its
untried chat suggestion. Search-only reference catalogs remain on demand. A durable
scope marker prevents a later manual pause from being undone. Background MCP
leases hide and reject tools not declared read-only; this is a trusted-server
declaration, not proof that an upstream server is harmless. Responsibilities
persist their next due time before running. A ten-minute
local wake invokes a read-only agent with selected tools, with bounded failure
backoff and no replay of missed intervals. Generic observations require real
successful scoped receipts. Source identity/revision deduplicates tasks;
failed retrieval never implies deletion. `mcp_host::responsibility` combines
fresh source evidence with `supervision`'s bounded model assessment before
automatic local preparation. Sensitive or uncertain work holds for approval.
An instruction/connection change invalidates prior source eligibility.

Explicit approval, a direct human work request, or qualifying standing authority
allows a builder. Authorized builders, independent reviewers and recovery
supervisors use full local filesystem access, including dependency setup,
caches, temporary directories and shell networking. Codex applies its full-access
flag to both new and resumed sessions. Explicit read-only requests, planning,
chat, background source checks and tool-free extraction retain restricted runs.
A reviewer must return a structured passing verdict, no findings, complete
host-observed changed-file coverage and successful command receipts. The host
compares HEAD, index, Git control metadata, tracked contents (including existing
dirty files) and nonignored untracked content before and after review. Source
mutations or an unavailable snapshot reject the review; ignored test output is
allowed. These observations detect changes, not prevent them or establish
filesystem isolation for full-access processes.

A fresh recovery supervisor investigates worker errors, rejected reviews and
questions raised during directly authorized planning. It may direct concrete
next steps to the same saved ticket session or ask one necessary user-owned
question, never accept a failed review or grant authority. Setup can improve
without a code diff, so unchanged patches alone do not stop recovery. Three
supervised retries per worker bound repeated failure. The original diff base,
captured authority, cancellation, reply revisions, grants, budgets and dependency
scope remain in force. Recovery supervisors may prepare ignored dependencies
but their source changes also fail the host snapshot check. Split parents never
use the builder repair loop. A pending supervisor question blocks automatic readmission and approval until
a reply produces a new plan, preserving the existing approved scope.
Builders receive the original checkout as a reference for local dependency
setup and must preserve unrelated files and shared dependencies.
`decomposition` validates bounded two/three-subtask proposals. Explicit approval
creates ordinary child tickets, sharing the same scheduler limits. Dependencies
receive earlier verified patches in separate scopes. The daemon integrates
child patches into the isolated parent and independently verifies that result;
conflicts preserve every worktree and block completion. No original-checkout
merge or publication occurs. See `docs/evidence/parallel-verifier.md` for proof
and historical verification limits; `docs/evidence/full-access-supervision.md` records the current execution model.
Cancellation terminates the process group; interrupted tasks fail on restart
with worktrees retained. Model risk and source interpretation are judgments,
not proof. Full-access task processes have no filesystem isolation between workspaces. Local MCP executables are explicitly trusted host software,
not sandboxed plugins. No cross-workspace context is deliberately injected.

The full workspace is a regular Dock/Cmd-Tab window. The quick panel remains
resident, and `order_out` hides only that window. Native task rows deep-open
the workspace. Historical Codex Desktop/Paseo providers and their polling are
disabled unless `NEKO_LEGACY_AGENTS` is set. The historical sections below
describe those compatibility providers, not the default execution path.

IPC frames are capped at 16 MiB and the socket is owner-only (0600). Secret
request fields redact Debug output and never appear in returned snapshots.
`NEKO_DATA_DIR` optionally selects an absolute isolated development data path.

## Five crates, and the boundary they exist to enforce

```
crates/
  neko-protocol/  wire types + framing. Depends on serde and nothing else.
  neko-core/      daemon-owned logic: SQLite, indexes, ranking, launching, providers
  neko-daemon/    a thin binary hosting neko-core behind a Unix socket
  neko-client/    SDK: a persistent, auto-reconnecting connection to the daemon
  neko/           the GPUI app: window, panel, hotkey capture, onboarding, themes
```

The rule the split exists to enforce: **`neko` depends on `neko-client` and
`neko-protocol` only, never on `neko-core`.** The app physically cannot reach
past the daemon boundary to open SQLite or read the app index itself. Check it
with `grep neko-core crates/neko/Cargo.toml` — no hit.

## Two resident processes

```
   ⌥Space
     │
     ▼
┌─────────────────────┐   length-prefixed JSON   ┌──────────────────────────┐
│ neko (client)       │  ◄────────────────────►  │ neko-daemon              │
│  GPUI window        │   ~/Library/Application   │  SQLite (single writer)  │
│  panel + text field │   Support/neko/neko.sock  │  app index (Spotlight)   │
│  live OS hotkey     │                           │  clipboard capture loop  │
│  themes, onboarding │                           │  icon extraction thread  │
└─────────────────────┘                           │  providers + local Codex │
                                                  └──────────────────────────┘
```

Both stay resident. The window is opened once at startup and only ever hidden
and re-shown, never closed — that is what makes a warm summon a few
milliseconds rather than a window creation. The daemon survives the client
being killed; a fresh client opens a new socket connection to the same live
daemon with no handshake.

`crates/neko/src/daemon_launcher.rs` spawns `neko-daemon` unconditionally at
client startup. A redundant spawn is a no-op: the daemon's `bind_singleton`
(`crates/neko-daemon/src/server.rs`) pings the existing socket and exits
quietly, or takes over a stale socket file if nothing answers.

`socket_path()` and `database_path()` (`crates/neko-protocol/src/lib.rs`) are
fixed per-user paths, not per-checkout. Two builds of neko on one machine talk
to whichever daemon started first.

### Why the daemon owns SQLite

One writer, no locking protocol, no migration races. The database holds the
clipboard history, settings (hotkey combo, onboarding flags, theme id) and app
launch counts. Multiple clients can connect at once — the daemon serializes
every write behind its own `Mutex<Db>`. `crates/neko-core/src/db.rs`.

### Why the hotkey is registered in the client

The daemon owns and persists the *setting*
(`crates/neko-core/src/hotkey.rs`). The live OS registration happens in the
client (`crates/neko/src/hotkey_client.rs`), for two reasons. `global-hotkey`
needs a live run loop, and the client already has GPUI's foreground executor
polling for events — routing a keypress through IPC would add a socket
round-trip to the exact path warm summon is measured on. And only a live
registration attempt can prove a third-party app is not already holding the
combo, which only the client can make.

`HotkeyController::rebind` registers the candidate *before* unregistering the
old combo, so a failed rebind never loses the working hotkey.

## The wire protocol

`crates/neko-protocol/src/lib.rs`, ~450 lines, no logic. A frame is a u32-LE
byte length followed by a JSON payload (`write_frame`/`read_frame`). Local IPC
only, so JSON's debuggability beats the bytes a binary codec would save.

```rust
enum Frame {
    Request  { id: u64, request:  Request },
    Response { id: u64, response: Response },
    Event(Event),          // server-initiated, not correlated to any request
}
```

Two properties worth knowing before reading either side:

**Responses can arrive out of order.** The daemon spawns a thread per
*request*, not per connection, so a 1.5s file search cannot queue every
following keystroke behind it. `neko-client` correlates by request id in a
`pending` map, never by arrival order.

**One request can be answered more than once.** `Response::SearchResults`
carries `complete: bool`, and `Response::ends_request()` is the single place
that rule is written down. See "Two-phase search" below.

Only three request variants matter for extending the app, and all three are
generic over provider:

| Request | Meaning |
| --- | --- |
| `Search { query, limit, provider }` | `provider: None` is the merged root list; `Some(id)` scopes to one provider — this is the mode seam |
| `Activate { kind, id, action, query }` | `kind` routes to the provider; `action: None` is its primary action, `Some(id)` a secondary one; `query` is the search field's contents at the moment Enter was pressed, for the rows whose action takes an argument |

`SearchItem` carries everything a row needs to render itself, and four of its
fields exist because a surface needed something the row shape could not say:
`meter` (a `0..=1` reading plus labelled stats — a usage window, a disk, a
download), `keeps_open` (confirming this row is a *step*, so the panel stays
and re-searches instead of getting out of the way), `preview` (many lines for
a detail pane — a terminal's captured screen), and `enters_mode`. Each is
additive with a serde default, and none of them is a provider-identity switch:
`panel.rs` reads the field, never the `kind`.

`Event::AttentionChanged { count }` is the one server-initiated message that
is not about data a client asked for — it is how a hidden panel's Dock badge
learns an agent is blocked.
| `GetTheme` | There is deliberately no `SetTheme` — committing a theme is `Activate { kind: "theme", .. }` |

## Providers

`crates/neko-core/src/provider.rs`. Object-safe by construction, so the daemon
holds a plain `Vec<Box<dyn Provider>>`.

```rust
pub trait Provider: Send + Sync {
    fn id(&self) -> &'static str;             // its usual SearchItem::kind and Activate route
    fn section_label(&self) -> &'static str;  // "Applications", "Clipboard", …
    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate>;
    fn activate(&self, id: &str) -> Result<(), ProviderError>;

    fn defers_for(&self, query: &str) -> bool { false }        // slow for this query?
    fn search_cancellable(&self, …, cancel: &Cancel) -> …      // defaults to search()
    fn activate_with_query(&self, id, query) -> …              // defaults to activate()
    fn perform_action(&self, id, action_id) -> …               // defaults to an error
}
```

Eleven are registered in the root list, in `AppState::with_test_providers`
(`crates/neko-daemon/src/server.rs`):

| id | Source of truth | File |
| --- | --- | --- |
| `app` | `mdfind`, plus a plain scan of the sealed system volume | `crates/neko-core/src/apps.rs` |
| `file` | `mdfind`, prefix match only, bounded and cancellable | `crates/neko-core/src/files.rs` |
| `clipboard` | SQLite, filled by a polling capture loop | `crates/neko-core/src/clipboard.rs` |
| `settings` | `/System/Library/ExtensionKit/Extensions/*.appex`, scanned once | `crates/neko-core/src/settings.rs` |
| `command` | A compiled-in table | `crates/neko-core/src/commands.rs` |
| `theme` | `neko_protocol::BUILTIN_THEMES` | `crates/neko-core/src/themes.rs` |
| `preference` | neko's own settings, in the same SQLite KV table | `crates/neko-core/src/preferences.rs` |
| `codex-tasks` | warmed projection from the supervised local Codex app-server | `crates/neko-core/src/codex.rs` |
| `codex-approval` | pending approvals from that same projection | `crates/neko-core/src/codex.rs` |
| `agent` | Paseo's own agent documents on disk | `crates/neko-core/src/agents.rs` |
| `permission` | Paseo's daemon over MCP — agents blocked waiting for you | `crates/neko-core/src/permissions.rs` |

Ten more are **mode-only** (reachable by a scoped search or an activation,
never by a root-list query): `folder-scope` (`preferences.rs`), `new-agent`
(`new_agent.rs`), `usage` (`usage.rs`), `schedule` (`schedules.rs`), `ask`
(`ask.rs`), `terminal` (`terminals.rs`), `agent-control` (`agents.rs`),
`conversation` (`conversation.rs`), `codex-task` and `new-codex-task`
(`codex.rs`).

**The panel knows nothing about any of them.** `crates/neko/src/panel.rs` has
no `match` on provider identity anywhere. Everything a row needs to render —
section header, action verb, icon, badge, subtitle, secondary actions, whether
confirming it enters a mode — is data the provider set on the `SearchItem`.
That is what makes adding another one cheap; see
[adding-a-provider.md](adding-a-provider.md).

Most rows use their producing provider's id as `SearchItem::kind`. The Codex
task-discovery provider is the intentional exception: its registered id is
`codex-tasks`, but its tile rows are kind `codex-task` so their mode/detail
route reaches the scoped provider that owns those actions. The panel still
receives only ordinary row data.

Two source-of-truth notes that repeatedly surprise people:

- Spotlight does not index the sealed system volume, which is why `apps.rs`
  unions `mdfind` with a plain directory scan. Finder needs a named allowlist
  on top of that, because it lives loose in `/System/Library/CoreServices`
  beside ~112 background agents that no static plist signal separates it from.
- File search matches names that **start with** the query, not names that
  contain it. A leading wildcard forces Spotlight off its index: measured
  ~13 seconds for one character, versus well under a second as a prefix.

## Ranking: `search::allocate`

`crates/neko-core/src/search.rs`. Every provider returns
`Candidate { score, item }` and owns its own matching. The one cross-provider
contract is that a higher score is better, on roughly the scale `fuzzy_score`
produces — a small dependency-free subsequence scorer with contiguous-run and
word-boundary bonuses.

`allocate` merges them into one bounded response in three passes:

```
1. Reserve   every provider with any candidate takes one slot, before
             anything else is allocated. Including the first-registered
             one — a "primary provider wins ties" assumption is itself a
             hard-coded provider identity, and it silently produced an
             empty Applications section once.

2. Interleave  spend the rest of `limit` one slot at a time on the highest
               remaining candidate anywhere. Clipboard alone has a cap
               (`clipboard_max_slots`, half the limit) so one long paste
               cannot take nine of ten rows.

3. Order     stable-sort the sections by their own top candidate's score.
             Registration order is only the tie-break. Clipboard's recency
             boost is discounted for this comparison: freshness ranks rows
             within Clipboard, it should not decide which section leads.
```

Two per-provider bonuses exist and are deliberately gated:
`app_category_score` and `settings_category_score` both require a real prefix
match before applying. An ungated version promoted coincidental matches
("Xcode" for "code") over relevant files. Constants in that file were tuned
against a real corpus, not a fixture.

The client does the same reservation again in **pixels**:
`panel::fit_within_budget` splits results into contiguous same-`kind` runs,
reserves a header-plus-one-row for every section after the first, and rolls
unused budget forward. A response with room for a section is necessary but not
sufficient for a screen with room to draw it.

## Two-phase search

`files::FileProvider` shells out to `mdfind`, measured 55–990ms and bounded at
1.5s. The other five answer in microseconds. One combined response meant
nothing rendered until the slowest finished.

The daemon now partitions providers by `defers_for(query)` and answers twice:
the fast group immediately with `complete: false`, then the whole set with
`complete: true`. A query nobody defers for is still exactly one frame.
Keystroke to first render went from a median of 108.50ms to 0.36ms
(`docs/evidence/instant-search-report.md`).

Two consequences in the client:

- **Superseded searches are abandoned, not ignored.** The next `Search` on a
  connection cancels the previous one (`server::supersede_previous_search`),
  and `files.rs` kills the `mdfind` child rather than letting it finish into a
  discarded result. An abandoned query that keeps running competes with the one
  you actually want.
- **Late results append.** `panel::merge_late_results` keeps what is already on
  screen in its exact order and appends only what the deferred provider added.
  If making room would cost the selected row, the late section is not shown at
  all until the next keystroke — someone who has arrowed to row seven is about
  to press Enter.

## Commands and modes

A **command** is a provider row with `SearchItem::enters_mode: Some(id)`.
`panel::Root::confirm` checks that field first, and if it is set the row never
reaches `Request::Activate` at all — the client transitions into a mode
instead.

A **mode** is client-side UI state. `crates/neko/src/modes.rs` holds the pure
part (`ModeChrome`: id, scoped provider id, footer title, placeholder, whether
it has a detail pane, and what its body is); `panel.rs` drives the I/O. While a
mode is active, every keystroke searches with `Request::Search { provider:
Some(id) }`. Escape or the back arrow exits and restores the query you had
typed before entering, verbatim.

**Modes do not nest**, and `panel::Root::active_mode` is a single `Option`.

Eleven exist: Clipboard History and Terminals (both with a detail pane, list
column 264pt), Themes (no detail pane — the preview *is* the panel, so a second
column would take 496pt away from the thing being previewed), Conversation,
Codex Tasks, New Agent, New Codex task, Agents, Schedules, Usage, and Ask neko.

**Three modes treat the query as a payload rather than a filter**: New Agent,
where what is typed is the task and the rows are directories to start it in;
New Codex task, where it is the first task turn for the selected project path;
and Agents, where it is the prompt to send to the selected session. Filtering
in any of them would shrink the list as you described the task and move the row
out from under the selection mid-sentence. That
needed no new mode machinery — a mode has always been "one provider's own list,
scoped by `Request::Search { provider }`", and a provider may ignore the query
when ranking. It did need one additive wire field, `Request::Activate`'s
`query`, because the row's action takes what was typed as an argument, and the
id must stay stable across keystrokes or the highlight (`resolve_selection`,
keyed on `(kind, id)`) resets and Enter starts the agent in the wrong
repository.

New Codex task has the same payload shape: the rows are only project paths
already visible in the local Codex projection (plus the optional current
project seam), while the query is the proposed task. The actor validates that
the chosen path still exists and still belongs to those rows, then sends the
explicit path to Codex. It never inherits the daemon's directory and never
creates a workspace or worktree.

**Preferences is a window, not a mode** — `crates/neko/src/preferences/`, split
`state.rs` (pure) / `view.rs` (GPUI and I/O) the same way onboarding is. The
`Preferences` command still carries `enters_mode` on the wire, because from the
provider's side "this row changes the UI rather than performing a daemon
action" is one statement; `panel::Root::confirm` is where a window and a mode
part company. It is opened through an injected closure
(`panel::PreferencesOpener`) which also hides the panel — GPUI's test platform
panics on both `open_window` and `App::hide`, so nothing may call them
unconditionally from the panel.

The window owns no settings. It reads and writes them through the same scoped
`Request::Search` and `Request::Activate` any surface would use, against
`neko_core::preferences`. Its `folder-scope` list lives in
`AppState::mode_providers` rather than the root-list providers: reachable by a
scoped search or an activation, never by a root-list query. That is the seam
for any list that only means something inside its own surface.

The `⌘K` actions menu is populated from the selected row's
`SearchItem::actions`, which already arrived with the last search response — no
round-trip to open it. A destructive action needs a second Enter to run;
moving the selection disarms it.

## The agent control plane

neko is a launcher and an agent control plane. `docs/plan-agent-control-plane.md`
is the plan it was built to; this is the shape that came out.

**Everything that *acts* goes through `neko_core::mcp`.** Paseo's daemon
exposes its whole agent surface at `POST /mcp/agents` as Model Context
Protocol over HTTP — 61 tools — and that one client is the only way anything
here reaches it. The endpoint comes from `~/.paseo/paseo.pid`'s `listen`
field, parsed as a socket address so a stale pid file fails discovery rather
than aiming a request somewhere unexpected. The client is stateless: it
re-initializes per call and holds nothing, because the daemon restarts
constantly during development and a client caching a dead session is silently
wrong until something notices.

**Reads and writes deliberately use different sources.** Listing agents comes
off disk; acting on one goes over MCP. Seeing your agents should not stop
working because a daemon restarted.

| surface | what it does | file |
| --- | --- | --- |
| **Needs you** | Agents blocked on a permission. Leads the root list, Enter approves, `⌘K` denies. | `permissions.rs` |
| Agents (root) | What is running, as tiles | `agents.rs` |
| Agents (mode) | Enter sends a follow-up prompt; `⌘K` sets the session mode | `agents.rs` |
| New Agent | Start one in a known project | `new_agent.rs` |
| Schedules | Paseo's cron. Enter pauses — never runs | `schedules.rs` |
| Terminals | What is open and what it last said | `terminals.rs` |
| Usage | Quota, read from each vendor's own API | `usage.rs` |
| **Ask neko** | A sentence becomes one tool call you confirm | `ask.rs` |

**The permission inbox is the reason the rest exists.** An agent that hits
something it may not do stops and waits, and the only way to notice used to be
switching to Paseo. `neko-daemon` polls for blocked agents every five seconds,
which both keeps the panel's answer warm (so those rows land in its *first*
frame) and drives `Event::AttentionChanged` → a **Dock tile badge**, the one
ambient surface neko has. Notifications need a bundle identifier and neko is a
bare Mach-O; gpui's menu-bar API is dead code.

**Ask neko proposes; it never acts alone.** Planning and running are separate
keystrokes with the exact call rendered between them — which is what
`SearchItem::keeps_open` exists for — and the model may only choose from a
fixed catalog of eight verbs, so nothing becomes possible through it that was
not already possible by hand.

## Codex quick attention loop

`neko-daemon/src/codex.rs` supervises exactly one `codex app-server --stdio`
child. It owns the JSON-RPC conversation, including startup's initial task-list
request, notifications, task start, and approval responses. Providers in
`neko_core::codex` do not speak to a process: palette search takes a short read
lock on the warmed `Snapshot` and turns that data into tiles or rows. That
keeps a keystroke local and prevents a second child, a transport round trip, or
session-file parsing from entering the search path.

The bootstrap handshake requests the app-server's experimental capability.
`thread/turns/list` is used only after it was accepted, only for the task a
person explicitly opened, and only for at most 40 visible summaries. The
snapshot retains at most one selected task's activity; it is a compact,
read-only quick view, not a transcript cache.

Codex approval notifications become **Needs you** rows with `Approve`/`Decline`
actions and a reason or other readable detail when the app-server supplies one.
The exact request id and current thread are checked again at the actor edge
before a response is written, so a stale row cannot decide a newer request.
Decline is marked destructive and therefore requires the panel's second,
explicit confirmation. Permission-profile grants use the app-server's
permission response shape; command and file decisions use their own response
shape.

The child is optional. A missing executable, a signed-out app-server, failed
bootstrap, EOF, or control write marks the snapshot unavailable and retries
with bounded backoff. Existing task rows remain visibly unavailable rather
than being passed off as current; approval actions return an inline unavailable
error. This state has no bearing on the daemon's SQLite/index providers or on
the launcher window.

This is intentionally not a persistent Neko workspace and not a third-party
work inbox. Those are separate parent-product decisions. The delivered surface
only reflects local Codex tasks and explicit approvals through Codex's own
local app-server contract. An indexed task may expose only its title and
opening request; in that case Neko shows that request and explicitly marks the
conversation unavailable rather than rendering a fabricated transcript. See
`docs/evidence/codex-quick-attention-loop-report.md`.

## Agents on disk

`neko_core::agents` reads Paseo's own on-disk agent documents
(`~/.paseo/agents/<workspace>/<id>.json`) — no subprocess, no network, no MCP.
Running agents answer an empty root query (the one provider where that is the
right answer); idle ones need a query and are off by default. Closed and
Paseo-internal agents are never shown.

`Backend` is an enum with one variant. A second agent source costs a variant, a
read function, and one match arm — it does not touch the provider, the wire
protocol, or the client.

Configured in Preferences → Agents: the source (shown with a live census), a
master toggle, and whether idle agents match.

**Starting one** is `neko_core::new_agent` (the `New Agent` command and mode).
Rows are the projects Paseo already knows about
(`~/.paseo/projects/projects.json`), so the working directory is chosen rather
than inferred; the tool is the one the most recent real agent used in that
directory, because `paseo run` requires an explicit `--provider` and there is
no default to inherit. Enter shells out to the `paseo` CLI, waits for it to
confirm, and reports any failure inline in the panel's footer. The child's
environment is stripped of `PASEO_AGENT_ID`/`PASEO_AGENT_CWD`/
`PASEO_WORKSPACE_ID`: with those inherited, the CLI resolves the *caller's*
workspace and `--cwd` silently loses.

## Motion

`crates/neko/src/motion.rs`. Two one-shot fade specs, and **one shared clock**.

`PulseClock` is the only sanctioned way to drive a repeating animation. It
ticks at 12.5Hz rather than frame rate, stops entirely when nothing is using
it, and never starts under reduce-motion. It is driven from `render`, because
only the render pass knows whether the thing being animated is actually on
screen. Nothing else may call `.repeat()`.

## Themes

`crates/neko/src/theme.rs`. Colour is a swappable table; geometry is not.

- Geometry, spacing and type stay `pub const`. A theme cannot move a row or
  resize the panel.
- Colour lives on `Palette`, 22 `Rgba` fields. Every paint site reads
  `theme::active()`, which is one relaxed atomic load and a slice index into
  `&'static [Theme]`. No lock, no allocation, no `Arc` on the render path.
- A theme supplies *independent* colours as a `Spec`; one `const fn build()`
  derives every dependent token. A theme cannot redefine what a token means.

Seventeen ship. Light ones also move the real `NSWindow`'s `NSAppearance`,
because the native material behind the panel renders in it — a cream panel over
a `darkAqua` blur reads as a dark halo. See
[adding-a-theme.md](adding-a-theme.md).

## Icons

`crates/neko/src/assets.rs`. Two unrelated things are both called "icon" here:

- **Application icons** are real per-app rasters, extracted from
  `NSWorkspace::iconForFile` by the daemon and cached as 128px PNGs. They
  arrive on the wire as `Icon::Image(path)` and render through `img()`.
- **Everything else** — clipboard entries, files, folders, commands, agents,
  preferences — has no per-item artwork and renders a `Glyph`: a small, closed
  vocabulary of marks the client knows how to draw.

Glyphs used to be hand-composed stacks of `div()`s. They are now **vendored
Lucide SVGs rendered through `gpui::svg()`**, which needed no dependency —
only an `AssetSource` and some files. `assets::NekoAssets` is that source: a
compile-time `include_bytes!` table installed once on the `Application`
builder in `main.rs` (`with_assets`, which must happen before `run`, because
it is also what rebuilds gpui's `SvgRenderer` around the source).

Three consequences of how gpui renders an SVG, all of which shape the code:

- **It renders an alpha mask, tinted by the element's `text_color`.** Colour
  still comes from `theme::active()` at every call site, so a theme change
  re-tints every icon with no per-theme asset. It also means **an icon can only
  be one colour** — which is why `Glyph::Palette`, four swatches in the live
  theme's own colours, stays hand-painted, and why `assets::glyph_icon` returns
  `None` for it. `Glyph::Agent`/`AgentLive` keep a painted presence dot
  composited over the SVG for the same reason.
- **A filled icon would render as a solid blob.** Lucide's set is stroke-only;
  a test asserts every vendored file is `fill="none"` on a 24×24 grid.
- **An unresolvable path is not an error.** `paint_svg` draws nothing and
  returns `Ok`. So a typo'd asset path is a silent hole in a row — which is why
  `assets.rs`'s tests resolve every path, rasterise every file through gpui's
  own `SvgRenderer`, and assert the result actually has coverage.

Adding an icon: drop the file in `crates/neko/assets/icons/lucide/`, add one
row to `ICONS` and one constant to `assets::icon`, and pin its byte length in
the test. Licence paperwork is `NOTICE`, `THIRD_PARTY_LICENSES/lucide-ISC.txt`,
and the table in `crates/neko/src/components/vendor/MANIFEST.md`. **Lucide is
ISC, not MIT** — permissive either way, but the distinction is recorded because
this repo's own notes got it wrong first.

## The window, and why it looks the way it does

`crates/neko/src/main.rs` opens one `WindowKind::PopUp` window and configures
it in a fixed order. Each of these lines is load-bearing.

**Fixed 760pt wide, never resized at runtime.** Entering a mode does not resize
the `NSWindow`. gpui's own private `viewport_size` — the size it lays out and
paints the root element against — stops resyncing after a window has been shown
and hidden a few times, and no public API forces it. Four workarounds were
tried and all failed. So the window is created at
`theme::PANEL_WIDTH_WITH_DETAIL_PX` for the process's whole lifetime and the
panel fills it. `docs/evidence/mode-resize-seam-fix-report.md`.

**No shadow at all is drawn by neko, and AppKit's is disabled.**
`material::disable_native_shadow` turns off the automatic window shadow, and
the panel `div` carries no `.shadow_lg()`. Both were invisible for most of the
project's life because the window was exactly the panel's width; once it was
not, both spilled into the margin as a black halo. Two overlapping sources, two
separate fixes, one symptom reported three times.

**`NSTitledWindowMask` is cleared, and the rendering view is re-made first
responder in the same call.** gpui creates this window titled no matter what
`WindowOptions.titlebar` says, and AppKit draws its own ~1pt highlight on the
top edge of a titled window. Clearing bit 0 removes it —
`material::clear_titled_style_mask`. But `setStyleMask:` makes AppKit rebuild
the frame view **and resets the first responder with it**, and gpui only calls
`makeFirstResponder:` once at window creation. Miss the re-assert and keyboard
input dies silently while every property still reads back correct. Anything
that mutates this window's style mask must do the same, and must be verified by
actually typing — `Window::dispatch_keystroke` enters below AppKit's responder
chain and reports success on a build a real keypress cannot reach.
`docs/evidence/titled-window-first-responder-fix-report.md`.

**Native material, not gpui's `Blurred`.** `crates/neko/src/material.rs`
installs `NSGlassEffectView` where the OS has it, falling back to
`NSVisualEffectView(.popover)`, then to plain opaque. The invariant that makes
this work: background views go into `window.contentView()`, the real
`NSWindow`'s root view — **never** as a subview of gpui's rendering view, which
would silently eat every pixel gpui draws. Every install is read back and
verified rather than trusted (`material::verify_installed`).

**Multi-display.** The window opens once, so `upper_third()` can only place the
first summon. `display_placement::reposition_to_cursor_display` moves it to the
`NSScreen` under the cursor before each later summon, reading
`NSEvent.mouseLocation()` — permission-free, unlike asking which window is
active, which needs the same Accessibility grant as the hotkey.

**It can also be dragged, and neko drives the gesture itself.** Grab the input
row or the footer. The obvious implementation — `start_window_move()` →
`performWindowDragWithEvent:` — shipped first and was replaced, because it runs
**AppKit's own modal event loop** until the mouse comes up: there is no moment
inside it to measure proximity to a snap target, choose one, or draw a guide.
So `is_movable` is back to `false` (that flag only governs the API no longer
used) and three pieces do the work:

- `crates/neko/src/snap.rs` — **pure, no gpui.** Visible frame + panel size +
  home + desired origin → snapped origin and guides. All the correctness, and
  all the tests, live here. One coordinate space throughout, AppKit's own
  (points, y-up, bottom-left origins), so the drag never converts anything.
- `crates/neko/src/window_drag.rs` — the native half: cursor, window move, and
  the guide overlay. Injected into the panel as `Rc<dyn PanelDrag>`, because
  gpui's test platform panics on `window_handle()` and `open_window`.
- `panel::Root` — four verbs and one bool.

Targets are the visible frame's edges and centres plus **home**, the position a
summon puts the panel at, computed through the same `upper_third_offset` that
places every summon so the guide cannot drift from it. Within
`snap::SNAP_THRESHOLD_PT` (16pt) a guide appears; the one that will actually
take the panel is drawn strongly and any other in reach is muted. The desired
origin is hard-clamped into the visible frame first, so a drag cannot leave the
panel somewhere it can no longer be picked up; Escape puts it back.

Guides are a second, transparent, click-through, never-key `PopUp` window
ordered below the panel — an element cannot paint outside its own window, and
every guide is at a screen edge. It is opened lazily (a plain click, or a drag
that never nears a target, opens no window) and torn down on every path that
ends a drag. `docs/evidence/drag-snap-guides-report.md`, including the one thing
that could not be checked here: **no real drag was ever performed**, because
synthesising mouse input is forbidden in this repo.

**Spaces.** gpui sets `CanJoinAllSpaces | FullScreenAuxiliary` for any
`WindowKind::PopUp`. `crates/neko/src/spaces.rs` reads the bits back at every
launch rather than assuming.

## The gpui dependency

`crates/neko` depends on the `wingleeio/zed` fork of gpui, pinned by rev, with
one local patch applied on top by `scripts/setup-gpui-patch.sh`.

- **Why the fork.** It has `paint_backdrop_blur`, `EdgeFade`, a real mac
  implementation of `start_window_move()`, and a fix for a
  `windowDidBecomeKey:` self-deadlock that reliably hung the published crate
  after one or two real activations.
- **Why the patch.** The fork throttles any non-key window to ~30fps with no
  exemption for a pending `on_next_frame` callback — something a caller is
  explicitly waiting on. Warm summon regressed to ~26–40ms. No public API opts
  out, so the fix is a dependency patch, not a neko-side change. After it: mean
  8.56ms. `docs/evidence/gpui-inactive-window-throttle-fix-report.md`.
- **What it costs.** The fork pulls GPL-3.0-or-later code in unconditionally.
  See the README's licence section.

## Where to look next

| Question | File |
| --- | --- |
| How a row is rendered | `crates/neko/src/panel.rs` |
| Where an icon comes from | `crates/neko/src/assets.rs` |
| How the text field edits, selects, and pastes | `crates/neko/src/text_field.rs` |
| How the daemon dispatches a request | `crates/neko-daemon/src/server.rs`, `handle_request` |
| What the three-step first run does | `crates/neko/src/onboarding/` (`setup.rs` owns welcome, Mac and workspace selection; `view.rs` owns native lifecycle and shortcut I/O) |
| Why any of the above is the way it is | `AGENTS.md`, and the report it points at in `docs/evidence/` |
# Workspace skills

`neko-core::skills` discovers global and workspace SKILL.md files afresh for
each run. Chat and ticket roles receive a bounded, workspace-first catalog of
paths and descriptions; the agent reads a relevant local skill in place, without
copying or enabling it in onboarding. Neko-installed skill proposals remain a
separate opt-in: their full instructions are injected only after explicit
workspace activation and a content-hash check. A skill never grants tools.
Codex's own automatic skill instructions remain disabled for these runs, and
this catalog does not provide filesystem read privacy. See
`docs/evidence/skills-phase3.md` for installation limits and earlier tested
boundaries.

## Agent profiles and scheduled planning

`neko-protocol::agent_profiles` defines user-owned identities, workspace
assignments and directional global-memory read grants. `neko-core::agent_profiles`
validates/persists configuration through the workbench snapshot and builds bounded
profile context. Default migration preserves legacy Neko ownership. Memory and
conversation entries retain their original profile identity. Workspace tools
stay behind the same explicit grants; shared memory never carries capabilities.
Profile revisions participate in worker authority and capability issuance so
stale workers cannot gain permissions from later configuration. Assignment is
blocked by unfinished work, pending chat and enabled watches/schedules.

`scheduled_plans` stores recurrence drafts in the same workbench snapshot as
tasks. Claiming an occurrence and adding its ticket is one persisted operation;
restart does not duplicate it. Final COUNT/UNTIL occurrences queue once and
disable future runs. `schedule_time` bounds traversal, supports timezones/DST,
and rejects invalid or unbounded evaluation. Imports resolve filesystem paths
outside the shared database mutex and match against fresh state inside it.
See `docs/evidence/agent-profiles.md` and `docs/evidence/schedules-and-import.md`.

## Follow-up memory learning

`neko-core::memory_learning` persists a bounded queue and immutable proposals;
the daemon runs one independent follow-up worker for chat/ticket memory and
completed-ticket skills. Extraction receives bounded evidence in an empty scratch
directory, read-only with no bridge or shell tools. Native actors suppress
ambient CLI tool paths so Neko's own scoped bridge and worker limits remain the
authority. Source fingerprints, profile/workspace fences and retry-attempt tokens
reject stale output. The Memory page accepts or dismisses proposals explicitly;
accepting the exact stored proposal and resolving it share one transaction.
Ticket decisions have a separate bounded allowance from user-created memories.
See `docs/evidence/memory-learning.md` for limits and verification boundaries.


## Contextual decision records and working style

`neko-protocol::decision_context` defines bounded decision episodes and contextual
working preferences. `neko-core::decision_context` persists them in a separate
versioned setting on the existing single-writer database. Snapshot overlays expose
records and preferences to the native client; public commands allow scoped
correction and preference save/keep/dismiss/forget, never arbitrary host evidence
or outcome injection. Corrections append a revision without rewriting the original
observation. With learning enabled, a correction proposes guidance for that exact
ticket. Changed guidance returns to proposed until explicitly kept.

Daemon planning, verification and failure commits capture host observations in the
same transaction as task state. Decisions record the runtime and relevant confirmed
preference versions from the actual prompt snapshot. Confirmed guidance reaches
scoped chat, responsibility and task prompts only when memory use is enabled and
complete context words or ticket identifiers match, subject to exceptions. Existing
RunAuthority, receipt freshness, responsibility and sandbox rules still gate work.
Guidance never creates capabilities. Cancellation records an unknown reason;
expected effect and observed host status remain separate. Useful interaction
outcomes remain unknown until observed, and local acceptance makes no application
or publication claim. Home and agent chats show real records; Memory / Working
style offers contextual controls. No passive cross-app capture is added.
