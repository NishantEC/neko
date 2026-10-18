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
        let command = payload["tool_input"]["command"].as_str().unwrap_or("");
        if payload["tool_name"] == "Bash" && heavy_command(command) {
            if let Some(reason) = &health.holding {
                println!("{}", serde_json::json!({
                    "hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "deny"},
                    "systemMessage": format!("[Neko] {reason} Queue this and say so in your reply."),
                }));
            }
        }
        return Some(0);
    }
    println!("{}", summary(&health));
    match &health.holding {
        Some(reason) => { eprintln!("{reason}"); Some(1) }
        None => Some(0),
    }
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
        let mut h = health(5.0, 100.0, "warning");
        apply_policy(&mut h, 14);
        assert_eq!(h.cap, Some(1), "memory warning: one at a time");
    }

    #[test]
    fn only_work_starting_commands_are_heavy() {
        assert!(heavy_command("cd x && pnpm install --offline"));
        assert!(heavy_command("xcodebuild -scheme App build"));
        assert!(!heavy_command("git status"));
        assert!(!heavy_command("ls node_modules"));
    }
}
