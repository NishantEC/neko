//! The daemon end of the self-improvement loop (`neko_core::improve`): the idle thread that
//! gathers signals and asks the cheapest model for suggestions, the `Request::Improve` handler,
//! and the real effects (packs, tasks, checkpoints) behind apply and undo.
//! See docs/knowledge/self-improvement.md.
use crate::server::AppState;
use neko_core::improve::{self, signals, Effects, World};
use neko_core::inbox;
use neko_protocol::goals::GoalCommand;
use neko_protocol::improve::ImproveCommand;
use neko_protocol::workbench::{Command, Snapshot, TaskStatus};
use neko_protocol::Response;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How often the thread looks at whether a pass is due and the Mac is idle.
const WAKE: Duration = Duration::from_secs(60);
/// What the model is given at most per run of text it is read from.
const MAX_TEXT: usize = 16 * 1024;
/// How far back runs, turns and learned corrections are read.
const LOOKBACK_MS: i64 = 14 * improve::DAY_MS;

static EVENT: AtomicBool = AtomicBool::new(false);

/// A run failed (or a turn did): gather soon, instead of waiting for the next 6 hours. Cheap, and
/// does nothing while Suggestions are off (the thread checks).
pub fn note_event() {
    EVENT.store(true, Ordering::Relaxed);
}

pub fn spawn(state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("neko-improve".into())
        .spawn(move || loop {
            std::thread::sleep(WAKE);
            let now = neko_core::workbench::now_ms();
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tick(&state, now))) {
                Ok(Err(error)) => eprintln!("neko improve: {error}"),
                Err(_) => eprintln!("neko improve: panicked; continuing"),
                Ok(Ok(_)) => {}
            }
        })
        .map(|_| ())
        .unwrap_or_else(|error| eprintln!("neko improve: cannot start: {error}"));
}

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, neko_core::Db> {
    state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// One look. Verifying what was applied is cheap and always happens while Suggestions are on; a
/// gathering pass needs the Mac idle (nothing held, no goal, no worker) and is due every 6 hours or
/// after an event (at most every 10 minutes). Returns how many suggestions were made.
pub(crate) fn tick(state: &AppState, now: i64) -> Result<usize, String> {
    if !improve::enabled(&lock(state)) {
        return Ok(0);
    }
    let fx = Real { state };
    improve::verify(&state.db, &fx, &neko_protocol::support_dir(), now)?;
    let (gather, event) = {
        let db = lock(state);
        (signals::gather_due(&db, now), EVENT.load(Ordering::Relaxed) && signals::event_due(&db, now))
    };
    if !(gather || event) || !state.workbench.idle_for_curator()? {
        return Ok(0);
    }
    EVENT.store(false, Ordering::Relaxed);
    let made = gather_pass(state, now, &|prompt| crate::workbench::ask_cheap_model(&state.db, prompt))?;
    {
        let db = lock(state);
        if gather {
            signals::note_gather(&db, now)?;
        } else {
            signals::note_event(&db, now)?;
        }
    }
    Ok(made)
}

/// The pass itself, with the model behind `ask` so tests need none.
pub(crate) fn gather_pass(state: &AppState, now: i64, ask: &dyn Fn(&str) -> Result<String, String>) -> Result<usize, String> {
    if !improve::enabled(&lock(state)) {
        return Ok(0);
    }
    // Listing packs takes the database lock itself, so it is done before this one is held.
    let packs: Vec<(String, String)> = neko_core::packs::list(&state.db, &neko_core::packs::Paths::system(), &neko_core::packs::Env::system()).map(|l| l.catalog.into_iter().map(|c| (c.info.name, c.info.description)).collect()).unwrap_or_default();
    let (inputs, world) = {
        let db = lock(state);
        let snapshot = neko_core::workbench::load(&db)?;
        (inputs_of(&db, &snapshot, now), world_of(packs, &db, &snapshot))
    };
    Ok(improve::run_pass(&state.db, &inputs, &world, ask, now)?.len())
}

fn world_of(packs: Vec<(String, String)>, db: &neko_core::Db, snapshot: &Snapshot) -> World {
    World {
        packs,
        dev_checkout: dev_checkout_of(snapshot).is_some(),
        default_workspace: snapshot.workspaces.first().map(|w| w.id.clone()).unwrap_or_default(),
        orders: neko_core::routines::load(db).map(|s| s.standing_orders).unwrap_or_default(),
    }
}

fn dev_checkout_of(snapshot: &Snapshot) -> Option<String> {
    snapshot.self_update.dev_checkout.clone().filter(|d| !d.trim().is_empty() && Path::new(d).is_dir())
}

fn cut(text: &str) -> String {
    text.chars().take(MAX_TEXT).collect()
}

/// What the signals are made from: failed runs and tasks, chat turns, what was learned as a
/// correction, and Inbox items (suggestions and goal questions are not asks).
fn inputs_of(db: &neko_core::Db, snapshot: &Snapshot, now: i64) -> signals::Inputs {
    use signals::Seen;
    let recent = |at: i64| now - at < LOOKBACK_MS;
    let mut inputs = signals::Inputs::default();
    for task in snapshot.tasks.iter().filter(|t| t.status == TaskStatus::Failed && recent(t.updated_at_ms)) {
        let events: Vec<&str> = task.events.iter().rev().take(40).map(|e| e.message.as_str()).collect();
        inputs.failures.push(Seen { id: format!("task:{}", task.id), at_ms: task.updated_at_ms, text: cut(&format!("{}\n{}", events.join("\n"), task.result)), workspace_id: task.workspace_id.clone() });
    }
    for message in snapshot.conversation.iter().filter(|m| m.role == neko_protocol::workbench::ChatRole::Neko && !m.pending && recent(m.at_ms)).rev().take(200) {
        inputs.failures.push(Seen { id: format!("chat:{}", message.id), at_ms: message.at_ms, text: cut(&message.text()), workspace_id: message.workspace_id.clone().unwrap_or_default() });
    }
    if let Ok(store) = neko_core::routines::load(db) {
        for routine in &store.routines {
            for run in neko_core::routines::load_runs(db, &routine.id).unwrap_or_default().into_iter().filter(|r| !r.ok && recent(r.started_ms)) {
                inputs.failures.push(Seen { id: format!("run:{}", run.id), at_ms: run.started_ms, text: cut(&format!("{}\n{}", run.error.clone().unwrap_or_default(), run.text)), workspace_id: routine.workspace_id.clone() });
            }
            if routine.id.starts_with(improve::ROUTINE_PREFIX) || routine.id.starts_with("goals:") {
                continue;
            }
            for run in neko_core::routines::load_runs(db, &routine.id).unwrap_or_default() {
                for item in run.needs_you.iter().filter(|i| i.created_ms > 0 && recent(i.created_ms)) {
                    inputs.asks.push(Seen { id: format!("{}", item.seq), at_ms: item.created_ms, text: item.title.clone(), workspace_id: routine.workspace_id.clone() });
                }
            }
        }
    }
    // Corrections: what memory learning proposed or kept, and what ticket lessons proposed.
    for p in neko_core::memory_learning::proposals(db, snapshot).unwrap_or_default().into_iter().filter(|p| recent(p.created_at_ms)) {
        inputs.corrections.push(Seen { id: format!("memprop:{}", p.id), at_ms: p.created_at_ms, text: p.text, workspace_id: p.workspace_id.unwrap_or_default() });
    }
    for e in neko_core::neko_memory::load(db).unwrap_or_default().into_iter().filter(|e| e.source != "user" && recent(e.created_at_ms)) {
        inputs.corrections.push(Seen { id: format!("mem:{}", e.id), at_ms: e.created_at_ms, text: e.text, workspace_id: e.workspace_id.unwrap_or_default() });
    }
    if let Ok(skills) = neko_core::skills::load(db) {
        // A lesson proposal has no date of its own: it is dated when first seen.
        for p in skills.proposals.iter().filter(|p| p.source.starts_with("Completed ticket")) {
            inputs.corrections.push(Seen { id: format!("lesson:{}", p.id), at_ms: now, text: p.name.clone(), workspace_id: p.workspace_id.clone() });
        }
    }
    inputs
}

// ---------------------------------------------------------------------------
// Effects
// ---------------------------------------------------------------------------

struct Real<'a> {
    state: &'a AppState,
}

impl Real<'_> {
    fn snapshot(&self) -> Result<Snapshot, String> {
        neko_core::workbench::load(&lock(self.state))
    }

    fn listing(&self) -> Option<neko_protocol::packs::PackListing> {
        neko_core::packs::list(&self.state.db, &neko_core::packs::Paths::system(), &neko_core::packs::Env::system()).ok()
    }

    /// A goal's ticket, found by the objective it was made with.
    fn make_task(&self, workspace_id: &str, objective: &str, done_check: &str) -> Result<String, String> {
        let after = self.state.workbench.command(Command::Goals(GoalCommand::Create { workspace_id: workspace_id.to_owned(), objective: objective.to_owned(), done_check: done_check.to_owned(), constraints: String::new(), max_turns: None, ticket_id: None, conversation_id: None }))?;
        after.goals.iter().filter(|g| g.objective == objective && g.workspace_id == workspace_id).max_by_key(|g| g.created_ms).and_then(|g| g.ticket_id.clone()).ok_or_else(|| "The task was not created".to_owned())
    }
}

impl Effects for Real<'_> {
    fn catalog_has(&self, pack: &str) -> bool {
        self.listing().is_some_and(|l| l.catalog.iter().any(|c| c.info.name == pack))
    }

    fn pack_installed(&self, pack: &str) -> bool {
        self.listing().is_some_and(|l| l.installed.iter().any(|p| p.info.name == pack))
    }

    fn remove_pack(&self, pack: &str) -> Result<(), String> {
        neko_core::packs::remove(&self.state.db, &neko_core::packs::Paths::system(), pack)
    }

    fn dev_target(&self) -> Result<(String, String), String> {
        let snapshot = self.snapshot()?;
        let checkout = dev_checkout_of(&snapshot).ok_or("Set a Development checkout in Settings, Neko updates, before Neko makes a change to its own code")?;
        let canonical = |p: &str| std::fs::canonicalize(p).unwrap_or_else(|_| p.into());
        let workspace = snapshot.workspaces.iter().find(|w| canonical(&w.repository) == canonical(&checkout)).ok_or("The Development checkout is not a workspace yet; add it as one first")?;
        let check = if Path::new(&checkout).join("scripts/check.sh").is_file() { "bash scripts/check.sh" } else { "" };
        Ok((workspace.id.clone(), check.to_owned()))
    }

    fn create_task(&self, workspace_id: &str, objective: &str, done_check: &str) -> Result<String, String> {
        self.make_task(workspace_id, objective, done_check)
    }

    fn undo_task(&self, task_id: &str, title: &str) -> Result<String, String> {
        let snapshot = self.snapshot()?;
        let task = snapshot.tasks.iter().find(|t| t.id == task_id).ok_or("That task no longer exists")?;
        let list = self.state.workbench.checkpoint_list(task_id, false).unwrap_or_default();
        if list.busy {
            return Err("The task is still running; stop it first, then undo".into());
        }
        let Some(first) = list.runs.iter().filter(|r| !r.undone).map(|r| r.n).min() else {
            // Nothing ran, so there is nothing to put back: the task is cancelled instead.
            if matches!(task.status, TaskStatus::Queued | TaskStatus::AwaitingApproval) {
                self.state.workbench.command(Command::CancelTask { task_id: task_id.to_owned() })?;
                return Ok("Cancelled the task, which had not run yet".into());
            }
            return Err("The task has no runs to undo".into());
        };
        let undo = |snapshot_unsaved: bool| self.state.workbench.command(Command::Checkpoints(neko_protocol::checkpoints::CheckpointCommand::Undo { task_id: task_id.to_owned(), run: first, through_later: true, snapshot_unsaved, revert: false }));
        let mut result = undo(false);
        if matches!(&result, Err(m) if m.starts_with("checkpoint:unsaved_changes")) {
            result = undo(true);
        }
        match result {
            Ok(_) => Ok("Undid the task's runs through their checkpoints".into()),
            Err(message) if message.starts_with("checkpoint:busy") => Err("The task is still running; stop it first, then undo".into()),
            // The work was pushed, or others committed after it, or history moved: a revert task, never a rewrite.
            Err(message) if ["checkpoint:pushed", "checkpoint:foreign_commits", "checkpoint:failed"].iter().any(|c| message.starts_with(c)) => {
                let objective = format!("Revert the change that the task \"{title}\" made (task {task_id}): find its commits on the branch or pull request, add commits that undo them without rewriting history, and run the repository's checks.");
                self.make_task(&task.workspace_id, &objective, "")?;
                Ok("The change was already pushed, so a revert task was started".into())
            }
            Err(message) => Err(message),
        }
    }
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// `Request::Improve`. Every apply, reject and undo here is a click (or an accepted Inbox default).
pub fn handle(state: &AppState, command: ImproveCommand) -> Response {
    let now = neko_core::workbench::now_ms();
    let fx = Real { state };
    let data = neko_protocol::support_dir();
    let (mut open_pack, mut note) = (None, None);
    let done: Result<(), String> = match command {
        ImproveCommand::List => improve::verify(&state.db, &fx, &data, now).map(|_| ()),
        ImproveCommand::Apply { id } => improve::apply(&state.db, &fx, &data, &id, now, inbox::ANSWERED).map(|a| {
            open_pack = a.open_pack;
            note = Some(a.note);
        }),
        ImproveCommand::Reject { id } => improve::reject(&state.db, &data, &id, now, inbox::BY_YOU),
        ImproveCommand::Undo { id } => improve::undo(&state.db, &fx, &data, &id, now, inbox::ANSWERED).map(|said| note = Some(said)),
        ImproveCommand::SetEnabled { enabled } => improve::set_enabled(&lock(state), enabled),
    };
    if let Err(message) = done {
        return Response::Error { message };
    }
    let db = lock(state);
    if let Ok(snapshot) = neko_core::workbench::load(&db) {
        // A skill a suggestion changed or removed shows up in Skills right away.
        let _ = neko_core::skills::refresh(&db, &snapshot.workspaces);
        let mut reply = improve::view(&db, now, dev_checkout_of(&snapshot).is_some());
        (reply.open_pack, reply.note) = (open_pack, note);
        return Response::Improve(reply);
    }
    let mut reply = improve::view(&db, now, false);
    (reply.open_pack, reply.note) = (open_pack, note);
    Response::Improve(reply)
}

/// The Inbox default of a suggestion item is its apply action; of a "That didn't help" item, Undo.
pub fn answer_default(state: &AppState, link: &str, closed_as: &str) -> Result<String, String> {
    let now = neko_core::workbench::now_ms();
    let fx = Real { state };
    let data = neko_protocol::support_dir();
    if let Some(id) = link.strip_prefix(improve::UNDO_LINK) {
        return improve::undo(&state.db, &fx, &data, id, now, closed_as);
    }
    let id = link.strip_prefix(improve::LINK).ok_or("Not a suggestion")?;
    let applied = improve::apply(&state.db, &fx, &data, id, now, closed_as)?;
    let db = lock(state);
    if let Ok(snapshot) = neko_core::workbench::load(&db) {
        let _ = neko_core::skills::refresh(&db, &snapshot.workspaces);
    }
    Ok(match applied.open_pack {
        Some(source) => format!("{} Find it under Setup, Improvements ({source}).", applied.note),
        None => applied.note,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::improve::ImproveState;

    fn state() -> AppState {
        crate::server::tests::test_state_for_phone()
    }

    #[test]
    fn the_idle_pass_makes_a_suggestion_from_a_failed_run_with_a_stub_model_and_delivers_one_item() {
        let state = state();
        let now = 50 * improve::DAY_MS;
        let mut snapshot = neko_core::workbench::load(&lock(&state)).unwrap();
        snapshot.workspaces.push(neko_protocol::workbench::Workspace { id: "w1".into(), name: "Home".into(), repository: "/tmp/not-real".into(), instructions: String::new(), away_enabled: false });
        neko_core::workbench::save(&lock(&state), &snapshot).unwrap();
        // A routine run that failed because a command is missing.
        {
            let db = lock(&state);
            let mut store = neko_core::routines::load(&db).unwrap();
            store.routines.push(neko_protocol::routines::Routine {
                id: "r1".into(),
                workspace_id: "w1".into(),
                kind: neko_protocol::routines::RoutineKind::Custom,
                name: "Brief".into(),
                instruction: String::new(),
                cadence: neko_protocol::routines::Cadence::Daily { hour: 9, minute: 0 },
                enabled: false,
                model: String::new(),
                paseo_schedule: None,
                browser: false,
                computer: false,
                customized: true,
                catch_up: None,
            });
            neko_core::routines::save_definitions(&db, &store).unwrap();
            neko_core::routines::push_run(&db, "r1", neko_protocol::routines::RoutineRun { id: "run1".into(), routine_id: "r1".into(), started_ms: now - 1000, ok: false, error: Some("zsh: command not found: rg".into()), ..Default::default() }).unwrap();
        }
        let answer = r#"{"kind":"standing_order","title":"Tell agents where rg lives","why":"A run could not find rg.","change":"When a command is not found, look in /opt/homebrew/bin before giving up.","default_label":"Add this standing order"}"#;
        let made = gather_pass(&state, now, &|prompt: &str| {
            assert!(prompt.contains("missing_tool:rg") && prompt.contains("command not found: rg"));
            Ok(answer.to_owned())
        })
        .unwrap();
        assert_eq!(made, 1);
        let reply = improve::view(&lock(&state), now, false);
        assert_eq!(reply.proposals.len(), 1);
        assert_eq!((reply.proposals[0].state, reply.proposals[0].inbox_seq.is_some()), (ImproveState::Open, true));
        // Asked again: the key already has an open suggestion, so the model is not asked at all.
        assert_eq!(gather_pass(&state, now + 1000, &|_: &str| panic!("asked again")).unwrap(), 0);
    }
}
