//! Neko as the Mac's steward: what the machine can take right now.
//!
//! This Mac has restarted under agent load (Oct 11, load 240, 52 GB of node
//! builds) and run out of memory again at night (Oct 17). Neko reads the
//! machine every 15 seconds — load, memory pressure, compressed memory and
//! swap, free disk, kernel_task (high when macOS throttles to cool down) and
//! memory per kind of agent — and turns that into one limit on new work:
//! hold everything, or cap how many agents run at once. Running work is never
//! touched; only new work waits.
//!
//! The same reading answers `neko-daemon --machine-check`, so agents outside
//! Neko (a Claude Code hook) are held to the same limits.

use neko_protocol::workbench::{AgentMemory, MachineHealth};
use std::process::Command;
use std::sync::Mutex;

/// Below this much free disk, no new agent work starts: agents that cannot
/// write die silently and their output is lost.
pub const DISK_FLOOR_GB: f64 = 15.0;
/// The one-minute load at which nothing new starts, day or night.
pub const LOAD_CEILING: f64 = 20.0;
/// At night, heavy work waits while the load is above this.
pub const NIGHT_LOAD_CEILING: f64 = 12.0;
/// Night hours (local time, start inclusive, end exclusive).
pub const NIGHT: (u32, u32) = (1, 6);
/// A reading older than this is ignored rather than trusted.
pub const STALE_MS: i64 = 2 * 60 * 1000;

static LATEST: Mutex<Option<MachineHealth>> = Mutex::new(None);

/// The last reading, if it is fresh.
pub fn latest() -> Option<MachineHealth> {
    let health = LATEST.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()?;
    (crate::now_unix_ms() - health.at_ms <= STALE_MS).then_some(health)
}

pub fn record(health: MachineHealth) {
    *LATEST.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(health);
}

fn run(program: &str, args: &[&str]) -> String {
    Command::new(program).args(args).output().ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default()
}

fn finite(x: f64) -> f64 { if x.is_finite() { x } else { 0.0 } }

/// `{ 7.91 8.51 7.77 }` → 7.91
pub fn parse_load(text: &str) -> f64 {
    finite(text.trim().trim_matches(|c| c == '{' || c == '}').split_whitespace().next().and_then(|v| v.parse().ok()).unwrap_or(0.0))
}

/// `total = 5120.00M  used = 3850.25M ...` → (used GB, total GB)
pub fn parse_swap(text: &str) -> (f64, f64) {
    let value = |key: &str| -> f64 {
        text.split(key).nth(1).and_then(|rest| rest.split_whitespace().next())
            .and_then(|v| {
                let (number, unit) = v.split_at(v.len().saturating_sub(1));
                let n: f64 = number.parse().ok()?;
                Some(match unit { "G" => n, "M" => n / 1024.0, "K" => n / 1024.0 / 1024.0, _ => n / 1024.0 / 1024.0 / 1024.0 })
            })
            .unwrap_or(0.0)
    };
    (finite(value("used = ")), finite(value("total = ")))
}

/// Compressed memory in GB from `vm_stat`.
pub fn parse_compressed(vm_stat: &str) -> f64 {
    let page: f64 = vm_stat.split("page size of ").nth(1).and_then(|r| r.split_whitespace().next()).and_then(|v| v.parse().ok()).unwrap_or(16384.0);
    let pages: f64 = vm_stat.lines().find(|l| l.starts_with("Pages occupied by compressor"))
        .and_then(|l| l.split(':').nth(1)).map(|v| v.trim().trim_end_matches('.')).and_then(|v| v.parse().ok()).unwrap_or(0.0);
    finite(pages * page / 1024.0 / 1024.0 / 1024.0)
}

/// Free GB from `df -k /`.
pub fn parse_free_disk(df: &str) -> f64 {
    finite(df.lines().nth(1).and_then(|l| l.split_whitespace().nth(3)).and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.0) / 1024.0 / 1024.0)
}

pub fn pressure_word(level: &str) -> &'static str {
    match level.trim() {
        "4" => "critical",
        "2" => "warning",
        _ => "normal",
    }
}

/// Which kind of agent a process belongs to, if any.
fn agent_kind(command: &str) -> Option<&'static str> {
    let name = command.rsplit('/').next().unwrap_or(command);
    Some(match name {
        "claude" => "Claude Code sessions",
        "codex" => "Codex sessions",
        "node" | "esbuild" | "tsc" | "vite" | "turbo" | "jest" => "Node builds, tests and dev servers",
        n if n.starts_with("Paseo") => "Paseo",
        n if n.starts_with("python") || n == "Python" => "Python (MCP helpers, servers)",
        "neko" | "neko-daemon" => "Neko",
        _ => return None,
    })
}

/// Memory per kind of agent from `ps -axo rss=,comm=`, largest first.
pub fn parse_agents(ps: &str) -> Vec<AgentMemory> {
    let mut groups: std::collections::BTreeMap<&str, (u32, u64)> = Default::default();
    for line in ps.lines() {
        let line = line.trim();
        let Some((rss, command)) = line.split_once(char::is_whitespace) else { continue };
        let (Ok(kb), Some(kind)) = (rss.parse::<u64>(), agent_kind(command.trim())) else { continue };
        let entry = groups.entry(kind).or_default();
        entry.0 += 1;
        entry.1 += kb;
    }
    let mut agents: Vec<AgentMemory> = groups.into_iter()
        .map(|(name, (processes, kb))| AgentMemory { name: name.into(), processes, mb: kb / 1024 }).collect();
    agents.sort_by_key(|a| std::cmp::Reverse(a.mb));
    agents
}

/// `top` sample of kernel_task's CPU share.
pub fn parse_kernel_task(top: &str) -> f64 {
    finite(top.lines().rev().find(|l| l.contains("kernel_task"))
        .and_then(|l| l.split_whitespace().last()).and_then(|v| v.parse().ok()).unwrap_or(0.0))
}

pub fn local_hour(unix_ms: i64) -> u32 {
    let seconds = (unix_ms / 1000) as libc::time_t;
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    // SAFETY: localtime_r writes only into the provided tm.
    unsafe { libc::localtime_r(&seconds, &mut tm) };
    tm.tm_hour.clamp(0, 23) as u32
}

/// Asks the shared `~/.agents/bin/machine-ok` (every agent on this Mac obeys
/// it): exit 1 means wait, and its `WAIT <reason> (...)` line says why. A Mac
/// without the script, or a script that fails to run, never holds work.
fn comfort_wait() -> Option<String> {
    let script = std::path::PathBuf::from(std::env::var_os("HOME")?).join(".agents/bin/machine-ok");
    if !script.is_file() { return None; }
    let output = Command::new(&script).output().ok()?;
    if output.status.code() != Some(1) { return None; }
    parse_comfort_wait(&String::from_utf8_lossy(&output.stdout))
}

/// `WAIT owner is using the Mac with load 14.99 (thermal=…)` → the reason.
pub fn parse_comfort_wait(line: &str) -> Option<String> {
    let rest = line.lines().next()?.trim().strip_prefix("WAIT")?.trim();
    let reason = rest.split(" (").next().unwrap_or(rest).trim();
    Some(if reason.is_empty() { "the shared machine check says wait".into() } else { reason.into() })
}

/// Night, holding and cap, from the readings. Pure, so it is tested alone.
pub fn apply_policy(health: &mut MachineHealth, hour: u32) {
    health.night = hour >= NIGHT.0 && hour < NIGHT.1;
    health.holding = if health.free_disk_gb > 0.0 && health.free_disk_gb < DISK_FLOOR_GB {
        Some(format!("Only {:.0} GB of disk is free. New work waits until there is {:.0} GB, so agents don't fail mid-write.", health.free_disk_gb, DISK_FLOOR_GB))
    } else if health.pressure == "critical" {
        Some("Memory is critically low. New work waits until macOS has room again.".into())
    } else if health.load >= LOAD_CEILING {
        Some(format!("The Mac is overloaded (load {:.0}). New work waits until it drops below {:.0}.", health.load, LOAD_CEILING))
    } else if health.night && health.load > NIGHT_LOAD_CEILING {
        Some(format!("Night hours, and the load is {:.0}. New work waits until it drops to {:.0}.", health.load, NIGHT_LOAD_CEILING))
    } else if let Some(reason) = &health.comfort_wait {
        Some(format!("Keeping the Mac cool ({reason}). New work waits until it settles."))
    } else {
        None
    };
    health.cap = (health.night || health.pressure == "warning").then_some(1);
}

/// Reads the Mac now. `with_kernel_task` adds a ~2 s `top` sample.
pub fn sense(with_kernel_task: bool) -> MachineHealth {
    let (swap_used_gb, swap_total_gb) = parse_swap(&run("/usr/sbin/sysctl", &["-n", "vm.swapusage"]));
    let mut health = MachineHealth {
        at_ms: crate::now_unix_ms(),
        load: parse_load(&run("/usr/sbin/sysctl", &["-n", "vm.loadavg"])),
        cores: run("/usr/sbin/sysctl", &["-n", "hw.ncpu"]).trim().parse().unwrap_or(0),
        free_disk_gb: parse_free_disk(&run("/bin/df", &["-k", "/"])),
        compressed_gb: parse_compressed(&run("/usr/bin/vm_stat", &[])),
        swap_used_gb,
        swap_total_gb,
        pressure: pressure_word(&run("/usr/sbin/sysctl", &["-n", "kern.memorystatus_vm_pressure_level"])).into(),
        kernel_task_cpu: if with_kernel_task {
            parse_kernel_task(&run("/usr/bin/top", &["-l", "2", "-s", "1", "-pid", "0", "-stats", "pid,command,cpu"]))
        } else {
            latest().map(|h| h.kernel_task_cpu).unwrap_or(0.0)
        },
        agents: parse_agents(&run("/bin/ps", &["-axo", "rss=,comm="])),
        comfort_wait: comfort_wait(),
        ..Default::default()
    };
    let hour = local_hour(health.at_ms);
    apply_policy(&mut health, hour);
    health
}

/// Commands that start heavy or disk-hungry work, for the hook other agents
/// call (`neko-daemon --machine-check --hook`).
pub fn heavy_command(command: &str) -> bool {
    [
        "paseo run", "treehouse get", "fm-spawn", "npm install", "npm ci", "pnpm install", "pnpm i ", "yarn install",
        "docker build", "xcodebuild", "cargo build", "cargo test", "swift build", "turbo run", "yarn build",
        "pnpm build", "npm run build", "yarn test", "pnpm test", "npx jest", "vitest run",
    ].iter().any(|p| command.contains(p)) || command.trim_end().ends_with("pnpm i")
}

/// The "This machine" part of a rulebook, for agents outside Neko.
pub fn machine_rules(rulebook: &str) -> String {
    let start = rulebook.find("## This machine").map(|i| i + "## This machine".len());
    let section = start.map(|i| {
        let rest = &rulebook[i..];
        rest[..rest.find("\n## ").unwrap_or(rest.len())].trim().to_owned()
    });
    let mut text = section.unwrap_or_else(|| rulebook.trim().to_owned());
    text.push_str(&format!("\nTest runners use at most {TEST_WORKERS} workers (jest --maxWorkers={TEST_WORKERS}, vitest --maxWorkers={TEST_WORKERS}, pytest -n {TEST_WORKERS}, turbo --concurrency={TEST_WORKERS})."));
    text
}

/// Environment that caps test runners in any process Neko starts, for the
/// runners that read one (Vitest's pools, pytest-xdist's `-n auto`). Jest and
/// Turbo read none, so the hook and the rulebook cap those instead.
pub const TEST_WORKER_ENV: [(&str, &str); 3] = [
    ("VITEST_MAX_THREADS", "2"),
    ("VITEST_MAX_FORKS", "2"),
    ("PYTEST_XDIST_AUTO_NUM_WORKERS", "2"),
];

/// Processes that build or test, by their command line.
fn heavy_process(args: &str) -> bool {
    let program = args.split_whitespace().next().unwrap_or("").rsplit('/').next().unwrap_or("");
    matches!(program, "esbuild" | "tsc" | "xcodebuild" | "swift-frontend" | "rustc" | "clang" | "webpack")
        || (program == "node" && ["jest", "vitest", "tsc", "esbuild", "turbo", "webpack", "eslint", "next build"].iter().any(|k| args.contains(k)))
}

/// At night with the load over the night ceiling, lowers the priority of
/// build and test processes so the Mac keeps up. Nothing is stopped; the
/// work just yields to everything else. Returns how many were slowed.
pub fn slow_heavy_jobs(health: &MachineHealth) -> u32 {
    if !(health.night && health.load > NIGHT_LOAD_CEILING) { return 0; }
    let user = run("/usr/bin/id", &["-un"]).trim().to_owned();
    let pids: Vec<String> = run("/bin/ps", &["-axo", "pid=,nice=,user=,args="]).lines().filter_map(|line| {
        let mut parts = line.split_whitespace();
        let (pid, nice, owner) = (parts.next()?, parts.next()?.parse::<i32>().ok()?, parts.next()?);
        let args: String = parts.collect::<Vec<_>>().join(" ");
        (owner == user && nice < 10 && heavy_process(&args)).then(|| pid.to_owned())
    }).collect();
    if pids.is_empty() { return 0; }
    let mut command = vec!["-n", "10", "-p"];
    command.extend(pids.iter().map(String::as_str));
    run("/usr/bin/renice", &command);
    pids.len() as u32
}

/// Rule 11: test runners use at most this many workers.
pub const TEST_WORKERS: u32 = 2;

fn split_segments(command: &str) -> Vec<(String, String)> {
    // (segment, separator that follows it)
    let mut out = Vec::new();
    let mut current = String::new();
    let chars: Vec<char> = command.chars().collect();
    let mut i = 0;
    let (mut single, mut double) = (false, false);
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            _ => {}
        }
        if !single && !double {
            let two: String = chars[i..chars.len().min(i + 2)].iter().collect();
            if two == "&&" || two == "||" {
                out.push((std::mem::take(&mut current), two));
                i += 2;
                continue;
            }
            if c == ';' || c == '|' || c == '\n' {
                out.push((std::mem::take(&mut current), c.to_string()));
                i += 1;
                continue;
            }
        }
        current.push(c);
        i += 1;
    }
    out.push((current, String::new()));
    out
}

fn has_flag(words: &[&str], flags: &[&str]) -> bool {
    words.iter().any(|w| flags.iter().any(|f| w == f || w.starts_with(&format!("{f}="))))
}

/// The same command with its test runners capped at `TEST_WORKERS`, or None
/// when nothing needed capping. `script` returns a package's `test` script
/// for a directory (relative to the command's working directory), so
/// `yarn test` is capped according to what it actually runs.
pub fn cap_test_workers(command: &str, script: &dyn Fn(Option<&str>) -> Option<String>) -> Option<String> {
    let n = TEST_WORKERS;
    let mut changed = false;
    let mut directory: Option<String> = None;
    let mut rebuilt = String::new();
    for (segment, separator) in split_segments(command) {
        let words: Vec<&str> = segment.split_whitespace().collect();
        let mut extra = String::new();
        let mut replaced = segment.clone();
        let is = |name: &str| words.iter().any(|w| *w == name || w.ends_with(&format!("/{name}")));
        if words.first() == Some(&"cd") {
            directory = words.get(1).map(|d| d.trim_matches(|c| c == '"' || c == '\'').to_owned());
        } else if is("jest") && !has_flag(&words, &["--maxWorkers", "-w", "--runInBand", "-i"]) {
            extra = format!(" --maxWorkers={n}");
        } else if is("vitest") && !has_flag(&words, &["--maxWorkers", "--no-file-parallelism", "--poolOptions.threads.maxThreads"]) {
            extra = format!(" --maxWorkers={n}");
        } else if is("turbo") && words.contains(&"run") && !has_flag(&words, &["--concurrency"]) {
            extra = format!(" --concurrency={n}");
        } else if is("pytest") {
            // Only an explicit or automatic -n above the cap is changed.
            let mut out = Vec::new();
            let mut iter = words.iter().peekable();
            while let Some(w) = iter.next() {
                if (*w == "-n" || *w == "--numprocesses") && iter.peek().is_some_and(|v| v.parse::<u32>().map_or(**v == "auto" || **v == "logical", |k| k > n)) {
                    iter.next();
                    out.push(format!("{w} {n}"));
                    changed = true;
                } else {
                    out.push(w.to_string());
                }
            }
            if changed { replaced = format!("{}{}", if segment.starts_with(' ') { " " } else { "" }, out.join(" ")) + if segment.ends_with(' ') { " " } else { "" }; }
        } else if let [tool, rest @ ..] = words.as_slice() {
            let runs_test = matches!(*tool, "yarn" | "pnpm") && rest.first() == Some(&"test")
                || *tool == "npm" && (rest.first() == Some(&"test") || rest.starts_with(&["run", "test"]));
            if runs_test && !has_flag(&words, &["--maxWorkers", "--concurrency"]) {
                if let Some(test) = script(directory.as_deref()) {
                    let pass = if *tool == "npm" { " --" } else { "" };
                    if test.contains("jest") || test.contains("vitest") {
                        extra = format!("{pass} --maxWorkers={n}");
                    } else if test.contains("turbo") {
                        extra = format!("{pass} --concurrency={n}");
                    }
                }
            }
        }
        if !extra.is_empty() {
            changed = true;
            let trimmed = replaced.trim_end();
            let tail = &replaced[trimmed.len()..];
            replaced = format!("{trimmed}{extra}{tail}");
        }
        rebuilt.push_str(&replaced);
        rebuilt.push_str(&separator);
    }
    changed.then_some(rebuilt)
}

/// Where Neko publishes the machine rules agents outside it should follow.
pub fn rules_path() -> std::path::PathBuf {
    neko_protocol::support_dir().join("agent-rules.md")
}

/// Writes the rules agents outside Neko get, when they changed.
pub fn publish_rules(text: &str) {
    let path = rules_path();
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text) {
        let temporary = path.with_extension("tmp");
        if std::fs::write(&temporary, text).is_ok() {
            let _ = std::fs::rename(&temporary, &path);
        }
    }
}

/// The text to hand a session once per rules change: Neko's machine rules,
/// plus a pointer when ~/.agents/RESOURCES.md changed. Records that this
/// session has now seen this version. None when it already had.
fn rules_notice(session: &str) -> Option<String> {
    use sha2::{Digest, Sha256};
    if session.is_empty() { return None; }
    let rules = std::fs::read_to_string(rules_path()).unwrap_or_default();
    let resources = std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".agents/RESOURCES.md"))
        .and_then(|p| std::fs::read(p).ok()).unwrap_or_default();
    let digest = format!("{:x}", Sha256::new().chain_update(rules.as_bytes()).chain_update(&resources).finalize());
    let state_path = neko_protocol::support_dir().join("hook-sessions.json");
    let mut seen: std::collections::BTreeMap<String, (String, i64)> = std::fs::read_to_string(&state_path).ok()
        .and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    if seen.get(session).is_some_and(|(d, _)| *d == digest) { return None; }
    let now = crate::now_unix_ms();
    seen.retain(|_, (_, at)| now - *at < 7 * 24 * 3600 * 1000);
    seen.insert(session.to_owned(), (digest, now));
    let temporary = state_path.with_extension(format!("tmp{}", std::process::id()));
    if std::fs::write(&temporary, serde_json::to_string(&seen).unwrap_or_default()).is_ok() {
        let _ = std::fs::rename(&temporary, &state_path);
    }
    (!rules.trim().is_empty()).then(|| format!(
        "Neko's machine rules apply to this session now, even though it started earlier. Follow them from your next command on:\n{rules}\nIf ~/.agents/RESOURCES.md changed since you last read it, read it again."
    ))
}

/// `neko-daemon --machine-check`: one line about the Mac, exit 1 while new
/// work is held. `--machine-check --hook`: Claude Code PreToolUse mode; reads
/// the hook JSON on stdin and denies work-starting shell commands while held.
/// Returns the exit code when this process was asked to check.
pub fn check_if_requested() -> Option<i32> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) != Some("--machine-check") {
        return None;
    }
    let health = sense(false);
    if args.iter().any(|a| a == "--hook") {
        let mut input = String::new();
        let _ = std::io::Read::read_to_string(&mut std::io::stdin(), &mut input);
        let payload: serde_json::Value = serde_json::from_str(&input).unwrap_or_default();
        if let Some(output) = hook_decision(&payload, &health) {
            println!("{output}");
        }
        return Some(0);
    }
    println!("{}", summary(&health));
    match &health.holding {
        Some(reason) => { eprintln!("{reason}"); Some(1) }
        None => Some(0),
    }
}

/// What the PreToolUse hook says about one tool call, if anything: refuse
/// heavy work while the Mac is held, cap test runners (rewriting the command
/// where the session skips permission prompts anyway, otherwise refusing with
/// the capped command to run), and hand over rule changes once per session.
pub fn hook_decision(payload: &serde_json::Value, health: &MachineHealth) -> Option<serde_json::Value> {
    let notice = rules_notice(payload["session_id"].as_str().unwrap_or(""));
    let mut output = serde_json::json!({"hookEventName": "PreToolUse"});
    if let Some(text) = &notice {
        output["additionalContext"] = serde_json::json!(text);
    }
    if payload["tool_name"] == "Bash" {
        let command = payload["tool_input"]["command"].as_str().unwrap_or("");
        let cwd = std::path::PathBuf::from(payload["cwd"].as_str().unwrap_or("."));
        if heavy_command(command) {
            if let Some(reason) = &health.holding {
                output["permissionDecision"] = serde_json::json!("deny");
                output["permissionDecisionReason"] = serde_json::json!(format!("[Neko] {reason} Queue this and say so in your reply."));
                return Some(serde_json::json!({"hookSpecificOutput": output}));
            }
        }
        let script = |dir: Option<&str>| -> Option<String> {
            let base = dir.map(|d| cwd.join(d)).unwrap_or_else(|| cwd.clone());
            let manifest: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(base.join("package.json")).ok()?).ok()?;
            manifest["scripts"]["test"].as_str().map(str::to_owned)
        };
        if let Some(capped) = cap_test_workers(command, &script) {
            if payload["permission_mode"] == "bypassPermissions" {
                let mut input = payload["tool_input"].clone();
                input["command"] = serde_json::json!(capped);
                output["permissionDecision"] = serde_json::json!("allow");
                output["permissionDecisionReason"] = serde_json::json!(format!("[Neko] Test runners use at most {TEST_WORKERS} workers on this Mac."));
                output["updatedInput"] = input;
            } else {
                output["permissionDecision"] = serde_json::json!("deny");
                output["permissionDecisionReason"] = serde_json::json!(format!("[Neko] Test runners use at most {TEST_WORKERS} workers on this Mac. Run this instead: {capped}"));
            }
            return Some(serde_json::json!({"hookSpecificOutput": output}));
        }
    }
    notice.map(|_| serde_json::json!({"hookSpecificOutput": output}))
}

/// One line: load, disk, memory, and the night limit when it applies.
pub fn summary(h: &MachineHealth) -> String {
    let mut line = format!("load {:.1} · {:.0} GB free · memory {} ({:.1} GB compressed, swap {:.1}/{:.1} GB)", h.load, h.free_disk_gb, h.pressure, h.compressed_gb, h.swap_used_gb, h.swap_total_gb);
    if h.night { line.push_str(" · night: one heavy job at a time"); }
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_shared_machine_check_reason_is_read_from_its_wait_line() {
        assert_eq!(parse_comfort_wait("WAIT owner is using the Mac with load 14.99 (thermal=nominal memory=normal)\n").as_deref(), Some("owner is using the Mac with load 14.99"));
        assert_eq!(parse_comfort_wait("WAIT\n").as_deref(), Some("the shared machine check says wait"));
        assert_eq!(parse_comfort_wait("OK thermal=nominal"), None);
    }

    #[test]
    fn readings_parse_from_real_macos_output() {
        assert_eq!(parse_load("{ 7.91 8.51 7.77 }"), 7.91);
        let (used, total) = parse_swap("total = 5120.00M  used = 3850.25M  free = 1269.75M  (encrypted)");
        assert!((total - 5.0).abs() < 0.01 && (used - 3.76).abs() < 0.01);
        let vm = "Mach Virtual Memory Statistics: (page size of 16384 bytes)\nPages free:      75792.\nPages occupied by compressor:   329244.\n";
        assert!((parse_compressed(vm) - 5.02).abs() < 0.01);
        assert!((parse_free_disk("Filesystem 1024-blocks Used Available Capacity\n/dev/disk3s1s1 482797652 12275296 151889532 8% 458725 1518895320 0% /") - 144.85).abs() < 0.1);
        assert_eq!(parse_kernel_task("PID COMMAND %CPU\n0 kernel_task 10.3"), 10.3);
        let agents = parse_agents("300000 /opt/homebrew/bin/claude\n200000 claude\n50000 node\n10000 /usr/bin/login\n");
        assert_eq!(agents[0], AgentMemory { name: "Claude Code sessions".into(), processes: 2, mb: 488 });
        assert_eq!(agents.len(), 2, "processes that are not agents are not counted");
    }

    fn health(load: f64, free: f64, pressure: &str) -> MachineHealth {
        MachineHealth { load, free_disk_gb: free, pressure: pressure.into(), ..Default::default() }
    }

    #[test]
    fn the_policy_holds_on_disk_memory_and_load_and_runs_one_at_a_time_at_night() {
        let mut h = health(5.0, 100.0, "normal");
        apply_policy(&mut h, 14);
        assert_eq!((h.holding.is_none(), h.cap, h.night), (true, None, false), "a healthy day");
        let mut h = health(5.0, 9.0, "normal");
        apply_policy(&mut h, 14);
        assert!(h.holding.unwrap().contains("9 GB"));
        let mut h = health(5.0, 100.0, "critical");
        apply_policy(&mut h, 14);
        assert!(h.holding.unwrap().contains("Memory"));
        let mut h = health(21.0, 100.0, "normal");
        apply_policy(&mut h, 14);
        assert!(h.holding.is_some());
        let mut h = health(10.0, 100.0, "normal");
        apply_policy(&mut h, 3);
        assert_eq!((h.holding.is_none(), h.cap, h.night), (true, Some(1), true), "night: one at a time");
        let mut h = health(13.0, 100.0, "normal");
        apply_policy(&mut h, 3);
        assert!(h.holding.unwrap().contains("Night"));
        let mut h = health(13.0, 100.0, "normal");
        apply_policy(&mut h, 6);
        assert!(h.holding.is_none() && !h.night, "06:00 is day again");
        h.comfort_wait = Some("owner is using the Mac with load 15".into());
        apply_policy(&mut h, 14);
        assert!(h.holding.as_deref().is_some_and(|r| r.contains("Keeping the Mac cool") && r.contains("load 15")), "{:?}", h.holding);
        let mut h = health(5.0, 100.0, "warning");
        apply_policy(&mut h, 14);
        assert_eq!(h.cap, Some(1), "memory warning: one at a time");
    }

    #[test]
    fn test_runners_are_capped_at_two_workers_wherever_they_sit_in_a_command() {
        let none = |_: Option<&str>| None;
        assert_eq!(cap_test_workers("npx jest src/a.test.ts", &none).as_deref(), Some("npx jest src/a.test.ts --maxWorkers=2"));
        assert_eq!(cap_test_workers("cd app && npx jest && echo done", &none).as_deref(), Some("cd app && npx jest --maxWorkers=2 && echo done"));
        assert_eq!(cap_test_workers("npx jest --maxWorkers=4", &none), None, "an explicit choice is left alone");
        assert_eq!(cap_test_workers("npx vitest run", &none).as_deref(), Some("npx vitest run --maxWorkers=2"));
        assert_eq!(cap_test_workers("turbo run test lint:tsc", &none).as_deref(), Some("turbo run test lint:tsc --concurrency=2"));
        assert_eq!(cap_test_workers("pytest -n auto tests", &none).as_deref(), Some("pytest -n 2 tests"));
        assert_eq!(cap_test_workers("pytest -n 2 tests", &none), None);
        assert_eq!(cap_test_workers("git status | head", &none), None);
        assert_eq!(cap_test_workers("echo 'npx jest' > notes", &none), None, "quoted text is not a command");
        let jest = |_: Option<&str>| Some("jest --coverage".to_owned());
        assert_eq!(cap_test_workers("yarn test src", &jest).as_deref(), Some("yarn test src --maxWorkers=2"));
        assert_eq!(cap_test_workers("npm test", &jest).as_deref(), Some("npm test -- --maxWorkers=2"));
        let turbo = |dir: Option<&str>| (dir == Some("apps/web")).then(|| "turbo run test".to_owned());
        assert_eq!(cap_test_workers("cd apps/web && pnpm test", &turbo).as_deref(), Some("cd apps/web && pnpm test --concurrency=2"));
    }

    #[test]
    fn agents_outside_neko_get_the_machine_section_plus_the_worker_cap() {
        let rules = machine_rules("# Rules\n\n## Engineering\nTest first.\n\n## This machine\n1. One heavy job.\n\n## Repositories\nPRs only.");
        assert!(rules.starts_with("1. One heavy job.") && rules.contains("--maxWorkers=2") && !rules.contains("PRs only"));
        assert!(heavy_process("/opt/homebrew/bin/node /repo/node_modules/.bin/jest --watch"));
        assert!(heavy_process("/usr/bin/xcodebuild -scheme App"));
        assert!(!heavy_process("/opt/homebrew/bin/node /repo/node_modules/.bin/vite dev"), "a dev server is not a build");
    }

    #[test]
    fn only_work_starting_commands_are_heavy() {
        assert!(heavy_command("cd x && pnpm install --offline"));
        assert!(heavy_command("xcodebuild -scheme App build"));
        assert!(!heavy_command("git status"));
        assert!(!heavy_command("ls node_modules"));
    }
}
