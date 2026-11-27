//! New thread from end to end, headless, against a real daemon on a temporary data folder and the fake CLI: a folder chosen from the
//! project chip and sent to at once (the folder race) starts the thread in that project; a repository offers where the thread works
//! and the branch it starts from, and "In the folder" makes no worktree; ⌘Return starts a thread and keeps the screen open.
//!
//! Needs the daemon binary: `NEKO_DAEMON_BIN` (scripts/check.sh builds it and sets it). The daemon started here is stopped by the
//! PID it wrote, pass or fail; the real data folder is never touched.

mod support;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use neko_app::state::{App, Daemon};
use neko_protocol::{GENERAL, TaskStatus};
use rune::{Key, Mods, Runtime, TextEdit};
use support::*;

struct Folder {
    dir: PathBuf,
    pid: Option<i32>,
}

impl Drop for Folder {
    fn drop(&mut self) {
        if let Some(pid) = self.pid {
            // SAFETY: the daemon this test's app started, by the PID it wrote.
            unsafe { libc::kill(pid, libc::SIGKILL) };
        }
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn until(rt: &mut Runtime<App>, what: &str, ok: impl Fn(&Runtime<App>) -> bool) {
    let end = Instant::now() + Duration::from_secs(30);
    while !ok(rt) {
        assert!(Instant::now() < end, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(20));
        rt.frame(0.0);
    }
    rt.frame(0.0);
}

fn has_key(rt: &Runtime<App>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn label_of(rt: &Runtime<App>, key: &str) -> Option<String> {
    rt.elements().into_iter().find(|e| e.key.as_deref() == Some(key)).and_then(|e| e.label)
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[test]
fn a_folder_sent_to_at_once_a_repositorys_chips_in_the_folder_and_command_return() {
    let bin = PathBuf::from(std::env::var("NEKO_DAEMON_BIN").expect("set NEKO_DAEMON_BIN to a built neko-daemon (scripts/check.sh does)"));
    let fake = bin.with_file_name("neko-fake-claude");
    assert!(fake.is_file(), "build the fake CLI first: cargo build -p neko-core --bins ({} is missing)", fake.display());
    // SAFETY: set before any thread of this test reads the environment; this is the only test of the file that starts a daemon.
    unsafe { std::env::set_var("NEKO_CLAUDE_BIN", &fake) };
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let mut folder = Folder { dir: std::env::temp_dir().join(format!("neko-newthread-{}-{nanos}", std::process::id())), pid: None };
    let home = folder.dir.join("home");
    std::fs::create_dir_all(home.join(".claude")).unwrap();
    // SAFETY: as above.
    unsafe { std::env::set_var("HOME", &home) };
    let data = folder.dir.join("data");
    let repo = folder.dir.join("acme-api");
    std::fs::create_dir_all(&repo).unwrap();
    let repo = repo.canonicalize().unwrap();
    git(&repo, &["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("README.md"), "# Acme\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "commit", "-q", "-m", "first"]);
    git(&repo, &["branch", "release-2"]);

    let mut rt = runtime((1200.0, 800.0), false, |_| {});
    let conn = neko_app::link::Conn::start(data.clone(), Some(bin), || {});
    rt.update(move |s, _| {
        s.daemon = Daemon::Starting;
        s.conn = Some(conn);
    });
    until(&mut rt, "the app to start the daemon and connect", |rt| rt.state.daemon == Daemon::Connected);
    folder.pid = neko_protocol::client::find(&data).ok().map(|(i, _)| i.pid as i32);
    assert_eq!(rt.state.workspace, GENERAL);

    // The folder race: the folder is chosen and Return pressed before the daemon has answered. The chip names the folder at once,
    // the words stay in the box (it is held, not sent), and the thread then starts in that project.
    let path = repo.to_string_lossy().into_owned();
    rt.update(move |s, _| {
        s.draft = TextEdit::new("Race to the repository [fake:plain]");
        s.add_workspace(path, true);
        assert_eq!(s.chip_project_name(), "acme-api", "the chip names the folder at once");
        s.submit();
        assert!(s.creating.is_none() && s.new_thread.held == Some(false), "the send waits for the project");
        assert_eq!(s.draft.text, "Race to the repository [fake:plain]", "the words are still in the box");
    });
    until(&mut rt, "the thread to open", |rt| rt.state.open_task().is_some());
    let raced = rt.state.open_task().unwrap().clone();
    assert_ne!(raced.workspace, GENERAL, "the thread is in the folder's project, not in No project");
    assert_eq!(rt.state.workspace_of(&raced.workspace).map(|w| w.name.as_str()), Some("acme-api"));
    assert_eq!((raced.cat.as_deref(), raced.in_folder), (None, false));
    until(&mut rt, "its run to finish", |rt| rt.state.open_task().is_some_and(|t| t.status == TaskStatus::Done));

    // A refused folder leaves nothing held: the chip goes back.
    rt.update(|s, _| {
        s.draft = TextEdit::new("Never sent");
        s.add_workspace("/nowhere/at/all".into(), true);
        s.submit();
        assert_eq!(s.new_thread.held, Some(false));
    });
    until(&mut rt, "the refusal", |rt| rt.state.refusal.is_some());
    assert!(rt.state.new_thread.held.is_none() && !rt.state.adding_folder(), "a refused folder holds nothing");
    assert_eq!(rt.state.chip_project_name(), "acme-api", "the chip is the project it had");
    assert_eq!(rt.state.draft.text, "Never sent", "the words are still in the box");
    rt.update(|s, _| {
        s.refusal = None;
        s.draft = TextEdit::default();
    });

    // A repository offers where the thread works and the branch it starts from.
    press(&mut rt, Key::Command('n'), Mods { cmd: true, ..Mods::default() });
    until(&mut rt, "the repository's branches", |rt| rt.state.branch_list().is_some());
    until(&mut rt, "its chips", |rt| has_key(rt, "place-picker") && has_key(rt, "branch-picker"));
    assert_eq!(label_of(&rt, "place-picker").as_deref(), Some("Where it works: New worktree"));
    assert_eq!(label_of(&rt, "branch-picker").as_deref(), Some("Starts from the branch main"));
    assert_eq!(rt.state.branch_list().unwrap().names, ["main", "release-2"], "the checkout's own first");

    // A worktree from another branch: the choice reaches the thread.
    rt.update(|s, _| s.choose_branch(Some("release-2".into())));
    settle(&mut rt);
    assert_eq!(label_of(&rt, "branch-picker").as_deref(), Some("Starts from the branch release-2"));
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Text("From the other branch [fake:plain]".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the thread to open", |rt| rt.state.open_task().is_some_and(|t| t.words.starts_with("From the other")));
    let from = rt.state.open_task().unwrap().clone();
    assert_eq!(from.from_branch.as_deref(), Some("release-2"));
    until(&mut rt, "its run to finish", |rt| rt.state.open_task().is_some_and(|t| t.status == TaskStatus::Done));

    // In the folder: the branch chip goes, and no worktree is made.
    press(&mut rt, Key::Command('n'), Mods { cmd: true, ..Mods::default() });
    until(&mut rt, "the chips again", |rt| has_key(rt, "place-picker"));
    assert_eq!(rt.state.new_thread.from_branch.as_deref(), Some("release-2"), "the choice stays while the project does");
    rt.update(|s, _| s.choose_place(true));
    settle(&mut rt);
    assert_eq!(label_of(&rt, "place-picker").as_deref(), Some("Where it works: In the folder"));
    assert!(!has_key(&rt, "branch-picker"), "in the folder the branch is the checkout's own");
    let worktrees_before = git(&repo, &["worktree", "list"]).lines().count();
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Text("Work right here [fake:plain]".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods::default());
    until(&mut rt, "the thread to open", |rt| rt.state.open_task().is_some_and(|t| t.words.starts_with("Work right here")));
    let inside = rt.state.open_task().unwrap().clone();
    assert!(inside.in_folder && inside.from_branch.is_none(), "{inside:?}");
    until(&mut rt, "its run to finish", |rt| rt.state.open_task().is_some_and(|t| t.status == TaskStatus::Done));
    assert_eq!(git(&repo, &["worktree", "list"]).lines().count(), worktrees_before, "no worktree was made");
    assert_eq!(git(&repo, &["branch", "--list", "neko/*"]), "", "no neko/ branch");

    // ⌘Return starts a thread and keeps the screen: an empty box, no thread open, the next one typed at once.
    press(&mut rt, Key::Command('n'), Mods { cmd: true, ..Mods::default() });
    let before = rt.state.tasks.len();
    click(&mut rt, "composer-input");
    press(&mut rt, Key::Text("First in the background [fake:plain]".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods { cmd: true, ..Mods::default() });
    until(&mut rt, "the first background thread", |rt| rt.state.tasks.len() == before + 1);
    assert!(rt.state.open.is_none() && has_key(&rt, "new-thread-headline"), "the screen stays");
    assert!(rt.state.draft.text.is_empty(), "the box is empty");
    until(&mut rt, "the line that says it started", |rt| rt.state.notice.as_deref().is_some_and(|n| n.starts_with("Started First in the background")));
    press(&mut rt, Key::Text("Second in the background [fake:plain]".into()), Mods::default());
    press(&mut rt, Key::Enter, Mods { cmd: true, ..Mods::default() });
    until(&mut rt, "the second background thread", |rt| rt.state.tasks.len() == before + 2);
    assert!(rt.state.open.is_none(), "still the start screen");
    let started: Vec<&str> = rt.state.tasks.iter().filter(|t| t.words.contains("in the background")).map(|t| t.words.as_str()).collect();
    assert_eq!(started.len(), 2, "{started:?}");
    until(&mut rt, "both to finish", |rt| rt.state.tasks.iter().filter(|t| t.words.contains("in the background")).all(|t| t.status == TaskStatus::Done));
}
