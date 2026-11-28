//! Making a Cattain by asking (docs/knowledge/cats.md, "Made by asking"), in the real daemon with the fake CLI as `claude` and Neko's own
//! MCP server really started by it: a thread's agent lists the Cattains and makes one; every screen hears the cat (with the thread that
//! made it) and its RULES.md; the thread gets the note; Undo retires it. A kitten is not offered the tool, and is refused if it calls it;
//! the person can keep Cattains to making by hand; at 16 the agent is refused with the house listed. Every wait has a deadline.

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

fn rig(name: &str) -> Rig {
    let dir = temp_dir(name);
    let record = dir.join("record.jsonl");
    let r = record.to_string_lossy().into_owned();
    Rig { d: Daemon::start_with(dir, &[("NEKO_FAKE_RECORD", &r), ("NEKO_WAKE_BATCH_MS", "300")]), record }
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

    /// The tools Neko's server offered, by the label of the `tools` step that read them.
    fn offered(&self, label: &str) -> Vec<String> {
        self.recorded().iter().find(|v| v["label"] == label).and_then(|v| v["neko_tools"].as_str().map(|n| n.split(',').map(str::to_owned).collect())).unwrap_or_default()
    }

    /// What the runs were handed as their system text, in order.
    fn appended(&self) -> Vec<String> {
        self.recorded()
            .iter()
            .filter_map(|v| v["argv"].as_array().cloned())
            .filter_map(|a| a.iter().position(|x| x == "--append-system-prompt").and_then(|i| a.get(i + 1)).and_then(Value::as_str).map(str::to_owned))
            .collect()
    }
}

fn create(c: &mut Client, words: &str) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: None, in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
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

fn cats(c: &mut Client) -> Vec<Cat> {
    match c.request(Request::ListCats).unwrap() {
        Reply::Cats { cats } => cats,
        r => panic!("cats: {r:?}"),
    }
}

fn done(c: &mut Client, id: &TaskId) {
    until("the thread to end", || {
        let t = task(c, id);
        t.status == TaskStatus::Done && t.thread_status == ThreadStatus::Idle
    });
}

/// The words of a thread whose fake agent plays `steps` through the real MCP hop, one per line.
fn script(steps: &[String]) -> String {
    let mut w = String::from("Make a Cattain for the iOS work [fake:mcp]");
    for s in steps {
        w.push('\n');
        w.push_str(s);
    }
    w
}

fn make(args: Value) -> String {
    format!("mcp neko make_cattain {args}")
}

fn notes(t: &Transcript) -> Vec<(String, String, Option<String>)> {
    t.parts.iter().filter_map(|p| if let PartBody::CattainMade { cat, name, purpose } = &p.body { Some((cat.clone(), name.clone(), purpose.clone())) } else { None }).collect()
}

fn events(c: &mut Client) -> Vec<Change> {
    let mut got = Vec::new();
    while let Some(ServerMessage::Event { change, .. }) = c.next_event(Duration::from_millis(100)).unwrap() {
        got.push(change);
    }
    got
}

fn set_by_hand(c: &mut Client, by_hand: bool) {
    let s = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: false, queue_first: false, ask_first_subagent: false, cattains_by_hand: by_hand };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

#[test]
fn a_threads_agent_makes_a_cattain_every_screen_hears_it_the_thread_has_the_note_and_undo_retires_it() {
    let r = rig("make-cattain");
    let mut c = r.client();
    let mut watch = r.client();
    let t = create(&mut c, &script(&["tools d0".into(), "mcp neko list_cattains {}".into(), make(json!({"purpose": "iOS work"}))]));
    done(&mut c, &t.id);
    // The agent was offered both tools, read the house, and was told the new one's name and to tell the person.
    let offered = r.offered("d0");
    assert!(offered.iter().any(|n| n == "make_cattain") && offered.iter().any(|n| n == "list_cattains"), "{offered:?}");
    let listed = r.mcp_calls("list_cattains");
    assert_eq!(listed.len(), 1);
    assert!(listed[0]["text"].as_str().unwrap().starts_with("3 of at most 16 Cattains are at work:\n- miso: at rest"), "{}", listed[0]["text"]);
    let made = r.mcp_calls("make_cattain");
    assert_eq!((made.len(), made[0]["error"].as_bool()), (1, Some(false)), "{made:?}");
    let said = made[0]["text"].as_str().unwrap();
    assert!(said.starts_with("Made a Cattain: yuzu (iOS work). Address it as yuzu.") && said.contains("Tell the person you made yuzu"), "{said}");
    assert!(made[0]["ms"].as_u64().unwrap() < 2000, "it answers at once");
    // The cat, with the thread that made it, the first free face and RULES.md on disk.
    let yuzu = cats(&mut c).into_iter().find(|x| x.name == "yuzu").expect("yuzu is in the house");
    assert_eq!((yuzu.face.as_str(), yuzu.purpose.as_deref(), yuzu.made_in.as_ref(), yuzu.thread.as_ref()), ("tabby", Some("iOS work"), Some(&t.id), None));
    let Reply::CattainFile { file } = c.request(Request::GetCattainFile { cat: yuzu.id.clone(), name: "RULES.md".into() }).unwrap() else { panic!("the file") };
    assert_eq!((file.text.as_str(), file.previous), ("Stay on your purpose (iOS work); ask the person before you take on other work.\n", false));
    // Every screen heard the cat and its file; the thread has the note, once, and the step.
    let heard = events(&mut watch);
    assert!(heard.iter().any(|ch| matches!(ch, Change::CatAdded { cat } if cat.id == yuzu.id && cat.made_in.as_ref() == Some(&t.id))), "cat_added with made_in");
    assert!(heard.iter().any(|ch| matches!(ch, Change::CattainFileChanged { file } if file.cat == yuzu.id && file.name == "RULES.md")));
    let tr = transcript(&mut c, &t.id);
    assert_eq!(notes(&tr), [(yuzu.id.clone(), "yuzu".to_owned(), Some("iOS work".to_owned()))]);
    assert!(tr.parts.iter().any(|p| matches!(&p.body, PartBody::Tool { name, summary, .. } if name == "mcp__neko__make_cattain" && summary == "iOS work")));
    // The same purpose again names yuzu, and makes nothing.
    let again = create(&mut c, &script(&[make(json!({"purpose": "ios work"}))]));
    done(&mut c, &again.id);
    let said = r.mcp_calls("make_cattain")[1]["text"].as_str().unwrap().to_owned();
    assert!(said.starts_with("yuzu already looks after this (iOS work), so no Cattain was made."), "{said}");
    assert!(notes(&transcript(&mut c, &again.id)).is_empty(), "no note when nothing was made");
    assert_eq!(cats(&mut c).len(), 4);
    // Undo: retired, and every screen hears it. A cat made by hand cannot be undone.
    let Reply::Cat { cat } = c.request(Request::UndoMakeCattain { cat: yuzu.id.clone() }).unwrap() else { panic!("undo") };
    assert!(!cat.is_active());
    assert!(events(&mut watch).iter().any(|ch| matches!(ch, Change::CatUpdated { cat } if cat.id == yuzu.id && !cat.is_active())));
    assert!(matches!(c.request(Request::UndoMakeCattain { cat: "cat_miso".into() }).unwrap(), Reply::Error { code: ErrorCode::Refused, message } if message.contains("made by hand")));
    // The thread's agent is told about Cattains; the words say when to make one.
    let told = r.appended();
    assert!(told[0].contains("mcp__neko__make_cattain only when the person asks for one"), "{}", told[0]);
}

#[test]
fn a_kitten_is_not_offered_the_tool_and_is_refused_if_it_calls_it() {
    let r = rig("make-cattain-kitten");
    let mut c = r.client();
    let prompt = format!("[fake:steps]\ntools k1\n{}\nmcp neko list_cattains {{}}", make(json!({"purpose": "iOS work"})));
    let t = create(&mut c, &script(&[format!("mcp neko start_subagent {}", json!({"title": "Look", "prompt": prompt}))]));
    until("the kitten's report to wake the thread and the thread to end", || {
        let x = task(&mut c, &t.id);
        x.status == TaskStatus::Done && x.thread_status == ThreadStatus::Idle && r.recorded().iter().any(|v| v["label"] == "k1")
    });
    assert!(!r.offered("k1").iter().any(|n| n.contains("cattain")), "{:?}", r.offered("k1"));
    let calls: Vec<Value> = r.recorded().iter().filter_map(|v| v.get("mcp_call").cloned()).filter(|m| m["tool"].as_str().is_some_and(|n| n.contains("cattain"))).collect();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls.iter().all(|m| m["error"] == true && m["text"].as_str().unwrap().contains("No such tool available")), "{calls:?}");
    assert_eq!(cats(&mut c).len(), 3, "nothing made");
    // A kitten is never told about Cattains.
    let told = r.appended();
    assert!(told.iter().any(|a| a.contains("You are Subagent 1") && !a.contains("make_cattain")));
}

#[test]
fn the_person_can_keep_cattains_to_making_by_hand() {
    let r = rig("make-cattain-by-hand");
    let mut c = r.client();
    set_by_hand(&mut c, true);
    let t = create(&mut c, &script(&["tools d0".into(), make(json!({"purpose": "iOS work"}))]));
    done(&mut c, &t.id);
    assert!(!r.offered("d0").iter().any(|n| n.contains("cattain")), "not offered");
    assert!(r.mcp_calls("make_cattain")[0]["text"].as_str().unwrap().contains("No such tool available"));
    assert!(!r.appended()[0].contains("make_cattain"), "not told");
    assert_eq!(cats(&mut c).len(), 3);
    set_by_hand(&mut c, false);
    let t = create(&mut c, &script(&["tools d1".into()]));
    done(&mut c, &t.id);
    assert!(r.offered("d1").iter().any(|n| n == "make_cattain"), "on again");
}

#[test]
fn at_sixteen_the_agent_is_refused_with_the_house_listed_and_nothing_is_made() {
    let r = rig("make-cattain-limit");
    let mut c = r.client();
    for i in 0..13 {
        assert!(matches!(c.request(Request::CreateCat { name: format!("cat {i}"), face: None, purpose: Some(format!("Work {i}")), workspace: None }).unwrap(), Reply::Cat { .. }));
    }
    let t = create(&mut c, &script(&[make(json!({"purpose": "iOS work"}))]));
    done(&mut c, &t.id);
    let m = r.mcp_calls("make_cattain").remove(0);
    let said = m["text"].as_str().unwrap();
    assert_eq!(m["error"], true);
    assert!(said.starts_with("Neko keeps at most 16 Cattains at work, and 16 are:\n- miso\n- mochi\n- tofu\n- cat 0 (Work 0)\n") && said.contains("No Cattain was made"), "{said}");
    assert_eq!(cats(&mut c).len(), 16);
    assert!(notes(&transcript(&mut c, &t.id)).is_empty());
}
