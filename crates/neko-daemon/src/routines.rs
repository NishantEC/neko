//! Runs routines (`neko_core::routines`): one thread wakes every 30 seconds
//! and starts the most overdue enabled routine, at most one at a time, never
//! while the machine steward holds new work. A run is Claude Code in the
//! scope's folder with the owner's full setup, so it has that folder's MCP
//! servers and sign-ins. It acts, then its reply's json block becomes a
//! `RoutineRun`. See docs/knowledge/neko-runs-the-day.md.
use crate::server::AppState;
use neko_core::routines::{self as core, Store};
use neko_protocol::routines::{RoutineNotice, RoutineRun, RoutineState, RoutineView, ScopeProposal};
use neko_protocol::workbench::{AgentRuntime, Command, Workspace};
use neko_protocol::{Request, Response};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, MutexGuard};
use std::time::Duration;

static BUSY: AtomicBool = AtomicBool::new(false);
const TICK: Duration = Duration::from_secs(30);
const RUN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
const KEEP_TEXT: usize = 4000;
const VIEW_RUNS: usize = 10;

fn db(state: &AppState) -> MutexGuard<'_, neko_core::Db> {
    state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn claude_json(home: &Path) -> Option<serde_json::Value> {
    let text = std::fs::read_to_string(home.join(".claude.json")).ok()?;
    serde_json::from_str(&text).ok()
}

/// Scopes on disk matched to workspaces, plus every other workspace as a
/// scope of its own. With `create`, a scope without a workspace gets one.
fn scopes(state: &AppState, create: bool) -> Result<Vec<ScopeProposal>, String> {
    let home = home();
    let mut proposals = neko_core::scopes::discover(&home, claude_json(&home).as_ref());
    let mut snapshot = neko_core::workbench::load(&db(state))?;
    let folders_of = |snapshot: &neko_protocol::workbench::Snapshot, id: &str, repository: &str| {
        snapshot.workspace_folders.get(id).cloned().unwrap_or_else(|| vec![repository.to_string()])
    };
    for proposal in &mut proposals {
        let folder = proposal.folders[0].clone();
        let existing = snapshot
            .workspaces
            .iter()
            .find(|w| folders_of(&snapshot, &w.id, &w.repository).contains(&folder))
            .map(|w| w.id.clone());
        proposal.workspace_id = match existing {
            Some(id) => Some(id),
            None if create => {
                let workspace = Workspace {
                    id: neko_core::workbench::new_id(),
                    name: proposal.name.clone(),
                    repository: folder.clone(),
                    instructions: String::new(),
                    away_enabled: false,
                };
                let id = workspace.id.clone();
                snapshot = state.workbench.command(Command::SaveWorkspaceWithFolders { workspace, folders: vec![folder] })?;
                Some(id)
            }
            None => None,
        };
    }
    for workspace in &snapshot.workspaces {
        if proposals.iter().any(|p| p.workspace_id.as_deref() == Some(workspace.id.as_str())) {
            continue;
        }
        proposals.push(ScopeProposal {
            name: workspace.name.clone(),
            folders: folders_of(&snapshot, &workspace.id, &workspace.repository),
            servers: Vec::new(),
            problems: Vec::new(),
            workspace_id: Some(workspace.id.clone()),
        });
    }
    Ok(proposals)
}

/// Creates the scopes' workspaces and their three routines once.
fn ensure_seeded(state: &AppState) -> Result<(), String> {
    if core::load(&db(state))?.seeded {
        return Ok(());
    }
    let scopes = scopes(state, true)?;
    let db = db(state);
    let mut store = core::load(&db)?;
    let now = neko_core::workbench::now_ms();
    for routine in core::seeds(&scopes) {
        if !store.routines.iter().any(|r| r.id == routine.id) {
            // First runs follow the cadence, so seeding never starts a burst.
            let first = RoutineState { next_due_ms: Some(core::next_due_local(&routine.cadence, 0, now)), ..Default::default() };
            core::save_state(&db, &routine.id, &first)?;
            store.routines.push(routine);
        }
    }
    store.seeded = true;
    core::save_definitions(&db, &store)
}

/// At start no run is in flight, whatever the rows say.
fn clear_stale_running(db: &neko_core::Db) -> Result<(), String> {
    for routine in core::load(db)?.routines {
        let mut state = core::load_state(db, &routine.id)?;
        if state.running {
            state.running = false;
            state.next_due_ms = None;
            core::save_state(db, &routine.id, &state)?;
        }
    }
    Ok(())
}

/// The enabled routine that is most overdue, if any is due.
pub(crate) fn pick_due(store: &Store, states: &[(String, RoutineState)], now_ms: i64) -> Option<String> {
    store
        .routines
        .iter()
        .filter(|r| r.enabled)
        .filter_map(|r| {
            let state = states.iter().find(|(id, _)| id == &r.id).map(|(_, s)| s);
            if state.is_some_and(|s| s.running) {
                return None;
            }
            let due = state.and_then(|s| s.next_due_ms).unwrap_or(0);
            (due <= now_ms).then_some((due, r.id.clone()))
        })
        .min()
        .map(|(_, id)| id)
}

pub fn spawn(state: Arc<AppState>) {
    if let Err(error) = clear_stale_running(&db(&state)) {
        eprintln!("routines: {error}");
    }
    std::thread::spawn(move || loop {
        std::thread::sleep(TICK);
        if let Err(error) = tick(&state) {
            eprintln!("routines: {error}");
        }
    });
}

fn tick(state: &Arc<AppState>) -> Result<(), String> {
    if BUSY.load(Ordering::SeqCst) {
        return Ok(());
    }
    if neko_core::machine::latest().is_some_and(|health| health.holding.is_some()) {
        return Ok(());
    }
    ensure_seeded(state)?;
    let now = neko_core::workbench::now_ms();
    let (routine, scope) = {
        let store = core::load(&db(state))?;
        let states = store
            .routines
            .iter()
            .map(|r| Ok((r.id.clone(), core::load_state(&db(state), &r.id)?)))
            .collect::<Result<Vec<_>, String>>()?;
        let Some(id) = pick_due(&store, &states, now) else { return Ok(()) };
        let routine = store.routines.iter().find(|r| r.id == id).cloned().ok_or("Routine vanished")?;
        let scope = scopes(state, false)?.into_iter().find(|s| s.workspace_id.as_deref() == Some(routine.workspace_id.as_str()));
        (routine, scope)
    };
    {
        let db = db(state);
        let mut routine_state = core::load_state(&db, &routine.id)?;
        routine_state.running = true;
        core::save_state(&db, &routine.id, &routine_state)?;
    }
    BUSY.store(true, Ordering::SeqCst);
    let state = state.clone();
    std::thread::spawn(move || {
        let result = run_one(&state, &routine, scope);
        if let Err(error) = result {
            eprintln!("routine {}: {error}", routine.id);
        }
        BUSY.store(false, Ordering::SeqCst);
    });
    Ok(())
}

fn run_one(state: &AppState, routine: &neko_protocol::routines::Routine, scope: Option<ScopeProposal>) -> Result<(), String> {
    let started = neko_core::workbench::now_ms();
    let (orders, memory) = {
        let db = db(state);
        (core::load(&db)?.orders_for(&routine.workspace_id).to_string(), core::load_state(&db, &routine.id)?.memory)
    };
    let outcome: Result<(String, core::Parsed), (String, String)> = match scope {
        None => Err(("This routine's workspace no longer exists".into(), String::new())),
        Some(scope) => {
            let folder = PathBuf::from(&scope.folders[0]);
            if !folder.is_dir() {
                Err((format!("Scope folder missing: {}", folder.display()), String::new()))
            } else {
                let spec = neko_core::native_runner::RunSpec {
                    directory: folder,
                    prompt: core::build_prompt(routine, &scope, &orders, &memory, &core::now_iso()),
                    writable: true,
                    timeout: RUN_TIMEOUT,
                    runtime: AgentRuntime {
                        provider: "claude".into(),
                        model: if routine.model.is_empty() { "sonnet".into() } else { routine.model.clone() },
                        ..Default::default()
                    },
                };
                let cancel = AtomicBool::new(false);
                match neko_core::native_runner::run(&spec, &cancel, |_| {}) {
                    Ok(text) => match core::parse_reply(&text) {
                        Ok(parsed) => Ok((text, parsed)),
                        Err(error) => Err((error, text)),
                    },
                    Err(error) => Err((error, String::new())),
                }
            }
        }
    };
    let finished = neko_core::workbench::now_ms();
    let db = db(state);
    let mut routine_state = core::load_state(&db, &routine.id)?;
    let run = match outcome {
        Ok((_, parsed)) => {
            let mut needs_you = parsed.needs_you;
            if !needs_you.is_empty() {
                let first = core::reserve_seq(&db, needs_you.len() as u64)?;
                for (offset, item) in needs_you.iter_mut().enumerate() {
                    item.seq = first + offset as u64;
                }
            }
            routine_state.quiet_streak = if parsed.quiet { routine_state.quiet_streak.saturating_add(1) } else { 0 };
            routine_state.memory = parsed.memory;
            routine_state.note = None;
            RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: started, finished_ms: Some(finished), ok: true, quiet: parsed.quiet, did: parsed.did, needs_you, error: None, text: String::new() }
        }
        Err((error, text)) => {
            let mut end = text.len().min(KEEP_TEXT);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: started, finished_ms: Some(finished), ok: false, quiet: false, did: Vec::new(), needs_you: Vec::new(), error: Some(error), text: text[..end].to_string() }
        }
    };
    let ok = run.ok;
    core::push_run(&db, &routine.id, run)?;
    routine_state.running = false;
    routine_state.last_run_ms = Some(finished);
    routine_state.next_due_ms = Some(core::next_due_local(&routine.cadence, routine_state.quiet_streak, finished));
    core::save_state(&db, &routine.id, &routine_state)?;
    drop(db);
    if ok && !routine_state.paseo_paused {
        if let Some(name) = &routine.paseo_schedule {
            let note = pause_paseo(name);
            let db = self::db(state);
            let mut routine_state = core::load_state(&db, &routine.id)?;
            routine_state.paseo_paused = note.is_ok();
            routine_state.note = Some(note.unwrap_or_else(|e| e));
            core::save_state(&db, &routine.id, &routine_state)?;
        }
    }
    Ok(())
}

/// Pauses the Paseo schedule this routine replaces, found by name.
fn pause_paseo(name: &str) -> Result<String, String> {
    let dir = home().join(".paseo/schedules");
    let id = std::fs::read_dir(&dir)
        .map_err(|e| format!("Paseo schedules unreadable: {e}"))?
        .flatten()
        .find_map(|entry| {
            let value: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(entry.path()).ok()?).ok()?;
            (value["name"] == name && value["status"] == "active").then(|| value["id"].as_str().map(str::to_string)).flatten()
        });
    let Some(id) = id else { return Ok(format!("Paseo schedule {name} was not active")) };
    let paseo = ["/.local/bin/paseo", "/.npm-global/bin/paseo"]
        .iter()
        .map(|p| PathBuf::from(format!("{}{p}", home().display())))
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("paseo"));
    let output = std::process::Command::new(paseo)
        .args(["schedule", "pause", &id])
        .output()
        .map_err(|e| format!("Could not pause Paseo schedule {name}: {e}"))?;
    if output.status.success() {
        Ok(format!("Paused Paseo schedule {name}; Neko runs it now"))
    } else {
        Err(format!("Could not pause Paseo schedule {name}: {}", String::from_utf8_lossy(&output.stderr).trim()))
    }
}

fn views(state: &AppState) -> Result<Response, String> {
    ensure_seeded(state)?;
    let scopes = scopes(state, false)?;
    let db = db(state);
    let store = core::load(&db)?;
    let mut routines = Vec::new();
    for routine in &store.routines {
        let mut runs = core::load_runs(&db, &routine.id)?;
        runs.truncate(VIEW_RUNS);
        routines.push(RoutineView { routine: routine.clone(), state: core::load_state(&db, &routine.id)?, runs });
    }
    let mut orders = store.standing_orders.clone();
    for scope in &scopes {
        if let Some(id) = &scope.workspace_id {
            orders.entry(id.clone()).or_insert_with(|| store.orders_for(id).to_string());
        }
    }
    Ok(Response::Routines { routines, standing_orders: orders, scopes })
}

fn notifications(state: &AppState, after: u64) -> Result<Response, String> {
    let scopes = scopes(state, false)?;
    let db = db(state);
    let store = core::load(&db)?;
    let latest = core::latest_seq(&db)?;
    let mut items = Vec::new();
    if after < latest {
        for routine in &store.routines {
            let scope = scopes.iter().find(|s| s.workspace_id.as_deref() == Some(routine.workspace_id.as_str())).map(|s| s.name.clone()).unwrap_or_default();
            for run in core::load_runs(&db, &routine.id)? {
                for item in run.needs_you.into_iter().filter(|i| i.seq > after && !i.done) {
                    items.push(RoutineNotice { scope: scope.clone(), routine: routine.name.clone(), item });
                }
            }
        }
    }
    items.sort_by_key(|notice| notice.item.seq);
    Ok(Response::RoutineNotifications { items, latest })
}

fn edit(state: &AppState, request: Request) -> Result<(), String> {
    let db = db(state);
    let mut store = core::load(&db)?;
    match request {
        Request::SaveRoutine { routine } => {
            if routine.id.trim().is_empty() || routine.name.trim().is_empty() || routine.instruction.trim().is_empty() {
                return Err("A routine needs a name and an instruction".into());
            }
            match store.routines.iter_mut().find(|r| r.id == routine.id) {
                Some(existing) => *existing = routine,
                None => store.routines.push(routine),
            }
            core::save_definitions(&db, &store)
        }
        Request::RunRoutineNow { id } => {
            let mut routine_state = core::load_state(&db, &id)?;
            routine_state.next_due_ms = Some(0);
            core::save_state(&db, &id, &routine_state)
        }
        Request::SetRoutineEnabled { id, enabled } => {
            let routine = store.routines.iter_mut().find(|r| r.id == id).ok_or("No such routine")?;
            routine.enabled = enabled;
            core::save_definitions(&db, &store)
        }
        Request::SetStandingOrders { workspace_id, text } => {
            store.standing_orders.insert(workspace_id, text);
            core::save_definitions(&db, &store)
        }
        Request::ClearNeedsYou { routine_id, run_id, seq } => {
            let mut runs = core::load_runs(&db, &routine_id)?;
            let item = runs
                .iter_mut()
                .find(|r| r.id == run_id)
                .and_then(|r| r.needs_you.iter_mut().find(|i| i.seq == seq))
                .ok_or("That item is gone")?;
            item.done = true;
            core::save_runs(&db, &routine_id, &runs)
        }
        _ => Err("Not a routine request".into()),
    }
}

pub fn handle(state: &AppState, request: Request) -> Response {
    let result = match request {
        Request::Routines => views(state),
        Request::RoutineNotifications { after } => notifications(state, after),
        other => edit(state, other).and_then(|()| views(state)),
    };
    result.unwrap_or_else(|message| Response::Error { message })
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::routines::{Cadence, Routine, RoutineKind};

    fn routine(id: &str, enabled: bool) -> Routine {
        Routine { id: id.into(), workspace_id: "w".into(), kind: RoutineKind::Custom, name: id.into(), instruction: "x".into(), cadence: Cadence::Daily { hour: 9, minute: 0 }, enabled, model: String::new(), paseo_schedule: None }
    }

    fn due(ms: Option<i64>, running: bool) -> RoutineState {
        RoutineState { next_due_ms: ms, running, ..Default::default() }
    }

    #[test]
    fn pick_due_chooses_most_overdue_enabled() {
        let store = Store { routines: vec![routine("a", true), routine("b", true), routine("c", false), routine("d", true)], ..Default::default() };
        let states = vec![("a".into(), due(Some(50), false)), ("b".into(), due(Some(10), false)), ("c".into(), due(Some(0), false)), ("d".into(), due(Some(500), false))];
        assert_eq!(pick_due(&store, &states, 100).as_deref(), Some("b"));
        assert_eq!(pick_due(&store, &states, 5), None);
        // A routine with no state yet is due now; a running one is skipped.
        let states = vec![("a".into(), due(Some(0), true))];
        assert_eq!(pick_due(&store, &states, 1).as_deref(), Some("b"));
    }

    #[test]
    fn stale_running_is_cleared() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let store = Store { routines: vec![routine("a", true)], ..Default::default() };
        core::save_definitions(&db, &store).unwrap();
        core::save_state(&db, "a", &due(Some(999), true)).unwrap();
        clear_stale_running(&db).unwrap();
        let state = core::load_state(&db, "a").unwrap();
        assert!(!state.running);
        assert_eq!(state.next_due_ms, None);
    }
}
