//! Noticing a workspace's folder going missing or coming back while Neko runs (docs/knowledge/workspaces.md).
//!
//! While at least one screen is subscribed, one thread looks at the workspaces' folders every few seconds (a `stat` each) and pushes
//! a `workspace_updated` for any whose "missing" changed. With no screen it sleeps until one subscribes: an idle daemon with nobody
//! watching sets no timer. Tasks starting, and the app asking for the list, already check the folder at that moment.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use neko_protocol::{Change, ServerMessage};

use crate::server::{Shared, lock};

/// How often a watched daemon looks at its workspaces' folders.
pub const EVERY: Duration = Duration::from_secs(3);

static THREAD: OnceLock<std::thread::Thread> = OnceLock::new();

/// A screen subscribed: the watcher wakes (it sleeps while nobody watches).
pub fn screen_subscribed() {
    if let Some(t) = THREAD.get() {
        t.unpark();
    }
}

/// Starts the watcher thread.
pub fn spawn(shared: Arc<Shared>) -> std::io::Result<()> {
    let h = std::thread::Builder::new().name("neko-folders".into()).spawn(move || watch(&shared))?;
    let _ = THREAD.set(h.thread().clone());
    Ok(())
}

fn watch(shared: &Shared) {
    let mut known: HashMap<String, bool> = HashMap::new();
    loop {
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        let watched = shared.screens();
        if !watched {
            // Nobody to tell: sleep until a screen subscribes (or the daemon stops, which unparks too).
            known.clear();
            std::thread::park();
            continue;
        }
        let list = lock(&shared.core).workspaces().unwrap_or_default();
        for w in list {
            let was = known.insert(w.id.clone(), w.missing);
            // One not seen before counts as found (as it was when added): a folder gone since is pushed at once.
            if was.unwrap_or(false) != w.missing {
                shared.broadcast(&ServerMessage::Event { revision: 0, change: Change::WorkspaceUpdated { workspace: w } });
            }
        }
        std::thread::park_timeout(EVERY);
    }
}

/// The daemon is stopping: the watcher ends now.
pub fn stopping() {
    screen_subscribed();
}
