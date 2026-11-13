//! Goals that keep working: the loop. See docs/knowledge/goals-that-keep-working.md.
//!
//! A goal works through its ticket, so every admission rule (machine steward,
//! writable caps, budgets, worktrees, review, the stall watchdog) already
//! applies. One turn is one run of the ticket. The goal's tick watches the
//! ticket; when a turn ends it runs the goal's check, files the result in the
//! evidence ledger, asks a cheap judge, and applies the ruling (the rules for
//! what a ruling may do are in `neko_core::goals`). Model work and the check
//! run without the database lock; every change to a goal goes through
//! `commit`, which re-reads the goal and drops the change if the person (or
//! another tick) moved it meanwhile.
use super::*;
use neko_core::goals as core;
use neko_protocol::goals::*;

/// The goal tick.
const TICK: Duration = Duration::from_secs(3);
/// A check that runs longer than this is ended and recorded as failed.
const CHECK_TIMEOUT: Duration = Duration::from_secs(10 * 60);
/// A running goal's clock is moved this often (its own deferred row, never
/// the goal's), and never by more than `MAX_CLOCK_STEP_MS` at once, so a
/// sleeping Mac or a stopped daemon does not spend the wall-clock budget.
const CLOCK_EVERY_MS: i64 = 15_000;
const MAX_CLOCK_STEP_MS: i64 = 60_000;
/// What a ticket's failure says when the evidence gate gave up.
pub(super) const BLOCKED_PREFIX: &str = "Blocked:";

static GOAL_LOCK: Mutex<()> = Mutex::new(());

/// Everything the loop needs from outside, so tests stub the model, the
/// shell, the clock and GitHub.
pub(super) trait Env: Sync {
    fn now_ms(&self) -> i64;
    fn run_check(&self, directory: &std::path::Path, command: &str) -> CheckResult;
    /// The judge's raw answer.
    fn judge(&self, prompt: &str) -> Result<String, String>;
    /// The machine steward says hold new work.
    fn machine_held(&self) -> bool;
    /// `OPEN`, `MERGED` or `CLOSED`.
    fn pr_state(&self, pr: &str) -> Option<String>;
    fn pid_alive(&self, pid: u32) -> bool;
    /// Whether a piece of evidence is real. A path must be inside the folder
    /// and created or changed since the goal began (a file that was already
    /// there proves nothing); a pull request must exist; a link must answer.
    fn verify(&self, directory: &std::path::Path, since_ms: i64, evidence: &core::Evidence) -> bool {
        match evidence {
            core::Evidence::Path(relative) => artifact_since(directory, relative, since_ms),
            core::Evidence::PullRequest(pr) => self.pr_state(pr).is_some(),
            core::Evidence::Url(url) => self.url_ok(url),
        }
    }
    /// A link that answers (a pull request URL is checked as the pull request).
    fn url_ok(&self, url: &str) -> bool;
}

/// `relative` names something inside `directory` that was modified at or after `since_ms`.
pub(super) fn artifact_since(directory: &std::path::Path, relative: &str, since_ms: i64) -> bool {
    let (Ok(root), Ok(path)) = (directory.canonicalize(), directory.join(relative).canonicalize()) else { return false };
    if path == root || !path.starts_with(&root) {
        return false;
    }
    let modified = |p: &std::path::Path| std::fs::metadata(p).and_then(|m| m.modified()).ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis() as i64);
    if path.is_dir() {
        // A folder counts when something in it is new.
        return std::fs::read_dir(&path).is_ok_and(|entries| entries.flatten().any(|e| modified(&e.path()) >= since_ms));
    }
    modified(&path) >= since_ms
}

pub(super) struct Real {
    pub db: Arc<Mutex<Db>>,
}

impl Env for Real {
    fn now_ms(&self) -> i64 {
        store::now_ms()
    }

    fn run_check(&self, directory: &std::path::Path, command: &str) -> CheckResult {
        run_shell(directory, command, CHECK_TIMEOUT, store::now_ms())
    }

    fn judge(&self, prompt: &str) -> Result<String, String> {
        let claude_ok = !neko_core::runtime_health::load(&self.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).is_bad();
        let mut last = String::from("No judge model is available");
        for runtime in judge_runtimes(claude_ok) {
            let scratch = std::env::temp_dir().join(format!("neko-goal-judge-{}", store::new_id()));
            let result = (|| {
                std::fs::create_dir(&scratch).map_err(|e| format!("Cannot create the judge's scratch folder: {e}"))?;
                native_runner::extract(
                    &native_runner::RunSpec { directory: scratch.clone(), prompt: prompt.to_owned(), writable: false, timeout: Duration::from_secs(90), runtime },
                    &AtomicBool::new(false),
                )
            })();
            let _ = std::fs::remove_dir(&scratch);
            match result {
                Ok(text) => return Ok(text),
                Err(error) => last = error,
            }
        }
        Err(last)
    }

    fn machine_held(&self) -> bool {
        neko_core::machine::latest().is_some_and(|health| health.holding.is_some())
    }

    fn pr_state(&self, pr: &str) -> Option<String> {
        static LAST: Mutex<Vec<(String, i64, Option<String>)>> = Mutex::new(Vec::new());
        let now = store::now_ms();
        let mut last = LAST.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((_, at, state)) = last.iter().find(|(key, _, _)| key == pr) {
            if now - at < 60_000 {
                return state.clone();
            }
        }
        let gh = native_runner::publish::resolve_gh().ok()?;
        let (repo, number) = pr.split_once('#')?;
        let output = std::process::Command::new(gh)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .args(["pr", "view", number, "--repo", repo, "--json", "state"])
            .output()
            .ok()
            .filter(|o| o.status.success());
        let state = output
            .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok())
            .and_then(|v| v["state"].as_str().map(str::to_owned));
        last.retain(|(key, _, _)| key != pr);
        last.push((pr.to_owned(), now, state.clone()));
        state
    }

    fn url_ok(&self, url: &str) -> bool {
        // A GitHub pull request link is the pull request.
        let parts: Vec<&str> = url.trim_start_matches("https://").trim_start_matches("http://").split('/').collect();
        if let ["github.com", owner, repo, "pull", number, ..] = parts.as_slice() {
            return self.pr_state(&format!("{owner}/{repo}#{number}")).is_some();
        }
        std::process::Command::new("/usr/bin/curl")
            .args(["-sIL", "--max-time", "10", "-o", "/dev/null", "-w", "%{http_code}", url])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .is_some_and(|code| code.trim().starts_with('2'))
    }

    fn pid_alive(&self, pid: u32) -> bool {
        // Signal 0 only asks whether the process exists.
        unsafe { libc::kill(pid as i32, 0) == 0 || *libc::__error() == libc::EPERM }
    }
}

/// The cheapest models that can judge: Claude Code's Haiku, then the
/// smallest Codex model the catalog lists (a "mini"), else Codex's default.
pub(super) fn judge_runtimes(claude_ok: bool) -> Vec<AgentRuntime> {
    let mut runtimes = Vec::new();
    if claude_ok {
        runtimes.push(AgentRuntime { provider: "claude".into(), model: "haiku".into(), ..Default::default() });
    }
    let catalog = neko_core::agent_catalog::catalog(false);
    let mini = catalog
        .sources
        .iter()
        .filter(|s| s.provider.is_empty() || s.provider == "codex")
        .flat_map(|s| s.models.iter())
        .find(|m| m.id.to_ascii_lowercase().contains("mini") && m.access != neko_protocol::agent_models::ModelAccess::Unavailable)
        .map(|m| m.id.clone());
    runtimes.push(AgentRuntime { provider: String::new(), model: mini.unwrap_or_default(), ..Default::default() });
    runtimes
}

/// Runs `command` through the shell in `directory`, both output streams
/// merged, ended (with its process group) after `limit`.
pub(super) fn run_shell(directory: &std::path::Path, command: &str, limit: Duration, now_ms: i64) -> CheckResult {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    let home = std::env::var("HOME").unwrap_or_default();
    let path = format!("{}:{home}/.cargo/bin:{home}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", std::env::var("PATH").unwrap_or_default());
    let mut shell = std::process::Command::new("/bin/sh");
    shell.arg("-c").arg(format!("exec 2>&1; {command}")).current_dir(directory).env("PATH", path).env("CI", "1").env("NO_COLOR", "1").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).process_group(0);
    for (name, value) in neko_core::machine::TEST_WORKER_ENV {
        shell.env(name, value);
    }
    let mut result = CheckResult { command: command.to_owned(), exit_code: None, tail: String::new(), at_ms: now_ms, source: "neko".into() };
    let mut child = match shell.spawn() {
        Ok(child) => child,
        Err(error) => {
            result.tail = format!("Could not start the check: {error}");
            return result;
        }
    };
    let pid = child.id() as i32;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader = {
        let buffer = buffer.clone();
        std::thread::spawn(move || {
            let mut chunk = [0_u8; 8192];
            while let Ok(n) = stdout.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let mut buffer = buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                buffer.extend_from_slice(&chunk[..n]);
                // Only the end matters.
                let extra = buffer.len().saturating_sub(64 * 1024);
                buffer.drain(..extra);
            }
        })
    };
    let started = std::time::Instant::now();
    let mut timed_out = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) if started.elapsed() >= limit => {
                timed_out = true;
                unsafe { libc::kill(-pid, libc::SIGKILL) };
                let _ = child.kill();
                break child.wait().ok();
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(_) => break None,
        }
    };
    // A background process that kept the pipe open must not hold the check.
    unsafe { libc::kill(-pid, libc::SIGKILL) };
    // A process that left its group and kept the pipe open must not hold the
    // check: give the reader a moment, then go on with what it has (it ends
    // on its own when the pipe closes).
    let waited = std::time::Instant::now();
    while !reader.is_finished() && waited.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(20));
    }
    let text = String::from_utf8_lossy(&buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned();
    result.exit_code = if timed_out { None } else { status.and_then(|s| s.code()).map(i64::from) };
    // A long output goes to a file under the data dir; the prompt gets its head, tail and path.
    result.tail = if text.len() > neko_core::spill::LIMIT { neko_core::spill::bounded("check", &text) } else { core::tail(&text, core::TAIL_BYTES) };
    if timed_out {
        result.tail = format!("Ended after {} minutes.\n{}", limit.as_secs() / 60, result.tail);
    }
    result
}

// ---------------------------------------------------------------------------
// Creating goals
// ---------------------------------------------------------------------------

/// What a new goal is made from.
pub(crate) struct Spec<'a> {
    pub workspace_id: &'a str,
    pub objective: &'a str,
    pub done_check: &'a str,
    pub constraints: &'a str,
    pub max_turns: Option<u32>,
    pub ticket_id: Option<&'a str>,
    pub conversation_id: Option<&'a str>,
    /// The person set it (`/goal`, the app): its check may run and it asks for
    /// local work. False for a goal a model proposed in a reply: it is stored
    /// paused, with its check unrun, until the person resumes it.
    pub by_person: bool,
}

/// Stores a goal and makes sure a ticket works it: the one named, or a new
/// one in the workspace. A goal the person set is their own request for local
/// work (nothing is published); it starts at once. It never widens a ticket:
/// the ticket keeps its assessment and its scope, and a read-only ticket
/// cannot carry a goal.
pub(crate) fn create(db: &Db, spec: &Spec) -> Result<Goal, String> {
    let now = store::now_ms();
    let mut goal = core::new_goal(spec.workspace_id, spec.objective, spec.done_check, spec.constraints, spec.max_turns, now)?;
    goal.conversation_id = spec.conversation_id.map(str::to_owned);
    let snapshot = store::load(db)?;
    if !snapshot.workspaces.iter().any(|w| w.id == spec.workspace_id) {
        return Err("That workspace no longer exists".into());
    }
    let mut start_directly = None;
    match spec.ticket_id {
        Some(id) => {
            let task = snapshot.tasks.iter().find(|t| t.id == id).ok_or("That ticket no longer exists")?;
            if task.workspace_id != spec.workspace_id {
                return Err("That ticket is in another workspace".into());
            }
            if task.status == TaskStatus::Cancelled {
                return Err("That ticket was cancelled".into());
            }
            if snapshot.task_read_only.contains(id) {
                return Err("That ticket is read-only, so it cannot carry a goal".into());
            }
            goal.ticket_id = Some(id.to_owned());
            // Attaching a goal the person set to a ticket that has not started
            // is them asking for it to be worked.
            if task.status == TaskStatus::Queued && spec.by_person {
                start_directly = Some(id.to_owned());
            }
            if task.status == TaskStatus::AwaitingApproval {
                goal.status = GoalStatus::Waiting;
                goal.wait = Some(Wait { reason: "The ticket is waiting for your approval of its plan.".into(), ticket: true, ..Wait::default() });
            }
        }
        None => {
            let title = core::head(spec.objective.lines().next().unwrap_or(spec.objective), 80);
            let mut text = spec.objective.trim().to_owned();
            if !goal.done_check.is_empty() {
                text.push_str(&format!("\n\nDone when this passes in the ticket's folder: {}", goal.done_check));
            }
            if !goal.constraints.is_empty() {
                text.push_str(&format!("\n\nConstraints: {}", goal.constraints));
            }
            let mut after = store::apply(db, Command::CreateTask { workspace_id: spec.workspace_id.to_owned(), title, goal: text })?;
            let task = after.tasks.last_mut().ok_or("The ticket was not created")?;
            task.request = neko_chat::ticket_request(spec.objective);
            goal.ticket_id = Some(task.id.clone());
            if spec.by_person {
                start_directly = Some(task.id.clone());
            }
            store::save(db, &after)?;
        }
    }
    if !spec.by_person {
        goal.status = GoalStatus::Paused;
        goal.paused_reason = "Neko suggested this goal and its check. Read the check, then Resume to start it.".into();
    }
    goal.turns = 1;
    goal.in_turn = true;
    // The goal row first: a ticket that already has a goal is refused before
    // anything about the ticket changes.
    core::create(db, goal.clone())?;
    let note = if spec.by_person { "Goal set" } else { "Goal suggested by Neko; waiting for you" };
    core::push_journal(db, &goal.subject(), JournalEntry { at_ms: now, turn: 1, note: note.into(), plan: core::head(spec.objective, 200), ..Default::default() })?;
    if let Some(id) = start_directly {
        let mut after = store::load(db)?;
        after.start_when_planned.insert(id);
        store::save(db, &after)?;
    }
    Ok(goal)
}

// ---------------------------------------------------------------------------
// `/goal` in the chat
// ---------------------------------------------------------------------------

/// What a `/goal` message asks for.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Slash {
    /// `/goal` alone: where the goals stand.
    List,
    /// `/goal <objective> [--check <command>] [--turns <n>]`.
    Set { objective: String, done_check: String, max_turns: Option<u32> },
}

/// Reads a chat message that is a `/goal` command; None for anything else.
pub(crate) fn slash(text: &str) -> Option<Slash> {
    let rest = text.trim().strip_prefix("/goal")?;
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let mut rest = rest.trim().to_owned();
    if rest.is_empty() {
        return Some(Slash::List);
    }
    let mut max_turns = None;
    if let Some((before, after)) = rest.rsplit_once(" --turns ") {
        if let Ok(n) = after.trim().parse::<u32>() {
            max_turns = Some(n);
            rest = before.trim().to_owned();
        }
    }
    let (objective, done_check) = match rest.split_once(" --check ") {
        Some((objective, check)) => (objective.trim().to_owned(), check.trim().to_owned()),
        None => (rest, String::new()),
    };
    Some(Slash::Set { objective, done_check, max_turns })
}

/// The chat's answer to a `/goal` message, and the tickets it opened.
pub(crate) fn slash_reply(db: &Db, command: Slash, workspace_id: Option<&str>) -> (String, Vec<String>) {
    let snapshot = match store::load(db) {
        Ok(snapshot) => snapshot,
        Err(error) => return (format!("I could not read the goals: {error}"), vec![]),
    };
    match command {
        Slash::List => {
            let active: Vec<&Goal> = snapshot.goals.iter().filter(|g| !g.status.is_finished()).collect();
            let mut text = String::from("`/goal <what you want finished>` keeps Neko working until it is proven done. Add `--check <command>` for a command that passes only when it is done, and `--turns <n>` for a turn budget (20 by default).");
            for goal in active.iter().take(5) {
                text.push_str(&format!("\n- {:?}: {} (turn {}/{})", goal.status, core::head(&goal.objective, 100), goal.turns, goal.budget.max_turns));
            }
            (text, vec![])
        }
        Slash::Set { objective, done_check, max_turns } => {
            let workspace = workspace_id.map(str::to_owned).or_else(|| (snapshot.workspaces.len() == 1).then(|| snapshot.workspaces[0].id.clone()));
            let Some(workspace) = workspace else { return ("Choose a workspace for this goal first, then send it again.".into(), vec![]) };
            let profile = snapshot.agent_profiles.for_scope(Some(&workspace)).to_owned();
            let conversation = neko_core::runtime_selection::home_id(&profile, workspace_id);
            match create(db, &Spec { workspace_id: &workspace, objective: &objective, done_check: &done_check, constraints: "", max_turns, ticket_id: None, conversation_id: Some(&conversation), by_person: true }) {
                Ok(goal) => (
                    format!("Goal set: {}. I will keep working turn after turn, up to {} turns, and it is done only when {}.", core::head(&goal.objective, 160), goal.budget.max_turns, if goal.done_check.is_empty() { "a file or link proves it".to_owned() } else { format!("`{}` passes", core::head(&goal.done_check, 120)) }),
                    goal.ticket_id.into_iter().collect(),
                ),
                Err(error) => (format!("I could not set the goal: {error}"), vec![]),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The tick
// ---------------------------------------------------------------------------

/// Applies `change` to the goal as it is now, if `still` says it is still the
/// goal the tick looked at. False when the goal moved in the meantime.
fn commit(db: &Arc<Mutex<Db>>, id: &str, still: impl Fn(&Goal) -> bool, change: impl FnOnce(&Db, &mut Goal) -> Result<(), String>) -> Result<bool, String> {
    let _goals = GOAL_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = db.lock().map_err(|_| "Goal storage unavailable")?;
    let Some(mut goal) = core::load(&db, id)? else { return Ok(false) };
    if !still(&goal) {
        return Ok(false);
    }
    db.atomic(|| {
        change(&db, &mut goal)?;
        goal.updated_ms = store::now_ms();
        core::save(&db, &goal)
    })?;
    Ok(true)
}

/// One pass over every active goal, one after another (tests, and the
/// reference for what a pass does).
pub(super) fn tick(db: &Arc<Mutex<Db>>, env: &dyn Env) -> Result<(), String> {
    let goals = core::load_all(&*db.lock().map_err(|_| "Goal storage unavailable")?)?;
    for goal in goals.into_iter().filter(|g| g.status.is_active()) {
        let id = goal.id.clone();
        if let Err(error) = step(db, env, goal) {
            eprintln!("neko goals: {id}: {error}");
        }
    }
    Ok(())
}

/// Goals whose step is running on a thread now.
static IN_FLIGHT: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// The daemon's pass: every active goal steps on its own thread, so a slow
/// check or a slow judge holds that goal only, never the others, and a goal
/// is never stepped twice at once.
fn tick_concurrently(db: &Arc<Mutex<Db>>, env: &Arc<Real>) -> Result<(), String> {
    let goals = core::load_all(&*db.lock().map_err(|_| "Goal storage unavailable")?)?;
    for goal in goals.into_iter().filter(|g| g.status.is_active()) {
        {
            let mut busy = IN_FLIGHT.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            if busy.contains(&goal.id) {
                continue;
            }
            busy.push(goal.id.clone());
        }
        let (db, env, id) = (db.clone(), env.clone(), goal.id.clone());
        let spawned = std::thread::Builder::new().name(format!("neko-goal-{id}")).spawn({
            let id = id.clone();
            move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| step(&db, &*env, goal)));
                match result {
                    Ok(Err(error)) => eprintln!("neko goals: {id}: {error}"),
                    Err(_) => eprintln!("neko goals: {id}: step panicked; continuing"),
                    Ok(Ok(())) => {}
                }
                IN_FLIGHT.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retain(|g| *g != id);
            }
        });
        if spawned.is_err() {
            IN_FLIGHT.lock().unwrap_or_else(std::sync::PoisonError::into_inner).retain(|g| *g != id);
        }
    }
    Ok(())
}

fn step(db: &Arc<Mutex<Db>>, env: &dyn Env, mut goal: Goal) -> Result<(), String> {
    let now = env.now_ms();
    // Only running time counts, and only a running goal writes its clock.
    if goal.status == GoalStatus::Running {
        if goal.last_tick_ms <= 0 {
            goal.last_tick_ms = now;
        } else if now - goal.last_tick_ms >= CLOCK_EVERY_MS {
            goal.wall_ms += (now - goal.last_tick_ms).clamp(0, MAX_CLOCK_STEP_MS);
            goal.last_tick_ms = now;
            core::save_clock_only(&*db.lock().map_err(|_| "Goal storage unavailable")?, &goal)?;
        }
    }
    match goal.status {
        GoalStatus::Running => running(db, env, goal),
        GoalStatus::Waiting => waiting(db, env, goal),
        GoalStatus::Held => held(db, env, goal),
        // Blocked waits for the person's answer; paused and finished goals are not ticked.
        _ => Ok(()),
    }
}

/// The ticket's last failure, in its own words.
fn failure_of(task: &Task) -> String {
    task.events
        .iter()
        .rev()
        .find(|e| matches!(e.role.as_str(), "supervisor" | "system") && !e.message.starts_with("USAGE "))
        .map(|e| e.message.clone())
        .unwrap_or_else(|| "The turn failed".into())
}

fn task_directory(snapshot: &Snapshot, task: &Task) -> std::path::PathBuf {
    match (&task.worktree, snapshot.workspaces.iter().find(|w| w.id == task.workspace_id)) {
        (Some(path), _) => path.into(),
        (None, Some(workspace)) => snapshot.root_for(task, workspace).into(),
        (None, None) => std::path::PathBuf::from("."),
    }
}

fn running(db: &Arc<Mutex<Db>>, env: &dyn Env, goal: Goal) -> Result<(), String> {
    let snapshot = store::load(&*db.lock().map_err(|_| "Goal storage unavailable")?)?;
    let Some(task) = goal.ticket_id.as_deref().and_then(|id| snapshot.tasks.iter().find(|t| t.id == id)) else {
        commit(db, &goal.id, |g| g.status == GoalStatus::Running, |_, g| {
            g.status = GoalStatus::Stopped;
            g.paused_reason = "Its ticket is gone.".into();
            Ok(())
        })?;
        return Ok(());
    };
    let (turn, id) = (goal.turns, goal.id.clone());
    let same_turn = move |g: &Goal| g.status == GoalStatus::Running && g.turns == turn && g.in_turn;
    if !goal.in_turn {
        // The person already sent the ticket on (a reply of their own): that run is the turn.
        if matches!(task.status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing) {
            let turn = goal.turns;
            commit(db, &id, move |g| g.status == GoalStatus::Running && g.turns == turn && !g.in_turn, |_, g| {
                g.in_turn = true;
                Ok(())
            })?;
            return Ok(());
        }
        // A wait ended, an answer came, or the person resumed: the next turn is due.
        if env.machine_held() {
            return Ok(());
        }
        // The daily spend ceiling: no new turn today (one Inbox item asks to raise it).
        if crate::routine_care::spend_held(&*db.lock().map_err(|_| "Goal storage unavailable")?, env.now_ms()) {
            return Ok(());
        }
        let (still, now) = (move |g: &Goal| g.status == GoalStatus::Running && g.turns == turn && !g.in_turn, env.now_ms());
        commit(db, &id, still, |db, g| {
            if let Some(why) = core::budget_reached(g, task_cost_cents(task)) {
                return pause(db, g, &why, now);
            }
            let reason = std::mem::take(&mut g.reason);
            let text = core::continue_prompt(g, g.last_check.as_ref(), &reason, None);
            issue_turn(db, g, &text, true)
        })?;
        return Ok(());
    }
    match task.status {
        TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => Ok(()),
        TaskStatus::AwaitingApproval => {
            commit(db, &id, same_turn, |_, g| {
                g.status = GoalStatus::Waiting;
                g.wait = Some(Wait { reason: "The ticket is waiting for your approval of its plan.".into(), ticket: true, ..Wait::default() });
                Ok(())
            })?;
            Ok(())
        }
        TaskStatus::Cancelled => {
            commit(db, &id, same_turn, |_, g| {
                g.status = GoalStatus::Stopped;
                g.paused_reason = "Its ticket was cancelled.".into();
                Ok(())
            })?;
            Ok(())
        }
        TaskStatus::ReadyForReview | TaskStatus::Completed => evaluate(db, env, &goal, &snapshot, task, None),
        TaskStatus::Failed => failed_turn(db, env, &goal, &snapshot, task),
    }
}

fn task_cost_cents(task: &Task) -> f64 {
    store::task_cost_usd(task) * 100.0
}

/// Queues the goal's next turn on its ticket.
fn issue_turn(db: &Db, goal: &mut Goal, text: &str, count: bool) -> Result<(), String> {
    let ticket = goal.ticket_id.clone().ok_or("The goal has no ticket")?;
    if count {
        goal.turns += 1;
    }
    store::goal_turn(db, &ticket, &format!("Goal turn {} of {}.\n{}", goal.turns, goal.budget.max_turns, text))?;
    goal.in_turn = true;
    goal.status = GoalStatus::Running;
    goal.last_tick_ms = store::now_ms();
    Ok(())
}

/// Pauses at a cap with its state, and raises the one Inbox item that
/// offers more.
fn pause(db: &Db, goal: &mut Goal, why: &str, now: i64) -> Result<(), String> {
    goal.status = GoalStatus::Paused;
    goal.in_turn = false;
    goal.paused_reason = format!("Paused: {why}.");
    if goal.extend_seq.is_none() {
        let more = extend_by(goal);
        let detail = format!("{} {}\nGoal: {}\nTurn {} of {}. Choose Extend on the goal to keep going, or leave it paused.", "Paused:", why, core::head(&goal.objective, 300), goal.turns, goal.budget.max_turns);
        goal.extend_seq = Some(crate::routines::raise_goal_item(db, &goal.workspace_id, &format!("Extend by {more} turns?"), &detail, &format!("goal:{}", goal.id))?);
    }
    core::push_journal(db, &goal.subject(), JournalEntry { at_ms: now, turn: goal.turns, note: goal.paused_reason.clone(), ..Default::default() })
}

/// What "Extend" offers: half the cap again, at least five turns.
pub(crate) fn extend_by(goal: &Goal) -> u32 {
    (goal.budget.max_turns / 2).max(5)
}

fn failed_turn(db: &Arc<Mutex<Db>>, env: &dyn Env, goal: &Goal, snapshot: &Snapshot, task: &Task) -> Result<(), String> {
    let error = failure_of(task);
    let now = env.now_ms();
    let (turn, id) = (goal.turns, goal.id.clone());
    let same_turn = move |g: &Goal| g.status == GoalStatus::Running && g.turns == turn && g.in_turn;
    if neko_core::runtime_health::is_usage_limit(&error) {
        // Parked until the limit resets; the Codex fallback is not changed here.
        let until = core::hold_until_ms(&error, now);
        commit(db, &id, same_turn, |db, g| {
            g.status = GoalStatus::Held;
            g.hold_until_ms = Some(until);
            core::push_journal(db, &g.subject(), JournalEntry { at_ms: now, turn: g.turns, note: format!("Usage limit; held until {}", neko_core::inbox::iso_from_ms(until)), session_id: snapshot.task_sessions.get(&task.id).cloned(), ..Default::default() })
        })?;
        return Ok(());
    }
    if let Some(blocked) = error.strip_prefix(BLOCKED_PREFIX) {
        let question = format!("{} Keep trying, or change the approach?", core::head(blocked.trim(), 400));
        commit(db, &id, same_turn, |db, g| block(db, g, &question, "Keep trying with a different approach.", now))?;
        return Ok(());
    }
    if core::is_stall(&error) && goal.stall_resumes == 0 {
        // A run with no output for ten minutes was ended; resume it once.
        commit(db, &id, same_turn, |db, g| {
            g.stall_resumes = 1;
            core::push_journal(db, &g.subject(), JournalEntry { at_ms: now, turn: g.turns, note: "Stalled with no output for ten minutes; resumed once".into(), session_id: snapshot.task_sessions.get(&task.id).cloned(), ..Default::default() })?;
            let text = format!("The previous turn was ended because it produced no output for ten minutes. Pick up where the journal says you were.\n{}", core::context(g));
            issue_turn(db, g, &text, false)
        })?;
        return Ok(());
    }
    evaluate(db, env, goal, snapshot, task, Some(error))
}

/// Raises one question with a default answer, then waits for the person.
fn block(db: &Db, goal: &mut Goal, question: &str, default_answer: &str, now: i64) -> Result<(), String> {
    goal.status = GoalStatus::Blocked;
    goal.in_turn = false;
    let detail = format!("{question}\nIf you do not answer, Neko will go with: {default_answer}\nGoal: {}", core::head(&goal.objective, 300));
    let seq = crate::routines::raise_goal_item(db, &goal.workspace_id, &core::head(question, 120), &detail, &format!("goal:{}", goal.id))?;
    goal.blocked = Some(Blocked { question: question.to_owned(), default_answer: default_answer.to_owned(), inbox_seq: Some(seq) });
    core::push_journal(db, &goal.subject(), JournalEntry { at_ms: now, turn: goal.turns, note: format!("Blocked: {}", core::head(question, 160)), ..Default::default() })
}

/// A turn ended: check, judge, rule.
fn evaluate(db: &Arc<Mutex<Db>>, env: &dyn Env, goal: &Goal, snapshot: &Snapshot, task: &Task, error: Option<String>) -> Result<(), String> {
    // The check can be heavy; a held machine waits for the next tick.
    if env.machine_held() {
        return Ok(());
    }
    // A parked folder comes back (at the same path) before its check runs there.
    if task.parked.is_some() {
        let _ = super::parking::ensure_restored(db, &task.id, &AtomicBool::new(false));
    }
    let directory = task_directory(snapshot, task);
    let started = std::time::Instant::now();
    let check = (!goal.done_check.trim().is_empty()).then(|| env.run_check(&directory, goal.done_check.trim()));
    // Time spent checking counts against the wall-clock budget.
    let spent_ms = started.elapsed().as_millis() as i64;
    let mut answer = task.result.clone();
    if let Some(error) = &error {
        answer.push_str(&format!("\n\nThe turn stopped with: {error}"));
    }
    let prompt = core::judge_prompt(goal, check.as_ref(), &answer, &directory.to_string_lossy());
    let now = env.now_ms();
    let judged = env.judge(&prompt).and_then(|text| core::parse_ruling(&text, now));
    let (ruling, judge_failed) = match judged {
        Ok(ruling) => (core::enforce(goal, check.as_ref(), ruling, &|evidence| env.verify(&directory, goal.created_ms, evidence)), false),
        // Fail open: a judge that cannot answer never ends the goal, and the
        // turn cap still bounds it.
        Err(failure) => (
            core::Ruling { verdict: core::Verdict::Continue, reason: format!("The judge could not rule ({}); keep going.", core::head(&failure, 160)), evidence: String::new(), question: String::new(), default_answer: String::new(), wait: None, plan: String::new(), done: String::new(), next: String::new() },
            true,
        ),
    };
    let session = snapshot.task_sessions.get(&task.id).cloned();
    let cost = task_cost_cents(task);
    let (turn, id) = (goal.turns, goal.id.clone());
    let same_turn = move |g: &Goal| g.status == GoalStatus::Running && g.turns == turn && g.in_turn;
    let stalled = error.as_deref().is_some_and(core::is_stall);
    commit(db, &id, same_turn, |db, g| {
        g.wall_ms += spent_ms;
        if let Some(check) = &check {
            core::push_ledger(db, &g.subject(), check.clone())?;
            g.last_check = Some(check.clone());
        }
        let done_line = if ruling.done.is_empty() { core::head(&answer.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or(""), 160) } else { ruling.done.clone() };
        core::push_journal(db, &g.subject(), JournalEntry { at_ms: now, turn: g.turns, plan: ruling.plan.clone(), done: done_line, next: ruling.next.clone(), session_id: session.clone(), note: ruling.verdict.word().into() })?;
        g.verdict = ruling.verdict.word().into();
        g.reason = ruling.reason.clone();
        g.judge_failures = if judge_failed { g.judge_failures + 1 } else { 0 };
        if !stalled {
            g.stall_resumes = 0;
        }
        match ruling.verdict {
            core::Verdict::Done => {
                g.status = GoalStatus::Done;
                g.in_turn = false;
                g.wait = None;
                g.blocked = None;
                Ok(())
            }
            core::Verdict::Continue => {
                if let Some(why) = core::budget_reached(g, cost) {
                    return pause(db, g, &why, now);
                }
                let text = core::continue_prompt(g, check.as_ref(), &ruling.reason, error.as_deref());
                issue_turn(db, g, &text, true)
            }
            core::Verdict::Blocked => block(db, g, &ruling.question, &ruling.default_answer, now),
            core::Verdict::Wait => {
                g.status = GoalStatus::Waiting;
                g.in_turn = false;
                g.wait = ruling.wait.clone();
                Ok(())
            }
        }
    })?;
    Ok(())
}

fn waiting(db: &Arc<Mutex<Db>>, env: &dyn Env, goal: Goal) -> Result<(), String> {
    let Some(wait) = goal.wait.clone() else {
        commit(db, &goal.id, |g| g.status == GoalStatus::Waiting, |_, g| {
            g.status = GoalStatus::Running;
            Ok(())
        })?;
        return Ok(());
    };
    let now = env.now_ms();
    let met = if wait.ticket {
        let snapshot = store::load(&*db.lock().map_err(|_| "Goal storage unavailable")?)?;
        goal.ticket_id.as_deref().and_then(|id| snapshot.tasks.iter().find(|t| t.id == id)).is_none_or(|t| t.status != TaskStatus::AwaitingApproval)
    } else {
        wait.until_ms.is_some_and(|at| now >= at)
            || match (&wait.pull_request, wait.pr_state.as_deref()) {
                (Some(pr), Some(wanted)) => env.pr_state(pr).is_some_and(|state| {
                    let state = state.to_ascii_lowercase();
                    state == wanted || (wanted == "closed" && state == "merged")
                }),
                _ => false,
            }
            || wait.pid.is_some_and(|pid| !env.pid_alive(pid))
    };
    if met {
        commit(db, &goal.id, |g| g.status == GoalStatus::Waiting && g.wait.as_ref() == Some(&wait), |db, g| {
            g.status = GoalStatus::Running;
            g.wait = None;
            g.last_tick_ms = now;
            if !wait.ticket {
                g.reason = format!("The wait is over ({}). Carry on.", core::head(&wait.reason, 200));
                core::push_journal(db, &g.subject(), JournalEntry { at_ms: now, turn: g.turns, note: "Wait over".into(), ..Default::default() })?;
            }
            Ok(())
        })?;
    }
    Ok(())
}

fn held(db: &Arc<Mutex<Db>>, env: &dyn Env, goal: Goal) -> Result<(), String> {
    let now = env.now_ms();
    if goal.hold_until_ms.is_some_and(|at| now < at) || env.machine_held() {
        return Ok(());
    }
    commit(db, &goal.id, |g| g.status == GoalStatus::Held, |db, g| {
        g.hold_until_ms = None;
        g.stall_resumes = 0;
        core::push_journal(db, &g.subject(), JournalEntry { at_ms: now, turn: g.turns, note: "Limit reset; resumed".into(), ..Default::default() })?;
        let text = format!("The usage limit has reset. Pick up where the journal says you were.\n{}", core::context(g));
        issue_turn(db, g, &text, false)
    })?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Crash and restart
// ---------------------------------------------------------------------------

/// At daemon start (after the tickets were recovered): a goal whose turn was
/// in flight resumes from its journal. The ticket's own recovery requeued or
/// failed the run; the journal gets the note and the session that was in use,
/// and the next prompt carries the goal, journal and ledger (`prompt_context`),
/// so a runtime that cannot resume the session still starts informed.
pub(super) fn resume_sweep(db: &Arc<Mutex<Db>>, now: i64) -> Result<usize, String> {
    let _goals = GOAL_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = db.lock().map_err(|_| "Goal storage unavailable")?;
    let snapshot = store::load(&db)?;
    let mut resumed = 0;
    for mut goal in core::load_all(&db)? {
        if goal.status != GoalStatus::Running || !goal.in_turn {
            continue;
        }
        let Some(task) = goal.ticket_id.as_deref().and_then(|id| snapshot.tasks.iter().find(|t| t.id == id)) else { continue };
        if !matches!(task.status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing) {
            continue;
        }
        core::push_journal(&db, &goal.subject(), JournalEntry { at_ms: now, turn: goal.turns, note: "Resumed after a restart".into(), session_id: snapshot.task_sessions.get(&task.id).cloned(), ..Default::default() })?;
        // The clock restarts here: the time the daemon was down is not spent.
        goal.last_tick_ms = now;
        core::save(&db, &goal)?;
        resumed += 1;
    }
    Ok(resumed)
}

/// Whether an unfinished goal works this ticket.
pub(super) fn has_open_goal(db: &Db, task_id: &str) -> bool {
    core::load_all(db).is_ok_and(|goals| goals.iter().any(|g| g.ticket_id.as_deref() == Some(task_id) && !g.status.is_finished()))
}

/// What a ticket's agent is told when its ticket works a goal: the goal, the
/// journal and the ledger. Empty for any other ticket.
pub(super) fn prompt_context(db: &Db, task_id: &str) -> String {
    core::load_all(db)
        .ok()
        .and_then(|goals| goals.into_iter().find(|g| g.ticket_id.as_deref() == Some(task_id) && g.status.is_active()))
        .map(|g| core::context(&g))
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// The evidence gate for code work
// ---------------------------------------------------------------------------

pub(super) enum GateOutcome {
    /// Reaches review.
    Pass,
    /// Another turn: the builder is told to run the checks and fix.
    Retry(String),
    /// Out of extra turns: the ticket is blocked with the failing output.
    Blocked(String),
}

/// A ticket that changed code cannot reach ReadyForReview without a passing
/// check in its ledger. The ledger gets this run's command receipts (the
/// agent's real checks and the independent review's);
/// at most `GATE_EXTRA_TURNS` extra "run the checks and fix" turns, then
/// blocked. Edits that only touch prose skip it.
pub(super) fn evidence_gate(db: &Db, task_id: &str, changed: &[String], builder_receipts: &[String], review_receipts: &[String], approved: &[String], extra_turns: u32, now: i64) -> Result<GateOutcome, String> {
    // Only real checks count (`core::real_check`): the ticket's approved checks, the
    // goal's own check and recognised test, type-check and lint runners, not reads.
    let goal = core::load_all(db)?.into_iter().find(|g| g.ticket_id.as_deref() == Some(task_id) && !g.status.is_finished());
    let mut accepted = approved.to_vec();
    accepted.extend(goal.iter().map(|g| g.done_check.clone()).filter(|c| !c.trim().is_empty()));
    let mut entries = core::entries_from_receipts(builder_receipts, "agent", &accepted, now);
    entries.extend(core::entries_from_receipts(review_receipts, "review", &accepted, now));
    let subject = goal.map(|g| g.subject()).unwrap_or_else(|| task_id.to_owned());
    for entry in &entries {
        core::push_ledger(db, &subject, entry.clone())?;
    }
    Ok(match core::gate(changed, &entries, None) {
        core::Gate::Skip | core::Gate::Pass => GateOutcome::Pass,
        core::Gate::Missing(reason) if extra_turns < core::GATE_EXTRA_TURNS => GateOutcome::Retry(core::gate_instruction(&reason)),
        core::Gate::Missing(reason) => GateOutcome::Blocked(format!("{BLOCKED_PREFIX} the checks are not passing after {} extra turns. {}", core::GATE_EXTRA_TURNS, core::head(&reason, 1200))),
    })
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

impl Controller {
    pub(super) fn goal_command(&self, command: GoalCommand) -> Result<Snapshot, String> {
        let _goals = GOAL_LOCK.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let db = self.db.lock().map_err(|_| "Goal storage unavailable")?;
        let now = store::now_ms();
        let find = |id: &str| core::load(&db, id)?.ok_or_else(|| "That goal no longer exists".to_owned());
        match command {
            GoalCommand::Create { workspace_id, objective, done_check, constraints, max_turns, ticket_id, conversation_id } => {
                create(&db, &Spec { workspace_id: &workspace_id, objective: &objective, done_check: &done_check, constraints: &constraints, max_turns, ticket_id: ticket_id.as_deref(), conversation_id: conversation_id.as_deref(), by_person: true })?;
            }
            GoalCommand::Pause { id } => {
                let mut goal = find(&id)?;
                if !goal.status.is_active() {
                    return Err("That goal is not running".into());
                }
                goal.status = GoalStatus::Paused;
                goal.paused_reason = "You paused it.".into();
                core::save(&db, &goal)?;
            }
            GoalCommand::Resume { id } => {
                let mut goal = find(&id)?;
                if goal.status != GoalStatus::Paused {
                    return Err("Only a paused goal can be resumed".into());
                }
                if goal.turns >= goal.budget.max_turns && !goal.in_turn {
                    return Err("It used its turns. Extend it to keep going.".into());
                }
                goal.status = restored_status(&goal);
                goal.paused_reason.clear();
                goal.last_tick_ms = now;
                core::save(&db, &goal)?;
            }
            GoalCommand::Extend { id, turns } => {
                let mut goal = find(&id)?;
                if goal.status.is_finished() {
                    return Err("That goal is finished".into());
                }
                let turns = turns.clamp(1, 100);
                goal.budget.max_turns = (goal.budget.max_turns + turns).min(300);
                // A time or cost cap that stopped it moves too, in proportion.
                if goal.budget.max_wall_ms > 0 && goal.wall_ms >= goal.budget.max_wall_ms {
                    goal.budget.max_wall_ms = goal.wall_ms + i64::from(turns) * 30 * 60 * 1000;
                }
                if let Some(cap) = goal.budget.max_cost_cents.as_mut() {
                    *cap += (*cap / 2).max(100);
                }
                if matches!(goal.status, GoalStatus::Paused) {
                    goal.status = restored_status(&goal);
                    goal.paused_reason.clear();
                    goal.last_tick_ms = now;
                }
                if let Some(seq) = goal.extend_seq.take() {
                    let _ = crate::routines::close_goal_item(&db, seq);
                }
                core::push_journal(&db, &goal.subject(), JournalEntry { at_ms: now, turn: goal.turns, note: format!("Extended by {turns} turns"), ..Default::default() })?;
                core::save(&db, &goal)?;
            }
            GoalCommand::MarkDone { id } => {
                let mut goal = find(&id)?;
                goal.status = GoalStatus::Done;
                goal.in_turn = false;
                goal.verdict = "DONE".into();
                goal.reason = "You marked it done.".into();
                core::push_journal(&db, &goal.subject(), JournalEntry { at_ms: now, turn: goal.turns, note: "Marked done by you".into(), ..Default::default() })?;
                close_items(&db, &mut goal);
                core::save(&db, &goal)?;
            }
            GoalCommand::Stop { id } => {
                let mut goal = find(&id)?;
                goal.status = GoalStatus::Stopped;
                goal.in_turn = false;
                goal.paused_reason = "You stopped it.".into();
                core::push_journal(&db, &goal.subject(), JournalEntry { at_ms: now, turn: goal.turns, note: "Stopped by you".into(), ..Default::default() })?;
                close_items(&db, &mut goal);
                core::save(&db, &goal)?;
            }
            GoalCommand::Answer { id, text } => {
                let mut goal = find(&id)?;
                let Some(blocked) = goal.blocked.take() else { return Err("That goal is not waiting for an answer".into()) };
                let answer = if text.trim().is_empty() { blocked.default_answer.clone() } else { core::head(&text, 1000) };
                goal.status = GoalStatus::Running;
                goal.in_turn = false;
                goal.last_tick_ms = now;
                goal.reason = format!("You answered \"{}\" to: {}", answer, core::head(&blocked.question, 300));
                if let Some(seq) = blocked.inbox_seq {
                    let _ = crate::routines::close_goal_item(&db, seq);
                }
                core::push_journal(&db, &goal.subject(), JournalEntry { at_ms: now, turn: goal.turns, note: format!("Answered: {}", core::head(&answer, 120)), ..Default::default() })?;
                core::save(&db, &goal)?;
            }
        }
        store::load(&db)
    }

    /// The goal loop: resumes what a restart interrupted, then ticks.
    pub(super) fn start_goals(self: &Arc<Self>) {
        match resume_sweep(&self.db, store::now_ms()) {
            Ok(0) => {}
            Ok(n) => eprintln!("neko goals: resumed {n} from their journals"),
            Err(error) => eprintln!("neko goals: cannot resume: {error}"),
        }
        let env = Arc::new(Real { db: self.db.clone() });
        let db = self.db.clone();
        std::thread::Builder::new()
            .name("neko-goals".into())
            .spawn(move || loop {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| tick_concurrently(&db, &env))) {
                    Ok(Err(error)) => eprintln!("neko goals: {error}"),
                    Err(_) => eprintln!("neko goals: tick panicked; continuing"),
                    Ok(Ok(())) => {}
                }
                std::thread::sleep(TICK);
            })
            .map(|_| ())
            .unwrap_or_else(|error| eprintln!("neko goals: cannot start: {error}"));
    }
}

/// What a paused goal goes back to: the question it was blocked on, the wait or
/// the usage-limit hold it was in (not a fresh one), else running.
fn restored_status(goal: &Goal) -> GoalStatus {
    if goal.blocked.is_some() {
        GoalStatus::Blocked
    } else if goal.wait.is_some() {
        GoalStatus::Waiting
    } else if goal.hold_until_ms.is_some() {
        GoalStatus::Held
    } else {
        GoalStatus::Running
    }
}

/// Closes the Inbox items a finished goal no longer needs answered.
fn close_items(db: &Db, goal: &mut Goal) {
    for seq in [goal.extend_seq.take(), goal.blocked.take().and_then(|b| b.inbox_seq)].into_iter().flatten() {
        let _ = crate::routines::close_goal_item(db, seq);
    }
}

#[cfg(test)]
#[path = "goals_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "goals_chat_tests.rs"]
mod chat_tests;

#[cfg(test)]
#[path = "goals_more_tests.rs"]
mod more_tests;
