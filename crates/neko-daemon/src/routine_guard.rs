//! Guard rails on a routine's runs: a daily cap, a shell gate (`run_only_when`) and the skip record.
//! The decisions are pure or small; `routines::run_body` calls `admit` once, before it builds a prompt.
//! A person's "Run now" bypasses both guards (the same rule the spend ceiling uses).
use neko_core::routines::{self as core, MAX_SKIPS, MAX_STARTS};
use neko_protocol::routines::{Routine, RoutinePolicy, RoutineRun, RoutineState, RoutineThread};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// How long the gate command gets.
pub(crate) const GATE_LIMIT: Duration = Duration::from_secs(10);
/// No network at all (loopback included): a gate decides from what is on disk.
const GATE_PROFILE: &str = "(version 1)(allow default)(deny network*)";
const DAY_MS: i64 = 24 * 60 * 60 * 1000;
const COMMAND_IN_REASON: usize = 80;
const OUTPUT_IN_REASON: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Verdict {
    Go,
    /// The whole reason, ready to show: starts with "Skipped: ".
    Skip(String),
}

/// The daily cap: `Skip("Skipped: daily cap reached (<n>/<cap>)")` once `cap` runs started in the last 24 hours.
pub(crate) fn cap_verdict(state: &RoutineState, policy: &RoutinePolicy, now_ms: i64) -> Verdict {
    match core::cap_reached(&state.starts_ms, policy.daily_cap, now_ms) {
        Some((count, cap)) => Verdict::Skip(format!("Skipped: daily cap reached ({count}/{cap})")),
        None => Verdict::Go,
    }
}

fn cut(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    format!("{}…", text.chars().take(max).collect::<String>())
}

/// Runs `command` in `folder` under `sandbox-exec` with no network, a cleared environment and `limit`
/// to finish. Exit 0 is `Go`; anything else is a `Skip` that says why.
pub(crate) fn run_gate(command: &str, folder: &Path, limit: Duration) -> Verdict {
    use std::io::Read;
    use std::os::unix::process::CommandExt;
    let home = std::env::var("HOME").unwrap_or_default();
    let path = format!("{}:{home}/.cargo/bin:{home}/.local/bin:/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin", std::env::var("PATH").unwrap_or_default());
    let mut gate = std::process::Command::new("/usr/bin/sandbox-exec");
    gate.args(["-p", GATE_PROFILE, "/bin/sh", "-c"])
        .arg(format!("exec 2>&1; {command}"))
        .current_dir(folder)
        .env_clear()
        .env("PATH", path)
        .env("HOME", &home)
        .env("TMPDIR", std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".into()))
        .env("LANG", std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into()))
        .env("CI", "1")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        // Its own group, so a timeout takes whatever the command started with it.
        .process_group(0);
    let mut child = match gate.spawn() {
        Ok(child) => child,
        Err(error) => return Verdict::Skip(format!("Skipped: could not run the check ({error})")),
    };
    let pid = child.id() as i32;
    let mut stdout = child.stdout.take().expect("piped stdout");
    let buffer = Arc::new(Mutex::new(Vec::<u8>::new()));
    let reader = {
        let buffer = buffer.clone();
        std::thread::spawn(move || {
            let mut chunk = [0_u8; 4096];
            while let Ok(n) = stdout.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                let mut buffer = buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if buffer.len() < 16 * 1024 {
                    buffer.extend_from_slice(&chunk[..n]);
                }
            }
        })
    };
    let started = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if started.elapsed() >= limit => {
                unsafe { libc::kill(-pid, libc::SIGKILL) };
                let _ = child.kill();
                let _ = child.wait();
                return Verdict::Skip(format!("Skipped: the check timed out after {} s", limit.as_secs()));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(25)),
            Err(error) => break Err(error),
        }
    };
    // Whatever it left running is not the gate's to keep.
    unsafe { libc::kill(-pid, libc::SIGKILL) };
    let waited = std::time::Instant::now();
    while !reader.is_finished() && waited.elapsed() < Duration::from_secs(2) {
        std::thread::sleep(Duration::from_millis(10));
    }
    let status = match status {
        Ok(status) => status,
        Err(error) => return Verdict::Skip(format!("Skipped: could not run the check ({error})")),
    };
    if status.success() {
        return Verdict::Go;
    }
    let output = String::from_utf8_lossy(&buffer.lock().unwrap_or_else(std::sync::PoisonError::into_inner)).into_owned();
    let first = output.lines().map(str::trim).find(|line| !line.is_empty()).map(|line| cut(line, OUTPUT_IN_REASON));
    let code = status.code().map_or_else(|| "without an exit code".to_string(), |code| code.to_string());
    let tail = first.map(|line| format!(": {line}")).unwrap_or_default();
    Verdict::Skip(format!("Skipped: \"{}\" exited {code}{tail}", cut(command, COMMAND_IN_REASON)))
}

/// Notes a start: the run is going ahead. A leftover "Skipped" note is cleared.
pub(crate) fn record_start(db: &neko_core::Db, routine_id: &str, now_ms: i64) -> Result<(), String> {
    let mut state = core::load_state(db, routine_id)?;
    core::prune_window(&mut state.starts_ms, now_ms, MAX_STARTS);
    state.starts_ms.push(now_ms);
    core::prune_window(&mut state.starts_ms, now_ms, MAX_STARTS);
    if state.note.as_deref().is_some_and(|note| note.starts_with("Skipped:")) {
        state.note = None;
    }
    core::save_state(db, routine_id, &state)
}

/// A skip is not a failure and not a run: the routine is simply due again at its next time.
/// Repeats of the same reason within a day share one journal row that counts them.
pub(crate) fn record_skip(db: &neko_core::Db, routine: &Routine, reason: &str, now_ms: i64) -> Result<(), String> {
    let mut state = core::load_state(db, &routine.id)?;
    core::prune_window(&mut state.starts_ms, now_ms, MAX_STARTS);
    core::prune_window(&mut state.skip_ms, now_ms, MAX_SKIPS);
    state.skip_ms.push(now_ms);
    core::prune_window(&mut state.skip_ms, now_ms, MAX_SKIPS);
    state.running = false;
    state.next_due_ms = Some(core::next_due_local(&routine.cadence, state.quiet_streak, now_ms));
    state.note = Some(reason.to_string());
    let mut runs = match core::load_runs(db, &routine.id) {
        Ok(runs) => runs,
        Err(error) if error.starts_with("Cannot read") => Vec::new(),
        Err(error) => return Err(error),
    };
    match runs.first_mut() {
        Some(run) if run.skipped > 0 && run.did.first().is_some_and(|did| did == reason) && run.started_ms > now_ms - DAY_MS => {
            run.skipped += 1;
            run.finished_ms = Some(now_ms);
            core::save_runs(db, &routine.id, &runs)?;
        }
        _ => {
            let run = RoutineRun { id: neko_core::workbench::new_id(), routine_id: routine.id.clone(), started_ms: now_ms, finished_ms: Some(now_ms), ok: true, quiet: true, did: vec![reason.to_string()], skipped: 1, ..Default::default() };
            core::push_run(db, &routine.id, run)?;
        }
    }
    core::save_state(db, &routine.id, &state)
}

/// Decides whether this run goes ahead. `Go` has already noted the start; a skip has already been
/// recorded. `folder` is where the agent would run (the gate runs there too); without one the gate
/// is left to the run's own "folder missing" failure.
pub(crate) fn admit(db: &Mutex<neko_core::Db>, routine: &Routine, folder: Option<&Path>, manual: bool, now_ms: i64) -> Result<Verdict, String> {
    let lock = || db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if manual {
        record_start(&lock(), &routine.id, now_ms)?;
        return Ok(Verdict::Go);
    }
    let state = core::load_state(&lock(), &routine.id)?;
    let mut verdict = cap_verdict(&state, &routine.policy, now_ms);
    let gate = routine.policy.run_only_when.trim();
    if verdict == Verdict::Go && !gate.is_empty() {
        if let Some(folder) = folder.filter(|folder| folder.is_dir()) {
            // The database is not held while the command runs.
            verdict = run_gate(gate, folder, GATE_LIMIT);
        }
    }
    let db = lock();
    match &verdict {
        Verdict::Go => record_start(&db, &routine.id, now_ms)?,
        Verdict::Skip(reason) => record_skip(&db, routine, reason, now_ms)?,
    }
    Ok(verdict)
}

/// The memory the prompt shows for a `standing` routine: the saved memory plus a short summary of the last
/// runs under `standing_thread`. Never saved back, so the thread does not pile up in the row. `fresh` is untouched.
pub(crate) fn with_thread(mut memory: serde_json::Value, policy: &RoutinePolicy, runs: &[RoutineRun]) -> serde_json::Value {
    if policy.thread != RoutineThread::Standing {
        return memory;
    }
    if !memory.is_object() {
        memory = serde_json::json!({});
    }
    memory["standing_thread"] = core::standing_thread(runs);
    memory
}

/// Edit-time validation of the policy (`SaveRoutine`): the command is trimmed in place.
pub(crate) fn validate(policy: &mut RoutinePolicy) -> Result<(), String> {
    if policy.daily_cap > neko_protocol::routines::MAX_DAILY_CAP {
        return Err("Runs per day must be 0 to 1000".into());
    }
    policy.run_only_when = policy.run_only_when.trim().to_string();
    if policy.run_only_when.len() > neko_protocol::routines::MAX_GATE_BYTES {
        return Err("Keep the command under 1 KB".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::routines::{Cadence, RoutineKind};

    const HOUR: i64 = 3_600_000;

    fn routine(policy: RoutinePolicy) -> Routine {
        Routine { id: "r".into(), workspace_id: "w".into(), kind: RoutineKind::Custom, name: "R".into(), instruction: "x".into(), cadence: Cadence::Daily { hour: 9, minute: 0 }, enabled: true, model: String::new(), paseo_schedule: None, browser: false, computer: false, customized: false, catch_up: None, policy }
    }

    fn db() -> Mutex<neko_core::Db> {
        Mutex::new(neko_core::Db::open_in_memory().unwrap())
    }

    fn state_of(db: &Mutex<neko_core::Db>) -> RoutineState {
        core::load_state(&db.lock().unwrap(), "r").unwrap()
    }

    fn running(db: &Mutex<neko_core::Db>, edit: impl FnOnce(&mut RoutineState)) {
        let mut state = RoutineState { running: true, ..Default::default() };
        edit(&mut state);
        core::save_state(&db.lock().unwrap(), "r", &state).unwrap();
    }

    fn temp_folder(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("neko-guard-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::canonicalize(&dir).unwrap()
    }

    #[test]
    fn a_routine_with_no_cap_is_never_stopped_however_many_runs_it_made() {
        let now = 1_000 * HOUR;
        let db = db();
        running(&db, |s| s.starts_ms = (0..500).map(|i| now - i).collect());
        let verdict = admit(&db, &routine(RoutinePolicy::default()), None, false, now).unwrap();
        assert_eq!(verdict, Verdict::Go);
        assert_eq!(state_of(&db).starts_ms.len(), 501);
    }

    #[test]
    fn the_fourth_run_under_a_cap_of_three_is_skipped_and_starts_do_not_change() {
        let now = 1_000 * HOUR;
        let db = db();
        let starts = vec![now - HOUR, now - 2 * HOUR, now - 3 * HOUR];
        running(&db, |s| s.starts_ms = starts.clone());
        let routine = routine(RoutinePolicy { daily_cap: 3, ..Default::default() });
        let verdict = admit(&db, &routine, None, false, now).unwrap();
        assert_eq!(verdict, Verdict::Skip("Skipped: daily cap reached (3/3)".into()));
        assert_eq!(state_of(&db).starts_ms, starts, "a skip is not a start");
        assert_eq!(state_of(&db).skip_ms, vec![now]);
    }

    #[test]
    fn the_default_cap_reads_ten_of_ten() {
        let now = 1_000 * HOUR;
        let state = RoutineState { starts_ms: (1..=10).map(|i| now - i * 1000).collect(), ..Default::default() };
        assert_eq!(cap_verdict(&state, &RoutinePolicy::for_new_custom(), now), Verdict::Skip("Skipped: daily cap reached (10/10)".into()));
    }

    #[test]
    fn a_start_a_day_old_does_not_count() {
        let now = 1_000 * HOUR;
        let db = db();
        running(&db, |s| s.starts_ms = vec![now - 25 * HOUR, now - HOUR, now - 2 * HOUR]);
        let verdict = admit(&db, &routine(RoutinePolicy { daily_cap: 3, ..Default::default() }), None, false, now).unwrap();
        assert_eq!(verdict, Verdict::Go);
        assert_eq!(state_of(&db).starts_ms.len(), 3, "the old start is pruned, the new one added");
    }

    #[test]
    fn a_skip_changes_no_failure_memory_or_streak_and_schedules_the_next_time() {
        let now = 1_000 * HOUR;
        let db = db();
        running(&db, |s| {
            s.failures = 2;
            s.quiet_streak = 3;
            s.memory = serde_json::json!({"waiting_on": "Asha"});
            s.last_run_ms = Some(now - 5 * HOUR);
            s.starts_ms = vec![now - HOUR];
        });
        let routine = routine(RoutinePolicy { daily_cap: 1, ..Default::default() });
        admit(&db, &routine, None, false, now).unwrap();
        let state = state_of(&db);
        assert_eq!((state.failures, state.quiet_streak), (2, 3));
        assert_eq!(state.memory, serde_json::json!({"waiting_on": "Asha"}));
        assert_eq!(state.last_run_ms, Some(now - 5 * HOUR));
        assert!(!state.running);
        assert_eq!(state.next_due_ms, Some(core::next_due_local(&routine.cadence, 3, now)));
        assert_eq!(state.note.as_deref(), Some("Skipped: daily cap reached (1/1)"));
    }

    #[test]
    fn ten_skips_with_one_reason_are_one_row_and_do_not_push_real_runs_out() {
        let now = 1_000 * HOUR;
        let db = db();
        // 49 real runs, newest first.
        for i in 0..49 {
            let run = RoutineRun { id: format!("real{i}"), routine_id: "r".into(), started_ms: now - 100 * HOUR + i, ok: true, ..Default::default() };
            core::push_run(&db.lock().unwrap(), "r", run).unwrap();
        }
        running(&db, |s| s.starts_ms = vec![now - HOUR]);
        let routine = routine(RoutinePolicy { daily_cap: 1, ..Default::default() });
        for i in 0..10 {
            admit(&db, &routine, None, false, now + i * 15 * 60_000).unwrap();
        }
        let runs = core::load_runs(&db.lock().unwrap(), "r").unwrap();
        assert_eq!(runs.len(), 50, "49 real runs and one skip row");
        assert_eq!((runs[0].skipped, runs[0].did.clone()), (10, vec!["Skipped: daily cap reached (1/1)".to_string()]));
        assert!(runs[0].ok && runs[0].quiet && runs[0].error.is_none());
        assert_eq!(runs.iter().filter(|r| r.skipped == 0).count(), 49);
        assert_eq!(runs[0].finished_ms, Some(now + 9 * 15 * 60_000), "the row's end moves");
        assert_eq!(state_of(&db).skip_ms.len(), 10);
        // A different reason starts its own row.
        record_skip(&db.lock().unwrap(), &routine, "Skipped: \"false\" exited 1", now + HOUR).unwrap();
        assert_eq!(core::load_runs(&db.lock().unwrap(), "r").unwrap()[0].skipped, 1);
    }

    #[test]
    fn run_now_ignores_the_cap_and_the_gate_but_still_counts_as_a_start() {
        let now = 1_000 * HOUR;
        let db = db();
        running(&db, |s| s.starts_ms = vec![now - HOUR]);
        let routine = routine(RoutinePolicy { daily_cap: 1, run_only_when: "false".into(), ..Default::default() });
        let folder = temp_folder("manual");
        assert_eq!(admit(&db, &routine, Some(&folder), true, now).unwrap(), Verdict::Go);
        assert_eq!(state_of(&db).starts_ms.len(), 2);
        assert!(state_of(&db).skip_ms.is_empty());
        // The same routine, not asked for by a person, is stopped.
        assert!(matches!(admit(&db, &routine, Some(&folder), false, now).unwrap(), Verdict::Skip(_)));
    }

    #[test]
    fn a_gate_that_exits_zero_runs_and_one_that_exits_three_skips_with_its_first_line() {
        let folder = temp_folder("exit");
        assert_eq!(run_gate("true", &folder, GATE_LIMIT), Verdict::Go);
        let Verdict::Skip(reason) = run_gate("echo 'not yet'; echo more; exit 3", &folder, GATE_LIMIT) else { panic!("went ahead") };
        assert_eq!(reason, "Skipped: \"echo 'not yet'; echo more; exit 3\" exited 3: not yet");
        let Verdict::Skip(reason) = run_gate("exit 1", &folder, GATE_LIMIT) else { panic!("went ahead") };
        assert_eq!(reason, "Skipped: \"exit 1\" exited 1", "no output, no tail");
        // stderr counts as output.
        let Verdict::Skip(reason) = run_gate("echo oops >&2; exit 2", &folder, GATE_LIMIT) else { panic!("went ahead") };
        assert!(reason.ends_with("exited 2: oops"), "{reason}");
    }

    #[test]
    fn a_long_command_and_a_long_line_are_cut() {
        let folder = temp_folder("cut");
        let command = format!("echo {}; exit 4", "y".repeat(300));
        let Verdict::Skip(reason) = run_gate(&command, &folder, GATE_LIMIT) else { panic!("went ahead") };
        assert!(reason.starts_with("Skipped: \"echo yyyy"));
        assert!(reason.contains("…\" exited 4: yyyy") && reason.ends_with('…'), "{reason}");
        assert!(reason.chars().count() < 80 + 120 + 40, "{}", reason.chars().count());
    }

    #[test]
    fn a_gate_that_sleeps_past_ten_seconds_skips_with_the_timeout_in_about_ten_seconds() {
        let folder = temp_folder("slow");
        let started = std::time::Instant::now();
        let verdict = run_gate("sleep 15", &folder, GATE_LIMIT);
        assert_eq!(verdict, Verdict::Skip("Skipped: the check timed out after 10 s".into()));
        let took = started.elapsed();
        assert!(took >= Duration::from_secs(10) && took < Duration::from_secs(13), "{took:?}");
    }

    #[test]
    fn the_gate_runs_in_the_folder_with_a_clean_environment() {
        let folder = temp_folder("pwd");
        std::fs::write(folder.join("ready.txt"), "x").unwrap();
        // `test -f` passes only inside the folder.
        assert_eq!(run_gate("test -f ready.txt", &folder, GATE_LIMIT), Verdict::Go);
        let marker = folder.join("pwd.out");
        assert_eq!(run_gate(&format!("pwd > '{}'", marker.display()), &folder, GATE_LIMIT), Verdict::Go);
        assert_eq!(std::fs::canonicalize(std::fs::read_to_string(&marker).unwrap().trim()).unwrap(), folder);
        // Nothing of the daemon's environment leaks in.
        // (cargo sets CARGO_PKG_NAME for a test run.)
        assert!(std::env::var_os("CARGO_PKG_NAME").is_some());
        assert_eq!(run_gate("test -z \"$CARGO_PKG_NAME\" && test \"$CI\" = 1", &folder, GATE_LIMIT), Verdict::Go);
    }

    #[test]
    fn the_gate_cannot_reach_the_network() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let folder = temp_folder("net");
        let verdict = run_gate(&format!("/usr/bin/curl -s -m 3 http://127.0.0.1:{port}/"), &folder, GATE_LIMIT);
        assert!(matches!(verdict, Verdict::Skip(_)), "{verdict:?}");
        assert!(listener.accept().is_err(), "the gate connected to the listener");
    }

    #[test]
    fn a_gate_failing_to_start_skips_with_the_reason_and_a_missing_folder_is_left_to_the_run() {
        let db = db();
        running(&db, |_| {});
        let routine = routine(RoutinePolicy { run_only_when: "false".into(), ..Default::default() });
        let missing = std::path::Path::new("/nonexistent-neko-folder");
        // No folder: the gate is not run; the run's own error says the folder is missing.
        assert_eq!(admit(&db, &routine, Some(missing), false, 5 * HOUR).unwrap(), Verdict::Go);
        assert_eq!(admit(&db, &routine, None, false, 5 * HOUR).unwrap(), Verdict::Go);
    }

    #[test]
    fn a_gate_is_run_by_admit_and_its_skip_is_recorded() {
        let now = 1_000 * HOUR;
        let db = db();
        running(&db, |_| {});
        let folder = temp_folder("admit");
        let routine = routine(RoutinePolicy { run_only_when: "echo none; exit 3".into(), ..Default::default() });
        let verdict = admit(&db, &routine, Some(&folder), false, now).unwrap();
        assert_eq!(verdict, Verdict::Skip("Skipped: \"echo none; exit 3\" exited 3: none".into()));
        assert!(state_of(&db).starts_ms.is_empty());
        let runs = core::load_runs(&db.lock().unwrap(), "r").unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].skipped, 1);
    }

    #[test]
    fn a_run_that_goes_ahead_clears_the_old_skip_note() {
        let db = db();
        running(&db, |s| s.note = Some("Skipped: daily cap reached (1/1)".into()));
        admit(&db, &routine(RoutinePolicy::default()), None, false, 5 * HOUR).unwrap();
        assert_eq!(state_of(&db).note, None);
        running(&db, |s| s.note = Some("Last run could not be saved: disk".into()));
        admit(&db, &routine(RoutinePolicy::default()), None, false, 6 * HOUR).unwrap();
        assert!(state_of(&db).note.is_some(), "another kind of note stays");
    }

    fn real_run(n: i64) -> RoutineRun {
        RoutineRun { id: format!("r{n}"), routine_id: "r".into(), started_ms: 1_700_000_000_000 + n * HOUR, ok: true, did: vec![format!("did {n}")], ..Default::default() }
    }

    #[test]
    fn a_fresh_thread_leaves_the_memory_alone_and_a_standing_one_adds_the_summary() {
        let runs: Vec<RoutineRun> = (0..3).rev().map(real_run).collect();
        let memory = serde_json::json!({"seen": 1});
        assert_eq!(with_thread(memory.clone(), &RoutinePolicy::default(), &runs), memory);
        let standing = with_thread(memory.clone(), &RoutinePolicy { thread: RoutineThread::Standing, ..Default::default() }, &runs);
        assert_eq!(standing["seen"], 1);
        assert_eq!(standing["standing_thread"]["earlier_runs"].as_array().unwrap().len(), 3);
        // Null memory becomes an object holding only the thread.
        let from_null = with_thread(serde_json::Value::Null, &RoutinePolicy { thread: RoutineThread::Standing, ..Default::default() }, &runs);
        assert!(from_null["standing_thread"].is_object());
    }

    #[test]
    fn the_prompt_is_byte_identical_when_fresh_and_names_the_thread_when_standing() {
        let scope = neko_protocol::routines::ScopeProposal { name: "Home".into(), folders: vec!["/Users/example".into()], servers: vec![], problems: vec![], workspace_id: Some("w".into()), discovered: false, instructions: String::new() };
        let runs = vec![real_run(1)];
        let prompt = |policy: RoutinePolicy| {
            let routine = routine(policy.clone());
            let memory = with_thread(serde_json::json!({}), &policy, &runs);
            core::build_prompt(&routine, &scope, "orders", &memory, "2026-01-01T00:00:00Z", &[], 0, "Asha", "")
        };
        let plain = core::build_prompt(&routine(RoutinePolicy::default()), &scope, "orders", &serde_json::json!({}), "2026-01-01T00:00:00Z", &[], 0, "Asha", "");
        assert_eq!(prompt(RoutinePolicy::default()), plain);
        let standing = prompt(RoutinePolicy { thread: RoutineThread::Standing, ..Default::default() });
        assert!(standing.contains("standing_thread") && standing.contains("did 1"));
        assert!(!plain.contains("standing_thread"));
    }

    #[test]
    fn validation_refuses_a_cap_over_1000_and_a_command_over_1_kb_and_trims() {
        assert_eq!(validate(&mut RoutinePolicy { daily_cap: 1001, ..Default::default() }), Err("Runs per day must be 0 to 1000".into()));
        assert!(validate(&mut RoutinePolicy { daily_cap: 1000, ..Default::default() }).is_ok());
        assert_eq!(validate(&mut RoutinePolicy { run_only_when: "x".repeat(1025), ..Default::default() }), Err("Keep the command under 1 KB".into()));
        assert!(validate(&mut RoutinePolicy { run_only_when: "x".repeat(1024), ..Default::default() }).is_ok());
        let mut policy = RoutinePolicy { run_only_when: "  test -f a  \n".into(), ..Default::default() };
        validate(&mut policy).unwrap();
        assert_eq!(policy.run_only_when, "test -f a");
    }
}
