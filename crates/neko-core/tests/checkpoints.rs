//! Checkpoints on real temporary git repositories (docs/knowledge/undo.md): what a snapshot holds, that it touches nothing of the
//! person's, that restore puts back exact trees, the size guard, the shadow repository, and the states git must be refused in.

mod support;

use std::collections::BTreeMap;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use neko_core::checkpoint::{self, Folder, Kind, Limits, Refusal};
use neko_protocol::FileStatus;
use support::Temp;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "-c", "commit.gpgsign=false", "-c", "init.defaultBranch=main"]).args(args).env("GIT_TERMINAL_PROMPT", "0").output().unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn write(dir: &Path, path: &str, bytes: &[u8]) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

/// A repository with a commit: README, src/lib.rs, a script, a binary, an ignore rule.
fn repo(t: &Temp, name: &str) -> PathBuf {
    let r = t.0.join(name);
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "--quiet"]);
    write(&r, "README.md", b"# acme-api\nA small project.\n");
    write(&r, "src/lib.rs", b"pub fn one() -> u32 {\n    1\n}\n");
    write(&r, "run.sh", b"#!/bin/sh\necho hi\n");
    write(&r, "logo.png", &[0x89, b'P', b'N', b'G', 0, 1, 2, 3]);
    write(&r, ".gitignore", b"target/\n*.log\n");
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "--quiet", "-m", "first"]);
    r
}

fn repo_folder(work: &Path) -> Folder {
    Folder { kind: Kind::Repo, work: work.to_path_buf(), shadow: None }
}

/// Everything of the person's that a checkpoint must not change, byte for byte.
fn untouched(repo: &Path) -> BTreeMap<String, Vec<u8>> {
    let gd = PathBuf::from(git(repo, &["rev-parse", "--absolute-git-dir"]).trim());
    let mut m = BTreeMap::new();
    for f in ["index", "HEAD", "config", "packed-refs", "FETCH_HEAD", "ORIG_HEAD", "logs/HEAD"] {
        m.insert(f.to_owned(), std::fs::read(gd.join(f)).unwrap_or_default());
    }
    m.insert("branches".into(), git(repo, &["for-each-ref", "--format=%(refname) %(objectname)", "refs/heads", "refs/remotes", "refs/stash", "refs/tags"]).into_bytes());
    m.insert("stash".into(), git(repo, &["stash", "list"]).into_bytes());
    m.insert("status".into(), Command::new("git").arg("-C").arg(repo).args(["status", "--porcelain=v2", "--untracked-files=all", "--branch"]).env("GIT_OPTIONAL_LOCKS", "0").output().unwrap().stdout);
    m.insert("files".into(), tree_listing(repo).into_bytes());
    m
}

/// Every file under `dir` (but `.git`), with its mode, size and modification time.
fn tree_listing(dir: &Path) -> String {
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().flatten().collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            if e.file_name() == ".git" {
                continue;
            }
            let m = std::fs::symlink_metadata(e.path()).unwrap();
            let rel = e.path().strip_prefix(base).unwrap().to_string_lossy().into_owned();
            out.push(format!("{rel} {:o} {} {:?}", m.permissions().mode(), m.len(), m.modified().ok()));
            if m.is_dir() {
                walk(base, &e.path(), out);
            }
        }
    }
    walk(dir, dir, &mut out);
    out.join("\n")
}

/// Every file's bytes, mode and link target under `dir` (but `.git` and ignored build output): what "exactly as it was" means.
fn contents(dir: &Path) -> BTreeMap<String, (u32, Vec<u8>)> {
    let mut m = BTreeMap::new();
    fn walk(base: &Path, dir: &Path, m: &mut BTreeMap<String, (u32, Vec<u8>)>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            if e.file_name() == ".git" || e.file_name() == "target" {
                continue;
            }
            let p = e.path();
            let meta = std::fs::symlink_metadata(&p).unwrap();
            let rel = p.strip_prefix(base).unwrap().to_string_lossy().into_owned();
            if meta.file_type().is_symlink() {
                m.insert(rel, (0o120000, std::fs::read_link(&p).unwrap().to_string_lossy().into_owned().into_bytes()));
            } else if meta.is_dir() {
                walk(base, &p, m);
            } else {
                m.insert(rel, (meta.permissions().mode() & 0o111, std::fs::read(&p).unwrap()));
            }
        }
    }
    walk(dir, dir, &mut m);
    m
}

fn tmp(t: &Temp) -> PathBuf {
    t.0.join("neko-data").join("undo").join("tmp")
}

#[test]
fn a_snapshot_holds_untracked_files_respects_ignores_and_touches_nothing() {
    let t = Temp::new("cp-snapshot");
    let r = repo(&t, "acme-api");
    // A staged change, an unstaged one, an untracked file, ignored ones, and a stash.
    write(&r, "README.md", b"# acme-api\nA tiny project.\n");
    git(&r, &["stash", "--quiet"]);
    write(&r, "src/lib.rs", b"pub fn one() -> u32 {\n    2\n}\n");
    git(&r, &["add", "src/lib.rs"]);
    write(&r, "README.md", b"# acme-api\nChanged, not staged.\n");
    write(&r, "notes/todo.md", b"- ship it\n");
    write(&r, "debug.log", b"ignored\n");
    write(&r, "target/out.bin", b"ignored build output");
    let before = untouched(&r);
    let f = repo_folder(&r);
    let s = checkpoint::take(&f, &tmp(&t), "Neko checkpoint", &Limits::default()).unwrap();
    checkpoint::keep(&f, "refs/neko/checkpoints/task_1/run_1/start", &s.commit).unwrap();
    assert_eq!(untouched(&r), before, "index, HEAD, branches, stash, status and the files are byte for byte the same");

    let listed = git(&r, &["ls-tree", "-r", "--name-only", &s.tree]);
    for want in ["README.md", "src/lib.rs", "notes/todo.md", ".gitignore", "logo.png", "run.sh"] {
        assert!(listed.lines().any(|l| l == want), "{want} is in the snapshot: {listed}");
    }
    assert!(!listed.contains("debug.log") && !listed.contains("target/"), "ignored files stay out: {listed}");
    assert_eq!(git(&r, &["show", &format!("{}:README.md", s.tree)]), "# acme-api\nChanged, not staged.\n", "the working file, not the staged one");
    assert_eq!(s.head.as_deref(), Some(git(&r, &["rev-parse", "HEAD"]).trim()));
    assert_eq!(git(&r, &["rev-parse", &format!("{}^", s.commit)]).trim(), git(&r, &["rev-parse", "HEAD"]).trim(), "its parent is HEAD");
    assert_eq!(git(&r, &["branch", "--contains", &s.commit]).trim(), "", "on no branch");
    assert_eq!(s.files, 6);
    assert!(s.stored >= 2, "the new and changed files: {s:?}");
    assert!(std::fs::read_dir(tmp(&t)).unwrap().next().is_none(), "the temporary index is gone");
}

/// Makes every kind of change: an edit, a new file in a new subfolder, a deletion, a rename, a mode change, a binary edit, a link.
fn change_everything(r: &Path) {
    write(r, "src/lib.rs", b"pub fn one() -> u32 {\n    1\n}\n// two\n");
    write(r, "docs/guide/start.md", b"# Start\n");
    std::fs::remove_file(r.join("README.md")).unwrap();
    std::fs::rename(r.join("src/lib.rs"), r.join("src/core.rs")).unwrap();
    std::fs::set_permissions(r.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    write(r, "logo.png", &[0x89, b'P', b'N', b'G', 0, 9, 9, 9, 9, 9]);
    std::os::unix::fs::symlink("docs/guide/start.md", r.join("START")).unwrap();
}

#[test]
fn undo_and_redo_put_back_exact_trees_and_review_shows_exactly_the_change() {
    let t = Temp::new("cp-restore");
    let base = repo(&t, "acme-api");
    // A task's worktree, as Neko makes one.
    let wt = t.0.join("worktrees").join("abcd1234");
    git(&base, &["worktree", "add", "--quiet", "-b", "neko/abcd1234", &wt.to_string_lossy(), "HEAD"]);
    let f = repo_folder(&wt);
    let limits = Limits::default();
    let start = checkpoint::take(&f, &tmp(&t), "start", &limits).unwrap();
    let before = contents(&wt);
    change_everything(&wt);
    let after = contents(&wt);
    let end = checkpoint::take(&f, &tmp(&t), "end", &limits).unwrap();
    let person = untouched(&base);

    // Review: exactly the run's change.
    let files = checkpoint::files(&f, &start.tree, &end.tree).unwrap();
    let by = |p: &str| files.iter().find(|x| x.path == p).unwrap_or_else(|| panic!("{p} in {files:?}"));
    assert_eq!(by("src/core.rs").status, FileStatus::Renamed);
    assert_eq!(by("src/core.rs").old_path.as_deref(), Some("src/lib.rs"));
    assert_eq!(by("README.md").status, FileStatus::Deleted);
    assert_eq!(by("docs/guide/start.md").status, FileStatus::Added);
    assert_eq!(by("run.sh").new_mode.as_deref(), Some("100755"));
    assert!(by("logo.png").binary);
    assert_eq!((by("logo.png").old_size, by("logo.png").new_size), (Some(8), Some(10)));
    assert_eq!(by("START").status, FileStatus::Added);
    assert_eq!(files.len(), 6, "{files:?}");
    let lines = checkpoint::patch(&f, &start.tree, &end.tree, &["src/core.rs", "src/lib.rs"]).unwrap();
    assert!(lines[0].starts_with("@@"), "{lines:?}");
    assert!(lines.iter().any(|l| l == "+// two"), "{lines:?}");
    assert_eq!(lines.iter().filter(|l| l.starts_with('-')).count(), 0);
    let c = checkpoint::changes(&f, &start.tree, &end.tree).unwrap();
    assert_eq!((c.files, c.added, c.removed), (6, 3, 2), "{files:?}");

    // Undo: the folder as it was at the start, every kind of change taken back.
    let now = checkpoint::tree_now(&f, &tmp(&t), &limits).unwrap();
    assert_eq!(now, end.tree, "nothing changed since the end");
    checkpoint::restore(&f, &tmp(&t), &end.tree, &start.tree, &limits).unwrap();
    assert_eq!(contents(&wt), before, "undo restores the exact tree");
    assert!(!wt.join("docs").exists(), "an emptied folder goes");
    assert_eq!(checkpoint::tree_now(&f, &tmp(&t), &limits).unwrap(), start.tree);
    // Redo.
    checkpoint::restore(&f, &tmp(&t), &start.tree, &end.tree, &limits).unwrap();
    assert_eq!(contents(&wt), after, "redo restores the exact tree");
    assert_eq!(untouched(&base), person, "the person's checkout, index, branch and stash are untouched throughout");
    assert_eq!(git(&wt, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(), "neko/abcd1234", "the task's branch never moved");
    assert_eq!(git(&wt, &["rev-parse", "HEAD"]).trim(), start.head.as_deref().unwrap());
}

#[test]
fn files_changed_since_are_named_and_a_restore_over_them_is_refused_by_git() {
    let t = Temp::new("cp-since");
    let r = repo(&t, "acme-api");
    let f = repo_folder(&r);
    let limits = Limits::default();
    let start = checkpoint::take(&f, &tmp(&t), "start", &limits).unwrap();
    write(&r, "README.md", b"# acme-api\nRun's edit.\n");
    let end = checkpoint::take(&f, &tmp(&t), "end", &limits).unwrap();
    // The person edits after the run.
    write(&r, "README.md", b"# acme-api\nThe person's edit.\n");
    write(&r, "mine.txt", b"mine\n");
    let now = checkpoint::tree_now(&f, &tmp(&t), &limits).unwrap();
    assert_ne!(now, end.tree);
    let mut differ = checkpoint::differing(&f, &end.tree, &now).unwrap();
    differ.sort();
    assert_eq!(differ, vec!["README.md".to_owned(), "mine.txt".to_owned()]);
    // Restoring as if the folder were still at the end is refused, and writes nothing.
    let held = contents(&r);
    assert!(checkpoint::restore(&f, &tmp(&t), &end.tree, &start.tree, &limits).is_err());
    assert_eq!(contents(&r), held, "a refused restore wrote nothing");
    // The confirm path: the folder as it is now is the restore's starting point (saved first by the daemon).
    checkpoint::restore(&f, &tmp(&t), &now, &start.tree, &limits).unwrap();
    assert_eq!(std::fs::read_to_string(r.join("README.md")).unwrap(), "# acme-api\nA small project.\n");
    assert!(!r.join("mine.txt").exists());
    checkpoint::restore(&f, &tmp(&t), &start.tree, &now, &limits).unwrap();
    assert_eq!(std::fs::read_to_string(r.join("mine.txt")).unwrap(), "mine\n", "the saved state comes back whole");
}

#[test]
fn the_size_guard_refuses_a_snapshot_and_says_why() {
    let t = Temp::new("cp-size");
    let r = repo(&t, "acme-api");
    for i in 0..30 {
        write(&r, &format!("data/{i}.txt"), &[b'x'; 100]);
    }
    let f = repo_folder(&r);
    let few = Limits { files: 10, ..Limits::default() };
    let e = checkpoint::take(&f, &tmp(&t), "start", &few).unwrap_err();
    assert!(matches!(e, Refusal::TooBig { files: 30, .. }), "{e:?}");
    assert_eq!(e.to_string(), "the folder has 30 new or changed files (3.0 KB), more than undo keeps (10 files or 100.0 MB)");
    let small = Limits { bytes: 1_000, ..Limits::default() };
    assert!(matches!(checkpoint::take(&f, &tmp(&t), "start", &small), Err(Refusal::TooBig { .. })));
    assert!(checkpoint::refs_under(&f, "refs/neko/checkpoints/").is_empty(), "no checkpoint was kept");
}

#[test]
fn a_plain_folder_gets_a_shadow_repository_and_nothing_is_written_into_it() {
    let t = Temp::new("cp-shadow");
    let dir = t.0.join("notes");
    write(&dir, "plan.md", b"# Plan\n");
    write(&dir, "sub/a.txt", b"a\n");
    write(&dir, ".DS_Store", b"finder");
    let data = t.0.join("neko-data");
    let task = neko_protocol::TaskId("task_01KAB0000000000000ABCDWXYZ".into());
    let place = neko_core::Place { workdir: dir.to_string_lossy().into_owned(), plain: true, ..Default::default() };
    let f = checkpoint::folder_for(&data, &task, &place).unwrap();
    assert_eq!(f.kind, Kind::Shadow);
    assert_eq!(f.shadow.as_deref(), Some(data.join("undo").join("abcdwxyz.git").as_path()));
    let listing = tree_listing(&dir);
    let limits = Limits::default();
    let start = checkpoint::take(&f, &checkpoint::tmp_dir(&data), "start", &limits).unwrap();
    checkpoint::keep(&f, "refs/neko/checkpoints/task_1/run_1/start", &start.commit).unwrap();
    assert_eq!(tree_listing(&dir), listing, "nothing was written into the person's folder");
    assert!(!dir.join(".git").exists());
    assert_eq!(start.head, None);
    assert_eq!(start.files, 2, ".DS_Store is left out");
    let before = contents(&dir);
    write(&dir, "plan.md", b"# Plan\n\n1. Ship\n");
    std::fs::remove_file(dir.join("sub/a.txt")).unwrap();
    write(&dir, "new.txt", b"new\n");
    let end = checkpoint::take(&f, &checkpoint::tmp_dir(&data), "end", &limits).unwrap();
    assert_eq!(checkpoint::changes(&f, &start.tree, &end.tree).unwrap().files, 3);
    checkpoint::restore(&f, &checkpoint::tmp_dir(&data), &end.tree, &start.tree, &limits).unwrap();
    let mut back = contents(&dir);
    back.remove(".DS_Store");
    let mut want = before.clone();
    want.remove(".DS_Store");
    assert_eq!(back, want, "undo in a plain folder restores the exact tree");
    assert!(dir.join(".DS_Store").exists(), "a file undo does not keep is left alone");
    assert!(!dir.join(".git").exists(), "still no repository in the folder");

    // Housekeeping: the refs go, then the shadow's objects.
    assert_eq!(checkpoint::drop_refs(&f, "refs/neko/checkpoints/task_1/"), 1);
    checkpoint::shadow_gc(&f);
    assert!(!checkpoint::has(&f, &start.commit), "after its refs and a gc, the shadow keeps nothing of it");
}

#[test]
fn a_repository_mid_merge_with_no_commits_or_a_locked_index_is_refused_plainly() {
    let t = Temp::new("cp-states");
    let limits = Limits::default();

    // No commits yet: refused, for the folder itself and for a plain workspace inside it.
    let empty = t.0.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    git(&empty, &["init", "--quiet"]);
    write(&empty, "a.txt", b"a\n");
    assert_eq!(checkpoint::take(&repo_folder(&empty), &tmp(&t), "start", &limits).unwrap_err(), Refusal::NoCommits);
    let task = neko_protocol::TaskId("task_01KAB0000000000000ABCDWXYZ".into());
    let place = neko_core::Place { workdir: empty.to_string_lossy().into_owned(), plain: true, ..Default::default() };
    assert_eq!(checkpoint::folder_for(&t.0.join("data"), &task, &place).unwrap_err(), Refusal::NoCommits);
    assert_eq!(Refusal::NoCommits.to_string(), "this folder is a git repository with no commits yet");

    // Mid-merge: a conflict left by a real merge.
    let r = repo(&t, "acme-api");
    git(&r, &["checkout", "--quiet", "-b", "other"]);
    write(&r, "README.md", b"# acme-api\nOther.\n");
    git(&r, &["commit", "--quiet", "-am", "other"]);
    git(&r, &["checkout", "--quiet", "main"]);
    write(&r, "README.md", b"# acme-api\nMain.\n");
    git(&r, &["commit", "--quiet", "-am", "main"]);
    let merged = Command::new("git").arg("-C").arg(&r).args(["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "merge", "other"]).output().unwrap();
    assert!(!merged.status.success(), "the merge conflicts");
    let before = untouched(&r);
    let e = checkpoint::take(&repo_folder(&r), &tmp(&t), "start", &limits).unwrap_err();
    assert_eq!(e, Refusal::Midway("merge"));
    assert_eq!(e.to_string(), "the repository is in the middle of a merge");
    assert_eq!(untouched(&r), before, "the merge in progress is untouched");
    git(&r, &["merge", "--abort"]);

    // A locked index: a snapshot reads a copy and is fine; a restore is refused.
    let f = repo_folder(&r);
    let start = checkpoint::take(&f, &tmp(&t), "start", &limits).unwrap();
    write(&r, "README.md", b"changed\n");
    let end = checkpoint::take(&f, &tmp(&t), "end", &limits).unwrap();
    let lock = r.join(".git").join("index.lock");
    std::fs::write(&lock, b"").unwrap();
    let locked = checkpoint::take(&f, &tmp(&t), "again", &limits);
    assert!(locked.is_ok(), "a snapshot never needs the lock: {locked:?}");
    assert_eq!(checkpoint::restore(&f, &tmp(&t), &end.tree, &start.tree, &limits).unwrap_err(), Refusal::IndexLocked);
    assert_eq!(std::fs::read_to_string(r.join("README.md")).unwrap(), "changed\n");
    assert!(lock.exists(), "someone else's lock is left alone");
    std::fs::remove_file(&lock).unwrap();
}

#[test]
fn a_detached_head_and_a_submodule_are_safe() {
    let t = Temp::new("cp-detached");
    let limits = Limits::default();
    let lib = repo(&t, "acme-lib");
    let r = repo(&t, "acme-api");
    let added = Command::new("git").arg("-C").arg(&r).args(["-c", "user.name=Asha", "-c", "user.email=asha@example.com", "-c", "protocol.file.allow=always", "submodule", "add", "--quiet"]).arg(&lib).arg("vendor/lib").output().unwrap();
    assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));
    git(&r, &["commit", "--quiet", "-m", "lib"]);
    git(&r, &["checkout", "--quiet", "--detach", "HEAD"]);
    let f = repo_folder(&r);
    let before = untouched(&r);
    let sub_before = tree_listing(&r.join("vendor/lib"));
    let start = checkpoint::take(&f, &tmp(&t), "start", &limits).unwrap();
    assert_eq!(untouched(&r), before);
    write(&r, "src/lib.rs", b"pub fn one() -> u32 {\n    3\n}\n");
    let end = checkpoint::take(&f, &tmp(&t), "end", &limits).unwrap();
    let files = checkpoint::files(&f, &start.tree, &end.tree).unwrap();
    assert_eq!(files.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), vec!["src/lib.rs"], "the submodule is not a change");
    checkpoint::restore(&f, &tmp(&t), &end.tree, &start.tree, &limits).unwrap();
    assert_eq!(std::fs::read_to_string(r.join("src/lib.rs")).unwrap(), "pub fn one() -> u32 {\n    1\n}\n");
    assert_eq!(tree_listing(&r.join("vendor/lib")), sub_before, "the submodule's files are untouched");
    assert_eq!(git(&r, &["rev-parse", "HEAD"]).trim(), start.head.as_deref().unwrap(), "HEAD stays detached where it was");
}

#[test]
fn refs_are_neko_s_own_and_go_with_housekeeping() {
    let t = Temp::new("cp-refs");
    let r = repo(&t, "acme-api");
    let f = repo_folder(&r);
    let s = checkpoint::take(&f, &tmp(&t), "start", &Limits::default()).unwrap();
    assert!(checkpoint::keep(&f, "refs/heads/main", &s.commit).is_err(), "never a branch");
    for run in ["run_1", "run_2"] {
        for kind in ["start", "end"] {
            checkpoint::keep(&f, &format!("refs/neko/checkpoints/task_1/{run}/{kind}"), &s.commit).unwrap();
        }
    }
    checkpoint::keep(&f, "refs/neko/checkpoints/task_2/run_3/start", &s.commit).unwrap();
    assert_eq!(checkpoint::refs_under(&f, "refs/neko/checkpoints/task_1/").len(), 4);
    assert!(checkpoint::drop_ref(&f, "refs/neko/checkpoints/task_1/run_1/start"));
    assert_eq!(checkpoint::drop_refs(&f, "refs/neko/checkpoints/task_1/"), 3);
    assert_eq!(checkpoint::refs_under(&f, "refs/neko/checkpoints/").len(), 1, "another task's stay");
    assert_eq!(checkpoint::drop_refs(&f, "refs/heads/"), 0, "only Neko's refs are ever deleted");
    assert!(git(&r, &["branch", "--list"]).contains("main"));
}

/// The measurement in docs/knowledge/undo.md: a checkpoint of a repository with 5,000 files (each about 2 KB and different, in 50 folders),
/// first with nothing changed, then with 50 files changed and 50 new, and the first one of a 5,000-file plain folder.
#[test]
fn a_checkpoint_of_5000_files_is_quick() {
    let t = Temp::new("cp-measure");
    let r = t.0.join("big");
    std::fs::create_dir_all(&r).unwrap();
    git(&r, &["init", "--quiet"]);
    let body: Vec<u8> = (0..2_000).map(|i| b'a' + (i % 26) as u8).collect();
    let unique = |i: usize| [format!("file {i}\n").into_bytes(), body.clone()].concat();
    for i in 0..5_000 {
        write(&r, &format!("dir{:02}/file{i}.txt", i % 50), &unique(i));
    }
    git(&r, &["add", "-A"]);
    git(&r, &["commit", "--quiet", "-m", "five thousand"]);
    let f = repo_folder(&r);
    let limits = Limits::default();
    let quiet = checkpoint::take(&f, &tmp(&t), "start", &limits).unwrap();
    for i in 0..50 {
        write(&r, &format!("dir{:02}/file{i}.txt", i % 50), b"changed\n");
        write(&r, &format!("new/n{i}.txt"), &unique(10_000 + i));
    }
    let objects = || checkpoint::disk_bytes(&r.join(".git").join("objects"));
    let o0 = objects();
    let busy = checkpoint::take(&f, &tmp(&t), "end", &limits).unwrap();
    let grew = objects() - o0;
    let shadow_dir = t.0.join("plain");
    for i in 0..5_000 {
        write(&shadow_dir, &format!("dir{:02}/file{i}.txt", i % 50), &unique(i));
    }
    let shadow = Folder { kind: Kind::Shadow, work: shadow_dir.clone(), shadow: Some(t.0.join("data/undo/plain.git")) };
    let first = checkpoint::take(&shadow, &tmp(&t), "start", &limits).unwrap();
    let again = checkpoint::take(&shadow, &tmp(&t), "end", &limits).unwrap();
    let shadow_bytes = checkpoint::disk_bytes(&t.0.join("data/undo/plain.git"));
    eprintln!(
        "measure: repository of 5,000 files: unchanged {:?}; 100 changed or new {:?}, objects +{} bytes; plain folder of 5,000 files: first {:?} ({} bytes in the shadow), next {:?}",
        quiet.took, busy.took, grew, first.took, shadow_bytes, again.took
    );
    assert_eq!(quiet.files, 5_000);
    assert_eq!(busy.stored, 100);
    assert!(quiet.took < Duration::from_secs(3) && busy.took < Duration::from_secs(3), "{quiet:?} {busy:?}");
    assert!(first.took < Duration::from_secs(10), "{first:?}");
}
