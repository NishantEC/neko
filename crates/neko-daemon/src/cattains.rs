//! A Cattain's files and what its runs are handed, as the daemon serves them (docs/knowledge/cats.md, "Cattains"). The rules are in
//! `neko_core::cattains`; this file reads the folder under the core's lock and tells every screen what changed.

use neko_protocol::{Change, ErrorCode, Reply, ServerMessage, Task};

use crate::server::{Shared, lock};

/// Every screen hears each file that changed.
fn tell(shared: &Shared, changed: neko_core::cattains::Changed) {
    for (file, revision) in changed {
        shared.broadcast(&ServerMessage::Event { revision, change: Change::CattainFileChanged { file } });
    }
}

/// What a run of `task` is handed by its cat (RULES.md or CREW.md, read from the folder now), telling every screen about files the
/// Cattain changed meanwhile. A Cattain's own thread whose folder cannot be read does not run; anything else runs without a block.
pub fn handed(shared: &Shared, task: &Task) -> Result<Option<neko_core::cattains::Handed>, String> {
    let core = &mut *lock(&shared.core);
    let fresh = core.store.get_task(&task.id).ok().flatten().unwrap_or_else(|| task.clone());
    match core.handed(&fresh) {
        Ok((h, changed)) => {
            tell(shared, changed);
            Ok(h)
        }
        Err(e) if fresh.cattain => Err(format!("Neko could not read {}'s folder: {e}", fresh.title)),
        Err(_) => Ok(None),
    }
}

/// Looks at a Cattain's folder (after a run of its own thread) and tells every screen what changed.
pub fn look(shared: &Shared, cat: &str) {
    let changed = lock(&shared.core).sync_cattain(cat).unwrap_or_default();
    tell(shared, changed);
}

fn refused(e: neko_core::CatError) -> Reply {
    use neko_core::CatError as C;
    let code = match &e {
        C::NotFound => ErrorCode::NotFound,
        C::InvalidName(_) => ErrorCode::InvalidName,
        C::Refused(_) => ErrorCode::Refused,
        C::Store(_) => ErrorCode::Internal,
    };
    Reply::error(code, e.to_string())
}

/// `open_cattain`: its own thread, made now if it has none (every screen hears the thread, then the cat that names it).
pub fn open(shared: &Shared, cat: &str) -> Reply {
    let core = &mut *lock(&shared.core);
    match core.cattain_thread(cat) {
        Ok((task, cat, made)) => {
            if let Some(revision) = made {
                shared.broadcast(&ServerMessage::Event { revision, change: Change::TaskCreated { task: task.clone() } });
                shared.broadcast(&ServerMessage::Event { revision, change: Change::CatUpdated { cat } });
            }
            Reply::Task { task }
        }
        Err(e) => refused(e),
    }
}

/// `get_cattain_file`: the file as it is on disk now.
pub fn get(shared: &Shared, cat: &str, name: &str) -> Reply {
    let found = lock(&shared.core).cattain_file(cat, name);
    match found {
        Ok((file, changed)) => {
            tell(shared, changed);
            Reply::CattainFile { file }
        }
        Err(e) => refused(e),
    }
}

/// `save_cattain_file` and `restore_cattain_file`: the file as it is now, to the asker and to every screen.
pub fn change(shared: &Shared, cat: &str, name: &str, text: Option<&str>) -> Reply {
    let core = &mut *lock(&shared.core);
    let before = core.store.revision().unwrap_or(0);
    let done = match text {
        Some(t) => core.save_cattain_file(cat, name, t),
        None => core.restore_cattain_file(cat, name),
    };
    match done {
        Ok((file, revision)) => {
            if revision > before {
                shared.broadcast(&ServerMessage::Event { revision, change: Change::CattainFileChanged { file: file.clone() } });
            }
            Reply::CattainFile { file }
        }
        Err(e) => refused(e),
    }
}
