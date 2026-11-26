//! `--window`: the real app in a real window, on the run's temporary folders (docs/knowledge/qa.md).
//!
//! The app binary is hard-linked under its own name (so its Dock item is told apart from any other Neko) and launched with
//! `RUNE_OPEN_INACTIVE` (it takes no focus), `RUNE_DIRS_ROOT` (its prefs stay in the run's folder) and `NEKO_DATA_DIR`. The
//! scenario becomes a `--window-script` that Rune replays on the wall clock: typing, keys and taps on fixed keys. Waits become
//! pauses (a window script cannot wait on what happens). Marks in the script tell this side when to act: capture the window
//! (`screencapture -l`, that window only, even behind others), read what the window says to assistive technology (to check an
//! expectation by its words), or press a control whose key is made at run time by its accessible name (AXPress, no pointer, no
//! focus). The video is made of window captures taken twice a second, never of a screen rectangle that could show other windows.
//! At the end the Dock menu is read once and the window must have been on screen.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use super::scenario::Step;
use super::{Run, cut, glob};

/// The name the window's binary runs under (its Dock item and process name).
const NAME: &str = "neko-qa-win";
/// How long a scenario's wait pauses in a window, when it gives no `window_pause`.
const PAUSE: f64 = 3.0;
/// The pause after a press by accessible name (reading the window's tree takes a few seconds the first time).
const PRESS_PAUSE: f64 = 6.0;
/// The pause after a check (reading the window's tree).
const CHECK_PAUSE: f64 = 2.5;

fn scripts() -> PathBuf {
    std::env::var_os("NEKO_QA_SCRIPTS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("scripts/qa"))
}

fn osascript_js(script: &str, args: &[&str]) -> Result<String, String> {
    let out = Command::new("osascript").arg("-l").arg("JavaScript").arg(scripts().join(script)).args(args).output().map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(format!("{script}: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// The app's normal window: (id, x, y, w, h, onscreen, title).
fn window_of(pid: u32) -> Option<(u64, f64, f64, f64, f64, bool, String)> {
    let text = osascript_js("window-info.js", &[&pid.to_string()]).ok()?;
    let list: Vec<Value> = serde_json::from_str(&text).ok()?;
    let w = list.iter().filter(|w| w["layer"] == 0 && w["w"].as_f64().unwrap_or(0.0) > 200.0).max_by_key(|w| (w["w"].as_f64().unwrap_or(0.0) * w["h"].as_f64().unwrap_or(0.0)) as u64)?;
    Some((w["id"].as_u64()?, w["x"].as_f64()?, w["y"].as_f64()?, w["w"].as_f64()?, w["h"].as_f64()?, w["onscreen"].as_bool().unwrap_or(false), w["title"].as_str().unwrap_or("").to_owned()))
}

fn capture(id: u64, path: &Path) -> Result<(), String> {
    let st = Command::new("screencapture").args(["-x", "-o", "-l"]).arg(id.to_string()).arg(path).status().map_err(|e| e.to_string())?;
    if st.success() && path.is_file() { Ok(()) } else { Err(format!("screencapture -l {id} failed")) }
}

/// One planned event in the window script, or a mark this side acts on.
enum Plan {
    Events(Vec<Value>),
    Mark(String, Action),
}

#[derive(Clone)]
enum Action {
    Snap,
    Check,
    Press(String),
}

/// A key as the window script names it.
fn script_key(k: &str) -> Result<Value, String> {
    let parts: Vec<&str> = k.split('+').collect();
    let (name, held) = parts.split_last().ok_or("an empty key")?;
    let mut e = json!({"type": "key"});
    for m in held {
        e[*m] = json!(true);
    }
    let name = match *name {
        "return" | "enter" => "Enter",
        "escape" | "esc" => "Escape",
        "tab" => "Tab",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "space" => " ",
        "backspace" => "Backspace",
        other => other,
    };
    e["name"] = json!(name);
    Ok(e)
}

/// The accessible name to press for a click whose key is made at run time.
fn press_name(sel: &Value) -> Option<String> {
    if let Some(k) = sel["key"].as_str()
        && k.contains('*')
    {
        for (suffix, name) in [("-allow", "Allow once"), ("-always", "Always allow"), ("-deny", "Deny")] {
            if glob(&format!("*{suffix}"), k) {
                return Some(name.to_owned());
            }
        }
    }
    sel["name"].as_str().or(sel["label_is"].as_str()).or(sel["label"].as_str()).map(str::to_owned)
}

/// The scenario as a window script and marks. Steps a window cannot do are left out and said so.
fn plan(r: &mut Run, steps: &[Step]) -> Result<(Vec<Plan>, Vec<(String, bool)>, Vec<String>), String> {
    let mut out = Vec::new();
    let mut folders: Vec<(String, bool)> = Vec::new();
    let mut left_out = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        let v = &s.v;
        if v["window"].as_bool() == Some(false) {
            left_out.push(format!("step {} ({})", s.n, s.kind()));
            continue;
        }
        match s.kind() {
            "click" => match v["on"]["key"].as_str().filter(|k| !k.contains('*')) {
                Some(k) if v["on"].get("name").is_none() && v["on"].get("label").is_none() && v["on"].get("label_is").is_none() => out.push(Plan::Events(vec![json!({"type": "tap", "key": k}), json!({"pause": 0.6})])),
                _ => {
                    let name = press_name(&v["on"]).ok_or(format!("step {}: no accessible name to press for {}", s.n, v["on"]))?;
                    out.push(Plan::Mark(format!("press:{i}"), Action::Press(name)));
                }
            },
            "type" => out.push(Plan::Events(vec![json!({"type": "type", "text": v["text"], "every": 0.03}), json!({"pause": 0.1 + 0.03 * v["text"].as_str().unwrap_or("").chars().count() as f64})])),
            "key" => {
                for _ in 0..v["times"].as_u64().unwrap_or(1) {
                    out.push(Plan::Events(vec![script_key(v["key"].as_str().unwrap_or(""))?, json!({"pause": 0.4})]));
                }
            }
            "sleep" => out.push(Plan::Events(vec![json!({"pause": v["secs"].as_f64().unwrap_or(1.0)})])),
            "snap" => out.push(Plan::Mark(format!("snap:{i}"), Action::Snap)),
            "wait" | "assert" => {
                let pause = v["window_pause"].as_f64().unwrap_or(if s.kind() == "wait" { PAUSE.min(v["within"].as_f64().unwrap_or(PAUSE)) } else { 0.2 });
                out.push(Plan::Events(vec![json!({"pause": pause})]));
                if s.expect().is_some() {
                    out.push(Plan::Mark(format!("check:{i}"), Action::Check));
                }
            }
            "open-link" => {
                out.push(Plan::Mark(format!("link:{i}"), Action::Check));
                out.push(Plan::Events(vec![json!({"pause": v["window_pause"].as_f64().unwrap_or(PAUSE)})]));
            }
            "cold-link" => {}
            "allow-until" => {
                out.push(Plan::Events(vec![json!({"pause": v["window_pause"].as_f64().unwrap_or(PAUSE)})]));
                out.push(Plan::Mark(format!("press:{i}"), Action::Press("Allow once".into())));
                out.push(Plan::Events(vec![json!({"pause": PAUSE})]));
                out.push(Plan::Mark(format!("check:{i}"), Action::Check));
            }
            "choose-folder" | "add-workspace" if out.iter().all(|p| !matches!(p, Plan::Events(e) if e.iter().any(|x| x["type"] == "key" && x["name"] == "Enter"))) => {
                // The folder panel would take focus: the folder is added before launch instead (and chosen, for choose-folder).
                folders.push((v["path"].as_str().unwrap_or("").to_owned(), s.kind() == "choose-folder"));
            }
            "git-record" => {
                let repo = v["repo"].as_str().unwrap_or("").to_owned();
                let st = super::git_state(&repo, &r.places.home)?;
                r.notes_marks.insert(v["as"].as_str().unwrap_or("before").to_owned(), format!("{repo}\n{st}"));
            }
            other => left_out.push(format!("step {} ({other})", s.n)),
        }
    }
    Ok((out, folders, left_out))
}

/// Turns the plan into the script's events, with times.
fn script(plan: &[Plan]) -> (Vec<Value>, f64) {
    let mut t = 1.5;
    let mut events = Vec::new();
    for p in plan {
        match p {
            Plan::Events(list) => {
                for e in list {
                    if let Some(pause) = e["pause"].as_f64() {
                        t += pause;
                        continue;
                    }
                    let mut e = e.clone();
                    e["t"] = json!((t * 1000.0).round() / 1000.0);
                    events.push(e);
                }
            }
            Plan::Mark(name, a) => {
                events.push(json!({"t": t, "type": "mark", "name": name}));
                t += match a {
                    Action::Press(_) => PRESS_PAUSE,
                    Action::Check => CHECK_PAUSE,
                    Action::Snap => 0.8,
                };
            }
        }
    }
    t += 1.0;
    events.push(json!({"t": t, "type": "mark", "name": "end"}));

    (events, t)
}

/// What the window says to assistive technology: each element's words.
fn ax_words(pid: u32) -> Vec<String> {
    // Reading the tree can fail while the window changes under it: once more after a moment.
    let text = osascript_js("ax-texts.js", &[&pid.to_string()]).ok().filter(|t| t.len() > 2).or_else(|| {
        std::thread::sleep(Duration::from_millis(500));
        osascript_js("ax-texts.js", &[&pid.to_string()]).ok()
    });
    let Some(text) = text else { return Vec::new() };
    let list: Vec<Value> = serde_json::from_str(&text).unwrap_or_default();
    list.iter().map(|e| [e["name"].as_str(), e["desc"].as_str(), e["value"].as_str()].iter().flatten().copied().collect::<Vec<_>>().join(" ")).filter(|s| !s.trim().is_empty()).collect()
}

/// Checks a condition by the window's words. Ok(Some(())) held, Err failed, Ok(None) not checkable by words.
fn check_words(c: &Value, words: &[String], title: &str) -> Result<Option<()>, String> {
    if let Some(list) = c["all"].as_array() {
        let mut any = false;
        for x in list {
            if check_words(x, words, title)?.is_some() {
                any = true;
            }
        }
        return Ok(any.then_some(()));
    }
    let wants = |sel: &Value| -> Vec<String> { ["text", "label", "label_is", "name"].iter().filter_map(|k| sel[*k].as_str().map(str::to_owned)).collect() };
    let seen = |w: &[String]| words.iter().any(|x| w.iter().all(|want| x.contains(want.as_str())));
    if let Some(sel) = c.get("see") {
        let w = wants(sel);
        if w.is_empty() {
            return Ok(None);
        }
        return if seen(&w) { Ok(Some(())) } else { Err(format!("the window does not say {:?}; it says: {}", w.join(" "), cut(&words.join(" | "), 400))) };
    }
    if let Some(sel) = c.get("gone") {
        let w = wants(sel);
        if w.is_empty() || sel.get("in").is_some() {
            return Ok(None);
        }
        return if seen(&w) { Err(format!("the window still says {:?}", w.join(" "))) } else { Ok(Some(())) };
    }
    if let Some(sel) = c.get("count") {
        let w = wants(&sel["of"]);
        if w.is_empty() || sel["of"].get("in").is_some() || sel["of"].get("role").is_some() {
            return Ok(None);
        }
        let n = words.iter().filter(|x| w.iter().all(|want| x.contains(want.as_str()))).count() as u64;
        let min = sel["min"].as_u64().or(sel["is"].as_u64()).unwrap_or(0);
        return if n >= min { Ok(Some(())) } else { Err(format!("the window says {:?} {n} time(s)", w.join(" "))) };
    }
    if let Some(t) = c["title"].as_str() {
        return if title == t { Ok(Some(())) } else { Err(format!("the window's title is {title:?}")) };
    }
    Ok(None)
}

pub fn run(r: &mut Run) -> Result<(), String> {
    let app = r.o.app.clone().ok_or("--window needs --app")?;
    let steps: Vec<Step> = std::mem::take(&mut r.s.steps);
    let (plan, folders, left_out) = plan(r, &steps)?;
    if !left_out.is_empty() {
        r.notes_info.push(format!("Not replayed in a window (headless covers them): {}", left_out.join(", ")));
    }
    let (events, end_t) = script(&plan);
    let script_path = r.places.work.join("window-script.json");
    std::fs::write(&script_path, serde_json::to_vec_pretty(&events).unwrap_or_default()).map_err(|e| e.to_string())?;
    let rune_root = r.places.work.join("rune");
    // The folder panel would take focus: folders are added as workspaces by the daemon's own request before launch, and a chosen
    // one is made the composer's choice where the app keeps it (its prefs, in the run's folder).
    for (f, choose) in &folders {
        let (info, token) = neko_protocol::client::find(&r.places.data).map_err(|e| e.to_string())?;
        let mut c = neko_protocol::client::Client::connect(info.port, &token, "neko-qa").map_err(|e| e.to_string())?;
        let id = match c.request(neko_protocol::Request::AddWorkspace { folder: f.clone() }).map_err(|e| e.to_string())? {
            neko_protocol::Reply::Workspace { workspace, .. } => workspace.id,
            other => return Err(format!("adding the workspace: {other:?}")),
        };
        c.close();
        r.say(&format!("added workspace {} before launch (the daemon's add_workspace)", super::short(f)));
        if *choose {
            let dir = rune_root.join("config").join(neko_app::PREFS_ID);
            std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            std::fs::write(dir.join("prefs.json"), serde_json::to_vec(&json!({ neko_app::WORKSPACE_KEY: id })).unwrap_or_default()).map_err(|e| e.to_string())?;
        }
    }
    let log_path = r.places.work.join("app.log");
    let notify_log = r.places.work.join("notify.jsonl");
    let (tx, rx) = mpsc::channel::<String>();
    let launched = Instant::now();
    let (mut child, pid, dock_name): (Option<std::process::Child>, u32, String) = match std::env::var_os("NEKO_QA_BUNDLE").map(PathBuf::from).filter(|_| r.s.bundle || std::env::var_os("NEKO_QA_ALL_BUNDLE").is_some()) {
        // A development bundle (scripts/build-app.sh --dev into the run's own folder, renamed "Neko QA.app" so its Dock item is
        // told apart from any other Neko): only a bundle gets notifications, the Dock badge, the link and a proper Dock item.
        // `open -n -g` starts a new copy in the background; its PID is found by the exact path of its executable.
        Some(bundle) => {
            let exe = bundle.join("Contents/MacOS/neko");
            let out = r.places.work.join("app.out");
            std::fs::write(&out, "").map_err(|e| e.to_string())?;
            let mut c = Command::new("open");
            // `-g` keeps it in the background only where the scenario asks for an inactive window.
            c.arg("-n");
            if r.s.inactive {
                c.arg("-g");
            }
            c.arg(&bundle).arg("--stdout").arg(&out).arg("--stderr").arg(&out);
            if r.s.inactive {
                c.arg("--env").arg("RUNE_OPEN_INACTIVE=1");
            }
            for (k, v) in [("RUNE_DIRS_ROOT", rune_root.display().to_string()), ("NEKO_DATA_DIR", r.places.data.display().to_string()), ("NEKO_NOTIFY_LOG", notify_log.display().to_string())] {
                c.arg("--env").arg(format!("{k}={v}"));
            }
            c.arg("--args").arg("--window-script").arg(&script_path);
            let st = c.status().map_err(|e| format!("open: {e}"))?;
            if !st.success() {
                return Err(format!("open ended with {st}"));
            }
            let mut pid = None;
            while pid.is_none() && launched.elapsed() < Duration::from_secs(15) {
                pid = pid_of(&exe);
                std::thread::sleep(Duration::from_millis(50));
            }
            let pid = pid.ok_or("the bundle's process did not appear")?;
            follow_file(out, tx);
            let name = bundle.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Neko".into());
            (None, pid, name)
        }
        None => {
            // The bare binary, hard-linked under its own name (its Dock item and process are told apart from any other Neko).
            let bin = r.places.work.join(NAME);
            std::fs::hard_link(&app, &bin).or_else(|_| std::fs::copy(&app, &bin).map(|_| ())).map_err(|e| format!("{}: {e}", bin.display()))?;
            let mut cmd = Command::new(&bin);
            if r.s.inactive {
                cmd.env("RUNE_OPEN_INACTIVE", "1");
            }
            let mut ch = cmd
                .arg("--window-script")
                .arg(&script_path)
                .env("RUNE_DIRS_ROOT", &rune_root)
                .env("NEKO_DATA_DIR", &r.places.data)
                .env("NEKO_DAEMON_BIN", &r.o.daemon)
                .env("NEKO_NOTIFY_LOG", &notify_log)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| format!("the app could not start: {e}"))?;
            let stdout = ch.stdout.take().ok_or("no stdout")?;
            let stderr = ch.stderr.take().ok_or("no stderr")?;
            let tx2 = tx.clone();
            std::thread::spawn(move || {
                for l in BufReader::new(stdout).lines().map_while(Result::ok) {
                    let _ = tx.send(l);
                }
            });
            std::thread::spawn(move || {
                for l in BufReader::new(stderr).lines().map_while(Result::ok) {
                    let _ = tx2.send(l);
                }
            });
            let pid = ch.id();
            (Some(ch), pid, NAME.to_owned())
        }
    };
    r.started("neko (window)", pid);
    r.say(&format!("app started in a window (pid {pid}), script {:.0} s", end_t));
    let mut log = std::fs::File::create(&log_path).map_err(|e| e.to_string())?;

    // The window, and how long it took to appear.
    let mut win = None;
    while launched.elapsed() < Duration::from_secs(20) {
        if let Some(w) = window_of(pid).filter(|w| w.5) {
            win = Some(w);
            break;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let appeared = launched.elapsed().as_secs_f64();
    // An inactive window (the scenario asks, e.g. notifications) must not take focus. The others open active, as a person (and
    // VoiceOver) uses the window: an inactive window's accessibility tree does not follow its changes (docs/knowledge/qa.md).
    let launch_step = Step { n: 0, v: json!({"do": "launch", "expect": if r.s.inactive { "The window opens on screen, without taking focus" } else { "The window opens on screen" }}) };
    let Some((id, x, y, w, h, _, _)) = win else {
        r.outcome(&launch_step, Some(false), "no window on screen within 20 s".into(), appeared, None);
        stop(&mut child, pid);
        return Err("no window".into());
    };
    let frontmost = Command::new("osascript").arg("-e").arg(format!("tell application \"System Events\" to get frontmost of (first application process whose unix id is {pid})")).output().map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true").unwrap_or(false);
    let on_display = x + w > 0.0 && y + h > 0.0;
    r.outcome(
        &launch_step,
        Some(on_display && !(r.s.inactive && frontmost)),
        if !on_display { format!("the window is off every display at {x},{y}") } else if r.s.inactive && frontmost { "the app took focus".into() } else { String::new() },
        appeared,
        None,
    );
    r.say(&format!("window {id} at {x},{y} {w}x{h} after {appeared:.2} s"));

    // Captures twice a second for the video, of this window only.
    let frames = r.places.work.join("frames");
    std::fs::create_dir_all(&frames).map_err(|e| e.to_string())?;
    let done = Arc::new(AtomicBool::new(false));
    let shooter = {
        let (done, frames) = (done.clone(), frames.clone());
        std::thread::spawn(move || {
            let mut n = 0u32;
            while !done.load(Ordering::SeqCst) {
                let t = Instant::now();
                if capture(id, &frames.join(format!("f{n:05}.png"))).is_ok() {
                    n += 1;
                }
                std::thread::sleep(Duration::from_millis(500).saturating_sub(t.elapsed()));
            }
            n
        })
    };

    // Follow the script: act on each mark as it passes.
    let deadline = Instant::now() + Duration::from_secs_f64(end_t + 60.0);
    let mut first_frame = None;
    let mut script_errors = Vec::new();
    let mut finished = false;
    let mut failed_at: Option<usize> = None;
    while Instant::now() < deadline {
        let Ok(line) = rx.recv_timeout(Duration::from_millis(200)) else {
            if exited(&mut child, pid) {
                break;
            }
            continue;
        };
        use std::io::Write;
        let _ = writeln!(log, "{line}");
        if let Some(ms) = line.strip_prefix("rune: first frame took ").and_then(|s| s.split(' ').next()) {
            first_frame = Some(ms.to_owned());
        }
        if line.starts_with("rune-script-error") {
            script_errors.push(line.clone());
        }
        let Some(name) = line.strip_prefix("rune-mark ").and_then(|s| s.split(' ').next()) else { continue };
        if name == "end" {
            finished = true;
            continue;
        }
        let (kind, i) = name.split_once(':').unwrap_or((name, "0"));
        let i: usize = i.parse().unwrap_or(0);
        let Some(step) = steps.get(i) else { continue };
        if failed_at.is_some() && kind != "snap" {
            continue;
        }
        let t = Instant::now();
        match kind {
            "snap" => {
                let rel = format!("steps/{}.png", step.stem());
                match capture(id, &r.o.out.join(&rel)) {
                    Ok(()) => r.outcome(step, Some(true), String::new(), 0.0, Some(rel)),
                    Err(e) => r.outcome(step, Some(false), e, 0.0, None),
                }
            }
            "press" => {
                let Some(Plan::Mark(_, Action::Press(label))) = plan.iter().find(|p| matches!(p, Plan::Mark(n, _) if n == name)) else { continue };
                let label = label.clone();
                match osascript_js("ax-press.js", &[&pid.to_string(), &label]) {
                    Ok(s) if s == "pressed" => r.say(&format!("pressed {label:?} (AXPress)")),
                    other => {
                        let rel = format!("steps/{}-FAILED.png", step.stem());
                        let _ = capture(id, &r.o.out.join(&rel));
                        let why = match other {
                            Err(e) => format!("pressing {label:?} by its accessible name failed: {e}"),
                            Ok(s) => format!("no control named {label:?} to press in the window ({})", cut(&s, 300)),
                        };
                        r.outcome(step, Some(false), why, t.elapsed().as_secs_f64(), Some(rel));
                        failed_at = Some(i);
                    }
                }
            }
            "link" => {
                // The task's link, opened by this bundle (`open -a`, so no other Neko on the Mac takes it). This brings Neko to the front.
                let ok = link_target(r, step).and_then(|url| {
                    let bundle = std::env::var("NEKO_QA_BUNDLE").map_err(|_| "links need the bundle".to_owned())?;
                    let st = Command::new("open").arg("-a").arg(&bundle).arg(&url).status().map_err(|e| e.to_string())?;
                    if st.success() { Ok(url) } else { Err(format!("open {url} ended with {st}")) }
                });
                match ok {
                    Ok(url) => r.say(&format!("opened {url}")),
                    Err(e) => {
                        r.outcome(step, Some(false), e, t.elapsed().as_secs_f64(), None);
                        failed_at = Some(i);
                    }
                }
            }
            "check" => {
                let cond = if step.kind() == "allow-until" { &step.v["until"] } else if step.kind() == "wait" { &step.v["until"] } else { &step.v["that"] };
                let title = window_of(pid).map(|w| w.6).unwrap_or_default();
                let words = ax_words(pid);
                let rel = format!("steps/{}.png", step.stem());
                let pic = capture(id, &r.o.out.join(&rel)).ok().map(|_| rel);
                let daemon_side = check_other(r, cond);
                match (check_words(cond, &words, &title), daemon_side) {
                    // The system asking the person for permission is not the app's failure: it needs the person once.
                    (_, Some(Err(e))) if e.starts_with("needs the person once") => r.outcome(step, None, e, t.elapsed().as_secs_f64(), pic),
                    (Err(e), _) | (_, Some(Err(e))) => {
                        r.outcome(step, Some(false), e, t.elapsed().as_secs_f64(), pic);
                        if step.v["soft"].as_bool() != Some(true) {
                            failed_at = Some(i);
                        }
                    }
                    (Ok(Some(())), _) | (_, Some(Ok(()))) => r.outcome(step, Some(true), "checked by the window's words".into(), t.elapsed().as_secs_f64(), pic),
                    (Ok(None), None) => r.outcome(step, None, "picture only: this check names keys a real window does not expose".into(), t.elapsed().as_secs_f64(), pic),
                }
            }
            _ => {}
        }
    }
    // Steps never reached (the script ended early).
    if failed_at.is_some() {
        for s in &steps {
            if s.expect().is_some() && !r.outcomes.iter().any(|o| o.step == s.n) {
                r.outcome(s, None, "not reached: an earlier step failed".into(), 0.0, None);
            }
        }
    }
    let script_step = Step { n: steps.len() + 1, v: json!({"do": "script", "expect": "The whole scenario replayed in the real window"}) };
    r.outcome(&script_step, Some(finished && script_errors.is_empty()), if script_errors.is_empty() { if finished { String::new() } else { "the script did not reach its end".into() } } else { script_errors.join("; ") }, 0.0, None);

    // The Dock menu, read once.
    let dock_step = Step { n: steps.len() + 2, v: json!({"do": "dock", "expect": "The Dock menu is normal: not \"Application Not Responding\", and it has Quit"}) };
    let dock = Command::new("osascript").arg(scripts().join("dock-menu.applescript")).arg(&dock_name).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    let items: Vec<&str> = dock.lines().filter(|l| !l.trim().is_empty()).collect();
    let dock_ok = items.iter().any(|i| i.starts_with("Quit")) && !items.iter().any(|i| i.contains("Not Responding"));
    r.outcome(&dock_step, Some(dock_ok), if dock_ok { String::new() } else { format!("the Dock menu reads: {}", items.join(" / ")) }, 0.0, None);
    r.say(&format!("Dock menu: {}", items.join(" / ")));
    // From cold: with the app quit, the task's link starts it at that task.
    if let Some(step) = steps.iter().find(|s| s.kind() == "cold-link")
        && failed_at.is_none()
    {
        let t = Instant::now();
        let end = Instant::now() + Duration::from_secs(8);
        while Instant::now() < end && !exited(&mut child, pid) {
            std::thread::sleep(Duration::from_millis(100));
        }
        stop(&mut child, pid);
        let result = cold_link(r, step, &rune_root, &notify_log);
        match result {
            Ok((cold_pid, title)) => {
                let want = step.v["title"].as_str().unwrap_or("");
                let ok = title.starts_with(want);
                r.outcome(step, Some(ok), if ok { String::new() } else { format!("the window opened on {title:?}") }, t.elapsed().as_secs_f64(), None);
                r.started("neko (cold link)", cold_pid);
                let mut none = None;
                stop(&mut none, cold_pid);
            }
            Err(e) => r.outcome(step, Some(false), e, t.elapsed().as_secs_f64(), None),
        }
    }
    if let Some(ms) = first_frame {
        r.notes_info.push(format!("Window on screen {appeared:.2} s after launch; Rune reported the first frame after {ms} ms."));
    }

    // Quit (the script asked for it), else stop it by its PID.
    let end = Instant::now() + Duration::from_secs(8);
    while Instant::now() < end && !exited(&mut child, pid) {
        std::thread::sleep(Duration::from_millis(100));
    }
    stop(&mut child, pid);
    done.store(true, Ordering::SeqCst);
    let shots = shooter.join().unwrap_or(0);
    while let Ok(l) = rx.try_recv() {
        use std::io::Write;
        let _ = writeln!(log, "{l}");
    }
    r.s.steps = steps;
    // The video: the window's captures at two a second.
    if shots == 0 {
        return Err("no window captures for the video".into());
    }
    let st = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y", "-framerate", "2", "-i"])
        .arg(frames.join("f%05d.png"))
        .args(["-vf", "scale=trunc(iw/4)*2:trunc(ih/4)*2", "-c:v", "libx264", "-preset", "veryfast", "-crf", "24", "-pix_fmt", "yuv420p", "-movflags", "+faststart"])
        .arg(r.o.out.join("video.mp4"))
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("ffmpeg ended with {st}"));
    }
    Ok(())
}

/// Whether the app has ended: its child (the bare binary) or its PID (a bundle `open` started).
fn exited(child: &mut Option<std::process::Child>, pid: u32) -> bool {
    match child {
        Some(c) => matches!(c.try_wait(), Ok(Some(_))),
        // SAFETY: signal 0 only asks whether the process this run started is still there.
        None => unsafe { libc::kill(pid as i32, 0) != 0 },
    }
}

/// Stops the app this run started, by its PID: SIGTERM, then SIGKILL after three seconds.
fn stop(child: &mut Option<std::process::Child>, pid: u32) {
    if exited(child, pid) {
        return;
    }
    // SAFETY: the app this run started, by its PID.
    unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    let end = Instant::now() + Duration::from_secs(3);
    while Instant::now() < end {
        if exited(child, pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    // SAFETY: as above.
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
    if let Some(c) = child {
        let _ = c.wait();
    }
}

/// The PID of the process running exactly this executable.
fn pid_of(exe: &Path) -> Option<u32> {
    let out = Command::new("ps").args(["-axo", "pid=,comm="]).output().ok()?;
    let want = exe.to_string_lossy();
    String::from_utf8_lossy(&out.stdout).lines().find_map(|l| {
        let l = l.trim_start();
        let (pid, comm) = l.split_once(' ')?;
        (comm.trim() == want).then(|| pid.parse().ok()).flatten()
    })
}

/// Sends each new line of a file as it is written (the bundle's stdout, which `open` writes to a file).
fn follow_file(path: PathBuf, tx: mpsc::Sender<String>) {
    std::thread::spawn(move || {
        let Ok(f) = std::fs::File::open(&path) else { return };
        let mut reader = BufReader::new(f);
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => std::thread::sleep(Duration::from_millis(50)),
                Ok(_) if line.ends_with('\n') => {
                    if tx.send(line.trim_end().to_owned()).is_err() {
                        return;
                    }
                }
                Ok(_) => {
                    // A part of a line: wait for the rest.
                    let mut rest = String::new();
                    while !rest.ends_with('\n') {
                        match reader.read_line(&mut rest) {
                            Ok(0) => std::thread::sleep(Duration::from_millis(50)),
                            Ok(_) => {}
                            Err(_) => return,
                        }
                    }
                    line.push_str(&rest);
                    if tx.send(line.trim_end().to_owned()).is_err() {
                        return;
                    }
                }
                Err(_) => return,
            }
        }
    });
}

/// The checks that do not look at the screen (git, files, commands), as headless runs them. None when `c` has none.
fn check_other(r: &Run, c: &Value) -> Option<Result<(), String>> {
    if let Some(list) = c["all"].as_array() {
        let results: Vec<Result<(), String>> = list.iter().filter_map(|x| check_other(r, x)).collect();
        if results.is_empty() {
            return None;
        }
        return Some(results.into_iter().collect());
    }
    if c.get("notified").is_some() || c.get("badge").is_some() || c.get("withdrawn").is_some() || c.get("posts").is_some() {
        return Some(check_notify_log(r, c));
    }
    if let Some(name) = c["git_same"].as_str() {
        let (repo, before) = r.notes_marks.get(name).and_then(|v| v.split_once('\n')).map(|(a, b)| (a.to_owned(), b.to_owned()))?;
        return Some(super::git_state(&repo, &r.places.home).and_then(|now| if now == before { Ok(()) } else { Err(format!("the checkout changed: {before:?} → {now:?}")) }));
    }
    if let Some(sh) = c.get("sh") {
        let out = Command::new("/bin/sh").arg("-c").arg(sh["run"].as_str().unwrap_or("")).current_dir(&r.places.work).env("HOME", &r.places.home).output().ok()?;
        let text = String::from_utf8_lossy(&out.stdout).into_owned();
        return Some(match sh["has"].as_str() {
            Some(h) if !text.contains(h) => Err(format!("`{}` printed {:?}", sh["run"], cut(&text, 160))),
            _ => Ok(()),
        });
    }
    if let Some(f) = c.get("file") {
        let body = std::fs::read_to_string(f["path"].as_str().unwrap_or("")).unwrap_or_default();
        return Some(if body.contains(f["has"].as_str().unwrap_or("")) && !body.is_empty() { Ok(()) } else { Err("the file is missing or wrong".into()) });
    }
    None
}

/// The app's own log of what it posted (`NEKO_NOTIFY_LOG`): one JSON line per notification posted or taken back, and per badge.
fn notify_events(r: &Run) -> Vec<Value> {
    std::fs::read_to_string(r.places.work.join("notify.jsonl")).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
}

/// `notified` {title, body, buttons}: a post like that was made; `badge` n: the last badge count; `withdrawn`: a post was taken back;
/// `posts` {max, titled?}: no more than that many were posted (only those whose title starts with `titled`, when given). A post the system refused while it asks the person for permission says so.
pub(crate) fn check_notify_log(r: &Run, c: &Value) -> Result<(), String> {
    let ev = notify_events(r);
    let posts: Vec<&Value> = ev.iter().filter(|e| e.get("post").is_some()).collect();
    if let Some(n) = c.get("notified") {
        let found = posts.iter().find(|p| {
            n["title"].as_str().is_none_or(|t| p["title"].as_str().is_some_and(|x| x.starts_with(t)))
                && n["body"].as_str().is_none_or(|b| p["body"].as_str().is_some_and(|x| x.contains(b)))
                && n["buttons"].as_array().is_none_or(|bs| bs.iter().all(|b| p["buttons"].as_array().is_some_and(|pb| pb.contains(b))))
        });
        let Some(p) = found else { return Err(format!("no such notification was posted; the app posted: {}", cut(&serde_json::to_string(&posts).unwrap_or_default(), 300))) };
        if p["posted"] == json!(false) {
            return Err("needs the person once: the system did not take the notification (it may be asking for permission)".into());
        }
    }
    if let Some(want) = c["badge"].as_u64() {
        let last = ev.iter().rev().find_map(|e| e["badge"].as_u64());
        if last != Some(want) {
            return Err(format!("the Dock badge is {last:?}"));
        }
    }
    if c.get("withdrawn").is_some() && !ev.iter().any(|e| e.get("withdraw").is_some()) {
        return Err("no notification was taken back".into());
    }
    if let Some(max) = c["posts"]["max"].as_u64() {
        // `titled` counts only the posts whose title starts so (the morning briefing's, say).
        let n = posts.iter().filter(|p| c["posts"]["titled"].as_str().is_none_or(|t| p["title"].as_str().is_some_and(|x| x.starts_with(t)))).count() as u64;
        if n > max {
            return Err(format!("{n} notifications were posted (at most {max})"));
        }
    }
    Ok(())
}

/// `neko://task/<id>` for the task whose title starts with the step's `title`, asked of the daemon.
fn link_target(r: &Run, step: &Step) -> Result<String, String> {
    use neko_protocol::{Reply, Request};
    let want = step.v["title"].as_str().unwrap_or("");
    let (info, token) = neko_protocol::client::find(&r.places.data).map_err(|e| e.to_string())?;
    let mut c = neko_protocol::client::Client::connect(info.port, &token, "neko-qa").map_err(|e| e.to_string())?;
    let tasks = match c.request(Request::ListTasks).map_err(|e| e.to_string())? {
        Reply::Tasks { tasks } => tasks,
        other => return Err(format!("list_tasks: {other:?}")),
    };
    c.close();
    let task = tasks.into_iter().find(|t| t.title.starts_with(want)).ok_or(format!("no task titled {want:?}"))?;
    Ok(format!("neko://task/{}", task.id.0))
}

/// Starts the bundle from cold with the task's link, waits for its window, and says what its title is.
fn cold_link(r: &Run, step: &Step, rune_root: &Path, notify_log: &Path) -> Result<(u32, String), String> {
    let url = link_target(r, step)?;
    let bundle = PathBuf::from(std::env::var("NEKO_QA_BUNDLE").map_err(|_| "links need the bundle".to_owned())?);
    let exe = bundle.join("Contents/MacOS/neko");
    let mut c = Command::new("open");
    c.args(["-n", "-g", "-a"]).arg(&bundle);
    for (k, v) in [("RUNE_DIRS_ROOT", rune_root.display().to_string()), ("NEKO_DATA_DIR", r.places.data.display().to_string()), ("NEKO_NOTIFY_LOG", notify_log.display().to_string())] {
        c.arg("--env").arg(format!("{k}={v}"));
    }
    let st = c.arg(&url).status().map_err(|e| e.to_string())?;
    if !st.success() {
        return Err(format!("open {url} ended with {st}"));
    }
    let began = Instant::now();
    let mut pid = None;
    while pid.is_none() && began.elapsed() < Duration::from_secs(15) {
        pid = pid_of(&exe);
        std::thread::sleep(Duration::from_millis(100));
    }
    let pid = pid.ok_or("the app did not start from the link")?;
    let want = step.v["title"].as_str().unwrap_or("");
    let mut title = String::new();
    while began.elapsed() < Duration::from_secs(20) {
        if let Some(w) = window_of(pid) {
            title = w.6;
            if title.starts_with(want) {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok((pid, title))
}
