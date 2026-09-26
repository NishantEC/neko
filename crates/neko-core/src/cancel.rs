//! A cooperative cancellation signal, shared between the daemon's
//! request-dispatch loop and whichever provider is doing real, interruptible
//! I/O for that request.
//!
//! **Why this exists at all**: `FileProvider`'s `mdfind` round-trip is
//! bounded (`files::QUERY_TIMEOUT`, 1.5s) but not *abandonable* — before
//! this type, a search whose query had already been superseded by the next
//! keystroke still ran its child process to completion (or to its own
//! timeout) with nobody left who wanted the answer. On a search-as-you-type
//! field that means every character typed leaves one more `mdfind` running,
//! all of them competing for the same Spotlight query planner, making the
//! *current* query slower the faster the captain types. Cancelling the
//! superseded query — killing the child process, not just discarding its
//! output — is what stops that pile-up.
//!
//! Deliberately a plain `Arc<AtomicBool>` rather than a channel or a
//! `Condvar`: the only two operations anyone needs are "set it" (from the
//! dispatch thread, when a newer request for the same client arrives) and
//! "poll it" (from inside a provider's own read loop, which is already
//! waking on a short interval to enforce its wall-clock bound). A cancelled
//! token is never un-cancelled — one-shot, so a stale clone can't
//! resurrect.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// A one-shot, clonable "the caller stopped caring" flag. Cloning shares
/// the same underlying signal, so the dispatch thread's copy and the
/// provider's copy are the same token.
#[derive(Clone, Debug, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    /// A fresh, not-yet-cancelled token.
    pub fn new() -> Self {
        Self::default()
    }

    /// A token nobody holds the other end of — for call sites that have no
    /// supersede concept at all (a direct `Provider::search` call from a
    /// test, or a request type that can't be superseded). Reads as
    /// "never cancelled" forever.
    pub fn never() -> Self {
        Self::default()
    }

    /// Signal every holder of this token that the work is no longer wanted.
    /// Idempotent.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fresh_token_is_not_cancelled() {
        assert!(!Cancel::new().is_cancelled());
        assert!(!Cancel::never().is_cancelled());
    }

    #[test]
    fn cancelling_one_clone_cancels_every_clone() {
        let token = Cancel::new();
        let clone = token.clone();
        assert!(!clone.is_cancelled());
        token.cancel();
        assert!(
            clone.is_cancelled(),
            "clones share one signal, they are not independent copies"
        );
    }

    #[test]
    fn cancelling_twice_is_idempotent_and_never_reverts() {
        let token = Cancel::new();
        token.cancel();
        token.cancel();
        assert!(token.is_cancelled());
    }
}
