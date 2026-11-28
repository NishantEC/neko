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

// ---- Making one by asking (version 22) ----------------------------------------------------------------------------------------

fn answer(text: impl Into<String>, error: bool) -> serde_json::Value {
    serde_json::json!({"text": text.into(), "error": error})
}

/// A thread's agent may make Cattains: it is not a subagent, and the person did not keep them to making by hand.
pub fn may_make(core: &neko_core::Core, task: &neko_protocol::TaskId) -> bool {
    let thread = core.store.get_task(task).ok().flatten().is_some_and(|t| t.subagent.is_none());
    thread && !core.store.settings().is_ok_and(|s| s.cattains_by_hand)
}

/// Neko's `make_cattain` tool, from the run `run`: the Cattain is made (or one with that purpose is named), every screen hears it,
/// and the thread that asked gets its note. The words tell the agent its name and to tell the person.
pub fn make_tool(shared: &Shared, run: &crate::runs::McpRun, args: &serde_json::Value) -> serde_json::Value {
    let text = |k: &str| args[k].as_str();
    let Some(purpose) = text("purpose") else { return answer("make_cattain needs a purpose: a few words on what the Cattain looks after.", true) };
    let ask = neko_core::cats::MakeAsk { name: text("name"), purpose, project: text("project"), rules: text("rules") };
    let made = {
        let core = &mut *lock(&shared.core);
        if core.store.get_task(&run.task).ok().flatten().is_some_and(|t| t.subagent.is_some()) {
            return answer("Only a thread's own agent can make a Cattain, not a subagent. Say in your report that one is needed.", true);
        }
        if !may_make(core, &run.task) {
            return answer("The person makes Cattains by hand only (Settings ▸ Cattains). Tell them which one would help and why.", true);
        }
        core.make_cattain(&run.task, &ask)
    };
    match made {
        Ok(neko_core::cats::Made::Reused(cat)) => answer(
            format!(
                "{} already looks after this ({}), so no Cattain was made. Tell the person to write to {} in its own thread in Neko.",
                cat.name,
                cat.purpose.clone().unwrap_or_default(),
                cat.name
            ),
            false,
        ),
        Ok(neko_core::cats::Made::New { cat, revision, rules: (file, file_rev) }) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::CatAdded { cat: cat.clone() } });
            shared.broadcast(&ServerMessage::Event { revision: file_rev, change: Change::CattainFileChanged { file } });
            {
                let mut b = lock(&run.book);
                let mut core = lock(&shared.core);
                let now = core.now();
                if let Ok(outs) = b.cattain_note(&mut core, &cat, now) {
                    crate::runs::push(shared, &run.task, outs, run.control.as_ref());
                }
            }
            let project = cat.workspace.as_deref().and_then(|w| lock(&shared.core).store.get_workspace(w).ok().flatten()).map(|w| format!(" in the project {}", w.name)).unwrap_or_default();
            answer(
                format!(
                    "Made a Cattain: {} ({}){project}. Address it as {}. The person sees it in Neko (Settings ▸ Cattains, Search) and talks to it in its own thread; it has no work until they write to it there. Tell the person you made {} and what it is for.",
                    cat.name,
                    cat.purpose.clone().unwrap_or_default(),
                    cat.name,
                    cat.name
                ),
                false,
            )
        }
        Err(e) => answer(e.to_string(), true),
    }
}

/// Neko's `list_cattains` tool: the Cattains at work, in words. Read-only.
pub fn list_tool(shared: &Shared, run: &crate::runs::McpRun) -> serde_json::Value {
    let core = lock(&shared.core);
    if core.store.get_task(&run.task).ok().flatten().is_some_and(|t| t.subagent.is_some()) {
        return answer("Only a thread's own agent looks after Cattains, not a subagent.", true);
    }
    match core.cattains_text() {
        Ok(t) => answer(t, false),
        Err(e) => answer(format!("Neko could not list the Cattains: {e}"), true),
    }
}

/// `undo_make_cattain`: the toast's Undo. Every screen hears the cat, retired.
pub fn undo_make(shared: &Shared, cat: &str) -> Reply {
    let done = lock(&shared.core).undo_make_cattain(cat);
    match done {
        Ok((cat, revision)) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::CatUpdated { cat: cat.clone() } });
            Reply::Cat { cat }
        }
        Err(e) => refused(e),
    }
}
