//! A Cattain runs its crew (docs/knowledge/cats.md, "The crew"), in the real daemon with the fake CLI as `claude` and Neko's own MCP
//! server really started by it: miso's agent starts crew threads with `start_crew_thread`, rests Waiting on them, and is woken by their
//! turn ends in one batch (a `<neko-wake kind="crew">` each), after which it rewrites DIGEST.md, which every screen hears. A crew
//! thread's request reaches miso once and stays the person's to answer; miso's words reach a crew thread inside `<neko-from>`, and a
//! question the thread put to the person is not miso's to answer. A crew thread never has more access than miso. Only a Cattain's own
//! thread is offered the crew tools. Every wait has a deadline.

mod support;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use serde_json::{Value, json};
use support::*;

const T: Duration = Duration::from_secs(60);

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

fn rig(name: &str, env: &[(&str, &str)]) -> Rig {
    let dir = temp_dir(name);
    let record = dir.join("record.jsonl");
    let r = record.to_string_lossy().into_owned();
    let mut all: Vec<(&str, &str)> = vec![("NEKO_FAKE_RECORD", &r), ("NEKO_WAKE_BATCH_MS", "300"), ("NEKO_CREW_BATCH_MS", "1500")];
    all.extend_from_slice(env);
    Rig { d: Daemon::start_with(dir, &all), record }
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

    fn mcp_calls(&self, tool: &str) -> Vec<Value> {
        self.recorded().iter().filter_map(|v| v.get("mcp_call").cloned()).filter(|m| m["tool"] == tool).collect()
    }

    fn offered(&self, label: &str) -> Vec<String> {
        self.recorded().iter().find(|v| v["label"] == label).and_then(|v| v["neko_tools"].as_str().map(|n| n.split(',').map(str::to_owned).collect())).unwrap_or_default()
    }

    /// The messages the fake CLIs read on stdin, in order, as the user messages' text.
    fn said(&self) -> Vec<String> {
        self.recorded()
            .iter()
            .filter_map(|v| v["stdin"].as_object().filter(|m| m.get("type").and_then(Value::as_str) == Some("user")).cloned())
            .map(|m| m["message"]["content"].as_array().map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect()).or_else(|| m["message"]["content"].as_str().map(str::to_owned)).unwrap_or_default())
            .collect()
    }

    /// Each run's start, in order: its arguments.
    fn starts(&self) -> Vec<Vec<String>> {
        self.recorded().iter().filter_map(|v| v["argv"].as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect::<Vec<_>>())).filter(|a| a.iter().any(|x| x == "-p")).collect()
    }
}

fn task(c: &mut Client, id: &TaskId) -> Task {
    match c.request(Request::GetTask { task: id.clone() }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("task: {r:?}"),
    }
}

fn tasks(c: &mut Client) -> Vec<Task> {
    match c.request(Request::ListTasks).unwrap() {
        Reply::Tasks { tasks } => tasks,
        r => panic!("tasks: {r:?}"),
    }
}

fn transcript(c: &mut Client, id: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn set_mode(c: &mut Client, mode: PermissionMode) {
    let s = Settings { runtime: "claude".into(), permission_mode: mode, own_mcp: false, queue_first: false, ask_first_subagent: false, cattains_by_hand: false };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

/// miso's own thread, opened.
fn miso(c: &mut Client) -> Task {
    let Reply::Task { task } = c.request(Request::OpenCattain { cat: "cat_miso".into() }).unwrap() else { panic!("open_cattain") };
    task
}

/// The person writes in a thread; its fake agent plays `steps` through the real MCP hop.
fn write(c: &mut Client, thread: &TaskId, steps: &[String]) {
    let mut w = String::from("Fix the flaky tests, one thread each [fake:mcp]");
    for s in steps {
        w.push('\n');
        w.push_str(s);
    }
    assert_eq!(c.request(Request::FollowUp { task: thread.clone(), words: w, answering: None, mode: None }).unwrap(), Reply::Ok);
}

fn start_crew(title: &str, brief: &str) -> String {
    format!("mcp neko start_crew_thread {}", json!({"title": title, "brief": brief}))
}

fn crew_of_miso(c: &mut Client) -> Vec<Task> {
    let mut v: Vec<Task> = tasks(c).into_iter().filter(|t| t.started_by.as_deref() == Some("cat_miso")).collect();
    v.sort_by(|a, b| (a.created_ms, &a.id.0).cmp(&(b.created_ms, &b.id.0)));
    v
}

/// miso's crew threads with these titles, in this order (threads made in the same millisecond have no order of their own).
fn titled(c: &mut Client, titles: &[&str]) -> Vec<Task> {
    let all = crew_of_miso(c);
    titles.iter().filter_map(|n| all.iter().find(|t| t.title == *n).cloned()).collect()
}

fn crew_wakes(t: &Transcript) -> Vec<(TaskId, String, String)> {
    t.parts.iter().filter_map(|p| if let PartBody::CrewWake { thread, status, summary, .. } = &p.body { Some((thread.clone(), status.clone(), summary.clone())) } else { None }).collect()
}

fn digest(c: &mut Client) -> String {
    match c.request(Request::GetCattainFile { cat: "cat_miso".into(), name: "DIGEST.md".into() }).unwrap() {
        Reply::CattainFile { file } => file.text,
        r => panic!("{r:?}"),
    }
}

#[test]
fn a_cattain_starts_three_crew_threads_rests_on_them_and_is_woken_once_with_their_news_and_keeps_its_digest() {
    let r = rig("crew-three", &[]);
    let mut c = r.client();
    let own = miso(&mut c);
    write(
        &mut c,
        &own.id,
        &["tools own".into(), start_crew("Fix test one", "Fix the first flaky test [fake:work:3]"), start_crew("Fix test two", "Fix the second [fake:work:3]"), start_crew("Fix test three", "Fix the third [fake:work:3]")],
    );
    until("three crew threads", || crew_of_miso(&mut c).len() == 3);
    let crew = titled(&mut c, &["Fix test one", "Fix test two", "Fix test three"]);
    assert_eq!(crew.len(), 3, "the titles miso gave");
    for t in &crew {
        assert_eq!((t.cat.as_deref(), t.crew_of(), t.cattain, t.listed()), (Some("cat_miso"), Some("cat_miso"), false, true), "an ordinary thread in miso's crew");
        assert!(t.words.starts_with("Fix the"), "its words are miso's brief");
    }
    // The agent was offered the crew tools, and each start answered at once with the thread's id and the cue to end its turn.
    assert!(r.offered("own").iter().any(|n| n == "start_crew_thread") && r.offered("own").iter().any(|n| n == "message_crew"), "{:?}", r.offered("own"));
    let started = r.mcp_calls("start_crew_thread");
    assert_eq!(started.len(), 3);
    for (s, t) in started.iter().zip(&crew) {
        let said = s["text"].as_str().unwrap();
        assert!(s["error"] == false && said.contains(&t.id.0) && said.contains("Do not poll") && s["ms"].as_u64().unwrap() < 2000, "{said}");
    }
    // miso's turn ends while its crew works: it rests, Waiting on them, with no process.
    until("miso to wait on its crew", || task(&mut c, &own.id).status == TaskStatus::Waiting);
    assert_eq!(task(&mut c, &own.id).thread_status, ThreadStatus::Waiting);
    // Each crew thread ends; miso is woken with their news, in at most two batches, and rewrites its digest.
    until("the crew to finish", || crew_of_miso(&mut c).iter().all(|t| t.status == TaskStatus::Done));
    until("miso to read all three", || crew_wakes(&transcript(&mut c, &own.id)).len() == 3);
    until("miso to be done", || task(&mut c, &own.id).status == TaskStatus::Done);
    let wakes: Vec<String> = r.said().into_iter().filter(|m| m.starts_with("<neko-wake")).collect();
    assert!((1..=2).contains(&wakes.len()), "{} wake runs", wakes.len());
    assert_eq!(wakes.iter().map(|w| w.matches("<neko-wake kind=\"crew\"").count()).sum::<usize>(), 3, "one element per crew thread");
    let all = wakes.join("\n");
    for t in &crew {
        assert!(all.contains(&format!("thread=\"{}\" status=\"done\" title=\"{}\"", t.id.0, t.title)), "{all}");
    }
    assert!(wakes.iter().all(|w| w.ends_with(neko_core::crew::WAKE_CUE)), "each ends on what to do");
    let d = digest(&mut c);
    assert!(d.starts_with("## Needs you") && d.contains("## New\n\n- Fix test") && d.contains("Fix test three: done."), "{d}");
    // A wake of miso's, resumed: the same conversation, the same block.
    let starts = r.starts();
    let own_runs: Vec<&Vec<String>> = starts.iter().filter(|a| a.iter().any(|x| x.contains("<neko-cattain>"))).collect();
    assert!(own_runs.len() >= 2 && own_runs[1..].iter().all(|a| a.iter().any(|x| x == "--resume")), "the wake resumes miso's conversation");
    // A crew thread is told whose crew it is in and that miso hears it; it is not offered the crew tools.
    let crew_run = starts.iter().find(|a| a.iter().any(|x| x.contains("<neko-crew>"))).unwrap();
    assert!(crew_run.iter().any(|x| x.contains("miso hears when each of your turns ends")));
}

#[test]
fn a_crew_threads_request_tells_its_cattain_once_and_only_the_person_answers_it() {
    let r = rig("crew-needs-you", &[]);
    let mut c = r.client();
    set_mode(&mut c, PermissionMode::Ask);
    let own = miso(&mut c);
    write(&mut c, &own.id, &[start_crew("Run the tests", "Run the test suite [fake:hook-shell]"), "stay 8".into()]);
    until("the crew thread", || crew_of_miso(&mut c).len() == 1);
    let t = crew_of_miso(&mut c).remove(0);
    until("its request", || task(&mut c, &t.id).needs_you);
    // The person's Needs you has it, under miso.
    let Reply::Needs { items } = c.request(Request::ListNeeds).unwrap() else { panic!("needs") };
    let item = items.iter().find(|i| i.task == t.id).expect("the request is in Needs you");
    assert_eq!(item.cat, "cat_miso");
    let Need::Permission { permission } = &item.need else { panic!("a permission") };
    // miso hears it once, as a request it cannot approve.
    until("miso to hear the request", || crew_wakes(&transcript(&mut c, &own.id)).iter().any(|(_, s, _)| s == "needs_you"));
    let heard: Vec<String> = r.said().into_iter().filter(|m| m.contains("status=\"needs_you\"")).collect();
    assert_eq!(heard.len(), 1, "{:#?}", r.said());
    assert!(heard[0].contains("<request>") && heard[0].contains("You cannot approve it"), "{}", heard[0]);
    // Nothing of miso's answered it: the person does.
    assert_eq!(task(&mut c, &t.id).status, TaskStatus::NeedsYou);
    assert_eq!(c.request(Request::AnswerPermission { permission: permission.id.clone(), decision: Decision::AllowOnce }).unwrap(), Reply::Ok);
    until("the crew thread to finish", || task(&mut c, &t.id).status == TaskStatus::Done);
    until("miso to hear it ended", || crew_wakes(&transcript(&mut c, &own.id)).iter().any(|(id, s, _)| id == &t.id && s == "done"));
    assert_eq!(crew_wakes(&transcript(&mut c, &own.id)).iter().filter(|(_, s, _)| s == "needs_you").count(), 1, "the request told miso once");
}

#[test]
fn miso_writes_to_its_crew_inside_neko_from_and_cannot_answer_a_question_put_to_the_person() {
    let r = rig("crew-message", &[]);
    let mut c = r.client();
    let own = miso(&mut c);
    write(&mut c, &own.id, &[start_crew("Tidy docs", "Tidy the docs [fake:plain]"), start_crew("Pick a branch", "Ask which branch [fake:steps]\nmcp neko ask_person {\"question\":\"Which branch should I use?\"}")]);
    until("both crew threads to rest", || {
        let v = titled(&mut c, &["Tidy docs", "Pick a branch"]);
        v.len() == 2 && v[0].status == TaskStatus::Done && v[1].status == TaskStatus::Asked
    });
    let crew = titled(&mut c, &["Tidy docs", "Pick a branch"]);
    until("miso to be done", || matches!(task(&mut c, &own.id).status, TaskStatus::Done) && crew_wakes(&transcript(&mut c, &own.id)).len() == 2);
    let asked = crew_wakes(&transcript(&mut c, &own.id)).into_iter().find(|(id, ..)| id == &crew[1].id).unwrap();
    assert_eq!(asked.1, "asked");
    // The person tells miso; miso writes to both: the one at rest works again with miso's words, the question is refused.
    let to = |id: &TaskId, text: &str| format!("mcp neko message_crew {}", json!({"id": id.0, "text": text}));
    write(&mut c, &own.id, &["mcp neko crew {}".into(), to(&crew[0].id, "Also fix the typos [fake:plain]"), to(&crew[1].id, "Use main.")]);
    until("both messages", || r.mcp_calls("message_crew").len() == 2);
    let calls = r.mcp_calls("message_crew");
    assert_eq!(calls[0]["error"], false, "{}", calls[0]["text"]);
    assert!(calls[1]["error"] == true && calls[1]["text"].as_str().unwrap().contains("only they answer it"), "{}", calls[1]["text"]);
    let listed = r.mcp_calls("crew")[0]["text"].as_str().unwrap().to_owned();
    assert!(listed.contains(&format!("{} · Tidy docs · done", crew[0].id.0)) && listed.contains("Pick a branch · asked the person"), "{listed}");
    until("the first to read miso's words", || r.said().iter().any(|m| m.starts_with("<neko-from cattain=\"miso\">\nAlso fix the typos")));
    until("it ends again", || task(&mut c, &crew[0].id).status == TaskStatus::Done && transcript(&mut c, &crew[0].id).runs.len() == 2);
    // Its transcript keeps miso's words as they were sent; the question stays the person's.
    assert!(transcript(&mut c, &crew[0].id).parts.iter().any(|p| matches!(&p.body, PartBody::Person { text } if neko_core::crew::read_from_cattain(text) == Some(("miso", "Also fix the typos [fake:plain]")))));
    assert_eq!(task(&mut c, &crew[1].id).status, TaskStatus::Asked);
}

#[test]
fn a_crew_thread_never_has_more_access_than_its_cattain() {
    let r = rig("crew-access", &[]);
    let mut c = r.client();
    set_mode(&mut c, PermissionMode::Full);
    let Reply::Cat { .. } = c.request(Request::SetCatAccess { cat: "cat_miso".into(), access: Some(PermissionMode::Ask) }).unwrap() else { panic!("access") };
    let own = miso(&mut c);
    write(&mut c, &own.id, &[start_crew("Read", "Read the README [fake:plain]")]);
    until("the crew thread to end", || crew_of_miso(&mut c).first().is_some_and(|t| t.status == TaskStatus::Done));
    let starts = r.starts();
    let crew_run = starts.iter().find(|a| a.iter().any(|x| x.contains("<neko-crew>"))).unwrap();
    assert!(crew_run.iter().any(|x| x == "--settings"), "miso asks before risky steps, so its crew does too, though Settings say Full access");
    assert!(!crew_run.iter().any(|x| x == "--dangerously-skip-permissions"));
}

#[test]
fn only_a_cattains_own_thread_is_offered_the_crew_tools() {
    let r = rig("crew-only-cattain", &[]);
    let mut c = r.client();
    let words = "Look around [fake:mcp]\ntools thread\nmcp neko start_crew_thread {\"title\":\"X\",\"brief\":\"Y [fake:plain]\"}\nmcp neko crew {}";
    let Reply::Task { task: t } = c.request(Request::CreateTask { crew: None, words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() else { panic!("create") };
    until("it ends", || task(&mut c, &t.id).status == TaskStatus::Done);
    assert!(!r.offered("thread").iter().any(|n| n.contains("crew")), "{:?}", r.offered("thread"));
    // Claude Code has no such tool for it: the call fails before it reaches Neko (and Neko refuses it too, `crew.rs`).
    let started = r.mcp_calls("start_crew_thread");
    assert!(started[0]["error"] == true && started[0]["text"].as_str().unwrap().contains("No such tool"), "{started:?}");
    assert!(r.mcp_calls("crew")[0]["error"] == true);
    assert!(tasks(&mut c).iter().all(|t| t.started_by.is_none()), "nothing was started");
}
