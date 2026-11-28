//! A Cattain runs its crew (docs/knowledge/cats.md, "The crew"): the records and the words. A Cattain's own agent starts threads in
//! its crew with Neko's `start_crew_thread`, reads them with `crew` and writes to them with `message_crew`; each crew thread's turn end,
//! and each of its requests that waits on the person, wakes the Cattain once (a `<neko-wake kind="crew">`), batched over half a minute.
//! Nothing here starts a process; the daemon does that (`crates/neko-daemon/src/crew.rs`).

use neko_protocol::{PermissionMode, Task, TaskId, TaskStatus};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::Core;
use crate::store::{Store, StoreError, insert_task};

/// Crew wakes due within this long of the first go in one message (milliseconds). Longer than a kitten's two seconds: crew threads
/// are whole threads that end minutes apart, and each wake is a cold turn of the Cattain's on the person's own subscription.
pub const BATCH_MS: u64 = 30_000;
/// The most threads of one Cattain's crew that may be unfinished at once (queued, working, needing the person, waiting on kittens).
/// A guard against a loop, not a plan.
pub const LIVE_LIMIT: usize = 8;
/// The most times a Cattain is woken by its crew in an hour. Past it the crew's news waits (kept) until the hour allows, or until the
/// person writes to the Cattain, whose run reads it then: a Cattain and its crew can never wake each other for ever.
pub const WAKES_PER_HOUR: usize = 20;
/// The longest a crew thread's title is, in characters.
pub const TITLE_CHARS: usize = 80;

/// The narrower of two access modes: a crew thread never has more than its Cattain. "Ask before risky steps" is the narrowest (Neko
/// asks), then "Follow Claude Code's own rules", then "Full access". A Cattain with no access of its own has the mode in Settings.
pub fn crew_mode(settings: PermissionMode, cattain: Option<PermissionMode>) -> PermissionMode {
    let rank = |m: PermissionMode| match m {
        PermissionMode::Ask => 0,
        PermissionMode::Follow => 1,
        PermissionMode::Full => 2,
    };
    match cattain {
        Some(c) if rank(c) < rank(settings) => c,
        _ => settings,
    }
}

/// Whether a crew thread's turn end wakes its Cattain: a thread of its crew the person or the Cattain started, not a round's (a round
/// runs by itself at its times; S13 decides how rounds speak to their Cattain), not one put away by the person.
pub fn tells_its_cattain(t: &Task) -> bool {
    t.crew_of().is_some() && t.round.is_none()
}

/// Whether a crew thread has not finished: queued, working, needing the person or waiting on its kittens.
pub fn is_live(t: &Task) -> bool {
    matches!(t.status, TaskStatus::New | TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Waiting)
}

/// How a crew thread stands, for the Cattain's agent.
pub fn state_words(t: &Task) -> &'static str {
    match t.status {
        TaskStatus::New => "queued: it starts when there is room",
        TaskStatus::Working => "working",
        TaskStatus::NeedsYou => "working, and waiting on the person to allow a step",
        TaskStatus::Waiting => "waiting on its own subagents",
        TaskStatus::Done => "done",
        TaskStatus::Failed => "failed",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Interrupted => "interrupted",
        TaskStatus::Asked => "asked the person a question and waits for their answer",
    }
}

/// A request of a crew thread waiting on the person that has not woken its Cattain yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CrewRequest {
    pub permission: String,
    pub thread: TaskId,
    pub title: String,
    pub detail: String,
}

impl Store {
    /// Every thread in a Cattain's crew (not its own thread, not a kitten), newest first.
    pub fn crew_of(&self, cat: &str) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {} FROM tasks WHERE cat_id = ?1 AND parent_id IS NULL AND cattain = 0 ORDER BY created_ms DESC, id DESC", crate::store::TASK_COLUMNS))?;
        let rows = st.query_map([cat], crate::store::task_of)?;
        let tasks: Vec<Task> = rows.collect::<Result<_, _>>()?;
        tasks.into_iter().map(|t| self.derived(t)).collect()
    }

    /// Crew threads a Cattain's agent started that have not begun (no room when they were made), oldest first.
    pub fn queued_crew(&self) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {} FROM tasks WHERE started_by IS NOT NULL AND parent_id IS NULL AND status = 'new' AND archived_ms IS NULL \
             AND NOT EXISTS (SELECT 1 FROM runs r WHERE r.task_id = tasks.id) ORDER BY created_ms, id",
            crate::store::TASK_COLUMNS
        ))?;
        let rows = st.query_map([], crate::store::task_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Whether this run of a crew thread has woken its Cattain already (a turn end wakes it once).
    pub fn crew_wake_for_run(&self, thread: &TaskId, run: &neko_protocol::RunId) -> Result<bool, StoreError> {
        Ok(self.conn.query_row("SELECT 1 FROM wakes WHERE kind = 'crew' AND subagent_id = ?1 AND run_id = ?2 LIMIT 1", params![thread.0, run.0], |_| Ok(())).optional()?.is_some())
    }

    /// How many times this thread was woken by its crew since `since_ms` (wakes delivered together count once).
    pub fn crew_wakes_since(&self, task: &TaskId, since_ms: i64) -> Result<usize, StoreError> {
        let n: i64 = self.conn.query_row(
            "SELECT COUNT(DISTINCT delivered_ms) FROM wakes WHERE kind = 'crew' AND task_id = ?1 AND state = 'delivered' AND delivered_ms >= ?2",
            params![task.0, since_ms],
            |r| r.get(0),
        )?;
        Ok(n as usize)
    }

    /// Requests of crew threads (their own, not their kittens') waiting on the person that have not woken their Cattain yet, oldest
    /// first. A round's thread is left out, as its turn ends are.
    pub fn crew_requests(&self) -> Result<Vec<CrewRequest>, StoreError> {
        let mut st = self.conn.prepare_cached(
            "SELECT p.id, p.task_id, p.title, p.detail FROM permissions p JOIN tasks t ON t.id = p.task_id \
             WHERE p.state = 'pending' AND t.parent_id IS NULL AND t.cattain = 0 AND t.cat_id IS NOT NULL AND t.cat_id != '' AND t.round_id IS NULL \
             AND NOT EXISTS (SELECT 1 FROM wakes w WHERE w.kind = 'crew' AND w.about = p.id) ORDER BY p.asked_ms, p.id",
        )?;
        let rows = st.query_map([], |r| Ok(CrewRequest { permission: r.get(0)?, thread: TaskId(r.get(1)?), title: r.get(2)?, detail: r.get(3)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Whether a thread has ever run (it has a conversation to resume).
    pub fn has_run(&self, task: &TaskId) -> Result<bool, StoreError> {
        Ok(self.conn.query_row("SELECT 1 FROM runs WHERE task_id = ?1 LIMIT 1", [&task.0], |_| Ok(())).optional()?.is_some())
    }
}

/// What `start_crew_thread` was asked for.
#[derive(Clone, Debug, Default)]
pub struct CrewAsk<'a> {
    pub title: &'a str,
    /// Everything the thread is told: its first message.
    pub brief: &'a str,
    /// A project by its name or its folder. None: the Cattain's project, else No project.
    pub project: Option<&'a str>,
    /// Work in the project's own folder, not a worktree of its own.
    pub in_folder: bool,
}

impl Core {
    /// A thread in `cat`'s crew, started by its agent: the brief as its words, the title given, `started_by` the Cattain, in the project
    /// named (else the Cattain's). Not started: the daemon starts it when there is room. Refused in words the agent reads.
    pub fn create_crew_thread(&mut self, cat: &str, ask: &CrewAsk<'_>) -> Result<(Task, u64), String> {
        let c = self.cat(cat).map_err(|e| e.to_string())?;
        if !c.is_active() {
            return Err(format!("{} is retired and takes no new crew. Tell the person; they can bring it back in Settings ▸ Cattains.", c.name));
        }
        let title: String = ask.title.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(TITLE_CHARS).collect();
        if title.is_empty() {
            return Err("A crew thread needs a title (a few words).".into());
        }
        if ask.brief.trim().is_empty() {
            return Err("A crew thread needs a brief: it sees nothing of your thread, only what you write.".into());
        }
        let live = self.store.crew_of(&c.id).map_err(|e| e.to_string())?.iter().filter(|t| tells_its_cattain(t) && is_live(t)).count();
        if live >= LIVE_LIMIT {
            return Err(format!("{live} threads of your crew have not finished, the most Neko runs for one Cattain. Wait for them (Neko wakes you as each turn ends) before you start more."));
        }
        let workspace = match ask.project.map(str::trim).filter(|p| !p.is_empty()) {
            Some(p) => Some(self.project_named(p).map_err(|e| e.to_string().replace("make the Cattain with none", "use your own project"))?),
            None => c.workspace.clone().filter(|w| self.store.get_workspace(w).ok().flatten().is_some_and(|w| w.is_active())),
        };
        let (id, w, _, now) = self.plan_task(ask.brief, workspace.as_deref(), Some(&c.id)).map_err(|e| e.to_string())?;
        let mut task = Task::blank(id, &w.id, ask.brief, &title, now, Some(&c.id));
        task.folder = w.folder.clone();
        task.in_folder = ask.in_folder && w.folder.is_some();
        task.started_by = Some(c.id.clone());
        let tx = self.store.conn.transaction().map_err(|e| e.to_string())?;
        let rev = insert_task(&tx, &task).map_err(|e| e.to_string())?;
        tx.commit().map_err(|e| e.to_string())?;
        let task = self.store.get_task(&task.id).map_err(|e| e.to_string())?.unwrap_or(task);
        Ok((task, rev))
    }
}

// ---- What the agent is told ---------------------------------------------------------------------------------------------------

/// What `start_crew_thread` answers: who it is and the cue to end the turn. `started`: it works now (else it is queued).
pub fn started_text(task: &Task, project: Option<&str>, started: bool) -> String {
    let place = project.map(|p| format!(" in the project {p}")).unwrap_or_default();
    let state = if started { "It is working now." } else { "It is queued behind other work and starts when there is room." };
    format!(
        "\"{}\" is started in your crew{place} (thread {}). {state} It reads your CREW.md on every run. Neko wakes you when its turn ends or when it needs the person. Do not poll or wait. If you have nothing else to do, update DIGEST.md and NOTES.md and end your turn now.",
        task.title, task.id.0
    )
}

/// The tools only a Cattain's own thread is offered, as MCP's `tools/list` gives them.
pub fn tools() -> Vec<Value> {
    vec![
        json!({
            "name": "start_crew_thread",
            "description": "Starts a thread in your crew: a whole thread of the person's, with its own agent (and, in a project, a worktree of its own), that reads your CREW.md on every run. \
It sees only the brief, so write it in full. Returns at once with its id. Neko wakes you when its turn ends or when it needs the person: do not poll, and end your turn when you have nothing else to do. \
One thread for each separate piece of work. You cannot approve its steps: it asks the person.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "A few words naming the work, as the person will read it in the sidebar."},
                    "brief": {"type": "string", "description": "Everything the thread needs, in full: it sees nothing else of yours."},
                    "project": {"type": "string", "description": "The project it works in, by its name in Neko or its folder. Default: your own project."},
                    "placement": {"type": "string", "enum": ["worktree", "folder"], "description": "worktree (a copy of its own on its own branch, the default) or folder (the project's own folder)."}
                },
                "required": ["title", "brief"],
                "additionalProperties": false
            }
        }),
        json!({
            "name": "crew",
            "description": "Lists the threads of your crew: id, title, state, project and last line. Read-only and instant. Do not call it to wait: Neko wakes you as each turn ends. With id, returns that thread's last words in full.",
            "inputSchema": {
                "type": "object",
                "properties": { "id": {"type": "string", "description": "A crew thread's id, to read its last words in full."} },
                "additionalProperties": false
            }
        }),
        json!({
            "name": "message_crew",
            "description": "Writes to a thread of your crew: more to do, a correction, or the person's decision passed on (quote it). Returns at once. A thread at rest starts again with your words; \
one that works reads them at its next step (steer), after its turn (queue) or at once, its turn cut short (interrupt). Your words approve no step, and you cannot answer a question it asked the person.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "id": {"type": "string", "description": "The crew thread's id (or its exact title)."},
                    "text": {"type": "string", "description": "What you tell it, in full."},
                    "mode": {"type": "string", "enum": ["steer", "queue", "interrupt"], "description": "For a thread that is working: when it reads your words. Default steer."}
                },
                "required": ["id", "text"],
                "additionalProperties": false
            }
        }),
    ]
}

/// The words the Cattain sends a crew thread, inside `<neko-from>`, and reading them back (the app reads them too, so they are the
/// protocol's).
pub use neko_protocol::cats::{from_cattain, read_from_cattain};

/// What the end of a crew wake tells the Cattain to do.
pub const WAKE_CUE: &str = "Read what changed, then rewrite DIGEST.md (Needs you, New, In progress) and NOTES.md. Write to a crew thread with mcp__neko__message_crew only when it needs \
more from you. You cannot approve a crew thread's steps or answer what it asked the person; those stay under Needs you. Never tell a crew thread to merge, push, publish, delete \
or pay unless the person asked for it in your thread. Tag the person with mcp__neko__ask_person only for a decision that is theirs. Then end your turn.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crew_thread_never_has_more_access_than_its_cattain() {
        use PermissionMode::{Ask, Follow, Full};
        assert_eq!(crew_mode(Full, Some(Ask)), Ask);
        assert_eq!(crew_mode(Full, Some(Follow)), Follow);
        assert_eq!(crew_mode(Ask, Some(Full)), Ask, "Settings narrower than the Cattain: Settings");
        assert_eq!(crew_mode(Follow, None), Follow, "a Cattain with no access of its own: Settings");
    }

    #[test]
    fn a_message_from_the_cattain_reads_back_exactly() {
        let m = from_cattain("miso", "The person said: \"merge them\".\n");
        assert_eq!(m, "<neko-from cattain=\"miso\">\nThe person said: \"merge them\".\n</neko-from>");
        assert_eq!(read_from_cattain(&m), Some(("miso", "The person said: \"merge them\".")));
        assert_eq!(read_from_cattain("merge them"), None);
    }

    #[test]
    fn the_tools_are_three_and_tell_the_agent_not_to_poll() {
        let t = tools();
        let names: Vec<&str> = t.iter().filter_map(|x| x["name"].as_str()).collect();
        assert_eq!(names, ["start_crew_thread", "crew", "message_crew"]);
        assert!(t[0]["description"].as_str().unwrap().contains("do not poll") && t[1]["description"].as_str().unwrap().contains("Do not call it to wait"));
        assert_eq!(t[0]["inputSchema"]["required"], json!(["title", "brief"]));
        assert!(WAKE_CUE.contains("Never tell a crew thread to merge") && WAKE_CUE.contains("DIGEST.md"));
    }
}
