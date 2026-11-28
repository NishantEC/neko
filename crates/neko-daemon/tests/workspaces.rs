//! Workspaces through the real daemon, with the fake CLI as `claude`: a task in each of two workspaces and one in General, all
//! still where they were after a restart; removing a workspace and a missing folder; the tidy rule measured over 20 finished
//! tasks; removing a task, refused while its worktree holds unsaved work. Every wait has a deadline.

mod support;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use neko_protocol::client::Client;
use neko_protocol::*;
use support::*;

const T: Duration = Duration::from_secs(60);

fn watcher(d: &Daemon) -> Client {
    let mut c = d.client();
    assert!(matches!(c.request(Request::Subscribe).unwrap(), Reply::Subscribed { .. }));
    c
}

fn events_until(c: &mut Client, what: &str, mut done: impl FnMut(&Change) -> bool) -> Vec<Change> {
    let end = Instant::now() + T;
    let mut got = Vec::new();
    loop {
        assert!(Instant::now() < end, "timed out waiting for {what}; got {} events", got.len());
        if let Some(ServerMessage::Event { change, .. }) = c.next_event(Duration::from_millis(200)).unwrap() {
            let stop = done(&change);
            got.push(change);
            if stop {
                return got;
            }
        }
    }
}

fn add(c: &mut Client, folder: &Path) -> Workspace {
    match c.request(Request::AddWorkspace { folder: folder.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, .. } => workspace,
        r => panic!("add: {r:?}"),
    }
}

fn create(c: &mut Client, words: &str, workspace: Option<&Workspace>) -> Task {
    match c.request(Request::CreateTask { crew: None, words: words.into(), workspace: workspace.map(|w| w.id.clone()), in_folder: false, from_branch: None }).unwrap() {
        Reply::Task { task } => task,
        r => panic!("create: {r:?}"),
    }
}

/// Waits until every task in `ids` has finished its run (done), whatever order the events come in.
fn all_done(c: &mut Client, ids: &[TaskId]) {
    let mut left: std::collections::HashSet<TaskId> = ids.iter().cloned().collect();
    events_until(c, "every task done", |ch| {
        if let Change::TaskUpdated { task } = ch
            && task.status == TaskStatus::Done
        {
            left.remove(&task.id);
        }
        left.is_empty()
    });
}

fn workspaces(c: &mut Client) -> Vec<Workspace> {
    match c.request(Request::ListWorkspaces).unwrap() {
        Reply::Workspaces { workspaces } => workspaces,
        r => panic!("list: {r:?}"),
    }
}

fn tasks(c: &mut Client) -> Vec<Task> {
    match c.request(Request::ListTasks).unwrap() {
        Reply::Tasks { tasks } => tasks,
        r => panic!("tasks: {r:?}"),
    }
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn repo(dir: &Path) -> PathBuf {
    let r = dir.join("acme-api");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "-q", "-b", "main"]);
    std::fs::write(r.join("README.md"), "# Acme\nA small project.\n").unwrap();
    git(&r, &["add", "."]);
    git(&r, &["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "commit", "-q", "-m", "first"]);
    r.canonicalize().unwrap()
}

/// The folders under `dir` (none when it is not there).
fn folders_in(dir: &Path) -> usize {
    std::fs::read_dir(dir).map(|d| d.filter_map(Result::ok).filter(|e| e.path().is_dir()).count()).unwrap_or(0)
}

/// A temp folder of the test's own, removed at the end (pass or fail), with its repository's worktrees pruned.
struct Outside(PathBuf);

impl Drop for Outside {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn two_workspaces_and_general_are_all_where_they_were_after_a_restart() {
    let outside = Outside(temp_dir("ws-e2e-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    let notes = outside.0.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let dir = temp_dir("ws-e2e");
    let mut d = Daemon::start_in(dir.clone());
    d.keep_dir = true;
    let mut c = watcher(&d);
    let acme = add(&mut c, &r);
    let plain = add(&mut c, &notes);
    assert_eq!((acme.name.as_str(), plain.name.as_str()), ("acme-api", "notes"));
    let a = create(&mut c, "Look at acme [fake:plain]", Some(&acme));
    let n = create(&mut c, "Look at the notes [fake:plain]", Some(&plain));
    let g = create(&mut c, "Anything at all [fake:plain]", None);
    assert_eq!((a.workspace.as_str(), n.workspace.as_str(), g.workspace.as_str()), (acme.id.as_str(), plain.id.as_str(), GENERAL));
    all_done(&mut c, &[a.id.clone(), n.id.clone(), g.id.clone()]);
    let before_ws = workspaces(&mut c);
    let before_tasks = tasks(&mut c);
    drop(c);
    assert!(d.stop().success());

    let mut d = Daemon::start_in(dir.clone());
    let mut c = watcher(&d);
    assert_eq!(workspaces(&mut c), before_ws, "the workspaces are as they were");
    assert_eq!(workspaces(&mut c).iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["General", "acme-api", "notes"]);
    assert_eq!(tasks(&mut c), before_tasks, "every task is in its workspace, as it was");
    // A follow-up in the repository's task works again, though its empty worktree was tidied away after the first run.
    assert_eq!(c.request(Request::FollowUp { task: a.id.clone(), words: "Once more".into(), answering: None, mode: None }).unwrap(), Reply::Ok);
    let ok = events_until(&mut c, "the follow-up runs", |ch| matches!(ch, Change::TaskUpdated { task } if task.id == a.id && matches!(task.status, TaskStatus::Done | TaskStatus::Failed)));
    assert!(matches!(ok.last(), Some(Change::TaskUpdated { task }) if task.status == TaskStatus::Done), "{ok:?}");
    drop(c);
    assert!(d.stop().success());
    d.keep_dir = false;
    let _ = git(&r, &["worktree", "prune"]);
}

#[test]
fn removing_a_workspace_and_a_missing_folder() {
    let outside = Outside(temp_dir("ws-remove-outside").canonicalize().unwrap());
    let acme_dir = outside.0.join("acme-api");
    let other_dir = outside.0.join("other");
    std::fs::create_dir_all(&acme_dir).unwrap();
    std::fs::create_dir_all(&other_dir).unwrap();
    std::fs::write(acme_dir.join("README.md"), "kept\n").unwrap();
    let d = Daemon::start("ws-remove");
    let mut c = watcher(&d);
    let acme = add(&mut c, &acme_dir);
    events_until(&mut c, "added", |ch| matches!(ch, Change::WorkspaceAdded { workspace } if workspace.id == acme.id));
    let t = create(&mut c, "Before [fake:plain]", Some(&acme));
    all_done(&mut c, std::slice::from_ref(&t.id));
    // The same folder again: the same workspace, no new one.
    match c.request(Request::AddWorkspace { folder: acme_dir.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, existed } => assert!(existed && workspace.id == acme.id),
        r => panic!("{r:?}"),
    }
    match c.request(Request::RemoveWorkspace { workspace: acme.id.clone() }).unwrap() {
        Reply::Workspace { workspace, .. } => assert!(workspace.id == acme.id && workspace.removed_ms.is_some()),
        r => panic!("{r:?}"),
    }
    events_until(&mut c, "removed", |ch| matches!(ch, Change::WorkspaceUpdated { workspace } if workspace.id == acme.id && workspace.removed_ms.is_some()));
    assert_eq!(std::fs::read_to_string(acme_dir.join("README.md")).unwrap(), "kept\n", "nothing on disk is touched");
    assert!(tasks(&mut c).iter().any(|x| x.id == t.id), "its task stays readable");
    assert!(matches!(c.request(Request::CreateTask { crew: None, words: "After".into(), workspace: Some(acme.id.clone()), in_folder: false, from_branch: None }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
    assert!(matches!(c.request(Request::RemoveWorkspace { workspace: GENERAL.into() }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
    assert!(matches!(c.request(Request::RenameWorkspace { workspace: GENERAL.into(), name: "Mine".into() }).unwrap(), Reply::Error { code: ErrorCode::Refused, .. }));
    assert!(matches!(c.request(Request::AddWorkspace { folder: "/nonexistent/acme".into() }).unwrap(), Reply::Error { code: ErrorCode::InvalidFolder, .. }));

    // A folder that goes missing: said, a new task refused in plain words, Locate brings it back.
    let other = add(&mut c, &other_dir);
    let moved = outside.0.join("moved");
    std::fs::rename(&other_dir, &moved).unwrap();
    assert!(workspaces(&mut c).iter().any(|w| w.id == other.id && w.missing), "folder not found");
    match c.request(Request::CreateTask { crew: None, words: "Hi".into(), workspace: Some(other.id.clone()), in_folder: false, from_branch: None }).unwrap() {
        Reply::Error { code: ErrorCode::Refused, message } => assert!(message.contains("was not found"), "{message}"),
        r => panic!("{r:?}"),
    }
    match c.request(Request::LocateWorkspace { workspace: other.id.clone(), folder: moved.to_string_lossy().into_owned() }).unwrap() {
        Reply::Workspace { workspace, .. } => assert!(!workspace.missing && workspace.folder.as_deref() == Some(moved.to_string_lossy().as_ref())),
        r => panic!("{r:?}"),
    }
    let renamed = match c.request(Request::RenameWorkspace { workspace: other.id.clone(), name: "Acme notes".into() }).unwrap() {
        Reply::Workspace { workspace, .. } => workspace,
        r => panic!("{r:?}"),
    };
    assert_eq!(renamed.name, "Acme notes");
    events_until(&mut c, "renamed", |ch| matches!(ch, Change::WorkspaceUpdated { workspace } if workspace.name == "Acme notes"));
}

#[test]
fn twenty_finished_tasks_leave_no_folders_behind_and_work_is_kept() {
    let outside = Outside(temp_dir("ws-tidy-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    let d = Daemon::start("ws-tidy");
    let mut c = watcher(&d);
    let acme = add(&mut c, &r);
    let mut ids = Vec::new();
    for i in 0..10 {
        ids.push(create(&mut c, &format!("Look {i} [fake:plain]"), Some(&acme)).id);
        ids.push(create(&mut c, &format!("Think {i} [fake:plain]"), None).id);
    }
    all_done(&mut c, &ids);
    let worktrees = folders_in(&d.dir.join(files::WORKTREES));
    let scratch = folders_in(&d.dir.join(files::SCRATCH));
    let branches = git(&r, &["branch", "--list", "neko/*"]);
    let listed = git(&r, &["worktree", "list"]).lines().count();
    eprintln!("after 20 finished tasks (10 in a repository, 10 in General): {worktrees} worktree folders, {scratch} scratch folders, {} neko branches, {listed} worktrees git lists", branches.lines().count());
    assert_eq!((worktrees, scratch), (0, 0), "nothing with no work in it is left behind");
    assert_eq!(branches, "", "no empty branches are left");
    assert_eq!(listed, 1, "git lists only the person's own checkout");
    assert_eq!(tasks(&mut c).len(), 20, "the tasks themselves stay");

    // A task that edits files keeps its worktree.
    let s = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Ask, own_mcp: false, queue_first: false, ask_first_subagent: false };
    c.request(Request::SetSettings { settings: s }).unwrap();
    let work = create(&mut c, "Tidy the README [fake:tools]", Some(&acme));
    all_done(&mut c, std::slice::from_ref(&work.id));
    assert_eq!(folders_in(&d.dir.join(files::WORKTREES)), 1, "the worktree with work in it is kept");
    let _ = git(&r, &["worktree", "prune"]);
}

#[test]
fn removing_a_task_refuses_unsaved_work_and_keeps_commits_on_the_branch() {
    let outside = Outside(temp_dir("ws-rm-task-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    let d = Daemon::start("ws-rm-task");
    let mut c = watcher(&d);
    let s = Settings { runtime: "claude".into(), permission_mode: PermissionMode::Ask, own_mcp: false, queue_first: false, ask_first_subagent: false };
    c.request(Request::SetSettings { settings: s }).unwrap();
    let acme = add(&mut c, &r);
    let work = create(&mut c, "Tidy the README [fake:tools]", Some(&acme));
    let plain = create(&mut c, "Just words [fake:plain]", None);
    all_done(&mut c, &[work.id.clone(), plain.id.clone()]);
    let tree = d.dir.join(files::WORKTREES).join(work.id.0[work.id.0.len() - 8..].to_lowercase());
    assert!(tree.is_dir());
    match c.request(Request::RemoveTask { task: work.id.clone() }).unwrap() {
        Reply::Error { code: ErrorCode::UnsavedWork, message } => assert!(message.contains("not committed"), "{message}"),
        r => panic!("unsaved work must not be deleted: {r:?}"),
    }
    assert!(tree.join("NOTES.md").is_file(), "nothing was deleted");
    assert!(tasks(&mut c).iter().any(|t| t.id == work.id), "the task stays");
    // Committed: the worktree goes, the branch keeps the commit.
    git(&tree, &["add", "-A"]);
    git(&tree, &["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "commit", "-q", "-m", "notes"]);
    assert_eq!(c.request(Request::RemoveTask { task: work.id.clone() }).unwrap(), Reply::Ok);
    events_until(&mut c, "removed", |ch| matches!(ch, Change::TaskRemoved { task } if *task == work.id));
    assert!(!tree.exists(), "the worktree folder is gone");
    let branch = format!("neko/{}", work.id.0[work.id.0.len() - 8..].to_lowercase());
    assert_eq!(git(&r, &["log", "--format=%s", "-1", &branch]), "notes", "the commit is kept on its branch");
    assert!(matches!(c.request(Request::GetTask { task: work.id.clone() }).unwrap(), Reply::Error { code: ErrorCode::NotFound, .. }));
    // A task with nothing left in its scratch folder is removed with it.
    assert_eq!(c.request(Request::RemoveTask { task: plain.id.clone() }).unwrap(), Reply::Ok);
    assert_eq!(tasks(&mut c).len(), 0);
    assert_eq!(folders_in(&d.dir.join(files::SCRATCH)), 0);
    let _ = git(&r, &["worktree", "prune"]);
}

fn create_placed(c: &mut Client, words: &str, workspace: &Workspace, in_folder: bool, from_branch: Option<&str>) -> Reply {
    c.request(Request::CreateTask { crew: None, words: words.into(), workspace: Some(workspace.id.clone()), in_folder, from_branch: from_branch.map(str::to_owned) }).unwrap()
}

fn run_of(c: &mut Client, task: &TaskId) -> Run {
    match c.request(Request::GetTranscript { task: task.clone() }).unwrap() {
        Reply::Transcript { transcript } => transcript.runs.into_iter().next().expect("a run"),
        r => panic!("transcript: {r:?}"),
    }
}

#[test]
fn a_new_thread_says_where_it_can_start_from_and_can_work_in_the_folder_itself_or_from_a_chosen_branch() {
    let outside = Outside(temp_dir("ws-place-outside").canonicalize().unwrap());
    let r = repo(&outside.0);
    git(&r, &["branch", "release-2"]);
    let notes = outside.0.join("notes");
    std::fs::create_dir_all(&notes).unwrap();
    let d = Daemon::start("ws-place");
    let mut c = watcher(&d);
    let acme = add(&mut c, &r);
    let plain = add(&mut c, &notes);

    // Where a thread can start from: a repository says its branches (the checkout's first), anything else says there are none.
    let asked = |c: &mut Client, w: &str| c.request(Request::ListBranches { workspace: w.into() }).unwrap();
    assert_eq!(asked(&mut c, &acme.id), Reply::Branches { workspace: acme.id.clone(), git: true, current: Some("main".into()), branches: vec!["main".into(), "release-2".into()] });
    assert_eq!(asked(&mut c, &plain.id), Reply::Branches { workspace: plain.id.clone(), git: false, current: None, branches: Vec::new() });
    assert_eq!(asked(&mut c, GENERAL), Reply::Branches { workspace: GENERAL.into(), git: false, current: None, branches: Vec::new() });
    assert!(matches!(asked(&mut c, "ws_nobody"), Reply::Error { code: ErrorCode::NotFound, .. }));

    // A branch that is not a branch name is refused before anything is made.
    assert!(matches!(create_placed(&mut c, "x", &acme, false, Some("--upload-pack=x")), Reply::Error { code: ErrorCode::Refused, .. }));
    assert_eq!(tasks(&mut c).len(), 0);

    // In the folder: the run works in the person's own checkout; no worktree, no `neko/` branch.
    let Reply::Task { task: inside } = create_placed(&mut c, "Look here [fake:plain]", &acme, true, None) else { panic!("in the folder") };
    assert!(inside.in_folder && inside.cat.is_none());
    all_done(&mut c, std::slice::from_ref(&inside.id));
    let run = run_of(&mut c, &inside.id);
    assert_eq!((run.workdir.as_str(), run.branch.clone()), (r.to_string_lossy().as_ref(), None));
    assert_eq!(folders_in(&d.dir.join(files::WORKTREES)), 0);
    assert_eq!(git(&r, &["branch", "--list", "neko/*"]), "");
    assert_eq!(git(&r, &["worktree", "list"]).lines().count(), 1);

    // From a branch: its own worktree, and a branch of its own from the chosen one's tip.
    let tip = git(&r, &["rev-parse", "release-2"]);
    let Reply::Task { task: from } = create_placed(&mut c, "Look there [fake:plain]", &acme, false, Some("release-2")) else { panic!("from a branch") };
    assert_eq!(from.from_branch.as_deref(), Some("release-2"));
    all_done(&mut c, std::slice::from_ref(&from.id));
    let run = run_of(&mut c, &from.id);
    assert!(run.branch.as_deref().is_some_and(|b| b.starts_with("neko/")), "{run:?}");
    assert!(run.workdir.contains(files::WORKTREES), "{run:?}");
    // The stored thread says what was chosen, after a fresh read.
    let again = tasks(&mut c).into_iter().find(|t| t.id == from.id).unwrap();
    assert_eq!((again.in_folder, again.from_branch.as_deref(), again.cat), (false, Some("release-2"), None));
    assert_eq!(git(&r, &["rev-parse", "release-2"]), tip, "the person's branch did not move");
    let _ = git(&r, &["worktree", "prune"]);
}
