//! The installed app keeping itself current from the published repository.
//! One thread, woken by the update commands and otherwise every few seconds:
//!
//! 1. reads what the detached installer reported (`result.json`);
//! 2. looks at the published repository at most every 10 minutes (a cheap
//!    `ls-remote` first), moves its own clean clone (`<data>/source`) to
//!    `origin/main`, and decides the commits since what is applied
//!    (`neko_core::self_update::decide`): nothing (docs only), queue, or wait
//!    for a click when auto-update is off;
//! 3. web part: builds `web/` of the clone into `<data>/web-live.next`, smoke
//!    checks it and swaps it in atomically; open pages are told
//!    (`Event::WebUpdated`);
//! 4. native part: once nothing is running or publishing and `machine-ok`
//!    says go, hands the reinstall to `scripts/self-update.sh` of the clone,
//!    started in its own process group so it outlives this daemon (which the
//!    install stops). The script installs, relaunches, health-checks, restores
//!    the newest backup on failure, and writes `result.json` for the next
//!    daemon.
//!
//! The development checkout is never touched here. Decisions are pure and
//! tested in `neko_core::self_update`; this file is the I/O around them. See
//! `docs/knowledge/neko-updates-itself.md`.

use neko_core::self_update::{self as core, Decision, IdleInputs, Source};
use neko_core::workbench as store;
use neko_protocol::workbench::{ChatAction, ChatActionStatus, Snapshot, UpdateCheck, UpdatePlan, UpdateRecord};
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

/// Says something in the chat (and on a task, when one is named); `apply`
/// adds the one-click "Apply update" action on that task.
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

/// What touches the outside world, so the whole loop can be driven in a test
/// against a local bare repository without building anything or asking the
/// Mac how busy it is.
struct Hooks {
    /// Builds the clone's `web/` into `<data>/web-live.next`.
    build_web: fn(&Path, &Path, bool) -> Result<(), String>,
    /// Starts the detached installer of the clone for a plan.
    launch: fn(&Path, &Path, &UpdatePlan) -> Result<(), String>,
    /// `machine-ok`'s WAIT reason, if it says wait.
    machine_wait: fn() -> Option<String>,
}

impl Hooks {
    fn real() -> Hooks {
        Hooks { build_web: core::build_web, launch: spawn_installer, machine_wait: core::machine_wait }
    }
}

fn tick(state: &Arc<AppState>, data: &Path) -> Result<(), String> {
    let source = {
        let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
        Source::configured(data, &snapshot.self_update.source)
    };
    tick_with(state, data, core::bundle_commit(), &source, &Hooks::real())
}

/// One pass. `bundle` is the running app's commit (`None` outside an
/// installed app, where there is nothing to reinstall); `source` is where the
/// published repository is cloned from and to.
fn tick_with(state: &Arc<AppState>, data: &Path, bundle: Option<String>, source: &Source, hooks: &Hooks) -> Result<(), String> {
    consume_install_report(state, data)?;
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let su = snapshot.self_update.clone();

    if su.roll_back_web {
        // Needs no clone: it only moves the `web-live` link back.
        let outcome = core::roll_back_web(data);
        let summary = match &outcome {
            Ok(()) => "Rolled the web UI back to the previous version.".to_owned(),
            Err(error) => format!("Couldn't roll the web UI back: {error}"),
        };
        let ok = outcome.is_ok();
        with_snapshot(state, |s| {
            s.self_update.roll_back_web = false;
            s.self_update.status.clear();
            if ok {
                let from = s.self_update.web_commit.take().unwrap_or_default();
                s.self_update.last_rollback = Some(record(&from, &summary));
            } else {
                s.self_update.last_failure = Some(record("", &summary));
            }
        })?;
        if ok {
            crate::server::broadcast_event(state, &Event::WebUpdated { commit: "previous".into() });
        }
        tell(state, None, &summary, false);
    }

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

    refresh(state, source, hooks)?;
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let su = snapshot.self_update.clone();
    if !source.dir.join(".git").exists() {
        return Ok(());
    }
    let clone = source.dir.as_path();
    let head = core::head(clone)?;
    let clone_text = clone.to_string_lossy().into_owned();

    // Where this app starts: what was applied, else what is running, else
    // (the app's commit is not in the published history) the published tip
    // when the app's tree matches it, else the tip applied once. History is
    // never replayed.
    let mut applied = core::starting_point(clone, &head, su.applied_through.as_deref(), bundle.as_deref(), None);
    if applied.is_none() && bundle.is_some() {
        let tree = bundle.as_deref().and_then(|commit| {
            std::iter::once(clone.to_path_buf())
                .chain(su.dev_checkout.iter().map(PathBuf::from))
                .chain(core::bundle_source_path())
                .find_map(|repository| core::tree_of(&repository, commit))
        });
        applied = core::starting_point(clone, &head, None, bundle.as_deref(), tree.as_deref());
    } else if applied.is_none() {
        applied = Some(head.clone());
    }

    // A build swapped in outside this thread (`scripts/ship-web.sh`, or its
    // `--rollback`) is adopted: its folder is labelled with its commit.
    let mut web_commit = su.web_commit.clone();
    let live_label = core::live_web_label(data);
    let adopted = match &live_label {
        Some(label) if web_commit.as_deref().map(core::short) != Some(label.as_str()) => {
            core::resolve_commit(clone, label).inspect(|commit| web_commit = Some(commit.clone()))
        }
        None if web_commit.is_some() => { web_commit = None; None }
        _ => None,
    };
    // A newer app (installed by hand) outranks an older live web UI.
    let stale_web = match (&web_commit, &bundle) {
        (Some(web), Some(bundle)) => web == bundle || !core::is_ancestor(clone, bundle, web),
        _ => false,
    };
    if stale_web {
        core::clear_web(data);
    }
    // The folder the app was built in, when the person had set it to land on main.
    let built_in = core::bundle_source_path().map(|p| p.to_string_lossy().into_owned());
    with_snapshot(state, |s| {
        let built_in = built_in.filter(|p| s.land_on_main.contains(p.as_str()));
        let u = &mut s.self_update;
        // Before 2026-11-13 the updater built from the development checkout
        // and recorded it here. It is now only a development checkout.
        if let Some(old) = u.checkout.as_deref().filter(|old| Path::new(old) != clone && Path::new(old).join(".git").exists()) {
            if u.dev_checkout.is_none() { u.dev_checkout = Some(old.to_owned()); }
        } else if u.checkout.is_none() && u.dev_checkout.is_none() {
            // The updater never recorded a checkout, but the app was built in a
            // folder the person had set to land on main: that is the one they develop in.
            if let Some(built_in) = built_in {
                u.dev_checkout = Some(built_in);
            }
        }
        u.checkout = Some(clone_text.clone());
        if applied.is_some() { u.applied_through = applied.clone(); }
        if bundle.is_some() { u.native_commit = bundle.clone(); }
        u.web_commit = if stale_web { None } else { web_commit.clone() };
        if let Some(commit) = adopted.as_ref().filter(|_| !stale_web) {
            let summary = format!("The web UI at {} went live (shipped by hand).", core::short(commit));
            u.last_update = Some(record(commit, &summary));
        }
        // Pending only because auto-update was off: go once it is on again.
        if u.queued.is_none() && u.pending.as_ref().is_some_and(|p| core::releasable(p, u)) {
            u.queued = u.pending.take();
        }
    })?;
    if let Some(commit) = adopted.filter(|_| !stale_web) {
        crate::server::broadcast_event(state, &Event::WebUpdated { commit });
    }

    plan(state, clone, &head, applied.as_deref(), bundle.as_deref())?;
    apply(state, clone, data, bundle.as_deref(), hooks)
}

/// Looks at the published repository when it is time (every 10 minutes, at
/// once for "Update now" or a missing clone): one `ls-remote`, and only when
/// `main` moved a fetch, `reset --hard` and `clean`. Does nothing while the
/// Mac says WAIT, unless the owner asked. Failures are recorded as the last
/// check's error; going offline is normal and never a failed update.
fn refresh(state: &Arc<AppState>, source: &Source, hooks: &Hooks) -> Result<(), String> {
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let su = &snapshot.self_update;
    let now = store::now_ms();
    let have_clone = source.dir.join(".git").exists();
    let due = su.requested || !have_clone || su.check.at_ms == 0 || su.check.at_ms > now || now - su.check.at_ms >= core::CHECK_EVERY_MS;
    if !due {
        return Ok(());
    }
    if !su.requested {
        if let Some(wait) = (hooks.machine_wait)().or_else(|| neko_core::machine::latest().and_then(|h| h.holding)) {
            eprintln!("neko self-update: not checking for updates while the Mac says wait ({wait})");
            return Ok(());
        }
    }
    let outcome = core::remote_head(source).and_then(|remote| {
        let local = if have_clone { core::head(&source.dir).ok() } else { None };
        if local.as_deref() != Some(remote.as_str()) {
            core::sync_clone(source)?;
        }
        Ok(remote)
    });
    let requested = su.requested;
    let previous = su.check.remote_head.clone();
    with_snapshot(state, |s| {
        let check = &mut s.self_update.check;
        *check = match &outcome {
            Ok(remote) => UpdateCheck { at_ms: now, remote_head: Some(remote.clone()), error: String::new() },
            Err(error) => UpdateCheck { at_ms: now, remote_head: previous.clone(), error: error.clone() },
        };
        if let (Err(error), true) = (&outcome, requested) {
            s.self_update.requested = false;
            s.self_update.status = format!("Couldn't reach the update source: {error}");
        }
    })?;
    Ok(())
}

/// Turns new published commits into a queued or pending update.
fn plan(state: &Arc<AppState>, clone: &Path, head: &str, applied: Option<&str>, bundle: Option<&str>) -> Result<(), String> {
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let su = &snapshot.self_update;
    if su.queued.is_some() {
        return Ok(());
    }
    let requested = su.requested;
    if !requested && applied == Some(head) {
        return Ok(());
    }
    if !requested && su.pending.as_ref().is_some_and(|p| p.to == head) {
        return Ok(());
    }
    // "Update now" also reinstalls an app that is behind, even when the web
    // part was already applied.
    let from = match bundle {
        Some(b) if requested && b != head && core::is_ancestor(clone, b, head) => Some(b.to_owned()),
        _ => applied.map(str::to_owned),
    };
    if from.as_deref() == Some(head) {
        if requested {
            with_snapshot(state, |s| { s.self_update.requested = false; s.self_update.status = "Neko is up to date.".into(); })?;
        }
        return Ok(());
    }
    let paths = core::paths_between(clone, from.as_deref(), head)?;
    let decision = core::decide(from.as_deref(), head, &paths, None, requested, su, store::now_ms());
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
            let text = format!("Neko has an update waiting for you ({}): {} Apply it from Settings → Neko updates.", core::short(&plan.to), plan.reason);
            with_snapshot(state, |s| {
                s.self_update.requested = false;
                s.self_update.status.clear();
                s.self_update.pending = Some(plan);
            })?;
            tell(state, None, &text, false);
        }
    }
    Ok(())
}

/// Carries out the queued update: the web part now, the native part when idle.
fn apply(state: &Arc<AppState>, clone: &Path, data: &Path, bundle: Option<&str>, hooks: &Hooks) -> Result<(), String> {
    let snapshot = store::load(&state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner))?;
    let Some(mut queued) = snapshot.self_update.queued.clone() else { return Ok(()) };
    let set_status = |text: String| with_snapshot(state, |s| s.self_update.status = text);

    // The clone moves to `origin/main` whenever it is checked; an update that
    // waited (the Mac was busy, a task was running) is retargeted to the
    // newest published commit instead of waiting for a commit that is gone.
    let mut head = core::head(clone)?;
    if core::dirty(clone, None) {
        // Nobody else writes in the clone, so this is leftover build output.
        head = core::reset_clone(clone)?;
    }
    if head != queued.to {
        let from = queued.from.clone().filter(|f| core::resolve_commit(clone, f).is_some());
        let paths = core::paths_between(clone, from.as_deref(), &head)?;
        let class = core::classify(&paths);
        queued.to = head.clone();
        queued.from = from;
        queued.web = class.web;
        queued.native = class.native;
        queued.sensitive = class.sensitive;
        with_snapshot(state, |s| s.self_update.queued = Some(queued.clone()))?;
    }

    if queued.web {
        if let Some(wait) = (hooks.machine_wait)() {
            return set_status(format!("Waiting for the Mac to settle before building the web UI ({wait})."));
        }
        set_status(format!("Building the web UI at {}…", core::short(&head)))?;
        let lockfile = core::paths_between(clone, queued.from.as_deref(), &head)
            .map(|p| p.iter().any(|p| p == "web/pnpm-lock.yaml" || p == "web/package.json"))
            .unwrap_or(true);
        let outcome = (hooks.build_web)(clone, data, lockfile).and_then(|()| core::swap_in(data, core::short(&head)));
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
                let summary = format!("Couldn't update the web UI to {}: {}. The current UI stays.", core::short(&head), error.trim_end_matches(['.', ' ']));
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
                let inputs = IdleInputs {
                    workers, publishing, chat_running,
                    machine_wait: (hooks.machine_wait)()
                        .or_else(|| neko_core::machine::latest().and_then(|h| h.holding)),
                    checkout_at_target: core::head(clone).is_ok_and(|h| h == queued.to),
                    checkout_dirty: core::dirty(clone, None),
                };
                if let Err(reason) = core::idle_gate(&inputs) {
                    return set_status(reason);
                }
                start_installer(state, clone, data, &queued, hooks)?;
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
/// so it survives this daemon being stopped by the install it runs. Cargo's
/// output goes to the user's cache folder, not into the data folder.
fn start_installer(state: &Arc<AppState>, checkout: &Path, data: &Path, plan: &UpdatePlan, hooks: &Hooks) -> Result<(), String> {
    (hooks.launch)(checkout, data, plan)?;
    let text = format!("Reinstalling Neko at {} now that it is idle. It restarts on its own and rolls back if the new version is unhealthy.", core::short(&plan.to));
    with_snapshot(state, |s| {
        s.self_update.installing = Some(UpdatePlan { at_ms: store::now_ms(), ..plan.clone() });
        s.self_update.status = format!("Reinstalling Neko at {}…", core::short(&plan.to));
    })?;
    tell(state, plan.task_id.as_deref(), &text, false);
    Ok(())
}

fn spawn_installer(checkout: &Path, data: &Path, plan: &UpdatePlan) -> Result<(), String> {
    use std::os::unix::process::CommandExt;
    let script = checkout.join("scripts/self-update.sh");
    if !script.is_file() {
        return Err("scripts/self-update.sh is missing from the update source".into());
    }
    let folder = data.join("self-update");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(core::result_path(data));
    let log = std::fs::File::create(folder.join("install.log")).map_err(|e| e.to_string())?;
    let port = crate::web::configured_port().unwrap_or(0).to_string();
    let target = core::build_cache_dir();
    std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
    let mut command = std::process::Command::new("/bin/bash");
    command.arg(&script)
        .args(["--checkout", &checkout.to_string_lossy(), "--data-dir", &data.to_string_lossy(), "--commit", &plan.to, "--web-port", &port])
        .current_dir(checkout)
        .env("CARGO_BUILD_JOBS", "2")
        .env("CARGO_TARGET_DIR", &target)
        .stdin(std::process::Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .process_group(0);
    command.spawn().map_err(|e| format!("cannot start the installer: {e}"))?;
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
    use neko_protocol::workbench::Command;
    use std::process::Command as Process;

    fn run(dir: &Path, args: &[&str]) -> String {
        let out = Process::new("git").current_dir(dir).args(args)
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    fn write(dir: &Path, file: &str, text: &str) {
        let path = dir.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    fn stub_build(_clone: &Path, data: &Path, _lockfile: bool) -> Result<(), String> {
        let next = core::web_paths(data).next;
        let _ = std::fs::remove_dir_all(&next);
        write(&next, "index.html", r#"<script type="module" src="/assets/a.js"></script>"#);
        write(&next, "assets/a.js", "1");
        let log = data.join("web-builds-run");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        std::fs::write(&log, format!("{before}x")).map_err(|e| e.to_string())
    }

    fn stub_launch(_clone: &Path, data: &Path, plan: &UpdatePlan) -> Result<(), String> {
        let log = data.join("installer-ran");
        let before = std::fs::read_to_string(&log).unwrap_or_default();
        std::fs::write(&log, format!("{before}{} sensitive={}\n", plan.to, plan.sensitive.len())).map_err(|e| e.to_string())
    }

    fn calm() -> Option<String> { None }

    /// `machine-ok` saying WAIT while a test holds the flag.
    static WAIT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    fn sometimes_busy() -> Option<String> {
        WAIT.load(std::sync::atomic::Ordering::SeqCst).then(|| "load 18".to_owned())
    }

    const HOOKS: Hooks = Hooks { build_web: stub_build, launch: stub_launch, machine_wait: calm };

    /// A local bare "public" repository, a working copy that publishes to it,
    /// a development checkout cloned from it, and a data folder.
    struct Rig {
        _keep: tempfile::TempDir,
        root: PathBuf,
        publisher: PathBuf,
        dev: PathBuf,
        data: PathBuf,
        source: Source,
        state: Arc<AppState>,
    }

    impl Rig {
        fn new() -> Rig {
            let keep = tempfile::tempdir().unwrap();
            let root = keep.path().canonicalize().unwrap();
            let bare = root.join("public.git");
            let publisher = root.join("publisher");
            let dev = root.join("dev");
            let data = root.join("data");
            std::fs::create_dir_all(&data).unwrap();
            run(&root, &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
            run(&root, &["init", "-q", "-b", "main", publisher.to_str().unwrap()]);
            run(&publisher, &["remote", "add", "origin", bare.to_str().unwrap()]);
            write(&publisher, "crates/neko-daemon/Cargo.toml", "[package]");
            write(&publisher, "web/package.json", "{}");
            write(&publisher, "README.md", "first");
            run(&publisher, &["add", "-A"]);
            run(&publisher, &["commit", "-q", "-m", "base"]);
            run(&publisher, &["push", "-q", "origin", "main"]);
            // The development checkout: its own clone, with work nobody may touch.
            run(&root, &["clone", "-q", bare.to_str().unwrap(), dev.to_str().unwrap()]);
            write(&dev, "unpushed.txt", "local commit");
            run(&dev, &["add", "-A"]);
            run(&dev, &["commit", "-q", "-m", "local only"]);
            write(&dev, "README.md", "uncommitted edit");
            write(&dev, "scratch/untracked.txt", "scratch");
            let state = Arc::new(AppState::with_test_providers(
                Db::open_in_memory().unwrap(), vec![], Some(neko_core::files::FileProvider::empty()),
                neko_core::settings::SettingsProvider::with_panes(Vec::new()),
                neko_core::permissions::PermissionsProvider::disabled(),
            ));
            let source = Source { remote: bare.to_string_lossy().into_owned(), dir: data.join("source") };
            Rig { _keep: keep, root, publisher, dev, data, source, state }
        }

        /// A commit published to the "public" repository.
        fn publish(&self, file: &str, text: &str) -> String {
            write(&self.publisher, file, text);
            run(&self.publisher, &["add", "-A"]);
            run(&self.publisher, &["commit", "-q", "-m", file]);
            run(&self.publisher, &["push", "-q", "origin", "main"]);
            run(&self.publisher, &["rev-parse", "HEAD"])
        }

        fn tick(&self, bundle: Option<&str>) {
            tick_with(&self.state, &self.data, bundle.map(str::to_owned), &self.source, &HOOKS).unwrap();
        }

        fn snapshot(&self) -> Snapshot {
            store::load(&self.state.db.lock().unwrap()).unwrap()
        }

        fn command(&self, command: Command) {
            store::apply(&self.state.db.lock().unwrap(), command).unwrap();
        }

        /// Pretends the last look was long ago, so the next tick looks again.
        fn age_check(&self) {
            with_snapshot(&self.state, |s| s.self_update.check.at_ms -= 11 * 60 * 1000).unwrap();
        }

        fn builds(&self) -> usize {
            std::fs::read_to_string(self.data.join("web-builds-run")).unwrap_or_default().len()
        }

        fn installs(&self) -> Vec<String> {
            std::fs::read_to_string(self.data.join("installer-ran")).unwrap_or_default().lines().map(str::to_owned).collect()
        }

        fn dev_state(&self) -> (String, String, String) {
            (run(&self.dev, &["rev-parse", "HEAD"]), run(&self.dev, &["status", "--porcelain", "--ignored"]), run(&self.dev, &["log", "--format=%H"]))
        }
    }

    #[test]
    fn the_use_copy_follows_the_published_repository_and_never_touches_the_dev_checkout() {
        let rig = Rig::new();
        let base = run(&rig.publisher, &["rev-parse", "HEAD"]);
        let dev_before = rig.dev_state();

        // First run: the clone is made, the running app (at the published tip)
        // is adopted, nothing is built or reinstalled.
        rig.tick(Some(&base));
        assert!(rig.source.dir.join(".git").exists(), "the updater owns a clone at <data>/source");
        let su = rig.snapshot().self_update;
        assert_eq!(su.checkout.as_deref(), Some(&*rig.source.dir.to_string_lossy()));
        assert_eq!(su.applied_through.as_deref(), Some(base.as_str()));
        assert_eq!(su.check.remote_head.as_deref(), Some(base.as_str()));
        assert!(su.check.at_ms > 0 && su.check.error.is_empty());
        assert_eq!((rig.builds(), rig.installs().len()), (0, 0));

        // A new published commit is not seen before the next check is due
        // (at most one look every 10 minutes)…
        let web = rig.publish("web/src/a.css", "a{}");
        rig.tick(Some(&base));
        assert_eq!(rig.snapshot().self_update.applied_through.as_deref(), Some(base.as_str()));
        // …and a web-only range is built and swapped in, with no reinstall.
        rig.age_check();
        rig.tick(Some(&base));
        let su = rig.snapshot().self_update;
        assert_eq!(su.applied_through.as_deref(), Some(web.as_str()));
        assert_eq!(su.web_commit.as_deref(), Some(web.as_str()));
        assert_eq!((rig.builds(), rig.installs().len()), (1, 0));
        assert!(core::live_web_root(&rig.data).is_some());
        assert!(su.last_update.unwrap().summary.contains("UI live"));

        // A docs-only range does nothing.
        let docs = rig.publish("docs/notes.md", "words");
        rig.age_check();
        rig.tick(Some(&base));
        let su = rig.snapshot().self_update;
        assert_eq!(su.applied_through.as_deref(), Some(docs.as_str()), "taken as applied");
        assert_eq!((rig.builds(), rig.installs().len()), (1, 0));
        assert!(su.queued.is_none() && su.pending.is_none());

        // A native range makes the install call (stubbed) and records it.
        let native = rig.publish("crates/neko-core/src/triage.rs", "// x");
        rig.age_check();
        rig.tick(Some(&base));
        assert_eq!(rig.installs(), vec![format!("{native} sensitive=0")]);
        let su = rig.snapshot().self_update;
        assert_eq!(su.installing.as_ref().map(|p| p.to.as_str()), Some(native.as_str()));
        assert_eq!(su.applied_through.as_deref(), Some(native.as_str()));
        // While it installs, nothing else happens.
        rig.tick(Some(&base));
        assert_eq!(rig.installs().len(), 1);
        // The installer's report is recorded once.
        std::fs::create_dir_all(rig.data.join("self-update")).unwrap();
        std::fs::write(core::result_path(&rig.data), format!(r#"{{"commit":"{native}","status":"updated","reason":""}}"#)).unwrap();
        rig.tick(Some(&native));
        let su = rig.snapshot().self_update;
        assert!(su.installing.is_none());
        assert_eq!(su.native_commit.as_deref(), Some(native.as_str()));

        // Safety-critical paths apply on their own too, and the plan names them.
        let critical = rig.publish("crates/neko-daemon/src/web.rs", "// z");
        rig.age_check();
        rig.tick(Some(&native));
        assert_eq!(rig.installs().last().unwrap(), &format!("{critical} sensitive=1"));
        std::fs::write(core::result_path(&rig.data), format!(r#"{{"commit":"{critical}","status":"updated","reason":""}}"#)).unwrap();
        rig.tick(Some(&critical));

        // Switching auto-update off makes it wait for a click; the click applies it.
        rig.command(Command::SetSelfUpdate { auto_ui: true, auto_native: false });
        let later = rig.publish("crates/neko-core/src/apps.rs", "// y");
        rig.age_check();
        rig.tick(Some(&critical));
        let su = rig.snapshot().self_update;
        let pending = su.pending.expect("auto-update off means it waits");
        assert!(pending.reason.contains("Auto-update is off for the app"));
        assert_eq!(rig.installs().len(), 2);
        rig.command(Command::ApplyPendingUpdate);
        rig.tick(Some(&critical));
        assert_eq!(rig.installs().last().unwrap(), &format!("{later} sensitive=0"));

        // The clone is exactly the published tip, with no commits of its own.
        assert_eq!(run(&rig.source.dir, &["rev-parse", "HEAD"]), later);
        assert_eq!(run(&rig.source.dir, &["status", "--porcelain"]), "");
        assert_eq!(run(&rig.source.dir, &["rev-list", "--count", "HEAD"]), run(&rig.publisher, &["rev-list", "--count", "HEAD"]));
        // The development checkout is exactly as it was.
        assert_eq!(rig.dev_state(), dev_before);
    }

    #[test]
    fn a_dirty_clone_is_reset_and_a_waiting_update_follows_the_newest_commit() {
        let rig = Rig::new();
        let base = run(&rig.publisher, &["rev-parse", "HEAD"]);
        rig.tick(Some(&base));
        let first = rig.publish("crates/neko-core/src/a.rs", "// a");
        rig.age_check();
        let busy = Hooks { machine_wait: sometimes_busy, ..HOOKS };
        let wait = |on: bool| WAIT.store(on, std::sync::atomic::Ordering::SeqCst);
        // The Mac says wait: nothing is even checked.
        wait(true);
        tick_with(&rig.state, &rig.data, Some(base.clone()), &rig.source, &busy).unwrap();
        assert_eq!(rig.snapshot().self_update.applied_through.as_deref(), Some(base.as_str()));
        assert_eq!(rig.snapshot().self_update.check.remote_head.as_deref(), Some(base.as_str()));
        // The owner asked, so it is checked and queued, then waits for the Mac.
        rig.command(Command::UpdateNekoNow);
        tick_with(&rig.state, &rig.data, Some(base.clone()), &rig.source, &busy).unwrap();
        let su = rig.snapshot().self_update;
        assert_eq!(su.queued.as_ref().map(|p| p.to.as_str()), Some(first.as_str()));
        assert!(su.status.contains("load 18"));
        // Another commit is published and build output litters the clone meanwhile.
        write(&rig.source.dir, "crates/neko-daemon/Cargo.toml", "[package] # edited");
        write(&rig.source.dir, "stray/output.bin", "junk");
        let second = rig.publish("crates/neko-core/src/b.rs", "// b");
        rig.command(Command::UpdateNekoNow);
        tick_with(&rig.state, &rig.data, Some(base.clone()), &rig.source, &busy).unwrap();
        assert_eq!(rig.snapshot().self_update.queued.as_ref().map(|p| p.to.as_str()), Some(second.as_str()), "retargeted to the newest");
        // When the Mac settles it is installed once, at the newest commit.
        wait(false);
        tick_with(&rig.state, &rig.data, Some(base.clone()), &rig.source, &busy).unwrap();
        assert_eq!(rig.installs(), vec![format!("{second} sensitive=0")]);
        assert!(!rig.source.dir.join("stray").exists());
        assert_eq!(run(&rig.source.dir, &["status", "--porcelain"]), "");
    }

    #[test]
    fn an_app_built_from_unpublished_history_is_adopted_when_its_files_match() {
        let rig = Rig::new();
        // A commit with the published tip's exact files but another history,
        // living only in the development checkout.
        let tree = run(&rig.publisher, &["rev-parse", "HEAD^{tree}"]);
        let rewritten = run(&rig.dev, &["commit-tree", &tree, "-m", "same files, rewritten history"]);
        with_snapshot(&rig.state, |s| s.self_update.dev_checkout = Some(rig.dev.to_string_lossy().into_owned())).unwrap();
        rig.tick(Some(&rewritten));
        let published = run(&rig.publisher, &["rev-parse", "HEAD"]);
        let su = rig.snapshot().self_update;
        assert_eq!(su.applied_through.as_deref(), Some(published.as_str()), "adopted as applied");
        assert_eq!((rig.builds(), rig.installs().len()), (0, 0), "no needless reinstall");
        assert!(su.queued.is_none() && su.pending.is_none());
    }

    #[test]
    fn an_app_that_does_not_match_the_published_history_is_applied_once() {
        let rig = Rig::new();
        let published = run(&rig.publisher, &["rev-parse", "HEAD"]);
        write(&rig.dev, "different.txt", "an app built from something else");
        run(&rig.dev, &["add", "-A"]);
        run(&rig.dev, &["commit", "-q", "-m", "different"]);
        let foreign = run(&rig.dev, &["rev-parse", "HEAD"]);
        with_snapshot(&rig.state, |s| s.self_update.dev_checkout = Some(rig.dev.to_string_lossy().into_owned())).unwrap();
        rig.tick(Some(&foreign));
        // The whole tree is classified: the web UI is built and the app reinstalled, once.
        assert_eq!(rig.builds(), 1);
        assert_eq!(rig.installs(), vec![format!("{published} sensitive=0")]);
        // Not again on the next look.
        std::fs::remove_file(core::result_path(&rig.data)).ok();
        with_snapshot(&rig.state, |s| s.self_update.installing = None).unwrap();
        rig.age_check();
        rig.tick(Some(&foreign));
        assert_eq!((rig.builds(), rig.installs().len()), (1, 1));
    }

    #[test]
    fn the_old_checkout_becomes_the_development_checkout_once() {
        let rig = Rig::new();
        let base = run(&rig.publisher, &["rev-parse", "HEAD"]);
        // What the previous version recorded: the updater's checkout was the dev one.
        with_snapshot(&rig.state, |s| s.self_update.checkout = Some(rig.dev.to_string_lossy().into_owned())).unwrap();
        rig.tick(Some(&base));
        let su = rig.snapshot().self_update;
        assert_eq!(su.dev_checkout.as_deref(), Some(&*rig.dev.to_string_lossy()));
        assert_eq!(su.checkout.as_deref(), Some(&*rig.source.dir.to_string_lossy()));
        // Cleared by the owner, it stays cleared.
        rig.command(Command::SetDevCheckout { path: String::new() });
        rig.tick(Some(&base));
        assert_eq!(rig.snapshot().self_update.dev_checkout, None);
        let _ = &rig.root;
    }

    #[test]
    fn an_unreachable_source_is_recorded_as_the_last_checks_error_not_a_failed_update() {
        let rig = Rig::new();
        let missing = Source { remote: rig.root.join("nowhere.git").to_string_lossy().into_owned(), dir: rig.data.join("source") };
        tick_with(&rig.state, &rig.data, None, &missing, &HOOKS).unwrap();
        let su = rig.snapshot().self_update;
        assert!(!su.check.error.is_empty() && su.check.at_ms > 0);
        assert!(su.last_failure.is_none() && su.queued.is_none());
        assert!(!rig.data.join("source").exists());
    }

    #[test]
    fn the_source_and_the_development_checkout_are_settings() {
        let rig = Rig::new();
        rig.command(Command::SetUpdateSource { repository: "https://github.com/acme/neko-fork.git".into() });
        assert_eq!(rig.snapshot().self_update.source, "acme/neko-fork");
        let configured = Source::configured(&rig.data, &rig.snapshot().self_update.source);
        assert_eq!(configured.remote, "https://github.com/acme/neko-fork.git");
        assert_eq!(configured.dir, rig.data.join("source"));
        assert!(store::apply(&rig.state.db.lock().unwrap(), Command::SetUpdateSource { repository: "not a repo".into() }).is_err());
        rig.command(Command::SetUpdateSource { repository: String::new() });
        assert_eq!(Source::configured(&rig.data, "").remote, format!("https://github.com/{}.git", core::DEFAULT_REPOSITORY));
        rig.command(Command::SetDevCheckout { path: rig.dev.to_string_lossy().into_owned() });
        assert_eq!(rig.snapshot().self_update.dev_checkout.as_deref(), Some(&*rig.dev.to_string_lossy()));
        assert!(store::apply(&rig.state.db.lock().unwrap(), Command::SetDevCheckout { path: rig.data.to_string_lossy().into_owned() }).is_err(), "not a repository");
    }

    #[test]
    fn a_web_build_shipped_by_hand_is_adopted_and_outranked_by_a_newer_app() {
        let rig = Rig::new();
        let app = run(&rig.publisher, &["rev-parse", "HEAD"]);
        let shipped = rig.publish("web/src/a.css", "a{}");
        // What `scripts/ship-web.sh` leaves: a build labelled with its commit.
        let build = rig.data.join("web-builds").join(core::short(&shipped));
        std::fs::create_dir_all(&build).unwrap();
        std::fs::write(build.join("index.html"), "x").unwrap();
        std::os::unix::fs::symlink(Path::new("web-builds").join(core::short(&shipped)), rig.data.join("web-live")).unwrap();
        // Applied already, so the thread itself builds nothing.
        with_snapshot(&rig.state, |s| s.self_update.applied_through = Some(shipped.clone())).unwrap();

        rig.tick(Some(&app));
        let su = rig.snapshot().self_update;
        assert_eq!(su.web_commit.as_deref(), Some(shipped.as_str()));
        assert!(su.last_update.unwrap().summary.contains("shipped by hand"));
        assert!(core::live_web_root(&rig.data).is_some(), "the older app does not outrank it");

        // The app is then installed at the shipped commit: its own UI is served again.
        rig.tick(Some(&shipped));
        assert_eq!(rig.snapshot().self_update.web_commit, None);
        assert!(core::live_web_root(&rig.data).is_none());
    }

    #[test]
    fn rolling_back_the_web_ui_needs_no_clone_and_is_recorded() {
        let data = tempfile::tempdir().unwrap();
        let state = Arc::new(AppState::with_test_providers(
            Db::open_in_memory().unwrap(), vec![], Some(neko_core::files::FileProvider::empty()),
            neko_core::settings::SettingsProvider::with_panes(Vec::new()),
            neko_core::permissions::PermissionsProvider::disabled(),
        ));
        // Two builds, the second live and the first kept as the previous one.
        let builds = data.path().join("web-builds");
        for name in ["one", "two"] {
            std::fs::create_dir_all(builds.join(name)).unwrap();
            std::fs::write(builds.join(name).join("index.html"), name).unwrap();
        }
        std::os::unix::fs::symlink("web-builds/two", data.path().join("web-live")).unwrap();
        std::os::unix::fs::symlink("web-builds/one", data.path().join("web-live.prev")).unwrap();
        with_snapshot(&state, |s| s.self_update.web_commit = Some("abc123".into())).unwrap();
        store::apply(&state.db.lock().unwrap(), Command::RollBackWebUi).unwrap();
        let nowhere = Source { remote: data.path().join("none.git").to_string_lossy().into_owned(), dir: data.path().join("source") };
        tick_with(&state, data.path(), None, &nowhere, &HOOKS).unwrap();
        let su = store::load(&state.db.lock().unwrap()).unwrap().self_update;
        assert!(!su.roll_back_web && su.status.is_empty());
        assert_eq!(su.web_commit, None);
        let record = su.last_rollback.expect("recorded");
        assert_eq!(record.commit, "abc123");
        assert!(record.summary.contains("previous version"));
        assert_eq!(std::fs::read_link(data.path().join("web-live")).unwrap().to_string_lossy(), "web-builds/one");
        assert!(!data.path().join("web-live.prev").exists(), "nothing left to roll back to");
    }
}
