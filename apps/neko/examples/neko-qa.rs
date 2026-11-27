//! `neko-qa`: drives the real app through one scenario against a real daemon and records it (docs/knowledge/qa.md).
//!
//! Run by `scripts/qa/run.sh`, which builds it, makes the temporary folders and checks afterwards that nothing it started is left.
//!
//! ```text
//! neko-qa --scenario FILE --out DIR --work DIR --daemon BIN [--fake BIN] [--cli fake|real] [--window --app BIN] [--commit SHA]
//! ```
//!
//! Headless (the default): the app's own view, state and link run in Rune's headless runtime, connected to a real `neko-daemon`
//! on a temporary data folder. Unlike Rune's `--script` (which replays on a virtual clock), each step can wait on the wall clock
//! for what the daemon and the CLI do, with a deadline, while frames keep being drawn: every frame goes to ffmpeg for the video,
//! steps that ask for one leave a picture, and each check leaves the accessibility tree.
//!
//! `--window`: the real app in a real window (`qa/window.rs`).

#[path = "qa/record.rs"]
mod record;
#[path = "qa/scenario.rs"]
mod scenario;
#[path = "qa/window.rs"]
mod window;

use std::collections::BTreeMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use neko_app::state::{App, Daemon};
use rune::{Env, ElementInfo, Key, KeyEvent, Mods, Runtime};
use serde_json::{Value, json};

use record::Recorder;
use scenario::{Places, Scenario, Step};

/// Frames per second of the video.
const FPS: f64 = 8.0;
/// A step's wait when the scenario gives none, in seconds.
const WAIT: f64 = 15.0;

pub struct Opts {
    pub scenario: PathBuf,
    pub out: PathBuf,
    pub work: PathBuf,
    pub daemon: PathBuf,
    pub fake: Option<PathBuf>,
    pub app: Option<PathBuf>,
    pub real: bool,
    pub window: bool,
    pub commit: String,
}

fn opts() -> Result<Opts, String> {
    let mut a = std::env::args().skip(1);
    let (mut scenario, mut out, mut work, mut daemon, mut fake, mut app) = (None, None, None, None, None, None);
    let (mut real, mut window, mut commit) = (false, false, "unknown".to_owned());
    while let Some(x) = a.next() {
        let mut val = || a.next().ok_or(format!("{x} needs a value"));
        match x.as_str() {
            "--scenario" => scenario = Some(PathBuf::from(val()?)),
            "--out" => out = Some(PathBuf::from(val()?)),
            "--work" => work = Some(PathBuf::from(val()?)),
            "--daemon" => daemon = Some(PathBuf::from(val()?)),
            "--fake" => fake = Some(PathBuf::from(val()?)),
            "--app" => app = Some(PathBuf::from(val()?)),
            "--cli" => real = val()? == "real",
            "--window" => window = true,
            "--commit" => commit = val()?,
            other => return Err(format!("unknown argument {other}")),
        }
    }
    let o = Opts {
        scenario: scenario.ok_or("--scenario FILE")?,
        out: out.ok_or("--out DIR")?,
        work: work.ok_or("--work DIR")?,
        daemon: daemon.ok_or("--daemon BIN")?,
        fake,
        app,
        real,
        window,
        commit,
    };
    if !o.real && o.fake.is_none() {
        return Err("--cli fake needs --fake BIN".into());
    }
    if o.window && o.app.is_none() {
        return Err("--window needs --app BIN".into());
    }
    Ok(o)
}

fn main() {
    let o = match opts() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("neko-qa: {e}");
            std::process::exit(2);
        }
    };
    std::process::exit(match run(&o) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(e) => {
            eprintln!("neko-qa: {e}");
            2
        }
    });
}

/// One expectation's outcome.
pub struct Outcome {
    pub step: usize,
    pub expect: String,
    pub ok: Option<bool>,
    pub reason: String,
    pub secs: f64,
    pub picture: Option<String>,
}

/// What every run shares: the folders, the log, the PIDs it started.
pub struct Run {
    pub o: Opts,
    pub s: Scenario,
    pub places: Places,
    pub log: std::fs::File,
    pub began: Instant,
    pub outcomes: Vec<Outcome>,
    pub daemon: Option<Child>,
    /// Problems that fail the run (a recording that broke).
    pub notes: Vec<String>,
    /// Facts worth reading that fail nothing (launch time, steps a window leaves out).
    pub notes_info: Vec<String>,
    /// Records made during the run (a checkout's state before), by name.
    pub notes_marks: BTreeMap<String, String>,
}

impl Run {
    pub fn say(&mut self, line: &str) {
        let t = self.began.elapsed().as_secs_f64();
        let _ = writeln!(self.log, "[{t:7.2}] {line}");
        println!("  {line}");
    }

    /// Records a PID this run started, for run.sh's check that nothing is left.
    pub fn started(&mut self, what: &str, pid: u32) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(self.o.out.join(".pids")) {
            let _ = writeln!(f, "{pid} {what}");
        }
    }

    /// The daemon's environment: fake runs get a HOME of their own, a short PATH and the fake CLI; real runs the person's own
    /// environment (their login), the cheapest model, and nothing of the Claude Code session this rig may run inside.
    fn daemon_command(&self, extra: &BTreeMap<String, String>) -> Command {
        let mut c = Command::new(&self.o.daemon);
        c.arg("--data-dir").arg(&self.places.data);
        if self.o.real {
            for (k, _) in std::env::vars_os() {
                let k = k.to_string_lossy().into_owned();
                if k.starts_with("CLAUDE") || k.starts_with("NEKO_") || k == "ANTHROPIC_API_KEY" {
                    c.env_remove(&k);
                }
            }
            c.env("NEKO_CLAUDE_MODEL", std::env::var("NEKO_QA_MODEL").unwrap_or_else(|_| "haiku".into()));
        } else {
            c.env_clear();
            c.env("HOME", &self.places.home).env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin").env("LANG", "en_US.UTF-8").env("TMPDIR", self.places.work.join("tmp"));
            if let Some(f) = &self.o.fake {
                c.env("NEKO_CLAUDE_BIN", f);
            }
        }
        for (k, v) in &self.s.daemon_env {
            c.env(k, v);
        }
        for (k, v) in extra {
            c.env(k, v);
        }
        c
    }

    pub fn start_daemon(&mut self, extra: &BTreeMap<String, String>) -> Result<(), String> {
        use std::os::unix::process::CommandExt;
        let log = std::fs::OpenOptions::new().create(true).append(true).open(self.places.work.join("daemon.log")).map_err(|e| e.to_string())?;
        let child = self.daemon_command(extra).stdin(Stdio::null()).stdout(log.try_clone().map_err(|e| e.to_string())?).stderr(log).process_group(0).spawn().map_err(|e| format!("the daemon could not start: {e}"))?;
        let pid = child.id();
        self.started("neko-daemon", pid);
        self.daemon = Some(child);
        let end = Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok((info, _)) = neko_protocol::client::find(&self.places.data)
                && info.pid == pid
            {
                break;
            }
            if let Some(Ok(Some(st))) = self.daemon.as_mut().map(Child::try_wait) {
                return Err(format!("the daemon ended at start: {st}"));
            }
            if Instant::now() > end {
                return Err("the daemon did not write daemon.json within 15 s".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        self.say(&format!("daemon started (pid {pid})"));
        Ok(())
    }

    /// Stops the daemon this run started: SIGTERM (or SIGKILL), then SIGKILL after five seconds. Its runs' processes are
    /// recorded first, so run.sh can check they went with it.
    pub fn stop_daemon(&mut self, kill: bool) -> Result<(), String> {
        let Some(mut child) = self.daemon.take() else { return Ok(()) };
        let pid = child.id() as i32;
        let below = descendants(pid);
        for (p, what) in &below {
            self.started(what, *p);
        }
        // SAFETY: the daemon this run started, by its PID.
        unsafe { libc::kill(pid, if kill { libc::SIGKILL } else { libc::SIGTERM }) };
        let end = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(Some(_)) = child.try_wait() {
                break;
            }
            if Instant::now() > end {
                // SAFETY: as above.
                unsafe { libc::kill(pid, libc::SIGKILL) };
                let _ = child.wait();
                self.say("the daemon did not stop within 5 s and was killed");
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        self.say(&format!("daemon stopped ({})", if kill { "SIGKILL" } else { "SIGTERM" }));
        // Its runs' processes (guardian, CLI, what they started) must follow it: the run guard allows itself about two seconds.
        let end = Instant::now() + Duration::from_secs(4);
        // SAFETY: signal 0 only asks whether a process this run recorded still exists.
        let alive = |p: u32| unsafe { libc::kill(p as i32, 0) == 0 };
        while Instant::now() < end && below.iter().any(|(p, _)| alive(*p)) {
            std::thread::sleep(Duration::from_millis(50));
        }
        let left: Vec<String> = below.iter().filter(|(p, _)| alive(*p)).map(|(p, w)| format!("{w} ({p})")).collect();
        if !left.is_empty() {
            let line = format!("Still running 4 s after the daemon stopped: {}", left.join(", "));
            self.say(&line);
            self.notes.push(line);
        }
        Ok(())
    }

    pub fn outcome(&mut self, step: &Step, ok: Option<bool>, reason: String, secs: f64, picture: Option<String>) {
        let expect = step.expect().map(str::to_owned).unwrap_or_else(|| format!("Step {} ({}) runs", step.n, step.kind()));
        let mark = match ok {
            Some(true) => "pass",
            Some(false) => "FAIL",
            None => "skip",
        };
        self.say(&format!("{mark}  {:02} {expect}{}", step.n, if reason.is_empty() { String::new() } else { format!(" — {reason}") }));
        self.outcomes.push(Outcome { step: step.n, expect, ok, reason, secs, picture });
    }

    /// The usage the daemon's runs reported (tokens, cost), asked of the daemon itself.
    pub fn usage(&self) -> Option<Value> {
        let (info, token) = neko_protocol::client::find(&self.places.data).ok()?;
        let mut c = neko_protocol::client::Client::connect(info.port, &token, "neko-qa").ok()?;
        let r = c.request(neko_protocol::Request::DayUsage { from_ms: 0 }).ok()?;
        c.close();
        match r {
            neko_protocol::Reply::DayUsage { usage } => serde_json::to_value(usage).ok(),
            _ => None,
        }
    }
}

/// Every process below `root`: (pid, what).
pub fn descendants(root: i32) -> Vec<(u32, String)> {
    let Ok(out) = Command::new("ps").args(["-axo", "pid=,ppid=,comm="]).output() else { return Vec::new() };
    let rows: Vec<(i32, i32, String)> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|l| {
            let mut w = l.split_whitespace();
            Some((w.next()?.parse().ok()?, w.next()?.parse().ok()?, w.collect::<Vec<_>>().join(" ")))
        })
        .collect();
    let mut found = Vec::new();
    let mut frontier = vec![root];
    while let Some(p) = frontier.pop() {
        for (pid, ppid, comm) in &rows {
            if *ppid == p && !found.iter().any(|(f, _)| *f == *pid as u32) {
                found.push((*pid as u32, Path::new(comm).file_name().map_or(comm.clone(), |n| n.to_string_lossy().into_owned())));
                frontier.push(*pid);
            }
        }
    }
    found
}

fn run(o: &Opts) -> Result<bool, String> {
    let places = Places { work: o.work.clone(), files: o.work.join("files"), home: o.work.join("home"), data: o.work.join("data"), fake_mcp: o.fake.as_ref().map(|f| f.with_file_name("neko-fake-mcp")).unwrap_or_default() };
    // The app's prefs (frame, last workspace, folded sections) live in the run's folder, never the person's.
    // SAFETY: set before any other thread starts.
    unsafe { std::env::set_var("RUNE_DIRS_ROOT", o.work.join("rune")) };
    for d in [&places.files, &places.home.join(".claude"), &places.data, &o.work.join("tmp"), &o.out.join("steps"), &o.out.join("tree")] {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let s = scenario::load(&o.scenario, &places)?;
    let cli = if o.real { "real" } else { "fake" };
    if !s.cli.iter().any(|c| c == cli) {
        return Err(format!("{} does not run with --cli {cli} (it runs with {})", s.name, s.cli.join(", ")));
    }
    if !o.window && !s.headless {
        return Err(format!("{} runs only in a window (--window)", s.name));
    }
    if o.window && !s.window {
        return Err(format!("{} has no --window form", s.name));
    }
    scenario::make_files(&s, &places)?;
    if let Some(settings) = &s.claude_settings {
        std::fs::write(places.home.join(".claude/settings.json"), serde_json::to_vec_pretty(settings).unwrap_or_default()).map_err(|e| e.to_string())?;
    }
    let log = std::fs::File::create(o.out.join("driver.log")).map_err(|e| e.to_string())?;
    let opts = Opts { scenario: o.scenario.clone(), out: o.out.clone(), work: o.work.clone(), daemon: o.daemon.clone(), fake: o.fake.clone(), app: o.app.clone(), real: o.real, window: o.window, commit: o.commit.clone() };
    let mut r = Run { o: opts, s, places, log, began: Instant::now(), outcomes: Vec::new(), daemon: None, notes: Vec::new(), notes_info: Vec::new(), notes_marks: BTreeMap::new() };
    println!("{} ({}, {})", r.s.name, cli, if o.window { "window" } else { "headless" });
    let started_utc = utc_now();
    r.start_daemon(&BTreeMap::new())?;
    let video = if o.window { window::run(&mut r) } else { headless(&mut r) };
    let usage = if o.real { r.usage() } else { None };
    r.stop_daemon(false)?;
    if let Err(e) = &video {
        r.notes.push(format!("Recording: {e}"));
    }
    write_results(&r, &started_utc, usage)
}

// ---------------------------------------------------------------------------------------------------------------------------
// Headless

/// The window's size and the person's home, for the app's first state.
static FIRST: OnceLock<((f32, f32), Option<String>)> = OnceLock::new();

/// The app's first state in a QA run: the real clock, the person's home for `~/` paths, waiting for the daemon.
fn qa_state() -> App {
    let (size, home) = FIRST.get().cloned().unwrap_or(((1200.0, 800.0), None));
    let (now_ms, utc_offset_s) = neko_app::state::system_clock();
    // As the real window does: the last workspace and the folded sections from the prefs, and choices written back. The prefs are
    // the run's own (`RUNE_DIRS_ROOT` points into its folder), so a quit and relaunch keeps them and nothing else is touched.
    let prefs = rune::prefs::Prefs::open(neko_app::PREFS_ID);
    let workspace = prefs.get::<String>(neko_app::WORKSPACE_KEY).filter(|w| !w.is_empty()).unwrap_or_else(|| neko_protocol::GENERAL.to_owned());
    let folded = prefs.get::<Vec<String>>(neko_app::FOLDED_KEY).unwrap_or_default().into_iter().collect();
    App { daemon: Daemon::Starting, now_ms, utc_offset_s, clock: neko_app::state::system_clock, home, width: Some(size.0), persist: true, workspace, folded, ..App::default() }
}

/// A poster that posts nothing: it writes what would have been posted, as `NEKO_NOTIFY_LOG` has it (`notify::System` logs the same).
struct LogPoster(PathBuf);

impl LogPoster {
    fn write(&self, event: Value) {
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&self.0) {
            let _ = writeln!(f, "{event}");
        }
    }
}

impl neko_app::notify::Poster for LogPoster {
    fn post(&self, n: &neko_app::notify::Note) {
        let buttons: Vec<String> = n
            .buttons
            .iter()
            .map(|b| match b {
                neko_app::notify::Button::Action { label, .. } => (*label).to_owned(),
                neko_app::notify::Button::Reply { label, .. } => format!("reply: {label}"),
            })
            .collect();
        self.write(json!({"post": n.id, "title": n.title, "body": n.body, "buttons": buttons, "posted": true}));
    }
    fn withdraw(&self, ids: &[String]) {
        self.write(json!({"withdraw": ids}));
    }
}

/// `{{utc+N}}` in a step's words: the UTC time of day N minutes from now, as HH:MM (for a time the daemon, started with `TZ=UTC`, reads).
fn with_times(text: &str) -> String {
    let mut out = text.to_owned();
    while let Some(at) = out.find("{{utc+") {
        let Some(end) = out[at..].find("}}") else { break };
        let minutes: i64 = out[at + 6..at + end].parse().unwrap_or(0);
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let m = (now / 60 + minutes).rem_euclid(24 * 60);
        out.replace_range(at..at + end + 2, &format!("{:02}:{:02}", m / 60, m % 60));
    }
    out
}

fn new_app(r: &Run) -> Runtime<App> {
    let _ = FIRST.set((r.s.size, std::env::var("HOME").ok()));
    let mut rt = neko_app::app().init(qa_state).headless_runtime(r.s.size, Env::new(false, 2.0));
    let conn = neko_app::link::Conn::start(r.places.data.clone(), None, || {});
    // Nothing reaches the system: what the app would post goes to the run's own log, in the form the window runs' log has, so the
    // same checks (`notified`, `withdrawn`, `posts`) read it.
    let log = r.places.work.join("notify.jsonl");
    rt.update(move |s, _| {
        s.notifier = neko_app::notify::Notifier::new(Box::new(LogPoster(log)));
        s.link_started(conn);
    });
    rt
}

struct Headless<'a> {
    r: &'a mut Run,
    rt: Option<Runtime<App>>,
    rec: Recorder,
    last: Instant,
    marks: BTreeMap<String, String>,
}

impl Headless<'_> {
    /// One frame on the wall clock, and the video's share of it.
    fn pump(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last).as_secs_f64().min(0.1);
        self.last = now;
        if let Some(rt) = &mut self.rt {
            rt.frame(dt);
            self.rec.tick(rt);
        }
    }

    fn rt(&mut self) -> Result<&mut Runtime<App>, String> {
        self.rt.as_mut().ok_or_else(|| "the app is not running".to_owned())
    }

    fn sleep(&mut self, secs: f64) {
        let end = Instant::now() + Duration::from_secs_f64(secs);
        while Instant::now() < end {
            self.pump();
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    fn snap(&mut self, stem: &str) -> Result<String, String> {
        let rel = format!("steps/{stem}.png");
        let path = self.r.o.out.join(&rel);
        let rt = self.rt.as_mut().ok_or("the app is not running")?;
        self.rec.png(rt, &path)?;
        self.tree(stem)?;
        Ok(rel)
    }

    fn tree(&mut self, stem: &str) -> Result<(), String> {
        let rt = self.rt.as_ref().ok_or("the app is not running")?;
        let tree = rt.accessibility_json();
        std::fs::write(self.r.o.out.join(format!("tree/{stem}.json")), serde_json::to_vec_pretty(&tree).unwrap_or_default()).map_err(|e| e.to_string())
    }

    fn click(&mut self, sel: &Value) -> Result<(), String> {
        // As a person would: wait (up to a second) for the target to stop moving before pressing it.
        let end = Instant::now() + Duration::from_secs(1);
        while Instant::now() < end && {
            let all = self.rt()?.elements();
            all.iter().any(|e| matches(&all, e, sel) && (e.moving || e.opacity < 0.99))
        } {
            self.pump();
            std::thread::sleep(Duration::from_millis(15));
        }
        let rt = self.rt()?;
        let all = rt.elements();
        let e = pick(&all, sel)?;
        let r = if e.visible[2] > 0.0 && e.visible[3] > 0.0 { e.visible } else { e.rect };
        let (x, y) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0);
        rt.pointer_move(x, y);
        rt.pointer_down(x, y);
        rt.pointer_up(x, y);
        self.pump();
        self.pump();
        Ok(())
    }

    fn key(&mut self, ev: KeyEvent) -> Result<(), String> {
        self.rt()?.key(ev);
        self.pump();
        self.pump();
        Ok(())
    }

    /// Whether `c` holds now; Err says what was seen instead.
    fn check(&self, c: &Value) -> Result<(), String> {
        let rt = self.rt.as_ref();
        if let Some(list) = c["all"].as_array() {
            return list.iter().try_for_each(|x| self.check(x));
        }
        if let Some(list) = c["any"].as_array() {
            let mut why = Vec::new();
            for x in list {
                match self.check(x) {
                    Ok(()) => return Ok(()),
                    Err(e) => why.push(e),
                }
            }
            return Err(why.join("; and "));
        }
        if c.get("notified").is_some() || c.get("withdrawn").is_some() || c.get("posts").is_some() {
            return window::check_notify_log(self.r, c);
        }
        if let Some(sel) = c.get("see") {
            let rt = rt.ok_or("the app is not running")?;
            return pick(&rt.elements(), sel).map(|_| ());
        }
        if let Some(sel) = c.get("gone") {
            let rt = rt.ok_or("the app is not running")?;
            let all = rt.elements();
            let n = matching(&all, sel).len();
            return if n == 0 { Ok(()) } else { Err(format!("{n} element(s) match {sel} (expected none)")) };
        }
        if let Some(sel) = c.get("count") {
            let rt = rt.ok_or("the app is not running")?;
            let all = rt.elements();
            let n = matching(&all, &sel["of"]).len() as u64;
            let (min, max) = (sel["min"].as_u64().or(sel["is"].as_u64()).unwrap_or(0), sel["max"].as_u64().or(sel["is"].as_u64()).unwrap_or(u64::MAX));
            return if n >= min && n <= max { Ok(()) } else { Err(format!("{n} element(s) match {}", sel["of"])) };
        }
        if let Some(sel) = c.get("focus") {
            let rt = rt.ok_or("the app is not running")?;
            let all = rt.elements();
            let Some(f) = all.iter().find(|e| e.focused && !e.exiting) else { return Err("nothing has keyboard focus".into()) };
            if !matches(&all, f, sel) {
                return Err(format!("focus is on {}, not {sel}", describe(f)));
            }
            if !f.focus_visible && c["ring"].as_bool() != Some(false) {
                return Err(format!("{} has focus but no focus ring is drawn", describe(f)));
            }
            return Ok(());
        }
        if let Some(t) = c["title"].as_str() {
            let rt = rt.ok_or("the app is not running")?;
            let got = rt.state.window_title();
            return if got == t { Ok(()) } else { Err(format!("the window's title is {got:?}")) };
        }
        if let Some(want) = c["daemon"].as_str() {
            let rt = rt.ok_or("the app is not running")?;
            let got = match &rt.state.daemon {
                Daemon::Connected => "connected",
                Daemon::Starting => "starting",
                Daemon::Lost(_) => "lost",
                Daemon::Absent => "absent",
            };
            return if got == want { Ok(()) } else { Err(format!("the app's link is {got}")) };
        }
        if let Some(f) = c.get("file") {
            let path = f["path"].as_str().unwrap_or("");
            let body = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", short(path)))?;
            return match f["has"].as_str() {
                Some(h) if !body.contains(h) => Err(format!("{} holds {:?}", short(path), cut(&body, 80))),
                _ => Ok(()),
            };
        }
        if let Some(p) = c["no_file"].as_str() {
            return if Path::new(p).exists() { Err(format!("{} exists", short(p))) } else { Ok(()) };
        }
        if let Some(sh) = c.get("sh") {
            let cmd = sh["run"].as_str().unwrap_or("");
            let out = Command::new("/bin/sh")
                .arg("-c")
                .arg(cmd)
                .current_dir(&self.r.places.work)
                .env("HOME", &self.r.places.home)
                .env("GIT_CONFIG_NOSYSTEM", "1")
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .map_err(|e| e.to_string())?;
            let text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if let Some(h) = sh["has"].as_str()
                && !text.contains(h)
            {
                return Err(format!("`{}` printed {:?}", short(cmd), cut(&text, 160)));
            }
            if let Some(h) = sh["is"].as_str()
                && text != h
            {
                return Err(format!("`{}` printed {:?}, not {h:?}", short(cmd), cut(&text, 160)));
            }
            return Ok(());
        }
        if let Some(name) = c["git_same"].as_str() {
            let (repo, before) = self.marks.get(name).cloned().map(|v| v.split_once('\n').map(|(a, b)| (a.to_owned(), b.to_owned())).unwrap_or_default()).ok_or(format!("no git record named {name}"))?;
            let now = git_state(&repo, &self.r.places.home)?;
            return if now == before { Ok(()) } else { Err(format!("the checkout changed: before {:?}, now {:?}", cut(&before, 160), cut(&now, 160))) };
        }
        Err(format!("unknown check {c}"))
    }

    fn wait(&mut self, c: &Value, within: f64) -> Result<(), String> {
        let end = Instant::now() + Duration::from_secs_f64(within);
        loop {
            self.pump();
            match self.check(c) {
                Ok(()) => return Ok(()),
                Err(e) if Instant::now() > end => return Err(format!("after {within} s: {e}")),
                Err(_) => std::thread::sleep(Duration::from_millis(15)),
            }
        }
    }

    /// One step. Ok(None) for an action; Ok(Some(picture)) or Err for a check.
    fn step(&mut self, step: &Step) -> Result<Option<String>, String> {
        let v = &step.v;
        let within = v["within"].as_f64().unwrap_or(WAIT);
        match step.kind() {
            "snap" => {
                self.pump();
                return self.snap(&step.stem()).map(Some);
            }
            "click" => self.click(&v["on"])?,
            "type" => self.key(KeyEvent { key: Key::Text(with_times(v["text"].as_str().unwrap_or(""))), mods: Mods::default() })?,
            "key" => {
                let n = v["times"].as_u64().unwrap_or(1);
                for _ in 0..n {
                    self.key(parse_key(v["key"].as_str().unwrap_or(""))?)?;
                }
            }
            "sleep" => self.sleep(v["secs"].as_f64().unwrap_or(1.0)),
            "wait" => {
                self.wait(&v["until"], within)?;
            }
            "assert" => {
                self.pump();
                self.check(&v["that"])?;
            }
            "choose-folder" | "add-workspace" => {
                // The Mac's folder panel cannot open headless: Rune's headless runtime holds the app's own request for it, and
                // this answers it with the path, so the app's own code adds the folder as a workspace.
                let path = v["path"].as_str().unwrap_or("").to_owned();
                if step.kind() == "choose-folder" {
                    // Home's composer: the workspace picker's "Add folder…" (the new workspace is then chosen).
                    self.click(&json!({"key": "workspace-picker"}))?;
                    self.click(&json!({"name": "Add folder…", "role": "menuitem"}))?;
                } else {
                    self.click(&json!({"key": "add-workspace"}))?;
                }
                self.pump();
                self.rt()?.resolve_file_dialog(Some(PathBuf::from(&path))).map_err(|e| format!("the folder panel: {e:?}"))?;
                self.pump();
                self.pump();
            }
            "git-record" => {
                let repo = v["repo"].as_str().unwrap_or("").to_owned();
                let st = git_state(&repo, &self.r.places.home)?;
                self.marks.insert(v["as"].as_str().unwrap_or("before").to_owned(), format!("{repo}\n{st}"));
            }
            "remove" => {
                let p = PathBuf::from(v["path"].as_str().unwrap_or(""));
                if !p.starts_with(&self.r.places.work) {
                    return Err(format!("refusing to remove {} (outside the run's folder)", p.display()));
                }
                std::fs::remove_dir_all(&p).or_else(|_| std::fs::remove_file(&p)).map_err(|e| format!("{}: {e}", p.display()))?;
            }
            "quit-app" => {
                self.rt = None;
                self.r.say("app quit (its link closed)");
            }
            "start-app" => {
                let rt = new_app(self.r);
                self.rt = Some(rt);
                self.r.say("app started");
            }
            "stop-daemon" => self.r.stop_daemon(v["signal"].as_str() == Some("kill"))?,
            "start-daemon" => {
                let env: BTreeMap<String, String> = v["env"].as_object().map(|o| o.iter().map(|(k, x)| (k.clone(), x.as_str().unwrap_or("").to_owned())).collect()).unwrap_or_default();
                self.r.start_daemon(&env)?;
            }
            "tab-to" => {
                // Tab (or Shift+Tab) until `on` has focus, checking a focus ring is drawn at every stop on the way.
                let back = v["back"].as_bool().unwrap_or(false);
                let max = v["max"].as_u64().unwrap_or(20);
                let mut stops = Vec::new();
                for _ in 0..max {
                    self.key(KeyEvent { key: Key::Tab, mods: Mods { shift: back, ..Mods::default() } })?;
                    let all = self.rt()?.elements();
                    let Some(f) = all.iter().find(|e| e.focused && !e.exiting) else {
                        stops.push("(nothing)".to_owned());
                        continue;
                    };
                    stops.push(f.key.clone().or(f.label.clone()).unwrap_or_else(|| f.role.to_owned()));
                    if !f.focus_visible {
                        return Err(format!("no focus ring at {}; stops: {}", describe(f), stops.join(" → ")));
                    }
                    if matches(&all, f, &v["on"]) {
                        self.r.say(&format!("tab stops: {}", stops.join(" → ")));
                        return Ok(None);
                    }
                }
                return Err(format!("{} not reached in {max} presses; stops: {}", v["on"], stops.join(" → ")));
            }
            "allow-until" => {
                // Answers every request with Allow once until `until` holds (a real agent may ask more than once).
                let end = Instant::now() + Duration::from_secs_f64(within);
                let mut allowed = 0;
                loop {
                    self.pump();
                    if self.check(&v["until"]).is_ok() {
                        break;
                    }
                    let any = self.rt()?.elements().iter().any(|e| !e.exiting && e.key.as_deref().is_some_and(|k| glob("perm-*-allow", k)));
                    if any {
                        if allowed == 0 && v["snap_first"].as_bool() == Some(true) {
                            let _ = self.snap(&format!("{}-request", step.stem()));
                        }
                        self.click(&json!({"key": "perm-*-allow"}))?;
                        allowed += 1;
                        self.sleep(0.3);
                    }
                    if Instant::now() > end {
                        return Err(format!("after {within} s ({allowed} allowed): {}", self.check(&v["until"]).err().unwrap_or_default()));
                    }
                    std::thread::sleep(Duration::from_millis(30));
                }
                self.r.say(&format!("allowed {allowed} request(s)"));
            }
            "run" => {
                // A person's own change on disk (an edit by hand), inside the run's folder only.
                let cmd = v["cmd"].as_str().unwrap_or("");
                let st = Command::new("/bin/sh").arg("-c").arg(cmd).current_dir(&self.r.places.work).env("HOME", &self.r.places.home).env("GIT_CONFIG_NOSYSTEM", "1").env("GIT_CONFIG_GLOBAL", "/dev/null").status().map_err(|e| e.to_string())?;
                if !st.success() {
                    return Err(format!("`{}` ended with {st}", short(cmd)));
                }
            }
            "stale-answer" => {
                // A second answer to a question already answered, sent as the app sends one (follow_up naming the run it answers),
                // straight to the daemon: it must be refused.
                use neko_protocol::{Reply, Request};
                let (info, token) = neko_protocol::client::find(&self.r.places.data).map_err(|e| e.to_string())?;
                let mut c = neko_protocol::client::Client::connect(info.port, &token, "neko-qa").map_err(|e| e.to_string())?;
                let want = v["task"].as_str().unwrap_or("");
                let tasks = match c.request(Request::ListTasks).map_err(|e| e.to_string())? {
                    Reply::Tasks { tasks } => tasks,
                    other => return Err(format!("list_tasks: {other:?}")),
                };
                let task = tasks.into_iter().find(|t| t.title.starts_with(want)).ok_or(format!("no task titled {want:?}"))?;
                let runs = match c.request(Request::GetTranscript { task: task.id.clone() }).map_err(|e| e.to_string())? {
                    Reply::Transcript { transcript } => transcript.runs,
                    other => return Err(format!("get_transcript: {other:?}")),
                };
                let asked = runs.iter().rev().nth(1).ok_or("the task has no earlier run")?.id.clone();
                let reply = c.request(Request::FollowUp { task: task.id, words: v["words"].as_str().unwrap_or("Again").to_owned(), answering: Some(asked) }).map_err(|e| e.to_string())?;
                c.close();
                match reply {
                    Reply::Error { message, .. } => self.r.say(&format!("refused: {message}")),
                    other => return Err(format!("a second answer was taken: {other:?}")),
                }
            }
            "clock" => {
                // The rounds scheduler's clock, `minutes` ahead of the real one (a debug daemon started with NEKO_DEBUG_CLOCK=1 only).
                use neko_protocol::{Reply, Request};
                let (info, token) = neko_protocol::client::find(&self.r.places.data).map_err(|e| e.to_string())?;
                let mut c = neko_protocol::client::Client::connect(info.port, &token, "neko-qa").map_err(|e| e.to_string())?;
                let offset_ms = v["minutes"].as_i64().map(|m| m * 60_000);
                let reply = c.request(Request::DebugClock { offset_ms }).map_err(|e| e.to_string())?;
                c.close();
                if reply != Reply::Ok {
                    return Err(format!("debug_clock: {reply:?}"));
                }
                self.r.say(&format!("the scheduler's clock is {} minutes ahead", v["minutes"].as_i64().unwrap_or(0)));
            }
            "dump" => {
                let rt = self.rt()?;
                let lines: Vec<String> = rt.elements().iter().filter(|e| !e.exiting && (e.key.is_some() || e.text.is_some() || e.label.is_some())).map(describe).collect();
                std::fs::write(self.r.o.out.join(format!("tree/{}-elements.txt", step.stem())), lines.join("\n")).map_err(|e| e.to_string())?;
            }
            other => return Err(format!("unknown step {other:?}")),
        }
        // A check after an action, if the step asks for one.
        if let Some(c) = v.get("then") {
            self.wait(c, within)?;
        }
        Ok(None)
    }
}

fn headless(r: &mut Run) -> Result<(), String> {
    let video = r.o.out.join("video.mp4");
    let ffmpeg_log = r.places.work.join("ffmpeg.log");
    let (rec, pid) = Recorder::new(r.s.size, 2.0, Some(&video), FPS, &ffmpeg_log)?;
    if let Some(p) = pid {
        r.started("ffmpeg", p);
    }
    let rt = new_app(r);
    let steps: Vec<Step> = std::mem::take(&mut r.s.steps);
    let mut h = Headless { r, rt: Some(rt), rec, last: Instant::now(), marks: BTreeMap::new() };
    // The app connects to the daemon before the first step, as a person would see it after launch.
    let _ = h.wait(&json!({"daemon": "connected"}), 15.0);
    let mut failed = false;
    for step in &steps {
        if failed {
            if step.expect().is_some() {
                h.r.outcome(step, None, "not reached: an earlier step failed".into(), 0.0, None);
            }
            continue;
        }
        let t = Instant::now();
        match h.step(step) {
            Ok(pic) => {
                if step.expect().is_some() {
                    let _ = h.tree(&step.stem());
                    let pic = pic.or_else(|| step.v["snap"].as_bool().filter(|b| *b).and_then(|_| h.snap(&step.stem()).ok()));
                    h.r.outcome(step, Some(true), String::new(), t.elapsed().as_secs_f64(), pic);
                }
            }
            Err(e) => {
                let stem = format!("{}-FAILED", step.stem());
                let pic = h.snap(&stem).ok();
                let _ = h.step(&Step { n: step.n, v: json!({"do": "dump", "name": "FAILED"}) });
                h.r.outcome(step, Some(false), e, t.elapsed().as_secs_f64(), pic);
                // A soft check records its failure and the journey goes on.
                failed = step.v["soft"].as_bool() != Some(true);
            }
        }
    }
    if h.rt.is_some() {
        h.sleep(0.5);
        let _ = h.snap(&format!("{:02}-end", steps.len() + 1));
    }
    h.r.s.steps = steps;
    let mut rec = h.rec;
    // The app's link closes before the daemon stops.
    drop(h.rt.take());
    rec.finish()
}

// ---------------------------------------------------------------------------------------------------------------------------
// Selectors

/// Whether `pattern` (with `*` for any run of characters) matches `s` whole.
pub fn glob(pattern: &str, s: &str) -> bool {
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == s;
    }
    let mut rest = s;
    for (i, p) in parts.iter().enumerate() {
        if i == 0 {
            let Some(r) = rest.strip_prefix(p) else { return false };
            rest = r;
        } else if i == parts.len() - 1 {
            return rest.ends_with(p);
        } else {
            let Some(at) = rest.find(p) else { return false };
            rest = &rest[at + p.len()..];
        }
    }
    true
}

fn matches(all: &[ElementInfo], e: &ElementInfo, sel: &Value) -> bool {
    if e.exiting {
        return false;
    }
    let has = |field: &Option<String>, want: &str| field.as_deref().is_some_and(|t| t.contains(want));
    if let Some(k) = sel["key"].as_str()
        && !e.key.as_deref().is_some_and(|key| glob(k, key))
    {
        return false;
    }
    if let Some(t) = sel["text"].as_str()
        && !has(&e.text, t)
    {
        return false;
    }
    if let Some(t) = sel["label"].as_str()
        && !has(&e.label, t)
    {
        return false;
    }
    if let Some(t) = sel["label_is"].as_str()
        && e.label.as_deref() != Some(t)
    {
        return false;
    }
    if let Some(t) = sel["name"].as_str()
        && !(has(&e.text, t) || has(&e.label, t) || has(&e.description, t))
    {
        return false;
    }
    if let Some(t) = sel["role"].as_str()
        && e.role != t
    {
        return false;
    }
    if let Some(f) = sel["focused"].as_bool()
        && e.focused != f
    {
        return false;
    }
    if let Some(inside) = sel.get("in") {
        let mut p = e.parent;
        let mut found = false;
        while let Some(i) = p {
            if all.get(i).is_some_and(|a| matches(all, a, inside)) {
                found = true;
                break;
            }
            p = all.get(i).and_then(|a| a.parent);
        }
        if !found {
            return false;
        }
    }
    true
}

fn matching<'a>(all: &'a [ElementInfo], sel: &Value) -> Vec<&'a ElementInfo> {
    all.iter().filter(|e| matches(all, e, sel)).collect()
}

/// The element `sel` names (its `nth` match, the first by default); Err says what is on screen instead.
fn pick<'a>(all: &'a [ElementInfo], sel: &Value) -> Result<&'a ElementInfo, String> {
    let found = matching(all, sel);
    let nth = sel["nth"].as_u64().unwrap_or(0) as usize;
    found.get(nth).copied().ok_or_else(|| {
        let texts: Vec<String> = all.iter().filter(|e| !e.exiting).filter_map(|e| e.text.clone().filter(|t| !t.trim().is_empty())).map(|t| cut(&t, 60)).take(40).collect();
        format!("nothing matches {sel} ({} found); on screen: {}", found.len(), texts.join(" | "))
    })
}

fn describe(e: &ElementInfo) -> String {
    let mut s = format!("[{}]", e.role);
    if let Some(k) = &e.key {
        s.push_str(&format!(" key={k}"));
    }
    if let Some(l) = &e.label {
        s.push_str(&format!(" label={:?}", cut(l, 60)));
    }
    if let Some(t) = &e.text {
        s.push_str(&format!(" text={:?}", cut(t, 60)));
    }
    if e.focused {
        s.push_str(if e.focus_visible { " (focused, ring)" } else { " (focused, no ring)" });
    }
    s
}

pub fn cut(s: &str, n: usize) -> String {
    let one: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if one.chars().count() <= n { one } else { format!("{}…", one.chars().take(n).collect::<String>()) }
}

/// A path or command with the person's home as `~`.
pub fn short(s: &str) -> String {
    match std::env::var("HOME") {
        Ok(h) if !h.is_empty() => s.replace(&h, "~"),
        _ => s.to_owned(),
    }
}

/// `return`, `escape`, `tab`, `shift+tab`, `up`, `down`, `space`, `cmd+n`, `ctrl+cmd+s`, `cmd+,`.
pub fn parse_key(s: &str) -> Result<KeyEvent, String> {
    let mut mods = Mods::default();
    let parts: Vec<&str> = s.split('+').collect();
    let (name, held) = parts.split_last().ok_or("an empty key")?;
    for m in held {
        match *m {
            "cmd" => mods.cmd = true,
            "ctrl" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" | "option" => mods.alt = true,
            other => return Err(format!("unknown modifier {other}")),
        }
    }
    let key = match *name {
        "return" | "enter" => Key::Enter,
        "escape" | "esc" => Key::Escape,
        "tab" => Key::Tab,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        "space" => Key::Text(" ".into()),
        "backspace" => Key::Backspace,
        c if c.chars().count() == 1 && mods.cmd => Key::Command(c.chars().next().unwrap_or(' ').to_ascii_lowercase()),
        c if c.chars().count() == 1 => Key::Text(c.to_owned()),
        other => return Err(format!("unknown key {other}")),
    };
    Ok(KeyEvent { key, mods })
}

/// HEAD, the branch and the status of a checkout, as one string.
pub fn git_state(repo: &str, home: &Path) -> Result<String, String> {
    let dir = Path::new(repo);
    let head = scenario::git(dir, home, &["rev-parse", "HEAD"])?;
    let branch = scenario::git(dir, home, &["branch", "--show-current"])?;
    let status = scenario::git(dir, home, &["status", "--porcelain"])?;
    Ok(format!("HEAD {} on {} status [{}]", head.trim(), branch.trim(), status.trim()))
}

// ---------------------------------------------------------------------------------------------------------------------------
// Results

pub fn utc_now() -> String {
    let out = Command::new("date").args(["-u", "+%Y-%m-%dT%H:%M:%SZ"]).output();
    out.map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

fn write_results(r: &Run, started_utc: &str, usage: Option<Value>) -> Result<bool, String> {
    let checks: Vec<&Outcome> = r.outcomes.iter().collect();
    let picture_only = |c: &Outcome| c.ok.is_none() && (c.reason.starts_with("picture only") || c.reason.starts_with("needs the person once"));
    let passed = !checks.is_empty() && checks.iter().all(|c| c.ok == Some(true) || picture_only(c)) && r.notes.is_empty();
    let mode = if r.o.window { "window" } else { "headless" };
    let cli = if r.o.real { "real" } else { "fake" };
    let result = json!({
        "scenario": r.s.name,
        "title": r.s.title,
        "commit": r.o.commit,
        "cli": cli,
        "mode": mode,
        "passed": passed,
        "started_utc": started_utc,
        "duration_s": (r.began.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "expectations": checks.iter().map(|c| json!({"step": c.step, "expect": c.expect, "result": match c.ok { Some(true) => "pass", Some(false) => "fail", None if picture_only(c) => if c.reason.starts_with("needs") { "needs the person once" } else { "picture only" }, None => "not reached" }, "reason": c.reason, "secs": (c.secs * 100.0).round() / 100.0, "picture": c.picture})).collect::<Vec<_>>(),
        "notes": r.notes,
        "info": r.notes_info,
        "usage": usage,
        "video": if r.o.out.join("video.mp4").is_file() { Some("video.mp4") } else { None },
    });
    std::fs::write(r.o.out.join("result.json"), serde_json::to_vec_pretty(&result).unwrap_or_default()).map_err(|e| e.to_string())?;
    // The human summary.
    let mut md = format!("# {} — {}\n\n{}\n\n", r.s.name, r.s.title, r.s.about);
    md.push_str(&format!("Commit `{}`, CLI {cli}, {mode}, started {started_utc}, took {:.1} s. **{}**\n\n", r.o.commit, r.began.elapsed().as_secs_f64(), if passed { "PASSED" } else { "FAILED" }));
    md.push_str("| Step | Expectation | Result | Picture |\n| --- | --- | --- | --- |\n");
    for c in &checks {
        let res = match c.ok {
            Some(true) => "pass".to_owned(),
            Some(false) => format!("**FAIL**: {}", c.reason.replace('|', "\\|")),
            None if picture_only(c) => if c.reason.starts_with("needs") { "needs the person once".to_owned() } else { "picture only".to_owned() },
            None => "not reached".to_owned(),
        };
        md.push_str(&format!("| {:02} | {} | {} | {} |\n", c.step, c.expect.replace('|', "\\|"), res, c.picture.clone().unwrap_or_default()));
    }
    for n in r.notes.iter().chain(&r.notes_info) {
        md.push_str(&format!("\n- {n}"));
    }
    if let Some(u) = &usage {
        md.push_str(&format!("\n\nReal runs reported: {} run(s), {} tokens in, {} out, cost {} (as Claude Code reports it).\n", u["runs"], u["input_tokens"], u["output_tokens"], u["cost_usd"]));
    }
    md.push('\n');
    std::fs::write(r.o.out.join("report.md"), md).map_err(|e| e.to_string())?;
    // The log: the driver's, the daemon's, ffmpeg's; the person's home as `~`.
    let mut log = String::new();
    for (name, path) in [("driver", r.o.out.join("driver.log")), ("daemon", r.places.work.join("daemon.log")), ("app (window)", r.places.work.join("app.log")), ("notifications the app posted (window)", r.places.work.join("notify.jsonl")), ("ffmpeg", r.places.work.join("ffmpeg.log"))] {
        if let Ok(t) = std::fs::read_to_string(&path) {
            log.push_str(&format!("==== {name} ====\n{}\n", short(&t)));
        }
    }
    let _ = std::fs::remove_file(r.o.out.join("driver.log"));
    std::fs::write(r.o.out.join("log.txt"), log).map_err(|e| e.to_string())?;
    println!("{} {}: {}", if passed { "PASS" } else { "FAIL" }, r.s.name, r.o.out.display());
    Ok(passed)
}
