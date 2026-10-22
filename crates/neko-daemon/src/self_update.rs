//! Neko updating itself from its own checkout. One thread, woken by the
//! update commands and otherwise every few seconds:
//!
//! 1. reads what the detached installer reported (`result.json`);
//! 2. notices a Neko task's landed commit on the checkout's `main` (or the
//!    owner's "Update Neko now") and decides it (`neko_core::self_update::
//!    decide`): nothing, queue, or wait for a click;
//! 3. web part: builds `web/` into `<data>/web-live.next`, smoke checks it and
//!    swaps it in atomically; open pages are told (`Event::WebUpdated`);
//! 4. native part: once nothing is running or publishing and `machine-ok`
//!    says go, hands the reinstall to `scripts/self-update.sh`, started in its
//!    own process group so it outlives this daemon (which the install stops).
//!    The script installs, relaunches, health-checks, restores the newest
//!    backup on failure, and writes `result.json` for the next daemon.
//!
//! Decisions are pure and tested in `neko_core::self_update`; this file is the
//! I/O around them. See AGENTS.md, "Neko updates itself".

use neko_core::self_update::{self as core, Decision, IdleInputs};
use neko_core::workbench as store;
use neko_protocol::workbench::{ChatAction, ChatActionStatus, Snapshot, UpdatePlan, UpdateRecord};
use neko_protocol::Event;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::server::AppState;

const TICK: Duration = Duration::from_secs(10);
/// A reinstall that has not reported after this long is recorded as failed.
const INSTALL_DEADLINE_MS: i64 = 60 * 60 * 1000;

static WAKE: (Mutex<bool>, Condvar) = (Mutex::new(false), Condvar::new());

/// Run a tick now (after an update command).
pub fn wake() {
    let (flag, condvar) = &WAKE;
    *flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
    condvar.notify_all();
}

fn sleep_until_woken(timeout: Duration) {
    let (flag, condvar) = &WAKE;
    let guard = flag.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let (mut guard, _) = condvar.wait_timeout_while(guard, timeout, |woken| !*woken)
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *guard = false;
}

pub fn spawn(state: Arc<AppState>) {
    if std::env::var_os("NEKO_SELF_UPDATE").is_some_and(|v| v == "0") {
        eprintln!("neko-daemon: self-update off (NEKO_SELF_UPDATE=0)");
        return;
    }
    std::thread::Builder::new().name("neko-self-update".into()).spawn(move || loop {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tick(&state, &data_dir()))) {
            Ok(Err(error)) => eprintln!("neko self-update: {error}"),
            Err(_) => eprintln!("neko self-update: tick panicked; continuing"),
            Ok(Ok(())) => {}
        }
        sleep_until_woken(TICK);
    }).expect("spawn self-update thread");
}

fn data_dir() -> PathBuf {
    neko_protocol::support_dir()
}

/// Loads, changes and saves the snapshot under the lock; returns what `f` did.
fn with_snapshot<T>(state: &AppState, f: impl FnOnce(&mut Snapshot) -> T) -> Result<T, String> {
    let db = state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut snapshot = store::load(&db)?;
    let out = f(&mut snapshot);
    store::save(&db, &snapshot)?;
    Ok(out)
}

/// Says something on the task that brought the update (when there is one)
/// and in the chat; `apply` adds the one-click "Apply update" action.
fn tell(state: &AppState, task_id: Option<&str>, text: &str, apply: bool) {
    let db = state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let mut ticket_ids = Vec::new();
    if let Some(id) = task_id {
        if let Ok(mut snapshot) = store::load(&db) {
            if let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == id) {
                store::append_event(task, "supervisor", text);
                task.updated_at_ms = store::now_ms();
                ticket_ids.push(id.to_owned());
                let _ = store::save(&db, &snapshot);
            }
        }
    }
    let actions = match (apply, task_id) {
        (true, Some(id)) => vec![ChatAction {
            id: store::new_id(), task_id: id.to_owned(), kind: "apply_update".into(),
            text: String::new(), status: ChatActionStatus::Proposed, note: String::new(),
        }],
        _ => Vec::new(),
    };
    if let Err(error) = neko_core::neko_chat::post_notice(&db, text, ticket_ids, actions) {
        eprintln!("neko self-update: cannot post to chat: {error}");
    }
}

fn record(commit: &str, summary: &str) -> UpdateRecord {
    UpdateRecord { commit: commit.to_owned(), at_ms: store::now_ms(), summary: summary.to_owned() }
}

fn tick(state: &Arc<AppState>, data: &Path) -> Result<(), String> {
    tick_with(state, data, core::bundle_commit())
}

/// One pass. `bundle` is the running app's commit (`None` outside an
/// installed app, where there is nothing to reinstall).
fn tick_with(state: &Arc<AppState>, data: &Path, bundle: Option<String>) -> Result<(), String> {
    consume_install_report(state, data)?;
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let su = snapshot.self_update.clone();

    if let Some(installing) = &su.installing {
        if store::now_ms() - installing.at_ms > INSTALL_DEADLINE_MS {
            let summary = "The reinstall never reported back; check self-update/install.log in Neko's data folder.";
            with_snapshot(state, |s| {
                s.self_update.installing = None;
                s.self_update.status.clear();
                s.self_update.last_failure = Some(record(&installing.to, summary));
            })?;
            tell(state, installing.task_id.as_deref(), summary, false);
        }
        return Ok(());
    }

    let candidates: Vec<String> = su.checkout.iter().cloned()
        .chain(snapshot.land_on_main.iter().cloned())
        .collect();
    let Some(checkout) = core::find_checkout(candidates.iter()) else {
        if su.requested || su.queued.is_some() {
            with_snapshot(state, |s| {
                s.self_update.requested = false;
                s.self_update.status = "Neko's own checkout was not found. Turn on “Land on main” for the neko repository.".into();
            })?;
        }
        return Ok(());
    };
    let head = core::main_head(&checkout)?;
    let checkout_text = checkout.to_string_lossy().into_owned();

    // The first time: start from what is running, so history is not replayed.
    let applied = match &su.applied_through {
        Some(applied) if core::is_ancestor(&checkout, applied, &head) => applied.clone(),
        _ => bundle.clone().filter(|b| core::is_ancestor(&checkout, b, &head)).unwrap_or_else(|| head.clone()),
    };
    // A newer app (installed by hand) outranks an older live web UI.
    let stale_web = match (&su.web_commit, &bundle) {
        (Some(web), Some(bundle)) => web == bundle || !core::is_ancestor(&checkout, bundle, web),
        _ => false,
    };
    if stale_web {
        core::clear_web(data);
    }
    with_snapshot(state, |s| {
        let u = &mut s.self_update;
        u.checkout = Some(checkout_text.clone());
        u.applied_through = Some(applied.clone());
        if bundle.is_some() { u.native_commit = bundle.clone(); }
        if stale_web { u.web_commit = None; }
        // Pending only because auto-update was off: go once it is on again.
        if u.queued.is_none() && u.pending.as_ref().is_some_and(|p| core::releasable(p, u)) {
            u.queued = u.pending.take();
        }
    })?;

    plan(state, &checkout, &head, &applied, bundle.as_deref())?;
    apply(state, &checkout, data, bundle.as_deref())
}

/// Turns new commits on `main` into a queued or pending update.
fn plan(state: &Arc<AppState>, checkout: &Path, head: &str, applied: &str, bundle: Option<&str>) -> Result<(), String> {
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let su = &snapshot.self_update;
    if su.queued.is_some() {
        return Ok(());
    }
    let requested = su.requested;
    // The task whose landed commit is new since the last applied one.
    let landed = snapshot.tasks.iter()
        .filter_map(|t| t.landed_commit.as_deref().map(|c| (t, c)))
        .filter(|(_, c)| *c != applied && core::is_ancestor(checkout, c, head) && !core::is_ancestor(checkout, c, applied))
        .max_by_key(|(t, _)| t.updated_at_ms)
        .map(|(t, _)| t.id.clone());
    let already_pending = su.pending.as_ref().is_some_and(|p| p.to == head);
    if !requested && (landed.is_none() || already_pending) {
        return Ok(());
    }
    // "Update now" also reinstalls an app that is behind, even when the web
    // part was already applied.
    let from = match bundle {
        Some(b) if requested && b != head && core::is_ancestor(checkout, b, head) => b.to_owned(),
        _ => applied.to_owned(),
    };
    if from == head {
        if requested {
            with_snapshot(state, |s| { s.self_update.requested = false; s.self_update.status = "Neko is up to date.".into(); })?;
        }
        return Ok(());
    }
    let paths = core::changed_paths(checkout, &from, head)?;
    let task_id = landed.or_else(|| su.pending.as_ref().and_then(|p| p.task_id.clone()));
    let decision = core::decide(Some(&from), head, &paths, task_id, requested, su, store::now_ms());
    match decision {
        Decision::Nothing => {
            with_snapshot(state, |s| {
                s.self_update.requested = false;
                s.self_update.applied_through = Some(head.to_owned());
                s.self_update.status = if requested { "Neko is up to date.".into() } else { String::new() };
            })?;
        }
        Decision::Queue(plan) => {
            with_snapshot(state, |s| {
                s.self_update.requested = false;
                s.self_update.pending = None;
                s.self_update.queued = Some(plan);
            })?;
        }
        Decision::Pending(plan) => {
            let text = format!("Neko has an update waiting for you ({}): {} Apply it from Settings → System, or here.", core::short(&plan.to), plan.reason);
            let task_id = plan.task_id.clone();
            with_snapshot(state, |s| {
                s.self_update.requested = false;
                s.self_update.status.clear();
                s.self_update.pending = Some(plan);
            })?;
            tell(state, task_id.as_deref(), &text, true);
        }
    }
    Ok(())
}

/// Carries out the queued update: the web part now, the native part when idle.
fn apply(state: &Arc<AppState>, checkout: &Path, data: &Path, bundle: Option<&str>) -> Result<(), String> {
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let Some(mut queued) = snapshot.self_update.queued.clone() else { return Ok(()) };
    let set_status = |text: String| with_snapshot(state, |s| s.self_update.status = text);

    if queued.web {
        if let Some(wait) = core::machine_wait() {
            return set_status(format!("Waiting for the Mac to settle before building the web UI ({wait})."));
        }
        let head = core::head(checkout)?;
        if head != queued.to && !core::is_ancestor(checkout, &queued.to, &head) {
            return set_status("Waiting: the neko checkout is not on main.".into());
        }
        if core::dirty(checkout, Some("web")) {
            return set_status("Waiting: web/ in the neko checkout has uncommitted changes.".into());
        }
        set_status(format!("Building the web UI at {}…", core::short(&head)))?;
        let lockfile = core::changed_paths(checkout, queued.from.as_deref().unwrap_or(&head), &head)
            .map(|p| p.iter().any(|p| p == "web/pnpm-lock.yaml" || p == "web/package.json"))
            .unwrap_or(true);
        let outcome = core::build_web(checkout, data, lockfile).and_then(|()| core::swap_in(data, core::short(&head)));
        let task_id = queued.task_id.clone();
        match outcome {
            Ok(()) => {
                let summary = format!("Neko updated itself to {} (UI live).", core::short(&head));
                with_snapshot(state, |s| {
                    s.self_update.web_commit = Some(head.clone());
                    s.self_update.last_update = Some(record(&head, &summary));
                })?;
                crate::server::broadcast_event(state, &Event::WebUpdated { commit: head.clone() });
                tell(state, task_id.as_deref(), &summary, false);
            }
            Err(error) => {
                let summary = format!("Couldn't update the web UI to {}: {error}. The current UI stays.", core::short(&head));
                with_snapshot(state, |s| s.self_update.last_failure = Some(record(&head, &summary)))?;
                tell(state, task_id.as_deref(), &summary, false);
            }
        }
        queued.web = false;
        with_snapshot(state, |s| s.self_update.queued = Some(queued.clone()))?;
    }

    if queued.native {
        match bundle {
            // Not an installed app (a development run): nothing to reinstall.
            None => queued.native = false,
            // Already installed by hand.
            Some(b) if b == queued.to => queued.native = false,
            Some(_) => {
                let (workers, publishing, chat_running) = state.workbench.busy();
                let head = core::head(checkout).unwrap_or_default();
                let inputs = IdleInputs {
                    workers, publishing, chat_running,
                    machine_wait: core::machine_wait()
                        .or_else(|| neko_core::machine::latest().and_then(|h| h.holding)),
                    checkout_at_target: head == queued.to,
                    checkout_dirty: core::dirty(checkout, None),
                };
                if let Err(reason) = core::idle_gate(&inputs) {
                    return set_status(reason);
                }
                start_installer(state, checkout, data, &queued)?;
                queued.native = false;
            }
        }
    }

    with_snapshot(state, |s| {
        s.self_update.applied_through = Some(queued.to.clone());
        s.self_update.queued = None;
        if s.self_update.installing.is_none() { s.self_update.status.clear(); }
    })?;
    Ok(())
}

/// Hands the reinstall to `scripts/self-update.sh`, in its own process group
/// so it survives this daemon being stopped by the install it runs.
fn start_installer(state: &Arc<AppState>, checkout: &Path, data: &Path, plan: &UpdatePlan) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let script = checkout.join("scripts/self-update.sh");
    if !script.is_file() {
        return Err("scripts/self-update.sh is missing from the checkout".into());
    }
    let folder = data.join("self-update");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(core::result_path(data));
    let log = std::fs::File::create(folder.join("install.log")).map_err(|e| e.to_string())?;
    let port = crate::web::configured_port().unwrap_or(0).to_string();
    let mut command = std::process::Command::new("/bin/bash");
    command.arg(&script)
        .args(["--checkout", &checkout.to_string_lossy(), "--data-dir", &data.to_string_lossy(), "--commit", &plan.to, "--web-port", &port])
        .current_dir(checkout)
        .env("CARGO_BUILD_JOBS", "2")
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .process_group(0);
    command.spawn().map_err(|e| format!("cannot start the installer: {e}"))?;
    let text = format!("Reinstalling Neko at {} now that it is idle. It restarts on its own and rolls back if the new version is unhealthy.", core::short(&plan.to));
    with_snapshot(state, |s| {
        s.self_update.installing = Some(UpdatePlan { at_ms: store::now_ms(), ..plan.clone() });
        s.self_update.status = format!("Reinstalling Neko at {}…", core::short(&plan.to));
    })?;
    tell(state, plan.task_id.as_deref(), &text, false);
    Ok(())
}

/// What the installer reported, recorded once (here or in the next daemon).
fn consume_install_report(state: &Arc<AppState>, data: &Path) -> Result<(), String> {
    let Some(result) = core::take_result(data) else { return Ok(()) };
    let task_id = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?
        .self_update.installing.as_ref().and_then(|p| p.task_id.clone());
    let short = core::short(&result.commit).to_owned();
    let summary = match result.status.as_str() {
        "updated" => format!("Neko updated itself to {short} (app reinstalled)."),
        "rolled_back" => format!("Rolled back the update to {short}: {}. The previous app is running again.", result.reason),
        _ => format!("Couldn't reinstall Neko at {short}: {}. The current app keeps running.", result.reason),
    };
    with_snapshot(state, |s| {
        let u = &mut s.self_update;
        u.installing = None;
        u.status.clear();
        match result.status.as_str() {
            "updated" => {
                u.native_commit = Some(result.commit.clone());
                u.last_update = Some(record(&result.commit, &summary));
            }
            "rolled_back" => u.last_rollback = Some(record(&result.commit, &summary)),
            _ => u.last_failure = Some(record(&result.commit, &summary)),
        }
    })?;
    tell(state, task_id.as_deref(), &summary, false);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_core::Db;
    use neko_protocol::workbench::{Command, Task, TaskStatus, Workspace};
    use std::process::Command as Process;

    fn run(dir: &Path, args: &[&str]) -> String {
        let out = Process::new("git").current_dir(dir).args(args)
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn commit(dir: &Path, file: &str, text: &str) -> String {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
        run(dir, &["add", "-A"]);
        run(dir, &["commit", "-q", "-m", file]);
        run(dir, &["rev-parse", "HEAD"])
    }

    fn task(id: &str) -> Task {
        Task {
            id: id.into(), workspace_id: "w".into(), issue_id: None, title: format!("Task {id}"), goal: "g".into(),
            status: TaskStatus::Completed, plan: String::new(), result: String::new(), worktree: None, events: vec![],
            created_at_ms: 0, updated_at_ms: 0, source_revision: None, base_commit: None, pull_request: None,
            landed_commit: None, supervision: None, display_title: None, parked: None,
        }
    }

    fn snapshot(state: &AppState) -> Snapshot {
        store::load(&state.db.lock().unwrap()).unwrap()
    }

    fn land(state: &AppState, id: &str, commit: &str) {
        with_snapshot(state, |s| {
            let mut t = task(id);
            t.landed_commit = Some(commit.to_owned());
            t.updated_at_ms = store::now_ms();
            s.tasks.push(t);
        }).unwrap();
    }

    #[test]
    fn landed_changes_are_applied_held_for_a_click_and_reported() {
        let repo = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let dir = repo.path().canonicalize().unwrap();
        run(&dir, &["init", "-q", "-b", "main"]);
        commit(&dir, "crates/neko-daemon/Cargo.toml", "[package]");
        let base = commit(&dir, "web/package.json", "{}");
        let state = Arc::new(AppState::with_test_providers(
            Db::open_in_memory().unwrap(), vec![], Some(neko_core::files::FileProvider::empty()),
            neko_core::settings::SettingsProvider::with_panes(Vec::new()),
            neko_core::permissions::PermissionsProvider::disabled(),
        ));
        with_snapshot(&state, |s| {
            s.workspaces.push(Workspace { id: "w".into(), name: "neko".into(), repository: dir.to_string_lossy().into(), instructions: String::new(), away_enabled: false });
            s.land_on_main.insert(dir.to_string_lossy().into());
        }).unwrap();

        // First pass: what is there now is the starting point, not an update.
        tick_with(&state, data.path(), None).unwrap();
        let su = snapshot(&state).self_update;
        assert_eq!(su.applied_through.as_deref(), Some(base.as_str()));
        assert_eq!(su.checkout.as_deref(), Some(&*dir.to_string_lossy()));

        // An ordinary native change from a landed task is applied (there is no
        // installed app in a test, so nothing is reinstalled).
        let ordinary = commit(&dir, "crates/neko-core/src/triage.rs", "// x");
        land(&state, "a", &ordinary);
        tick_with(&state, data.path(), None).unwrap();
        let su = snapshot(&state).self_update;
        assert_eq!(su.applied_through.as_deref(), Some(ordinary.as_str()));
        assert!(su.queued.is_none() && su.pending.is_none());

        // A commit nobody landed through a task is not applied on its own.
        let by_hand = commit(&dir, "crates/neko-core/src/apps.rs", "// y");
        tick_with(&state, data.path(), None).unwrap();
        assert_eq!(snapshot(&state).self_update.applied_through.as_deref(), Some(ordinary.as_str()));

        // A safety-critical change waits for the owner, says so on the task and
        // in chat, and offers the one-click action there.
        let critical = commit(&dir, "crates/neko-daemon/src/web.rs", "// z");
        land(&state, "b", &critical);
        tick_with(&state, data.path(), None).unwrap();
        let s = snapshot(&state);
        let pending = s.self_update.pending.clone().expect("waits for a click");
        assert_eq!((pending.to.as_str(), pending.task_id.as_deref()), (critical.as_str(), Some("b")));
        assert_eq!(pending.sensitive, vec!["crates/neko-daemon/src/web.rs".to_owned()]);
        assert_eq!(s.self_update.applied_through.as_deref(), Some(ordinary.as_str()), "nothing applied");
        assert!(s.tasks.iter().find(|t| t.id == "b").unwrap().events.iter().any(|e| e.message.contains("waiting for you")));
        let notice = s.conversation.last().unwrap();
        assert!(notice.text.contains("safety-critical") && notice.actions[0].kind == "apply_update");
        // Ticking again neither re-plans nor repeats the notice.
        tick_with(&state, data.path(), None).unwrap();
        assert_eq!(snapshot(&state).conversation.len(), s.conversation.len());
        let _ = by_hand;

        // The chat action applies it, through the same command as the button.
        let action = neko_core::chief_of_staff::command(&notice.actions[0]).unwrap();
        assert!(matches!(action, Command::ApplyPendingUpdate));
        store::apply(&state.db.lock().unwrap(), action).unwrap();
        tick_with(&state, data.path(), None).unwrap();
        let su = snapshot(&state).self_update;
        assert_eq!(su.applied_through.as_deref(), Some(critical.as_str()));
        assert!(su.pending.is_none() && su.queued.is_none());

        // The installer's report is recorded once, on the task and in chat.
        with_snapshot(&state, |s| s.self_update.installing = Some(UpdatePlan { to: critical.clone(), task_id: Some("b".into()), at_ms: store::now_ms(), ..UpdatePlan::default() })).unwrap();
        std::fs::create_dir_all(data.path().join("self-update")).unwrap();
        std::fs::write(core::result_path(data.path()), format!(r#"{{"commit":"{critical}","status":"rolled_back","reason":"the new version did not become healthy"}}"#)).unwrap();
        tick_with(&state, data.path(), None).unwrap();
        let s = snapshot(&state);
        assert!(s.self_update.installing.is_none());
        assert!(s.self_update.last_rollback.as_ref().unwrap().summary.contains("did not become healthy"));
        assert!(s.conversation.last().unwrap().text.starts_with("Rolled back"));
        assert!(!core::result_path(data.path()).exists());
    }

    #[test]
    fn update_now_and_the_auto_switches() {
        let repo = tempfile::tempdir().unwrap();
        let data = tempfile::tempdir().unwrap();
        let dir = repo.path().canonicalize().unwrap();
        run(&dir, &["init", "-q", "-b", "main"]);
        commit(&dir, "crates/neko-daemon/Cargo.toml", "[package]");
        commit(&dir, "web/package.json", "{}");
        let state = Arc::new(AppState::with_test_providers(
            Db::open_in_memory().unwrap(), vec![], Some(neko_core::files::FileProvider::empty()),
            neko_core::settings::SettingsProvider::with_panes(Vec::new()),
            neko_core::permissions::PermissionsProvider::disabled(),
        ));
        with_snapshot(&state, |s| {
            s.workspaces.push(Workspace { id: "w".into(), name: "neko".into(), repository: dir.to_string_lossy().into(), instructions: String::new(), away_enabled: false });
            s.land_on_main.insert(dir.to_string_lossy().into());
        }).unwrap();
        tick_with(&state, data.path(), None).unwrap();
        // Auto-update off for the app: a landed native change waits.
        store::apply(&state.db.lock().unwrap(), Command::SetSelfUpdate { auto_ui: true, auto_native: false }).unwrap();
        let change = commit(&dir, "crates/neko-core/src/triage.rs", "// x");
        land(&state, "a", &change);
        tick_with(&state, data.path(), None).unwrap();
        let pending = snapshot(&state).self_update.pending.expect("off means it waits");
        assert!(pending.reason.contains("Auto-update is off for the app"));
        // Switching it back on releases it.
        store::apply(&state.db.lock().unwrap(), Command::SetSelfUpdate { auto_ui: true, auto_native: true }).unwrap();
        tick_with(&state, data.path(), None).unwrap();
        assert_eq!(snapshot(&state).self_update.applied_through.as_deref(), Some(change.as_str()));
        // Update now, with nothing new, says so.
        store::apply(&state.db.lock().unwrap(), Command::UpdateNekoNow).unwrap();
        tick_with(&state, data.path(), None).unwrap();
        let su = snapshot(&state).self_update;
        assert!(!su.requested);
        assert_eq!(su.status, "Neko is up to date.");
        // Update now applies a commit made by hand (docs only: nothing to do).
        let doc = commit(&dir, "AGENTS.md", "notes");
        store::apply(&state.db.lock().unwrap(), Command::UpdateNekoNow).unwrap();
        tick_with(&state, data.path(), None).unwrap();
        assert_eq!(snapshot(&state).self_update.applied_through.as_deref(), Some(doc.as_str()));
        assert!(store::apply(&state.db.lock().unwrap(), Command::ApplyPendingUpdate).is_err(), "nothing waiting");
    }
}
