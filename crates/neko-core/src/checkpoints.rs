//! Per-run checkpoints: the state of a run's folder at its start and end, kept
//! in hidden refs (`refs/neko/checkpoints/<task>/…`) so the person's branches
//! never gain commits, with per-run diffs and undo (and redo). A snapshot is a
//! commit of the folder's whole working state (tracked, modified and untracked
//! files, honouring `.gitignore`) built in a temporary index: the person's
//! index, HEAD and files are only read. See docs/knowledge/checkpoints-and-undo.md.
//!
//! The git work is plain functions over a directory (`capture`, `diff`,
//! `restore_tree`) so it applies to a ticket worktree and to any checkout alike;
//! the rows (`neko_checkpoints_v1:<task>`, their own settings rows, never the
//! workbench snapshot) say which refs belong to which run.
use crate::db::Db;
use crate::native_runner::{git_command, git_without_filters};
use serde::{Deserialize, Serialize};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

/// Files over this size are left out of a snapshot (and the skip is recorded).
pub const MAX_FILE_BYTES: u64 = 5 * 1024 * 1024;
/// Checkpoint refs kept per ticket; the oldest go first.
pub const MAX_PER_TASK: usize = 50;
/// Checkpoints older than this are deleted.
pub const KEEP_MS: i64 = 14 * 24 * 60 * 60 * 1000;
/// Skipped files listed per snapshot (the count is kept in full).
const MAX_SKIPPED_LISTED: usize = 20;
/// The most of a diff returned for display.
pub const MAX_DIFF_BYTES: usize = 512 * 1024;

const ROW: &str = "neko_checkpoints_v1:";
const INDEX: &str = "neko_checkpoints_index_v1";
const REF_ROOT: &str = "refs/neko/checkpoints";

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Skipped {
    pub path: String,
    pub bytes: u64,
}

/// One snapshot: a commit under a hidden ref, its tree and the HEAD it was taken at.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Point {
    pub commit: String,
    pub tree: String,
    pub head: Option<String>,
    pub skipped: Vec<Skipped>,
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    #[default]
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Run {
    pub n: u32,
    pub start: Point,
    pub end: Option<Point>,
    pub started_ms: i64,
    pub ended_ms: i64,
    pub outcome: Outcome,
    /// Files changed and lines added and removed, start to end.
    pub files: usize,
    pub added: usize,
    pub removed: usize,
    /// Restored away by an undo (until a redo brings it back).
    pub undone: bool,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExtraKind {
    /// Taken before a restore, so the undo can be undone.
    #[default]
    Safety,
    /// Taken at the person's request for edits made outside any run.
    Manual,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Extra {
    /// The ref's last component (`safe3`).
    pub id: String,
    pub kind: ExtraKind,
    pub point: Point,
    pub at_ms: i64,
    pub note: String,
}

/// What a redo brings back.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Redo {
    pub extra_id: String,
    pub undone_runs: Vec<u32>,
    pub label: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Row {
    /// The source checkout whose refs hold the checkpoints (for cleanup when the folder is gone).
    pub repo: String,
    pub next_run: u32,
    pub next_extra: u32,
    pub runs: Vec<Run>,
    pub extras: Vec<Extra>,
    /// The tree the folder should have right now, if nobody edited it: the last
    /// run's end, or what the last undo, redo or manual snapshot left.
    pub expected_tree: String,
    pub redo: Option<Redo>,
    /// Told to the next agent turn (once): what was undone.
    pub notice: String,
}

// ---------------------------------------------------------------------------
// Git plumbing
// ---------------------------------------------------------------------------

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).trim().to_owned()
}

fn run_command(mut command: Command, index: Option<&Path>, stdin: &[u8]) -> Result<Vec<u8>, String> {
    if let Some(index) = index {
        command.env("GIT_INDEX_FILE", index);
    }
    command.env("GIT_AUTHOR_NAME", "Neko").env("GIT_AUTHOR_EMAIL", "neko@localhost");
    command.env("GIT_COMMITTER_NAME", "Neko").env("GIT_COMMITTER_EMAIL", "neko@localhost");
    command.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(if stdin.is_empty() { Stdio::null() } else { Stdio::piped() });
    let mut child = command.spawn().map_err(|e| format!("Cannot run git: {e}"))?;
    if !stdin.is_empty() {
        if let Some(mut pipe) = child.stdin.take() {
            // A broken pipe shows up as git's own failure below.
            let _ = pipe.write_all(stdin);
        }
    }
    let output = child.wait_with_output().map_err(|e| format!("Cannot run git: {e}"))?;
    if !output.status.success() {
        return Err(format!("git failed: {}", text(&output.stderr).chars().take(300).collect::<String>()));
    }
    Ok(output.stdout)
}

/// Plain git (no filters touched): reads and ref updates.
fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    let mut command = git_command(dir);
    command.args(args);
    run_command(command, None, &[]).map(|b| text(&b))
}

/// Git that may compare or add working files: repository filters are switched off,
/// so a snapshot never runs repository code.
fn git_files(dir: &Path, index: Option<&Path>, args: &[&str], stdin: &[u8]) -> Result<Vec<u8>, String> {
    let mut command = git_without_filters(dir, &AtomicBool::new(false))?;
    command.args(args);
    run_command(command, index, stdin)
}

/// A scratch index inside the git directory, deleted when dropped.
struct TempIndex(PathBuf);

impl TempIndex {
    fn empty(dir: &Path) -> Result<Self, String> {
        let git_dir = PathBuf::from(git(dir, &["rev-parse", "--absolute-git-dir"])?);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.subsec_nanos());
        let path = git_dir.join(format!("neko-checkpoint-index-{}-{nanos}", std::process::id()));
        Ok(Self(path))
    }

    fn seeded(dir: &Path) -> Result<Self, String> {
        let temp = Self::empty(dir)?;
        // A copy of the person's index keeps its stat cache, so only files that
        // changed are read again; it is a copy, the original is never written.
        let real = PathBuf::from(git(dir, &["rev-parse", "--path-format=absolute", "--git-path", "index"])?);
        if real.is_file() {
            std::fs::copy(&real, &temp.0).map_err(|e| format!("Cannot copy the index: {e}"))?;
        }
        Ok(temp)
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(self.0.with_extension("lock"));
    }
}

fn toplevel(dir: &Path) -> Result<PathBuf, String> {
    Ok(PathBuf::from(git(dir, &["rev-parse", "--show-toplevel"]).map_err(|_| "This folder is not a Git checkout".to_owned())?))
}

fn head_of(dir: &Path) -> Option<String> {
    git(dir, &["rev-parse", "-q", "--verify", "HEAD^{commit}"]).ok().filter(|h| !h.is_empty())
}

/// The tree of the folder's whole working state, and the files left out for size.
fn working_tree(dir: &Path) -> Result<(String, Vec<Skipped>, usize), String> {
    let top = toplevel(dir)?;
    let index = TempIndex::seeded(&top)?;
    if !index.0.exists() {
        if let Some(head) = head_of(&top) {
            git_files(&top, Some(&index.0), &["read-tree", &head], &[])?;
        }
    }
    // Modified and untracked files, the ones that could be big and new.
    let listed = git_files(&top, None, &["ls-files", "-z", "-m", "-o", "--exclude-standard"], &[])?;
    let mut skipped = Vec::new();
    let mut skipped_count = 0;
    let mut args: Vec<String> = vec!["add".into(), "-A".into(), "--".into(), ".".into()];
    for path in listed.split(|b| *b == 0).filter(|p| !p.is_empty()) {
        let path = String::from_utf8_lossy(path).into_owned();
        let Ok(meta) = std::fs::symlink_metadata(top.join(&path)) else { continue };
        if meta.is_file() && meta.len() > MAX_FILE_BYTES {
            args.push(format!(":(exclude,literal){path}"));
            skipped_count += 1;
            if skipped.len() < MAX_SKIPPED_LISTED {
                skipped.push(Skipped { path, bytes: meta.len() });
            }
        }
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    git_files(&top, Some(&index.0), &args, &[])?;
    let tree = text(&git_files(&top, Some(&index.0), &["write-tree"], &[])?);
    Ok((tree, skipped, skipped_count))
}

/// Snapshots the folder under `ref_name` (a commit whose parent is the current
/// HEAD, so the objects stay reachable). The person's index, HEAD and files are untouched.
pub fn capture(dir: &Path, ref_name: &str, message: &str) -> Result<Point, String> {
    let (tree, skipped, skipped_count) = working_tree(dir)?;
    let head = head_of(dir);
    let mut args = vec!["commit-tree", tree.as_str()];
    if let Some(head) = &head {
        args.extend(["-p", head.as_str()]);
    }
    args.extend(["-m", message]);
    let commit = text(&git_files(dir, None, &["-c", "commit.gpgsign=false"].iter().chain(args.iter()).copied().collect::<Vec<_>>(), &[])?);
    git(dir, &["update-ref", ref_name, &commit])?;
    Ok(Point { commit, tree, head, skipped, skipped_count })
}

/// A per-run diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Diff {
    pub files: Vec<String>,
    pub added: usize,
    pub removed: usize,
    pub patch: String,
    pub truncated: bool,
}

/// Files changed and lines added and removed between two commits.
pub fn stat(dir: &Path, from: &str, to: &str) -> Result<(usize, usize, usize), String> {
    let numstat = git_files(dir, None, &["diff", "--numstat", "-z", "--no-renames", "--no-ext-diff", "--no-textconv", from, to], &[])?;
    let (mut files, mut added, mut removed) = (0, 0, 0);
    for record in numstat.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = String::from_utf8_lossy(record);
        let mut parts = record.splitn(3, '\t');
        added += parts.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        removed += parts.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or(0);
        files += 1;
    }
    Ok((files, added, removed))
}

/// What changed between two snapshots (or any two commits).
pub fn diff(dir: &Path, from: &str, to: &str) -> Result<Diff, String> {
    let numstat = git_files(dir, None, &["diff", "--numstat", "-z", "--no-renames", "--no-ext-diff", "--no-textconv", from, to], &[])?;
    let mut out = Diff::default();
    for record in numstat.split(|b| *b == 0).filter(|r| !r.is_empty()) {
        let record = String::from_utf8_lossy(record);
        let mut parts = record.splitn(3, '\t');
        let (add, del, path) = (parts.next().unwrap_or("0"), parts.next().unwrap_or("0"), parts.next().unwrap_or(""));
        out.added += add.parse::<usize>().unwrap_or(0);
        out.removed += del.parse::<usize>().unwrap_or(0);
        out.files.push(path.to_owned());
    }
    let patch = git_files(dir, None, &["diff", "--binary", "--no-renames", "--no-ext-diff", "--no-textconv", from, to], &[])?;
    out.truncated = patch.len() > MAX_DIFF_BYTES;
    out.patch = String::from_utf8_lossy(&patch[..patch.len().min(MAX_DIFF_BYTES)]).into_owned();
    if out.truncated {
        out.patch.push_str("\n… (patch shortened)");
    }
    Ok(out)
}

fn names(dir: &Path, from: &str, to: &str, filter: &str) -> Result<Vec<String>, String> {
    let out = git_files(dir, None, &["diff-tree", "-r", "--name-only", "--no-renames", "-z", &format!("--diff-filter={filter}"), from, to], &[])?;
    Ok(out.split(|b| *b == 0).filter(|p| !p.is_empty()).map(|p| String::from_utf8_lossy(p).into_owned()).collect())
}

fn safe_relative(path: &str) -> bool {
    !path.is_empty() && Path::new(path).components().all(|c| matches!(c, std::path::Component::Normal(_)))
}

/// Makes the folder's files match `target_tree`. `current_tree` is the tree of
/// the folder as it is now (the safety snapshot just taken): files only it has
/// are deleted, files that differ are written. Ignored files, files over the
/// size guard and anything outside both trees are left alone.
pub fn restore_tree(dir: &Path, current_tree: &str, target_tree: &str) -> Result<(), String> {
    let top = toplevel(dir)?;
    for path in names(&top, target_tree, current_tree, "A")? {
        if !safe_relative(&path) {
            continue;
        }
        let file = top.join(&path);
        if std::fs::symlink_metadata(&file).is_ok_and(|m| !m.is_dir()) {
            std::fs::remove_file(&file).map_err(|e| format!("Cannot remove {path}: {e}"))?;
        }
        // Folders only this run made go too, once empty.
        let mut parent = file.parent();
        while let Some(folder) = parent.filter(|p| *p != top && p.starts_with(&top)) {
            if std::fs::remove_dir(folder).is_err() {
                break;
            }
            parent = folder.parent();
        }
    }
    let mut write: Vec<u8> = Vec::new();
    for path in names(&top, current_tree, target_tree, "ACMRT")? {
        if safe_relative(&path) {
            write.extend_from_slice(path.as_bytes());
            write.push(0);
        }
    }
    if !write.is_empty() {
        let index = TempIndex::empty(&top)?;
        git_files(&top, Some(&index.0), &["read-tree", target_tree], &[])?;
        git_files(&top, Some(&index.0), &["checkout-index", "-f", "-z", "--stdin"], &write)?;
    }
    Ok(())
}

/// Points the person's index at `head` (their files stay as restored), or empties it for an unborn branch.
fn reset_index(dir: &Path, head: Option<&str>) -> Result<(), String> {
    match head {
        Some(head) => git(dir, &["reset", "-q", "--mixed", head]).map(|_| ()),
        None => git(dir, &["read-tree", "--empty"]).map(|_| ()),
    }
}

fn is_ancestor(dir: &Path, ancestor: &str, of: &str) -> bool {
    let mut command = git_command(dir);
    command.args(["merge-base", "--is-ancestor", ancestor, of]);
    run_command(command, None, &[]).is_ok()
}

/// The commits made since `head` (oldest first), when `head` is an ancestor of the current HEAD.
fn commits_since(dir: &Path, head: &str) -> Result<Vec<String>, String> {
    Ok(git(dir, &["rev-list", "--reverse", &format!("{head}..HEAD")])?.lines().map(str::to_owned).collect())
}

/// Whether a commit is already on a remote-tracking branch.
fn on_a_remote(dir: &Path, commit: &str) -> bool {
    git(dir, &["for-each-ref", "--count=1", "--format=%(refname)", "--contains", commit, "refs/remotes"]).is_ok_and(|r| !r.is_empty())
}

fn leaf(task_id: &str) -> String {
    task_id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect()
}

fn ref_for(task_id: &str, name: &str) -> String {
    format!("{REF_ROOT}/{}/{name}", leaf(task_id))
}

/// Deletes every ref under the given names.
fn delete_refs(dir: &Path, refs: &[String]) {
    if refs.is_empty() {
        return;
    }
    let input: String = refs.iter().map(|r| format!("delete {r}\n")).collect();
    let mut command = git_command(dir);
    command.args(["update-ref", "--stdin"]);
    let _ = run_command(command, None, input.as_bytes());
}

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

fn lock(db: &Mutex<Db>) -> std::sync::MutexGuard<'_, Db> {
    db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn read_key<T: for<'de> Deserialize<'de> + Default>(db: &Db, key: &str) -> Result<T, String> {
    match db.get_setting(key).map_err(|e| e.to_string())? {
        Some(text) => serde_json::from_str(&text).map_err(|e| format!("Cannot read {key}: {e}")),
        None => Ok(T::default()),
    }
}

/// The ticket's checkpoint row (empty when it has none).
pub fn row(db: &Mutex<Db>, task_id: &str) -> Result<Row, String> {
    read_key(&lock(db), &format!("{ROW}{task_id}"))
}

/// Rows and the index are read and written whole; one writer at a time keeps the index whole.
static ROWS: Mutex<()> = Mutex::new(());

fn save_row(db: &Mutex<Db>, task_id: &str, row: &Row) -> Result<(), String> {
    let _one = ROWS.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let db = lock(db);
    let key = format!("{ROW}{task_id}");
    let mut index: Vec<String> = read_key(&db, INDEX)?;
    if row.runs.is_empty() && row.extras.is_empty() {
        db.delete_setting(&key).map_err(|e| e.to_string())?;
        if index.iter().any(|id| id == task_id) {
            index.retain(|id| id != task_id);
            db.set_setting(INDEX, &serde_json::to_string(&index).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
        }
        return Ok(());
    }
    db.set_setting(&key, &serde_json::to_string(row).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if !index.iter().any(|id| id == task_id) {
        index.push(task_id.to_owned());
        db.set_setting(INDEX, &serde_json::to_string(&index).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Tickets that have checkpoints.
pub fn tasks_with_checkpoints(db: &Mutex<Db>) -> Vec<String> {
    read_key(&lock(db), INDEX).unwrap_or_default()
}

fn ref_count(row: &Row) -> usize {
    row.runs.iter().map(|r| 1 + usize::from(r.end.is_some())).sum::<usize>() + row.extras.len()
}

/// Drops the oldest checkpoints until the ticket is within `MAX_PER_TASK`: extras
/// first (but never the one a redo needs), then the oldest runs after the first.
fn enforce_cap(dir: &Path, task_id: &str, row: &mut Row) {
    let mut doomed = Vec::new();
    while ref_count(row) > MAX_PER_TASK {
        let keep = row.redo.as_ref().map(|r| r.extra_id.clone());
        if let Some(i) = row.extras.iter().position(|e| Some(&e.id) != keep.as_ref()) {
            doomed.push(ref_for(task_id, &row.extras.remove(i).id));
        } else if row.runs.len() > 1 {
            let run = row.runs.remove(1);
            doomed.push(ref_for(task_id, &format!("{}-start", run.n)));
            if run.end.is_some() {
                doomed.push(ref_for(task_id, &format!("{}-end", run.n)));
            }
        } else {
            break;
        }
    }
    delete_refs(dir, &doomed);
}

/// Starts a run: snapshots the folder under `<n>-start`. Returns the run number.
/// A new run ends the chance to redo an earlier undo.
pub fn begin_run(db: &Mutex<Db>, task_id: &str, dir: &Path, now_ms: i64) -> Result<u32, String> {
    let mut row = row(db, task_id)?;
    let n = row.next_run + 1;
    let start = capture(dir, &ref_for(task_id, &format!("{n}-start")), &format!("Neko checkpoint: start of run {n}"))?;
    if row.repo.is_empty() {
        row.repo = crate::native_runner::publish::source_checkout(dir, &AtomicBool::new(false)).map_or_else(|_| dir.to_string_lossy().into_owned(), |p| p.to_string_lossy().into_owned());
    }
    // A run that never ended (the daemon stopped mid-run) ends where this one starts.
    for stale in row.runs.iter_mut().filter(|r| r.end.is_none()) {
        stale.end = Some(start.clone());
        stale.ended_ms = now_ms;
        stale.outcome = Outcome::Cancelled;
        let (files, added, removed) = stat(dir, &stale.start.commit, &start.commit).unwrap_or_default();
        (stale.files, stale.added, stale.removed) = (files, added, removed);
    }
    row.next_run = n;
    row.expected_tree = start.tree.clone();
    row.runs.push(Run { n, start, end: None, started_ms: now_ms, ended_ms: 0, outcome: Outcome::Running, ..Default::default() });
    if let Some(redo) = row.redo.take() {
        row.extras.retain(|e| e.id != redo.extra_id);
        delete_refs(dir, &[ref_for(task_id, &redo.extra_id)]);
    }
    enforce_cap(dir, task_id, &mut row);
    save_row(db, task_id, &row)?;
    Ok(n)
}

/// Ends a run: snapshots the folder under `<n>-end`. Returns the run as stored.
pub fn end_run(db: &Mutex<Db>, task_id: &str, n: u32, dir: &Path, outcome: Outcome, now_ms: i64) -> Result<Run, String> {
    let end = capture(dir, &ref_for(task_id, &format!("{n}-end")), &format!("Neko checkpoint: end of run {n}"))?;
    let mut row = row(db, task_id)?;
    let run = row.runs.iter_mut().find(|r| r.n == n).ok_or("That run's checkpoint is gone")?;
    (run.files, run.added, run.removed) = stat(dir, &run.start.commit, &end.commit).unwrap_or_default();
    run.end = Some(end.clone());
    run.ended_ms = now_ms;
    run.outcome = outcome;
    let stored = run.clone();
    row.expected_tree = end.tree;
    enforce_cap(dir, task_id, &mut row);
    save_row(db, task_id, &row)?;
    Ok(stored)
}

/// The diff of one run (start to end; to the folder's current state for a run still going),
/// or of the whole task (the first kept start to the last end) when `run` is `None`.
pub fn diff_of(db: &Mutex<Db>, task_id: &str, dir: &Path, run: Option<u32>) -> Result<Diff, String> {
    let row = row(db, task_id)?;
    let (from, to) = match run {
        Some(n) => {
            let run = row.runs.iter().find(|r| r.n == n).ok_or_else(|| format!("No checkpoint for run {n}"))?;
            (run.start.commit.clone(), run.end.as_ref().ok_or("That run has not finished yet")?.commit.clone())
        }
        None => {
            let first = row.runs.first().ok_or("No runs have been checkpointed yet")?;
            let last = row.runs.iter().rev().find_map(|r| r.end.as_ref()).ok_or("No run has finished yet")?;
            (first.start.commit.clone(), last.commit.clone())
        }
    };
    diff(dir, &from, &to)
}

// ---------------------------------------------------------------------------
// Undo and redo
// ---------------------------------------------------------------------------

/// Why an undo or redo did not happen. Each has a code the app keys its offer on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// Later runs exist; undoing this one means undoing them too.
    LaterRuns(Vec<u32>),
    /// The folder has changes that are in no checkpoint.
    Unsaved,
    /// A commit of the run was pushed or is in an open pull request.
    Pushed(String),
    /// A run is working in the folder.
    Busy,
    /// No such checkpoint, already undone, or nothing to redo.
    Nothing(String),
    Failed(String),
}

impl Refusal {
    pub fn code(&self) -> &'static str {
        match self {
            Self::LaterRuns(_) => "later_runs",
            Self::Unsaved => "unsaved_changes",
            Self::Pushed(_) => "pushed",
            Self::Busy => "busy",
            Self::Nothing(_) => "nothing",
            Self::Failed(_) => "failed",
        }
    }
    pub fn message(&self) -> String {
        match self {
            Self::LaterRuns(runs) => format!(
                "Later runs exist ({}). Undoing this run would undo them too: use \"Undo back to here\" if that is what you want.",
                runs.iter().map(|n| format!("run {n}")).collect::<Vec<_>>().join(", ")
            ),
            Self::Unsaved => "The folder has changes made after the last run that are in no checkpoint. Snapshot them first so this undo can be undone, then try again.".into(),
            Self::Pushed(detail) => format!("{detail} Rewinding would leave the pushed history behind, so nothing was changed. A revert commit undoes the run without rewriting history."),
            Self::Busy => "A run is working in this folder right now. Stop it first.".into(),
            Self::Nothing(text) | Self::Failed(text) => text.clone(),
        }
    }
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl From<String> for Refusal {
    fn from(error: String) -> Self {
        Self::Failed(error)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UndoOptions {
    pub run: u32,
    /// Undo the later runs too ("Undo back to here").
    pub through_later: bool,
    /// Snapshot edits no checkpoint holds first, then undo.
    pub snapshot_unsaved: bool,
    /// Keep the history and add a commit that reverts the run instead of rewinding.
    pub revert: bool,
    /// The ticket has an open pull request.
    pub pull_request_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Done {
    /// Runs restored away.
    pub runs: Vec<u32>,
    pub message: String,
    pub reverted_by_commit: bool,
}

/// Whether the folder has changes that are in no checkpoint (it is not as the last run, undo or redo left it).
pub fn unsaved(dir: &Path, row: &Row) -> Result<bool, String> {
    matching(dir, row).map(|same| !same)
}

fn matching(dir: &Path, row: &Row) -> Result<bool, String> {
    if row.expected_tree.is_empty() {
        return Ok(true);
    }
    Ok(working_tree(dir)?.0 == row.expected_tree)
}

/// A run's pushed state: the commits the run (and anything after it) added to
/// HEAD that are on a remote, or in an open pull request.
fn pushed_reason(dir: &Path, start_head: &Option<String>, pull_request_open: bool) -> Result<Option<String>, Refusal> {
    let now = head_of(dir);
    let Some(start) = start_head.as_deref() else {
        return if now.is_some() { Err(Refusal::Failed("This run made the repository's first commit, which cannot be rewound; undo it with a revert instead.".into())) } else { Ok(None) };
    };
    let Some(now) = now else { return Ok(None) };
    if start == now {
        return Ok(None);
    }
    if !is_ancestor(dir, start, &now) {
        return Err(Refusal::Failed("The branch's history was rewritten since this run started, so it cannot be rewound safely.".into()));
    }
    let commits = commits_since(dir, start)?;
    let Some(oldest) = commits.first() else { return Ok(None) };
    if on_a_remote(dir, oldest) {
        return Ok(Some("A commit made during or after this run was already pushed.".into()));
    }
    if pull_request_open {
        return Ok(Some("A commit made during or after this run is in an open pull request.".into()));
    }
    Ok(None)
}

fn push_extra(row: &mut Row, task_id: &str, dir: &Path, kind: ExtraKind, note: &str, now_ms: i64) -> Result<Extra, String> {
    row.next_extra += 1;
    let id = format!("{}{}", if kind == ExtraKind::Safety { "safe" } else { "manual" }, row.next_extra);
    let point = capture(dir, &ref_for(task_id, &id), &format!("Neko checkpoint: {note}"))?;
    let extra = Extra { id, kind, point, at_ms: now_ms, note: note.to_owned() };
    row.extras.push(extra.clone());
    Ok(extra)
}

/// Every refusal an undo can have, decided without changing anything. Returns the run
/// to restore and whether edits no checkpoint holds must be snapshotted first.
fn check_undo(row: &Row, dir: &Path, options: &UndoOptions, running: bool) -> Result<(Run, bool), Refusal> {
    if running {
        return Err(Refusal::Busy);
    }
    let run = row.runs.iter().find(|r| r.n == options.run).cloned().ok_or_else(|| Refusal::Nothing(format!("Run {} has no checkpoint (it is older than the ones kept, or never started).", options.run)))?;
    if run.undone {
        return Err(Refusal::Nothing(format!("Run {} is already undone.", run.n)));
    }
    if run.end.is_none() {
        return Err(Refusal::Busy);
    }
    let later: Vec<u32> = row.runs.iter().filter(|r| r.n > run.n && !r.undone).map(|r| r.n).collect();
    if !later.is_empty() && !options.through_later {
        return Err(Refusal::LaterRuns(later));
    }
    if !options.revert {
        if let Some(detail) = pushed_reason(dir, &run.start.head, options.pull_request_open)? {
            return Err(Refusal::Pushed(detail));
        }
    }
    let unsaved = !matching(dir, row)?;
    if unsaved && !options.snapshot_unsaved {
        return Err(Refusal::Unsaved);
    }
    Ok((run, unsaved))
}

/// The text of a chat `undo_run` action: the run number, then any of `back`
/// (also undo the later runs), `snapshot` (keep unsaved edits first), `revert`.
pub fn parse_action_text(text: &str) -> Option<UndoOptions> {
    let mut words = text.split_whitespace();
    let run = words.next()?.parse().ok()?;
    let mut options = UndoOptions { run, ..Default::default() };
    for word in words {
        match word {
            "back" => options.through_later = true,
            "snapshot" => options.snapshot_unsaved = true,
            "revert" => options.revert = true,
            _ => return None,
        }
    }
    Some(options)
}

/// Whether an undo would go ahead, and which runs it would restore away; nothing is changed.
pub fn preview_undo(db: &Mutex<Db>, task_id: &str, dir: &Path, options: UndoOptions, running: bool) -> Result<Vec<u32>, Refusal> {
    let row = row(db, task_id)?;
    let (run, _) = check_undo(&row, dir, &options, running)?;
    Ok(row.runs.iter().filter(|r| r.n >= run.n && !r.undone).map(|r| r.n).collect())
}

/// Restores the folder to the start of run `options.run`. `running` says a run is
/// working in the folder now. Nothing is changed unless every check passes; a
/// safety checkpoint is taken first so the undo can be redone.
pub fn undo(db: &Mutex<Db>, task_id: &str, dir: &Path, options: UndoOptions, running: bool, now_ms: i64) -> Result<Done, Refusal> {
    let mut row = row(db, task_id)?;
    let (run, unsaved) = check_undo(&row, dir, &options, running)?;
    if unsaved {
        let extra = push_extra(&mut row, task_id, dir, ExtraKind::Manual, "changes made outside any run", now_ms)?;
        row.expected_tree = extra.point.tree;
    }
    // The way back.
    let safety = push_extra(&mut row, task_id, dir, ExtraKind::Safety, &format!("before undoing run {}", run.n), now_ms)?;
    let mut undone: Vec<u32> = row.runs.iter().filter(|r| r.n >= run.n && !r.undone).map(|r| r.n).collect();
    undone.sort_unstable();
    // The files first; the history only when asked (a revert keeps it).
    restore_tree(dir, &safety.point.tree, &run.start.tree)?;
    // A revert keeps the history: one new commit whose tree is the committed state the run
    // started from (the run's uncommitted work was only ever in the files).
    let mut reverted_by_commit = false;
    if options.revert {
        if let (Some(head), Some(start)) = (head_of(dir), run.start.head.as_deref()) {
            let target = git(dir, &["rev-parse", &format!("{start}^{{tree}}")])?;
            if git(dir, &["rev-parse", "HEAD^{tree}"])? != target {
                let commit = text(&git_files(dir, None, &["-c", "commit.gpgsign=false", "commit-tree", &target, "-p", &head, "-m", &format!("Revert run {} (Neko)", run.n)], &[])?);
                git(dir, &["update-ref", "-m", &format!("neko: revert run {}", run.n), "HEAD", &commit, &head])?;
                reset_index(dir, Some(&commit))?;
                reverted_by_commit = true;
            }
        }
    }
    if !reverted_by_commit && !options.revert {
        reset_index(dir, run.start.head.as_deref())?;
    } else if !reverted_by_commit {
        reset_index(dir, head_of(dir).as_deref())?;
    }
    for r in &mut row.runs {
        if undone.contains(&r.n) {
            r.undone = true;
        }
    }
    row.expected_tree = run.start.tree.clone();
    row.redo = Some(Redo { extra_id: safety.id, undone_runs: undone.clone(), label: format!("undo of run {}", run.n) });
    let which = undone.iter().map(|n| format!("run {n}")).collect::<Vec<_>>().join(", ");
    row.notice = format!(
        "The person undid {which}: the files in this folder were restored to how they were before {}{}. Everything those runs changed is gone from the folder unless the person redoes it, so do not assume it exists; re-read the files before relying on them.",
        if undone.len() == 1 { "that run started" } else { "the first of them started" },
        if reverted_by_commit { " (kept in history as a revert commit)" } else { "" }
    );
    enforce_cap(dir, task_id, &mut row);
    save_row(db, task_id, &row)?;
    Ok(Done { message: format!("Undid {which}. The folder is back to how it was before run {} started. Redo brings it back.", run.n), runs: undone, reverted_by_commit })
}

/// Undoes the last undo: restores the safety checkpoint it took.
pub fn redo(db: &Mutex<Db>, task_id: &str, dir: &Path, running: bool) -> Result<Done, Refusal> {
    if running {
        return Err(Refusal::Busy);
    }
    let mut row = row(db, task_id)?;
    let redo = row.redo.clone().ok_or_else(|| Refusal::Nothing("There is no undo to redo.".into()))?;
    let extra = row.extras.iter().find(|e| e.id == redo.extra_id).cloned().ok_or_else(|| Refusal::Nothing("The checkpoint this redo needs is gone.".into()))?;
    if !matching(dir, &row)? {
        return Err(Refusal::Unsaved);
    }
    let current = capture(dir, &ref_for(task_id, "redo-from"), "Neko checkpoint: before redo")?;
    restore_tree(dir, &current.tree, &extra.point.tree)?;
    match (&extra.point.head, head_of(dir)) {
        (Some(target), Some(now)) if *target != now => {
            if !is_ancestor(dir, &now, target) && !is_ancestor(dir, target, &now) {
                return Err(Refusal::Failed("The branch moved to different history since the undo, so the redo cannot move it back.".into()));
            }
            reset_index(dir, Some(target))?;
        }
        (head, _) => reset_index(dir, head.as_deref())?,
    }
    delete_refs(dir, &[ref_for(task_id, "redo-from")]);
    for r in &mut row.runs {
        if redo.undone_runs.contains(&r.n) {
            r.undone = false;
        }
    }
    row.expected_tree = extra.point.tree.clone();
    row.extras.retain(|e| e.id != redo.extra_id);
    delete_refs(dir, &[ref_for(task_id, &redo.extra_id)]);
    row.redo = None;
    let which = redo.undone_runs.iter().map(|n| format!("run {n}")).collect::<Vec<_>>().join(", ");
    row.notice = format!("The person redid {which}: the files in this folder are back to how those runs left them.");
    save_row(db, task_id, &row)?;
    Ok(Done { message: format!("Redid {which}."), runs: redo.undone_runs, reverted_by_commit: false })
}

/// Snapshots edits made outside any run, on request.
pub fn snapshot_unsaved(db: &Mutex<Db>, task_id: &str, dir: &Path, now_ms: i64) -> Result<(), String> {
    let mut row = row(db, task_id)?;
    let extra = push_extra(&mut row, task_id, dir, ExtraKind::Manual, "changes made outside any run", now_ms)?;
    row.expected_tree = extra.point.tree;
    enforce_cap(dir, task_id, &mut row);
    save_row(db, task_id, &row)
}

/// What the next turn is told about an undo or redo, once.
pub fn take_notice(db: &Mutex<Db>, task_id: &str) -> Option<String> {
    let mut row = row(db, task_id).ok()?;
    if row.notice.is_empty() {
        return None;
    }
    let notice = std::mem::take(&mut row.notice);
    save_row(db, task_id, &row).ok()?;
    Some(notice)
}

// ---------------------------------------------------------------------------
// Housekeeping
// ---------------------------------------------------------------------------

fn repo_dir(row: &Row, fallback: Option<&Path>) -> Option<PathBuf> {
    fallback.filter(|p| p.is_dir()).map(Path::to_path_buf).or_else(|| Some(PathBuf::from(&row.repo)).filter(|p| p.is_dir()))
}

fn collect(task_id: &str, run: &Run) -> Vec<String> {
    let mut refs = vec![ref_for(task_id, &format!("{}-start", run.n))];
    if run.end.is_some() {
        refs.push(ref_for(task_id, &format!("{}-end", run.n)));
    }
    refs
}

/// Deletes all of a ticket's checkpoints (it is Completed and parked, or gone), then `git gc --auto`.
pub fn delete_task(db: &Mutex<Db>, task_id: &str, folder: Option<&Path>) -> Result<(), String> {
    let row = row(db, task_id)?;
    if let Some(dir) = repo_dir(&row, folder) {
        let mut refs: Vec<String> = row.runs.iter().flat_map(|r| collect(task_id, r)).collect();
        refs.extend(row.extras.iter().map(|e| ref_for(task_id, &e.id)));
        refs.push(ref_for(task_id, "redo-from"));
        delete_refs(&dir, &refs);
        let _ = git(&dir, &["gc", "--auto", "--quiet"]);
    }
    save_row(db, task_id, &Row::default())
}

/// Deletes checkpoints older than `KEEP_MS`. Returns how many refs were dropped.
pub fn expire(db: &Mutex<Db>, now_ms: i64) -> usize {
    let mut dropped = 0;
    for task_id in tasks_with_checkpoints(db) {
        let Ok(mut row) = row(db, &task_id) else { continue };
        let Some(dir) = repo_dir(&row, None) else { continue };
        let old = |at: i64| at > 0 && now_ms - at > KEEP_MS;
        let mut doomed = Vec::new();
        let keep = row.redo.as_ref().map(|r| r.extra_id.clone());
        row.runs.retain(|run| {
            let expired = old(if run.ended_ms > 0 { run.ended_ms } else { run.started_ms }) && run.outcome != Outcome::Running;
            if expired {
                doomed.extend(collect(&task_id, run));
            }
            !expired
        });
        row.extras.retain(|extra| {
            let expired = old(extra.at_ms) && Some(&extra.id) != keep.as_ref();
            if expired {
                doomed.push(ref_for(&task_id, &extra.id));
            }
            !expired
        });
        if doomed.is_empty() {
            continue;
        }
        dropped += doomed.len();
        delete_refs(&dir, &doomed);
        let _ = git(&dir, &["gc", "--auto", "--quiet"]);
        let _ = save_row(db, &task_id, &row);
    }
    dropped
}

/// Hidden refs a ticket has in the repository (for checks and cleanup).
pub fn refs_of(dir: &Path, task_id: &str) -> Vec<String> {
    git(dir, &["for-each-ref", "--format=%(refname)", &format!("{REF_ROOT}/{}/", leaf(task_id))]).map(|t| t.lines().map(str::to_owned).collect()).unwrap_or_default()
}

#[cfg(test)]
mod tests;
