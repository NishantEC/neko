//! Questions go up (docs/knowledge/subagents.md, "Questions go up"), in the real daemon with the fake CLI as `claude` and Neko's own MCP
//! server really started by it: a kitten asks the agent that started it with `ask_parent` and ends its turn; that agent is woken with
//! `kind="question"`, answers with `message_subagent` (the kitten resumes with the words) or passes it up with `ask_person` (the person
//! sees it in Needs you, labelled with the kitten, and their answer goes back down); a question goes up one level at a time; one the
//! agent left unanswered goes up by itself; reports wait while a thread waits for the person; Stop takes a waiting question back.
//! Every wait has a deadline and waits on what it needs.

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

fn rig(name: &str, extra: &[(&str, &str)]) -> Rig {
    let dir = temp_dir(name);
    let record = dir.join("record.jsonl");
    let r = record.to_string_lossy().into_owned();
    let mut env: Vec<(&str, &str)> = vec![("NEKO_FAKE_RECORD", &r), ("NEKO_WAKE_BATCH_MS", "300")];
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

    /// The user messages the fake CLI read, in order.
    fn messages(&self) -> Vec<String> {
        self.recorded()
            .iter()
            .filter_map(|v| v["stdin"].as_object().filter(|m| m.get("type").and_then(Value::as_str) == Some("user")).map(|m| m["message"]["content"].as_array().map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect::<String>()).unwrap_or_default()))
            .collect()
    }

    fn starts(&self) -> usize {
        self.recorded().iter().filter(|v| v.get("argv").is_some()).count()
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

fn task(c: &mut Client, id: &TaskId) -> Task {
    match c.request(Request::GetTask { task: id.clone() }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("task: {r:?}"),
    }
}

fn find_kitten(c: &mut Client, thread: &TaskId, number: &str) -> Option<Task> {
    tasks(c).into_iter().find(|t| t.subagent.as_ref().is_some_and(|s| s.root == *thread && s.number == number))
}

fn kitten(c: &mut Client, thread: &TaskId, number: &str) -> Task {
    find_kitten(c, thread, number).unwrap_or_else(|| panic!("no kitten {number}"))
}

fn transcript(c: &mut Client, id: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn needs(c: &mut Client) -> Vec<NeedsItem> {
    match c.request(Request::ListNeeds).unwrap() {
        Reply::Needs { items } => items,
        r => panic!("needs: {r:?}"),
    }
}

/// A thread's words: a script that plays `steps` through the real MCP hop (one per line).
fn script(steps: &[String]) -> String {
    let mut w = String::from("Split this up [fake:delegate]");
    for s in steps {
        w.push('\n');
        w.push_str(s);
    }
    w
}

fn start(title: &str, prompt: &str) -> String {
    format!("mcp neko start_subagent {}", json!({"title": title, "prompt": prompt}))
}

/// A kitten's prompt that asks its parent `question` and ends its turn.
fn asks(question: &str) -> String {
    format!("[fake:steps]\nmcp neko ask_parent {}", json!({ "question": question }))
}

const Q: &str = "Which database should the migration target: SQLite or Postgres?";

fn notes(t: &Transcript) -> Vec<(String, String, bool)> {
    t.parts.iter().filter_map(|p| if let PartBody::ToSubagent { number, text, answer } = &p.body { Some((number.clone(), text.clone(), *answer)) } else { None }).collect()
}

fn question_wakes(t: &Transcript) -> Vec<(String, Option<String>)> {
    t.parts.iter().filter_map(|p| if let PartBody::Wake { number, status, question, .. } = &p.body { (status == "asked").then(|| (number.clone(), question.clone())) } else { None }).collect()
}

fn persons(t: &Transcript) -> Vec<String> {
    t.parts.iter().filter_map(|p| if let PartBody::Person { text } = &p.body { Some(text.clone()) } else { None }).collect()
}

/// Every Needs you item that is a question.
fn questions(c: &mut Client) -> Vec<NeedsItem> {
    needs(c).into_iter().filter(|i| matches!(i.need, Need::Question { .. })).collect()
}

fn done(c: &mut Client, id: &TaskId) -> bool {
    let t = task(c, id);
    t.status == TaskStatus::Done && t.thread_status == ThreadStatus::Idle
}

#[test]
fn a_kittens_question_wakes_its_parent_which_answers_and_the_kitten_goes_on() {
    let r = rig("q-answer", &[("NEKO_FAKE_ON_QUESTION", "1=answer:Use SQLite: it is what the tests run on.")]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Plan the migration", &asks(Q))]));
    // The kitten ends asking: its card has the question, with the agent; nothing waits for the person.
    until("the parent to answer", || notes(&transcript(&mut c, &t.id)).len() == 1);
    assert!(questions(&mut c).is_empty(), "a kitten's question is its parent's, not the person's");
    until("the kitten to finish with the answer, and the thread to read its report", || done(&mut c, &t.id) && find_kitten(&mut c, &t.id, "1").is_some_and(|k| k.status == TaskStatus::Done));
    // The parent was woken with the question as a question, and told what to do with it.
    let woken: Vec<String> = r.messages().into_iter().filter(|m| m.contains("kind=\"question\"")).collect();
    assert_eq!(woken.len(), 1, "one question wake: {woken:?}");
    assert!(woken[0].contains(&format!("<question>{Q}</question>")) && woken[0].contains("mcp__neko__message_subagent (id \"1\")") && woken[0].contains("mcp__neko__ask_person"), "{}", woken[0]);
    let tr = transcript(&mut c, &t.id);
    assert_eq!(question_wakes(&tr), [("1".to_owned(), Some(Q.to_owned()))], "the note says which kitten asked, and what");
    assert_eq!(notes(&tr), [("1".to_owned(), "Use SQLite: it is what the tests run on.".to_owned(), true)], "the note says the agent answered");
    // The kitten resumed its own conversation with the answer as its next message, and said so in its report.
    let k = kitten(&mut c, &t.id, "1");
    assert!(k.subagent.as_ref().unwrap().question.is_none(), "an answered question is gone from the card");
    let kt = transcript(&mut c, &k.id);
    assert_eq!(kt.runs.len(), 2, "asked, then answered: two runs");
    assert_eq!(kt.runs[0].ask.as_ref().map(|a| (a.to, a.text.as_str())), Some((AskTo::Parent, Q)), "the first run carries its question");
    assert!(kt.runs[1].session.is_some() && kt.runs[1].session == kt.runs[0].session, "the same conversation, resumed");
    assert_eq!(persons(&kt).iter().filter(|p| *p == "Use SQLite: it is what the tests run on.").count(), 1, "{:?}", persons(&kt));
    let lines: Vec<String> = std::fs::read_to_string(&r.record).unwrap_or_default().lines().filter(|l| l.contains("Use SQLite: it is what the tests run on.")).map(|l| l.chars().take(400).collect()).collect();
    assert_eq!(r.messages().iter().filter(|m| *m == "Use SQLite: it is what the tests run on.").count(), 1, "the kitten read the answer once: {lines:#?}");
    assert!(r.messages().iter().any(|m| m.contains("kind=\"subagent\" id=\"1\" status=\"done\"") && m.contains("You said: Use SQLite")), "its second report wakes the thread");
    assert!(questions(&mut c).is_empty());
}

#[test]
fn a_question_only_the_person_can_answer_reaches_them_and_the_answer_goes_back_down() {
    let r = rig("q-person", &[("NEKO_FAKE_ON_QUESTION", "1=up")]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Plan the migration", &asks(Q))]));
    until("the question in Needs you", || questions(&mut c).len() == 1);
    let item = questions(&mut c).remove(0);
    let Need::Question { run, question, kitten: who, via, unanswered } = item.need.clone() else { unreachable!() };
    assert_eq!((question.as_str(), who.as_deref(), via.len(), unanswered), (Q, Some("1"), 0, false), "Kitten 1 asks, via the agent");
    assert_eq!((item.task.clone(), item.title.clone()), (t.id.clone(), t.title.clone()), "it is the thread's to answer");
    let thread = task(&mut c, &t.id);
    assert_eq!(thread.status, TaskStatus::Asked);
    assert!(thread.needs_you);
    let k = kitten(&mut c, &t.id, "1");
    assert_eq!(k.subagent.as_ref().unwrap().question.as_ref().map(|q| (q.text.as_str(), q.with)), Some((Q, QuestionWith::Person)), "the card says the person has it");
    // The person answers in the thread; the agent passes the answer down.
    let reply = format!("Postgres. [fake:mcp] mcp neko message_subagent {}", json!({"id": "1", "text": "The person says: Postgres."}));
    assert_eq!(c.request(Request::FollowUp { task: t.id.clone(), words: reply, answering: Some(run), mode: None }).unwrap(), Reply::Ok);
    until("the kitten to finish with the answer", || done(&mut c, &t.id) && find_kitten(&mut c, &t.id, "1").is_some_and(|k| k.status == TaskStatus::Done));
    assert!(questions(&mut c).is_empty(), "answered");
    assert_eq!(notes(&transcript(&mut c, &t.id)), [("1".to_owned(), "The person says: Postgres.".to_owned(), true)]);
    let k = kitten(&mut c, &t.id, "1");
    assert!(persons(&transcript(&mut c, &k.id)).iter().any(|p| p == "The person says: Postgres."));
}

#[test]
fn questions_go_up_one_level_at_a_time() {
    let r = rig("q-depth", &[("NEKO_FAKE_ON_QUESTION", "1.1=up|1=up")]);
    let mut c = r.client();
    let p1 = format!("[fake:steps]\n{}", start("Check the schema", &asks(Q)));
    let t = create(&mut c, &script(&[start("Plan the migration", &p1)]));
    until("the question in Needs you", || questions(&mut c).len() == 1);
    let Need::Question { question, kitten: who, via, .. } = questions(&mut c).remove(0).need else { unreachable!() };
    assert_eq!((question.as_str(), who.as_deref(), via.as_slice()), (Q, Some("1.1"), ["1".to_owned()].as_slice()), "Kitten 1.1 asks, via Kitten 1 and the agent");
    // Kitten 1 was woken by its own kitten's question and passed it up; the thread was woken by Kitten 1's, saying where it came from.
    let msgs = r.messages();
    assert!(msgs.iter().any(|m| m.contains("kind=\"question\" id=\"1.1\"") && m.contains("mcp__neko__ask_parent (subagent \"1.1\")")), "Kitten 1 is told to pass it to its parent");
    assert!(msgs.iter().any(|m| m.contains("kind=\"question\" id=\"1\" from=\"1.1\"")), "the thread hears whose it was");
    for n in ["1", "1.1"] {
        let q = kitten(&mut c, &t.id, n).subagent.unwrap().question.unwrap_or_else(|| panic!("Kitten {n} has a question"));
        assert_eq!((q.text.as_str(), q.with), (Q, QuestionWith::Person), "Kitten {n}'s card says the person has it");
    }
    assert_eq!(kitten(&mut c, &t.id, "1").subagent.unwrap().question.unwrap().chain, ["1.1"]);
}

#[test]
fn a_question_the_agent_left_unanswered_goes_up_by_itself() {
    // No rule: the woken thread reads the question and ends its turn without answering it or passing it on.
    let r = rig("q-unanswered", &[]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Plan the migration", &asks(Q))]));
    until("the question in Needs you", || questions(&mut c).len() == 1);
    let Need::Question { question, kitten: who, unanswered, .. } = questions(&mut c).remove(0).need else { unreachable!() };
    assert_eq!((question.as_str(), who.as_deref(), unanswered), (Q, Some("1"), true), "Neko passed it on, and says the agent did not answer");
    let last = transcript(&mut c, &t.id).runs.pop().unwrap();
    assert!(last.ask.as_ref().is_some_and(|a| a.unanswered && a.to == AskTo::Person), "{:?}", last.ask);
}

#[test]
fn reports_wait_while_the_thread_waits_for_the_persons_answer() {
    let r = rig("q-hold", &[("NEKO_FAKE_ON_QUESTION", "1=up")]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Plan the migration", &asks(Q)), start("Count the tables", "Count them [fake:long:3]")]));
    until("the question in Needs you", || questions(&mut c).len() == 1);
    until("Kitten 2 to finish", || find_kitten(&mut c, &t.id, "2").is_some_and(|k| k.status == TaskStatus::Done));
    let before = r.starts();
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(r.starts(), before, "the report waits: nothing wakes a thread that waits for the person");
    assert_eq!(task(&mut c, &t.id).status, TaskStatus::Asked);
    assert_eq!(questions(&mut c).len(), 1, "the question is still the person's");
    let Need::Question { run, .. } = questions(&mut c).remove(0).need else { unreachable!() };
    let reply = format!("SQLite. [fake:mcp] mcp neko message_subagent {}", json!({"id": "1", "text": "SQLite."}));
    assert_eq!(c.request(Request::FollowUp { task: t.id.clone(), words: reply, answering: Some(run), mode: None }).unwrap(), Reply::Ok);
    until("Kitten 2's report to be read", || transcript(&mut c, &t.id).parts.iter().any(|p| matches!(&p.body, PartBody::Wake { number, status, .. } if number == "2" && status == "done")));
    until("everything to end", || done(&mut c, &t.id));
}

#[test]
fn stop_takes_a_waiting_question_back_and_the_tools_refuse_what_they_cannot_do() {
    let r = rig("q-stop", &[]);
    let mut c = r.client();
    // The thread asks the person itself with no kitten named, and writes to kittens it does not have.
    let t = create(
        &mut c,
        &script(&[
            start("Plan the migration", &asks(Q)),
            format!("mcp neko message_subagent {}", json!({"id": "7", "text": "Hello."})),
            format!("mcp neko ask_parent {}", json!({"question": "Me?"})),
            format!("mcp neko ask_person {}", json!({"question": "Pass up what?", "subagent": "1"})),
        ]),
    );
    until("the unanswered question in Needs you", || questions(&mut c).len() == 1);
    let errors: Vec<String> = r.mcp_calls().into_iter().filter(|m| m["error"] == true).map(|m| m["text"].as_str().unwrap_or("").to_owned()).collect();
    assert!(errors.iter().any(|e| e.contains("You have no subagent 7")), "{errors:?}");
    assert!(errors.iter().any(|e| e.contains("No such tool available: ask_parent")), "a thread is not offered ask_parent: {errors:?}");
    assert!(errors.iter().any(|e| e.contains("Subagent 1 has not asked anything")), "the kitten had not asked yet: {errors:?}");
    // Stop on the thread: the kitten waiting on its answer is stopped, and its question goes with it.
    assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    until("the kitten to be stopped", || kitten(&mut c, &t.id, "1").status == TaskStatus::Stopped);
    assert!(kitten(&mut c, &t.id, "1").subagent.unwrap().question.is_none());
    until("the thread to be stopped and its question gone", || task(&mut c, &t.id).status == TaskStatus::Stopped && questions(&mut c).is_empty());
}
