//! Parking: a task's worktree exists only while an agent works in it.
//!
//! A worktree per ticket, kept forever, is what filled this Mac's disk:
//! every checkout carries its own build output and dependencies, and none was
//! ever removed. Parking moves a stopped task's work onto its own branch in
//! the shared repository (which costs a few objects) and removes the folder;
//! restoring recreates the folder at the same path from that branch, so a
//! saved agent session that remembers its directory still resumes.
//!
//! Nothing is lost that Git would keep: committed work stays on the branch,
//! and uncommitted work (tracked edits and non-ignored new files) is saved as
//! one marked commit that restoring takes back off, leaving the same changes
//! uncommitted again. Ignored files (dependencies, caches, build output) are
//! dropped; they are regenerated, and dependencies are re-seeded as
//! copy-on-write clones when the folder comes back.

use super::*;

/// The subject of the commit that holds uncommitted work while parked.
/// Restoring takes it off only when the branch tip still carries it.
pub const PARK_SUBJECT: &str = "neko: parked work in progress";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parked {
    /// The task's branch, which now holds all of its work.
    pub branch: String,
    /// True when uncommitted work was saved as a `PARK_SUBJECT` commit.
    pub wip: bool,
}

/// Saves a stopped task's work on its branch and removes its folder. Refuses
/// rather than guessing when Git is mid-operation (merge, rebase, cherry-pick)
/// or anything is left uncommitted after saving.
pub fn park_worktree(worktree: &Path, cancel: &AtomicBool) -> Result<Parked, String> {
    let git_dir = PathBuf::from(git_output(worktree, &["rev-parse", "--path-format=absolute", "--absolute-git-dir"], cancel)?.trim());
    for marker in ["MERGE_HEAD", "CHERRY_PICK_HEAD", "REVERT_HEAD", "rebase-merge", "rebase-apply", "index.lock"] {
        if git_dir.join(marker).exists() {
            return Err(format!("Not parked: Git is in the middle of an operation ({marker})"));
        }
    }
    let source = publish::source_checkout(worktree, cancel)?;
    let branch = match git_output(worktree, &["symbolic-ref", "--quiet", "--short", "HEAD"], cancel) {
        Ok(name) => name.trim().to_owned(),
        Err(_) => {
            // Detached work still needs a branch to live on.
            let name = format!(
                "codex/neko-{}",
                worktree.file_name().and_then(|n| n.to_str()).ok_or("Unnamed worktree")?
            );
            git_output(worktree, &["switch", "--quiet", "-c", &name], cancel)?;
            name
        }
    };
    let dirty = !git_output(worktree, &["status", "--porcelain", "-z", "--untracked-files=all"], cancel)?.is_empty();
    if dirty {
        git_output(worktree, &["add", "-A"], cancel)?;
        git_output(
            worktree,
            &[
                "-c", "user.name=Neko", "-c", "user.email=neko@localhost",
                "-c", "commit.gpgsign=false",
                "commit", "--quiet", "--no-verify", "-m", PARK_SUBJECT,
            ],
            cancel,
        )?;
        if !git_output(worktree, &["status", "--porcelain", "-z", "--untracked-files=all"], cancel)?.is_empty() {
            return Err("Not parked: some changes could not be saved on the branch".into());
        }
    }
    git_output(&source, &["worktree", "remove", "--force", &worktree.to_string_lossy()], cancel)?;
    // The folder can outlive `worktree remove` when it holds files Git does
    // not own (an unpopulated submodule's contents, say); it is ignored output.
    if worktree.exists() {
        fs::remove_dir_all(worktree).map_err(|e| format!("Parked, but the folder could not be removed: {e}"))?;
    }
    let _ = git_output(&source, &["worktree", "prune"], cancel);
    Ok(Parked { branch, wip: dirty })
}

/// Recreates a parked task's folder at `path` from its branch, takes the
/// saved uncommitted work back off so it is uncommitted again, and re-seeds
/// dependencies. `repository` is the source checkout the branch lives in.
pub fn restore_worktree(repository: &Path, path: &Path, parked: &Parked, cancel: &AtomicBool) -> Result<(), String> {
    if path.exists() {
        return Err(format!("Cannot restore: {} already exists", path.display()));
    }
    let repository = repository.canonicalize().map_err(|e| format!("Repository unavailable: {e}"))?;
    git_output(&repository, &["rev-parse", "--verify", &format!("refs/heads/{}^{{commit}}", parked.branch)], cancel)
        .map_err(|_| format!("Cannot restore: branch {} is gone", parked.branch))?;
    let checkout = checkout_lock(&repository, cancel)?;
    let guard = wait_for_checkout(&checkout, cancel)?;
    let _ = git_output(&repository, &["worktree", "prune"], cancel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create task worktree directory: {e}"))?;
    }
    let mut add = filterless_checkout(&repository, cancel)?;
    add.args(["worktree", "add"]).arg(path).arg(&parked.branch);
    let output = execute(add, &[], cancel, Duration::from_secs(120), |_| Ok(()))?;
    if !output.status.success() {
        return Err(format!("Restoring the worktree failed: {}", bounded_text(&output.stderr, MAX_EVENT)));
    }
    drop(guard);
    if parked.wip {
        let subject = git_output(path, &["log", "-1", "--format=%s"], cancel)?;
        if subject.trim() == PARK_SUBJECT {
            git_output(path, &["reset", "--quiet", "--mixed", "HEAD~1"], cancel)?;
        }
    }
    seed_dependency_folders(&repository, path, cancel);
    Ok(())
}

/// How `recreate_missing_worktree` brought a folder back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Recreated {
    /// Checked out from the task's own branch: every commit (and parked
    /// work) on it is back.
    FromBranch(String),
    /// The branch was gone, so a new one was started from this commit.
    FromBase { branch: String, start: String },
}

/// Recreates a task folder that disappeared without being parked (deleted by
/// hand, by an older Neko, or by a cleanup tool), at the same path so a saved
/// agent session still resumes. Uses the task's own `codex/neko-<folder>`
/// branch when it exists, otherwise starts that branch from `base` (the
/// commit the task began at) or the repository's HEAD. Never touches an
/// existing folder.
pub fn recreate_missing_worktree(repository: &Path, path: &Path, base: Option<&str>, cancel: &AtomicBool) -> Result<Recreated, String> {
    if path.exists() {
        return Err(format!("Cannot recreate: {} already exists", path.display()));
    }
    let name = path.file_name().and_then(|n| n.to_str()).ok_or("Unnamed worktree")?;
    let branch = format!("codex/neko-{name}");
    let repository = repository.canonicalize().map_err(|e| format!("Repository unavailable: {e}"))?;
    let checkout = checkout_lock(&repository, cancel)?;
    let guard = wait_for_checkout(&checkout, cancel)?;
    // Forget the missing folder's registration so its branch can be checked out again.
    let _ = git_output(&repository, &["worktree", "prune"], cancel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("Cannot create task worktree directory: {e}"))?;
    }
    let has_branch = git_output(&repository, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}^{{commit}}")], cancel).is_ok();
    let mut add = filterless_checkout(&repository, cancel)?;
    let recreated = if has_branch {
        add.args(["worktree", "add"]).arg(path).arg(&branch);
        Recreated::FromBranch(branch.clone())
    } else {
        let start = base
            .filter(|base| git_output(&repository, &["rev-parse", "--verify", "--quiet", &format!("{base}^{{commit}}")], cancel).is_ok())
            .unwrap_or("HEAD")
            .to_owned();
        add.args(["worktree", "add", "-b", &branch]).arg(path).arg(&start);
        Recreated::FromBase { branch: branch.clone(), start }
    };
    let output = execute(add, &[], cancel, Duration::from_secs(120), |_| Ok(()))?;
    if !output.status.success() {
        return Err(format!("Recreating the missing worktree failed: {}", bounded_text(&output.stderr, MAX_EVENT)));
    }
    drop(guard);
    seed_dependency_folders(&repository, path, cancel);
    Ok(recreated)
}

/// What a parked task changed since `base`, read from its branch in the
/// source checkout without restoring the folder.
pub fn parked_changes(repository: &Path, base: &str, branch: &str, cancel: &AtomicBool) -> Result<(Vec<String>, Vec<u8>), String> {
    let range = format!("{base}..refs/heads/{branch}");
    let files = git_output(repository, &["diff", "--name-only", "--no-ext-diff", "--no-textconv", "-z", &range, "--"], cancel)?
        .split('\0').filter(|s| !s.is_empty()).map(str::to_owned).collect();
    let patch = git_output(repository, &["diff", "--binary", "--no-ext-diff", "--no-textconv", &range, "--"], cancel)?.into_bytes();
    Ok((files, patch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) {
        let out = git_command(repo).args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join(".gitignore"), "node_modules/\n").unwrap();
        fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "base"]);
        let worktree = create_worktree_in(&repo, "t1", &dir.path().join("wt")).unwrap();
        (dir, repo, worktree)
    }

    #[test]
    fn parking_saves_uncommitted_work_and_restoring_gives_it_back_uncommitted() {
        let (_dir, repo, worktree) = fixture();
        let cancel = AtomicBool::new(false);
        let base = head(&worktree, &cancel).unwrap();
        fs::write(worktree.join("a.txt"), "edited\n").unwrap();
        fs::write(worktree.join("new.txt"), "brand new\n").unwrap();
        fs::create_dir(worktree.join("node_modules")).unwrap();
        fs::write(worktree.join("node_modules/dep.js"), "cache\n").unwrap();

        let parked = park_worktree(&worktree, &cancel).unwrap();
        assert!(parked.wip);
        assert_eq!(parked.branch, "codex/neko-t1");
        assert!(!worktree.exists(), "the folder is gone while parked");

        let (files, patch) = parked_changes(&repo, &base, &parked.branch, &cancel).unwrap();
        assert_eq!(files, vec!["a.txt".to_owned(), "new.txt".to_owned()]);
        assert!(String::from_utf8_lossy(&patch).contains("+edited"));

        restore_worktree(&repo, &worktree, &parked, &cancel).unwrap();
        assert_eq!(fs::read_to_string(worktree.join("a.txt")).unwrap(), "edited\n");
        assert_eq!(fs::read_to_string(worktree.join("new.txt")).unwrap(), "brand new\n");
        assert_eq!(head(&worktree, &cancel).unwrap(), base, "the saved work is uncommitted again");
        assert!(!worktree.join("node_modules/dep.js").exists(), "ignored output is not carried");
    }

    #[test]
    fn parking_a_clean_task_keeps_its_commits_and_adds_none() {
        let (_dir, repo, worktree) = fixture();
        let cancel = AtomicBool::new(false);
        fs::write(worktree.join("a.txt"), "committed\n").unwrap();
        git(&worktree, &["commit", "-qam", "real work"]);
        let tip = head(&worktree, &cancel).unwrap();
        let parked = park_worktree(&worktree, &cancel).unwrap();
        assert!(!parked.wip);
        restore_worktree(&repo, &worktree, &parked, &cancel).unwrap();
        assert_eq!(head(&worktree, &cancel).unwrap(), tip, "a real commit is never taken off");
    }

    #[test]
    fn a_missing_worktree_comes_back_from_its_branch_or_else_its_base() {
        let (_dir, repo, worktree) = fixture();
        let cancel = AtomicBool::new(false);
        let base = head(&worktree, &cancel).unwrap();
        fs::write(worktree.join("a.txt"), "committed work\n").unwrap();
        git(&worktree, &["commit", "-qam", "real work"]);
        let tip = head(&worktree, &cancel).unwrap();
        // Deleted behind Git's back: the registration is stale.
        fs::remove_dir_all(&worktree).unwrap();
        assert_eq!(recreate_missing_worktree(&repo, &worktree, Some(&base), &cancel).unwrap(), Recreated::FromBranch("codex/neko-t1".into()));
        assert_eq!(head(&worktree, &cancel).unwrap(), tip, "every commit on the branch is back");
        assert!(recreate_missing_worktree(&repo, &worktree, Some(&base), &cancel).is_err(), "an existing folder is never touched");

        // Branch gone too: start again from where the task began.
        fs::remove_dir_all(&worktree).unwrap();
        git(&repo, &["worktree", "prune"]);
        git(&repo, &["branch", "-D", "codex/neko-t1"]);
        assert_eq!(recreate_missing_worktree(&repo, &worktree, Some(&base), &cancel).unwrap(), Recreated::FromBase { branch: "codex/neko-t1".into(), start: base.clone() });
        assert_eq!(head(&worktree, &cancel).unwrap(), base);
    }

    #[test]
    fn parking_refuses_while_git_is_mid_operation() {
        let (_dir, _repo, worktree) = fixture();
        let cancel = AtomicBool::new(false);
        let git_dir = PathBuf::from(git_output(&worktree, &["rev-parse", "--absolute-git-dir"], &cancel).unwrap().trim());
        fs::write(git_dir.join("MERGE_HEAD"), "0000000000000000000000000000000000000000\n").unwrap();
        assert!(park_worktree(&worktree, &cancel).unwrap_err().contains("middle of an operation"));
        assert!(worktree.exists(), "nothing is removed when parking refuses");
    }
}
