//! Running tasks (docs/knowledge/runtimes.md, transcript.md, permissions.md, run-guard.md): one run at a time per task, each on its
//! own thread, its events written through the run's book and pushed to screens; Stop, answers and follow-ups reach it from any
//! connection. With no run active the daemon sets no timer at all.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use neko_core::conductor::{self, AnswerError, Out, RunBook, Who};
use neko_core::runtime::{Answer, Control, Detection, Failure, HookLink, Outcome, Runtime, StartSpec};
use neko_core::workplace::{self, PlaceError};
use neko_core::{Core, Place};
use neko_protocol::{Change, Decision, ErrorCode, PermissionId, PermissionMode, Pricing, Reply, RuntimeInfo, ServerMessage, Task, TaskId, TaskStatus, Tone};

use crate::server::{Shared, lock};

/// The longest one run may take, and the longest it may say nothing (not counting time waiting for the person).
pub struct Limits {
    pub wall: Duration,
    pub stall: Duration,
}

impl Limits {
    /// 30 minutes and 5 minutes; tests shorten them with `NEKO_RUN_LIMIT_MS` and `NEKO_RUN_STALL_MS`.
    pub fn from_env() -> Limits {
        let ms = |k: &str, d: u64| Duration::from_millis(std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d));
        Limits { wall: ms("NEKO_RUN_LIMIT_MS", 30 * 60_000), stall: ms("NEKO_RUN_STALL_MS", 5 * 60_000) }
    }
}

/// A task's run while it lives.
pub struct Active {
    pub control: Option<Arc<dyn Control>>,
    pub book: Option<Arc<Mutex<RunBook>>>,
    /// Stop was asked before the agent was up.
    pub cancelled: Arc<AtomicBool>,
    /// The secret this run's hook helper carries ("Ask before risky steps" only).
    pub secret: Option<String>,
}

/// A run, as its hook helper reaches it.
pub struct HookRun {
    pub task: TaskId,
    pub book: Arc<Mutex<RunBook>>,
    pub control: Option<Arc<dyn Control>>,
}

/// The running run whose hook carries `secret`.
pub fn by_secret(shared: &Shared, secret: &str) -> Option<HookRun> {
    lock(&shared.runs.active).iter().find_map(|(task, a)| match (&a.secret, &a.book) {
        (Some(s), Some(b)) if crate::files::same_token(s, secret) => Some(HookRun { task: task.clone(), book: b.clone(), control: a.control.clone() }),
        _ => None,
    })
}

static HOOK_CALLS: AtomicU64 = AtomicU64::new(1);

/// A number for a hook call that came without a tool-use id.
pub fn next_hook() -> u64 {
    HOOK_CALLS.fetch_add(1, Ordering::Relaxed)
}

/// The runtimes, what was found of them, and the active runs.
pub struct Runs {
    pub runtimes: Vec<Arc<dyn Runtime>>,
    pub active: Mutex<HashMap<TaskId, Active>>,
    found: Mutex<Option<(Instant, HashMap<String, Detection>)>>,
    pub data_dir: PathBuf,
    pub limits: Limits,
    /// Hook helpers waiting for an answer, by their request id.
    pub hooks: Mutex<HashMap<String, Sender<(Answer, String)>>>,
    /// Run threads still alive (a run's last writes come after it is released): shutdown waits for them before the checkpoint.
    pub threads: Arc<AtomicU64>,
}

impl Runs {
    pub fn new(runtimes: Vec<Arc<dyn Runtime>>, data_dir: PathBuf) -> Runs {
        Runs { runtimes, active: Mutex::new(HashMap::new()), found: Mutex::new(None), data_dir, limits: Limits::from_env(), hooks: Mutex::new(HashMap::new()), threads: Arc::new(AtomicU64::new(0)) }
    }

    /// What was found of each runtime, read again when older than `fresh`.
    pub fn detections(&self, fresh: Duration) -> HashMap<String, Detection> {
        if let Some((at, d)) = lock(&self.found).as_ref()
            && at.elapsed() < fresh
        {
            return d.clone();
        }
        let d: HashMap<String, Detection> = self.runtimes.iter().map(|r| (r.id().to_owned(), r.detect())).collect();
        *lock(&self.found) = Some((Instant::now(), d.clone()));
        d
    }

    /// Settings' list: each runtime Neko runs, then the ones it knows of but cannot run yet. `folder` is the folder whose own
    /// Claude Code settings count too (the newest task's).
    pub fn infos(&self, fresh: Duration, folder: Option<&std::path::Path>) -> Vec<RuntimeInfo> {
        let found = self.detections(fresh);
        let mut list: Vec<RuntimeInfo> = self
            .runtimes
            .iter()
            .map(|r| {
                let d = found.get(r.id()).cloned().unwrap_or_default();
                RuntimeInfo {
                    id: r.id().to_owned(),
                    name: r.name().to_owned(),
                    found: d.found,
                    version: d.version,
                    signed_in: d.signed_in,
                    login: d.login.map(|l| l.words(vendor(r.id()))),
                    sign_in: r.sign_in_line().to_owned(),
                    install: r.install_line().to_owned(),
                    supported: true,
                    // What its own settings let it do without asking: read, never written.
                    rules: (r.id() == "claude" && d.found).then(|| std::env::var_os("HOME").map(|h| neko_core::claude::rules::summary(std::path::Path::new(&h), folder))).flatten(),
                }
            })
            .collect();
        // Codex: listed so the person knows where it stands; this Neko cannot run it yet.
        let codex = neko_core::find::find("codex", None);
        let version = codex.as_ref().and_then(|p| {
            let mut c = std::process::Command::new(p);
            c.arg("--version").stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null());
            neko_core::find::run_for(c, Duration::from_secs(5)).and_then(|o| String::from_utf8_lossy(&o.stdout).lines().next().map(|l| l.trim().trim_start_matches("codex-cli").trim().to_owned()))
        });
        list.push(RuntimeInfo {
            id: "codex".into(),
            name: "Codex".into(),
            found: codex.is_some(),
            version,
            signed_in: None,
            login: None,
            sign_in: "codex login".into(),
            install: "npm install -g @openai/codex".into(),
            supported: false,
            rules: None,
        });
        list
    }
}

fn vendor(id: &str) -> &'static str {
    match id {
        "claude" => "Claude",
        _ => "Agent",
    }
}

/// Pushes a run book's changes, answers the runtime's requests and the waiting hooks. Called with the core locked, so events
/// leave in order.
pub fn push(shared: &Shared, task: &TaskId, outs: Vec<Out>, control: Option<&Arc<dyn Control>>) {
    for o in outs {
        match o {
            Out::Change(revision, change) => shared.broadcast(&ServerMessage::Event { revision, change }),
            Out::Delta { part, text } => shared.stream(task, &ServerMessage::Event { revision: 0, change: Change::PartDelta { task: task.clone(), part, text } }),
            Out::Answer { request, answer } => {
                if let Some(c) = control {
                    let _ = c.answer(&request, answer);
                }
            }
            Out::Activity { thinking } => shared.stream(task, &ServerMessage::Event { revision: 0, change: Change::Activity { task: task.clone(), thinking } }),
            Out::HookAnswer { request, answer, why } => {
                if let Some(tx) = lock(&shared.runs.hooks).remove(&request) {
                    let _ = tx.send((answer, why));
                }
            }
        }
    }
}

/// A notice outside a run, and the task's new status.
fn fail_before_run(shared: &Shared, task: &Task, text: String, next: Option<String>) {
    let mut core = lock(&shared.core);
    if let Ok(outs) = conductor::standalone_notice(&mut core, task, Tone::Error, text, next) {
        push(shared, &task.id, outs, None);
    }
    let now = core.now();
    if let Ok((t, rev)) = core.store.set_task_status(&task.id, TaskStatus::Failed, now) {
        shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t } });
    }
}

/// Starts the task's agent on `words` (None: the task's own words, for its first run). Refused when one is already running.
pub fn start(shared: &Arc<Shared>, task: Task, words: Option<String>) -> Result<(), Reply> {
    let cancelled = Arc::new(AtomicBool::new(false));
    {
        let mut active = lock(&shared.runs.active);
        if active.contains_key(&task.id) {
            return Err(Reply::error(ErrorCode::Busy, "Neko is already working on this task."));
        }
        active.insert(task.id.clone(), Active { control: None, book: None, cancelled: cancelled.clone(), secret: None });
    }
    let s = shared.clone();
    let id = task.id.clone();
    let threads = shared.runs.threads.clone();
    threads.fetch_add(1, Ordering::SeqCst);
    let spawned = std::thread::Builder::new().name(format!("neko-run-{}", workplace::short_id(&task.id))).spawn(move || {
        run(&s, &task, words, &cancelled);
        release(&s, &task.id, &cancelled);
        s.runs.threads.fetch_sub(1, Ordering::SeqCst);
    });
    if spawned.is_err() {
        threads.fetch_sub(1, Ordering::SeqCst);
        lock(&shared.runs.active).remove(&id);
        return Err(Reply::error(ErrorCode::Internal, "Neko could not start a thread for this run."));
    }
    Ok(())
}

/// Forgets this run (and only this one: a new run of the task may already be starting), so the task can take a follow-up. Done
/// before the run's last changes are pushed, so a screen that sees it end can send at once.
fn release(shared: &Shared, task: &TaskId, mine: &Arc<AtomicBool>) {
    let mut active = lock(&shared.runs.active);
    if active.get(task).is_some_and(|a| Arc::ptr_eq(&a.cancelled, mine)) {
        active.remove(task);
    }
}

fn run(shared: &Arc<Shared>, task: &Task, words: Option<String>, cancelled: &Arc<AtomicBool>) {
    // Where it works: prepared once, then the same for every follow-up.
    let stored = lock(&shared.core).store.place(&task.id);
    let place = match stored {
        // Its folder may have been tidied away after the last run: it comes back first.
        Ok(Some(p)) => workplace::reopen(&shared.runs.data_dir, &task.id, &p).map(|()| p),
        _ => workplace::prepare(&shared.runs.data_dir, &task.id, task.folder.as_deref()),
    };
    let place: Place = match place {
        Ok(p) => p,
        Err(PlaceError::Gone(f)) => {
            if let Some(w) = &words {
                person(shared, &task.id, w);
            }
            release(shared, &task.id, cancelled);
            return fail_before_run(shared, task, format!("The folder {f} is gone."), Some("Put it back, or start a new task with another folder.".into()));
        }
        Err(PlaceError::Other(why)) => {
            release(shared, &task.id, cancelled);
            return fail_before_run(shared, task, why, Some("Try again, or start a new task without a folder.".into()));
        }
    };
    {
        let mut core = lock(&shared.core);
        let now = core.now();
        let _ = core.store.set_place(&task.id, &place, now);
    }
    let settings = lock(&shared.core).store.settings().unwrap_or(neko_protocol::Settings { runtime: "claude".into(), permission_mode: Default::default() });
    let Some(runtime) = shared.runs.runtimes.iter().find(|r| r.id() == settings.runtime).cloned() else {
        if let Some(w) = &words {
            person(shared, &task.id, w);
        }
        release(shared, &task.id, cancelled);
        return fail_before_run(shared, task, format!("Neko cannot run {} yet.", settings.runtime), Some("Choose Claude Code in Settings (⌘,), then send a follow-up.".into()));
    };
    let detected = shared.runs.detections(Duration::from_secs(60)).remove(runtime.id()).unwrap_or_default();
    let who = Who {
        id: runtime.id().to_owned(),
        name: runtime.name().to_owned(),
        sign_in: runtime.sign_in_line().to_owned(),
        install: runtime.install_line().to_owned(),
        pricing: detected.login.as_ref().map_or(Pricing::Unknown, |l| l.pricing()),
    };
    let early = if !detected.found {
        Some(Failure::NotInstalled)
    } else if detected.signed_in == Some(false) {
        Some(Failure::NotSignedIn)
    } else {
        None
    };
    let resume = lock(&shared.core).store.runs_of(&task.id).ok().and_then(|runs| runs.iter().rev().find_map(|r| r.session.clone()));
    let begun = {
        let mut core = lock(&shared.core);
        let r = RunBook::begin(&mut core, &task.id, who.clone(), place.clone(), settings.permission_mode, resume.clone(), words.as_deref());
        r.map(|(book, outs)| {
            push(shared, &task.id, outs, None);
            book
        })
    };
    let Ok(book) = begun else {
        release(shared, &task.id, cancelled);
        return fail_before_run(shared, task, "Neko could not write to its store.".into(), None);
    };
    let book = Arc::new(Mutex::new(book));
    let finish = |outcome: Outcome| {
        let changes = workplace::changes(&place);
        // A worktree or scratch folder with nothing in it goes now, before a follow-up can start (it makes it again).
        workplace::tidy(&shared.runs.data_dir, &task.id, &place);
        release(shared, &task.id, cancelled);
        let mut b = lock(&book);
        let mut core = lock(&shared.core);
        let now = core.now();
        if let Ok(outs) = b.finish(&mut core, outcome, changes, now) {
            push(shared, &task.id, outs, None);
        }
    };
    if let Some(f) = early {
        return finish(Outcome::Failed(f));
    }
    if cancelled.load(Ordering::SeqCst) {
        return finish(Outcome::Stopped);
    }
    // Asking before risky steps, the run's hook helper reaches the daemon with a secret of its own.
    // The helper is this binary (`NEKO_HOOK_HELPER` names another, for tests); the runtime refuses to start without it.
    let helper = std::env::var_os("NEKO_HOOK_HELPER").filter(|h| !h.is_empty()).map(PathBuf::from).or_else(|| std::env::current_exe().ok()).unwrap_or_default();
    let hook = (settings.permission_mode == PermissionMode::Ask).then(|| crate::files::random_hex(32).ok()).flatten().map(|secret| HookLink { port: shared.port, secret, helper });
    if settings.permission_mode == PermissionMode::Ask && hook.is_none() {
        return finish(Outcome::Failed(Failure::Crashed { code: None, words: "Neko could not make a secret for this run's hook.".into() }));
    }
    if let Some(a) = lock(&shared.runs.active).get_mut(&task.id) {
        a.secret = hook.as_ref().map(|h| h.secret.clone());
        a.book = Some(book.clone());
    }
    let spec = StartSpec { words: words.clone().unwrap_or_else(|| task.words.clone()), workdir: PathBuf::from(&place.workdir), resume, mode: settings.permission_mode, hook };
    let started = match runtime.start(spec) {
        Ok(s) => s,
        Err(f) => return finish(Outcome::Failed(f)),
    };
    let control = started.control.clone();
    if let Some(a) = lock(&shared.runs.active).get_mut(&task.id) {
        a.control = Some(control.clone());
        a.book = Some(book.clone());
    }
    if cancelled.load(Ordering::SeqCst) {
        control.stop();
    }
    let began = Instant::now();
    let mut heard = Instant::now();
    let mut limit: Option<Failure> = None;
    loop {
        let ev = match started.events.recv_timeout(Duration::from_secs(1)) {
            Ok(ev) => {
                heard = Instant::now();
                Some(ev)
            }
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                finish(limit.take().map_or(Outcome::Failed(Failure::Crashed { code: None, words: String::new() }), Outcome::Failed));
                return;
            }
        };
        let mut b = lock(&book);
        // Waiting for the person is not a stall.
        if b.waiting() {
            heard = Instant::now();
        }
        if limit.is_none() {
            if began.elapsed() >= shared.runs.limits.wall {
                limit = Some(Failure::TimedOut);
                control.stop();
            } else if heard.elapsed() >= shared.runs.limits.stall {
                limit = Some(Failure::Stalled);
                control.stop();
            }
        }
        match ev {
            Some(neko_core::runtime::AgentEvent::Finished(o)) => {
                drop(b);
                // A run the limits ended failed with that reason, not "stopped".
                finish(match (limit.take(), o) {
                    (Some(f), Outcome::Stopped) => Outcome::Failed(f),
                    (_, o) => o,
                });
                return;
            }
            Some(neko_core::runtime::AgentEvent::Unchecked { tool_use }) => {
                // Claude Code ran a step our hook could not check. Asking before risky steps, that run stops now.
                if settings.permission_mode == PermissionMode::Ask && limit.is_none() {
                    limit = Some(Failure::Unchecked { step: tool_use.as_deref().and_then(|t| b.step_words(t)) });
                    control.stop();
                }
            }
            Some(ev) => {
                let mut core = lock(&shared.core);
                let now = core.now();
                if let Ok(outs) = b.on(&mut core, ev, now) {
                    push(shared, &task.id, outs, Some(&control));
                }
            }
            None => {
                let mut core = lock(&shared.core);
                let now = core.now();
                if let Ok(outs) = b.tick(&mut core, now) {
                    push(shared, &task.id, outs, Some(&control));
                }
            }
        }
    }
}

fn person(shared: &Shared, task: &TaskId, words: &str) {
    let mut core = lock(&shared.core);
    if let Ok(outs) = conductor::person_part(&mut core, task, words) {
        push(shared, task, outs, None);
    }
}

/// Words in a task's composer: steer the running agent, or start a new run that resumes the conversation.
pub fn follow_up(shared: &Arc<Shared>, task: Task, words: String) -> Reply {
    if words.trim().is_empty() {
        return Reply::error(ErrorCode::InvalidWords, "Type something first.");
    }
    let control = lock(&shared.runs.active).get(&task.id).map(|a| a.control.clone());
    match control {
        Some(Some(c)) => match c.steer(&words) {
            Ok(()) => {
                person(shared, &task.id, &words);
                Reply::Ok
            }
            Err(e) => Reply::error(ErrorCode::Busy, e.0),
        },
        Some(None) => Reply::error(ErrorCode::Busy, "Neko is still starting this task's agent."),
        None => match start(shared, task, Some(words)) {
            Ok(()) => Reply::Ok,
            Err(r) => r,
        },
    }
}

/// Stops the task's agent. Fine when none is running.
pub fn stop(shared: &Shared, task: &TaskId) -> Reply {
    if let Some(a) = lock(&shared.runs.active).get(task) {
        a.cancelled.store(true, Ordering::SeqCst);
        if let Some(c) = &a.control {
            c.stop();
        }
    }
    Reply::Ok
}

/// The person's answer to a request.
pub fn answer(shared: &Shared, perm: &PermissionId, d: Decision) -> Reply {
    let Ok(Some(p)) = lock(&shared.core).store.get_permission(perm) else { return Reply::error(ErrorCode::NotFound, "There is no such request.") };
    let (book, control) = match lock(&shared.runs.active).get(&p.task) {
        Some(Active { book: Some(b), control, .. }) => (b.clone(), control.clone()),
        _ => return Reply::error(ErrorCode::AlreadyAnswered, "This request has expired: its run has ended."),
    };
    let mut b = lock(&book);
    let mut core = lock(&shared.core);
    let now = core.now();
    match b.answer(&mut core, perm, d, now) {
        Ok(outs) => {
            push(shared, &p.task, outs, control.as_ref());
            Reply::Ok
        }
        Err(AnswerError::NotFound) => Reply::error(ErrorCode::NotFound, "There is no such request."),
        Err(AnswerError::Settled) => Reply::error(ErrorCode::AlreadyAnswered, "This request was already answered."),
        Err(AnswerError::Store(e)) => Reply::error(ErrorCode::Internal, e.to_string()),
    }
}

/// Removes a task: refused while its agent works, and when its folder holds unsaved work (nothing is deleted then).
pub fn remove_task(shared: &Shared, task: &TaskId) -> Reply {
    if lock(&shared.runs.active).contains_key(task) {
        return Reply::error(ErrorCode::Busy, "Neko is working on this task. Stop it first.");
    }
    match lock(&shared.core).store.get_task(task) {
        Ok(Some(_)) => {}
        Ok(None) => return Reply::error(ErrorCode::NotFound, "There is no such task."),
        Err(e) => return Reply::error(ErrorCode::Internal, e.to_string()),
    }
    let place = lock(&shared.core).store.place(task).ok().flatten();
    if let Some(p) = &place
        && let Err(why) = workplace::remove_place(&shared.runs.data_dir, task, p)
    {
        return Reply::error(ErrorCode::UnsavedWork, why);
    }
    let mut core = lock(&shared.core);
    // A run may have started meanwhile (a follow-up from another screen): then nothing is removed.
    if lock(&shared.runs.active).contains_key(task) {
        return Reply::error(ErrorCode::Busy, "Neko is working on this task. Stop it first.");
    }
    let now = core.now();
    match core.store.delete_task(task, now) {
        Ok(revision) => {
            shared.broadcast(&ServerMessage::Event { revision, change: Change::TaskRemoved { task: task.clone() } });
            Reply::Ok
        }
        Err(e) => Reply::error(ErrorCode::Internal, e.to_string()),
    }
}

/// At start: runs the last daemon left running are interrupted, with a notice in each task.
pub fn recover(core: &mut Core) {
    let Ok(tasks) = core.store.interrupt_leftovers(core.now()) else { return };
    for t in tasks {
        if let Ok(Some(task)) = core.store.get_task(&t) {
            let _ = conductor::standalone_notice(core, &task, Tone::Warning, "Neko stopped while this was running. What was written is kept.".into(), Some("Send a follow-up to carry on.".into()));
        }
    }
}
