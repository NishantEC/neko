//! Subagents in the real daemon, with the fake CLI as `claude` and Neko's own MCP server really started by it (docs/knowledge/subagents.md):
//! a thread starts subagents through the tool, ends its turn and rests with no process, each report wakes it with `--resume` (once for
//! a batch), Stop on the thread stops them all and leaves no process, a restart interrupts the working ones and still wakes the thread,
//! the tool never blocks, the limits queue, a subagent asks for the same permissions as its thread and no more. Every wait has a
//! deadline.

mod support;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use serde_json::{Value, json};
use support::*;

const T: Duration = Duration::from_secs(40);

fn until(what: &str, mut ok: impl FnMut() -> bool) {
    let end = Instant::now() + T;
    while !ok() {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(40));
    }
}

struct Rig {
    d: Daemon,
    record: PathBuf,
}

fn rig(name: &str, extra: &[(&str, &str)]) -> Rig {
    let dir = temp_dir(name);
    let record = dir.join("record.jsonl");
    let r = record.to_string_lossy().into_owned();
    let mut env: Vec<(&str, &str)> = vec![("NEKO_FAKE_RECORD", &r), ("NEKO_WAKE_BATCH_MS", "400")];
    env.extend_from_slice(extra);
    Rig { d: Daemon::start_with(dir, &env), record }
}

impl Rig {
    fn client(&self) -> Client {
        let mut c = self.d.client();
        assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
        c
    }

    fn recorded(&self) -> Vec<Value> {
        std::fs::read_to_string(&self.record).unwrap_or_default().lines().filter_map(|l| serde_json::from_str(l).ok()).collect()
    }

    /// The arguments of each run the fake CLI was started for, in order.
    fn argvs(&self) -> Vec<Vec<String>> {
        self.recorded().iter().filter_map(|v| v["argv"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())).collect()
    }

    /// The user messages the fake CLI read, with the index of the run that read them.
    fn messages(&self) -> Vec<(usize, String)> {
        let mut run = 0usize;
        let mut out = Vec::new();
        for v in self.recorded() {
            if v.get("argv").is_some() {
                run += 1;
            } else if let Some(m) = v["stdin"].as_object().filter(|m| m.get("type").and_then(Value::as_str) == Some("user")) {
                let text: String = m["message"]["content"].as_array().map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect()).unwrap_or_default();
                out.push((run, text));
            }
        }
        out
    }

    fn mcp_calls(&self) -> Vec<Value> {
        self.recorded().iter().filter_map(|v| v.get("mcp_call").cloned()).collect()
    }
}

fn tasks(c: &mut Client) -> Vec<Task> {
    match c.request(Request::ListTasks).unwrap() {
        Reply::Tasks { tasks } => tasks,
        r => panic!("tasks: {r:?}"),
    }
}

fn subagents(c: &mut Client, thread: &TaskId) -> Vec<Task> {
    let mut v: Vec<Task> = tasks(c).into_iter().filter(|t| t.subagent.as_ref().is_some_and(|s| s.root == *thread)).collect();
    v.sort_by(|a, b| a.subagent.as_ref().map(|s| s.number.clone()).cmp(&b.subagent.as_ref().map(|s| s.number.clone())));
    v
}

fn task(c: &mut Client, id: &TaskId) -> Task {
    match c.request(Request::GetTask { task: id.clone() }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("task: {r:?}"),
    }
}

fn transcript(c: &mut Client, id: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { words: words.into(), workspace: None, cat: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn set_mode(c: &mut Client, m: PermissionMode) {
    let s = Settings { runtime: "claude".into(), permission_mode: m, own_mcp: false };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

/// A thread's words: a script that starts one subagent per `(title, prompt)` through the real MCP hop, then `extra` lines.
fn delegate(subs: &[(&str, &str)], extra: &[&str]) -> String {
    let mut w = String::from("Do these in parallel [fake:delegate]");
    for (title, prompt) in subs {
        w.push_str(&format!("\nmcp neko start_subagent {}", json!({"title": title, "prompt": prompt})));
    }
    for e in extra {
        w.push('\n');
        w.push_str(e);
    }
    w
}

fn agent_words(t: &Transcript) -> String {
    t.parts.iter().filter_map(|p| if let PartBody::Agent { text, .. } = &p.body { Some(text.as_str()) } else { None }).collect::<Vec<_>>().join("\n")
}

fn wakes(t: &Transcript) -> Vec<(String, String)> {
    t.parts.iter().filter_map(|p| if let PartBody::Wake { number, status, .. } = &p.body { Some((number.clone(), status.clone())) } else { None }).collect()
}

fn cards(t: &Transcript) -> Vec<String> {
    t.parts.iter().filter_map(|p| if let PartBody::Subagent { number, .. } = &p.body { Some(number.clone()) } else { None }).collect()
}

/// Processes whose command line names this daemon's folder, other than the daemon itself.
fn processes_of(d: &Daemon) -> Vec<String> {
    let out = std::process::Command::new("ps").args(["-axo", "pid=,command="]).output().unwrap();
    let me = d.child.id().to_string();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.contains(d.dir.to_string_lossy().as_ref()) && l.trim_start().split_whitespace().next() != Some(me.as_str()))
        .map(str::to_owned)
        .collect()
}

#[test]
fn a_thread_starts_two_subagents_rests_with_no_process_and_is_woken_once_for_both() {
    let r = rig("wake", &[]);
    let mut c = r.client();
    // Both take a second, so the thread is seen resting while they work, and both end within one batch.
    let t = create(&mut c, &delegate(&[("Check links", "Look at the links [fake:long:1]"), ("Check images", "Look at the images [fake:long:1]")], &[]));
    until("the thread to rest", || task(&mut c, &t.id).status == TaskStatus::Waiting);
    let resting = task(&mut c, &t.id);
    assert_eq!((resting.thread_status, resting.needs_you), (ThreadStatus::Waiting, false), "the thread says what it is doing: it rests while its subagents work");
    // Resting: its run is over and nothing of it is left running; the subagents carry on.
    let tr = transcript(&mut c, &t.id);
    assert_eq!(tr.runs.len(), 1);
    assert_eq!(tr.runs[0].status, RunStatus::Done, "its turn ended");
    assert_eq!(cards(&tr), ["1", "2"], "a card where each was started");
    assert!(agent_words(&tr).contains("Done with my calls: 2 call(s)."), "{}", agent_words(&tr));
    let subs = subagents(&mut c, &t.id);
    assert_eq!(subs.len(), 2);
    assert_eq!(subs[0].subagent.as_ref().unwrap().number, "1");
    assert_eq!(subs[1].subagent.as_ref().unwrap().parent, t.id);
    assert_eq!(subs[0].title, "Check links");
    // Subagents are not threads: the thread's own fields are untouched and they share its workspace and cat.
    assert_eq!((subs[0].workspace.as_str(), subs[0].cat.as_str()), (t.workspace.as_str(), t.cat.as_str()));
    until("the thread to be woken and finish", || {
        let tr = transcript(&mut c, &t.id);
        tr.runs.len() == 2 && tr.runs[1].status == RunStatus::Done
    });
    let tr = transcript(&mut c, &t.id);
    assert_eq!(tr.runs.len(), 2, "one wake run for both reports: {:?}", wakes(&tr));
    let mut got = wakes(&tr);
    got.sort();
    assert_eq!(got, [("1".to_owned(), "done".to_owned()), ("2".to_owned(), "done".to_owned())], "each report, in the order they arrived");
    until("the thread to be done", || task(&mut c, &t.id).status == TaskStatus::Done);
    // What the real CLI would see: the run was resumed with the first run's session and the message carried both reports.
    let argvs = r.argvs();
    let session = tr.runs[0].session.clone().unwrap();
    let woken: Vec<(usize, String)> = r.messages().into_iter().filter(|(_, m)| m.starts_with("<neko-wake")).collect();
    assert_eq!(woken.len(), 1, "the report reached the CLI once: {woken:?}");
    let argv = &argvs[woken[0].0 - 1];
    let at = argv.iter().position(|x| x == "--resume").expect("resumed");
    assert_eq!(argv[at + 1], session, "with the first run's own session");
    let wake_msgs: Vec<String> = r.messages().into_iter().map(|(_, m)| m).filter(|m| m.starts_with("<neko-wake")).collect();
    assert_eq!(wake_msgs.len(), 1);
    assert_eq!(wake_msgs[0].matches("<neko-wake").count(), 2);
    assert!(wake_msgs[0].contains("id=\"1\" status=\"done\"") && wake_msgs[0].contains("id=\"2\" status=\"done\""), "{}", wake_msgs[0]);
    assert!(agent_words(&tr).contains("I read 2 report(s)."), "{}", agent_words(&tr));
    // The run's own tools: strict MCP, the neko server first, nothing secret in the command line or the config.
    for a in &argvs {
        assert!(a.iter().any(|x| x == "--strict-mcp-config"), "a run has only the servers Neko gave it: {a:?}");
        assert!(a.iter().any(|x| x == "--mcp-config"));
    }
    let cfg = r.recorded().into_iter().find_map(|v| v.get("mcp_config").cloned()).unwrap();
    let neko = &cfg["content"]["mcpServers"]["neko"];
    assert_eq!(neko["type"], "stdio");
    assert_eq!(neko["args"], json!(["--mcp"]));
    assert!(neko.get("env").is_none(), "the secret is in the CLI's environment, not the config: {neko}");
    assert!(neko["command"].as_str().unwrap().ends_with("neko-daemon"));
    let all = std::fs::read_to_string(&r.record).unwrap();
    assert!(!all.contains("NEKO_MCP_SECRET"), "no secret in what the CLI was handed");
    // Subagents never notify and are not listed as threads: the thread's list entry is the only one without a `subagent`.
    assert_eq!(tasks(&mut c).iter().filter(|t| t.subagent.is_none()).count(), 1);
}

#[test]
fn a_report_that_arrives_while_the_thread_still_works_is_steered_into_that_run() {
    let r = rig("steer", &[]);
    let mut c = r.client();
    let t = create(&mut c, &delegate(&[("Quick", "Hello [fake:plain]")], &["stay 3"]));
    until("the thread to be done", || task(&mut c, &t.id).status == TaskStatus::Done);
    let tr = transcript(&mut c, &t.id);
    assert_eq!(tr.runs.len(), 1, "no second run: the report went into the one that was going");
    assert_eq!(wakes(&tr), [("1".to_owned(), "done".to_owned())]);
    let woken: Vec<(usize, String)> = r.messages().into_iter().filter(|(_, m)| m.starts_with("<neko-wake")).collect();
    assert_eq!(woken.len(), 1);
    let note = tr.parts.iter().find(|p| matches!(p.body, PartBody::Wake { .. })).unwrap();
    assert_eq!(note.run.as_ref(), Some(&tr.runs[0].id), "its note is in the run that read it");
    assert!(agent_words(&tr).contains("I read 1 report(s)."), "{}", agent_words(&tr));
    assert!(!r.argvs().iter().any(|a| a.iter().any(|x| x == "--resume")), "nothing was resumed");
}

#[test]
fn stop_on_the_thread_stops_its_subagents_and_leaves_no_process() {
    let r = rig("stop", &[]);
    let mut c = r.client();
    let t = create(&mut c, &delegate(&[("Count A", "Count slowly [fake:slow]"), ("Count B", "Count slowly [fake:slow]")], &[]));
    until("both subagents to work and the thread to rest", || {
        task(&mut c, &t.id).status == TaskStatus::Waiting && subagents(&mut c, &t.id).iter().filter(|s| s.status == TaskStatus::Working).count() == 2
    });
    assert!(!processes_of(&r.d).is_empty(), "the subagents' runs are alive");
    let began = Instant::now();
    assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    until("everything to stop", || task(&mut c, &t.id).status == TaskStatus::Stopped && subagents(&mut c, &t.id).iter().all(|s| s.status == TaskStatus::Stopped));
    assert!(began.elapsed() < Duration::from_secs(5), "Stop is quick: {:?}", began.elapsed());
    until("no process left", || processes_of(&r.d).is_empty());
    // Their reports woke no one: the thread ran once.
    std::thread::sleep(Duration::from_millis(900));
    let tr = transcript(&mut c, &t.id);
    assert_eq!(tr.runs.len(), 1, "nothing resumed it");
    assert!(wakes(&tr).is_empty());
    assert!(!r.argvs().iter().any(|a| a.iter().any(|x| x == "--resume")), "nothing was resumed");
}

#[test]
fn a_restart_interrupts_a_working_subagent_and_the_thread_is_woken_with_that() {
    let mut r = rig("restart", &[]);
    r.d.keep_dir = true;
    let mut c = r.client();
    let t = create(&mut c, &delegate(&[("Count", "Count slowly [fake:slow]")], &[]));
    until("the subagent to work", || task(&mut c, &t.id).status == TaskStatus::Waiting && subagents(&mut c, &t.id).iter().any(|s| s.status == TaskStatus::Working));
    // Let its words reach the disk before the quit.
    std::thread::sleep(Duration::from_millis(3300));
    drop(c);
    let dir = r.d.dir.clone();
    let record = r.record.clone();
    r.d.stop();
    drop(r);
    let r = Rig { d: Daemon::start_with(dir, &[("NEKO_FAKE_RECORD", &record.to_string_lossy()), ("NEKO_WAKE_BATCH_MS", "400")]), record };
    let mut c = r.client();
    until("the thread to be woken and finish", || {
        let tr = transcript(&mut c, &t.id);
        tr.runs.len() == 2 && tr.runs[1].status == RunStatus::Done
    });
    let subs = subagents(&mut c, &t.id);
    assert_eq!(subs[0].status, TaskStatus::Interrupted, "marked interrupted");
    let tr = transcript(&mut c, &t.id);
    assert_eq!(wakes(&tr), [("1".to_owned(), "interrupted".to_owned())]);
    let sub_tr = transcript(&mut c, &subs[0].id);
    assert!(agent_words(&sub_tr).contains("slow"), "its words are kept: {:?}", agent_words(&sub_tr));
    let msg = r.messages().into_iter().map(|(_, m)| m).find(|m| m.starts_with("<neko-wake")).expect("the thread read the report");
    assert!(msg.contains("status=\"interrupted\""), "{msg}");
    assert!(msg.contains("slow"), "what it had said is in the report: {msg}");
    assert!(agent_words(&tr).contains("I read 1 report(s)."));
    until("the thread to be done", || task(&mut c, &t.id).status == TaskStatus::Done);
}

#[test]
fn the_tool_that_lists_subagents_answers_at_once_however_often_it_is_called() {
    let r = rig("poll", &[]);
    let mut c = r.client();
    let t = create(&mut c, &delegate(&[("Count", "Count slowly [fake:slow]")], &["poll 30"]));
    until("the poll to finish", || agent_words(&transcript(&mut c, &t.id)).contains("polled 30 times"));
    let polls: Vec<Value> = r.mcp_calls().into_iter().filter(|m| m["tool"] == "subagents").collect();
    assert_eq!(polls.len(), 30, "every call was made and logged");
    let slowest = polls.iter().filter_map(|m| m["ms"].as_u64()).max().unwrap();
    assert!(slowest < 1000, "the tool never blocks: slowest {slowest} ms");
    assert!(polls.iter().all(|m| m["error"] == false && m["text"].as_str().unwrap().contains("Do not call this to wait")), "{:?}", polls[0]);
    assert!(polls[0]["text"].as_str().unwrap().contains("Subagent 1 · Count · working"), "{}", polls[0]["text"]);
    assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    until("no process left", || processes_of(&r.d).is_empty());
}

#[test]
fn three_subagents_of_a_thread_run_at_once_and_the_rest_queue() {
    let r = rig("limits", &[]);
    let mut c = r.client();
    let subs: Vec<(String, &str)> = (1..=5).map(|i| (format!("Count {i}"), "Count slowly [fake:slow]")).collect();
    let pairs: Vec<(&str, &str)> = subs.iter().map(|(t, p)| (t.as_str(), *p)).collect();
    let t = create(&mut c, &delegate(&pairs, &[]));
    until("three working and two queued", || {
        let s = subagents(&mut c, &t.id);
        s.len() == 5 && s.iter().filter(|x| x.status == TaskStatus::Working).count() == 3 && s.iter().filter(|x| x.status == TaskStatus::New).count() == 2
    });
    let calls = r.mcp_calls();
    assert!(calls[3].get("text").unwrap().as_str().unwrap().contains("Subagent 4") && calls[3]["text"].as_str().unwrap().contains("queued"), "the agent is told: {}", calls[3]["text"]);
    assert!(calls[0]["text"].as_str().unwrap().contains("end your turn now"), "{}", calls[0]["text"]);
    assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    until("all stopped, queued ones too", || subagents(&mut c, &t.id).iter().all(|s| s.status == TaskStatus::Stopped));
    until("no process left", || processes_of(&r.d).is_empty());
}

#[test]
fn only_four_runs_work_at_once_across_the_daemon() {
    let r = rig("cap", &[]);
    let mut c = r.client();
    let a = create(&mut c, &delegate(&[("A1", "Count [fake:slow]"), ("A2", "Count [fake:slow]"), ("A3", "Count [fake:slow]")], &[]));
    let b = create(&mut c, &delegate(&[("B1", "Count [fake:slow]"), ("B2", "Count [fake:slow]"), ("B3", "Count [fake:slow]")], &[]));
    until("six subagents made", || subagents(&mut c, &a.id).len() == 3 && subagents(&mut c, &b.id).len() == 3);
    until("four working and two queued", || {
        let all: Vec<Task> = tasks(&mut c).into_iter().filter(|t| t.subagent.is_some()).collect();
        all.iter().filter(|x| x.status == TaskStatus::Working).count() == 4 && all.iter().filter(|x| x.status == TaskStatus::New).count() == 2
    });
    std::thread::sleep(Duration::from_millis(600));
    let all: Vec<Task> = tasks(&mut c).into_iter().filter(|t| t.subagent.is_some()).collect();
    assert_eq!(all.iter().filter(|x| x.status == TaskStatus::Working).count(), 4, "never a fifth");
    for t in [&a, &b] {
        assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    }
    until("no process left", || processes_of(&r.d).is_empty());
}

#[test]
fn a_subagent_asks_for_the_same_permissions_as_its_thread_and_the_request_says_who() {
    let r = rig("perm", &[]);
    let mut c = r.client();
    set_mode(&mut c, PermissionMode::Ask);
    let t = create(&mut c, &delegate(&[("Run the tests", "Run them [fake:permission]")], &[]));
    until("a request from the subagent", || match c.request(Request::ListNeeds).unwrap() {
        Reply::Needs { items } => items.iter().any(|i| i.subagent.is_some()),
        _ => false,
    });
    let items = match c.request(Request::ListNeeds).unwrap() {
        Reply::Needs { items } => items,
        r => panic!("{r:?}"),
    };
    assert_eq!(items.len(), 1, "the thread's own call to Neko's tool asked nothing: {items:?}");
    let item = &items[0];
    let sub = subagents(&mut c, &t.id).remove(0);
    assert_eq!(item.task, sub.id, "the request is the subagent's");
    assert_eq!(item.subagent.as_deref(), Some("1"));
    assert_eq!(item.title, t.title, "shown under its thread's title");
    let Need::Permission { permission } = &item.need else { panic!("{item:?}") };
    assert_eq!(permission.tool, "Bash");
    // A screen is told: the thread is pushed again with its flag on, though nothing in the thread itself changed.
    let end = Instant::now() + T;
    let mut heard = false;
    while !heard && Instant::now() < end {
        if let Some(ServerMessage::Event { change: Change::TaskUpdated { task }, .. }) = c.next_event(Duration::from_millis(200)).unwrap() {
            heard = task.id == t.id && task.needs_you;
        }
    }
    assert!(heard, "the thread's row turns amber on its own");
    let th = task(&mut c, &t.id);
    assert!(th.needs_you, "the thread's run has ended, yet a request of its subagent makes it need the person");
    assert!(th.needs_you);
    until("the thread to rest", || task(&mut c, &t.id).thread_status == ThreadStatus::Waiting);
    // It is asked in Ask mode like the thread would: the person says no, and the subagent reports that.
    assert_eq!(c.request(Request::AnswerPermission { permission: permission.id.clone(), decision: Decision::Deny }).unwrap(), Reply::Ok);
    until("the thread to read the report and finish", || {
        let tr = transcript(&mut c, &t.id);
        tr.runs.len() == 2 && tr.runs[1].status == RunStatus::Done
    });
    let msg = r.messages().into_iter().map(|(_, m)| m).find(|m| m.starts_with("<neko-wake")).unwrap();
    assert!(msg.contains("Okay, I did not run the tests."), "{msg}");
    // "Always allow" is the task's own: the subagent's start empty, whatever was said for the thread.
    let sub_tr = transcript(&mut c, &sub.id);
    assert_eq!(sub_tr.permissions.len(), 1);
    assert_eq!(sub_tr.permissions[0].state, PermissionState::Denied);
}

#[test]
fn a_subagent_is_read_not_talked_to_and_goes_with_its_thread() {
    let r = rig("guards", &[]);
    let mut c = r.client();
    let t = create(&mut c, &delegate(&[("Quick", "Hello [fake:plain]")], &[]));
    until("the thread to be done", || {
        let tr = transcript(&mut c, &t.id);
        tr.runs.len() == 2 && task(&mut c, &t.id).status == TaskStatus::Done
    });
    let sub = subagents(&mut c, &t.id).remove(0);
    match c.request(Request::FollowUp { task: sub.id.clone(), words: "More".into(), answering: None }).unwrap() {
        Reply::Error { code: ErrorCode::Refused, message } => assert!(message.contains("takes no messages yet"), "{message}"),
        r => panic!("{r:?}"),
    }
    match c.request(Request::RemoveTask { task: sub.id.clone() }).unwrap() {
        Reply::Error { code: ErrorCode::Refused, message } => assert!(message.contains("goes with its thread"), "{message}"),
        r => panic!("{r:?}"),
    }
    assert_eq!(c.request(Request::RemoveTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    assert!(tasks(&mut c).is_empty(), "the subagent went with its thread: {:?}", tasks(&mut c));
}

#[test]
fn a_reached_limit_holds_back_the_wake_until_it_resets() {
    let r = rig("limit", &[]);
    let mut c = r.client();
    let t = create(&mut c, &delegate(&[("Limited", "Go [fake:limit]")], &[]));
    until("the subagent to fail", || subagents(&mut c, &t.id).first().is_some_and(|s| s.status == TaskStatus::Failed));
    std::thread::sleep(Duration::from_millis(1500));
    assert_eq!(task(&mut c, &t.id).status, TaskStatus::Waiting, "the thread is not resumed while the limit holds");
    assert_eq!(transcript(&mut c, &t.id).runs.len(), 1);
    assert_eq!(r.argvs().len(), 2);
}

#[test]
fn the_user_can_have_runs_keep_their_own_mcp_servers() {
    let r = rig("own-mcp", &[]);
    let mut c = r.client();
    let s = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: true };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
    let t = create(&mut c, "Hello [fake:plain]");
    until("done", || task(&mut c, &t.id).status == TaskStatus::Done);
    let argvs = r.argvs();
    assert!(!argvs[0].iter().any(|a| a == "--strict-mcp-config"), "{:?}", argvs[0]);
    assert!(argvs[0].iter().any(|a| a == "--mcp-config"), "Neko's own server is still given");
}

#[test]
fn a_run_cannot_reach_neko_with_another_runs_secret() {
    let r = rig("secret", &[]);
    let _c = r.client();
    use std::io::{Read, Write};
    let port = r.d.port();
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    let body = r#"{"op":"call","tool":"subagents","arguments":{}}"#;
    write!(s, "POST /v1/mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer not-a-run-secret\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
    let mut reply = String::new();
    s.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 401"), "{reply}");
    // The install token is no run's secret either.
    let mut s = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
    write!(s, "POST /v1/mcp HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", r.d.token(), body.len()).unwrap();
    let mut reply = String::new();
    s.read_to_string(&mut reply).unwrap();
    assert!(reply.starts_with("HTTP/1.1 401"), "{reply}");
}
