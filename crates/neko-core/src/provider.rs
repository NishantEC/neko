//! The provider seam every result type in the search list is built on:
//! application search, clipboard history, file search, and — per the plan
//! (`data/dim/plan.md`) — agent capability later, "just another result type
//! in the same fast list." This supersedes the standalone `AgentProvider`
//! trait a previous task added as a placeholder (`id`/`query` only, no
//! score, no activation, nothing registered anywhere): once a second real
//! provider (clipboard) and a third (file search) existed to design
//! against, the shape that actually falls out needs a normalized score (for
//! cross-provider ranking — see `search::allocate`) and a way to *perform*
//! a result's action, not just produce it. Keeping two overlapping
//! provider-shaped traits around — this one and the old speculative one —
//! would leave a future agent-capability task with a choice to make about
//! which seam to build against, which defeats the point of having a seam at
//! all. A future `AgentProvider` implementation is just another `impl
//! Provider`, exactly like `apps::AppsProvider`, `clipboard::
//! ClipboardProvider`, and `files::FileProvider` below.

use crate::cancel::Cancel;
use crate::search::Candidate;

#[derive(Debug)]
pub struct ProviderError(pub String);

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ProviderError {}

/// Object-safe by construction (no generics, no `Self` return types) so the
/// daemon holds a plain `Vec<Box<dyn Provider>>` — registering a new result
/// type is exactly one line in `AppState::new` (`neko-daemon/src/server.rs`)
/// plus the `impl Provider` itself. Nothing else in the daemon, the wire
/// protocol, or the client needs to change; see `AGENTS.md`'s "Provider
/// abstraction" section for the full "what would a fourth provider touch"
/// accounting.
pub trait Provider: Send + Sync {
    /// A stable identifier: what every `SearchItem` this provider produces
    /// sets its own `kind` to, and what `Request::Activate` uses to route
    /// back here. Chosen once, lowercase, never surfaced to a person
    /// directly (the section header text is `section_label` below, not
    /// this).
    fn id(&self) -> &'static str;

    /// The line drawn above this provider's own contiguous run of results
    /// in the panel, e.g. "Applications", "Clipboard", "Files".
    fn section_label(&self) -> &'static str;

    /// Search this provider's own index/store and return scored
    /// candidates, in any order — `search::allocate` sorts each provider's
    /// own list before merging. A provider owns its own matching and
    /// scoring entirely; the one cross-provider contract is that a higher
    /// score means a better match, on roughly the scale `search::
    /// fuzzy_score` produces (every built-in provider's score is ultimately
    /// built from it) — see `search::allocate`'s doc comment for what
    /// "comparable enough" buys the ranking step downstream, and for what a
    /// provider scoring on a wildly different scale would need to do
    /// instead.
    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate>;

    /// Whether this provider's answer for `query` is expected to be slow
    /// enough that the daemon should **not** make every other provider's
    /// results wait behind it — see `neko-daemon`'s `handle_request` and
    /// `AGENTS.md`'s "Two-phase search" section. A provider that returns
    /// `true` here has its candidates delivered in a *second*
    /// `Response::SearchResults` frame (`complete: true`) after the fast
    /// providers' own results have already gone out (`complete: false`).
    ///
    /// Defaulted `false`, which is the honest answer for every provider
    /// that answers from memory or SQLite — apps, clipboard, settings,
    /// commands all finish in microseconds, and splitting them across two
    /// frames would cost a wire round-trip and a second client render for
    /// no gain. `files::FileProvider` is the one override today: it shells
    /// out to `mdfind`, whose *measured* time-to-first-result on a real,
    /// repository-heavy home directory ranges from under 100ms to over
    /// 1.2s (see that module's own doc comment).
    ///
    /// Takes the query because "slow" is query-dependent, not a fixed
    /// property of the provider: `FileProvider` returns nothing at all,
    /// instantly, below its own minimum query length, and answering that
    /// case in two frames would be strictly worse than one.
    fn defers_for(&self, _query: &str) -> bool {
        false
    }

    /// [`Provider::search`], but abandonable: `cancel` is set the moment
    /// the daemon learns this client no longer wants this query's answer
    /// (the next keystroke arrived). A provider doing real, interruptible
    /// I/O should poll it and return early — killing whatever child
    /// process or connection it started, not merely discarding the result
    /// — see `crate::cancel`'s module doc comment for why abandoning
    /// matters rather than just ignoring.
    ///
    /// **The daemon always calls this, never `search` directly.** Defaulted
    /// to ignore the token and delegate, so a provider with nothing
    /// interruptible to abandon (everything except `files::FileProvider`
    /// today) implements exactly one method, exactly as before this
    /// existed — the "registering a new provider is one `impl` plus one
    /// line" accounting in `AGENTS.md` is unchanged.
    fn search_cancellable(&self, query: &str, now_unix_ms: i64, _cancel: &Cancel) -> Vec<Candidate> {
        self.search(query, now_unix_ms)
    }

    /// Perform this provider's one primary action for a `SearchItem::id` it
    /// previously returned — launch an app, write the pasteboard, open a
    /// file. Called from `Request::Activate` when `action` is `None`.
    fn activate(&self, id: &str) -> Result<(), ProviderError>;

    /// [`Provider::activate`], plus the search field's own contents at the
    /// moment Enter was pressed (`Request::Activate`'s `query`).
    ///
    /// **The daemon always calls this, never `activate` directly** — the
    /// same arrangement `search_cancellable` has with `search`, and for the
    /// same reason: defaulted to drop the extra argument and delegate, so a
    /// provider whose rows are things to *open* implements exactly one
    /// method, exactly as before this existed.
    ///
    /// Overriding it is for the case where a row is a thing to do *with
    /// what was typed* rather than a thing to open — `new_agent::
    /// NewAgentProvider` is the first: its rows are working directories and
    /// the query is the prompt the agent gets. `Request::Activate`'s own
    /// doc comment records the two alternatives (prompt-in-the-id, and a
    /// provider that remembers its last query) that were tried before this
    /// and why both are silently wrong.
    fn activate_with_query(&self, id: &str, _query: &str) -> Result<(), ProviderError> {
        self.activate(id)
    }

    /// Perform a named *secondary* action from a row's own
    /// `SearchItem::actions` (e.g. clipboard's "copy"/"delete" alongside its
    /// default "paste") — called from `Request::Activate` when `action` is
    /// `Some(action_id)`. Defaulted so the three providers with nothing
    /// beyond their one primary action (apps, files, settings) don't need
    /// to implement this at all; only a provider that actually populates
    /// `SearchItem::actions` (clipboard) overrides it.
    /// Whether an **empty root-list query** should reach this provider.
    ///
    /// `search("")` cannot answer this on its own, because the same call
    /// serves two different questions: a root-list search with nothing typed
    /// yet, and a surface that deliberately scopes to one provider and wants
    /// its whole list (the `Themes` mode; the Preferences window loading its
    /// values). Those want opposite answers, so the distinction belongs to
    /// the provider rather than to the query string.
    ///
    /// `true` by default, which is right for anything whose full list is a
    /// useful empty state — apps offer the top apps, agents show what is
    /// running. Override to `false` when the list is only meaningful once
    /// someone has actually asked for it, which includes anything private
    /// enough that showing it unprompted is itself the problem (clipboard).
    fn answers_empty_root_query(&self) -> bool {
        true
    }

    fn perform_action(&self, _id: &str, action_id: &str) -> Result<(), ProviderError> {
        Err(ProviderError(format!("provider '{}' has no action '{action_id}'", self.id())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// A provider that implements only what a provider must, and records
    /// which method the trait's own defaults ended up calling.
    #[derive(Default)]
    struct MinimalProvider {
        activated: Mutex<Vec<String>>,
    }

    impl Provider for MinimalProvider {
        fn id(&self) -> &'static str {
            "minimal"
        }
        fn section_label(&self) -> &'static str {
            "Minimal"
        }
        fn search(&self, _query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
            Vec::new()
        }
        fn activate(&self, id: &str) -> Result<(), ProviderError> {
            self.activated.lock().unwrap().push(id.to_string());
            Ok(())
        }
    }

    #[test]
    fn a_provider_that_ignores_the_query_still_gets_its_activate_called() {
        // The daemon only ever calls `activate_with_query`, so this default
        // is what keeps "a new provider is one `impl` plus one line" true —
        // if it stopped delegating, every provider that never asked for a
        // query would silently stop activating at all.
        let provider = MinimalProvider::default();
        provider.activate_with_query("/Applications/Safari.app", "saf").unwrap();
        assert_eq!(provider.activated.lock().unwrap().as_slice(), ["/Applications/Safari.app"]);
    }
}
