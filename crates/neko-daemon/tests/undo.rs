//! Undo through the real daemon, with the fake CLI as `claude` editing real files (docs/knowledge/undo.md): run, review, undo,
//! redo, the follow-up that tells the agent; files changed since and "Undo anyway"; a run that committed; the size guard; a plain
//! folder through the shadow repository; housekeeping and the storage figure. Every wait has a deadline.

mod support;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::undo::LINE_COMMITTED;

use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(60);

fn watcher(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

fn wait_status(c: &mut Client, task: &TaskId, want: &[TaskStatus]) {
    let end = Instant::now() + T;
    loop {
        assert!(Instant::now() < end, "timed out waiting for {task:?} to be {want:?}");
        if let Some(ServerMessage::Event { change: Change::TaskUpdated { task: t }, .. }) = c.next_event(Duration::from_millis(200)).unwrap()
            && &t.id == task
            && want.contains(&t.status)
        {
            return;
        }
    }
}

fn add(c: &mut Client, folder: &Path) -> Workspace {
    match c.request(Request::AddWorkspace { folder: folder.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, .. } => workspace,
        r => panic!("add: {r:?}"),
    }
}

/// Makes a task and waits for its first run to end.
fn run_task(c: &mut Client, words: &str, w: Option<&Workspace>) -> Task {
    let task = match c.request(Request::CreateTask { words: words.into(), workspace: w.map(|w| w.id.clone()), cat: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    };
    wait_status(c, &task.id, &[TaskStatus::Done, TaskStatus::Failed]);
    task
}

fn transcript(c: &mut Client, task: &TaskId) -> Transcript {
    match c.request(Request::GetTranscript { task: task.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript,
        r => panic!("transcript: {r:?}"),
    }
}

fn last_run(c: &mut Client, task: &TaskId) -> Run {
    transcript(c, task).runs.pop().unwrap()
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").env("GIT_OPTIONAL_LOCKS", "0").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn write(dir: &Path, rel: &str, bytes: &[u8]) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// The files the fake's `edit` script changes, in a fresh folder.
fn seed(dir: &Path) {
    write(dir, "README.md", b"# Acme\nA small project.\n");
    write(dir, "OLD.md", b"# Old notes\n");
    write(dir, "docs/guide.md", b"# Guide\nStart here.\n");
    write(dir, "logo.png", &[0x89, b'P', b'N', b'G', 0, 1]);
}

fn repo(dir: &Path) -> PathBuf {
    let r = dir.join("acme-api");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    seed(&r);
    git(&r, &["add", "."]);
    git(&r, &["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "first"]);
    r.canonicalize().unwrap()
}

/// Every file under `dir` but `.git`: its bytes.
fn files(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    let mut m = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, m: &mut BTreeMap<String, Vec<u8>>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.file_name() == ".git" {
                continue;
            }
            if e.path().is_dir() {
                walk(base, &e.path(), m);
            } else {
                m.insert(e.path().strip_prefix(base).unwrap().to_string_lossy().into_owned(), std::fs::read(e.path()).unwrap());
            }
        }
    }
    walk(dir, dir, &mut m);
    m
}

/// What of the person's repository a run, a checkpoint, an undo and a redo must not change (the task's own `neko/` branch aside).
fn person(r: &Path) -> (Vec<u8>, String, String, String) {
    let refs = git(r, &["for-each-ref", "refs/heads", "refs/stash", "refs/tags", "refs/remotes"]).lines().filter(|l| !l.contains("refs/heads/neko/")).collect::<Vec<_>>().join("\n");
    (std::fs::read(r.join(".git/index")).unwrap(), git(r, &["status", "--porcelain=v2", "--branch", "--untracked-files=all"]), refs, git(r, &["stash", "list"]))
}

struct Outside(PathBuf);

impl Drop for Outside {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn undo(c: &mut Client, task: &TaskId, run: &RunId, anyway: bool) -> Reply {
    c.request(Request::UndoRun { task: task.clone(), run: run.clone(), anyway }).unwrap()
}

fn redo(c: &mut Client, task: &TaskId, run: &RunId, anyway: bool) -> Reply {
    c.request(Request::RedoRun { task: task.clone(), run: run.clone(), anyway }).unwrap()
}

#[test]
fn run_review_undo_redo_and_the_agent_is_told() {
    let outside = Outside(temp_dir("undo-e2e-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    let d = Daemon::start("undo-e2e");
    let mut c = watcher(&d);
    let w = add(&mut c, &r);
    let theirs = person(&r);
    let task = run_task(&mut c, "Tidy the project [fake:edit]", Some(&w));
    let run = last_run(&mut c, &task.id);
    let tree = PathBuf::from(&run.workdir);
    let u = run.undo.clone().expect("the run has its undo");
    assert_eq!(u.state, UndoState::Applied);
    assert_eq!(u.changes.map(|c| c.files), Some(5), "{u:?}");
    assert!(!u.committed);
    let refs = git(&r, &["for-each-ref", "--format=%(refname)", "refs/neko/"]);
    assert_eq!(refs.lines().count(), 2, "a start and an end checkpoint: {refs}");
    assert!(refs.contains(&format!("refs/neko/checkpoints/{}/{}/start", task.id.0, run.id.0)));

    // Review: exactly the run's change.
    let Reply::Diff { diff } = c.request(Request::GetDiff { task: task.id.clone(), run: Some(run.id.clone()) }).unwrap() else { panic!("no diff") };
    let by = |p: &str| diff.files.iter().find(|f| f.path == p).unwrap_or_else(|| panic!("{p} in {:?}", diff.files)).clone();
    assert_eq!(by("README.md").status, FileStatus::Modified);
    assert_eq!((by("README.md").added, by("README.md").removed), (1, 1));
    assert_eq!(by("src/retry/mod.rs").status, FileStatus::Added);
    assert_eq!(by("OLD.md").status, FileStatus::Deleted);
    assert_eq!(by("docs/start.md").old_path.as_deref(), Some("docs/guide.md"));
    assert!(by("logo.png").binary);
    assert_eq!((by("logo.png").old_size, by("logo.png").new_size), (Some(6), Some(8)));
    let Reply::DiffFile { file } = c.request(Request::GetDiffFile { task: task.id.clone(), run: Some(run.id.clone()), path: "README.md".into(), full: false }).unwrap() else { panic!("no file") };
    assert_eq!(file.lines, vec!["@@ -1,2 +1,2 @@", " # Acme", "-A small project.", "+A tidy project."]);
    let Reply::Diff { diff: all } = c.request(Request::GetDiff { task: task.id.clone(), run: None }).unwrap() else { panic!("no diff") };
    assert_eq!(all.files, diff.files, "one run: all changes are its changes");

    // Undo: the files as they were before the run; nothing of the person's moved.
    let changed = files(&tree);
    assert_eq!(undo(&mut c, &task.id, &run.id, false), Reply::Ok);
    assert_eq!(files(&tree), files(&r), "the worktree is as the run found it");
    assert_eq!(last_run(&mut c, &task.id).undo.unwrap().state, UndoState::Undone);
    assert_eq!(person(&r), theirs, "the person's checkout, index, branches and stash are untouched");
    assert_eq!(git(&tree, &["rev-parse", "--abbrev-ref", "HEAD"]), format!("neko/{}", &task.id.0[task.id.0.len() - 8..].to_lowercase()));

    // Redo, and undo again.
    assert_eq!(redo(&mut c, &task.id, &run.id, false), Reply::Ok);
    assert_eq!(files(&tree), changed, "redo puts the run's changes back exactly");
    assert_eq!(undo(&mut c, &task.id, &run.id, false), Reply::Ok);

    // The follow-up tells the agent, and the undone run can no longer be redone.
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "Carry on".into(), answering: None }).unwrap(), Reply::Ok);
    wait_status(&mut c, &task.id, &[TaskStatus::Done, TaskStatus::Failed]);
    let t = transcript(&mut c, &task.id);
    let said: Vec<&str> = t.parts.iter().filter_map(|p| if let PartBody::Agent { text, .. } = &p.body { Some(text.as_str()) } else { None }).collect();
    let heard = said.last().unwrap();
    assert!(heard.contains("the person undid the changes of your last run") && heard.ends_with("Carry on"), "the agent heard: {heard}");
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Person { text } if text == "Carry on")), "the person's words are stored as they typed them");
    assert!(t.parts.iter().any(|p| matches!(&p.body, PartBody::Notice { text, .. } if text.contains("can no longer be redone"))));
    // The follow-up changed nothing, so the worktree was as it began and was tidied away, its checkpoints with it.
    assert_eq!(t.runs[0].undo.as_ref().unwrap().state, UndoState::Removed);
    assert_eq!(t.runs[1].undo.as_ref().unwrap().state, UndoState::Removed);
    assert!(matches!(redo(&mut c, &task.id, &run.id, false), Reply::Error { code: ErrorCode::Refused, .. }));
    assert_eq!(person(&r), theirs);
}

#[test]
fn files_changed_since_are_named_and_undo_anyway_keeps_them_first() {
    let outside = Outside(temp_dir("undo-since-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    let d = Daemon::start("undo-since");
    let mut c = watcher(&d);
    let w = add(&mut c, &r);
    let task = run_task(&mut c, "Tidy the project [fake:edit]", Some(&w));
    let run = last_run(&mut c, &task.id);
    let tree = PathBuf::from(&run.workdir);
    // The person edits after the run.
    write(&tree, "README.md", b"# Acme\nMy own words.\n");
    write(&tree, "mine.txt", b"mine\n");
    let held = files(&tree);
    let Reply::ChangedSince { files: mut differ } = undo(&mut c, &task.id, &run.id, false) else { panic!("expected changed_since") };
    differ.sort();
    assert_eq!(differ, vec!["README.md".to_owned(), "mine.txt".to_owned()]);
    assert_eq!(files(&tree), held, "nothing was overwritten");
    assert_eq!(last_run(&mut c, &task.id).undo.unwrap().state, UndoState::Applied);
    // Undo anyway: the folder as it was is kept first, then the run is undone.
    assert_eq!(undo(&mut c, &task.id, &run.id, true), Reply::Ok);
    assert_eq!(files(&tree), files(&r), "undone over the person's edits");
    assert!(git(&r, &["for-each-ref", "--format=%(refname)", "refs/neko/"]).contains("before_undo-1"), "the state before was kept");
    // Redo brings back exactly what was there before the undo, the person's edits too.
    assert_eq!(redo(&mut c, &task.id, &run.id, false), Reply::Ok);
    assert_eq!(files(&tree), held);
}

#[test]
fn a_run_that_committed_is_not_undone_and_its_diff_stays_readable() {
    let outside = Outside(temp_dir("undo-commit-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    let d = Daemon::start("undo-commit");
    let mut c = watcher(&d);
    let w = add(&mut c, &r);
    let task = run_task(&mut c, "Write a changelog [fake:commit]", Some(&w));
    let run = last_run(&mut c, &task.id);
    let u = run.undo.clone().unwrap();
    assert!(u.committed, "{u:?}");
    let head = git(Path::new(&run.workdir), &["rev-parse", "HEAD"]);
    assert_eq!(undo(&mut c, &task.id, &run.id, false), Reply::error(ErrorCode::Refused, LINE_COMMITTED));
    assert_eq!(git(Path::new(&run.workdir), &["rev-parse", "HEAD"]), head, "no commit moved");
    let Reply::Diff { diff } = c.request(Request::GetDiff { task: task.id.clone(), run: Some(run.id.clone()) }).unwrap() else { panic!("no diff") };
    assert_eq!(diff.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), vec!["CHANGELOG.md"]);
}

#[test]
fn past_the_size_guard_there_is_no_checkpoint_and_the_run_says_why() {
    let d = Daemon::start_with(temp_dir("undo-size"), &[("NEKO_UNDO_MAX_FILES", "10")]);
    let mut c = watcher(&d);
    let task = run_task(&mut c, "Write many files [fake:many:30]", None);
    assert_eq!(transcript(&mut c, &task.id).task.status, TaskStatus::Done, "the run itself still works");
    let run = last_run(&mut c, &task.id);
    let u = run.undo.unwrap();
    assert_eq!(u.state, UndoState::Unavailable);
    let why = u.why.unwrap();
    assert!(why.starts_with("the folder has 30 new or changed files (") && why.ends_with("more than undo keeps (10 files or 100.0 MB)"), "{why}");
    let o = neko_protocol::undo::offers(&transcript(&mut c, &task.id).runs);
    assert_eq!(o[0].line, Some(format!("Undo is not available for this run: {why}.")));
    assert!(matches!(undo(&mut c, &task.id, &run.id, false), Reply::Error { code: ErrorCode::Refused, .. }));
}

#[test]
fn a_plain_folder_is_undone_through_the_shadow_repository_and_nothing_is_written_into_it() {
    let outside = Outside(temp_dir("undo-plain-outside").canonicalize().unwrap());
    let notes = outside.0.join("notes");
    seed(&notes);
    let before = files(&notes);
    let d = Daemon::start("undo-plain");
    let mut c = watcher(&d);
    let w = add(&mut c, &notes);
    let task = run_task(&mut c, "Tidy the notes [fake:edit]", Some(&w));
    let run = last_run(&mut c, &task.id);
    assert_eq!(run.undo.as_ref().unwrap().changes.map(|c| c.files), Some(5));
    assert!(!notes.join(".git").exists(), "no repository in the person's folder");
    let shadow = d.dir.join("undo").join(format!("{}.git", &task.id.0[task.id.0.len() - 8..].to_lowercase()));
    assert!(shadow.join("HEAD").is_file(), "the shadow repository is in Neko's data folder");
    assert_eq!(undo(&mut c, &task.id, &run.id, false), Reply::Ok);
    assert_eq!(files(&notes), before, "undone exactly, and nothing of Neko's is in the folder");
    let Reply::UndoStorage { storage } = c.request(Request::UndoStorage).unwrap() else { panic!("no storage") };
    assert!(storage.bytes > 0 && storage.checkpoints == 2 && storage.tasks == 1, "{storage:?}");
    // Removing the task removes its shadow repository; the person's folder stays.
    assert_eq!(c.request(Request::RemoveTask { task: task.id.clone() }).unwrap(), Reply::Ok);
    assert!(!shadow.exists());
    assert_eq!(files(&notes), before);
}

#[test]
fn housekeeping_prunes_old_checkpoints_removes_tidied_ones_and_clears_on_request() {
    let outside = Outside(temp_dir("undo-house-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    let d = Daemon::start_with(temp_dir("undo-house"), &[("NEKO_UNDO_KEEP_MS", "0")]);
    let mut c = watcher(&d);
    let w = add(&mut c, &r);
    let neko_refs = |task: &TaskId| git(&r, &["for-each-ref", "--format=%(refname)", &format!("refs/neko/checkpoints/{}/", task.0)]).lines().count();

    // A run that changed nothing: its worktree is tidied away, and its checkpoints with it.
    let quiet = run_task(&mut c, "Just look [fake:plain]", Some(&w));
    assert_eq!(neko_refs(&quiet.id), 0, "refs for a tidied worktree go with it");
    assert_eq!(last_run(&mut c, &quiet.id).undo.unwrap().state, UndoState::Removed);

    // Two runs: everything older than the newest run's pair goes when the task finishes (here at once).
    let task = run_task(&mut c, "Tidy the project [fake:edit]", Some(&w));
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "[fake:many:3]".into(), answering: None }).unwrap(), Reply::Ok);
    wait_status(&mut c, &task.id, &[TaskStatus::Done]);
    let t = transcript(&mut c, &task.id);
    assert_eq!(t.runs[0].undo.as_ref().unwrap().state, UndoState::Removed, "the older run's checkpoints were pruned");
    assert_eq!(t.runs[1].undo.as_ref().unwrap().state, UndoState::Applied, "never the newest pair");
    assert_eq!(neko_refs(&task.id), 2);

    // Clear undo history: every ref and row goes.
    assert_eq!(c.request(Request::ClearUndo).unwrap(), Reply::Ok);
    assert_eq!(git(&r, &["for-each-ref", "refs/neko/"]), "");
    assert_eq!(last_run(&mut c, &task.id).undo.unwrap().state, UndoState::Removed);
    let Reply::UndoStorage { storage } = c.request(Request::UndoStorage).unwrap() else { panic!("no storage") };
    assert_eq!((storage.checkpoints, storage.tasks), (0, 0));

    // Removing a task removes its refs.
    assert_eq!(c.request(Request::FollowUp { task: task.id.clone(), words: "[fake:many:2]".into(), answering: None }).unwrap(), Reply::Ok);
    wait_status(&mut c, &task.id, &[TaskStatus::Done]);
    assert_eq!(neko_refs(&task.id), 2);
    // Its worktree holds uncommitted files: the person commits them, so the task may go.
    let tree = PathBuf::from(&last_run(&mut c, &task.id).workdir);
    git(&tree, &["add", "-A"]);
    git(&tree, &["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "-c", "commit.gpgsign=false", "commit", "-q", "-m", "keep"]);
    assert_eq!(c.request(Request::RemoveTask { task: task.id.clone() }).unwrap(), Reply::Ok);
    assert_eq!(neko_refs(&task.id), 0, "a removed task's checkpoints go");
}
