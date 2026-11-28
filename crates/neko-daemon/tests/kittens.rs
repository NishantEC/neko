//! Kittens that work in a copy of their own, follow one another, wait as drafts, nest three deep and queue behind the limits, in the real
//! daemon with the fake CLI and Neko's own MCP server really started by it (docs/knowledge/subagents.md, "Where a kitten works").
//! The projects are real temporary git repositories. Every wait has a deadline and waits on what it needs.

mod support;
use std::path::{Path, PathBuf};
use std::process::Command;
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

    /// The user messages the fake CLI read.
    fn messages(&self) -> Vec<String> {
        self.recorded()
            .iter()
            .filter_map(|v| v["stdin"].as_object().filter(|m| m.get("type").and_then(Value::as_str) == Some("user")).map(|m| m["message"]["content"].as_array().map(|a| a.iter().filter_map(|p| p["text"].as_str()).collect::<String>()).unwrap_or_default()))
            .collect()
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

fn kittens(c: &mut Client, thread: &TaskId) -> Vec<Task> {
    let mut v: Vec<Task> = tasks(c).into_iter().filter(|t| t.subagent.as_ref().is_some_and(|s| s.root == *thread)).collect();
    v.sort_by(|a, b| a.subagent.as_ref().map(|s| s.number.clone()).cmp(&b.subagent.as_ref().map(|s| s.number.clone())));
    v
}

fn find_kitten(c: &mut Client, thread: &TaskId, number: &str) -> Option<Task> {
    kittens(c, thread).into_iter().find(|t| t.subagent.as_ref().is_some_and(|s| s.number == number))
}

fn kitten(c: &mut Client, thread: &TaskId, number: &str) -> Task {
    find_kitten(c, thread, number).unwrap_or_else(|| panic!("no kitten {number}"))
}

/// Whether a kitten exists and has this status (a kitten is made while the thread works, so a wait must not assume it is there yet).
fn kitten_is(c: &mut Client, thread: &TaskId, number: &str, status: TaskStatus) -> bool {
    find_kitten(c, thread, number).is_some_and(|k| k.status == status)
}

/// A kitten's transcript.
fn kt(c: &mut Client, thread: &TaskId, number: &str) -> Transcript {
    let id = kitten(c, thread, number).id;
    transcript(c, &id)
}

fn transcript(c: &mut Client, id: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: id.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn create(c: &mut Client, words: &str, workspace: Option<&Workspace>) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: workspace.map(|w| w.id.clone()), in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

fn add(c: &mut Client, folder: &Path) -> Workspace {
    match c.request(Request::AddWorkspace { folder: folder.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, .. } => workspace,
        r => panic!("add: {r:?}"),
    }
}

fn set_settings(c: &mut Client, ask_first_subagent: bool) {
    let s = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Follow, own_mcp: false, queue_first: false, ask_first_subagent, cattains_by_hand: false };
    assert_eq!(c.request(Request::SetSettings { settings: s.clone() }).unwrap(), Reply::Settings { settings: s });
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").env("GIT_OPTIONAL_LOCKS", "0").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().map(|o| o.status.success()).unwrap_or(false)
}

/// A real repository with one commit, in a folder of its own that is removed on drop.
struct Repo(PathBuf);

impl Repo {
    fn new(name: &str) -> Repo {
        let r = temp_dir(name).canonicalize().unwrap().join("acme-api");
        std::fs::create_dir_all(&r).unwrap();
        git(&r, &["init", "-q", "-b", "main"]);
        std::fs::write(r.join("README.md"), "# Acme\nA small project.\n").unwrap();
        git(&r, &["add", "."]);
        git(&r, &["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "first"]);
        Repo(r)
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(self.0.parent().unwrap());
    }
}

/// The part of a thread's id its branches are named from: the last eight characters, lower case.
fn short(t: &Task) -> String {
    t.id.0[t.id.0.len().saturating_sub(8)..].to_lowercase()
}

const COMMIT: &str = "git add -A && git -c user.name=Kit -c user.email=kit@example.com -c commit.gpgsign=false commit -qm work";

/// A thread's words: a script that plays `steps` through the real MCP hop (one per line).
fn script(steps: &[String]) -> String {
    let mut w = String::from("Split this up [fake:delegate]");
    for s in steps {
        w.push('\n');
        w.push_str(s);
    }
    w
}

fn start(title: &str, prompt: &str, extra: Value) -> String {
    let mut args = json!({"title": title, "prompt": prompt});
    for (k, v) in extra.as_object().into_iter().flatten() {
        args[k] = v.clone();
    }
    format!("mcp neko start_subagent {args}")
}

fn tool_outputs(t: &Transcript) -> Vec<String> {
    t.parts.iter().filter_map(|p| if let PartBody::Tool { output, .. } = &p.body { output.clone() } else { None }).collect()
}

fn wakes(t: &Transcript) -> Vec<(String, String, Option<String>, String)> {
    t.parts.iter().filter_map(|p| if let PartBody::Wake { number, status, branch, summary, .. } = &p.body { Some((number.clone(), status.clone(), branch.clone(), summary.clone())) } else { None }).collect()
}

fn processes_of(d: &Daemon) -> Vec<String> {
    let out = Command::new("ps").args(["-axo", "pid=,command="]).output().unwrap();
    let me = d.child.id().to_string();
    String::from_utf8_lossy(&out.stdout).lines().filter(|l| l.contains(d.dir.to_string_lossy().as_ref()) && l.trim_start().split_whitespace().next() != Some(me.as_str())).map(str::to_owned).collect()
}

fn done(c: &mut Client, thread: &TaskId) {
    until("the thread to be done after its wakes", || {
        let t = task(c, thread);
        let tr = transcript(c, thread);
        t.status == TaskStatus::Done && tr.runs.last().is_some_and(|r| r.status == RunStatus::Done) && !tr.runs.is_empty() && kittens(c, thread).iter().all(|k| !matches!(k.status, TaskStatus::New | TaskStatus::Working | TaskStatus::Waiting))
    });
}

// ---- Own copy -------------------------------------------------------------------------------------------------------------------

#[test]
fn a_kitten_with_its_own_copy_works_on_its_own_branch_from_the_threads_folder_as_it_is() {
    let repo = Repo::new("copy");
    let r = rig("copy", &[]);
    let mut c = r.client();
    let ws = add(&mut c, &repo.0);
    let prompt = format!("[fake:steps]\nsh cat notes.txt\nwrite k1.txt one\nsh {COMMIT}");
    let t = create(&mut c, &script(&["write notes.txt written-by-the-thread".into(), start("Write k1", &prompt, json!({"placement": "own_copy"}))]), Some(&ws));
    done(&mut c, &t.id);
    let k = kitten(&mut c, &t.id, "1");
    let of = k.subagent.clone().unwrap();
    let branch = format!("neko/{}-1", short(&t));
    assert_eq!((of.placement, of.branch.as_deref(), k.status), (SubPlace::OwnCopy, Some(branch.as_str()), TaskStatus::Done));
    // The branch exists in the person's repository and holds the kitten's work and what the thread had not committed.
    assert!(git_ok(&repo.0, &["rev-parse", "--verify", &format!("refs/heads/{branch}")]));
    assert_eq!(git(&repo.0, &["show", &format!("{branch}:k1.txt")]), "one");
    assert_eq!(git(&repo.0, &["show", &format!("{branch}:notes.txt")]), "written-by-the-thread", "the copy started from the folder as it was, uncommitted files included");
    // The kitten saw that file in its copy; the thread's folder has none of the kitten's.
    let kt = transcript(&mut c, &k.id);
    assert!(tool_outputs(&kt).iter().any(|o| o.contains("written-by-the-thread")), "{:?}", tool_outputs(&kt));
    let tt = transcript(&mut c, &t.id);
    let (thread_dir, kitten_dir) = (PathBuf::from(&tt.runs[0].workdir), PathBuf::from(&kt.runs[0].workdir));
    assert_ne!(thread_dir, kitten_dir, "a folder of its own");
    assert!(thread_dir.join("notes.txt").exists() && !thread_dir.join("k1.txt").exists());
    assert!(kitten_dir.join("k1.txt").exists());
    assert_eq!(kt.runs[0].branch.as_deref(), Some(branch.as_str()));
    // Its report names the branch, and the card and the note say so too.
    let w = wakes(&tt);
    assert_eq!(w.len(), 1);
    assert_eq!((w[0].0.as_str(), w[0].1.as_str(), w[0].2.as_deref()), ("1", "done", Some(branch.as_str())));
    let calls = r.mcp_calls();
    assert!(calls[0]["text"].as_str().unwrap().contains(&format!("on the branch {branch}")), "{calls:?}");
    // Its own run has its own checkpoints: what it did, committed.
    let undo = kt.runs[0].undo.clone().expect("its own undo state");
    assert_eq!(undo.state, UndoState::Applied);
    assert!(undo.committed && undo.changes.is_some_and(|c| c.files >= 1));
    // The person's own checkout is untouched.
    assert_eq!(git(&repo.0, &["status", "--porcelain"]), "");
    assert_eq!(git(&repo.0, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
    assert!(!repo.0.join("k1.txt").exists() && !repo.0.join("notes.txt").exists());
    // The kitten's copy is kept, with its commit; the thread's folder has work in it too.
    assert!(kitten_dir.is_dir());
    // And the agent was told about all of this, once, where it reads it.
    let line = &calls[0]["text"].as_str().unwrap();
    assert!(line.contains("end your turn now"), "{line}");
}

#[test]
fn a_kitten_in_the_shared_folder_has_checkpoints_of_that_folder_and_no_branch_of_its_own() {
    let repo = Repo::new("shared");
    let r = rig("shared", &[]);
    let mut c = r.client();
    let ws = add(&mut c, &repo.0);
    let t = create(&mut c, &script(&[start("Add s", "[fake:steps]\nwrite s.txt shared", json!({}))]), Some(&ws));
    done(&mut c, &t.id);

    let k = kitten(&mut c, &t.id, "1");
    assert_eq!((k.subagent.as_ref().unwrap().placement, k.subagent.as_ref().unwrap().branch.clone()), (SubPlace::Shared, None));
    let kt = transcript(&mut c, &k.id);
    let tt = transcript(&mut c, &t.id);
    assert_eq!(kt.runs[0].workdir, tt.runs[0].workdir, "the thread's own folder");
    let undo = kt.runs[0].undo.clone().expect("undo state");
    assert_eq!(undo.state, UndoState::Applied, "its checkpoints are of the thread's folder: {undo:?}");
    assert_eq!(undo.changes.map(|c| c.files), Some(1));
    let w = wakes(&tt);
    assert_eq!(w[0].2, None, "no branch of its own to merge");
}

#[test]
fn an_own_copy_needs_a_git_project() {
    let r = rig("nogit", &[]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Write", "Write it [fake:plain]", json!({"placement": "own_copy"}))]), None);
    until("the thread to end", || task(&mut c, &t.id).status == TaskStatus::Done);
    assert!(kittens(&mut c, &t.id).is_empty(), "nothing was started");
    let calls = r.mcp_calls();
    assert_eq!(calls[0]["error"], true);
    assert!(calls[0]["text"].as_str().unwrap().contains("git repository"), "{calls:?}");
}

#[test]
fn a_placement_that_is_not_known_is_refused_in_words() {
    let r = rig("badplace", &[]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Write", "Write it [fake:plain]", json!({"placement": "elsewhere"}))]), None);
    until("the thread to end", || task(&mut c, &t.id).status == TaskStatus::Done);
    assert!(kittens(&mut c, &t.id).is_empty());
    assert!(r.mcp_calls()[0]["text"].as_str().unwrap().contains("not one of shared and own_copy"));
}

#[test]
fn removing_a_thread_removes_a_clean_copy_and_stops_for_one_with_unsaved_work() {
    let repo = Repo::new("remove");
    let r = rig("remove", &[]);
    let mut c = r.client();
    let ws = add(&mut c, &repo.0);
    let clean = create(&mut c, &script(&[start("Commit", &format!("[fake:steps]\nwrite a.txt a\nsh {COMMIT}"), json!({"placement": "own_copy"}))]), Some(&ws));
    done(&mut c, &clean.id);
    let messy = create(&mut c, &script(&[start("Leave it", "[fake:steps]\nwrite b.txt b", json!({"placement": "own_copy"}))]), Some(&ws));
    done(&mut c, &messy.id);
    // The report of a copy with work it did not commit says so, with where its copy is.
    let mt = transcript(&mut c, &messy.id);
    let w = wakes(&mt);
    assert!(w[0].3.contains("not committed") && w[0].3.contains("Commit them there"), "{w:?}");
    let messy_branch = format!("neko/{}-1", short(&messy));
    let messy_dir = PathBuf::from(&kt(&mut c, &messy.id, "1").runs[0].workdir);
    assert!(messy_dir.join("b.txt").exists());
    // Removal is refused while the copy holds work that is not saved, and nothing is deleted.
    let refused = c.request(Request::RemoveTask { task: messy.id.clone() }).unwrap();
    assert!(matches!(&refused, Reply::Error { code: ErrorCode::UnsavedWork, message } if message.contains("Kitten 1")), "{refused:?}");
    assert!(messy_dir.join("b.txt").exists() && task(&mut c, &messy.id).id == messy.id);
    // A copy whose work is committed goes: its folder, never its branch.
    let clean_branch = format!("neko/{}-1", short(&clean));
    let clean_dir = PathBuf::from(&kt(&mut c, &clean.id, "1").runs[0].workdir);
    assert!(clean_dir.is_dir());
    // The thread's own folder has no changes of its own (the kitten wrote in the copy), so only the kitten's copy remains to remove.
    assert_eq!(c.request(Request::RemoveTask { task: clean.id.clone() }).unwrap(), Reply::Ok);
    assert!(!clean_dir.exists(), "its folder is gone");
    assert!(git_ok(&repo.0, &["rev-parse", "--verify", &format!("refs/heads/{clean_branch}")]), "its commits stay in the repository");
    assert!(git_ok(&repo.0, &["rev-parse", "--verify", &format!("refs/heads/{messy_branch}")]));
    assert!(kittens(&mut c, &clean.id).is_empty());
}

#[test]
fn a_thread_that_merges_a_kittens_branch_has_the_merge_in_its_own_change_and_undo_leaves_the_branch() {
    let repo = Repo::new("merge");
    let r = rig("merge", &[("NEKO_FAKE_WAKE_SH", "git merge --squash {branch}")]);
    let mut c = r.client();
    let ws = add(&mut c, &repo.0);
    let t = create(&mut c, &script(&[start("Write m", &format!("[fake:steps]\nwrite m.txt merged\nsh {COMMIT}"), json!({"placement": "own_copy"}))]), Some(&ws));
    done(&mut c, &t.id);
    let tt = transcript(&mut c, &t.id);
    assert_eq!(tt.runs.len(), 2, "one run to start the kitten, one woken to merge");
    let woken = tt.runs.last().unwrap();
    let thread_dir = PathBuf::from(&woken.workdir);
    assert!(thread_dir.join("m.txt").exists(), "the squash merge put the kitten's file in the thread's folder");
    // The merge is the woken run's own change: its checkpoints see the file.
    let undo = woken.undo.clone().expect("the woken run has an undo state");
    assert_eq!(undo.state, UndoState::Applied, "{undo:?}");
    assert_eq!(undo.changes.map(|c| c.files), Some(1));
    // Undoing that run puts the thread's folder back and leaves the kitten's branch and copy alone.
    let branch = format!("neko/{}-1", short(&t));
    let before = git(&repo.0, &["rev-parse", &branch]);
    let kitten_dir = PathBuf::from(&kt(&mut c, &t.id, "1").runs[0].workdir);
    let undone = c.request(Request::UndoRun { task: t.id.clone(), run: woken.id.clone(), anyway: false }).unwrap();
    assert_eq!(undone, Reply::Ok, "{undone:?}");
    assert!(!thread_dir.join("m.txt").exists());
    assert_eq!(git(&repo.0, &["rev-parse", &branch]), before, "the kitten's branch did not move");
    assert!(kitten_dir.join("m.txt").exists(), "nor did its copy");
}

// ---- Stacking -------------------------------------------------------------------------------------------------------------------

#[test]
fn a_stacked_kitten_starts_after_the_one_it_follows_from_its_branch() {
    let repo = Repo::new("stack");
    let r = rig("stack", &[]);
    let mut c = r.client();
    let ws = add(&mut c, &repo.0);
    let one = format!("[fake:steps]\nwrite a.txt one\nsh {COMMIT}\nstay 2");
    let two = format!("[fake:steps]\nsh cat a.txt\nwrite b.txt two\nsh {COMMIT}");
    let t = create(&mut c, &script(&[start("First", &one, json!({"placement": "own_copy"})), start("Second", &two, json!({"placement": "own_copy", "after": "1"}))]), Some(&ws));
    until("the second to be waiting for the first", || {
        let ks = kittens(&mut c, &t.id);
        ks.len() == 2 && ks[0].status == TaskStatus::Working && ks[1].status == TaskStatus::New
    });
    let second = kitten(&mut c, &t.id, "2");
    assert_eq!(second.subagent.as_ref().unwrap().after.as_deref(), Some("1"));
    assert!(!second.subagent.as_ref().unwrap().draft, "stacked, not a draft");
    // The agent was told it waits, and for whom.
    assert!(r.mcp_calls()[1]["text"].as_str().unwrap().contains("starts when Subagent 1 is done"), "{:?}", r.mcp_calls());
    done(&mut c, &t.id);
    let (a, b) = (kt(&mut c, &t.id, "1"), kt(&mut c, &t.id, "2"));
    assert!(b.runs[0].started_ms >= a.runs[0].ended_ms.unwrap(), "the second began only when the first had ended");
    assert!(tool_outputs(&b).iter().any(|o| o.contains("one")), "it saw the first one's file: {:?}", tool_outputs(&b));
    let (b1, b2) = (format!("neko/{}-1", short(&t)), format!("neko/{}-2", short(&t)));
    assert!(git_ok(&repo.0, &["merge-base", "--is-ancestor", &b1, &b2]), "its branch holds the first one's");
    assert_eq!(git(&repo.0, &["show", &format!("{b2}:b.txt")]), "two");
    // One wake for each report, in order, and the second's branch is the one to merge.
    let w = wakes(&transcript(&mut c, &t.id));
    assert_eq!(w.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["1", "2"]);
}

#[test]
fn a_stacked_kitten_whose_predecessor_failed_does_not_start_and_says_why() {
    let repo = Repo::new("stackfail");
    let r = rig("stackfail", &[]);
    let mut c = r.client();
    let ws = add(&mut c, &repo.0);
    let t = create(&mut c, &script(&[start("First", "Break [fake:crash]", json!({"placement": "own_copy"})), start("Second", "Never runs [fake:plain]", json!({"placement": "own_copy", "after": "1"}))]), Some(&ws));
    done(&mut c, &t.id);
    let second = kitten(&mut c, &t.id, "2");
    assert_eq!(second.status, TaskStatus::Stopped);
    assert!(transcript(&mut c, &second.id).runs.is_empty(), "it never ran");
    let w = wakes(&transcript(&mut c, &t.id));
    let found = w.iter().find(|x| x.0 == "2").expect("a report for the second");
    assert_eq!(found.1, "stopped");
    assert!(found.3.contains("which it follows, failed") && found.3.contains("was not started"), "{found:?}");
    assert!(!r.messages().iter().any(|m| m == "Never runs [fake:plain]"), "the fake never read its prompt");
}

#[test]
fn a_kitten_cannot_follow_one_that_does_not_exist() {
    let r = rig("nofollow", &[]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Second", "Later [fake:plain]", json!({"after": "7"}))]), None);
    until("the thread to end", || task(&mut c, &t.id).status == TaskStatus::Done);
    assert!(kittens(&mut c, &t.id).is_empty());
    assert!(r.mcp_calls()[0]["text"].as_str().unwrap().contains("no subagent 7 to start after"));
}

// ---- Drafts ---------------------------------------------------------------------------------------------------------------------

#[test]
fn a_draft_waits_for_the_person_who_may_edit_its_prompt_and_start_it() {
    let r = rig("draft", &[]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Later", "Original words [fake:plain]", json!({"draft": true})), start("Never", "Throw away [fake:plain]", json!({"draft": true}))]), None);
    until("the thread to end", || task(&mut c, &t.id).status == TaskStatus::Done);
    // Nothing waits on a draft: the thread is done, not Waiting, and nothing runs.
    std::thread::sleep(Duration::from_millis(600));
    assert_eq!(task(&mut c, &t.id).thread_status, ThreadStatus::Idle);
    let (a, b) = (kitten(&mut c, &t.id, "1"), kitten(&mut c, &t.id, "2"));
    assert!(a.subagent.as_ref().unwrap().draft && b.subagent.as_ref().unwrap().draft);
    assert_eq!((a.status, a.words.as_str()), (TaskStatus::New, "Original words [fake:plain]"));
    assert!(transcript(&mut c, &a.id).runs.is_empty());
    assert!(r.mcp_calls()[0]["text"].as_str().unwrap().contains("saved as a draft"));
    // Wrong asks are refused in words.
    assert!(matches!(c.request(Request::StartDraft { task: t.id.clone() }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
    assert!(matches!(c.request(Request::EditDraft { task: a.id.clone(), prompt: "   ".into() }).unwrap(), Reply::Error { code: ErrorCode::BadRequest, .. }));
    // Edit, then throw one away and start the other.
    let edited = c.request(Request::EditDraft { task: a.id.clone(), prompt: "Edited words [fake:plain]".into() }).unwrap();
    assert!(matches!(&edited, Reply::Task { task } if task.words == "Edited words [fake:plain]"), "{edited:?}");
    let gone = c.request(Request::DiscardDraft { task: b.id.clone() }).unwrap();
    assert!(matches!(&gone, Reply::Task { task } if task.status == TaskStatus::Stopped && task.subagent.as_ref().is_some_and(|s| s.discarded && !s.draft)), "{gone:?}");
    let started = c.request(Request::StartDraft { task: a.id.clone() }).unwrap();
    assert!(matches!(&started, Reply::Task { task } if task.subagent.as_ref().is_some_and(|s| !s.draft)), "{started:?}");
    // The thread now waits on it, and the report wakes the thread; the discarded one woke nobody.
    until("the thread to be woken", || {
        let tr = transcript(&mut c, &t.id);
        tr.runs.len() == 2 && tr.runs[1].status == RunStatus::Done
    });
    let tt = transcript(&mut c, &t.id);
    assert_eq!(wakes(&tt).iter().map(|x| (x.0.as_str(), x.1.as_str())).collect::<Vec<_>>(), [("1", "done")]);
    let said = r.messages();
    assert!(said.iter().any(|m| m == "Edited words [fake:plain]"), "the kitten read the edited prompt: {said:?}");
    assert!(!said.iter().any(|m| m == "Original words [fake:plain]" || m == "Throw away [fake:plain]"), "no kitten read the old or the thrown-away prompt: {said:?}");
    assert!(transcript(&mut c, &b.id).runs.is_empty(), "the discarded draft never ran");
    // Edits to what is not a draft any more are refused.
    assert!(matches!(c.request(Request::EditDraft { task: a.id.clone(), prompt: "Too late".into() }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
}

#[test]
fn the_agent_starts_its_drafts_with_a_tool_and_the_reports_wake_it() {
    let r = rig("startdrafts", &[]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("One", "One [fake:long:1]", json!({"draft": true})), start("Two", "Two [fake:long:1]", json!({"draft": true})), "mcp neko start_drafts {}".into()]), None);
    done(&mut c, &t.id);
    let calls = r.mcp_calls();
    assert!(calls[2]["text"].as_str().unwrap().contains("Started Subagent 1, Subagent 2"), "{calls:?}");
    let tt = transcript(&mut c, &t.id);
    assert_eq!(wakes(&tt).len(), 2);
    assert!(kittens(&mut c, &t.id).iter().all(|k| k.status == TaskStatus::Done));
}

#[test]
fn asking_before_a_threads_first_kitten_makes_it_a_draft_the_person_starts() {
    let r = rig("askfirst", &[]);
    let mut c = r.client();
    set_settings(&mut c, true);
    let t = create(&mut c, &script(&[start("First", "First [fake:plain]", json!({})), start("Second", "Second [fake:plain]", json!({}))]), None);
    // Only the first was held back; the second went ahead (and its report wakes the thread).
    until("the second to finish", || kitten_is(&mut c, &t.id, "2", TaskStatus::Done));
    let first = kitten(&mut c, &t.id, "1");
    assert!(first.subagent.as_ref().unwrap().draft && first.status == TaskStatus::New);
    assert!(r.mcp_calls()[0]["text"].as_str().unwrap().contains("press Start"), "{:?}", r.mcp_calls());
    assert!(!r.mcp_calls()[1]["text"].as_str().unwrap().contains("draft"));
    let started = c.request(Request::StartDraft { task: first.id.clone() }).unwrap();
    assert!(matches!(started, Reply::Task { .. }));
    until("the first to finish", || kitten_is(&mut c, &t.id, "1", TaskStatus::Done));
    done(&mut c, &t.id);
    // Off by default: no draft is made.
    set_settings(&mut c, false);
    let u = create(&mut c, &script(&[start("Only", "Only [fake:plain]", json!({}))]), None);
    done(&mut c, &u.id);
    assert!(!kitten(&mut c, &u.id, "1").subagent.as_ref().unwrap().draft);
}

#[test]
fn stopping_a_thread_throws_its_drafts_away_without_a_report() {
    let r = rig("stopdraft", &[]);
    let mut c = r.client();
    let t = create(&mut c, &script(&[start("Draft", "Later [fake:plain]", json!({"draft": true})), start("Runs", "Slow [fake:slow]", json!({}))]), None);
    until("the slow one to work", || kitten_is(&mut c, &t.id, "2", TaskStatus::Working));
    assert_eq!(c.request(Request::StopTask { task: t.id.clone() }).unwrap(), Reply::Ok);
    until("everything to stop", || processes_of(&r.d).is_empty() && task(&mut c, &t.id).status == TaskStatus::Stopped);
    let d = kitten(&mut c, &t.id, "1");
    assert!(d.subagent.as_ref().unwrap().discarded && d.status == TaskStatus::Stopped, "{d:?}");
    assert!(wakes(&transcript(&mut c, &t.id)).is_empty(), "a stopped thread is woken by nothing");
}

// ---- Depth ----------------------------------------------------------------------------------------------------------------------

#[test]
fn kittens_nest_three_deep_and_the_last_level_is_not_offered_the_tool_to_start_more() {
    let r = rig("depth", &[]);
    let mut c = r.client();
    let p3 = format!("[fake:steps]\ntools d3\n{}", start("Too deep", "Never", json!({})));
    let p2 = format!("[fake:steps]\ntools d2\n{}", start("Level three", &p3, json!({})));
    let p1 = format!("[fake:steps]\ntools d1\n{}", start("Level two", &p2, json!({})));
    let t = create(&mut c, &script(&["tools d0".into(), start("Level one", &p1, json!({}))]), None);
    done(&mut c, &t.id);
    let numbers: Vec<String> = kittens(&mut c, &t.id).iter().map(|k| k.subagent.as_ref().unwrap().number.clone()).collect();
    assert_eq!(numbers, ["1", "1.1", "1.1.1"], "no fourth level");
    let offered: Vec<(String, String)> = r.recorded().iter().filter_map(|v| v["neko_tools"].as_str().map(|n| (v["label"].as_str().unwrap_or("").to_owned(), n.to_owned()))).collect();
    assert_eq!(offered.len(), 4, "{offered:?}");
    for (label, names) in &offered {
        let has = names.split(',').any(|n| n == "start_subagent") && names.split(',').any(|n| n == "start_drafts");
        assert_eq!(has, label != "d3", "{label}: {names}");
        assert!(names.contains("subagents") && names.contains("stop_subagent"), "{label}: {names}");
    }
    // The last level that asked anyway was told there was no such tool.
    let failed: Vec<Value> = r.mcp_calls().into_iter().filter(|m| m["tool"] == "start_subagent" && m["error"] == true).collect();
    assert_eq!(failed.len(), 1, "{failed:?}");
    assert!(failed[0]["text"].as_str().unwrap().contains("No such tool available"));
    // The reports went up the chain: each level woken by the one below, the thread last.
    let w = wakes(&transcript(&mut c, &t.id));
    assert_eq!(w.iter().map(|x| x.0.as_str()).collect::<Vec<_>>(), ["1"]);
}

// ---- Limits and heat ------------------------------------------------------------------------------------------------------------

#[test]
fn a_warm_mac_queues_new_kittens_and_they_start_when_it_cools_but_the_persons_own_run_is_never_held() {
    let heat = temp_dir("heatfile").join("heat");
    std::fs::write(&heat, "warm").unwrap();
    let h = heat.to_string_lossy().into_owned();
    let r = rig("heat", &[("NEKO_HEAT_FILE", &h), ("NEKO_HEAT_RECHECK_MS", "150")]);
    let mut c = r.client();
    // The person's own thread runs at once, warm or not.
    let plain = create(&mut c, "Say hello [fake:plain]", None);
    until("the person's own run to end", || task(&mut c, &plain.id).status == TaskStatus::Done);
    let t = create(&mut c, &script(&[start("Waits", "Count [fake:long:1]", json!({}))]), None);
    until("the thread to end its turn", || kittens(&mut c, &t.id).len() == 1 && transcript(&mut c, &t.id).runs.first().is_some_and(|r| r.status == RunStatus::Done));
    std::thread::sleep(Duration::from_millis(900));
    let k = kitten(&mut c, &t.id, "1");
    assert_eq!(k.status, TaskStatus::New, "held back while the Mac is warm");
    assert!(r.mcp_calls()[0]["text"].as_str().unwrap().contains("queued"), "{:?}", r.mcp_calls());
    std::fs::write(&heat, "ok").unwrap();
    until("the kitten to start once the Mac is cool", || find_kitten(&mut c, &t.id, "1").is_some_and(|k| k.status != TaskStatus::New));
    done(&mut c, &t.id);
    let _ = std::fs::remove_dir_all(heat.parent().unwrap());
}

#[test]
fn five_kittens_asked_three_work_and_two_start_as_the_others_finish() {
    let r = rig("limit", &[]);
    let mut c = r.client();
    let steps: Vec<String> = (1..=5).map(|i| start(&format!("K{i}"), &format!("Count {i} [fake:long:1]"), json!({}))).collect();
    let t = create(&mut c, &script(&steps), None);
    until("three working and two queued", || {
        let ks = kittens(&mut c, &t.id);
        ks.iter().filter(|k| k.status == TaskStatus::Working).count() == 3 && ks.iter().filter(|k| k.status == TaskStatus::New).count() == 2
    });
    // The agent was told which were queued.
    let calls = r.mcp_calls();
    assert!(calls[3]["text"].as_str().unwrap().contains("queued behind other work"), "{calls:?}");
    done(&mut c, &t.id);
    assert_eq!(wakes(&transcript(&mut c, &t.id)).len(), 5);
}
