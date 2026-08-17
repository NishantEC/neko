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

    /// Perform this provider's one action for a `SearchItem::id` it
    /// previously returned — launch an app, write the pasteboard, open a
    /// file. Called from `Request::Activate`.
    fn activate(&self, id: &str) -> Result<(), ProviderError>;
}
