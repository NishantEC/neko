//! Neko's core: the store and its first records (docs/knowledge/store.md).
//!
//! Logic here never reads the system clock or makes a random id itself: a [`Clock`] and an [`Ids`] are handed in, so the tests
//! run the same every time. The daemon hands in [`SystemClock`] and [`RandomIds`].

pub mod clock;
pub mod store;
pub mod title;

pub use clock::{Clock, FixedClock, Ids, RandomIds, SequentialIds, SystemClock};
pub use store::{DEFAULT_WORKSPACE, Store, StoreError};

use neko_protocol::{MAX_WORDS_BYTES, Task, TaskId};

/// Why a new task was refused.
#[derive(Debug, PartialEq, Eq)]
pub enum NewTaskError {
    Empty,
    TooLong,
    Store(String),
}

impl std::fmt::Display for NewTaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NewTaskError::Empty => f.write_str("Type what you want done first."),
            NewTaskError::TooLong => write!(f, "That is longer than a task can be ({} KB). Put the long part in a file.", MAX_WORDS_BYTES / 1000),
            NewTaskError::Store(e) => write!(f, "The task could not be saved: {e}"),
        }
    }
}

/// The store with the clock and ids its writes need.
pub struct Core {
    pub store: Store,
    clock: Box<dyn Clock>,
    ids: Box<dyn Ids>,
}

impl Core {
    /// Opens (or makes) the store at `path` and makes sure the default workspace exists.
    pub fn open(path: &std::path::Path, clock: Box<dyn Clock>, ids: Box<dyn Ids>) -> Result<Core, StoreError> {
        let store = Store::open(path)?;
        store.ensure_default_workspace(clock.now_ms())?;
        Ok(Core { store, clock, ids })
    }

    /// Stores a task with the person's exact words, and the event that says so. Returns the task and the event's revision.
    pub fn create_task(&mut self, words: &str) -> Result<(Task, u64), NewTaskError> {
        if words.trim().is_empty() {
            return Err(NewTaskError::Empty);
        }
        if words.len() > MAX_WORDS_BYTES {
            return Err(NewTaskError::TooLong);
        }
        let now = self.clock.now_ms();
        let id = TaskId(format!("task_{}", self.ids.next_id(now)));
        self.store.create_task(&id, DEFAULT_WORKSPACE, words, now).map_err(|e| NewTaskError::Store(e.to_string()))
    }
}
