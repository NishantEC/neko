//! Runs routines (`neko_core::routines`): one thread wakes every 30 seconds
//! and starts the most overdue enabled routine, at most one at a time, never
//! while the machine steward holds new work. A run is Claude Code in the
//! scope's folder with the owner's full setup, so it has that folder's MCP
//! servers and sign-ins. It acts, then its reply's json block becomes a
//! `RoutineRun`. See docs/knowledge/neko-runs-the-day.md.
use crate::server::AppState;
use neko_core::inbox;
use crate::routine_care;
use neko_core::routines::{self as core, Store};
use neko_protocol::routines::{Cadence, NeedsYou, Routine, RoutineNotice, RoutineRun, RoutineState, RoutineView, ScopeProposal};
use neko_protocol::workbench::{AgentRuntime, Command, Workspace};
use neko_protocol::{Request, Response};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, MutexGuard};
use std::time::Duration;

static BUSY: AtomicBool = AtomicBool::new(false);
/// Held through seeding, so the runner and a page load never both create scopes.
static SEEDING: std::sync::Mutex<()> = std::sync::Mutex::new(());
const TICK: Duration = Duration::from_secs(30);
const RUN_TIMEOUT: Duration = Duration::from_secs(15 * 60);
/// Briefs and sweeps rebase, fix and test: they get longer.
const LONG_RUN_TIMEOUT: Duration = Duration::from_secs(30 * 60);
const KEEP_TEXT: usize = 4000;
const VIEW_RUNS: usize = 10;
const PASEO_TIMEOUT: Duration = Duration::from_secs(20);
const DISCOVERY_MAX_AGE: Duration = Duration::from_secs(600);
const DAILY_RETRY_MS: i64 = 30 * 60 * 1000;
const GIVE_UP_FOR: Duration = Duration::from_secs(3600);
/// The open items listed in a run's prompt.
const MAX_OPEN_IN_PROMPT: usize = 40;

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

/// ~/.claude.json, retried briefly: Claude Code rewrites it often, and a run
/// started on a half-written copy would miss its scope's servers.
fn claude_json_settled(home: &Path) -> Option<serde_json::Value> {
    for _ in 0..5 {
        if let Some(value) = claude_json(home) {
            return Some(value);
        }
        if !home.join(".claude.json").exists() {
            return None;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Discovery walks ~/Documents and parses ~/.claude.json (which Claude Code
/// rewrites often): reuse the last result while neither changed, for at most
/// ten minutes. A half-written ~/.claude.json keeps the previous result.
fn discover_cached(home: &Path) -> Vec<ScopeProposal> {
    type Cache = Option<(Option<std::time::SystemTime>, Vec<Option<std::time::SystemTime>>, std::time::Instant, Vec<ScopeProposal>)>;
    static CACHE: std::sync::Mutex<Cache> = std::sync::Mutex::new(None);
    let mtime = |path: PathBuf| std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let (config, documents) = (mtime(home.join(".claude.json")), neko_core::scopes::roots(home).into_iter().map(mtime).collect::<Vec<_>>());
    let mut cache = CACHE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((c, d, at, scopes)) = cache.as_ref() {
        if *c == config && *d == documents && at.elapsed() < DISCOVERY_MAX_AGE {
            return scopes.clone();
        }
    }
    let parsed = claude_json(home);
    if parsed.is_none() && home.join(".claude.json").exists() {
        if let Some((_, _, _, scopes)) = cache.as_ref() {
            return scopes.clone();
        }
    }
    let scopes = neko_core::scopes::discover(home, parsed.as_ref());
    *cache = Some((config, documents, std::time::Instant::now(), scopes.clone()));
    scopes
}

/// Scope folders Neko could not make a workspace for, so it stops trying
/// until the daemon restarts instead of creating one every tick.
/// Tried again after `GIVE_UP_FOR`, in case the error was passing.
static GAVE_UP: std::sync::Mutex<Vec<(String, std::time::Instant)>> = std::sync::Mutex::new(Vec::new());

/// `None` means for good: retrying would only make another workspace.
static GAVE_UP_FOR_GOOD: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

fn gave_up(folder: &str) -> bool {
    if GAVE_UP_FOR_GOOD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).iter().any(|f| f == folder) {
        return true;
    }
    let mut list = GAVE_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    list.retain(|(_, at)| at.elapsed() < GIVE_UP_FOR);
    list.iter().any(|(f, _)| f == folder)
}

fn give_up(folder: &str, why: &str, for_good: bool) {
    eprintln!("routines: not making a scope of {folder}{}: {why}", if for_good { "" } else { " for now" });
    if for_good {
        GAVE_UP_FOR_GOOD.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(folder.to_string());
    } else {
        GAVE_UP.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push((folder.to_string(), std::time::Instant::now()));
    }
}

/// Scopes on disk matched to workspaces, plus every other workspace as a
/// scope of its own. With `create`, a scope without a workspace gets one.
fn scopes(state: &AppState, create: bool) -> Result<Vec<ScopeProposal>, String> {
    let mut proposals = discover_cached(&home());
    let (mut workspaces, mut folders) = neko_core::workbench::load_workspaces(&db(state))?;
    let folders_of = |folders: &std::collections::BTreeMap<String, Vec<String>>, w: &Workspace| folders.get(&w.id).cloned().unwrap_or_else(|| vec![w.repository.clone()]);
    // Workspaces store canonical paths; compare like with like.
    let canonical = |path: &str| std::fs::canonicalize(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.to_string());
    for proposal in &mut proposals {
        let folder = canonical(&proposal.folders[0]);
        let find = |workspaces: &[Workspace], folders: &std::collections::BTreeMap<String, Vec<String>>| {
            workspaces.iter().find(|w| folders_of(folders, w).iter().any(|f| canonical(f) == folder)).map(|w| w.id.clone())
        };
        if let Some(existing) = find(&workspaces, &folders).and_then(|id| workspaces.iter().find(|w| w.id == id)) {
            // A workspace the owner named keeps its name.
            proposal.name = existing.name.clone();
        }
        proposal.workspace_id = match find(&workspaces, &folders) {
            Some(id) => Some(id),
            None if create && !gave_up(&folder) => {
                // An empty id asks the workbench for a new workspace.
                let workspace = Workspace { id: String::new(), name: proposal.name.clone(), repository: folder.clone(), instructions: String::new(), away_enabled: false };
                match state.workbench.command(Command::SaveWorkspaceWithFolders { workspace, folders: vec![folder.clone()] }) {
                    Ok(snapshot) => {
                        (workspaces, folders) = (snapshot.workspaces, snapshot.workspace_folders);
                        let found = find(&workspaces, &folders);
                        if found.is_none() {
                            give_up(&folder, "its workspace was created but cannot be matched", true);
                        }
                        found
                    }
                    Err(error) => {
                        give_up(&folder, &error, false);
                        None
                    }
                }
            }
            None => None,
        };
    }
    // Everyone has a personal scope: the home folder, unless a workspace
    // already covers it.
    let home_path = canonical(&home().to_string_lossy());
    let home_covered = workspaces.iter().any(|w| folders_of(&folders, w).iter().any(|f| canonical(f) == home_path));
    if create && !home_covered && !gave_up(&home_path) {
        let workspace = Workspace { id: String::new(), name: "Personal".into(), repository: home_path.clone(), instructions: String::new(), away_enabled: false };
        match state.workbench.command(Command::SaveWorkspaceWithFolders { workspace, folders: vec![home_path.clone()] }) {
            Ok(snapshot) => (workspaces, folders) = (snapshot.workspaces, snapshot.workspace_folders),
            Err(error) => give_up(&home_path, &error, false),
        }
    }
    // Not made yet (routines are off): still offered, so the first-run setup can
    // say what the personal scope would have (the global MCP servers).
    if !create && !home_covered {
        let servers = claude_json(&home())
            .and_then(|value| value["mcpServers"].as_object().map(|s| s.keys().cloned().collect::<Vec<_>>()))
            .unwrap_or_default()
            .into_iter()
            .map(|name| neko_protocol::routines::ScopeServer { name, origin: "~/.claude.json".into(), level: "scope".into(), repo: None })
            .collect();
        proposals.push(ScopeProposal { name: "Personal".into(), folders: vec![home_path.clone()], servers, problems: Vec::new(), workspace_id: None, discovered: false, instructions: String::new() });
    }
    for proposal in &mut proposals {
        if let Some(workspace) = workspaces.iter().find(|w| proposal.workspace_id.as_deref() == Some(w.id.as_str())) {
            proposal.instructions = workspace.instructions.clone();
        }
    }
    for workspace in &workspaces {
        if proposals.iter().any(|p| p.workspace_id.as_deref() == Some(workspace.id.as_str())) {
            continue;
        }
        proposals.push(ScopeProposal {
            name: workspace.name.clone(),
            folders: folders_of(&folders, workspace),
            servers: Vec::new(),
            problems: Vec::new(),
            workspace_id: Some(workspace.id.clone()),
            discovered: false,
            instructions: workspace.instructions.clone(),
        });
    }
    Ok(proposals)
}

/// Gives every scope its workspace and, once per workspace, its routines,
/// so a scope folder added later is picked up too.
fn ensure_seeded(state: &AppState) -> Result<(), String> {
    let _seeding = SEEDING.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    // Routines are off until the person turns them on: no workspace or routine
    // is made for what happens to be on disk.
    if !routines_on(state)? {
        return Ok(());
    }
    let scopes = scopes(state, true)?;
    let db = db(state);
    let mut store = core::load(&db)?;
    // Stores from before per-workspace seeding: what exists counts as seeded.
    if store.seeded && store.seeded_workspaces.is_empty() {
        store.seeded_workspaces = store.routines.iter().map(|r| r.workspace_id.clone()).collect();
    }
    // Scope folders found on disk, and the home workspace, get routines; a
    // workspace made for ticket work does not start acting on its own.
    let canonical = |path: &str| std::fs::canonicalize(path).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.trim_end_matches('/').to_string());
    let home_folder = canonical(&home().to_string_lossy());
    let all = scopes.clone();
    let fresh: Vec<ScopeProposal> = scopes
        .into_iter()
        .filter(|s| s.discovered || s.folders.iter().any(|f| canonical(f) == home_folder))
        .filter(|s| s.workspace_id.as_ref().is_some_and(|id| !store.seeded_workspaces.contains(id)))
        .collect();
    // Seeded instructions written by an older build are rewritten once,
    // unless the owner edited them (a custom routine keeps its own).
    if store.instructions_version < core::INSTRUCTIONS_VERSION {
        for routine in store.routines.iter_mut().filter(|r| r.kind != neko_protocol::routines::RoutineKind::Custom && !r.customized) {
            if let Some(scope) = all.iter().find(|s| s.workspace_id.as_deref() == Some(routine.workspace_id.as_str())) {
                let personal = scope.folders.iter().any(|f| canonical(f) == home_folder);
                routine.instruction = core::seed_instruction(routine.kind, personal);
            }
        }
        store.instructions_version = core::INSTRUCTIONS_VERSION;
        core::save_definitions(&db, &store)?;
    }
    // Every eligible scope gets each seeded routine once: a fresh scope gets
    // all of them, an existing one any kind added since (Reflect, 2026-11-06).
    let eligible: Vec<ScopeProposal> = all
        .iter()
        .filter(|s| s.discovered || s.folders.iter().any(|f| canonical(f) == home_folder))
        .filter(|s| s.workspace_id.is_some())
        .cloned()
        .collect();
    let missing: Vec<Routine> = core::seeds(&eligible, &home().to_string_lossy())
        .into_iter()
        .filter(|r| !store.routines.iter().any(|existing| existing.id == r.id))
        .filter(|r| store.seeded_workspaces.contains(&r.workspace_id) || fresh.iter().any(|s| s.workspace_id.as_deref() == Some(r.workspace_id.as_str())))
        .collect();
    if fresh.is_empty() && store.seeded && missing.is_empty() {
        return Ok(());
    }
    let now = neko_core::workbench::now_ms();
    for routine in missing {
        if !store.routines.iter().any(|r| r.id == routine.id) {
            // First runs follow the cadence, so seeding never starts a burst.
            let first = RoutineState { next_due_ms: Some(core::next_due_local(&routine.cadence, 0, now)), ..Default::default() };
            core::save_state(&db, &routine.id, &first)?;
            store.routines.push(routine);
        }
    }
    store.seeded_workspaces.extend(fresh.into_iter().filter_map(|s| s.workspace_id));
    store.seeded = true;
    core::save_definitions(&db, &store)
}

/// The master switch (`core::master_on`).
fn routines_on(state: &AppState) -> Result<bool, String> {
    let db = db(state);
    Ok(core::master_on(&db, &core::load(&db)?))
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

/// The enabled routine that is most overdue, if any is due. A "Run now"
/// comes first; a repeating routine waits for its hours (`may_run`), so a
/// wake at night does not fire it.
pub(crate) fn pick_due(store: &Store, states: &[(String, RoutineState)], now_ms: i64, may_run: &dyn Fn(&Cadence) -> bool) -> Option<String> {
    store
        .routines
        .iter()
        .filter_map(|r| {
            let state = states.iter().find(|(id, _)| id == &r.id).map(|(_, s)| s);
            if state.is_some_and(|s| s.running) {
                return None;
            }
            if state.is_some_and(|s| s.run_requested) {
                return Some((i64::MIN, r.id.clone()));
            }
            let due = state.and_then(|s| s.next_due_ms).unwrap_or(0);
            (r.enabled && due <= now_ms && may_run(&r.cadence)).then_some((due, r.id.clone()))
        })
        .min()
        .map(|(_, id)| id)
}

/// An edit changes what the routine does and when; whether it is on, where
/// it runs and which Paseo job it replaces stay as the daemon has them, so a
/// dialog opened before a Pause cannot undo it.
pub(crate) fn merge_edit(existing: &Routine, edit: Routine) -> Routine {
    // Once the owner changes the words, they are theirs for good.
    let customized = existing.customized || edit.instruction.trim() != existing.instruction.trim();
    Routine { id: existing.id.clone(), workspace_id: existing.workspace_id.clone(), kind: existing.kind, enabled: existing.enabled, paseo_schedule: existing.paseo_schedule.clone(), customized, ..edit }
}

/// Runs a short helper command, killing it after `limit`.
fn output_within(mut command: std::process::Command, limit: Duration) -> Result<std::process::Output, String> {
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let deadline = std::time::Instant::now() + limit;
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(_) => return child.wait_with_output().map_err(|e| e.to_string()),
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("timed out after {}s", limit.as_secs()));
            }
            None => std::thread::sleep(Duration::from_millis(100)),
        }
    }
}

/// Clears `BUSY` however the run ends, a panic included.
struct BusyGuard;

impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

/// The newest runs, plus any older run that still has an item open (or one
/// closed in the last few days, for "Closed recently"), so an unhandled item
/// never drops off the page.
pub(crate) fn runs_for_view(mut runs: Vec<RoutineRun>, now: i64) -> Vec<RoutineRun> {
    let older = runs.split_off(runs.len().min(VIEW_RUNS));
    runs.extend(older.into_iter().filter(|run| run.needs_you.iter().any(|item| !item.done || inbox::recently_closed(item, now))));
    runs
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
    let tick_ms = neko_core::workbench::now_ms();
    let down = routine_care::note_tick(&db(state), tick_ms);
    sweep_inbox(state);
    if BUSY.load(Ordering::SeqCst) {
        return Ok(());
    }
    if neko_core::machine::latest().is_some_and(|health| health.holding.is_some()) {
        return Ok(());
    }
    if !routines_on(state)? {
        return Ok(());
    }
    ensure_seeded(state)?;
    let now = neko_core::workbench::now_ms();
    let mut routine_missed = None;
    let (routine, scope) = {
        let store = core::load(&db(state))?;
        let states = store
            .routines
            .iter()
            .map(|r| {
                let db = db(state);
                // BUSY is false, so nothing is running: a set flag is stale.
                // A row that no longer decodes starts over rather than
                // blocking every routine.
                let mut saved = match core::load_state(&db, &r.id) {
                    Ok(saved) => saved,
                    Err(error) if error.starts_with("Cannot read") => {
                        let fresh = RoutineState::default();
                        core::save_state(&db, &r.id, &fresh)?;
                        fresh
                    }
                    Err(error) => return Err(error),
                };
                if saved.running {
                    saved.running = false;
                    core::save_state(&db, &r.id, &saved)?;
                }
                Ok((r.id.clone(), saved))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let Some(id) = pick_due(&store, &states, now, &|cadence| core::in_window_local(cadence, now)) else { return Ok(()) };
        let routine = store.routines.iter().find(|r| r.id == id).cloned().ok_or("Routine vanished")?;
        let picked = states.iter().find(|(i, _)| *i == id).map(|(_, s)| s.clone()).unwrap_or_default();
        // The daily spend ceiling pauses new routine runs (a "Run now" from the person still goes).
        if !picked.run_requested && routine_care::spend_held(&db(state), now) {
            return Ok(());
        }
        // Down across due times: one catch-up run with a note, or skip them when catch-up is off.
        match routine_care::plan(&routine, &picked, now, down, &chrono::Local) {
            routine_care::Plan::Skip(missed) => {
                let db = db(state);
                let mut skipped = core::load_state(&db, &routine.id)?;
                skipped.next_due_ms = Some(core::next_due_local(&routine.cadence, skipped.quiet_streak, now));
                skipped.note = Some(format!("Skipped {} missed run{} since {}: catch-up is off for this routine.", missed.count, if missed.count == 1 { "" } else { "s" }, local_time(missed.since_ms)));
                core::save_state(&db, &routine.id, &skipped)?;
                return Ok(());
            }
            routine_care::Plan::Run(missed) => routine_missed = missed,
        }
        let scope = scopes(state, false)?.into_iter().find(|s| s.workspace_id.as_deref() == Some(routine.workspace_id.as_str()));
        (routine, scope)
    };
    {
        let db = db(state);
        let mut routine_state = core::load_state(&db, &routine.id)?;
        routine_state.running = true;
        routine_state.run_requested = false;
        routine_state.missed = routine_missed;
        core::save_state(&db, &routine.id, &routine_state)?;
    }
    BUSY.store(true, Ordering::SeqCst);
    let state = state.clone();
    std::thread::spawn(move || {
        let _busy = BusyGuard;
        if let Err(error) = run_one(&state, &routine, scope) {
            eprintln!("routine {}: {error}", routine.id);
        }
    });
    Ok(())
}

/// Open items in this workspace, oldest first, without repeats, with how
/// long each has been open.
fn open_items(db: &neko_core::Db, workspace_id: &str, now: i64) -> Result<Vec<inbox::OpenItem>, String> {
    let mut items: Vec<(u64, inbox::OpenItem)> = Vec::new();
    for routine in core::load(db)?.routines.iter().filter(|r| r.workspace_id == workspace_id) {
        for run in core::load_runs(db, &routine.id).unwrap_or_default() {
            let started = run.started_ms;
            items.extend(run.needs_you.into_iter().filter(|i| !i.done).map(|i| {
                let open = inbox::OpenItem { age_days: inbox::age_days(&i, started, now), expires: i.expires_ms.map(inbox::iso_from_ms), title: i.title };
                (i.seq, open)
            }));
        }
    }
    items.sort_by_key(|(seq, _)| *seq);
    let mut seen = std::collections::HashSet::new();
    Ok(items.into_iter().filter(|(_, i)| seen.insert(title_key(&i.title))).map(|(_, i)| i).take(MAX_OPEN_IN_PROMPT).collect())
}

/// Applies `f` to every item of the routines in `workspace` (every scope
/// when `None`) and saves the rows it changed. `f` gets the routine, the
/// start of the run holding the item, and the item; it says whether it
/// changed it.
fn each_item(db: &neko_core::Db, workspace: Option<&str>, f: &mut dyn FnMut(&Routine, i64, &mut NeedsYou) -> bool) -> Result<(), String> {
    for routine in core::load(db)?.routines.iter().filter(|r| workspace.map_or(true, |w| r.workspace_id == w)) {
        let mut runs = core::load_runs(db, &routine.id).unwrap_or_default();
        let mut changed = false;
        for run in runs.iter_mut() {
            let started = run.started_ms;
            for item in run.needs_you.iter_mut() {
                changed |= f(routine, started, item);
            }
        }
        if changed {
            core::save_runs(db, &routine.id, &runs)?;
        }
    }
    Ok(())
}

/// Every needs-you item of every routine, open or closed, once per `seq`
/// (the phone channels read these).
pub(crate) fn all_items(db: &neko_core::Db) -> Result<Vec<NeedsYou>, String> {
    let mut seen = std::collections::HashSet::new();
    let mut items = Vec::new();
    for routine in core::load(db)?.routines {
        for run in core::load_runs(db, &routine.id).unwrap_or_default() {
            items.extend(run.needs_you.into_iter().filter(|i| i.seq == 0 || seen.insert(i.seq)));
        }
    }
    Ok(items)
}

/// The routine that holds a scope's goal items. It never runs (disabled, no
/// schedule the owner sees); it only gives goal questions a place in the
/// Inbox, where the phone channels already read them.
pub(crate) const GOALS_ROUTINE_PREFIX: &str = "goals:";

/// Raises one needs-you item for a goal ("Extend by 20 turns?", or a blocked
/// goal's question) and returns its `seq`. `link` is `goal:<id>`.
pub(crate) fn raise_goal_item(db: &neko_core::Db, workspace_id: &str, title: &str, detail: &str, link: &str) -> Result<u64, String> {
    let id = format!("{GOALS_ROUTINE_PREFIX}{workspace_id}");
    let mut store = core::load(db)?;
    if !store.routines.iter().any(|r| r.id == id) {
        store.routines.push(Routine {
            id: id.clone(), workspace_id: workspace_id.to_owned(), kind: neko_protocol::routines::RoutineKind::Custom,
            name: "Goals".into(), instruction: "Goals that keep working report here.".into(),
            cadence: Cadence::Daily { hour: 0, minute: 0 }, enabled: false, model: String::new(), paseo_schedule: None,
            browser: false, computer: false, customized: true, catch_up: None,
        });
        core::save_definitions(db, &store)?;
    }
    let now = neko_core::workbench::now_ms();
    let seq = core::reserve_seq(db, 1)?;
    let item = NeedsYou { seq, title: title.into(), detail: detail.into(), link: (!link.is_empty()).then(|| link.into()), created_ms: now, touched_ms: now, ..Default::default() };
    let run = RoutineRun { id: neko_core::workbench::new_id(), routine_id: id.clone(), started_ms: now, finished_ms: Some(now), ok: true, quiet: false, did: Vec::new(), needs_you: vec![item], error: None, text: String::new(), ..Default::default() };
    core::push_run(db, &id, run)?;
    Ok(seq)
}

/// Closes a goal's item once the person answered it another way.
pub(crate) fn close_goal_item(db: &neko_core::Db, seq: u64) -> Result<(), String> {
    close_seqs(db, &[seq], inbox::BY_YOU)
}

/// Closes the open items with these seqs (the owner completed their
/// reminders on the phone).
pub(crate) fn close_seqs(db: &neko_core::Db, seqs: &[u64], reason: &str) -> Result<(), String> {
    let now = neko_core::workbench::now_ms();
    each_item(db, None, &mut |_, _, item| {
        if item.done || !seqs.contains(&item.seq) {
            return false;
        }
        inbox::close(item, reason, now);
        true
    })
}

/// Closes every open item in the workspace whose title is in `titles`.
fn close_titles(db: &neko_core::Db, workspace_id: &str, titles: &[String], reason: &str) -> Result<(), String> {
    if titles.is_empty() {
        return Ok(());
    }
    let keys: std::collections::HashSet<String> = titles.iter().map(|t| title_key(t)).collect();
    // A setup failure that is fixed is fixed for every scope that raised it.
    let setups: std::collections::HashSet<String> = titles.iter().filter_map(|t| inbox::setup_key(t)).collect();
    let now = neko_core::workbench::now_ms();
    each_item(db, None, &mut |routine, _, item| {
        let here = routine.workspace_id == workspace_id && keys.contains(&title_key(&item.title));
        if item.done || !(here || inbox::setup_key(&item.title).is_some_and(|k| setups.contains(&k))) {
            return false;
        }
        inbox::close(item, reason, now);
        true
    })
}

/// Marks done every open item in the workspace whose title a run resolved.
fn resolve_items(db: &neko_core::Db, workspace_id: &str, titles: &[String]) -> Result<(), String> {
    close_titles(db, workspace_id, titles, inbox::BY_ROUTINE)
}

/// A reply's items that repeat an open one refresh it (newer detail, age
/// starts over) instead of being added again; a setup failure naming the
/// same connection counts as the same item in any scope. What is left is new
/// and gets its times.
fn absorb_repeats(db: &neko_core::Db, workspace_id: &str, items: &mut Vec<NeedsYou>, now: i64) -> Result<(), String> {
    let mut seen = std::collections::HashSet::new();
    items.retain(|i| seen.insert(title_key(&i.title)));
    let mut absorbed = vec![false; items.len()];
    if !items.is_empty() {
        each_item(db, None, &mut |routine, _, old| {
            if old.done {
                return false;
            }
            let same_scope = routine.workspace_id == workspace_id;
            for (n, new) in items.iter().enumerate() {
                if !absorbed[n] && inbox::same_item(&new.title, &old.title, same_scope) {
                    inbox::refresh(old, new, now);
                    absorbed[n] = true;
                    return true;
                }
            }
            false
        })?;
    }
    let mut keep = absorbed.into_iter().map(|a| !a);
    items.retain(|_| keep.next().unwrap_or(true));
    for item in items.iter_mut() {
        item.created_ms = now;
        item.touched_ms = now;
    }
    Ok(())
}

static LAST_SWEEP_MS: AtomicI64 = AtomicI64::new(0);
static LAST_PR_SWEEP_MS: AtomicI64 = AtomicI64::new(0);
static PR_SWEEPING: AtomicBool = AtomicBool::new(false);
const SWEEP_EVERY_MS: i64 = 5 * 60 * 1000;
const PR_SWEEP_EVERY_MS: i64 = 15 * 60 * 1000;
/// `gh` calls per sweep: the rest wait for the next one.
const MAX_PR_CALLS: usize = 6;
const PR_STATE: &str = "inbox_pr_state_v1";

/// Closes items whose time passed or that nobody touched for the age-out
/// period (every 5 minutes, rows only), and, on its own thread every 15
/// minutes, items whose pull request is merged or closed.
fn sweep_inbox(state: &Arc<AppState>) {
    let now = neko_core::workbench::now_ms();
    if now - LAST_SWEEP_MS.load(Ordering::SeqCst) < SWEEP_EVERY_MS {
        return;
    }
    LAST_SWEEP_MS.store(now, Ordering::SeqCst);
    if let Err(error) = sweep_time(&db(state), now) {
        eprintln!("inbox sweep: {error}");
    }
    let holding = neko_core::machine::latest().is_some_and(|health| health.holding.is_some());
    if cfg!(test) || holding || now - LAST_PR_SWEEP_MS.load(Ordering::SeqCst) < PR_SWEEP_EVERY_MS || PR_SWEEPING.swap(true, Ordering::SeqCst) {
        return;
    }
    LAST_PR_SWEEP_MS.store(now, Ordering::SeqCst);
    let database = state.db.clone();
    std::thread::spawn(move || {
        let _clear = PrSweepGuard;
        if let Ok(gh) = neko_core::native_runner::publish::resolve_gh() {
            if let Err(error) = sweep_prs(&database, neko_core::workbench::now_ms(), &|r| gh_pr_state(&gh, r)) {
                eprintln!("inbox pull request sweep: {error}");
            }
        }
    });
}

/// Clears `PR_SWEEPING` however the sweep ends, a panic included.
struct PrSweepGuard;

impl Drop for PrSweepGuard {
    fn drop(&mut self) {
        PR_SWEEPING.store(false, Ordering::SeqCst);
    }
}

fn sweep_time(db: &neko_core::Db, now: i64) -> Result<usize, String> {
    let days = core::load(db)?.age_out_days();
    // An outage's notice closes when Claude answers again; it is not aged out
    // (the health row would still say it was raised, so it would never return).
    let outage = [title_key(SIGNED_OUT_TITLE), title_key(LIMIT_TITLE)];
    let mut closed = 0;
    each_item(db, None, &mut |_, started, item| match inbox::time_reason(item, started, now, days).filter(|_| !outage.contains(&title_key(&item.title))) {
        Some(reason) => {
            inbox::close(item, reason, now);
            closed += 1;
            true
        }
        None => false,
    })?;
    Ok(closed)
}

/// Pull request state per `owner/repo#n`: the state, when it ended and when it
/// was read. Merged and closed are kept (a pull request rarely comes back);
/// open is read again after 15 minutes; a reference `gh` could not read
/// ("UNKNOWN": private, not a pull request) is retried after 6 hours, so a
/// few bad references never use up every sweep. Its own small row, never the
/// workbench snapshot.
type PrCache = std::collections::BTreeMap<String, (String, i64, i64)>;
const UNKNOWN_RETRY_MS: i64 = 6 * 60 * 60 * 1000;

/// The state and end time (ms) of a pull request; merged and closed ones end
/// at GitHub's `closedAt`.
fn gh_pr_state(gh: &Path, pr: &inbox::PrRef) -> Option<(String, i64)> {
    let mut command = std::process::Command::new(gh);
    command
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["pr", "view", &pr.number.to_string(), "--repo", &format!("{}/{}", pr.owner, pr.repo), "--json", "state,closedAt"]);
    let output = output_within(command, Duration::from_secs(15)).ok()?;
    if !output.status.success() {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    let state = value["state"].as_str().filter(|s| matches!(*s, "OPEN" | "MERGED" | "CLOSED"))?.to_string();
    let ended = value["closedAt"].as_str().and_then(inbox::parse_time_ms).unwrap_or(0);
    Some((state, ended))
}

/// Reads the state of the pull requests open items name (at most
/// `MAX_PR_CALLS` fresh lookups) and closes the items whose pull requests are
/// all merged or closed. `fetch` is `gh pr view` in production.
fn sweep_prs(db: &std::sync::Mutex<neko_core::Db>, now: i64, fetch: &dyn Fn(&inbox::PrRef) -> Option<(String, i64)>) -> Result<usize, String> {
    let lock = || db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut wanted = std::collections::BTreeSet::new();
    let mut cache: PrCache = {
        let db = lock();
        each_item(&db, None, &mut |_, _, item| {
            if !item.done && !item.kept {
                wanted.extend(inbox::pr_refs(item));
            }
            false
        })?;
        db.get_setting(PR_STATE).map_err(|e| e.to_string())?.and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
    };
    if wanted.is_empty() {
        if !cache.is_empty() {
            lock().set_setting_quiet(PR_STATE, "{}").map_err(|e| e.to_string())?;
        }
        return Ok(0);
    }
    let mut stale: Vec<&inbox::PrRef> = wanted
        .iter()
        .filter(|r| match cache.get(&r.key()) {
            None => true,
            Some((state, _, at)) => match state.as_str() {
                "OPEN" => now - at >= PR_SWEEP_EVERY_MS,
                "UNKNOWN" => now - at >= UNKNOWN_RETRY_MS,
                _ => false,
            },
        })
        .collect();
    stale.sort_by_key(|r| cache.get(&r.key()).map_or(0, |(_, _, at)| *at));
    for pr in stale.into_iter().take(MAX_PR_CALLS) {
        let (state, ended) = fetch(pr).unwrap_or_else(|| ("UNKNOWN".into(), 0));
        cache.insert(pr.key(), (state, ended, now));
    }
    cache.retain(|key, _| wanted.iter().any(|r| &r.key() == key));
    let db = lock();
    db.set_setting_quiet(PR_STATE, &serde_json::to_string(&cache).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mut closed = 0;
    each_item(&db, None, &mut |_, started, item| {
        if item.done || item.kept {
            return false;
        }
        let since = if item.created_ms > 0 { item.created_ms } else { started };
        match inbox::pr_reason(&inbox::pr_refs(item), since, &|r| cache.get(&r.key()).map(|(state, ended, _)| (state.clone(), *ended))) {
            Some(reason) => {
                inbox::close(item, reason, now);
                closed += 1;
                true
            }
            None => false,
        }
    })?;
    Ok(closed)
}

fn failure_title(routine: &Routine) -> String {
    format!("{} failed", routine.name)
}

/// Whether this routine's failure alert is already waiting on the owner.
fn alert_open(db: &neko_core::Db, routine: &Routine) -> Result<bool, String> {
    Ok(open_titles(db, &routine.workspace_id)?.contains(&title_key(&failure_title(routine))))
}

fn title_key(title: &str) -> String {
    inbox::title_key(title)
}

/// Titles of items still open anywhere in this workspace's routines.
fn open_titles(db: &neko_core::Db, workspace_id: &str) -> Result<std::collections::HashSet<String>, String> {
    let mut titles = std::collections::HashSet::new();
    for routine in core::load(db)?.routines.iter().filter(|r| r.workspace_id == workspace_id) {
        for run in core::load_runs(db, &routine.id).unwrap_or_default() {
            titles.extend(run.needs_you.iter().filter(|i| !i.done).map(|i| title_key(&i.title)));
        }
    }
    Ok(titles)
}

/// After a failure: a daily routine's first failure retries in 30 minutes,
/// anything else follows its cadence.
fn retry_or_next(cadence: &Cadence, state: &RoutineState, now: i64) -> i64 {
    if matches!(cadence, Cadence::Daily { .. }) && state.failures == 1 {
        now + DAILY_RETRY_MS
    } else {
        core::next_due_local(cadence, state.quiet_streak, now)
    }
}

/// Runs the routine; whatever happens, the routine is not left "running".
fn run_one(state: &AppState, routine: &Routine, scope: Option<ScopeProposal>) -> Result<(), String> {
    let mut pushed = None;
    let result = run_body(state, routine, scope, &mut pushed);
    if let Err(error) = &result {
        let db = db(state);
        let mut routine_state = match core::load_state(&db, &routine.id) {
            Ok(saved) if !saved.running => return result,
            Ok(saved) => saved,
            // Only a row that no longer decodes is replaced; a database that
            // cannot be read now cannot be written either.
            Err(read) if read.starts_with("Cannot read") => RoutineState::default(),
            Err(_) => return result,
        };
        let now = neko_core::workbench::now_ms();
        routine_state.running = false;
        if let Some(intended) = pushed {
            // The run (and any alert in it) is recorded; only saving its
            // state failed. Save what the run meant to: memory, counts, due.
            core::save_state(&db, &routine.id, &intended)?;
            return result;
        }
        routine_state.failures = routine_state.failures.saturating_add(1);
        routine_state.last_run_ms = Some(now);
        routine_state.next_due_ms = Some(retry_or_next(&routine.cadence, &routine_state, now));
        routine_state.note = Some(format!("Last run could not be saved: {error}"));
        if core::failure_alerts(&routine.cadence, routine_state.failures) && !alert_open(&db, routine).unwrap_or(false) {
            if let Ok(seq) = core::reserve_seq(&db, 1) {
                let alert = NeedsYou { seq, title: failure_title(routine), detail: format!("Neko could not save this run: {error}"), created_ms: now, touched_ms: now, ..Default::default() };
                let run = RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: now, finished_ms: Some(now), ok: false, quiet: false, did: Vec::new(), needs_you: vec![alert], error: Some(error.clone()), text: String::new(), ..Default::default() };
                let _ = core::push_run(&db, &routine.id, run);
            }
        }
        core::save_state(&db, &routine.id, &routine_state)?;
    }
    result
}

/// Once the run's row is saved, `pushed` holds the state the run is about to
/// save, so a failed save can be retried by `run_one` as it was meant.
fn run_body(state: &AppState, routine: &Routine, scope: Option<ScopeProposal>, pushed: &mut Option<RoutineState>) -> Result<(), String> {
    let started = neko_core::workbench::now_ms();
    let (orders, memory, open, age_out_days) = {
        let db = db(state);
        (core::load(&db)?.orders_for(&routine.workspace_id).to_string(), core::load_state(&db, &routine.id)?.memory, open_items(&db, &routine.workspace_id, neko_core::workbench::now_ms())?, core::load(&db)?.age_out_days())
    };
    let mut meta = AgentMeta::default();
    let outcome: Result<core::Parsed, (String, String)> = match scope.as_ref().and_then(|s| s.folders.first().map(|f| (s, f))) {
        None => Err(("This routine's workspace no longer exists".into(), String::new())),
        Some((scope, folder)) => {
            let folder = PathBuf::from(folder);
            if !folder.is_dir() {
                Err((format!("Scope folder missing: {}", folder.display()), String::new()))
            } else {
                let spec = neko_core::native_runner::RunSpec {
                    directory: folder,
                    prompt: core::build_prompt(routine, scope, &orders, &memory, &core::now_iso(), &open, age_out_days, &core::owner_name(), &neko_core::viewers::installed_section("{neko}", &neko_core::viewers::plugins_dir()))
                        .replace("{neko}", &std::env::current_exe().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "neko-daemon".into()))
                        .replace("{mac_care}", &core::install_mac_care().map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| "mac-care".into())),
                    writable: true,
                    timeout: if matches!(routine.kind, neko_protocol::routines::RoutineKind::Messages) { RUN_TIMEOUT } else { LONG_RUN_TIMEOUT },
                    runtime: AgentRuntime {
                        provider: "claude".into(),
                        model: if routine.model.is_empty() { "sonnet".into() } else { routine.model.clone() },
                        ..Default::default()
                    },
                };
                let cancel = AtomicBool::new(false);
                // Only this scope's MCP servers: no claude.ai connectors and
                // nothing configured for another scope.
                let home = home();
                let personal = std::fs::canonicalize(&spec.directory).ok() == std::fs::canonicalize(&home).ok();
                let mut config = neko_core::scopes::mcp_config(&scope.folders, claude_json_settled(&home).as_ref(), personal);
                if routine.computer {
                    if let Some(peekaboo) = neko_core::scopes::peekaboo() {
                        config = neko_core::scopes::with_computer_use(config, &peekaboo);
                    }
                }
                let ran = run_agent(state, routine, &spec, &config, &cancel);
                meta = ran.meta;
                match ran.result {
                    Ok(text) => core::parse_reply(&text).map_err(|error| (error, text)),
                    Err(error) => Err((error, String::new())),
                }
            }
        }
    };
    let finished = neko_core::workbench::now_ms();
    let db = db(state);
    // The definition as it is now: an edit made during the run applies.
    let current = core::load(&db)?.routines.into_iter().find(|r| r.id == routine.id).unwrap_or_else(|| routine.clone());
    let mut routine_state = core::load_state(&db, &routine.id)?;
    // Every runtime is out of usage: park the routine until the reset. Not a failure, no
    // alert, no run in its history; the row says so quietly.
    if let (Err(_), Some(until)) = (&outcome, meta.hold_until) {
        park_for_limit(&mut routine_state, until);
        *pushed = Some(routine_state.clone());
        core::save_state(&db, &routine.id, &routine_state)?;
        return Ok(());
    }
    routine_state.hold = None;
    // Due times that passed while Neko was down were caught up by this one run.
    let missed_note = routine_state.missed.take().map(|m| core::missed_note(&m));
    let note = match (&missed_note, meta.note.is_empty()) {
        (Some(missed), true) => missed.clone(),
        (Some(missed), false) => format!("{missed} {}", meta.note),
        (None, _) => meta.note.clone(),
    };
    let mut next_due = None;
    let run = match outcome {
        Ok(parsed) => {
            // A good run closes this routine's own failure alert.
            let mut resolved = parsed.resolved.clone();
            resolved.push(failure_title(&current));
            resolve_items(&db, &routine.workspace_id, &resolved)?;
            let mut needs_you = parsed.needs_you;
            // Another routine (or run) may already have raised it: refresh
            // that item rather than adding another.
            absorb_repeats(&db, &routine.workspace_id, &mut needs_you, finished)?;
            add_outage_notice(&db, routine, &meta, &mut needs_you)?;
            if !needs_you.is_empty() {
                let first = core::reserve_seq(&db, needs_you.len() as u64)?;
                for (offset, item) in needs_you.iter_mut().enumerate() {
                    item.seq = first + offset as u64;
                }
            }
            routine_state.quiet_streak = if parsed.quiet { routine_state.quiet_streak.saturating_add(1) } else { 0 };
            if routine_state.failures > 0 {
                routine_state.failures = 0;
                routine_state.note = None;
            }
            if core::memory_fits(&parsed.memory) {
                routine_state.memory = parsed.memory;
            } else {
                routine_state.note = Some(format!("The run asked to remember more than {} KB; its previous memory was kept.", core::MAX_MEMORY / 1024));
            }
            RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: started, finished_ms: Some(finished), ok: true, quiet: parsed.quiet, did: parsed.did, needs_you, error: None, text: String::new(), runtime: meta.runtime.clone(), note: note.clone(), cost_micro_usd: meta.cost_micro }
        }
        Err((error, text)) => {
            let mut end = text.len().min(KEEP_TEXT);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            routine_state.failures = routine_state.failures.saturating_add(1);
            let mut needs_you = Vec::new();
            add_outage_notice(&db, routine, &meta, &mut needs_you)?;
            if !needs_you.is_empty() {
                needs_you[0].seq = core::reserve_seq(&db, 1)?;
                needs_you[0].created_ms = finished;
                needs_you[0].touched_ms = finished;
            }
            if core::failure_alerts(&current.cadence, routine_state.failures) && !alert_open(&db, &current)? {
                needs_you.push(NeedsYou {
                    seq: core::reserve_seq(&db, 1)?,
                    title: failure_title(&current),
                    detail: format!("{error} Neko will try again{}.", if matches!(current.cadence, Cadence::Daily { .. }) { " in 30 minutes" } else { " on schedule" }),
                    link: None,
                    done: false,
                    created_ms: finished,
                    touched_ms: finished,
                    ..Default::default()
                });
            }
            next_due = Some(retry_or_next(&current.cadence, &routine_state, finished));
            RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: started, finished_ms: Some(finished), ok: false, quiet: false, did: Vec::new(), needs_you, error: Some(error), text: text[..end].to_string(), runtime: meta.runtime.clone(), note, cost_micro_usd: meta.cost_micro }
        }
    };
    let ok = run.ok;
    core::push_run(&db, &routine.id, run)?;
    routine_state.running = false;
    routine_state.last_run_ms = Some(finished);
    routine_state.next_due_ms = Some(next_due.unwrap_or_else(|| core::next_due_local(&current.cadence, routine_state.quiet_streak, finished)));
    *pushed = Some(routine_state.clone());
    core::save_state(&db, &routine.id, &routine_state)?;
    drop(db);
    if ok && !routine_state.paseo_paused {
        if let Some(name) = &current.paseo_schedule {
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

/// Raised once per outage, resolved when Claude Code works again.
const SIGNED_OUT_TITLE: &str = "Claude Code is signed out or its account changed";
const LIMIT_TITLE: &str = "Claude Code hit its usage limit";

/// The outage's notice title and what to do, from the error that started it.
fn outage_words(reason: &str, fallback: bool) -> (&'static str, &'static str) {
    let limit = neko_core::runtime_health::is_usage_limit(reason);
    match (limit, fallback) {
        (true, true) => (LIMIT_TITLE, "Routines are running on Codex until the limit resets or you add usage; this clears itself when Claude Code works again."),
        (true, false) => (LIMIT_TITLE, "No other agent is installed, so routines will keep failing until the limit resets or you add usage. This clears itself when Claude Code works again."),
        (false, true) => (SIGNED_OUT_TITLE, "Routines are running on Codex until it is back. To fix it, run `claude /login` in a terminal; this clears itself when Claude Code works again."),
        (false, false) => (SIGNED_OUT_TITLE, "No other agent is installed, so routines will keep failing until you run `claude /login` in a terminal. This clears itself when Claude Code works again."),
    }
}

/// What the agent call tells the run's record.
#[derive(Default)]
struct AgentMeta {
    /// "claude" or "codex"; empty when no agent ran.
    runtime: String,
    note: String,
    /// This run found Claude Code's login bad for the first time in an outage.
    new_outage: bool,
    /// Another runtime is installed to take over.
    fallback: bool,
    /// Every runtime that could run this reports a spent usage limit: park the routine
    /// until then (the earliest reset the errors name, else an hour on).
    hold_until: Option<i64>,
    /// What the run cost, in millionths of a dollar, when the agent reported a price.
    cost_micro: u64,
}

/// A time of day as the owner reads it ("Tue 14:00").
fn local_time(ms: i64) -> String {
    use chrono::TimeZone;
    chrono::Local.timestamp_millis_opt(ms).single().map_or_else(String::new, |t| t.format("%a %H:%M").to_string())
}

/// Parks a routine until the usage limit resets: due then, not failed, shown quietly.
fn park_for_limit(state: &mut RoutineState, until_ms: i64) {
    state.running = false;
    state.hold = Some(neko_protocol::routines::Hold { until_ms, reason: "usage limit".into() });
    state.next_due_ms = Some(until_ms);
}

/// The hold a failed run earns: every runtime tried (and any that is installed) is out of usage.
fn usage_hold(errors: &[&str], now_ms: i64) -> Option<i64> {
    (!errors.is_empty() && errors.iter().all(|e| neko_core::runtime_health::is_any_usage_limit(e))).then(|| neko_core::runtime_health::hold_until(errors, now_ms, &chrono::Local))
}

struct AgentRun {
    result: Result<String, String>,
    meta: AgentMeta,
}

const PROBE_LIMIT: Duration = Duration::from_secs(30);

/// Runs the routine's agent. Claude Code first; when its login or account is
/// bad (`runtime_health::is_auth_failure`) the same prompt runs once on Codex
/// with the same scope servers, and Claude Code is skipped until a probe says
/// it works again (at most every 30 minutes). Without another runtime the
/// behaviour is the old one, plus the once-per-outage notice.
fn run_agent(state: &AppState, routine: &Routine, spec: &neko_core::native_runner::RunSpec, config: &serde_json::Value, cancel: &AtomicBool) -> AgentRun {
    use neko_core::native_runner as runner;
    use neko_core::runtime_health as health;
    let fallback = runner::codex_installed();
    let mut meta = AgentMeta { runtime: "claude".into(), fallback, ..Default::default() };
    let now = neko_core::workbench::now_ms();
    let before = health::load(&db(state));
    let mut bad = before.is_bad();
    if bad && fallback && before.probe_due(now) {
        let _ = health::note_try(&db(state), now);
        if runner::probe_claude(PROBE_LIMIT).is_ok() {
            claude_works(&db(state));
            bad = false;
        }
    }
    let on_codex = |meta: &mut AgentMeta| {
        meta.runtime = "codex".into();
        let codex_spec = runner::RunSpec {
            directory: spec.directory.clone(),
            prompt: spec.prompt.clone(),
            writable: spec.writable,
            timeout: spec.timeout,
            runtime: AgentRuntime { provider: "codex".into(), ..Default::default() },
        };
        runner::run_codex_scoped(&codex_spec, config, cancel, |_| {}).map(|(text, skipped)| {
            let reason = neko_core::runtime_health::load(&db(state)).reason;
            let mut note = String::from(if neko_core::runtime_health::is_usage_limit(&reason) {
                "Claude Code is out of usage, so this run used Codex."
            } else {
                "Claude Code is signed out, so this run used Codex."
            });
            if !skipped.is_empty() {
                note.push_str(&format!(" Not available there: {}.", skipped.join(", ")));
            }
            if routine.browser {
                note.push_str(" Chrome control is Claude Code only.");
            }
            meta.note = note;
            text
        })
    };
    if bad && fallback {
        // The login may have gone bad in a chat, which raises no notice.
        meta.new_outage = before.notice.is_none();
        let result = on_codex(&mut meta);
        if let Err(codex) = &result {
            meta.hold_until = usage_hold(&[before.reason.as_str(), codex.as_str()], neko_core::workbench::now_ms());
        }
        return AgentRun { result, meta };
    }
    let mut cost_usd = 0.0;
    let claude = runner::run_claude_scoped(spec, config, routine.browser, cancel, |event| {
        if let Some(cost) = neko_core::spend::event_cost_usd(&event) {
            cost_usd += cost;
        }
    });
    meta.cost_micro = neko_core::spend::micro_usd(cost_usd);
    match claude {
        Ok(text) => {
            // Always: a chat may have marked the login bad during this run.
            claude_works(&db(state));
            AgentRun { result: Ok(text), meta }
        }
        Err(error) if health::is_auth_failure(&error) => {
            let db = db(state);
            let _ = health::mark_bad(&db, &error, neko_core::workbench::now_ms());
            meta.new_outage = health::load(&db).notice.is_none();
            drop(db);
            if !fallback {
                meta.hold_until = usage_hold(&[error.as_str()], neko_core::workbench::now_ms());
                return AgentRun { result: Err(error), meta };
            }
            let mut codex_error = String::new();
            let result = on_codex(&mut meta).map_err(|codex| {
                codex_error = codex.clone();
                format!("{error} The Codex fallback also failed: {codex}")
            });
            if result.is_err() {
                meta.hold_until = usage_hold(&[error.as_str(), codex_error.as_str()], neko_core::workbench::now_ms());
            }
            AgentRun { result, meta }
        }
        Err(error) => AgentRun { result: Err(error), meta },
    }
}

/// Claude Code answers again: forget the outage and close its Inbox item.
/// The home chat calls it too, so a chat that finds Claude working closes it.
pub(crate) fn claude_works(db: &neko_core::Db) {
    if let Ok(Some(before)) = neko_core::runtime_health::mark_good(db) {
        if let Some((workspace, title)) = before.notice {
            let _ = close_titles(db, &workspace, &[title], "Claude Code works again");
        }
    }
}

/// The one Inbox item of an outage, put on the run that found it.
fn add_outage_notice(db: &neko_core::Db, routine: &Routine, meta: &AgentMeta, needs_you: &mut Vec<NeedsYou>) -> Result<(), String> {
    if !meta.new_outage {
        return Ok(());
    }
    // Checked again under this lock: while this run was on Codex another run
    // may have raised the notice, or a probe or chat may have ended the outage.
    let health = neko_core::runtime_health::load(db);
    if !health.is_bad() || health.notice.is_some() {
        return Ok(());
    }
    // An item from an earlier outage still open here is adopted, not doubled.
    let (title, detail) = outage_words(&health.reason, meta.fallback);
    let open = open_titles(db, &routine.workspace_id)?;
    for existing in [SIGNED_OUT_TITLE, LIMIT_TITLE] {
        if open.contains(&title_key(existing)) {
            return neko_core::runtime_health::set_notice(db, &routine.workspace_id, existing);
        }
    }
    let now = neko_core::workbench::now_ms();
    needs_you.push(NeedsYou { seq: 0, title: title.into(), detail: detail.into(), created_ms: now, touched_ms: now, ..Default::default() });
    neko_core::runtime_health::set_notice(db, &routine.workspace_id, title)
}

/// The id of an active schedule called `name` in `paseo schedule ls --json`.
pub(crate) fn active_schedule_id(listing: &str, name: &str) -> Option<String> {
    let schedules: Vec<serde_json::Value> = serde_json::from_str(listing).ok()?;
    schedules
        .iter()
        .find(|s| s["name"] == name && s["status"] == "active")
        .and_then(|s| s["id"].as_str().map(str::to_string))
}

/// Pauses the Paseo schedule this routine replaces, found by name in Paseo's
/// live list (its schedule files can outlive a deleted schedule).
fn pause_paseo(name: &str) -> Result<String, String> {
    let paseo = ["/.local/bin/paseo", "/.npm-global/bin/paseo"]
        .iter()
        .map(|p| PathBuf::from(format!("{}{p}", home().display())))
        .find(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("paseo"));
    let mut list = std::process::Command::new(&paseo);
    list.args(["schedule", "ls", "--json"]);
    let listing = output_within(list, PASEO_TIMEOUT).map_err(|e| format!("Could not list Paseo schedules: {e}"))?;
    if !listing.status.success() {
        return Err(format!("Could not list Paseo schedules: {}", String::from_utf8_lossy(&listing.stderr).trim()));
    }
    let Some(id) = active_schedule_id(&String::from_utf8_lossy(&listing.stdout), name) else {
        return Ok(format!("Paseo schedule {name} is not running; Neko runs this now"));
    };
    let mut pause = std::process::Command::new(&paseo);
    pause.args(["schedule", "pause", &id]);
    let output = output_within(pause, PASEO_TIMEOUT).map_err(|e| format!("Could not pause Paseo schedule {name}: {e}"))?;
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
        let runs = runs_for_view(core::load_runs(&db, &routine.id)?, neko_core::workbench::now_ms());
        routines.push(RoutineView { routine: routine.clone(), state: core::load_state(&db, &routine.id)?, runs });
    }
    let mut orders = store.standing_orders.clone();
    for scope in &scopes {
        if let Some(id) = &scope.workspace_id {
            orders.entry(id.clone()).or_insert_with(|| store.orders_for(id).to_string());
        }
    }
    let routines_on = core::master_on(&db, &store);
    let spend_today = neko_core::workbench::load(&db).map(|snapshot| neko_core::spend::today(&db, &snapshot, neko_core::workbench::now_ms())).map(|t| neko_protocol::routines::SpendToday { tasks_usd: t.tasks_usd, routines_usd: t.routines_usd, ceiling_cents: t.ceiling_cents, held: t.reached() }).unwrap_or_default();
    Ok(Response::Routines { routines, standing_orders: orders, scopes, owner_name: core::owner_name(), age_out_days: store.age_out_days(), routines_on, spend_today })
}

fn notifications(state: &AppState, after: u64) -> Result<Response, String> {
    // The common answer, "nothing new", costs one small row read.
    let latest = core::latest_seq(&db(state))?;
    if after >= latest {
        return Ok(Response::RoutineNotifications { items: Vec::new(), latest });
    }
    let scopes = scopes(state, false)?;
    let db = db(state);
    let store = core::load(&db)?;
    let latest = core::latest_seq(&db)?;
    let mut items = Vec::new();
    {
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
            if routine.instruction.len() > core::MAX_INSTRUCTION {
                return Err(format!("Keep the instruction under {} KB", core::MAX_INSTRUCTION / 1024));
            }
            match store.routines.iter_mut().find(|r| r.id == routine.id) {
                Some(existing) => {
                    let rescheduled = existing.cadence != routine.cadence;
                    *existing = merge_edit(existing, routine);
                    if rescheduled {
                        let mut routine_state = core::load_state(&db, &existing.id)?;
                        if !routine_state.running {
                            let now = neko_core::workbench::now_ms();
                            routine_state.next_due_ms = Some(core::next_due_local(&existing.cadence, routine_state.quiet_streak, now));
                            core::save_state(&db, &existing.id, &routine_state)?;
                        }
                    }
                }
                None => store.routines.push(routine),
            }
            core::save_definitions(&db, &store)
        }
        Request::RunRoutineNow { id } => {
            if !store.routines.iter().any(|r| r.id == id) {
                return Err("No such routine".into());
            }
            let mut routine_state = core::load_state(&db, &id)?;
            routine_state.run_requested = true;
            core::save_state(&db, &id, &routine_state)
        }
        Request::SetRoutineEnabled { id, enabled } => {
            let routine = store.routines.iter_mut().find(|r| r.id == id).ok_or("No such routine")?;
            routine.enabled = enabled;
            core::save_definitions(&db, &store)
        }
        Request::SetRoutinesOn { enabled } => core::set_master(&db, enabled),
        Request::SetStandingOrders { workspace_id, text } => {
            if text.len() > core::MAX_ORDERS {
                return Err(format!("Keep standing orders under {} KB", core::MAX_ORDERS / 1024));
            }
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
            inbox::close(item, inbox::BY_YOU, neko_core::workbench::now_ms());
            core::save_runs(&db, &routine_id, &runs)
        }
        Request::ReopenNeedsYou { routine_id, run_id, seq } => {
            let mut runs = core::load_runs(&db, &routine_id)?;
            let item = runs
                .iter_mut()
                .find(|r| r.id == run_id)
                .and_then(|r| r.needs_you.iter_mut().find(|i| i.seq == seq))
                .ok_or("That item is gone")?;
            inbox::reopen(item, neko_core::workbench::now_ms());
            core::save_runs(&db, &routine_id, &runs)
        }
        Request::SetInboxAgeOutDays { days } => {
            if days > inbox::MAX_AGE_OUT_DAYS {
                return Err(format!("Choose {} days or fewer, or 0 to keep items until you close them", inbox::MAX_AGE_OUT_DAYS));
            }
            store.age_out_days = Some(days);
            core::save_definitions(&db, &store)
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
    use neko_protocol::routines::{Cadence, RoutineKind};

    fn routine(id: &str, enabled: bool) -> Routine {
        Routine { id: id.into(), workspace_id: "w".into(), kind: RoutineKind::Custom, name: id.into(), instruction: "x".into(), cadence: Cadence::Daily { hour: 9, minute: 0 }, enabled, model: String::new(), paseo_schedule: None, browser: true, computer: true, customized: false, catch_up: None }
    }

    fn due(ms: Option<i64>, running: bool) -> RoutineState {
        RoutineState { next_due_ms: ms, running, ..Default::default() }
    }

    #[test]
    fn a_routine_out_of_usage_everywhere_waits_for_the_reset_without_failing() {
        let now = neko_core::workbench::now_ms();
        let epoch = now / 1000 + 7200;
        let claude = format!("Claude AI usage limit reached|{epoch}");
        // Claude alone, no other runtime installed: the reset it names.
        assert_eq!(usage_hold(&[claude.as_str()], now), Some(epoch * 1000));
        // Claude and Codex both out: the earlier reset.
        let codex = "{\"type\":\"usage_limit_reached\",\"resets_in_seconds\":900}";
        assert_eq!(usage_hold(&[claude.as_str(), codex], now), Some(now + 900_000));
        // No reset in the error: an hour.
        assert_eq!(usage_hold(&["Credit balance is too low"], now), Some(now + 3_600_000));
        // One runtime still works or failed for another reason: no hold.
        assert_eq!(usage_hold(&[claude.as_str(), "timed out after 900s"], now), None);
        assert_eq!(usage_hold(&[], now), None);
        // The routine is parked at the reset, not counted as failed.
        let mut state = RoutineState { running: true, failures: 0, ..Default::default() };
        park_for_limit(&mut state, epoch * 1000);
        assert_eq!((state.running, state.failures, state.next_due_ms), (false, 0, Some(epoch * 1000)));
        assert_eq!(state.hold.as_ref().map(|h| h.reason.as_str()), Some("usage limit"));
        // Not picked before the reset, picked right after it.
        let store = Store { routines: vec![routine("a", true)], ..Default::default() };
        assert_eq!(pick_due(&store, &[("a".into(), state.clone())], epoch * 1000 - 1, &|_| true), None);
        assert_eq!(pick_due(&store, &[("a".into(), state)], epoch * 1000, &|_| true), Some("a".into()));
    }

    #[test]
    fn pick_due_chooses_most_overdue_enabled() {
        let store = Store { routines: vec![routine("a", true), routine("b", true), routine("c", false), routine("d", true)], ..Default::default() };
        let states = vec![("a".into(), due(Some(50), false)), ("b".into(), due(Some(10), false)), ("c".into(), due(Some(0), false)), ("d".into(), due(Some(500), false))];
        assert_eq!(pick_due(&store, &states, 100, &|_| true).as_deref(), Some("b"));
        assert_eq!(pick_due(&store, &states, 5, &|_| true), None);
        // A routine with no state yet is due now; a running one is skipped.
        let states = vec![("a".into(), due(Some(0), true))];
        assert_eq!(pick_due(&store, &states, 1, &|_| true).as_deref(), Some("b"));
    }

    #[test]
    fn run_now_runs_a_paused_routine_first() {
        let store = Store { routines: vec![routine("a", true), routine("b", false)], ..Default::default() };
        let states = vec![("a".into(), due(Some(0), false)), ("b".into(), RoutineState { next_due_ms: Some(i64::MAX), run_requested: true, ..Default::default() })];
        assert_eq!(pick_due(&store, &states, 100, &|_| true).as_deref(), Some("b"));
        // While it runs, the request waits for the run to end.
        let states = vec![("a".into(), due(Some(i64::MAX), false)), ("b".into(), RoutineState { running: true, run_requested: true, ..Default::default() })];
        assert_eq!(pick_due(&store, &states, 100, &|_| true), None);
    }

    #[test]
    fn repeating_routine_waits_for_its_hours() {
        let every = Routine { cadence: Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 }, ..routine("m", true) };
        let store = Store { routines: vec![every], ..Default::default() };
        let states = vec![("m".into(), due(Some(0), false))];
        assert_eq!(pick_due(&store, &states, 100, &|_| false), None);
        // Run now still runs it at night.
        let states = vec![("m".into(), RoutineState { run_requested: true, ..Default::default() })];
        assert_eq!(pick_due(&store, &states, 100, &|_| false).as_deref(), Some("m"));
    }

    #[test]
    fn open_titles_cover_the_whole_workspace() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let store = Store { routines: vec![routine("a", true), routine("b", true), Routine { workspace_id: "other".into(), ..routine("c", true) }], ..Default::default() };
        core::save_definitions(&db, &store).unwrap();
        let item = |title: &str, done: bool| neko_protocol::routines::NeedsYou { seq: 1, title: title.into(), detail: String::new(), link: None, done, ..Default::default() };
        let run = |rid: &str, items| RoutineRun { id: "x".into(), routine_id: rid.into(), started_ms: 0, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: items, error: None, text: String::new(), ..Default::default() };
        core::push_run(&db, "a", run("a", vec![item("Gmail not signed in", false), item("Old", true)])).unwrap();
        core::push_run(&db, "c", run("c", vec![item("Elsewhere", false)])).unwrap();
        let titles = open_titles(&db, "w").unwrap();
        assert!(titles.contains(&title_key("  gmail NOT signed in ")));
        assert!(!titles.contains("old"));
        assert!(!titles.contains("elsewhere"));
    }

    #[test]
    fn resolved_titles_close_items_across_the_workspace() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let store = Store { routines: vec![routine("a", true), routine("b", true)], ..Default::default() };
        core::save_definitions(&db, &store).unwrap();
        let item = |seq, title: &str| neko_protocol::routines::NeedsYou { seq, title: title.into(), detail: String::new(), link: None, done: false, ..Default::default() };
        let run = |rid: &str, items| RoutineRun { id: rid.into(), routine_id: rid.into(), started_ms: 0, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: items, error: None, text: String::new(), ..Default::default() };
        core::push_run(&db, "a", run("a", vec![item(2, "Gmail API not enabled"), item(3, "Sam")])).unwrap();
        core::push_run(&db, "b", run("b", vec![item(1, "gmail api NOT enabled")])).unwrap();
        let titles = |db: &neko_core::Db| open_items(db, "w", 0).unwrap().into_iter().map(|i| i.title).collect::<Vec<_>>();
        assert_eq!(titles(&db), vec!["gmail api NOT enabled", "Sam"]);
        resolve_items(&db, "w", &["Gmail API not enabled".into()]).unwrap();
        assert_eq!(titles(&db), vec!["Sam"]);
        let closed = &core::load_runs(&db, "a").unwrap()[0].needs_you[0];
        assert!(closed.done && closed.reason.as_deref() == Some("Resolved by routine") && closed.closed_ms.is_some());
    }

    fn inbox_item(seq: u64, title: &str, detail: &str, link: Option<&str>) -> NeedsYou {
        NeedsYou { seq, title: title.into(), detail: detail.into(), link: link.map(Into::into), created_ms: 1_000, touched_ms: 1_000, ..Default::default() }
    }

    fn inbox_fixture(items: Vec<NeedsYou>) -> neko_core::Db {
        let db = neko_core::Db::open_in_memory().unwrap();
        core::save_definitions(&db, &Store { routines: vec![routine("a", true), Routine { workspace_id: "other".into(), ..routine("c", true) }], ..Default::default() }).unwrap();
        let run = RoutineRun { id: "r1".into(), routine_id: "a".into(), started_ms: 1_000, ok: true, needs_you: items, ..Default::default() };
        core::push_run(&db, "a", run).unwrap();
        db
    }

    fn items_of(db: &neko_core::Db, routine: &str) -> Vec<NeedsYou> {
        core::load_runs(db, routine).unwrap().into_iter().flat_map(|r| r.needs_you).collect()
    }

    #[test]
    fn a_repeat_refreshes_the_open_item_instead_of_adding_one() {
        let db = inbox_fixture(vec![inbox_item(1, "Reply to Sam", "old", None), inbox_item(2, "Enable Google Calendar API", "project one", None)]);
        let other = RoutineRun { id: "r2".into(), routine_id: "c".into(), started_ms: 1_000, ok: true, needs_you: vec![inbox_item(3, "Sign in", "x", None)], ..Default::default() };
        core::push_run(&db, "c", other).unwrap();
        let mut reply = vec![
            inbox_item(0, "reply to sam!", "newer detail", None),
            inbox_item(0, "Reply to Sam", "same again", None),
            inbox_item(0, "Google Calendar needs re-auth", "other scope", None),
            inbox_item(0, "Something new", "", None),
        ];
        // Run in the other scope: the setup failure matches across scopes, the plain title does not.
        absorb_repeats(&db, "other", &mut reply, 9_000).unwrap();
        assert_eq!(reply.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), vec!["reply to sam!", "Something new"]);
        assert!(reply.iter().all(|i| i.created_ms == 9_000 && i.touched_ms == 9_000));
        let first = items_of(&db, "a");
        assert_eq!((first[1].detail.as_str(), first[1].touched_ms), ("other scope", 9_000), "setup failure refreshed across scopes");
        assert_eq!(first[0].touched_ms, 1_000, "a plain title is only matched within its scope");
        let mut same_scope = vec![inbox_item(0, "Reply to Sam", "newer detail", None)];
        absorb_repeats(&db, "w", &mut same_scope, 9_500).unwrap();
        assert!(same_scope.is_empty());
        let first = items_of(&db, "a");
        assert_eq!((first[0].detail.as_str(), first[0].touched_ms), ("newer detail", 9_500));
        assert_eq!(first.len(), 2, "nothing was added");
    }

    #[test]
    fn time_closes_expired_and_aged_items_with_a_reason() {
        let day = inbox::DAY_MS;
        let mut expiring = inbox_item(2, "Reply before the call", "", None);
        expiring.expires_ms = Some(50 * day);
        let mut fresh = inbox_item(3, "Fresh", "", None);
        fresh.touched_ms = 59 * day;
        let db = inbox_fixture(vec![inbox_item(1, "Stale", "", None), expiring, fresh]);
        assert_eq!(sweep_time(&db, 60 * day).unwrap(), 2);
        let items = items_of(&db, "a");
        assert_eq!(items.iter().map(|i| i.reason.as_deref()).collect::<Vec<_>>(), vec![Some("Aged out"), Some("Expired"), None]);
        // The setting changes the period; 0 turns it off.
        let mut store = core::load(&db).unwrap();
        store.age_out_days = Some(0);
        core::save_definitions(&db, &store).unwrap();
        let more = RoutineRun { id: "r3".into(), routine_id: "a".into(), started_ms: 1_000, ok: true, needs_you: vec![inbox_item(9, "Ancient", "", None)], ..Default::default() };
        core::push_run(&db, "a", more).unwrap();
        assert_eq!(sweep_time(&db, 400 * day).unwrap(), 0);
    }

    #[test]
    fn a_merged_or_closed_pull_request_closes_its_item() {
        let db = std::sync::Mutex::new(inbox_fixture(vec![
            inbox_item(1, "Review the change", "", Some("https://github.com/acme/widgets/pull/5")),
            inbox_item(2, "CI is red", "on acme/widgets#6", None),
            inbox_item(3, "Still going", "acme/widgets#7", None),
            inbox_item(4, "No pull request", "", None),
            NeedsYou { kept: true, ..inbox_item(5, "Reopened", "acme/widgets#5", None) },
        ]));
        let calls = std::cell::Cell::new(0);
        let fetch = |r: &inbox::PrRef| {
            calls.set(calls.get() + 1);
            Some((match r.number { 5 => "MERGED", 6 => "CLOSED", _ => "OPEN" }.to_string(), 5_000))
        };
        assert_eq!(sweep_prs(&db, 5_000, &fetch).unwrap(), 2);
        let items = items_of(&db.lock().unwrap(), "a");
        assert_eq!(items.iter().map(|i| i.reason.as_deref()).collect::<Vec<_>>(), vec![Some("PR merged"), Some("PR closed"), None, None, None]);
        assert_eq!(calls.get(), 3);
        // Terminal states are remembered; an open one is read again only after 15 minutes.
        assert_eq!(sweep_prs(&db, 6_000, &fetch).unwrap(), 0);
        assert_eq!(calls.get(), 3, "nothing was asked again within 15 minutes");
        sweep_prs(&db, 5_000 + PR_SWEEP_EVERY_MS, &fetch).unwrap();
        assert_eq!(calls.get(), 4);
    }

    #[test]
    fn at_most_a_few_lookups_per_sweep() {
        let items: Vec<NeedsYou> = (1..=10).map(|n| inbox_item(n, &format!("Item {n}"), &format!("acme/widgets#{}", 100 + n), None)).collect();
        let db = std::sync::Mutex::new(inbox_fixture(items));
        let calls = std::cell::Cell::new(0);
        let fetch = |_: &inbox::PrRef| {
            calls.set(calls.get() + 1);
            Some(("OPEN".to_string(), 0))
        };
        sweep_prs(&db, 5_000, &fetch).unwrap();
        assert_eq!(calls.get(), MAX_PR_CALLS);
        sweep_prs(&db, 6_000, &fetch).unwrap();
        assert_eq!(calls.get(), 10, "the rest were read on the next sweep");
    }

    #[test]
    fn unreadable_references_do_not_starve_the_real_ones() {
        let mut items: Vec<NeedsYou> = (1..=7).map(|n| inbox_item(n, &format!("Bad {n}"), &format!("acme/private#{n}"), None)).collect();
        items.push(inbox_item(8, "Real", "", Some("https://github.com/acme/widgets/pull/99")));
        let db = std::sync::Mutex::new(inbox_fixture(items));
        let fetch = |r: &inbox::PrRef| (r.number == 99).then(|| ("MERGED".to_string(), 5_000));
        sweep_prs(&db, 5_000, &fetch).unwrap();
        sweep_prs(&db, 6_000, &fetch).unwrap();
        let items = items_of(&db.lock().unwrap(), "a");
        assert_eq!(items[7].reason.as_deref(), Some("PR merged"), "failures are remembered, so the real pull request is reached");
    }

    #[test]
    fn a_pull_request_that_ended_before_the_item_was_raised_is_only_a_mention() {
        let db = std::sync::Mutex::new(inbox_fixture(vec![inbox_item(1, "Deploy after the fix lands", "waiting on acme/widgets#3", None)]));
        let old = |_: &inbox::PrRef| Some(("MERGED".to_string(), 1));
        let mut tight = items_of(&db.lock().unwrap(), "a");
        tight[0].created_ms = 10 * 60 * 1000;
        let run = RoutineRun { id: "r1".into(), routine_id: "a".into(), started_ms: 1_000, ok: true, needs_you: tight, ..Default::default() };
        core::save_runs(&db.lock().unwrap(), "a", &[run]).unwrap();
        assert_eq!(sweep_prs(&db, 20 * 60 * 1000, &old).unwrap(), 0);
    }

    #[test]
    fn a_fixed_setup_failure_closes_in_every_scope_and_an_outage_is_never_aged() {
        let db = inbox_fixture(vec![inbox_item(1, "Slack token expired", "", None), inbox_item(2, SIGNED_OUT_TITLE, "", None)]);
        let other = RoutineRun { id: "r2".into(), routine_id: "c".into(), started_ms: 1_000, ok: true, needs_you: vec![inbox_item(3, "Re-auth Slack", "", None)], ..Default::default() };
        core::push_run(&db, "c", other).unwrap();
        assert_eq!(sweep_time(&db, 100 * inbox::DAY_MS).unwrap(), 2, "the two Slack items age; the outage notice waits for Claude");
        assert!(!items_of(&db, "a")[1].done);
        let db = inbox_fixture(vec![inbox_item(1, "Slack token expired", "", None)]);
        let other = RoutineRun { id: "r2".into(), routine_id: "c".into(), started_ms: 1_000, ok: true, needs_you: vec![inbox_item(3, "Re-auth Slack", "", None)], ..Default::default() };
        core::push_run(&db, "c", other).unwrap();
        resolve_items(&db, "w", &["Slack token expired".into()]).unwrap();
        assert!(items_of(&db, "a")[0].done && items_of(&db, "c")[0].done);
    }

    #[test]
    fn a_closed_item_survives_the_run_trim_so_it_can_be_reopened() {
        let db = inbox_fixture(vec![inbox_item(1, "Old", "", None)]);
        let now = neko_core::workbench::now_ms();
        let mut runs = core::load_runs(&db, "a").unwrap();
        inbox::close(&mut runs[0].needs_you[0], inbox::AGED_OUT, now);
        core::save_runs(&db, "a", &runs).unwrap();
        for n in 0..55 {
            core::push_run(&db, "a", RoutineRun { id: format!("n{n}"), routine_id: "a".into(), started_ms: 2_000 + n, ok: true, ..Default::default() }).unwrap();
        }
        assert!(core::load_runs(&db, "a").unwrap().iter().any(|r| r.id == "r1"), "the run with a recently closed item is kept");
    }

    #[test]
    fn closed_items_reopen_and_stay_in_the_view_for_a_few_days() {
        let db = inbox_fixture(vec![inbox_item(1, "Merged thing", "", None)]);
        let mut runs = core::load_runs(&db, "a").unwrap();
        inbox::close(&mut runs[0].needs_you[0], inbox::PR_MERGED, 10 * inbox::DAY_MS);
        let mut old = runs[0].clone();
        old.id = "old".into();
        let mut filler: Vec<RoutineRun> = (0..VIEW_RUNS).map(|n| RoutineRun { id: format!("f{n}"), routine_id: "a".into(), started_ms: 5_000 + n as i64, ..Default::default() }).collect();
        filler.push(old);
        assert_eq!(runs_for_view(filler.clone(), 11 * inbox::DAY_MS).len(), VIEW_RUNS + 1, "closed yesterday: still listed");
        assert_eq!(runs_for_view(filler, 20 * inbox::DAY_MS).len(), VIEW_RUNS, "closed long ago: gone");
        inbox::reopen(&mut runs[0].needs_you[0], 12 * inbox::DAY_MS);
        let item = &runs[0].needs_you[0];
        assert!(!item.done && item.kept && item.reason.is_none());
    }

    #[test]
    fn a_failure_alert_is_not_raised_twice() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let sweep = Routine { name: "PR sweep".into(), ..routine("a", true) };
        core::save_definitions(&db, &Store { routines: vec![sweep.clone()], ..Default::default() }).unwrap();
        assert!(!alert_open(&db, &sweep).unwrap());
        let alert = neko_protocol::routines::NeedsYou { seq: 1, title: failure_title(&sweep), detail: String::new(), link: None, done: false, ..Default::default() };
        let run = RoutineRun { id: "x".into(), routine_id: "a".into(), started_ms: 0, finished_ms: None, ok: false, quiet: false, did: vec![], needs_you: vec![alert], error: None, text: String::new(), ..Default::default() };
        core::push_run(&db, "a", run).unwrap();
        assert!(alert_open(&db, &sweep).unwrap());
        resolve_items(&db, "w", &[failure_title(&sweep)]).unwrap();
        assert!(!alert_open(&db, &sweep).unwrap());
    }

    #[test]
    fn daily_retries_once_then_follows_cadence() {
        let daily = Cadence::Daily { hour: 10, minute: 30 };
        let first = RoutineState { failures: 1, ..Default::default() };
        assert_eq!(retry_or_next(&daily, &first, 1_000), 1_000 + DAILY_RETRY_MS);
        let second = RoutineState { failures: 2, ..Default::default() };
        assert!(retry_or_next(&daily, &second, 1_000) > 1_000 + DAILY_RETRY_MS);
    }

    #[test]
    fn edit_keeps_enabled_and_placement() {
        let existing = Routine { enabled: false, paseo_schedule: Some("p".into()), ..routine("a", false) };
        let edit = Routine { enabled: true, workspace_id: "other".into(), paseo_schedule: None, instruction: "new".into(), cadence: Cadence::Daily { hour: 7, minute: 0 }, ..routine("a", true) };
        let merged = merge_edit(&existing, edit);
        assert!(!merged.enabled);
        assert_eq!(merged.workspace_id, "w");
        assert_eq!(merged.paseo_schedule.as_deref(), Some("p"));
        assert_eq!(merged.instruction, "new");
        assert_eq!(merged.cadence, Cadence::Daily { hour: 7, minute: 0 });
        assert!(merged.customized, "a changed instruction is the owner's");
        let same_words = merge_edit(&merged, Routine { instruction: "new".into(), ..routine("a", true) });
        assert!(same_words.customized, "and stays theirs");
        let untouched = merge_edit(&routine("b", true), Routine { cadence: Cadence::Daily { hour: 8, minute: 0 }, ..routine("b", true) });
        assert!(!untouched.customized, "a cadence change leaves the words Neko's");
    }

    #[test]
    fn helper_command_is_killed_after_its_limit() {
        let mut sleep = std::process::Command::new("/bin/sleep");
        sleep.arg("5");
        let started = std::time::Instant::now();
        assert!(output_within(sleep, Duration::from_millis(300)).unwrap_err().starts_with("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
        let mut echo = std::process::Command::new("/bin/echo");
        echo.arg("hi");
        assert_eq!(String::from_utf8_lossy(&output_within(echo, Duration::from_secs(5)).unwrap().stdout).trim(), "hi");
    }

    #[test]
    fn busy_guard_clears_on_panic() {
        BUSY.store(true, Ordering::SeqCst);
        let _ = std::panic::catch_unwind(|| {
            let _busy = BusyGuard;
            panic!("run failed");
        });
        assert!(!BUSY.load(Ordering::SeqCst));
    }

    #[test]
    fn view_keeps_old_runs_with_open_items() {
        let run = |id: usize, open: bool| RoutineRun {
            id: id.to_string(), routine_id: "r".into(), started_ms: 0, finished_ms: None, ok: true, quiet: false, did: vec![],
            needs_you: if open { vec![neko_protocol::routines::NeedsYou { seq: id as u64, title: "t".into(), detail: String::new(), link: None, done: false, ..Default::default() }] } else { vec![] },
            error: None, text: String::new(), ..Default::default()
        };
        let runs: Vec<_> = (0..30).map(|i| run(i, i == 25)).collect();
        let shown = runs_for_view(runs, 0);
        assert_eq!(shown.len(), VIEW_RUNS + 1);
        assert_eq!(shown.last().unwrap().id, "25");
    }

    #[test]
    fn active_schedule_is_found_by_name_in_paseo_listing() {
        let listing = r#"[{"id":"a95bb666","name":"daily-work-digest","status":"active"},{"id":"x","name":"weekly-pr-sweep","status":"paused"}]"#;
        assert_eq!(active_schedule_id(listing, "daily-work-digest").as_deref(), Some("a95bb666"));
        assert_eq!(active_schedule_id(listing, "weekly-pr-sweep"), None);
        assert_eq!(active_schedule_id(listing, "message-watch-adaptive"), None);
        assert_eq!(active_schedule_id("not json", "daily-work-digest"), None);
    }

    #[test]
    fn the_signed_out_notice_is_raised_once_and_closes_when_claude_works() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let r = routine("a", true);
        core::save_definitions(&db, &Store { routines: vec![r.clone()], ..Default::default() }).unwrap();
        let mut items = Vec::new();
        add_outage_notice(&db, &r, &AgentMeta { fallback: true, ..Default::default() }, &mut items).unwrap();
        assert!(items.is_empty(), "no outage found by this run, no notice");
        neko_core::runtime_health::mark_bad(&db, "OAuth session expired", 5).unwrap();
        add_outage_notice(&db, &r, &AgentMeta { new_outage: true, fallback: true, ..Default::default() }, &mut items).unwrap();
        assert_eq!(items.len(), 1);
        assert!(items[0].detail.contains("claude /login") && items[0].detail.contains("Codex"));
        let health = neko_core::runtime_health::load(&db);
        assert_eq!(health.notice, Some(("w".into(), SIGNED_OUT_TITLE.into())));
        let mut again = Vec::new();
        add_outage_notice(&db, &r, &AgentMeta { new_outage: true, fallback: true, ..Default::default() }, &mut again).unwrap();
        assert!(again.is_empty(), "a concurrent run in the same outage raises nothing");
        // The run that raised it is saved, then Claude works again.
        let run = RoutineRun { id: "x".into(), routine_id: "a".into(), ok: true, needs_you: items, ..Default::default() };
        core::push_run(&db, "a", run).unwrap();
        let (workspace, title) = health.notice.unwrap();
        resolve_items(&db, &workspace, &[title]).unwrap();
        assert!(core::load_runs(&db, "a").unwrap()[0].needs_you[0].done);
        // An outage that ended while a run was on Codex raises nothing.
        neko_core::runtime_health::mark_good(&db).unwrap();
        add_outage_notice(&db, &r, &AgentMeta { new_outage: true, fallback: true, ..Default::default() }, &mut again).unwrap();
        assert!(again.is_empty());
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
