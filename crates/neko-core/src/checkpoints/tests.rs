use super::*;
use std::fs;

fn g(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git").current_dir(dir).args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
    assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// A repository with one commit, and a linked worktree "wt" on its own branch.
struct Fixture {
    _root: tempfile::TempDir,
    repo: PathBuf,
    wt: PathBuf,
    db: Mutex<Db>,
}

fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("repo");
    fs::create_dir(&repo).unwrap();
    g(&repo, &["init", "-q", "-b", "main"]);
    fs::write(repo.join(".gitignore"), "ignored.log\nbuild/\n").unwrap();
    fs::write(repo.join("a.txt"), "one\n").unwrap();
    fs::write(repo.join("keep.txt"), "keep\n").unwrap();
    g(&repo, &["add", "."]);
    g(&repo, &["commit", "-qm", "base"]);
    let wt = root.path().join("wt");
    g(&repo, &["worktree", "add", "-q", "-b", "codex/neko-t", wt.to_str().unwrap()]);
    Fixture { _root: root, repo, wt, db: Mutex::new(Db::open_in_memory().unwrap()) }
}

fn run(f: &Fixture, work: impl FnOnce(&Path)) -> u32 {
    let n = begin_run(&f.db, "t", &f.wt, 1_000).unwrap();
    work(&f.wt);
    end_run(&f.db, "t", n, &f.wt, Outcome::Done, 2_000).unwrap();
    n
}

fn undo_opts(run: u32) -> UndoOptions {
    UndoOptions { run, ..Default::default() }
}

fn git_dir(f: &Fixture) -> PathBuf {
    PathBuf::from(g(&f.wt, &["rev-parse", "--absolute-git-dir"]))
}

#[test]
fn a_snapshot_has_untracked_files_not_ignored_ones_and_leaves_the_index_and_head_alone() {
    let f = fixture();
    fs::write(f.wt.join("a.txt"), "two\n").unwrap();
    fs::write(f.wt.join("new.txt"), "new\n").unwrap();
    fs::write(f.wt.join("ignored.log"), "noise\n").unwrap();
    fs::create_dir(f.wt.join("build")).unwrap();
    fs::write(f.wt.join("build/out.o"), "obj\n").unwrap();
    fs::write(f.wt.join("keep.txt"), "staged edit\n").unwrap();
    g(&f.wt, &["add", "keep.txt"]); // something staged: it must stay as it is
    let head = g(&f.wt, &["rev-parse", "HEAD"]);
    let status = g(&f.wt, &["status", "--porcelain"]);
    let index_before = fs::read(git_dir(&f).join("index")).unwrap();
    let branches = g(&f.repo, &["for-each-ref", "refs/heads"]);

    let point = capture(&f.wt, "refs/neko/checkpoints/t/1-start", "x").unwrap();

    let files = g(&f.wt, &["ls-tree", "-r", "--name-only", &point.tree]);
    assert!(files.contains("new.txt"), "untracked included: {files}");
    assert!(!files.contains("ignored.log") && !files.contains("build/"), "ignored left out: {files}");
    assert_eq!(g(&f.wt, &["show", &format!("{}:a.txt", point.tree)]), "two");
    assert_eq!(g(&f.wt, &["rev-parse", "HEAD"]), head, "HEAD untouched");
    assert_eq!(g(&f.wt, &["status", "--porcelain"]), status, "status untouched");
    assert_eq!(fs::read(git_dir(&f).join("index")).unwrap(), index_before, "the index file is byte for byte the same");
    assert_eq!(g(&f.repo, &["for-each-ref", "refs/heads"]), branches, "no branch gained a commit");
    assert_eq!(g(&f.wt, &["rev-parse", "refs/neko/checkpoints/t/1-start"]), point.commit);
    assert!(fs::read_dir(git_dir(&f)).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with("neko-checkpoint-index")), "no scratch index left behind");
}

#[test]
fn a_plain_checkout_snapshots_too() {
    let f = fixture();
    fs::write(f.repo.join("direct.txt"), "d\n").unwrap();
    let point = capture(&f.repo, "refs/neko/checkpoints/p/1-start", "x").unwrap();
    assert!(g(&f.repo, &["ls-tree", "-r", "--name-only", &point.tree]).contains("direct.txt"));
}

#[test]
fn files_over_five_megabytes_are_skipped_and_recorded() {
    let f = fixture();
    fs::write(f.wt.join("big.bin"), vec![7u8; (MAX_FILE_BYTES + 1) as usize]).unwrap();
    fs::write(f.wt.join("small.txt"), "s\n").unwrap();
    let point = capture(&f.wt, "refs/neko/checkpoints/t/1-start", "x").unwrap();
    let files = g(&f.wt, &["ls-tree", "-r", "--name-only", &point.tree]);
    assert!(files.contains("small.txt") && !files.contains("big.bin"));
    assert_eq!(point.skipped_count, 1);
    assert_eq!(point.skipped[0].path, "big.bin");
    assert!(point.skipped[0].bytes > MAX_FILE_BYTES);
}

#[test]
fn a_run_has_its_own_diff_and_the_whole_task_has_one() {
    let f = fixture();
    run(&f, |d| fs::write(d.join("a.txt"), "one\ntwo\n").unwrap());
    run(&f, |d| {
        fs::write(d.join("b.txt"), "b\n").unwrap();
        fs::remove_file(d.join("keep.txt")).unwrap();
    });
    let first = diff_of(&f.db, "t", &f.wt, Some(1)).unwrap();
    assert_eq!(first.files, vec!["a.txt"]);
    assert_eq!((first.added, first.removed), (1, 0));
    assert!(first.patch.contains("+two"));
    let second = diff_of(&f.db, "t", &f.wt, Some(2)).unwrap();
    assert_eq!(second.files.len(), 2, "{:?}", second.files);
    assert!(!second.patch.contains("+two"), "run 2 does not show run 1's change");
    let whole = diff_of(&f.db, "t", &f.wt, None).unwrap();
    assert_eq!(whole.files.len(), 3);
    assert_eq!(row(&f.db, "t").unwrap().runs.len(), 2);
}

#[test]
fn undo_restores_the_run_start_and_redo_brings_it_back() {
    let f = fixture();
    let n = run(&f, |d| {
        fs::write(d.join("a.txt"), "changed\n").unwrap();
        fs::write(d.join("new.txt"), "new\n").unwrap();
        fs::remove_file(d.join("keep.txt")).unwrap();
        fs::create_dir(d.join("sub")).unwrap();
        fs::write(d.join("sub/deep.txt"), "deep\n").unwrap();
    });
    fs::write(f.wt.join("ignored.log"), "mine\n").unwrap();
    let done = undo(&f.db, "t", &f.wt, undo_opts(n), false, 3_000).unwrap();
    assert_eq!(done.runs, vec![n]);
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "one\n");
    assert_eq!(fs::read_to_string(f.wt.join("keep.txt")).unwrap(), "keep\n");
    assert!(!f.wt.join("new.txt").exists() && !f.wt.join("sub").exists(), "files and folders the run made are gone");
    assert_eq!(fs::read_to_string(f.wt.join("ignored.log")).unwrap(), "mine\n", "ignored files are never touched");
    assert!(g(&f.wt, &["status", "--porcelain"]).is_empty(), "the folder is clean again");
    assert!(row(&f.db, "t").unwrap().runs[0].undone);
    assert!(take_notice(&f.db, "t").unwrap().contains("undid run 1"));
    assert!(take_notice(&f.db, "t").is_none(), "the notice is told once");

    redo(&f.db, "t", &f.wt, false).unwrap();
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "changed\n");
    assert_eq!(fs::read_to_string(f.wt.join("sub/deep.txt")).unwrap(), "deep\n");
    assert!(!f.wt.join("keep.txt").exists());
    assert!(!row(&f.db, "t").unwrap().runs[0].undone);
    assert!(redo(&f.db, "t", &f.wt, false).is_err(), "only one redo");
}

#[test]
fn undo_refuses_when_later_runs_exist_and_undo_back_to_here_undoes_them() {
    let f = fixture();
    run(&f, |d| fs::write(d.join("a.txt"), "r1\n").unwrap());
    run(&f, |d| fs::write(d.join("a.txt"), "r2\n").unwrap());
    run(&f, |d| fs::write(d.join("b.txt"), "r3\n").unwrap());
    assert_eq!(undo(&f.db, "t", &f.wt, undo_opts(2), false, 3_000), Err(Refusal::LaterRuns(vec![3])));
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "r2\n", "a refusal changes nothing");
    let done = undo(&f.db, "t", &f.wt, UndoOptions { run: 2, through_later: true, ..Default::default() }, false, 3_000).unwrap();
    assert_eq!(done.runs, vec![2, 3]);
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "r1\n");
    assert!(!f.wt.join("b.txt").exists());
    // Run 1 is now the last live run.
    assert!(undo(&f.db, "t", &f.wt, undo_opts(1), false, 4_000).is_ok());
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "one\n");
}

#[test]
fn undo_refuses_on_edits_no_checkpoint_holds_unless_they_are_snapshotted_first() {
    let f = fixture();
    let n = run(&f, |d| fs::write(d.join("a.txt"), "run\n").unwrap());
    fs::write(f.wt.join("a.txt"), "mine after the run\n").unwrap();
    assert_eq!(undo(&f.db, "t", &f.wt, undo_opts(n), false, 3_000), Err(Refusal::Unsaved));
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "mine after the run\n");
    undo(&f.db, "t", &f.wt, UndoOptions { run: n, snapshot_unsaved: true, ..Default::default() }, false, 3_000).unwrap();
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "one\n");
    let r = row(&f.db, "t").unwrap();
    assert!(r.extras.iter().any(|e| e.kind == ExtraKind::Manual), "the edits were kept in a checkpoint");
    // And the undo's own safety checkpoint holds them, so a redo returns them.
    redo(&f.db, "t", &f.wt, false).unwrap();
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "mine after the run\n");
}

#[test]
fn undo_refuses_while_a_run_works_in_the_folder() {
    let f = fixture();
    let n = run(&f, |d| fs::write(d.join("a.txt"), "x\n").unwrap());
    assert_eq!(undo(&f.db, "t", &f.wt, undo_opts(n), true, 3_000), Err(Refusal::Busy));
    // A run with no end and no worker (the daemon stopped) can be undone like any other.
    let unfinished = begin_run(&f.db, "t", &f.wt, 5_000).unwrap();
    assert!(undo(&f.db, "t", &f.wt, undo_opts(unfinished), false, 6_000).is_ok());
}

#[test]
fn undo_never_overwrites_a_file_over_the_size_guard() {
    let f = fixture();
    let n = run(&f, |d| fs::write(d.join("a.txt"), "small\n").unwrap());
    let big = vec![9u8; (MAX_FILE_BYTES + 10) as usize];
    // After the run, a.txt grows past the guard: no checkpoint can hold it.
    fs::write(f.wt.join("a.txt"), &big).unwrap();
    let _ = undo(&f.db, "t", &f.wt, UndoOptions { run: n, snapshot_unsaved: true, ..Default::default() }, false, 3_000);
    assert_eq!(fs::read(f.wt.join("a.txt")).unwrap(), big, "a file Neko could not save is left as it is");
}

#[test]
fn a_failed_restore_puts_the_files_back() {
    let f = fixture();
    // Run 1 turns the file `a.txt` into a folder; an ignored file then lands in it.
    let n = run(&f, |d| {
        fs::remove_file(d.join("a.txt")).unwrap();
        fs::create_dir(d.join("a.txt")).unwrap();
        fs::write(d.join("a.txt/inner.txt"), "i\n").unwrap();
    });
    fs::write(f.wt.join("a.txt/inner.txt"), "mine\n").unwrap();
    let after = fs::read_to_string(f.wt.join("a.txt/inner.txt")).unwrap();
    let _ = undo(&f.db, "t", &f.wt, UndoOptions { run: n, snapshot_unsaved: true, ..Default::default() }, false, 3_000);
    // Whatever happened, nothing the person had was lost: either undone with a redo, or untouched.
    let kept = f.wt.join("a.txt/inner.txt").exists() || redo(&f.db, "t", &f.wt, false).is_ok();
    assert!(kept);
    assert_eq!(fs::read_to_string(f.wt.join("a.txt/inner.txt")).unwrap(), after);
}

fn with_remote(f: &Fixture) -> PathBuf {
    let bare = f.repo.parent().unwrap().join("origin.git");
    g(f.repo.parent().unwrap(), &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
    g(&f.repo, &["remote", "add", "origin", bare.to_str().unwrap()]);
    bare
}

fn commit_in_run(d: &Path) {
    fs::write(d.join("a.txt"), "committed\n").unwrap();
    g(d, &["commit", "-qam", "agent work"]);
}

#[test]
fn a_run_with_an_unpushed_commit_is_rewound_with_its_history() {
    let f = fixture();
    let base = g(&f.wt, &["rev-parse", "HEAD"]);
    let n = run(&f, commit_in_run);
    undo(&f.db, "t", &f.wt, undo_opts(n), false, 3_000).unwrap();
    assert_eq!(g(&f.wt, &["rev-parse", "HEAD"]), base, "the branch is back where the run started");
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "one\n");
    assert!(g(&f.wt, &["status", "--porcelain"]).is_empty());
    redo(&f.db, "t", &f.wt, false).unwrap();
    assert_ne!(g(&f.wt, &["rev-parse", "HEAD"]), base, "redo moves the branch forward again");
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "committed\n");
}

#[test]
fn undo_refuses_when_the_runs_commit_was_pushed_and_offers_a_revert_commit() {
    let f = fixture();
    let _bare = with_remote(&f);
    let n = run(&f, commit_in_run);
    g(&f.wt, &["push", "-q", "origin", "codex/neko-t"]);
    let refusal = undo(&f.db, "t", &f.wt, undo_opts(n), false, 3_000).unwrap_err();
    assert_eq!(refusal.code(), "pushed", "{refusal}");
    assert!(refusal.message().contains("revert commit"));
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "committed\n");

    let pushed_head = g(&f.wt, &["rev-parse", "HEAD"]);
    let done = undo(&f.db, "t", &f.wt, UndoOptions { run: n, revert: true, ..Default::default() }, false, 3_000).unwrap();
    assert!(done.reverted_by_commit);
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "one\n");
    assert_eq!(g(&f.wt, &["rev-parse", "HEAD~1"]), pushed_head, "history is kept; the revert is a new commit");
    assert!(g(&f.wt, &["status", "--porcelain"]).is_empty());
    assert!(g(&f.wt, &["log", "-1", "--format=%s"]).contains("Revert run 1"));
}

#[test]
fn a_revert_of_a_run_that_committed_nothing_adds_no_commit() {
    let f = fixture();
    let head = g(&f.wt, &["rev-parse", "HEAD"]);
    let n = run(&f, |d| fs::write(d.join("a.txt"), "loose\n").unwrap());
    let done = undo(&f.db, "t", &f.wt, UndoOptions { run: n, revert: true, ..Default::default() }, false, 3_000).unwrap();
    assert!(!done.reverted_by_commit);
    assert_eq!(g(&f.wt, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "one\n");
}

#[test]
fn a_commit_in_an_open_pull_request_also_refuses() {
    let f = fixture();
    let n = run(&f, commit_in_run);
    let refusal = undo(&f.db, "t", &f.wt, UndoOptions { run: n, pull_request_open: true, ..Default::default() }, false, 3_000).unwrap_err();
    assert_eq!(refusal.code(), "pushed");
    assert!(refusal.message().contains("pull request"));
    // A run that changed no commits is fine even with a pull request open.
    let n2 = run(&f, |d| fs::write(d.join("loose.txt"), "l\n").unwrap());
    assert!(undo(&f.db, "t", &f.wt, UndoOptions { run: n2, pull_request_open: true, ..Default::default() }, false, 4_000).is_ok());
}

#[test]
fn checkpoint_refs_are_never_pushed() {
    let f = fixture();
    let bare = with_remote(&f);
    run(&f, commit_in_run);
    assert!(!refs_of(&f.wt, "t").is_empty());
    g(&f.wt, &["push", "-q", "origin", "codex/neko-t"]);
    g(&f.repo, &["push", "-q", "origin", "main"]);
    g(&f.wt, &["push", "-q", "--all", "origin"]);
    g(&f.wt, &["push", "-q", "--tags", "origin"]);
    let remote_refs = g(&bare, &["for-each-ref", "--format=%(refname)"]);
    assert!(remote_refs.contains("refs/heads/codex/neko-t"));
    assert!(!remote_refs.contains("refs/neko"), "{remote_refs}");
}

#[test]
fn checkpoints_are_deleted_with_the_ticket_and_after_fourteen_days() {
    let f = fixture();
    run(&f, |d| fs::write(d.join("a.txt"), "x\n").unwrap());
    assert_eq!(refs_of(&f.wt, "t").len(), 2);
    assert_eq!(tasks_with_checkpoints(&f.db), vec!["t".to_owned()]);
    // Not yet old.
    assert_eq!(expire(&f.db, 2_000 + KEEP_MS - 1), 0);
    assert_eq!(refs_of(&f.wt, "t").len(), 2);
    assert_eq!(expire(&f.db, 2_000 + KEEP_MS + 1), 2);
    assert!(refs_of(&f.wt, "t").is_empty());
    assert!(tasks_with_checkpoints(&f.db).is_empty(), "an emptied row leaves the index");

    run(&f, |d| fs::write(d.join("a.txt"), "y\n").unwrap());
    assert_eq!(refs_of(&f.wt, "t").len(), 2);
    delete_task(&f.db, "t", Some(&f.wt)).unwrap();
    assert!(refs_of(&f.wt, "t").is_empty());
    assert!(row(&f.db, "t").unwrap().runs.is_empty());
}

#[test]
fn a_ticket_keeps_at_most_fifty_checkpoints() {
    let f = fixture();
    for i in 0..30 {
        run(&f, |d| fs::write(d.join("a.txt"), format!("{i}\n")).unwrap());
    }
    let refs = refs_of(&f.wt, "t");
    assert!(refs.len() <= MAX_PER_TASK, "{}", refs.len());
    let r = row(&f.db, "t").unwrap();
    assert_eq!(r.runs.first().unwrap().n, 1, "the first run stays for the whole-task diff");
    assert_eq!(r.runs.last().unwrap().n, 30);
    assert_eq!(ref_count(&r), refs.len());
}

fn persons_commit(d: &Path, file: &str, body: &str) {
    fs::write(d.join(file), body).unwrap();
    g(d, &["add", file]);
    g(d, &["commit", "-qm", &format!("person edits {file}")]);
}

#[test]
fn undo_refuses_when_commits_were_made_outside_any_run_and_names_them() {
    let f = fixture();
    let n = run(&f, commit_in_run);
    persons_commit(&f.wt, "mine.txt", "mine\n");
    let head = g(&f.wt, &["rev-parse", "HEAD"]);
    let refusal = undo(&f.db, "t", &f.wt, undo_opts(n), false, 3_000).unwrap_err();
    assert_eq!(refusal.code(), "foreign_commits", "{refusal}");
    assert!(refusal.message().contains("person edits mine.txt"), "{refusal}");
    assert_eq!(g(&f.wt, &["rev-parse", "HEAD"]), head, "nothing was rewound");
    assert_eq!(fs::read_to_string(f.wt.join("mine.txt")).unwrap(), "mine\n");
    // "Undo back to here" refuses the same way.
    run(&f, |d| fs::write(d.join("later.txt"), "x\n").unwrap());
    let back = undo(&f.db, "t", &f.wt, UndoOptions { run: n, through_later: true, ..Default::default() }, false, 4_000).unwrap_err();
    assert_eq!(back.code(), "foreign_commits");
}

#[test]
fn a_revert_after_foreign_commits_undoes_only_the_runs_own_changes() {
    let f = fixture();
    let n = run(&f, commit_in_run);
    persons_commit(&f.wt, "mine.txt", "mine\n");
    let done = undo(&f.db, "t", &f.wt, UndoOptions { run: n, revert: true, ..Default::default() }, false, 3_000).unwrap();
    assert!(done.reverted_by_commit);
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "one\n", "the run's change is gone");
    assert_eq!(fs::read_to_string(f.wt.join("mine.txt")).unwrap(), "mine\n", "the person's commit stays");
    assert!(g(&f.wt, &["log", "--format=%s"]).contains("person edits mine.txt"));
    assert!(g(&f.wt, &["log", "-1", "--format=%s"]).contains("Revert run 1"));
    assert!(g(&f.wt, &["status", "--porcelain"]).is_empty());
}

#[test]
fn a_revert_that_conflicts_with_a_foreign_commit_refuses_and_changes_nothing() {
    let f = fixture();
    let n = run(&f, commit_in_run);
    persons_commit(&f.wt, "a.txt", "person rewrote it\n");
    let head = g(&f.wt, &["rev-parse", "HEAD"]);
    let refusal = undo(&f.db, "t", &f.wt, UndoOptions { run: n, revert: true, ..Default::default() }, false, 3_000).unwrap_err();
    assert_eq!(refusal.code(), "failed", "{refusal}");
    assert!(refusal.message().contains("person edits a.txt"), "{refusal}");
    assert_eq!(g(&f.wt, &["rev-parse", "HEAD"]), head);
    assert_eq!(fs::read_to_string(f.wt.join("a.txt")).unwrap(), "person rewrote it\n");
}

#[test]
fn commits_made_by_runs_are_not_foreign() {
    let f = fixture();
    let first = run(&f, commit_in_run);
    run(&f, |d| {
        fs::write(d.join("b.txt"), "b\n").unwrap();
        g(d, &["add", "b.txt"]);
        g(d, &["commit", "-qm", "second agent commit"]);
    });
    assert!(undo(&f.db, "t", &f.wt, UndoOptions { run: first, through_later: true, ..Default::default() }, false, 3_000).is_ok());
}

#[test]
fn redo_checks_pushed_history_like_undo_does() {
    let f = fixture();
    let _bare = with_remote(&f);
    let n = run(&f, commit_in_run);
    g(&f.wt, &["push", "-q", "origin", "codex/neko-t"]);
    undo(&f.db, "t", &f.wt, UndoOptions { run: n, revert: true, ..Default::default() }, false, 3_000).unwrap();
    // Neko's own revert commit is now pushed: redo would move the branch back over it.
    g(&f.wt, &["push", "-q", "origin", "codex/neko-t"]);
    let refusal = redo(&f.db, "t", &f.wt, false).unwrap_err();
    assert_eq!(refusal.code(), "pushed", "{refusal}");

    // Unpushed, Neko's own revert commit may be dropped by a redo.
    let f = fixture();
    let _bare = with_remote(&f);
    let n = run(&f, commit_in_run);
    g(&f.wt, &["push", "-q", "origin", "codex/neko-t"]);
    undo(&f.db, "t", &f.wt, UndoOptions { run: n, revert: true, ..Default::default() }, false, 3_000).unwrap();
    assert!(redo(&f.db, "t", &f.wt, false).is_ok());

    // A commit of the person's made after the undo is never dropped.
    let f = fixture();
    let n = run(&f, commit_in_run);
    undo(&f.db, "t", &f.wt, undo_opts(n), false, 3_000).unwrap();
    persons_commit(&f.wt, "mine.txt", "m\n");
    let refusal = redo(&f.db, "t", &f.wt, false).unwrap_err();
    assert!(["failed", "unsaved_changes", "foreign_commits"].contains(&refusal.code()), "{refusal}");
    assert_eq!(fs::read_to_string(f.wt.join("mine.txt")).unwrap(), "m\n");
}
