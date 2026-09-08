# Adding a provider

A provider is one result type in the search list. The count intentionally
changes as neko grows; adding another is one `impl Provider` and one
registration line, and this page is the walkthrough.

The cost is not a claim. Several providers have been added since the trait
existed, including local Codex task/approval rows, and two earlier tasks wrote
down exactly what they touched:
`docs/evidence/settings-provider-report.md` ("The seam held") and
`docs/evidence/themes-report.md`. Both landed inside the accounting below.

Read [architecture.md](architecture.md) first if you have not.

## What you write

### 1. The provider

A new module in `crates/neko-core/src/`, plus a `pub mod` line in
`crates/neko-core/src/lib.rs`.

```rust
use neko_protocol::{Glyph, Icon, SearchItem};
use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

pub struct BookmarksProvider { /* your own index */ }

impl Provider for BookmarksProvider {
    fn id(&self) -> &'static str { "bookmark" }
    fn section_label(&self) -> &'static str { "Bookmarks" }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        self.entries
            .iter()
            .filter_map(|e| fuzzy_score(query, &e.title).map(|score| Candidate {
                score,
                item: SearchItem {
                    id: e.url.clone(),          // what activate() gets back
                    kind: "bookmark".into(),    // primary activation route
                    title: e.title.clone(),
                    subtitle: Some(e.host.clone()),
                    icon: Icon::Glyph(Glyph::Link),
                    section_label: "Bookmarks".into(),
                    action_label: "Open  ↵".into(),
                    // Everything below is "nothing to say here".
                    badge: None,
                    accessory: None,
                    enters_mode: None,
                    group_label: None,
                    actions: Vec::new(),
                    source: None,
                    meter: None,
                    keeps_open: false,
                    preview: None,
                    preview_markdown: false,
                    speaker: None,
                    images: Vec::new(),
                },
            }))
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        crate::launch::open_url(id).map_err(|e| ProviderError(e.to_string()))
    }
}
```

`SearchItem` does not derive `Default`, so every field is written out. See
`crates/neko-protocol/src/lib.rs` for what each one means; the optional fields
set to nothing above all read as "this provider has nothing to
say here".

### 2. The registration

One line in `AppState::with_test_providers`
(`crates/neko-daemon/src/server.rs`):

```rust
Box::new(neko_core::bookmarks::BookmarksProvider::new()),
```

Order matters only as a tie-break. `search::allocate` sorts sections by their
own top candidate's score; registration order decides ties. `ThemesProvider` is
registered last on purpose for exactly that reason.

That is the whole required surface.

## What you do not touch

Confirmed across four provider additions, not assumed:

| File | Why not |
| --- | --- |
| `crates/neko-protocol/src/lib.rs` | No new `Request`/`Response` variant. `Activate { kind, id }` routes by string at runtime. `SearchItem::kind` is a `String`, not a closed enum, so the wire layer never learns your provider exists. |
| `crates/neko/src/panel.rs` | No `match` on provider identity anywhere. Section header, icon, action verb, badge and secondary actions are all data you set on the `SearchItem`. |
| `crates/neko-core/src/search.rs` | `allocate`'s reservation and interleave passes are provider-count-agnostic. |
| `crates/neko-client/src/lib.rs` | Nothing there knows about result types. |

The one exception is a genuinely new *visual*. `Icon::Glyph` is a closed enum —
a bounded vocabulary of shapes the client knows how to paint, not a dispatch on
who produced the row. If none of `Text`, `Link`, `File`, `Folder`, `Clipboard`,
`Palette` fits, add one variant to `Glyph` and one arm to
`panel::glyph_element`. The standard path is a vendored SVG in
`crates/neko/assets/icons/` wired through `assets.rs`; `Glyph::Palette` is the
deliberate painted exception because its four live-theme swatches cannot be a
one-colour SVG mask.

For a normal provider, `SearchItem::kind` is the provider's `id()` and routes
primary activation back to it. A row that enters a mode may deliberately name
the mode provider instead: Codex task discovery has id `codex-tasks`, while its
rows use `codex-task` so their detail and secondary action route to that scoped
provider. This is still data, not panel-side knowledge of Codex.

If your rows have real per-item artwork, use `Icon::Image(path)` pointing at a
cached PNG and extract it in a background pass, the way
`crates/neko-core/src/icons.rs` does for apps. Never extract on the search
path — real AppKit icon extraction is tens of milliseconds each.

## The optional methods

All defaulted. Implement one only when you have a real reason.

**`defers_for(&self, query) -> bool`** — return `true` when answering *this
query* will be slow enough that the other providers should not wait. The daemon
then delivers your candidates in a second frame. Two providers override it:
`files::FileProvider` returns `false` below its minimum query length, and
`permissions::PermissionsProvider` gates on whether the daemon is reachable
*and* whether its cache is already warm. Both are the same rule — answering
"nothing, instantly" in two frames is strictly worse than one, and a warm
cache has no round trip to keep off the fast path.

**`search_cancellable(&self, query, now, cancel)`** — implement when your
search does interruptible I/O. The daemon always calls this, never `search`
directly; the default delegates. The point is not to ignore a superseded
result, it is to **abandon** it: `files.rs` kills the `mdfind` child, because a
query nobody wants any more still competes for CPU with the one they do.

**`activate_with_query(&self, id, query)`** — implement when your row's action
takes *what was typed* as an argument, rather than being a thing to open. The
daemon always calls this, never `activate` directly; the default drops the
query and delegates, so a provider that does not care implements nothing.
Four override it: `new_agent::NewAgentProvider` (its `id` is the working
directory and the query is the prompt), `agents::AgentControlProvider` (the
query is a follow-up prompt for an existing session), and `ask::AskProvider`
(the query is the request being planned), and
`codex::CodexStartTaskProvider` (the selected local project is the id and the
query is the task). Note the rule that forces
this — keep `id` **stable across keystrokes**, because `panel::resolve_selection`
follows the highlight by `(kind, id)`; an id that folds in the query resets the
selection on every character typed.

**`perform_action(&self, id, action_id)`** — implement when you populate
`SearchItem::actions` with secondary `⌘K` menu entries. Mark an action
`destructive: true` and the client requires a second Enter before running it.

Several providers do, and what belongs on Enter versus in the menu is the decision
worth thinking about rather than the code:

- `schedules` puts **Run now** in the menu and *pause* on Enter, because Enter
  is what a finger presses on the way past a list and running a schedule
  starts a real agent doing real work.
- `terminals` puts **Kill** in the menu and *open the folder* on Enter, for the
  same reason.
- `permissions` puts **Deny** in the menu but does **not** mark it destructive:
  denying is a normal answer, not a mis-key to guard against, and arming it
  behind a second Enter would make the safer reply the slower one.
- `codex-approval` puts **Approve** and **Decline** in the menu. Its decline
  is destructive because it rejects a current, explicit external request, so
  the panel asks for a second confirmation before the actor can send it.

Whatever you choose, `SearchItem::action_label` has to name it — it renders on
the selected row, and for a row whose Enter is not obvious it is the only thing
that says what will happen. `schedules` sets it per row (`"Pause ↵"` /
`"Resume ↵"`) because a label naming the wrong direction is the one thing a
person cannot recover from misreading.

**`answers_empty_root_query(&self) -> bool`** — return `false` when your whole
list only means something once somebody has asked for it (themes, preferences).
Return `true` only for something that is genuinely *news*: `agents` (what is
running), `codex-tasks` (the local Codex projection), and permission inboxes
(what is blocked waiting for you) earn it, and all are self-limiting — a
machine has a handful, not hundreds.
It gates the *root list with nothing typed* only; a scoped search still gets
your full list, which is how the Themes mode and the Preferences window load
theirs.

**`enters_mode`** on a `SearchItem` — not a trait method, but the same idea.
Set it and confirming the row transitions the panel into a client-side mode
instead of calling `activate` at all. See below.

## Scoring

Return `Candidate { score, item }` in any order — `allocate` sorts each
provider's own list. The only cross-provider contract is that higher is better,
on roughly the scale `search::fuzzy_score` produces. Every built-in provider's
score is ultimately built from it, which is why no normalization step exists.

Two things to know before you invent your own scale:

- **Length matters more than you expect.** `fuzzy_score`'s length penalty is
  sized for title-length strings. `ClipboardProvider` had to add
  `clipboard_length_normalization` because a match found once inside a
  several-hundred-character paste scored like a strong short-title match, and
  won nine of ten rows on a real query.
- **A category bonus must be gated.** `app_category_score` gives app rows a
  flat bonus, but only when the query is a real prefix of the title or of a
  significant word in it. An ungated version promoted coincidental matches
  ("Xcode" for "code") above genuinely relevant files. If your provider needs
  one, copy that shape, and tune it against a real corpus — the first constants
  tried in both existing bonuses were wrong, and only live queries caught it.

`fuzzy_score` is a strict in-order subsequence match over the whole string,
including spaces. "keyboard shortcuts" does not match a pane called "Keyboard".
If your titles are shorter than the phrases people type for them, carry alias
strings and score each one, taking the best — that is what
`commands.rs` does, and it is the right place to solve it. Do not change
`fuzzy_score`; it is shared by every provider.

## If your provider should also be a mode

A mode is a filtered, full-panel view of one provider, entered from a command
row. Clipboard History and Themes both work this way.

1. Add a `CommandSpec` to `COMMANDS` in `crates/neko-core/src/commands.rs`,
   with `mode: "bookmark"` and any alias phrases people would actually type.
2. Add a `ModeChrome` entry to `MODES` in `crates/neko/src/modes.rs`:
   the mode id, the scoped provider id, the footer title, the placeholder, and
   `has_detail`.

Nothing else. `Request::Search`'s `provider` field and the enter/exit logic in
`panel.rs` branch on "is a mode active", never on which one.

`has_detail: true` is the one real extra cost. There is no generic detail-pane
renderer, and building one speculatively would be the wrong abstraction — a
detail pane's content is inherently mode-specific. You would write your own,
next to `render_mode_detail` in `panel.rs`. `has_detail: false` costs nothing
beyond the two steps above; the Themes mode chose it deliberately, because the
live preview *is* the panel.

## If your provider talks to Paseo

Everything that acts on an agent, schedule or terminal goes through
`neko_core::mcp` — one MCP-over-HTTP client against the local daemon. Do not
add a second way to reach it.

Four rules that came out of the existing integrations:

- **Give it a `disabled()` constructor.** `AppState::new` delegates to
  `with_test_providers`, which the whole daemon suite goes through, so an
  always-live provider puts a real network round trip inside every hermetic
  test. Same shape as `FileProvider::empty()`.
- **Rediscover the daemon per call, do not hold it.** Paseo restarts
  constantly; a provider that resolved once stays silently dead for the rest
  of neko's process lifetime.
- **Never cache a failure.** An error yields an empty result, and caching that
  makes a daemon restart look like a feature disappearing for as long as the
  TTL runs.
- **Read the tool's schema before designing around it.** `list_terminals`
  takes `all: true`; the plan for that mode assumed a fan-out over 22
  workspaces, which would have been forty-four subprocesses per keystroke.
  `send_agent_prompt` defaults to *waiting for the agent to finish* for a
  top-level caller like neko, which would hold a request thread for minutes.
Both were in the schema.

## If your provider supervises a local tool

Codex is the model: the provider is a pure projection over a warmed local
snapshot, while one daemon-owned actor owns exactly one child process and its
protocol. `codex-tasks` and `codex-approval` search that snapshot under a
short read lock; they never start a child, read session files, or make a
transport request from a palette keystroke.

Keep control equally narrow. The Codex actor rechecks a visible task or exact
approval request before writing, and an unavailable child returns an inline
error rather than a guessed success. Capability-gated data belongs behind the
explicit activation that needs it: Codex history is requested only when a
person opens one task and only after the app-server accepted its experimental
capability. Do not turn a quick view into a background transcript cache.

Missing, stopped, or signed-out local tools are normal. Keep the rest of the
launcher usable, retain only clearly labelled stale rows if that is useful,
and make their actions honestly unavailable. A provider must never make a
local integration required for application, file, clipboard, or settings
search to work.

## Testing it

Providers are plain Rust with no GPUI dependency, so unit-test `search` and
`activate` directly. Follow the existing pattern for anything that touches the
outside world: give your provider a hermetic constructor for tests, the way
`FileProvider::empty()` and `SettingsProvider` do, so
`AppState::with_test_providers` can build a daemon with no real I/O in it.

Daemon-level tests go against `handle_request` directly
(`crates/neko-daemon/src/server.rs`, `mod tests`). Client-level ones use
`#[gpui::test]` with `TestAppContext` — headless, no window.

Do not launch the real `neko-daemon` to try something out. Its clipboard
capture loop polls the systemwide pasteboard regardless of `HOME`. Use
`crates/neko-daemon/src/bin/verify_harness.rs`, which hosts the real `server`
module and every real provider without starting that loop.
