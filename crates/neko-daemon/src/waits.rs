//! Timers and watches in the daemon (docs/knowledge/waits.md): Neko's tools for them (`wake_me`, `watch_files`, `watch_command`,
//! `stop_watching`), the person's Stop watching, and what the rounds scheduler thread does with them on each pass: a timer that came
//! due, a watch whose files changed or whose check succeeded, a watch whose week is over. Each becomes a wake, which the subagent hub
//! delivers like a kitten's report (resumed at rest, steered in while the thread works). The records and the words are in
//! `neko_core::waits`.
//!
//! - **Files** are watched with FSEvents (`fsevents.rs`): one stream per watch on its folder, whose callback only notes the paths that
//!   changed and unparks the scheduler. Changes made while the thread's own run goes (or in the moment after it ends) are the agent's
//!   own and do not count. A watch wakes its thread at most once a minute.
//! - **Checks** of a command watch run on a thread of their own, one at a time per watch, in the thread's folder, each in its own
//!   process group and ended after a minute; nothing they start outlives the daemon's stop.
//! - **Pause automatic work** (`settings.rounds_paused`) holds all of it: a timer that comes due is skipped and the thread says so, a
//!   watch does not wake, a check does not run.

use std::collections::HashMap;
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use neko_core::waits::{self as rules, WaitRow, WatchAsk};
use neko_core::subagents::WakeKind;
use neko_protocol::schedule::SystemZone;
use neko_protocol::waits::WATCH_MAX_MS;
use neko_protocol::{Change, ErrorCode, Reply, ServerMessage, TaskId, TaskStatus, Tone, WaitKind};
use serde_json::{Value, json};

use crate::runs::McpRun;
use crate::server::{Shared, lock};

/// What the files watches heard since the scheduler last looked: per watch, the changed paths and when they were first heard.
#[derive(Default)]
struct Heard {
    changes: HashMap<String, (Vec<String>, i64)>,
}

/// The daemon's watches: their FSEvents streams, the changes heard, the checks running.
#[derive(Default)]
pub struct Hub {
    streams: Mutex<HashMap<String, crate::fsevents::Stream>>,
    heard: Arc<Mutex<Heard>>,
    /// Command checks running now: the watch's id and the check's process group.
    checking: Mutex<HashMap<String, u32>>,
}

static HUB: OnceLock<Hub> = OnceLock::new();

fn hub() -> &'static Hub {
    HUB.get_or_init(Hub::default)
}

fn answer(text: impl Into<String>, error: bool) -> Value {
    json!({"text": text.into(), "error": error})
}

fn real_now() -> i64 {
    neko_core::Clock::now_ms(&neko_core::SystemClock)
}

/// The scheduler's now (the test clock moves it, in a debug build that allows one).
fn now(shared: &Shared) -> i64 {
    shared.rounds.now()
}

fn when(shared: &Shared, at: i64) -> String {
    rules::when_words(at, now(shared), &SystemZone)
}

/// Every screen hears the thread again: its waits changed.
pub fn touch(shared: &Shared, task: &TaskId) {
    let core = lock(&shared.core);
    if let (Ok(Some(t)), Ok(rev)) = (core.store.get_task(task), core.store.revision()) {
        shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t } });
    }
}

fn paused(shared: &Shared) -> bool {
    lock(&shared.core).store.rounds_paused().unwrap_or(false)
}

// ---- The tools --------------------------------------------------------------------------------------------------------------------

/// Whether this run's task may set timers and watches: a thread's (not a kitten's).
pub fn may_wait(core: &neko_core::Core, task: &TaskId) -> bool {
    core.store.get_task(task).ok().flatten().is_some_and(|t| t.subagent.is_none())
}

fn stopped(shared: &Shared, run: &McpRun) -> bool {
    lock(&shared.runs.active).get(&run.task).is_some_and(|a| a.cancelled.load(Ordering::SeqCst))
}

/// `wake_me {at | after, note}`.
pub fn wake_me(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    if stopped(shared, run) {
        return answer("The person stopped this thread, so no timer was set.", true);
    }
    let n = now(shared);
    let due = match rules::timer_due(args["at"].as_str(), args["after"].as_str(), n, &SystemZone) {
        Ok(d) => d,
        Err(why) => return answer(why, true),
    };
    let made = lock(&shared.core).set_timer(&run.task, due, args["note"].as_str(), n);
    match made {
        Ok((w, _)) => {
            touch(shared, &run.task);
            shared.rounds.poke();
            answer(rules::timer_text(&w, &when(shared, due)), false)
        }
        Err(why) => answer(why, true),
    }
}

/// The time a watch ends: `until` as a length of time or a time, else the default; at most a week.
fn until_of(shared: &Shared, until: Option<&str>, default_ms: i64) -> Result<i64, String> {
    let n = now(shared);
    match until.map(str::trim).filter(|u| !u.is_empty()) {
        None => Ok(n + default_ms),
        Some(u) => match rules::parse_after(u) {
            Ok(ms) => Ok(n + ms),
            Err(_) => rules::parse_at(u, n, &SystemZone),
        },
    }
}

fn workdir(run: &McpRun) -> String {
    lock(&run.book).run.workdir.clone()
}

/// `watch_files {paths, until?, note}`.
pub fn watch_files(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    if stopped(shared, run) {
        return answer("The person stopped this thread, so no watch was set.", true);
    }
    let paths: Vec<String> = match &args["paths"] {
        Value::Array(a) => a.iter().filter_map(|p| p.as_str().map(str::to_owned)).collect(),
        Value::String(s) => s.split(',').map(str::to_owned).collect(),
        _ => return answer("watch_files needs paths: a list of files, folders or patterns.", true),
    };
    let root = workdir(run);
    let paths = match rules::clean_paths(&paths, &root) {
        Ok(p) => p,
        Err(why) => return answer(why, true),
    };
    let until = match until_of(shared, args["until"].as_str(), WATCH_MAX_MS) {
        Ok(u) => u,
        Err(why) => return answer(why, true),
    };
    let made = lock(&shared.core).set_watch(&run.task, &root, WatchAsk::Files { paths }, until, args["note"].as_str(), now(shared));
    match made {
        Ok((w, _)) => {
            if let Some(row) = lock(&shared.core).store.wait_row(&w.id).ok().flatten() {
                start_stream(shared, &row);
            }
            touch(shared, &run.task);
            shared.rounds.poke();
            answer(rules::watch_text(&w, &when(shared, until)), false)
        }
        Err(why) => answer(why, true),
    }
}

/// `watch_command {command, every?, until?, note}`. Under "Ask before risky steps" the person was asked before the call reached here
/// (the policy reads this tool as a command: `neko_core::policy`).
pub fn watch_command(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    if stopped(shared, run) {
        return answer("The person stopped this thread, so no watch was set.", true);
    }
    let Some(command) = args["command"].as_str().map(str::trim).filter(|c| !c.is_empty()) else { return answer("watch_command needs the command to run.", true) };
    if command.len() > rules::COMMAND_BYTES {
        return answer("That command is too long for a watch. Put it in a script in your folder and watch the script.", true);
    }
    let every = match args["every"].as_str().map(str::trim).filter(|e| !e.is_empty()) {
        None => rules::EVERY_DEFAULT_MS,
        Some(e) => match rules::parse_after(e) {
            Ok(ms) if (rules::EVERY_MIN_MS..=rules::EVERY_MAX_MS).contains(&ms) => ms,
            Ok(_) => return answer("A check runs at most once a minute and at least once a day. Give every between 1m and 1d.", true),
            Err(why) => return answer(why, true),
        },
    };
    let until = match until_of(shared, args["until"].as_str(), rules::COMMAND_UNTIL_DEFAULT_MS) {
        Ok(u) => u,
        Err(why) => return answer(why, true),
    };
    let root = workdir(run);
    let made = lock(&shared.core).set_watch(&run.task, &root, WatchAsk::Command { command: command.to_owned(), every_ms: every }, until, args["note"].as_str(), now(shared));
    match made {
        Ok((w, _)) => {
            touch(shared, &run.task);
            shared.rounds.poke();
            answer(rules::watch_text(&w, &when(shared, until)), false)
        }
        Err(why) => answer(why, true),
    }
}

/// `stop_watching {id}`.
pub fn stop_watching(shared: &Arc<Shared>, run: &McpRun, args: &Value) -> Value {
    let Some(id) = args["id"].as_str().map(|s| s.trim().trim_matches('"')) else { return answer("stop_watching needs the id of a timer or watch.", true) };
    let mine = lock(&shared.core).store.wait_row(id).ok().flatten().filter(|w| w.task == run.task);
    match mine {
        Some(w) if w.state == "active" => {
            end(shared, &w, "stopped", "The agent stopped it.");
            answer(format!("{} is stopped.", if w.wait.kind == WaitKind::Timer { "The timer" } else { "The watch" }), false)
        }
        Some(_) => answer(format!("{id} has already ended."), false),
        None => answer(format!("This thread has no timer or watch {id}."), true),
    }
}

// ---- Ending one -------------------------------------------------------------------------------------------------------------------

/// Ends a timer or watch without a wake, and puts its thread at rest when it waited on nothing else.
fn end(shared: &Arc<Shared>, w: &WaitRow, state: &str, reason: &str) {
    let ended = {
        let mut core = lock(&shared.core);
        let now = core.now();
        core.store.end_wait(&w.wait.id, state, Some(reason), now).ok().flatten().is_some()
    };
    if ended {
        stop_stream(&w.wait.id);
        settle(shared, &w.task);
        touch(shared, &w.task);
    }
}

/// A thread that rested on its timers and watches and now waits on nothing is done (it rests, at peace).
pub fn settle(shared: &Arc<Shared>, task: &TaskId) {
    if lock(&shared.runs.active).contains_key(task) {
        return;
    }
    let Some(t) = lock(&shared.core).store.get_task(task).ok().flatten() else { return };
    if t.status != TaskStatus::Waiting || crate::subagents::waits_on_anything(shared, &t) {
        return;
    }
    let mut core = lock(&shared.core);
    let now = core.now();
    if let Ok((t, rev)) = core.store.set_task_status(task, TaskStatus::Done, now) {
        shared.broadcast(&ServerMessage::Event { revision: rev, change: Change::TaskUpdated { task: t } });
    }
}

/// The person's Stop watching (`stop_waiting`).
pub fn stop_request(shared: &Arc<Shared>, task: &TaskId, id: &str) -> Reply {
    let row = lock(&shared.core).store.wait_row(id).ok().flatten().filter(|w| &w.task == task && w.state == "active");
    match row {
        Some(w) => {
            end(shared, &w, "stopped", "The person stopped it.");
            Reply::Ok
        }
        None => Reply::error(ErrorCode::NotFound, "That timer or watch has already ended."),
    }
}

/// The thread was stopped or archived: its timers and watches end with it.
pub fn end_all(shared: &Arc<Shared>, task: &TaskId, reason: &str) {
    let ids = {
        let mut core = lock(&shared.core);
        let now = core.now();
        core.store.end_waits_of(task, "stopped", reason, now).unwrap_or_default()
    };
    if !ids.is_empty() {
        for id in &ids {
            stop_stream(id);
        }
        touch(shared, task);
    }
}

// ---- Files ------------------------------------------------------------------------------------------------------------------------

fn start_stream(shared: &Shared, w: &WaitRow) {
    let Some(root) = w.root.clone() else { return };
    // FSEvents gives real paths (a temporary folder's /var is /private/var): match against the folder as the disk names it.
    let real = std::fs::canonicalize(&root).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| root.clone());
    let id = w.wait.id.clone();
    let patterns = w.wait.paths.clone();
    let heard = hub().heard.clone();
    let thread = shared.rounds.thread();
    let prefix = format!("{}/", real.trim_end_matches('/'));
    let on_change: crate::fsevents::OnChange = Box::new(move |paths| {
        let matched: Vec<String> = paths.iter().filter_map(|p| p.strip_prefix(&prefix)).filter(|rel| patterns.iter().any(|pat| neko_protocol::waits::matches(pat, rel))).map(str::to_owned).collect();
        if matched.is_empty() {
            return;
        }
        {
            let mut h = lock(&heard);
            let entry = h.changes.entry(id.clone()).or_insert_with(|| (Vec::new(), real_now()));
            for m in matched {
                if !entry.0.contains(&m) && entry.0.len() < 50 {
                    entry.0.push(m);
                }
            }
        }
        if let Some(t) = &thread {
            t.unpark();
        }
    });
    if let Some(s) = crate::fsevents::watch(&real, on_change) {
        lock(&hub().streams).insert(w.wait.id.clone(), s);
    } else {
        eprintln!("neko-daemon: FSEvents would not watch {real}");
    }
}

fn stop_stream(id: &str) {
    lock(&hub().streams).remove(id);
    lock(&hub().heard).changes.remove(id);
}

/// At start: a stream for every files watch still waiting.
pub fn start(shared: &Shared) {
    let rows = lock(&shared.core).store.active_waits().unwrap_or_default();
    for w in rows.iter().filter(|w| w.wait.kind == WaitKind::Files) {
        start_stream(shared, w);
    }
}

/// The daemon is stopping: streams end, and every check running is ended with its process group.
pub fn stopping() {
    lock(&hub().streams).clear();
    for (_, pg) in lock(&hub().checking).drain() {
        unsafe {
            libc::kill(-(pg as i32), libc::SIGKILL);
        }
    }
}

/// How long after a thread's run ends its own changes may still be arriving (FSEvents tells them a second late).
const OWN_CHANGES_MS: i64 = 3_000;

/// Whether changes heard at `heard_ms` were the agent's own: its run was going then, or had only just ended.
fn own_changes(shared: &Shared, task: &TaskId, heard_ms: i64) -> bool {
    if lock(&shared.runs.active).contains_key(task) {
        return true;
    }
    let ended = lock(&shared.core).store.runs_of(task).ok().and_then(|r| r.last().and_then(|r| r.ended_ms));
    ended.is_some_and(|e| heard_ms <= e + OWN_CHANGES_MS)
}

// ---- The scheduler's pass ---------------------------------------------------------------------------------------------------------

/// What the scheduler does with timers and watches on each pass. Returns when it should look again (the scheduler's clock), if
/// something will come due by itself.
pub fn pass(shared: &Arc<Shared>) -> Option<i64> {
    let n = now(shared);
    let is_paused = paused(shared);
    let rows = lock(&shared.core).store.active_waits().unwrap_or_default();
    let by_id: HashMap<String, WaitRow> = rows.iter().map(|r| (r.wait.id.clone(), r.clone())).collect();
    let due = rules::due(rows, n);
    let mut woke = false;
    let mut next = due.next_ms;
    for w in due.timers {
        if is_paused {
            skip(shared, &w, n);
        } else {
            woke |= fire(shared, &w, WakeKind::Timer, "due", Vec::new(), Some("fired"));
        }
    }
    for w in due.ended {
        if is_paused {
            end(shared, &w, "ended", "It ended while automatic work was paused.");
        } else {
            woke |= fire(shared, &w, WakeKind::Watch, "ended", Vec::new(), Some("ended"));
        }
    }
    if !is_paused {
        for w in due.checks {
            check(shared, w);
        }
    }
    // Files that changed since the last pass.
    let heard: Vec<(String, (Vec<String>, i64))> = lock(&hub().heard).changes.drain().collect();
    for (id, (paths, at)) in heard {
        let Some(w) = by_id.get(&id) else { continue };
        if own_changes(shared, &w.task, at) || is_paused {
            continue;
        }
        // At most once a minute: what changed meanwhile waits for the next wake.
        if let Some(f) = w.fired_ms
            && n - f < rules::FILES_GAP_MS
        {
            let mut h = lock(&hub().heard);
            let e = h.changes.entry(id).or_insert_with(|| (Vec::new(), at));
            e.0.extend(paths);
            let again = f + rules::FILES_GAP_MS;
            next = Some(next.map_or(again, |x| x.min(again)));
            continue;
        }
        woke |= fire(shared, w, WakeKind::Watch, "changed", paths, None);
    }
    if woke {
        crate::subagents::poke(shared);
    }
    next
}

/// A timer or watch fires: its wake is stored (and the timer settled) in one transaction, and the thread hears.
fn fire(shared: &Arc<Shared>, w: &WaitRow, kind: WakeKind, status: &str, files: Vec<String>, ends: Option<&str>) -> bool {
    let ok = {
        let mut core = lock(&shared.core);
        let now = core.now();
        let wake = rules::wake_of(&mut core, w, kind, status, files, now);
        core.store.fire_wait(w, &wake, ends, now).is_ok()
    };
    if ok {
        if ends.is_some() {
            stop_stream(&w.wait.id);
        }
        touch(shared, &w.task);
    }
    ok
}

/// A timer came due while automatic work was paused: it is skipped, and the thread says so.
fn skip(shared: &Arc<Shared>, w: &WaitRow, n: i64) {
    let reason = "Automatic work was paused.";
    let ended = {
        let mut core = lock(&shared.core);
        let now = core.now();
        core.store.end_wait(&w.wait.id, "skipped", Some(reason), now).ok().flatten().is_some()
    };
    if !ended {
        return;
    }
    let at = w.wait.at_ms.map(|a| rules::when_words(a, n, &SystemZone)).unwrap_or_default();
    let note = if w.wait.note.is_empty() { String::new() } else { format!(" It was for: {}", w.wait.note) };
    let text = format!("Neko did not wake this thread at {at}: automatic work is paused.{note}");
    {
        let mut core = lock(&shared.core);
        if let Ok(Some(t)) = core.store.get_task(&w.task)
            && let Ok(outs) = neko_core::conductor::standalone_notice(&mut core, &t, Tone::Info, text, Some("Resume automatic work in Rounds, or write to the thread to carry on.".into()), None)
        {
            drop(core);
            crate::runs::push(shared, &w.task, outs, None);
        }
    }
    settle(shared, &w.task);
    touch(shared, &w.task);
}

/// The longest output of a check a wake carries (the end of it).
const OUTPUT_KEPT: usize = 2_000;

/// Runs a command watch's check on a thread of its own (one at a time per watch). Exit 0 fires the watch; anything else, or a check
/// that runs past a minute, is "not yet".
fn check(shared: &Arc<Shared>, w: WaitRow) {
    if lock(&hub().checking).contains_key(&w.wait.id) {
        return;
    }
    let Some(command) = w.wait.command.clone() else { return };
    let root = w.root.clone().unwrap_or_else(|| "/".into());
    let mut cmd = Command::new("/bin/sh");
    cmd.arg("-c").arg(&command).current_dir(&root).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).process_group(0);
    let child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            // The folder is gone, or the shell is: it counts as not yet, and is tried again.
            let mut core = lock(&shared.core);
            let _ = core.store.checked(&w.wait.id, now(shared), None);
            eprintln!("neko-daemon: a watch's check could not start: {e}");
            return;
        }
    };
    lock(&hub().checking).insert(w.wait.id.clone(), child.id());
    let shared = shared.clone();
    let _ = std::thread::Builder::new().name("neko-check".into()).spawn(move || {
        let (exit, output) = wait_for(child);
        lock(&hub().checking).remove(&w.wait.id);
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        let at = now(&shared);
        let _ = lock(&shared.core).store.checked(&w.wait.id, at, exit);
        let still = lock(&shared.core).store.wait_row(&w.wait.id).ok().flatten().filter(|r| r.state == "active");
        match still {
            Some(row) if exit == Some(0) && !paused(&shared) => {
                if fire(&shared, &row, WakeKind::Watch, "succeeded", output, Some("fired")) {
                    crate::subagents::poke(&shared);
                }
            }
            Some(_) => touch(&shared, &w.task),
            None => {}
        }
        shared.rounds.poke();
    });
}

/// Waits for a check, at most a minute (then its whole process group is ended). Its exit code (None: it did not finish), and the
/// last lines it printed.
fn wait_for(mut child: std::process::Child) -> (Option<i32>, Vec<String>) {
    use std::io::Read;
    let until = Instant::now() + Duration::from_millis(rules::CHECK_TIMEOUT_MS);
    let pg = child.id() as i32;
    let mut out = child.stdout.take();
    let mut err = child.stderr.take();
    // Read both pipes on threads of their own so a chatty check never blocks on a full pipe.
    let reader = |p: Option<Box<dyn Read + Send>>| std::thread::spawn(move || {
        let mut s = Vec::new();
        if let Some(mut p) = p {
            let _ = p.by_ref().take(256 * 1024).read_to_end(&mut s);
        }
        s
    });
    let o = reader(out.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let e = reader(err.take().map(|p| Box::new(p) as Box<dyn Read + Send>));
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.code(),
            Ok(None) if Instant::now() < until => std::thread::sleep(Duration::from_millis(200)),
            _ => {
                unsafe {
                    libc::kill(-pg, libc::SIGKILL);
                }
                let _ = child.wait();
                break None;
            }
        }
    };
    // Whatever the check left behind in its group goes too.
    unsafe {
        libc::kill(-pg, libc::SIGKILL);
    }
    let mut text = String::from_utf8_lossy(&o.join().unwrap_or_default()).into_owned();
    text.push_str(&String::from_utf8_lossy(&e.join().unwrap_or_default()));
    let mut tail = text.trim().to_owned();
    if tail.len() > OUTPUT_KEPT {
        let mut cut = tail.len() - OUTPUT_KEPT;
        while !tail.is_char_boundary(cut) {
            cut += 1;
        }
        tail = tail[cut..].to_owned();
    }
    (exit, tail.lines().map(str::to_owned).collect())
}

/// What a thread waits on that is a timer or watch, for the `waiting` check (`subagents.rs`).
pub fn has_waits(core: &neko_core::Core, task: &TaskId) -> bool {
    core.store.has_waits(task).unwrap_or(false)
}

/// The waits that end when the person archives a thread that waits only on them.
pub fn only_waits(shared: &Shared, t: &neko_protocol::Task) -> bool {
    !t.waits.is_empty() && !crate::subagents::has_live_subagents(shared, &t.id) && !crate::crew::has_live_crew(&lock(&shared.core), t)
}

