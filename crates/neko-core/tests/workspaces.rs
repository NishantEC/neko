//! Workspaces (docs/knowledge/workspaces.md): the migration from store version 3, adding (one per folder, links resolved),
//! renaming, removing (nothing on disk touched), a missing folder, locating, reordering; and the tidy rule for worktrees (never
//! deletes work) and removing a task.

use std::path::{Path, PathBuf};
use std::process::Command;

use neko_core::store::SCHEMA_VERSION;
use neko_core::workplace::{self, Tidied};
use neko_core::{Core, FixedClock, NewTaskError, SequentialIds, Store, WorkspaceError};
use neko_protocol::{GENERAL, TaskId};

/// A folder removed on drop, with links resolved (the temp folder is a link on macOS).
struct Temp(PathBuf);

impl Temp {
    fn new(name: &str) -> Temp {
        static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let dir = std::env::temp_dir().join(format!("neko-ws-{name}-{}-{n}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        Temp(dir.canonicalize().unwrap())
    }
}

impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn core(path: &Path) -> Core {
    Core::open(path, Box::new(FixedClock::new(1_764_000_000_000, 1000)), Box::new(SequentialIds::default())).unwrap()
}

fn s(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

#[test]
fn a_version_3_store_gets_a_workspace_per_folder_and_general_for_the_rest() {
    let t = Temp::new("migrate");
    let db = t.0.join("neko.sqlite3");
    let acme = t.0.join("acme-api");
    std::fs::create_dir_all(&acme).unwrap();
    // A link to the same folder: the same workspace.
    std::os::unix::fs::symlink(&acme, t.0.join("acme-link")).unwrap();
    let gone = t.0.join("old-notes");
    {
        // The store as version 3 left it: the default workspace named Home, tasks with and without a folder.
        let st = Store::open_at(&db, 3).unwrap();
        assert_eq!(st.version().unwrap(), 3);
        st.connection()
            .execute_batch(&format!(
                "INSERT INTO workspaces VALUES ('default', 'Home', 1);
                 INSERT INTO tasks (id, workspace_id, words, title, status, created_ms, folder) VALUES
                   ('task_A1', 'default', 'one', 'one', 'done', 10, '{acme}'),
                   ('task_A2', 'default', 'two', 'two', 'done', 20, '{link}'),
                   ('task_B1', 'default', 'three', 'three', 'failed', 30, '{gone}'),
                   ('task_C1', 'default', 'four', 'four', 'new', 40, NULL);",
                acme = s(&acme),
                link = s(&t.0.join("acme-link")),
                gone = s(&gone),
            ))
            .unwrap();
    }
    let c = core(&db);
    assert_eq!(c.store.version().unwrap(), SCHEMA_VERSION);
    let ws = c.workspaces().unwrap();
    let names: Vec<(&str, Option<&str>, bool)> = ws.iter().map(|w| (w.name.as_str(), w.folder.as_deref(), w.missing)).collect();
    let acme_s = s(&acme);
    let gone_s = s(&gone);
    assert_eq!(names, vec![("General", None, false), ("acme-api", Some(acme_s.as_str()), false), ("old-notes", Some(gone_s.as_str()), true)]);
    let of = |id: &str| c.store.get_task(&TaskId(id.into())).unwrap().unwrap().workspace;
    assert_eq!(of("task_A1"), ws[1].id);
    assert_eq!(of("task_A2"), ws[1].id, "a link to the same folder is the same workspace");
    assert_eq!(of("task_B1"), ws[2].id, "a folder that is gone still gets its workspace, shown as not found");
    assert_eq!(of("task_C1"), GENERAL, "no folder: General");
    assert_eq!(c.store.get_task(&TaskId("task_A2".into())).unwrap().unwrap().folder.as_deref(), Some(s(&t.0.join("acme-link")).as_str()), "a task's own folder is kept as it was");
    // Opening again changes nothing.
    drop(c);
    let c = core(&db);
    assert_eq!(c.workspaces().unwrap().len(), 3);
}

#[test]
fn a_folder_is_added_once_named_after_itself_and_a_folder_inside_is_its_own() {
    let t = Temp::new("add");
    let mut c = core(&t.0.join("neko.sqlite3"));
    let acme = t.0.join("acme-api");
    std::fs::create_dir_all(acme.join("web")).unwrap();
    let a = c.add_workspace(&s(&acme)).unwrap();
    assert!(!a.existed && a.revision.is_some());
    assert_eq!((a.workspace.name.as_str(), a.workspace.folder.as_deref()), ("acme-api", Some(s(&acme).as_str())));
    let again = c.add_workspace(&format!("{}/", s(&acme))).unwrap();
    assert!(again.existed && again.revision.is_none(), "the same folder again selects the one there is");
    assert_eq!(again.workspace.id, a.workspace.id);
    std::os::unix::fs::symlink(&acme, t.0.join("link")).unwrap();
    assert_eq!(c.add_workspace(&s(&t.0.join("link"))).unwrap().workspace.id, a.workspace.id, "links are resolved before comparing");
    let inner = c.add_workspace(&s(&acme.join("web"))).unwrap();
    assert!(!inner.existed && inner.workspace.id != a.workspace.id, "a folder inside a workspace is its own workspace");
    // Refused: not there, a file, relative.
    std::fs::write(t.0.join("file.txt"), "x").unwrap();
    for bad in [s(&t.0.join("nope")), s(&t.0.join("file.txt")), "acme-api".to_owned(), String::new()] {
        assert!(matches!(c.add_workspace(&bad), Err(WorkspaceError::Folder(_))), "{bad} is not a folder Neko can add");
    }
    let order: Vec<String> = c.workspaces().unwrap().into_iter().map(|w| w.name).collect();
    assert_eq!(order, ["General", "acme-api", "web"]);
}

#[test]
fn general_is_built_in_and_names_are_one_trimmed_line() {
    let t = Temp::new("names");
    let mut c = core(&t.0.join("neko.sqlite3"));
    assert!(matches!(c.rename_workspace(GENERAL, "Mine"), Err(WorkspaceError::Refused(_))));
    assert!(matches!(c.remove_workspace(GENERAL), Err(WorkspaceError::Refused(_))));
    let a = c.add_workspace(&s(&t.0)).unwrap().workspace;
    let (w, _) = c.rename_workspace(&a.id, "  Acme\n  API  ").unwrap();
    assert_eq!(w.name, "Acme API");
    assert!(matches!(c.rename_workspace(&a.id, "   "), Err(WorkspaceError::Refused(_))), "a name is needed");
    let long = "x".repeat(200);
    assert_eq!(c.rename_workspace(&a.id, &long).unwrap().0.name.chars().count(), neko_core::workspaces::MAX_NAME);
    assert!(matches!(c.rename_workspace("ws_nope", "x"), Err(WorkspaceError::NotFound)));
}

#[test]
fn removing_touches_nothing_on_disk_keeps_its_tasks_and_takes_no_new_task() {
    let t = Temp::new("remove");
    let mut c = core(&t.0.join("neko.sqlite3"));
    let acme = t.0.join("acme-api");
    std::fs::create_dir_all(&acme).unwrap();
    std::fs::write(acme.join("README.md"), "kept\n").unwrap();
    let w = c.add_workspace(&s(&acme)).unwrap().workspace;
    let (task, _) = c.create_task_in("Fix the build", Some(&w.id)).unwrap();
    assert_eq!((task.workspace.as_str(), task.folder.as_deref()), (w.id.as_str(), Some(s(&acme).as_str())));
    let (removed, _) = c.remove_workspace(&w.id).unwrap();
    assert!(removed.removed_ms.is_some());
    assert_eq!(std::fs::read_to_string(acme.join("README.md")).unwrap(), "kept\n", "nothing on disk is touched");
    assert_eq!(c.store.list_tasks(&w.id).unwrap().len(), 1, "its tasks stay");
    assert!(matches!(c.create_task_in("More", Some(&w.id)), Err(NewTaskError::Removed(_))));
    let listed = c.workspaces().unwrap();
    assert_eq!(listed.last().unwrap().id, w.id, "removed workspaces are listed last");
    // Adding the folder again brings it back, with its tasks.
    let back = c.add_workspace(&s(&acme)).unwrap();
    assert!(back.restored && back.workspace.id == w.id && back.workspace.removed_ms.is_none());
    assert!(c.create_task_in("More", Some(&w.id)).is_ok());
}

#[test]
fn a_missing_folder_is_said_refuses_new_tasks_and_can_be_located() {
    let t = Temp::new("missing");
    let mut c = core(&t.0.join("neko.sqlite3"));
    let acme = t.0.join("acme-api");
    std::fs::create_dir_all(&acme).unwrap();
    let w = c.add_workspace(&s(&acme)).unwrap().workspace;
    let (task, _) = c.create_task_in("Before it moved", Some(&w.id)).unwrap();
    let moved = t.0.join("code").join("acme-api");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::rename(&acme, &moved).unwrap();
    let listed = c.workspaces().unwrap();
    assert!(listed.iter().find(|x| x.id == w.id).unwrap().missing, "folder not found");
    match c.create_task_in("After", Some(&w.id)) {
        Err(e @ NewTaskError::FolderMissing(_)) => assert!(e.to_string().contains("was not found"), "{e}"),
        r => panic!("a new task in a missing folder is refused: {r:?}"),
    }
    assert_eq!(c.store.get_task(&task.id).unwrap().unwrap().title, "Before it moved", "its tasks stay readable");
    let (w2, _) = c.locate_workspace(&w.id, &s(&moved)).unwrap();
    assert!(!w2.missing && w2.folder.as_deref() == Some(s(&moved).as_str()));
    assert_eq!(c.store.get_task(&task.id).unwrap().unwrap().folder.as_deref(), Some(s(&moved).as_str()), "its tasks follow the folder");
    assert!(c.create_task_in("After", Some(&w.id)).is_ok());
    // A folder that is another workspace's cannot be located onto.
    let other = t.0.join("other");
    std::fs::create_dir_all(&other).unwrap();
    c.add_workspace(&s(&other)).unwrap();
    assert!(matches!(c.locate_workspace(&w.id, &s(&other)), Err(WorkspaceError::Refused(_))));
}

#[test]
fn workspaces_are_reordered_and_general_stays_first() {
    let t = Temp::new("order");
    let mut c = core(&t.0.join("neko.sqlite3"));
    let mut ids = Vec::new();
    for n in ["a", "b", "c"] {
        std::fs::create_dir_all(t.0.join(n)).unwrap();
        ids.push(c.add_workspace(&s(&t.0.join(n))).unwrap().workspace.id);
    }
    let names = |c: &Core| c.workspaces().unwrap().into_iter().map(|w| w.name).collect::<Vec<_>>();
    let (changed, _) = c.move_workspace(&ids[2], 0).unwrap();
    assert_eq!(changed.len(), 3);
    assert_eq!(names(&c), ["General", "c", "a", "b"]);
    c.move_workspace(&ids[2], 9).unwrap();
    assert_eq!(names(&c), ["General", "a", "b", "c"], "past the end is the end");
    assert!(matches!(c.move_workspace(GENERAL, 1), Err(WorkspaceError::Refused(_))));
    c.remove_workspace(&ids[0]).unwrap();
    assert_eq!(names(&c), ["General", "b", "c", "a"]);
    assert!(matches!(c.move_workspace(&ids[0], 0), Err(WorkspaceError::Refused(_))), "a removed workspace has no place to move to");
}

#[test]
fn every_change_is_one_event_and_a_removed_task_leaves_only_its_event() {
    let t = Temp::new("events");
    let mut c = core(&t.0.join("neko.sqlite3"));
    let w = c.add_workspace(&s(&t.0)).unwrap().workspace;
    c.rename_workspace(&w.id, "Acme").unwrap();
    c.remove_workspace(&w.id).unwrap();
    let (task, _) = c.create_task("Gone soon").unwrap();
    let now = c.now();
    c.store.delete_task(&task.id, now).unwrap();
    assert!(c.store.get_task(&task.id).unwrap().is_none());
    let kinds: Vec<String> = c.store.events_since(0).unwrap().into_iter().map(|e| e.1).collect();
    assert_eq!(kinds, ["workspace_added", "workspace_renamed", "workspace_removed", "task_created", "task_removed"]);
}

// The tidy rule, on real git repositories.

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with one commit, and a data folder.
fn repo(t: &Temp) -> (PathBuf, PathBuf) {
    let repo = t.0.join("acme-api");
    std::fs::create_dir_all(&repo).unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    git(&repo, &["config", "user.email", "asha@example.com"]);
    git(&repo, &["config", "user.name", "Asha"]);
    std::fs::write(repo.join("README.md"), "Acme\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "first"]);
    let data = t.0.join("data");
    std::fs::create_dir_all(&data).unwrap();
    (repo, data)
}

fn task(n: u32) -> TaskId {
    TaskId(format!("task_01KAB00000000000000000{n:04}"))
}

fn branches(repo: &Path) -> String {
    git(repo, &["branch", "--list", "neko/*"])
}

#[test]
fn a_worktree_with_nothing_in_it_is_tidied_with_its_branch_and_comes_back_for_a_follow_up() {
    let t = Temp::new("tidy-clean");
    let (repo, data) = repo(&t);
    let id = task(1);
    let p = workplace::prepare(&data, &id, Some(&s(&repo)), &Default::default()).unwrap();
    assert!(Path::new(&p.workdir).is_dir());
    assert_eq!(workplace::tidy(&data, &id, &p), Tidied::Removed { branch: true });
    assert!(!workplace::worktree_root(&data, &id).exists());
    assert_eq!(branches(&repo), "", "the empty branch went too");
    // A follow-up makes it again on the same branch name, from where it began.
    workplace::reopen(&data, &id, &p).unwrap();
    assert!(Path::new(&p.workdir).join("README.md").is_file());
    assert!(branches(&repo).contains(p.branch.as_deref().unwrap()));
}

#[test]
fn tidying_never_deletes_work() {
    let t = Temp::new("tidy-work");
    let (repo, data) = repo(&t);
    // An uncommitted file; a commit the base lacks (unpushed); both.
    let cases: [(&str, bool, bool); 3] = [("uncommitted", true, false), ("commit", false, true), ("both", true, true)];
    for (n, (what, dirty, commit)) in cases.iter().enumerate() {
        let id = task(10 + n as u32);
        let p = workplace::prepare(&data, &id, Some(&s(&repo)), &Default::default()).unwrap();
        let tree = Path::new(&p.workdir);
        if *commit {
            std::fs::write(tree.join("done.txt"), "work\n").unwrap();
            git(tree, &["add", "done.txt"]);
            git(tree, &["commit", "-q", "-m", "work"]);
        }
        if *dirty {
            std::fs::write(tree.join("draft.txt"), "unsaved\n").unwrap();
        }
        assert_eq!(workplace::tidy(&data, &id, &p), Tidied::Kept, "{what}: kept");
        assert!(tree.is_dir(), "{what}: the worktree is still there");
        assert!(branches(&repo).contains(p.branch.as_deref().unwrap()), "{what}: the branch is still there");
        if *dirty {
            assert_eq!(std::fs::read_to_string(tree.join("draft.txt")).unwrap(), "unsaved\n");
        }
        // Removing the task: refused while anything is uncommitted; with commits only, the folder goes and the branch keeps them.
        match workplace::remove_place(&data, &id, &p) {
            Err(why) => {
                assert!(*dirty, "{what}: refused only for unsaved work: {why}");
                assert!(why.contains("not committed"), "{why}");
                assert!(tree.join("draft.txt").is_file());
            }
            Ok(said) => {
                assert!(!*dirty, "{what}: {said}");
                assert!(!tree.exists());
                assert!(said.contains("keeps its 1 commit"), "{said}");
                assert_eq!(git(&repo, &["log", "--format=%s", "-1", p.branch.as_deref().unwrap()]), "work", "the commit is kept in the repository");
            }
        }
    }
}

#[test]
fn an_empty_scratch_folder_is_tidied_and_one_with_files_is_kept() {
    let t = Temp::new("scratch");
    let data = t.0.join("data");
    let (a, b) = (task(20), task(21));
    let pa = workplace::prepare(&data, &a, None, &Default::default()).unwrap();
    let pb = workplace::prepare(&data, &b, None, &Default::default()).unwrap();
    std::fs::write(Path::new(&pb.workdir).join("report.md"), "kept\n").unwrap();
    assert_eq!(workplace::tidy(&data, &a, &pa), Tidied::Removed { branch: false });
    assert!(!Path::new(&pa.workdir).exists());
    assert_eq!(workplace::tidy(&data, &b, &pb), Tidied::Kept);
    assert!(workplace::remove_place(&data, &b, &pb).is_err(), "files in a scratch folder are not deleted");
    workplace::reopen(&data, &a, &pa).unwrap();
    assert!(Path::new(&pa.workdir).is_dir(), "a follow-up makes it again");
    // A plain folder is the person's own: never touched.
    let plain = t.0.join("notes");
    std::fs::create_dir_all(&plain).unwrap();
    let pp = workplace::prepare(&data, &task(22), Some(&s(&plain)), &Default::default()).unwrap();
    assert!(pp.plain);
    assert_eq!(workplace::tidy(&data, &task(22), &pp), Tidied::Nothing);
    assert!(workplace::remove_place(&data, &task(22), &pp).is_ok());
    assert!(plain.is_dir());
}

#[test]
fn a_thread_can_start_from_a_chosen_branch_or_work_in_the_folder_itself() {
    let t = Temp::new("placement");
    let (repo, data) = repo(&t);
    // A second branch with one more commit, while the checkout stays on main.
    git(&repo, &["switch", "-q", "-c", "release/2"]);
    std::fs::write(repo.join("NOTES.md"), "second\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-q", "-m", "second"]);
    let tip = git(&repo, &["rev-parse", "release/2"]);
    git(&repo, &["switch", "-q", "main"]);
    let main_tip = git(&repo, &["rev-parse", "main"]);

    // What the chip offers: the checkout's branch first, then the rest by name.
    let b = workplace::branches(&repo);
    assert_eq!((b.git, b.current.as_deref(), b.names.as_slice()), (true, Some("main"), ["main".to_owned(), "release/2".to_owned()].as_slice()));
    assert_eq!(workplace::branches(&t.0), workplace::Branches::default(), "a folder that is not a repository offers nothing");
    assert_eq!(workplace::branches(&t.0.join("nowhere")), workplace::Branches::default());

    // Nothing chosen: a worktree on the commit the checkout is on.
    let a = workplace::prepare(&data, &task(30), Some(&s(&repo)), &Default::default()).unwrap();
    assert_eq!((a.base_commit.as_deref(), a.plain), (Some(main_tip.as_str()), false));
    // A branch chosen: the worktree starts from its tip, on its own `neko/` branch, and the person's checkout is untouched.
    let how = workplace::Placement { in_folder: false, from_branch: Some("release/2".into()) };
    let b = workplace::prepare(&data, &task(31), Some(&s(&repo)), &how).unwrap();
    assert_eq!((b.base_commit.as_deref(), b.plain), (Some(tip.as_str()), false));
    assert!(b.branch.as_deref().unwrap().starts_with("neko/"));
    assert!(Path::new(&b.workdir).join("NOTES.md").is_file(), "the chosen branch's files are there");
    assert_eq!(git(&repo, &["rev-parse", "--abbrev-ref", "HEAD"]), "main");
    // A branch that is not there, and a name git would take for an option, are refused in words.
    let gone = workplace::Placement { in_folder: false, from_branch: Some("nope".into()) };
    assert!(matches!(workplace::prepare(&data, &task(32), Some(&s(&repo)), &gone), Err(workplace::PlaceError::Other(w)) if w.contains("nope")));
    assert!(!workplace::branch_name_ok("--upload-pack=x") && !workplace::branch_name_ok("a b") && !workplace::branch_name_ok("") && workplace::branch_name_ok("release/2"));

    // In the folder: no worktree, no `neko/` branch; it works where the person's checkout is, and its first edit asks.
    let before = git(&repo, &["worktree", "list"]);
    let f = workplace::prepare(&data, &task(33), Some(&s(&repo)), &workplace::Placement { in_folder: true, from_branch: None }).unwrap();
    assert_eq!((f.workdir.as_str(), f.plain, f.branch.clone()), (s(&repo).as_str(), true, None));
    assert_eq!(git(&repo, &["worktree", "list"]), before, "no worktree was made");
    assert!(!workplace::worktree_root(&data, &task(33)).exists());
    // Nothing of the person's is tidied away, and removing the thread leaves the folder.
    assert_eq!(workplace::tidy(&data, &task(33), &f), Tidied::Nothing);
    assert!(workplace::remove_place(&data, &task(33), &f).is_ok());
    assert!(repo.join("README.md").is_file());

    // The choice is stored with the thread.
    let mut c = core(&t.0.join("neko.sqlite3"));
    let w = c.add_workspace(&s(&repo)).unwrap().workspace;
    let made = c.create_task_placed("Look at it", Some(&w.id), None, &workplace::Placement { in_folder: false, from_branch: Some("release/2".into()) }).unwrap().0;
    assert_eq!((made.in_folder, made.from_branch.as_deref(), made.cat), (false, Some("release/2"), None));
    let same = c.store.get_task(&made.id).unwrap().unwrap();
    assert_eq!((same.in_folder, same.from_branch.as_deref()), (false, Some("release/2")));
    let inside = c.create_task_placed("Edit it", Some(&w.id), None, &workplace::Placement { in_folder: true, from_branch: None }).unwrap().0;
    assert!(c.store.get_task(&inside.id).unwrap().unwrap().in_folder);
    assert!(matches!(c.create_task_placed("Bad", Some(&w.id), None, &workplace::Placement { in_folder: false, from_branch: Some("-x".into()) }), Err(NewTaskError::Branch(_))));
}
