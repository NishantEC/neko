//! The `AgentProvider` seam. Per the plan (`data/dim/plan.md`): "An
//! `AgentProvider`-shaped trait in core from day one, before any
//! implementation exists. Cheap now, expensive later." Nothing in the
//! daemon's search path calls this yet — it exists to prove the boundary
//! compiles and is object-safe, not to ship agent capability in this slice.
//! Wiring an `AgentProvider`'s results into `search::rank_apps`'s output as
//! a new `SearchItem` kind (matching the plan's "just another result type
//! in the same fast list") is the seam a future task plugs into.

use neko_protocol::SearchItem;

#[derive(Debug)]
pub struct AgentError(pub String);

impl std::fmt::Display for AgentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for AgentError {}

/// Object-safe by construction (no generics, no `Self` return types) so the
/// daemon can hold a `Vec<Box<dyn AgentProvider>>` once a real
/// implementation exists.
pub trait AgentProvider: Send + Sync {
    /// A stable identifier for this provider, e.g. for per-provider
    /// settings or attribution in a result row.
    fn id(&self) -> &str;

    /// Answer a query with zero or more results, in the same `SearchItem`
    /// shape the app-search path produces so the client's result list
    /// never needs to know which provider a row came from.
    fn query(&self, input: &str) -> Result<Vec<SearchItem>, AgentError>;
}

/// Proves the trait object boundary compiles; returns nothing. Not
/// registered anywhere in the daemon's request handling in this slice.
pub struct FakeProvider;

impl AgentProvider for FakeProvider {
    fn id(&self) -> &str {
        "fake"
    }

    fn query(&self, _input: &str) -> Result<Vec<SearchItem>, AgentError> {
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_provider_is_usable_as_a_trait_object() {
        let providers: Vec<Box<dyn AgentProvider>> = vec![Box::new(FakeProvider)];
        for provider in &providers {
            assert_eq!(provider.query("anything").unwrap(), Vec::new());
        }
        assert_eq!(providers[0].id(), "fake");
    }
}
