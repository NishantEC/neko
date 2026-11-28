//! Search (⌘K): what a screen asks the daemon to find, and what it finds (docs/knowledge/search.md).
//!
//! Only threads and kittens are found by the daemon, since they are the only things with words in a store: their titles and what the
//! person and the agent said in them. Cattains, projects and actions are the screen's own lists.

use serde::{Deserialize, Serialize};

use crate::{PartId, TaskId};

/// How many results a search with no limit gives, and the most it ever gives.
pub const DEFAULT_LIMIT: u32 = 12;
pub const MAX_LIMIT: u32 = 50;

/// What a result matched.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoundBy {
    /// The words of its title.
    Title,
    /// Something said in it; the result carries the words around the match and the part.
    Message,
    /// Nothing typed matched: it is here for what it is (the most recent, or what the filters kept).
    Recent,
}

/// One thread or kitten a search found. The screen has the task itself (its title, project, status and age); this says why it is here.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Found {
    pub task: TaskId,
    /// It is a kitten (a subagent), not a thread.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub kitten: bool,
    pub by: FoundBy,
    /// For a message: a line of the words around the match, one line, cut with "…" at either end.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub snippet: String,
    /// For a message: the part of the transcript it was said in, and its place in it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part: Option<PartId>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub seq: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}
