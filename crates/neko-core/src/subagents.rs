//! Subagents (docs/knowledge/subagents.md): a thread splits its work into numbered subagents, ends its turn, and is woken with each
//! one's report. This module is the records and the words: the store's rows for subagents and the reports that wake a thread, the
//! text the agent is told, the tools Neko's own MCP server offers and the message that wakes a thread. Nothing here starts a
//! process; the daemon does that (`crates/neko-daemon/src/subagents.rs`).

use neko_protocol::{SubPlace, SubagentOf, Task, TaskId, TaskStatus};
use rusqlite::{OptionalExtension, params};
use serde_json::{Value, json};

use crate::records::log_event;
use crate::store::{Store, StoreError, insert_task};
use crate::Core;

/// Subagents of one thread running at once; the rest wait their turn.
pub const MAX_RUNNING_PER_THREAD: usize = neko_protocol::SUBAGENTS_PER_THREAD;
/// Runs at once across the daemon that Neko starts by itself (a subagent's start, a wake); more wait. A run the person starts is
/// never held back, but it counts.
pub const MAX_RUNS: usize = neko_protocol::RUNS_ACROSS_NEKO;
/// How deep subagents nest: Subagent 1.2.3 is the last that may not start more.
pub const MAX_DEPTH: usize = neko_protocol::SUBAGENT_DEPTH;
/// The most subagents of one thread that may be waiting, running or unread at once. A guard against a loop, not a plan.
pub const MAX_LIVE_PER_THREAD: usize = 24;
/// The most of a report's words a wake carries (bytes).
pub const SUMMARY_LIMIT: usize = 16 * 1024;
/// Several wakes due within this long of the first go in one message (milliseconds).
pub const BATCH_MS: u64 = 2_000;

/// The name of Neko's own MCP server in a run's config, so its tools are `mcp__neko__*`. A plugin may not take it.
pub const SERVER: &str = "neko";

/// Whether a task that has this status has not ended: a subagent in one of these is one its thread still waits on.
pub fn is_live(s: TaskStatus) -> bool {
    matches!(s, TaskStatus::New | TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Waiting)
}

/// How a subagent ended, as a wake says it.
pub fn status_word(s: TaskStatus) -> &'static str {
    match s {
        TaskStatus::Done => "done",
        TaskStatus::Failed => "failed",
        TaskStatus::Stopped => "stopped",
        TaskStatus::Interrupted => "interrupted",
        TaskStatus::Asked => "asked",
        _ => "done",
    }
}

/// How a new subagent is placed (what `start_subagent` was told).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct NewSubagent {
    pub placement: SubPlace,
    /// The number of the sibling it starts after, from its branch.
    pub after: Option<String>,
    /// Written, not started.
    pub draft: bool,
}

/// What may be done to a draft.
#[derive(Clone, Copy, Debug)]
pub enum DraftChange<'a> {
    Start,
    Discard,
    Prompt(&'a str),
}

/// The branch a subagent's own copy works on: `neko/<the thread's short id>-<number, dots as dashes>`, so Kitten 1.2 of thread
/// `…4f2a` is `neko/4f2a-1-2`.
pub fn branch_name(root: &TaskId, number: &str) -> String {
    format!("neko/{}-{}", crate::workplace::short_id(root), number.replace('.', "-"))
}

/// One report that wakes a thread.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WakeRow {
    pub id: String,
    /// The thread woken.
    pub task: TaskId,
    /// The subagent that ended, its number and its title.
    pub subagent: TaskId,
    pub number: String,
    pub title: String,
    /// `done`, `failed`, `stopped`, `interrupted` or `asked`.
    pub status: String,
    pub summary: String,
    pub files: Vec<String>,
    pub branch: Option<String>,
    pub created_ms: i64,
}

impl Store {
    /// The subagents a task started directly, oldest first.
    pub fn children_of(&self, parent: &TaskId) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {} FROM tasks WHERE parent_id = ?1 ORDER BY created_ms, id", crate::store::TASK_COLUMNS))?;
        let rows = st.query_map([&parent.0], crate::store::task_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every subagent under a thread, at any depth, oldest first.
    pub fn subagents_of(&self, root: &TaskId) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {} FROM tasks WHERE root_id = ?1 ORDER BY created_ms, id", crate::store::TASK_COLUMNS))?;
        let rows = st.query_map([&root.0], crate::store::task_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Every subagent not yet started and meant to be (a draft is not), oldest first.
    pub fn queued_subagents(&self) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!("SELECT {} FROM tasks WHERE parent_id IS NOT NULL AND status = 'new' AND sub_draft = 0 ORDER BY created_ms, id", crate::store::TASK_COLUMNS))?;
        let rows = st.query_map([], crate::store::task_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// The number the next subagent of `parent` gets: `1`, `2`… under a thread, `1.1`, `1.2`… under Subagent 1.
    pub fn next_subagent_number(&self, parent: &Task) -> Result<String, StoreError> {
        let prefix = parent.subagent.as_ref().map(|s| format!("{}.", s.number)).unwrap_or_default();
        let last = self
            .children_of(&parent.id)?
            .iter()
            .filter_map(|c| c.subagent.as_ref().and_then(|s| s.number.rsplit('.').next().and_then(|n| n.parse::<u32>().ok())))
            .max()
            .unwrap_or(0);
        Ok(format!("{prefix}{}", last + 1))
    }

    /// The drafts a task wrote (direct subagents it has not started), oldest first.
    pub fn drafts_of(&self, parent: &TaskId) -> Result<Vec<Task>, StoreError> {
        Ok(self.children_of(parent)?.into_iter().filter(|t| t.subagent.as_ref().is_some_and(|s| s.draft)).collect())
    }

    /// A draft starts (`draft` false), becomes a discarded one (`discarded`), or its prompt is rewritten (`prompt`): one change, with its
    /// event. Returns the task whole.
    pub fn change_draft(&mut self, id: &TaskId, change: DraftChange<'_>, now_ms: i64) -> Result<(Task, u64), StoreError> {
        let tx = self.conn.transaction()?;
        let kind = match change {
            DraftChange::Start => {
                tx.execute("UPDATE tasks SET sub_draft = 0 WHERE id = ?1 AND sub_draft = 1", [&id.0])?;
                "draft_started"
            }
            DraftChange::Discard => {
                tx.execute("UPDATE tasks SET sub_draft = 2, status = 'stopped' WHERE id = ?1 AND sub_draft = 1", [&id.0])?;
                "draft_discarded"
            }
            DraftChange::Prompt(words) => {
                tx.execute("UPDATE tasks SET words = ?2 WHERE id = ?1 AND sub_draft = 1", params![id.0, words])?;
                "draft_edited"
            }
        };
        let seq = log_event(&tx, now_ms, &id.0, kind, json!({}))?;
        tx.commit()?;
        let task = self.get_task(id)?.ok_or(StoreError::Sqlite(rusqlite::Error::QueryReturnedNoRows))?;
        Ok((task, seq))
    }

    /// A subagent's own row: the thread's workspace, folder and cat, the prompt as its words, not yet started.
    pub fn insert_subagent(&mut self, id: &TaskId, parent: &Task, number: &str, title: &str, prompt: &str, now_ms: i64, how: &NewSubagent) -> Result<(Task, u64), StoreError> {
        let root = parent.subagent.as_ref().map_or_else(|| parent.id.clone(), |s| s.root.clone());
        let mut task = Task::blank(id.clone(), &parent.workspace, prompt, title, now_ms, parent.cat.as_deref());
        task.folder = parent.folder.clone();
        let branch = (how.placement == SubPlace::OwnCopy).then(|| branch_name(&root, number));
        task.subagent = Some(SubagentOf { placement: how.placement, after: how.after.clone(), draft: how.draft, branch, ..SubagentOf::new(parent.id.clone(), number, root) });
        let tx = self.conn.transaction()?;
        let seq = insert_task(&tx, &task)?;
        tx.commit()?;
        Ok((task, seq))
    }

    /// Stores a report that wakes a thread, `pending` until it is read.
    pub fn insert_wake(&mut self, w: &WakeRow, state: &str, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO wakes (id, task_id, subagent_id, status, summary, files, branch, state, created_ms) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![w.id, w.task.0, w.subagent.0, w.status, w.summary, serde_json::to_string(&w.files).unwrap_or_else(|_| "[]".into()), w.branch, state, w.created_ms],
        )?;
        let seq = log_event(&tx, now_ms, &w.subagent.0, "wake_created", json!({ "task": w.task.0, "status": w.status, "state": state }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Every report not read yet, oldest first, with its subagent's number and title.
    pub fn pending_wakes(&self) -> Result<Vec<WakeRow>, StoreError> {
        let mut st = self.conn.prepare_cached(
            "SELECT w.id, w.task_id, w.subagent_id, t.sub_number, t.title, w.status, w.summary, w.files, w.branch, w.created_ms \
             FROM wakes w JOIN tasks t ON t.id = w.subagent_id WHERE w.state = 'pending' ORDER BY w.created_ms, w.id",
        )?;
        let rows = st.query_map([], |r| {
            Ok(WakeRow {
                id: r.get(0)?,
                task: TaskId(r.get(1)?),
                subagent: TaskId(r.get(2)?),
                number: r.get::<_, Option<String>>(3)?.unwrap_or_default(),
                title: r.get(4)?,
                status: r.get(5)?,
                summary: r.get(6)?,
                files: serde_json::from_str(&r.get::<_, String>(7)?).unwrap_or_default(),
                branch: r.get(8)?,
                created_ms: r.get(9)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Whether this thread has a report waiting to be read.
    pub fn has_pending_wake(&self, task: &TaskId) -> Result<bool, StoreError> {
        Ok(self.conn.query_row("SELECT 1 FROM wakes WHERE task_id = ?1 AND state = 'pending' LIMIT 1", [&task.0], |_| Ok(())).optional()?.is_some())
    }

    /// Settles reports: `delivered` once read (they were put in front of the agent), `discarded` when the person stopped the work.
    pub fn settle_wakes(&mut self, ids: &[String], state: &str, now_ms: i64) -> Result<u64, StoreError> {
        let tx = self.conn.transaction()?;
        for id in ids {
            tx.execute("UPDATE wakes SET state = ?2, delivered_ms = ?3 WHERE id = ?1 AND state = 'pending'", params![id, state, now_ms])?;
        }
        let seq = log_event(&tx, now_ms, "wakes", &format!("wake_{state}"), json!({ "ids": ids }))?;
        tx.commit()?;
        Ok(seq)
    }

    /// Discards every report waiting for a thread (it was stopped).
    pub fn discard_wakes_of(&mut self, task: &TaskId, now_ms: i64) -> Result<(), StoreError> {
        let ids: Vec<String> = {
            let mut st = self.conn.prepare("SELECT id FROM wakes WHERE task_id = ?1 AND state = 'pending'")?;
            st.query_map([&task.0], |r| r.get(0))?.collect::<Result<_, _>>()?
        };
        if !ids.is_empty() {
            self.settle_wakes(&ids, "discarded", now_ms)?;
        }
        Ok(())
    }

    /// Subagents that ended with no report made: the daemon was cut off before it could.
    pub fn ended_without_wake(&self) -> Result<Vec<Task>, StoreError> {
        let mut st = self.conn.prepare_cached(&format!(
            "SELECT {} FROM tasks WHERE parent_id IS NOT NULL AND status IN ('done', 'failed', 'stopped', 'interrupted', 'asked') \
             AND sub_draft != 2 AND NOT EXISTS (SELECT 1 FROM wakes w WHERE w.subagent_id = tasks.id) ORDER BY created_ms, id",
            crate::store::TASK_COLUMNS
        ))?;
        let rows = st.query_map([], crate::store::task_of)?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Removes the reports about, and for, a task being deleted.
    pub fn delete_wakes_of_task(&mut self, task: &TaskId) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM wakes WHERE task_id = ?1 OR subagent_id = ?1", [&task.0])?;
        Ok(())
    }

    /// A subagent's final words in its newest run (what it reports), at most `limit` bytes.
    pub fn final_words(&self, task: &TaskId) -> Result<String, StoreError> {
        let run = self.runs_of(task)?.pop();
        let parts = self.parts_of(task)?;
        let last = parts.iter().rev().find_map(|p| match &p.body {
            neko_protocol::PartBody::Agent { text, .. } if run.as_ref().is_none_or(|r| p.run.as_ref() == Some(&r.id)) && !text.trim().is_empty() => Some(text.trim().to_owned()),
            _ => None,
        });
        Ok(last.unwrap_or_default())
    }
}

impl Core {
    /// Makes the subagent `title` with `prompt` under `parent`: numbered, not started. Refused past the depth and live limits, with
    /// words the agent reads.
    pub fn create_subagent(&mut self, parent: &TaskId, title: &str, prompt: &str, how: NewSubagent) -> Result<(Task, u64), String> {
        let Some(p) = self.store.get_task(parent).map_err(|e| e.to_string())? else { return Err("This thread is gone.".into()) };
        let depth = p.subagent.as_ref().map_or(0, SubagentOf::depth);
        if depth >= MAX_DEPTH {
            return Err(format!("You are Subagent {}, {MAX_DEPTH} levels down: subagents may not start more. Do the work yourself.", p.subagent.as_ref().map(|s| s.number.as_str()).unwrap_or("")));
        }
        let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(80).collect();
        if title.is_empty() {
            return Err("A subagent needs a title (a few words).".into());
        }
        if prompt.trim().is_empty() {
            return Err("A subagent needs a prompt: it sees nothing else.".into());
        }
        if prompt.len() > neko_protocol::MAX_WORDS_BYTES {
            return Err("That prompt is too long. Put the long part in a file and name the file.".into());
        }
        let root = p.subagent.as_ref().map_or_else(|| p.id.clone(), |s| s.root.clone());
        let live = self.store.subagents_of(&root).map_err(|e| e.to_string())?.iter().filter(|t| is_live(t.status)).count();
        if live >= MAX_LIVE_PER_THREAD {
            return Err(format!("{live} subagents of this thread have not finished. Wait for their reports before starting more."));
        }
        let mut how = how;
        if let Some(after) = how.after.take() {
            let want = after.trim().trim_start_matches("Subagent").trim_start_matches("Kitten").trim();
            let siblings = self.store.children_of(&p.id).map_err(|e| e.to_string())?;
            let Some(sib) = siblings.iter().find(|c| c.subagent.as_ref().is_some_and(|s| s.number == want)) else {
                return Err(format!("You have no subagent {want} to start after. Use the number a start_subagent answer gave you (one you started yourself)."));
            };
            if sib.subagent.as_ref().is_some_and(|s| s.discarded) {
                return Err(format!("Subagent {want} was discarded by the person, so nothing can follow it."));
            }
            how.after = Some(want.to_owned());
        }
        let number = self.store.next_subagent_number(&p).map_err(|e| e.to_string())?;
        let now = self.now();
        let id = TaskId(self.new_id("task"));
        self.store.insert_subagent(&id, &p, &number, &title, prompt, now, &how).map_err(|e| e.to_string())
    }
}

// ---- What the agent is told ---------------------------------------------------------------------------------------------------

/// The paragraph added to every run's system text, after the cat's rules. The same bytes every time, so a resumed run's cached
/// prompt is not spoiled.
pub const PARAGRAPH: &str = "Neko can run parts of your work as subagents: separate agents, each with its own context. Most work needs none, so do it yourself. \
Split a part off only when it is independent of the rest and can be told in full, because a subagent sees nothing but the prompt you write. \
Start each one with the mcp__neko__start_subagent tool. After starting them, end your turn when you have nothing else to do. Do not poll or wait: Neko wakes you with each report as a \
message that begins with <neko-wake>. You cannot approve a subagent's steps, because it asks the person itself. A subagent that only reads can work in your folder (placement shared). \
One that writes should have a copy of its own (placement own_copy): a worktree on its own branch, started from your folder as it is now, uncommitted changes included. \
A subagent that must build on another's work starts after it (after: that one's number) and begins from its branch. Merge a subagent's branch into yours yourself (with a merge, after you commit \
your own changes) and say that you did; a stacked subagent's branch already holds the one before it. With draft true a subagent is only written down: the person starts it, or you call \
start_drafts. Your own Agent tool is still fine for a quick read-only search inside one run. Neko's app shows subagents to the person as kittens (Subagent 1 is Kitten 1), so you may use \
either word when you write to them.";

/// What a subagent is told about itself, after the paragraph. The same bytes on every run of that subagent. `branch` is the branch of
/// its own copy, when it works in one.
pub fn subagent_block(number: &str, thread_title: &str, branch: Option<&str>) -> String {
    let place = match branch {
        Some(b) => format!(" You work in a copy of the project of your own, on the branch {b}. Commit your work there (add everything, then commit) before you finish: the agent that started you merges the branch, and only committed work is on it."),
        None => " You work in the same folder as the agent that started you.".to_owned(),
    };
    format!(
        "You are Subagent {number} of \"{thread_title}\". The agent that started you gave you this work and nothing else. Your final message is your report: what you did, the files you changed, and \
anything it must know. It is read by the agent that started you, not by the person. If you are stuck, say so in your final message instead of guessing.{place}"
    )
}

/// Who a subagent is, for its own few lines: its number, its thread's title and the branch of its own copy.
pub type Of<'a> = (&'a str, &'a str, Option<&'a str>);

/// The whole text added to a run's system text for a task: the cat's own block, then the paragraph, then (for a subagent) its own.
pub fn instructions(cat_block: Option<&str>, subagent: Option<Of<'_>>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(b) = cat_block.filter(|b| !b.is_empty()) {
        parts.push(b.to_owned());
    }
    parts.push(PARAGRAPH.to_owned());
    if let Some((number, thread, branch)) = subagent {
        parts.push(subagent_block(number, thread, branch));
    }
    parts.join("\n\n")
}

/// What `initialize` says about Neko's server. It did not visibly reach the model in a real run (docs/knowledge/subagents.md), so the
/// same guidance is in the paragraph above and in each tool's result.
pub const SERVER_INSTRUCTIONS: &str = "Neko's own tools. start_subagent runs a numbered subagent and returns at once; end your turn afterwards and Neko wakes you with its report. \
subagents lists them and never waits: do not call it to wait.";

/// How a subagent just made stands, for the answer to `start_subagent`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Working now.
    Started,
    /// Waiting for room (the limits, or the Mac being warm).
    Queued,
    /// Waiting for the sibling it follows.
    After(String),
    /// Written down for the person to start (their choice, or the agent's).
    Draft { asked: bool },
}

/// What `start_subagent` answers: the agent's cue to end its turn. `branch` is given when it works in a copy of its own.
pub fn started_text(number: &str, title: &str, standing: &Standing, branch: Option<&str>) -> String {
    let place = branch.map(|b| format!(" It works in a copy of its own on the branch {b}.")).unwrap_or_default();
    let wake = " Neko wakes you with its report when it ends. Do not poll or wait. If you have nothing else to do, end your turn now.";
    let (state, end) = match standing {
        Standing::Started => ("is started. It is working now.".to_owned(), wake),
        Standing::Queued => ("is queued behind other work and starts when there is room.".to_owned(), wake),
        Standing::After(n) => (format!("is queued: it starts when Subagent {n} is done, from its branch."), wake),
        Standing::Draft { asked: true } => ("is saved as a draft: the person asked to approve the first subagent of a thread. It runs when they press Start.".to_owned(), " Tell them what it is for, then end your turn; you are woken with its report once it has run."),
        Standing::Draft { asked: false } => ("is saved as a draft. It does not run until the person starts it or you call start_drafts.".to_owned(), " Nothing waits on a draft, so say what it is for and end your turn, or call start_drafts when you are ready."),
    };
    format!("Subagent {number} (\"{title}\") {state}{place}{end}")
}

fn cap(s: &str, n: usize) -> String {
    if s.len() <= n {
        return s.to_owned();
    }
    let mut cut = n;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    s[..cut].to_owned()
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;")
}

/// The message that wakes a thread: one `<neko-wake>` for each report, a blank line between. A report's words past
/// [`SUMMARY_LIMIT`] are cut, with where to read the rest.
pub fn wake_message(wakes: &[WakeRow]) -> String {
    wakes
        .iter()
        .map(|w| {
            let mut summary = cap(&w.summary, SUMMARY_LIMIT);
            if summary.len() < w.summary.len() {
                summary.push_str("\n… Call mcp__neko__subagents with this id to read the rest.");
            }
            let mut m = format!("<neko-wake kind=\"subagent\" id=\"{}\" status=\"{}\" title=\"{}\">\n<summary>{}</summary>\n", w.number, w.status, escape(&w.title).replace('"', "&quot;"), escape(&summary));
            if !w.files.is_empty() {
                m.push_str(&format!("<files>{}</files>\n", escape(&w.files.join("; "))));
            }
            if let Some(b) = &w.branch {
                m.push_str(&format!("<branch>{}</branch>\n", escape(b)));
            }
            m.push_str("</neko-wake>");
            m
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

/// The tools of Neko's own server, as MCP's `tools/list` gives them. `depth` is how deep the asking run is: at the last level a
/// subagent may not start more, so it is not offered the tool.
pub fn tools(depth: usize) -> Vec<Value> {
    let mut list = Vec::new();
    if depth < MAX_DEPTH {
        list.push(json!({
            "name": "start_subagent",
            "description": "Run part of your work as a numbered subagent: a separate agent with its own context. It sees only the prompt, so write it in full. \
Returns at once. After starting subagents, end your turn when you have nothing else to do: Neko wakes you with each report. Do not poll. Most work needs no subagent. \
Use placement own_copy for anything that writes (a worktree on its own branch, started from your folder as it is now), and shared only to read. \
With after, it starts when that subagent is done, from its branch. With draft true it is only written down for the person to start.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "title": {"type": "string", "description": "A few words naming the work."},
                    "prompt": {"type": "string", "description": "Everything the subagent needs, in full."},
                    "placement": {"type": "string", "enum": ["shared", "own_copy"], "description": "Where it works: shared (your folder, to read) or own_copy (a worktree of its own, to write). Default shared."},
                    "after": {"type": "string", "description": "The number of a subagent you started: this one waits for it and starts from its branch."},
                    "draft": {"type": "boolean", "description": "Only write it down: the person starts it, or you call start_drafts."}
                },
                "required": ["title", "prompt"],
                "additionalProperties": false
            }
        }));
        list.push(json!({
            "name": "start_drafts",
            "description": "Starts the drafts you wrote with start_subagent (draft true). Returns their numbers at once. Then end your turn: Neko wakes you with each report.",
            "inputSchema": { "type": "object", "properties": {}, "additionalProperties": false }
        }));
    }
    list.push(json!({
        "name": "subagents",
        "description": "Lists the subagents you started: number, title, state and last line. Read-only and instant. Do not call it to wait for them: Neko wakes you when they report. With id, returns that report in full.",
        "inputSchema": {
            "type": "object",
            "properties": { "id": {"type": "string", "description": "A subagent's number, to read its whole report."} },
            "additionalProperties": false
        }
    }));
    list.push(json!({
        "name": "stop_subagent",
        "description": "Stops a subagent you started, and any it started. You are woken with its report, which says it was stopped.",
        "inputSchema": {
            "type": "object",
            "properties": { "id": {"type": "string", "description": "The subagent's number."} },
            "required": ["id"],
            "additionalProperties": false
        }
    }));
    list
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wake(n: &str, status: &str, summary: &str) -> WakeRow {
        WakeRow { id: format!("w{n}"), task: TaskId("t".into()), subagent: TaskId(format!("s{n}")), number: n.into(), title: "Check links".into(), status: status.into(), summary: summary.into(), files: vec![], branch: None, created_ms: 0 }
    }

    #[test]
    fn a_wake_carries_each_report_in_its_own_element() {
        let mut a = wake("1", "done", "Fixed 2 links <fast>.");
        a.files = vec!["src/auth.rs +12 -3".into()];
        a.branch = Some("neko/4f2a".into());
        let m = wake_message(&[a, wake("2", "failed", "Could not run the tests.")]);
        assert!(m.starts_with("<neko-wake kind=\"subagent\" id=\"1\" status=\"done\" title=\"Check links\">\n<summary>Fixed 2 links &lt;fast>.</summary>\n<files>src/auth.rs +12 -3</files>\n<branch>neko/4f2a</branch>\n</neko-wake>\n\n<neko-wake"), "{m}");
        assert!(m.contains("id=\"2\" status=\"failed\""));
    }

    #[test]
    fn a_long_report_is_cut_with_where_to_read_the_rest() {
        let m = wake_message(&[wake("1", "done", &"x".repeat(SUMMARY_LIMIT + 50))]);
        assert!(m.contains("Call mcp__neko__subagents with this id"));
        assert!(m.len() < SUMMARY_LIMIT + 400);
    }

    #[test]
    fn a_branch_is_the_threads_short_id_and_the_number() {
        assert_eq!(branch_name(&TaskId("task_01KAB0000000000000ABCDWXYZ".into()), "1"), "neko/abcdwxyz-1");
        assert_eq!(branch_name(&TaskId("task_01KAB0000000000000ABCDWXYZ".into()), "1.2"), "neko/abcdwxyz-1-2");
    }

    #[test]
    fn a_subagent_at_the_last_level_is_not_offered_the_tool_to_start_more() {
        let names = |d| tools(d).iter().map(|t| t["name"].as_str().unwrap().to_owned()).collect::<Vec<_>>();
        assert_eq!(names(0), ["start_subagent", "start_drafts", "subagents", "stop_subagent"]);
        assert_eq!(names(MAX_DEPTH - 1), ["start_subagent", "start_drafts", "subagents", "stop_subagent"]);
        assert_eq!(names(MAX_DEPTH), ["subagents", "stop_subagent"]);
    }

    #[test]
    fn the_text_is_the_same_bytes_every_time() {
        assert_eq!(instructions(Some("Rules."), Some(("1.2", "Fix login", None))), instructions(Some("Rules."), Some(("1.2", "Fix login", None))));
        assert!(instructions(None, None).starts_with("Neko can run parts of your work"));
        assert!(instructions(Some("Rules."), None).starts_with("Rules.\n\nNeko can run"));
        assert!(instructions(None, Some(("1", "Fix login", None))).ends_with("the same folder as the agent that started you."));
        let own = instructions(None, Some(("1", "Fix login", Some("neko/4f2a-1"))));
        assert!(own.contains("on the branch neko/4f2a-1") && own.contains("Commit your work there"));
    }

    #[test]
    fn the_result_of_starting_tells_the_agent_to_end_its_turn() {
        assert!(started_text("1", "Check links", &Standing::Started, None).contains("end your turn now"));
        assert!(started_text("1", "Check links", &Standing::Queued, None).contains("queued"));
        let own = started_text("2", "Write tests", &Standing::After("1".into()), Some("neko/4f2a-2"));
        assert!(own.contains("starts when Subagent 1 is done") && own.contains("on the branch neko/4f2a-2"), "{own}");
        assert!(started_text("3", "Later", &Standing::Draft { asked: false }, None).contains("start_drafts"));
        assert!(started_text("1", "First", &Standing::Draft { asked: true }, None).contains("press Start"));
    }
}
