# Adding a provider

A provider is one result type in the search list. Six exist. Adding a seventh
is one `impl Provider` and one registration line, and this page is the
walkthrough.

The cost is not a claim. Four providers have been added since the trait
existed, and two of those tasks wrote down exactly what they touched:
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
                    kind: "bookmark".into(),    // must equal id()
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
`crates/neko-protocol/src/lib.rs` for what each one means; the six set to
nothing above are all optional and all read as "this provider has nothing to
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
`panel::glyph_element`. That is a data addition and a paint function. There is
no SVG asset pipeline in this codebase; glyphs are hand-painted from `div`s.

If your rows have real per-item artwork, use `Icon::Image(path)` pointing at a
cached PNG and extract it in a background pass, the way
`crates/neko-core/src/icons.rs` does for apps. Never extract on the search
path — real AppKit icon extraction is tens of milliseconds each.

## The optional methods

All defaulted. Implement one only when you have a real reason.

**`defers_for(&self, query) -> bool`** — return `true` when answering *this
query* will be slow enough that the other providers should not wait. The daemon
then delivers your candidates in a second frame. `files::FileProvider` is the
only override; it returns `false` below its minimum query length, because
answering "nothing, instantly" in two frames is strictly worse than one.

**`search_cancellable(&self, query, now, cancel)`** — implement when your
search does interruptible I/O. The daemon always calls this, never `search`
directly; the default delegates. The point is not to ignore a superseded
result, it is to **abandon** it: `files.rs` kills the `mdfind` child, because a
query nobody wants any more still competes for CPU with the one they do.

**`activate_with_query(&self, id, query)`** — implement when your row's action
takes *what was typed* as an argument, rather than being a thing to open. The
daemon always calls this, never `activate` directly; the default drops the
query and delegates, so a provider that does not care implements nothing.
`new_agent::NewAgentProvider` is the only override: its `id` is the working
directory and the query is the prompt the agent gets. Note the rule that forces
this — keep `id` **stable across keystrokes**, because `panel::resolve_selection`
follows the highlight by `(kind, id)`; an id that folds in the query resets the
selection on every character typed.

**`perform_action(&self, id, action_id)`** — implement when you populate
`SearchItem::actions` with secondary `⌘K` menu entries. Only
`ClipboardProvider` does. Mark an action `destructive: true` and the client
requires a second Enter before running it.

**`answers_empty_root_query(&self) -> bool`** — return `false` when your whole
list only means something once somebody has asked for it (themes, preferences).
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
