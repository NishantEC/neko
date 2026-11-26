//! Where a task works (docs/knowledge/runtimes.md, "Where it works").
//!
//! - No folder chosen: a fresh scratch folder of its own, under the data folder.
//! - A folder in a git repository: its own worktree on its own branch, `neko/<short id>`, from the commit the checkout is on. The
//!   person's checkout is never touched; nothing is merged or pushed.
//! - A folder that is not a repository: the task works there directly, and its first edit asks.

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
pub fn prepare(data_dir: &Path, task: &TaskId, folder: Option<&str>) -> Result<Place, PlaceError> {
    let short = short_id(task);
    let Some(folder) = folder else {
        let dir = data_dir.join(files::SCRATCH).join(&short);
        std::fs::create_dir_all(&dir).map_err(|e| PlaceError::Other(format!("Neko could not make a folder for this task: {e}")))?;
        return Ok(Place { workdir: dir.to_string_lossy().into_owned(), ..Place::default() });
    };
    let chosen = Path::new(folder);
    if !chosen.is_dir() {
        return Err(PlaceError::Gone(folder.to_owned()));
    }
    let root = git_out(chosen, &["rev-parse", "--show-toplevel"]).map(PathBuf::from);
    let base = root.as_ref().and_then(|_| git_out(chosen, &["rev-parse", "--verify", "HEAD"]));
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
