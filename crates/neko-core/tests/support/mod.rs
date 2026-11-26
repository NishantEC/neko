//! What the runtime tests share: the contract suite any runtime must pass, an in-process fake runtime, temp folders, and waiting
//! on a condition with a deadline (never a fixed sleep).
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use neko_core::runtime::{Action, AgentEvent, Answer, Control, ControlError, Detection, Failure, Outcome, Reported, Runtime, StartSpec, Started};
use neko_protocol::PermissionMode;

/// A folder removed on drop.
pub struct Temp(pub PathBuf);

impl Temp {
    pub fn new(name: &str) -> Temp {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("neko-rt-{name}-{}-{n}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir.canonicalize().unwrap())
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Waits until `ok` or the deadline; panics with `what` then.
pub fn until(what: &str, limit: Duration, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + limit;
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

pub fn alive(pid: i32) -> bool {
    // SAFETY: signal 0 only asks whether the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Reads a pid a script wrote, waiting for it.
pub fn pid_in(file: &Path) -> i32 {
    until("the pid file", Duration::from_secs(10), || std::fs::read_to_string(file).is_ok_and(|s| s.trim().parse::<i32>().is_ok()));
    std::fs::read_to_string(file).unwrap().trim().parse().unwrap()
}

/// Kills a process this test started (by its pid), if a test failed before the guard did.
pub struct Reap(pub i32);

impl Drop for Reap {
    fn drop(&mut self) {
        if self.0 > 0 && alive(self.0) {
            // SAFETY: a process this test's own script started, by the pid it wrote.
            unsafe { libc::kill(self.0, libc::SIGKILL) };
        }
    }
}

pub fn spec(words: &str, dir: &Path) -> StartSpec {
    StartSpec { words: words.into(), workdir: dir.to_path_buf(), resume: None, mode: PermissionMode::Follow, hook: None }
}

/// Every event of a run until it finishes (with a deadline), answering permission requests with `answer`.
pub fn drain(s: &Started, limit: Duration, mut answer: impl FnMut(&str, &Action) -> Option<Answer>) -> Vec<AgentEvent> {
    let end = Instant::now() + limit;
    let mut got = Vec::new();
    loop {
        let left = end.saturating_duration_since(Instant::now());
        assert!(!left.is_zero(), "the run did not finish in time; it said {got:?}");
        match s.events.recv_timeout(left) {
            Ok(e) => {
                if let AgentEvent::PermissionAsked { id, action, .. } = &e
                    && let Some(a) = answer(id, action)
                {
                    s.control.answer(id, a).unwrap();
                }
                let done = matches!(e, AgentEvent::Finished(_));
                got.push(e);
                if done {
                    return got;
                }
            }
            Err(mpsc::RecvTimeoutError::Timeout) => panic!("the run did not finish in time; it said {got:?}"),
            Err(mpsc::RecvTimeoutError::Disconnected) => panic!("the stream closed without Finished; it said {got:?}"),
        }
    }
}

pub fn text_of(events: &[AgentEvent]) -> String {
    events.iter().filter_map(|e| if let AgentEvent::TextDelta(t) = e { Some(t.as_str()) } else { None }).collect()
}

pub fn finished(events: &[AgentEvent]) -> &Outcome {
    match events.last() {
        Some(AgentEvent::Finished(o)) => o,
        other => panic!("the last event is not Finished: {other:?}"),
    }
}

/// The contract: what any runtime must do, whatever CLI is behind it. Scripts are named in the words (`[fake:NAME]`).
pub fn contract_suite(r: &dyn Runtime) {
    let dir = Temp::new("contract");
    let d = r.detect();
    assert!(d.found && d.version.is_some(), "{}: detect finds it and reads its version: {d:?}", r.id());

    // A plain answer: Started first, words, then exactly one Finished(Done), then the stream closes.
    let s = r.start(spec("Say hello [fake:plain]", &dir.0)).unwrap();
    let ev = drain(&s, Duration::from_secs(20), |_, _| None);
    assert!(matches!(ev[0], AgentEvent::Started { .. }), "{}: Started comes first: {ev:?}", r.id());
    assert!(!text_of(&ev).is_empty(), "{}: words arrive", r.id());
    assert_eq!(finished(&ev), &Outcome::Done);
    assert_eq!(ev.iter().filter(|e| matches!(e, AgentEvent::Finished(_))).count(), 1);
    assert!(s.events.recv_timeout(Duration::from_secs(5)).is_err(), "{}: nothing after Finished", r.id());
    let session = ev.iter().find_map(|e| if let AgentEvent::Started { session, .. } = e { session.clone() } else { None });
    assert!(session.is_some(), "{}: the conversation id is reported", r.id());

    // A follow-up resumes the same conversation.
    let mut again = spec("And then? [fake:plain]", &dir.0);
    again.resume = session.clone();
    let s = r.start(again).unwrap();
    let ev = drain(&s, Duration::from_secs(20), |_, _| None);
    assert!(matches!(&ev[0], AgentEvent::Started { session: s2, .. } if *s2 == session), "{}: resumed with the same id: {ev:?}", r.id());

    // A permission request waits for its answer; allowed, the run goes on. A second answer to it is refused.
    let s = r.start(spec("Run the tests [fake:permission]", &dir.0)).unwrap();
    let mut asked = None;
    let ev = drain(&s, Duration::from_secs(20), |id, a| {
        assert!(matches!(a, Action::Command { command } if command == "cargo test"));
        asked = Some(id.to_owned());
        Some(Answer::Allow)
    });
    assert_eq!(finished(&ev), &Outcome::Done);
    assert!(text_of(&ev).contains("pass"), "{}: the allowed step ran: {}", r.id(), text_of(&ev));
    assert!(s.control.answer(asked.as_deref().unwrap(), Answer::Allow).is_err(), "{}: answering twice is refused", r.id());

    // Denied, it does not run.
    let s = r.start(spec("Run the tests [fake:permission]", &dir.0)).unwrap();
    let ev = drain(&s, Duration::from_secs(20), |_, _| Some(Answer::Deny));
    assert!(text_of(&ev).contains("did not run"), "{}: denied: {}", r.id(), text_of(&ev));

    // Stop ends it within a second.
    let s = r.start(spec("Go slowly [fake:slow]", &dir.0)).unwrap();
    until("the first words", Duration::from_secs(20), || matches!(s.events.recv_timeout(Duration::from_millis(50)), Ok(AgentEvent::TextDelta(_))));
    let t = Instant::now();
    s.control.stop();
    let ev = drain(&s, Duration::from_secs(5), |_, _| None);
    assert_eq!(finished(&ev), &Outcome::Stopped);
    assert!(t.elapsed() < Duration::from_secs(1), "{}: Stop answered in {:?}", r.id(), t.elapsed());
    s.control.stop();

    // A folder that is gone fails before anything runs.
    let gone = dir.0.join("not-there");
    assert_eq!(r.start(spec("Hi", &gone)).err(), Some(Failure::FolderGone));
}

/// An in-process runtime that plays the same scripts as the fake CLI, to prove the suite itself and to stand in where no process
/// is wanted.
pub struct FakeRuntime;

struct FakeControl {
    stopped: AtomicBool,
    answers: Mutex<Option<Sender<(String, Answer)>>>,
    asked: Mutex<Vec<String>>,
}

impl Control for FakeControl {
    fn steer(&self, _: &str) -> Result<(), ControlError> {
        Ok(())
    }

    fn answer(&self, id: &str, a: Answer) -> Result<(), ControlError> {
        let mut asked = self.asked.lock().unwrap();
        let Some(i) = asked.iter().position(|x| x == id) else { return Err(ControlError("not waiting".into())) };
        asked.remove(i);
        let _ = self.answers.lock().unwrap().as_ref().map(|t| t.send((id.to_owned(), a)));
        Ok(())
    }

    fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        self.answers.lock().unwrap().take();
    }
}

impl Runtime for FakeRuntime {
    fn id(&self) -> &'static str {
        "fake"
    }
    fn name(&self) -> &'static str {
        "Fake"
    }
    fn sign_in_line(&self) -> &'static str {
        "fake login"
    }
    fn install_line(&self) -> &'static str {
        "fake install"
    }
    fn detect(&self) -> Detection {
        Detection { found: true, path: None, version: Some("1.0".into()), signed_in: Some(true), login: None }
    }
    fn start(&self, spec: StartSpec) -> Result<Started, Failure> {
        if !spec.workdir.is_dir() {
            return Err(Failure::FolderGone);
        }
        let (tx, rx) = mpsc::channel();
        let (atx, arx): (Sender<(String, Answer)>, Receiver<(String, Answer)>) = mpsc::channel();
        let control = Arc::new(FakeControl { stopped: AtomicBool::new(false), answers: Mutex::new(Some(atx)), asked: Mutex::new(Vec::new()) });
        let c = control.clone();
        std::thread::spawn(move || {
            let send = |e| {
                let _ = tx.send(e);
            };
            send(AgentEvent::Started { session: Some(spec.resume.clone().unwrap_or_else(|| "fake-session".into())), model: Some("fake-1".into()) });
            let w = &spec.words;
            let outcome = if w.contains("[fake:permission]") {
                c.asked.lock().unwrap().push("p1".into());
                send(AgentEvent::PermissionAsked { id: "p1".into(), tool: "Bash".into(), action: Action::Command { command: "cargo test".into() }, tool_use: None });
                match arx.recv() {
                    Ok((_, Answer::Allow)) => send(AgentEvent::TextDelta("The tests pass.".into())),
                    Ok((_, Answer::Deny)) => send(AgentEvent::TextDelta("Okay, I did not run the tests.".into())),
                    Err(_) => {}
                }
                if c.stopped.load(Ordering::SeqCst) { Outcome::Stopped } else { Outcome::Done }
            } else if w.contains("[fake:slow]") {
                let mut o = Outcome::Done;
                for i in 0..3000 {
                    if c.stopped.load(Ordering::SeqCst) {
                        o = Outcome::Stopped;
                        break;
                    }
                    send(AgentEvent::TextDelta(format!("slow{i} ")));
                    std::thread::sleep(Duration::from_millis(50));
                }
                o
            } else {
                send(AgentEvent::TextDelta("Hello.".into()));
                send(AgentEvent::TextDone);
                send(AgentEvent::Usage(Reported { input_tokens: 1, output_tokens: 2, ..Reported::default() }));
                Outcome::Done
            };
            send(AgentEvent::Finished(outcome));
        });
        let control: Arc<dyn Control> = control;
        Ok(Started { control, events: rx })
    }
}

/// The fake CLI binary, built with this package.
pub fn fake_cli() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_neko-fake-claude"))
}

/// The Claude runtime on the fake CLI, with the fake as its run guard too, and a HOME of its own so the person's own Claude Code
/// settings never reach a test.
pub fn claude_on_fake() -> neko_core::claude::ClaudeRuntime {
    let home = std::env::temp_dir().join(format!("neko-fake-home-{}", std::process::id()));
    let _ = std::fs::create_dir_all(&home);
    neko_core::claude::ClaudeRuntime::new(Some(fake_cli()), fake_cli()).with_env("HOME", &home.to_string_lossy())
}
