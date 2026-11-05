//! Runs routines (`neko_core::routines`): one thread wakes every 30 seconds
//! and starts the most overdue enabled routine, at most one at a time, never
//! while the machine steward holds new work. A run is Claude Code in the
//! scope's folder with the owner's full setup, so it has that folder's MCP
//! servers and sign-ins. It acts, then its reply's json block becomes a
//! `RoutineRun`. See docs/knowledge/neko-runs-the-day.md.
use crate::server::AppState;
use neko_core::routines::{self as core, Store};
use neko_protocol::routines::{Cadence, NeedsYou, Routine, RoutineNotice, RoutineRun, RoutineState, RoutineView, ScopeProposal};
use neko_protocol::workbench::{AgentRuntime, Command, Workspace};
use neko_protocol::{Request, Response};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
        for routine in store.routines.iter_mut().filter(|r| r.kind != neko_protocol::routines::RoutineKind::Custom) {
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
    Routine { id: existing.id.clone(), workspace_id: existing.workspace_id.clone(), kind: existing.kind, enabled: existing.enabled, paseo_schedule: existing.paseo_schedule.clone(), ..edit }
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

/// The newest runs, plus any older run that still has an item open, so an
/// unhandled item never drops off the page.
pub(crate) fn runs_for_view(mut runs: Vec<RoutineRun>) -> Vec<RoutineRun> {
    let older = runs.split_off(runs.len().min(VIEW_RUNS));
    runs.extend(older.into_iter().filter(|run| run.needs_you.iter().any(|item| !item.done)));
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
        let scope = scopes(state, false)?.into_iter().find(|s| s.workspace_id.as_deref() == Some(routine.workspace_id.as_str()));
        (routine, scope)
    };
    {
        let db = db(state);
        let mut routine_state = core::load_state(&db, &routine.id)?;
        routine_state.running = true;
        routine_state.run_requested = false;
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

/// Open item titles in this workspace, oldest first, without repeats.
fn open_items(db: &neko_core::Db, workspace_id: &str) -> Result<Vec<String>, String> {
    let mut items: Vec<(u64, String)> = Vec::new();
    for routine in core::load(db)?.routines.iter().filter(|r| r.workspace_id == workspace_id) {
        for run in core::load_runs(db, &routine.id).unwrap_or_default() {
            items.extend(run.needs_you.into_iter().filter(|i| !i.done).map(|i| (i.seq, i.title)));
        }
    }
    items.sort();
    let mut seen = std::collections::HashSet::new();
    Ok(items.into_iter().filter(|(_, t)| seen.insert(title_key(t))).map(|(_, t)| t).take(MAX_OPEN_IN_PROMPT).collect())
}

/// Marks done every open item in the workspace whose title a run resolved.
fn resolve_items(db: &neko_core::Db, workspace_id: &str, titles: &[String]) -> Result<(), String> {
    if titles.is_empty() {
        return Ok(());
    }
    let keys: std::collections::HashSet<String> = titles.iter().map(|t| title_key(t)).collect();
    for routine in core::load(db)?.routines.iter().filter(|r| r.workspace_id == workspace_id) {
        let mut runs = core::load_runs(db, &routine.id).unwrap_or_default();
        let mut changed = false;
        for item in runs.iter_mut().flat_map(|r| r.needs_you.iter_mut()).filter(|i| !i.done && keys.contains(&title_key(&i.title))) {
            item.done = true;
            changed = true;
        }
        if changed {
            core::save_runs(db, &routine.id, &runs)?;
        }
    }
    Ok(())
}

fn failure_title(routine: &Routine) -> String {
    format!("{} failed", routine.name)
}

/// Whether this routine's failure alert is already waiting on the owner.
fn alert_open(db: &neko_core::Db, routine: &Routine) -> Result<bool, String> {
    Ok(open_titles(db, &routine.workspace_id)?.contains(&title_key(&failure_title(routine))))
}

fn title_key(title: &str) -> String {
    title.trim().to_lowercase()
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
                let alert = NeedsYou { seq, title: failure_title(routine), detail: format!("Neko could not save this run: {error}"), link: None, done: false };
                let run = RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: now, finished_ms: Some(now), ok: false, quiet: false, did: Vec::new(), needs_you: vec![alert], error: Some(error.clone()), text: String::new() };
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
    let (orders, memory, open) = {
        let db = db(state);
        (core::load(&db)?.orders_for(&routine.workspace_id).to_string(), core::load_state(&db, &routine.id)?.memory, open_items(&db, &routine.workspace_id)?)
    };
    let outcome: Result<core::Parsed, (String, String)> = match scope.as_ref().and_then(|s| s.folders.first().map(|f| (s, f))) {
        None => Err(("This routine's workspace no longer exists".into(), String::new())),
        Some((scope, folder)) => {
            let folder = PathBuf::from(folder);
            if !folder.is_dir() {
                Err((format!("Scope folder missing: {}", folder.display()), String::new()))
            } else {
                let spec = neko_core::native_runner::RunSpec {
                    directory: folder,
                    prompt: core::build_prompt(routine, scope, &orders, &memory, &core::now_iso(), &open, &core::owner_name())
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
                match neko_core::native_runner::run_claude_scoped(&spec, &config, routine.browser, &cancel, |_| {}) {
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
    let mut next_due = None;
    let run = match outcome {
        Ok(parsed) => {
            // A good run closes this routine's own failure alert.
            let mut resolved = parsed.resolved.clone();
            resolved.push(failure_title(&current));
            resolve_items(&db, &routine.workspace_id, &resolved)?;
            let mut needs_you = parsed.needs_you;
            // Another routine in this scope may already have raised it.
            let open = open_titles(&db, &routine.workspace_id)?;
            needs_you.retain(|item| !open.contains(&title_key(&item.title)));
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
            RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: started, finished_ms: Some(finished), ok: true, quiet: parsed.quiet, did: parsed.did, needs_you, error: None, text: String::new() }
        }
        Err((error, text)) => {
            let mut end = text.len().min(KEEP_TEXT);
            while !text.is_char_boundary(end) {
                end -= 1;
            }
            routine_state.failures = routine_state.failures.saturating_add(1);
            let mut needs_you = Vec::new();
            if core::failure_alerts(&current.cadence, routine_state.failures) && !alert_open(&db, &current)? {
                needs_you.push(NeedsYou {
                    seq: core::reserve_seq(&db, 1)?,
                    title: failure_title(&current),
                    detail: format!("{error} Neko will try again{}.", if matches!(current.cadence, Cadence::Daily { .. }) { " in 30 minutes" } else { " on schedule" }),
                    link: None,
                    done: false,
                });
            }
            next_due = Some(retry_or_next(&current.cadence, &routine_state, finished));
            RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: started, finished_ms: Some(finished), ok: false, quiet: false, did: Vec::new(), needs_you, error: Some(error), text: text[..end].to_string() }
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
        let runs = runs_for_view(core::load_runs(&db, &routine.id)?);
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
    use neko_protocol::routines::{Cadence, RoutineKind};

    fn routine(id: &str, enabled: bool) -> Routine {
        Routine { id: id.into(), workspace_id: "w".into(), kind: RoutineKind::Custom, name: id.into(), instruction: "x".into(), cadence: Cadence::Daily { hour: 9, minute: 0 }, enabled, model: String::new(), paseo_schedule: None, browser: true, computer: true }
    }

    fn due(ms: Option<i64>, running: bool) -> RoutineState {
        RoutineState { next_due_ms: ms, running, ..Default::default() }
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
        let item = |title: &str, done: bool| neko_protocol::routines::NeedsYou { seq: 1, title: title.into(), detail: String::new(), link: None, done };
        let run = |rid: &str, items| RoutineRun { id: "x".into(), routine_id: rid.into(), started_ms: 0, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: items, error: None, text: String::new() };
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
        let item = |seq, title: &str| neko_protocol::routines::NeedsYou { seq, title: title.into(), detail: String::new(), link: None, done: false };
        let run = |rid: &str, items| RoutineRun { id: rid.into(), routine_id: rid.into(), started_ms: 0, finished_ms: None, ok: true, quiet: false, did: vec![], needs_you: items, error: None, text: String::new() };
        core::push_run(&db, "a", run("a", vec![item(2, "Gmail API not enabled"), item(3, "Sam")])).unwrap();
        core::push_run(&db, "b", run("b", vec![item(1, "gmail api NOT enabled")])).unwrap();
        assert_eq!(open_items(&db, "w").unwrap(), vec!["gmail api NOT enabled", "Sam"]);
        resolve_items(&db, "w", &["Gmail API not enabled".into()]).unwrap();
        assert_eq!(open_items(&db, "w").unwrap(), vec!["Sam"]);
    }

    #[test]
    fn a_failure_alert_is_not_raised_twice() {
        let db = neko_core::Db::open_in_memory().unwrap();
        let sweep = Routine { name: "PR sweep".into(), ..routine("a", true) };
        core::save_definitions(&db, &Store { routines: vec![sweep.clone()], ..Default::default() }).unwrap();
        assert!(!alert_open(&db, &sweep).unwrap());
        let alert = neko_protocol::routines::NeedsYou { seq: 1, title: failure_title(&sweep), detail: String::new(), link: None, done: false };
        let run = RoutineRun { id: "x".into(), routine_id: "a".into(), started_ms: 0, finished_ms: None, ok: false, quiet: false, did: vec![], needs_you: vec![alert], error: None, text: String::new() };
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
            needs_you: if open { vec![neko_protocol::routines::NeedsYou { seq: id as u64, title: "t".into(), detail: String::new(), link: None, done: false }] } else { vec![] },
            error: None, text: String::new(),
        };
        let runs: Vec<_> = (0..30).map(|i| run(i, i == 25)).collect();
        let shown = runs_for_view(runs);
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
