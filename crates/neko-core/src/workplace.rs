//! Where a task works (docs/knowledge/runtimes.md, "Where it works").
//!
//! - No folder chosen: a fresh scratch folder of its own, under the data folder.
//! - A folder in a git repository: its own worktree on its own branch, `neko/<short id>`, from the commit the checkout is on, or from
//!   the branch the person chose ([`Placement::from_branch`]). The person's checkout is never touched; nothing is merged or pushed.
//! - A folder that is not a repository, or one the person chose to work in itself ([`Placement::in_folder`]): the task works there
//!   directly, and its first edit asks.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use neko_protocol::{Changes, TaskId, files};

use crate::find;
use crate::records::Place;

/// Why a task's folder could not be prepared.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlaceError {
    /// The chosen folder is not there.
    Gone(String),
    /// Something else, in words.
    Other(String),
}

/// What the person chose about where a thread works, beyond its folder (docs/knowledge/threads.md, "New thread").
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Placement {
    /// Work in the project's folder itself, even a repository, not in a worktree of its own.
    pub in_folder: bool,
    /// The branch a worktree starts from (a local branch of the repository); None: the commit the checkout is on.
    pub from_branch: Option<String>,
}

/// Whether `name` can be a branch name Neko hands to git: not empty, no option-like start, none of git's forbidden characters.
pub fn branch_name_ok(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with(['-', '/', '.'])
        && !name.ends_with(['/', '.'])
        && !name.contains("..")
        && !name.contains("@{")
        && !name.ends_with(".lock")
        && !name.chars().any(|c| c.is_control() || matches!(c, ' ' | '~' | '^' | ':' | '?' | '*' | '[' | '\\'))
}

/// Where a new thread in a folder can start from: whether it is a repository with a commit, the branch the checkout is on, and the
/// local branches (the current one first, then by name).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Branches {
    pub git: bool,
    pub current: Option<String>,
    pub names: Vec<String>,
}

pub fn branches(folder: &Path) -> Branches {
    if !folder.is_dir() || git_out(folder, &["rev-parse", "--verify", "HEAD"]).is_none() {
        return Branches::default();
    }
    let current = git_out(folder, &["symbolic-ref", "--quiet", "--short", "HEAD"]).filter(|b| !b.is_empty());
    let mut names: Vec<String> = git_out(folder, &["for-each-ref", "--format=%(refname:short)", "--sort=refname", "refs/heads/"]).map(|o| o.lines().map(str::to_owned).filter(|l| !l.is_empty() && branch_name_ok(l)).collect()).unwrap_or_default();
    if let Some(c) = &current
        && let Some(at) = names.iter().position(|n| n == c)
    {
        let c = names.remove(at);
        names.insert(0, c);
    }
    Branches { git: true, current, names }
}

/// The part of a task's id that names its branch and folders: its last eight characters, lower case (the random part).
pub fn short_id(task: &TaskId) -> String {
    let id = task.0.as_str();
    id[id.len().saturating_sub(8)..].to_lowercase()
}

/// A git command in `dir`, never prompting.
fn git(dir: &Path) -> Command {
    let mut c = Command::new(find::find("git", None).unwrap_or_else(|| PathBuf::from("/usr/bin/git")));
    c.arg("-C").arg(dir).env("GIT_TERMINAL_PROMPT", "0").stdin(Stdio::null()).stderr(Stdio::null());
    c
}

/// Runs git and returns its trimmed output when it succeeds.
fn git_out(dir: &Path, args: &[&str]) -> Option<String> {
    let mut c = git(dir);
    c.args(args);
    let out = find::run_for(c, Duration::from_secs(60))?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
}

/// Makes (or finds again) the place a task works in.
pub fn prepare(data_dir: &Path, task: &TaskId, folder: Option<&str>, how: &Placement) -> Result<Place, PlaceError> {
    let short = short_id(task);
    let Some(folder) = folder else {
        let dir = data_dir.join(files::SCRATCH).join(&short);
        std::fs::create_dir_all(&dir).map_err(|e| PlaceError::Other(format!("Neko could not make a folder for this thread: {e}")))?;
        return Ok(Place { workdir: dir.to_string_lossy().into_owned(), ..Place::default() });
    };
    let chosen = Path::new(folder);
    if !chosen.is_dir() {
        return Err(PlaceError::Gone(folder.to_owned()));
    }
    if how.in_folder {
        // The person chose the folder itself, repository or not: it works there directly.
        return Ok(Place { workdir: folder.to_owned(), plain: true, ..Place::default() });
    }
    let root = git_out(chosen, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    // The commit the worktree starts from: the chosen branch's tip, else the commit the checkout is on.
    let start = how.from_branch.as_deref().filter(|b| branch_name_ok(b)).map_or_else(|| "HEAD".to_owned(), |b| format!("refs/heads/{b}^{{commit}}"));
    let base = root.as_ref().and_then(|_| git_out(chosen, &["rev-parse", "--verify", &start]));
    if root.is_some() && base.is_none() && how.from_branch.is_some() && git_out(chosen, &["rev-parse", "--verify", "HEAD"]).is_some() {
        return Err(PlaceError::Other(format!("Neko could not find the branch {} in {folder}.", how.from_branch.as_deref().unwrap_or_default())));
    }
    let (Some(root), Some(base)) = (root, base) else {
        // Not a repository (or one with no commit yet): it works there directly.
        return Ok(Place { workdir: folder.to_owned(), plain: true, ..Place::default() });
    };
    let branch = format!("neko/{short}");
    let tree = data_dir.join(files::WORKTREES).join(&short);
    if !tree.join(".git").exists() {
        if let Some(parent) = tree.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PlaceError::Other(e.to_string()))?;
        }
        let t = tree.to_string_lossy().into_owned();
        let made = git_out(&root, &["worktree", "add", "-b", &branch, &t, &base]).is_some() || git_out(&root, &["worktree", "add", &t, &branch]).is_some();
        if !made {
            return Err(PlaceError::Other(format!("Neko could not make a git worktree for {folder}.")));
        }
    }
    // The same place inside the worktree as the folder chosen inside the repository.
    let canon_root = root.canonicalize().unwrap_or(root.clone());
    let canon_chosen = chosen.canonicalize().unwrap_or(chosen.to_path_buf());
    let inner = canon_chosen.strip_prefix(&canon_root).map(Path::to_path_buf).unwrap_or_default();
    let workdir = tree.join(inner);
    Ok(Place { workdir: workdir.to_string_lossy().into_owned(), branch: Some(branch), base_commit: Some(base), repo: Some(root.to_string_lossy().into_owned()), plain: false })
}

/// The top folder of the repository `dir` is in (a worktree's own top folder inside one), when it has a commit: the only place a
/// subagent may be given a copy of.
pub fn copy_source(dir: &Path) -> Option<PathBuf> {
    if !dir.is_dir() || git_out(dir, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"]).is_none() {
        return None;
    }
    git_out(dir, &["rev-parse", "--show-toplevel"]).filter(|t| !t.is_empty()).map(PathBuf::from)
}

/// The commit an own copy starts from: what `root` (a repository's top folder or a worktree's) is now. When nothing there is uncommitted
/// that is its HEAD; otherwise a snapshot commit (HEAD plus every uncommitted and new file) that the copy's branch then holds, so
/// the copy starts from the folder as it is, as an agent would want. Nothing in `root` is touched (docs/knowledge/undo.md, "A checkpoint").
pub fn copy_base(data_dir: &Path, root: &Path, limits: &crate::checkpoint::Limits) -> Result<String, String> {
    let f = crate::checkpoint::Folder { kind: crate::checkpoint::Kind::Repo, work: root.to_path_buf(), shadow: None };
    let snap = crate::checkpoint::take(&f, &crate::checkpoint::tmp_dir(data_dir), "Neko: the start of an own copy", limits).map_err(|e| e.to_string())?;
    let head_tree = git_out(root, &["rev-parse", "--verify", "HEAD^{tree}"]);
    match (&snap.head, head_tree) {
        (Some(head), Some(t)) if t == snap.tree => Ok(head.clone()),
        _ => Ok(snap.commit),
    }
}

/// Makes an own copy: a worktree on `branch` from `base`, in the folder Neko keeps for `kitten`. `source` is the folder it copies (it
/// may be inside the repository; the copy's working folder is the same place inside the new tree), `repo` the person's repository
/// when `source` is one of Neko's worktrees of it.
pub fn prepare_copy(data_dir: &Path, kitten: &TaskId, branch: &str, source: &Path, base: &str, repo: Option<&str>) -> Result<Place, PlaceError> {
    if !branch_name_ok(branch) {
        return Err(PlaceError::Other("Neko could not name a branch for this copy.".into()));
    }
    let Some(root) = copy_source(source) else { return Err(PlaceError::Other("This folder is not a git repository with a commit, so it cannot be copied.".into())) };
    let tree = worktree_root(data_dir, kitten);
    if !tree.join(".git").exists() {
        if let Some(parent) = tree.parent() {
            std::fs::create_dir_all(parent).map_err(|e| PlaceError::Other(e.to_string()))?;
        }
        let t = tree.to_string_lossy().into_owned();
        let made = git_out(&root, &["worktree", "add", "-b", branch, &t, base]).is_some() || git_out(&root, &["worktree", "add", &t, branch]).is_some();
        if !made {
            return Err(PlaceError::Other("Neko could not make a copy of the project for this subagent.".into()));
        }
    }
    let canon_root = root.canonicalize().unwrap_or(root.clone());
    let canon_source = source.canonicalize().unwrap_or(source.to_path_buf());
    let inner = canon_source.strip_prefix(&canon_root).map(Path::to_path_buf).unwrap_or_default();
    Ok(Place { workdir: tree.join(inner).to_string_lossy().into_owned(), branch: Some(branch.to_owned()), base_commit: Some(base.to_owned()), repo: Some(repo.map_or_else(|| root.to_string_lossy().into_owned(), str::to_owned)), plain: false })
}

/// Whether the local branch `branch` exists in `repo`.
pub fn branch_exists(repo: &Path, branch: &str) -> bool {
    branch_name_ok(branch) && git_out(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_some()
}

/// The folder of a task's worktree (the place's working folder may be inside it).
pub fn worktree_root(data_dir: &Path, task: &TaskId) -> PathBuf {
    data_dir.join(files::WORKTREES).join(short_id(task))
}

/// The folder a task works in again, when it was tidied away after its last run: its worktree comes back on its own branch (or
/// a new one from where it began when the branch went too), a scratch folder is made again. A plain folder that is gone stays
/// gone.
pub fn reopen(data_dir: &Path, task: &TaskId, p: &Place) -> Result<(), PlaceError> {
    if Path::new(&p.workdir).is_dir() {
        return Ok(());
    }
    match (&p.branch, &p.repo, &p.base_commit) {
        (Some(branch), Some(repo), Some(base)) => {
            let repo = Path::new(repo);
            if !repo.is_dir() {
                return Err(PlaceError::Gone(repo.to_string_lossy().into_owned()));
            }
            let tree = worktree_root(data_dir, task);
            let t = tree.to_string_lossy().into_owned();
            let _ = git_out(repo, &["worktree", "prune"]);
            let made = git_out(repo, &["worktree", "add", &t, branch]).is_some() || git_out(repo, &["worktree", "add", "-b", branch, &t, base]).is_some();
            if !made || !Path::new(&p.workdir).is_dir() {
                return Err(PlaceError::Other(format!("Neko could not make the git worktree for {} again.", repo.display())));
            }
            Ok(())
        }
        _ if p.plain => Err(PlaceError::Gone(p.workdir.clone())),
        _ => std::fs::create_dir_all(&p.workdir).map_err(|e| PlaceError::Other(format!("Neko could not make a folder for this thread: {e}"))),
    }
}

/// After a repository moved (Locate…): its worktrees find it again (`git worktree repair`).
pub fn repair(repo: &Path, trees: &[PathBuf]) {
    let mut args = vec!["worktree".to_owned(), "repair".to_owned()];
    args.extend(trees.iter().filter(|t| t.exists()).map(|t| t.to_string_lossy().into_owned()));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let _ = git_out(repo, &args);
}

/// What a task's worktree holds, from git. None when git could not say (then nothing is deleted).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TreeState {
    /// Changes not committed: modified, staged or new files (ignored files do not count).
    pub uncommitted: bool,
    /// Commits on its branch that the commit it began from lacks.
    pub commits: u32,
    /// Its checkout is still on the task's own branch.
    pub on_branch: bool,
}

/// How the worktree at `tree` stands against `base`.
pub fn tree_state(tree: &Path, branch: &str, base: &str) -> Option<TreeState> {
    let mut status = git(tree);
    status.args(["status", "--porcelain", "--untracked-files=all"]);
    let out = find::run_for(status, Duration::from_secs(60))?;
    if !out.status.success() {
        return None;
    }
    let uncommitted = !out.stdout.iter().all(u8::is_ascii_whitespace);
    let commits = git_out(tree, &["rev-list", "--count", &format!("{base}..HEAD")])?.parse().ok()?;
    let head = git_out(tree, &["symbolic-ref", "--quiet", "--short", "HEAD"]).unwrap_or_default();
    Some(TreeState { uncommitted, commits, on_branch: head == branch })
}

/// What tidying did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tidied {
    /// The folder is gone; `branch` too when it had nothing on it.
    Removed { branch: bool },
    /// Kept: it holds work, or git could not say.
    Kept,
    /// Nothing to tidy: a plain folder (the person's own), or a folder already gone.
    Nothing,
}

/// Deletes the branch only while it still points where it began (so a commit made meanwhile keeps it).
fn delete_empty_branch(repo: &Path, branch: &str, base: &str) -> bool {
    git_out(repo, &["update-ref", "-d", &format!("refs/heads/{branch}"), base]).is_some()
}

/// After a run: a worktree whose branch has no commits the base lacks and no uncommitted changes is removed, and its branch with
/// it; an empty scratch folder is removed. Anything with work in it is kept. A follow-up makes the folder again ([`reopen`]).
pub fn tidy(data_dir: &Path, task: &TaskId, p: &Place) -> Tidied {
    match (&p.branch, &p.repo, &p.base_commit) {
        (Some(branch), Some(repo), Some(base)) => {
            let tree = worktree_root(data_dir, task);
            let repo = Path::new(repo);
            if !tree.exists() {
                return Tidied::Nothing;
            }
            let Some(st) = tree_state(&tree, branch, base) else { return Tidied::Kept };
            if st.uncommitted || st.commits > 0 || !st.on_branch {
                return Tidied::Kept;
            }
            // No --force: git itself refuses a worktree with changes, a second guard.
            if git_out(repo, &["worktree", "remove", &tree.to_string_lossy()]).is_none() {
                return Tidied::Kept;
            }
            Tidied::Removed { branch: delete_empty_branch(repo, branch, base) }
        }
        _ if p.plain => Tidied::Nothing,
        _ => {
            let dir = Path::new(&p.workdir);
            let empty = std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_none());
            if empty && std::fs::remove_dir(dir).is_ok() { Tidied::Removed { branch: false } } else if dir.exists() { Tidied::Kept } else { Tidied::Nothing }
        }
    }
}

/// Removing a task: its worktree goes when nothing in it is uncommitted (its branch stays when it has commits, so they are kept
/// in the repository); an empty scratch folder goes. Refused, with words to show, when the folder holds unsaved work: nothing is
/// deleted then. A plain folder is the person's own and is never touched.
pub fn remove_place(data_dir: &Path, task: &TaskId, p: &Place) -> Result<String, String> {
    match (&p.branch, &p.repo, &p.base_commit) {
        (Some(branch), Some(repo), Some(base)) => {
            let tree = worktree_root(data_dir, task);
            let repo = Path::new(repo);
            if !tree.exists() {
                let gone = delete_empty_branch(repo, branch, base);
                return Ok(if gone { format!("Its empty branch {branch} was deleted.") } else { String::new() });
            }
            let Some(st) = tree_state(&tree, branch, base) else {
                return Err("Neko could not read this thread's worktree with git, so it deleted nothing.".into());
            };
            if st.uncommitted {
                return Err("This thread's worktree has changes that are not committed. Neko deletes nothing with unsaved work.".into());
            }
            if git_out(repo, &["worktree", "remove", &tree.to_string_lossy()]).is_none() {
                return Err("git would not remove this thread's worktree, so Neko deleted nothing.".into());
            }
            if st.commits == 0 && st.on_branch && delete_empty_branch(repo, branch, base) {
                Ok(format!("Its worktree and its empty branch {branch} were deleted."))
            } else {
                let n = st.commits;
                Ok(format!("Its worktree was deleted. The branch {branch} keeps its {n} commit{} in the repository.", if n == 1 { "" } else { "s" }))
            }
        }
        _ if p.plain => Ok(String::new()),
        _ => {
            let dir = Path::new(&p.workdir);
            match std::fs::read_dir(dir).map(|mut d| d.next().is_none()) {
                Err(_) => Ok(String::new()),
                Ok(true) => {
                    let _ = std::fs::remove_dir(dir);
                    Ok(String::new())
                }
                Ok(false) => Err("This thread's scratch folder has files in it. Neko deletes nothing with unsaved work.".into()),
            }
        }
    }
}

/// What the task's branch changed since it began: files, lines added and removed (new files count as added). None outside a
/// worktree.
pub fn changes(p: &Place) -> Option<Changes> {
    let base = p.base_commit.as_deref()?;
    let dir = Path::new(&p.workdir);
    let numstat = git_out(dir, &["diff", "--numstat", base, "--", "."])?;
    let mut c = Changes::default();
    for line in numstat.lines() {
        let mut f = line.split('\t');
        c.files += 1;
        c.added += f.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        c.removed += f.next().and_then(|n| n.parse().ok()).unwrap_or(0);
    }
    if let Some(untracked) = git_out(dir, &["ls-files", "--others", "--exclude-standard", "-z", "--", "."]) {
        for name in untracked.split('\0').filter(|n| !n.is_empty()) {
            c.files += 1;
            if let Ok(bytes) = std::fs::read(dir.join(name))
                && bytes.len() <= 2_000_000
                && !bytes.contains(&0)
            {
                c.added += bytes.iter().filter(|b| **b == b'\n').count() as u32 + u32::from(!bytes.is_empty() && !bytes.ends_with(b"\n"));
            }
        }
    }
    Some(c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_short_id_is_the_random_tail() {
        assert_eq!(short_id(&TaskId("task_01KAB0000000000000ABCDWXYZ".into())), "abcdwxyz");
    }
}
