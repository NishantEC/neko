//! Checkpoints: pictures of a task's folder, taken at each run's start and end, so a run can be reviewed, undone and redone
//! (docs/knowledge/undo.md). Every git command undo runs is in this module.
//!
//! A checkpoint is a commit under Neko's own hidden refs, `refs/neko/checkpoints/<task>/<run>/<kind>`, made from a temporary index
//! file. It never touches the person's index, HEAD, branch, stash, remotes or config, and never writes a working file. A folder
//! that is not a git repository (a plain workspace, a scratch folder) gets a shadow repository in Neko's data folder with the
//! folder as its work tree, so nothing is written into the folder either.
//!
//! What git is asked, and nothing else:
//!
//! - `rev-parse --verify --quiet HEAD^{commit}`, `rev-parse --git-path <name>`: where things are, read only.
//! - `status --porcelain=v1 -z --untracked-files=all --no-renames --ignore-submodules=all`, on the temporary index: what is new
//!   or changed, for the size guard.
//! - `read-tree <tree>`, `add -A`, `write-tree`, `update-index -q --refresh`: on the temporary index only (`GIT_INDEX_FILE`).
//! - `commit-tree <tree> [-p HEAD] -m … --no-gpg-sign`: a commit on no branch.
//! - `update-ref refs/neko/checkpoints/…`, `for-each-ref refs/neko/checkpoints/…`, `update-ref -d refs/neko/checkpoints/…`:
//!   Neko's own refs only.
//! - `diff-tree -r -z -M --raw|--numstat`, `diff-tree -p -M --no-color --no-ext-diff`, `cat-file -s`: reading.
//! - Undo and redo only: `read-tree -m -u --no-recurse-submodules <from> <to>` on the temporary index, which writes the working
//!   files from one checkpoint's tree to another's.
//! - The shadow repository only (Neko's own): `init --bare`, `config`, `gc --prune=now`.
//!
//! Every command runs with `GIT_OPTIONAL_LOCKS=0` (so even a status writes no index), no fsmonitor, no untracked cache, no
//! automatic gc or maintenance, no prompt, and without any `GIT_*` variable the daemon may have inherited.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use neko_protocol::{Changes, DiffFile, FileStatus, TaskId, files as dfiles};

use crate::find;
use crate::records::Place;

/// Where Neko's checkpoint refs live in a repository.
pub const REF_ROOT: &str = "refs/neko/checkpoints";

/// What one checkpoint may take. A snapshot that would exceed them is not made, and the run says undo is not available for it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    /// New or changed files a snapshot may store (in a shadow repository's first one, every file).
    pub files: u64,
    /// Their bytes together.
    pub bytes: u64,
    /// The longest one snapshot may take; a run waits for its start checkpoint no longer than this.
    pub time: Duration,
}

impl Default for Limits {
    /// 10,000 files, 100 MB, 10 seconds.
    fn default() -> Limits {
        Limits { files: 10_000, bytes: 100_000_000, time: Duration::from_secs(10) }
    }
}

impl Limits {
    /// The defaults, or `NEKO_UNDO_MAX_FILES` and `NEKO_UNDO_MAX_BYTES` (tests).
    pub fn from_env() -> Limits {
        let d = Limits::default();
        let n = |k: &str, v: u64| std::env::var(k).ok().and_then(|s| s.parse().ok()).unwrap_or(v);
        Limits { files: n("NEKO_UNDO_MAX_FILES", d.files), bytes: n("NEKO_UNDO_MAX_BYTES", d.bytes), time: d.time }
    }
}

/// The kind of folder a checkpoint is of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A git repository's work tree (a task's worktree): checkpoints are kept in the repository.
    Repo,
    /// Any other folder: checkpoints are kept in a shadow repository Neko owns.
    Shadow,
}

impl Kind {
    pub fn name(self) -> &'static str {
        match self {
            Kind::Repo => "repo",
            Kind::Shadow => "shadow",
        }
    }

    pub fn from_name(s: &str) -> Kind {
        if s == "shadow" { Kind::Shadow } else { Kind::Repo }
    }
}

/// A folder checkpoints are taken of, and where they are kept.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Folder {
    pub kind: Kind,
    /// The work tree: the worktree's top folder, or the plain folder.
    pub work: PathBuf,
    /// The shadow repository (a bare repository under the data folder). None for a repository's own work tree.
    pub shadow: Option<PathBuf>,
}

/// Why no checkpoint was taken, or why undo or redo will not go ahead. Its words end a sentence ("Undo is not available for this
/// run: …").
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    NoCommits,
    /// In the middle of a merge, a rebase, a cherry-pick or a revert (the word).
    Midway(&'static str),
    IndexLocked,
    TooBig { files: u64, bytes: u64, limits: Limits },
    Slow(Duration),
    Gone,
    Failed(String),
}

impl std::fmt::Display for Refusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Refusal::NoCommits => f.write_str("this folder is a git repository with no commits yet"),
            Refusal::Midway(what) => write!(f, "the repository is in the middle of a {what}"),
            Refusal::IndexLocked => f.write_str("git is busy in this folder (its index is locked)"),
            Refusal::TooBig { files, bytes, limits } => write!(
                f,
                "the folder has {} new or changed files ({}), more than undo keeps ({} files or {})",
                thousands(*files),
                size_words(*bytes),
                thousands(limits.files),
                size_words(limits.bytes)
            ),
            Refusal::Slow(d) => write!(f, "taking the checkpoint took longer than {} seconds", d.as_secs()),
            Refusal::Gone => f.write_str("the task's folder is not there"),
            Refusal::Failed(why) => f.write_str(why),
        }
    }
}

/// `12345` as "12,345".
pub fn thousands(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Bytes in words: "512 bytes", "12.4 KB", "3.1 MB", "1.2 GB" (powers of 1000, as the Finder counts).
pub fn size_words(b: u64) -> String {
    match b {
        0..1_000 => format!("{b} byte{}", if b == 1 { "" } else { "s" }),
        1_000..1_000_000 => format!("{:.1} KB", b as f64 / 1e3),
        1_000_000..1_000_000_000 => format!("{:.1} MB", b as f64 / 1e6),
        _ => format!("{:.1} GB", b as f64 / 1e9),
    }
}

/// A snapshot taken: a commit on no branch, and its tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Snapshot {
    pub commit: String,
    pub tree: String,
    /// The repository's HEAD commit when it was taken (None in a shadow repository).
    pub head: Option<String>,
    /// Files in the snapshot.
    pub files: u64,
    /// New or changed files it stored, and their bytes.
    pub stored: u64,
    pub bytes: u64,
    pub took: Duration,
}

/// The variables a daemon started by a git hook or a terminal inside a repository may carry: none of them may steer these
/// commands to another repository, index or object store.
const INHERITED: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_COMMON_DIR",
    "GIT_NAMESPACE",
    "GIT_CEILING_DIRECTORIES",
    "GIT_DISCOVERY_ACROSS_FILESYSTEM",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_PREFIX",
    "GIT_QUARANTINE_PATH",
];

fn git_bin() -> PathBuf {
    find::find("git", None).unwrap_or_else(|| PathBuf::from("/usr/bin/git"))
}

/// A git command for `f`, with Neko's fixed settings; `index`: the temporary index it works on.
fn git(f: &Folder, index: Option<&Path>) -> Command {
    let mut c = Command::new(git_bin());
    c.current_dir(&f.work).stdin(Stdio::null()).stderr(Stdio::null());
    for k in INHERITED {
        c.env_remove(k);
    }
    c.env("GIT_TERMINAL_PROMPT", "0").env("GIT_OPTIONAL_LOCKS", "0").env("LC_ALL", "C");
    for kv in ["core.fsmonitor=false", "core.untrackedCache=false", "gc.auto=0", "maintenance.auto=false", "core.quotePath=false"] {
        c.arg("-c").arg(kv);
    }
    match &f.shadow {
        Some(g) => {
            c.arg("--git-dir").arg(g).arg("--work-tree").arg(&f.work);
        }
        None => {
            c.arg("-C").arg(&f.work);
        }
    }
    if let Some(i) = index {
        c.env("GIT_INDEX_FILE", i);
    }
    c
}

/// Runs a command until `until`: its output when it succeeded.
fn out_bytes(mut c: Command, until: Instant, args: &[&str]) -> Result<Option<Vec<u8>>, Refusal> {
    c.args(args);
    let left = until.saturating_duration_since(Instant::now());
    if left.is_zero() {
        return Err(Refusal::Slow(Duration::ZERO));
    }
    match find::run_for(c, left) {
        Some(o) => Ok(o.status.success().then_some(o.stdout)),
        None if Instant::now() >= until => Err(Refusal::Slow(Duration::ZERO)),
        None => Err(Refusal::Failed("git could not be run".into())),
    }
}

fn out(f: &Folder, index: Option<&Path>, until: Instant, args: &[&str]) -> Result<Option<String>, Refusal> {
    Ok(out_bytes(git(f, index), until, args)?.map(|b| String::from_utf8_lossy(&b).trim().to_owned()))
}

/// Runs and needs it to succeed; `what` says what failed.
fn must(f: &Folder, index: Option<&Path>, until: Instant, args: &[&str], what: &str) -> Result<String, Refusal> {
    out(f, index, until, args)?.ok_or_else(|| Refusal::Failed(format!("git could not {what}")))
}

/// A temporary index file, removed (with its lock) when dropped.
struct TempIndex(PathBuf);

impl TempIndex {
    fn new(tmp: &Path) -> Result<TempIndex, Refusal> {
        std::fs::create_dir_all(tmp).map_err(|e| Refusal::Failed(format!("Neko could not make its undo folder: {e}")))?;
        static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        Ok(TempIndex(tmp.join(format!("index-{}-{n}-{nanos}", std::process::id()))))
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let mut lock = self.0.clone().into_os_string();
        lock.push(".lock");
        let _ = std::fs::remove_file(lock);
    }
}

/// The repository's HEAD commit (None: no commits yet, or a shadow repository with none).
pub fn head(f: &Folder, until: Instant) -> Result<Option<String>, Refusal> {
    out(f, None, until, &["rev-parse", "--verify", "--quiet", "HEAD^{commit}"])
}

fn git_path(f: &Folder, until: Instant, name: &str) -> Result<PathBuf, Refusal> {
    let p = must(f, None, until, &["rev-parse", "--git-path", name], "find the repository's own folder")?;
    let p = PathBuf::from(p);
    Ok(if p.is_absolute() { p } else { f.work.join(p) })
}

/// What a repository is in the middle of, if anything: a merge, a rebase, a cherry-pick, a revert.
pub fn midway(f: &Folder, until: Instant) -> Result<Option<&'static str>, Refusal> {
    if f.kind == Kind::Shadow {
        return Ok(None);
    }
    for (name, what) in [("MERGE_HEAD", "merge"), ("rebase-merge", "rebase"), ("rebase-apply", "rebase"), ("CHERRY_PICK_HEAD", "cherry-pick"), ("REVERT_HEAD", "revert")] {
        if git_path(f, until, name)?.exists() {
            return Ok(Some(what));
        }
    }
    Ok(None)
}

/// Whether the repository's own index is locked: another git command is writing it.
pub fn index_locked(f: &Folder, until: Instant) -> Result<bool, Refusal> {
    if f.kind == Kind::Shadow {
        return Ok(false);
    }
    Ok(git_path(f, until, "index.lock")?.exists())
}

/// Makes the shadow repository if it is not there yet: a bare repository Neko owns, its own settings written into its own config.
fn ensure_shadow(f: &Folder, until: Instant) -> Result<(), Refusal> {
    let Some(g) = &f.shadow else { return Ok(()) };
    if g.join("HEAD").is_file() {
        return Ok(());
    }
    if let Some(parent) = g.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Refusal::Failed(format!("Neko could not make its undo folder: {e}")))?;
    }
    let mut c = Command::new(git_bin());
    c.stdin(Stdio::null()).stderr(Stdio::null()).current_dir(g.parent().unwrap_or(Path::new("/")));
    for k in INHERITED {
        c.env_remove(k);
    }
    c.env("GIT_TERMINAL_PROMPT", "0");
    out_bytes(c, until, &["init", "--quiet", "--bare", &g.to_string_lossy()])?.ok_or_else(|| Refusal::Failed("git could not make Neko's shadow repository".into()))?;
    // Byte for byte: no line-ending conversion between the folder and its checkpoints.
    for (k, v) in [("core.autocrlf", "false"), ("core.safecrlf", "false"), ("gc.auto", "0")] {
        let mut c = Command::new(git_bin());
        c.stdin(Stdio::null()).stderr(Stdio::null());
        for k in INHERITED {
            c.env_remove(k);
        }
        out_bytes(c, until, &["--git-dir", &g.to_string_lossy(), "config", k, v])?;
    }
    let _ = std::fs::create_dir_all(g.join("info"));
    let _ = std::fs::write(g.join("info").join("exclude"), ".DS_Store\n");
    Ok(())
}

/// The shadow's own index: kept between snapshots so the next one only reads what changed.
fn shadow_index(f: &Folder) -> Option<PathBuf> {
    f.shadow.as_ref().map(|g| g.join("neko-index"))
}

/// Fills the temporary index: a copy of the repository's own (its stat data makes the snapshot quick; reading it changes
/// nothing), else HEAD's tree, else nothing.
fn seed(f: &Folder, index: &TempIndex, head: Option<&str>, until: Instant) -> Result<(), Refusal> {
    let own = match f.kind {
        Kind::Repo => Some(git_path(f, until, "index")?),
        Kind::Shadow => shadow_index(f),
    };
    if let Some(own) = own.filter(|p| p.is_file())
        && std::fs::copy(&own, &index.0).is_ok()
    {
        return Ok(());
    }
    if let Some(h) = head {
        must(f, Some(&index.0), until, &["read-tree", h], "read the current commit")?;
    }
    Ok(())
}

/// What a snapshot would store: new or changed files (ignored files aside) and their bytes.
fn pending(f: &Folder, index: &TempIndex, until: Instant) -> Result<(u64, u64), Refusal> {
    let raw = out_bytes(git(f, Some(&index.0)), until, &["status", "--porcelain=v1", "-z", "--untracked-files=all", "--no-renames", "--ignore-submodules=all"])?
        .ok_or_else(|| Refusal::Failed("git could not read the folder".into()))?;
    let (mut files, mut bytes) = (0u64, 0u64);
    for entry in raw.split(|b| *b == 0).filter(|e| e.len() > 3) {
        let path = String::from_utf8_lossy(&entry[3..]);
        files += 1;
        if entry[1] != b'D'
            && let Ok(m) = std::fs::symlink_metadata(f.work.join(path.as_ref()))
            && m.is_file()
        {
            bytes += m.len();
        }
    }
    Ok((files, bytes))
}

/// The folder as a tree, in the temporary index: (tree, files stored, their bytes), refused past the limits.
fn tree_of(f: &Folder, index: &TempIndex, head: Option<&str>, limits: &Limits, until: Instant) -> Result<(String, u64, u64), Refusal> {
    seed(f, index, head, until)?;
    let (files, bytes) = pending(f, index, until)?;
    if files > limits.files || bytes > limits.bytes {
        return Err(Refusal::TooBig { files, bytes, limits: *limits });
    }
    must(f, Some(&index.0), until, &["add", "-A"], "read the folder's files")?;
    let tree = must(f, Some(&index.0), until, &["write-tree"], "write the folder's tree")?;
    Ok((tree, files, bytes))
}

/// Refuses what undo must not touch: a repository mid-merge or mid-rebase. A missing folder is gone.
fn ready(f: &Folder, until: Instant) -> Result<(), Refusal> {
    if !f.work.is_dir() {
        return Err(Refusal::Gone);
    }
    if let Some(what) = midway(f, until)? {
        return Err(Refusal::Midway(what));
    }
    Ok(())
}

/// Takes a snapshot of the folder: a commit on no branch (its parent HEAD, in a repository), not yet under a ref. The index,
/// HEAD, the branch, the stash and the working files are untouched. A repository with no commits yet is refused.
pub fn take(f: &Folder, tmp: &Path, message: &str, limits: &Limits) -> Result<Snapshot, Refusal> {
    let began = Instant::now();
    let until = began + limits.time;
    let slow = |e: Refusal| if let Refusal::Slow(_) = e { Refusal::Slow(limits.time) } else { e };
    let run = || -> Result<Snapshot, Refusal> {
        ready(f, until)?;
        ensure_shadow(f, until)?;
        let head = head(f, until)?;
        if f.kind == Kind::Repo && head.is_none() {
            return Err(Refusal::NoCommits);
        }
        let index = TempIndex::new(tmp)?;
        let (tree, stored, bytes) = tree_of(f, &index, head.as_deref(), limits, until)?;
        let files = out_bytes(git(f, Some(&index.0)), until, &["ls-files", "-z"])?.map_or(0, |b| b.split(|c| *c == 0).filter(|e| !e.is_empty()).count() as u64);
        let mut c = git(f, None);
        c.env("GIT_AUTHOR_NAME", "Neko").env("GIT_AUTHOR_EMAIL", "neko@neko.invalid").env("GIT_COMMITTER_NAME", "Neko").env("GIT_COMMITTER_EMAIL", "neko@neko.invalid");
        let mut args = vec!["commit-tree", tree.as_str(), "--no-gpg-sign", "-m", message];
        if let Some(h) = &head {
            args.extend(["-p", h.as_str()]);
        }
        let commit = out_bytes(c, until, &args)?.map(|b| String::from_utf8_lossy(&b).trim().to_owned()).filter(|c| !c.is_empty()).ok_or_else(|| Refusal::Failed("git could not write the checkpoint".into()))?;
        if let Some(keep) = shadow_index(f) {
            let _ = std::fs::copy(&index.0, keep);
        }
        Ok(Snapshot { commit, tree, head, files, stored, bytes, took: began.elapsed() })
    };
    run().map_err(slow)
}

/// The folder's tree as it is now, written to the object store only (no commit, no ref): to compare with a checkpoint's.
pub fn tree_now(f: &Folder, tmp: &Path, limits: &Limits) -> Result<String, Refusal> {
    let until = Instant::now() + limits.time;
    ready(f, until)?;
    ensure_shadow(f, until)?;
    let head = head(f, until)?;
    let index = TempIndex::new(tmp)?;
    tree_of(f, &index, head.as_deref(), limits, until).map(|t| t.0)
}

/// Puts a checkpoint under Neko's ref.
pub fn keep(f: &Folder, refname: &str, commit: &str) -> Result<(), Refusal> {
    if !refname.starts_with(REF_ROOT) {
        return Err(Refusal::Failed("Neko keeps checkpoints only under its own refs".into()));
    }
    must(f, None, Instant::now() + Duration::from_secs(30), &["update-ref", refname, commit], "keep the checkpoint").map(|_| ())
}

/// Neko's refs under `prefix` (`refs/neko/checkpoints/<task>/`), as they are now.
pub fn refs_under(f: &Folder, prefix: &str) -> Vec<String> {
    if !prefix.starts_with(REF_ROOT) || !f.work.is_dir() {
        return Vec::new();
    }
    out(f, None, Instant::now() + Duration::from_secs(30), &["for-each-ref", "--format=%(refname)", prefix]).ok().flatten().map(|s| s.lines().map(str::to_owned).collect()).unwrap_or_default()
}

/// Deletes Neko's refs under `prefix`. Commits stay in the object store until git's own gc; nothing else is touched.
pub fn drop_refs(f: &Folder, prefix: &str) -> usize {
    let until = Instant::now() + Duration::from_secs(60);
    refs_under(f, prefix).iter().filter(|r| r.starts_with(REF_ROOT) && out(f, None, until, &["update-ref", "-d", r]).ok().flatten().is_some()).count()
}

/// Deletes one of Neko's refs.
pub fn drop_ref(f: &Folder, refname: &str) -> bool {
    refname.starts_with(REF_ROOT) && f.work.is_dir() && out(f, None, Instant::now() + Duration::from_secs(30), &["update-ref", "-d", refname]).ok().flatten().is_some()
}

/// Frees what a shadow repository no longer refers to (after its refs went). Neko's own repository only.
pub fn shadow_gc(f: &Folder) {
    if f.kind == Kind::Shadow && f.shadow.as_ref().is_some_and(|g| g.join("HEAD").is_file()) {
        let _ = out(f, None, Instant::now() + Duration::from_secs(120), &["gc", "--quiet", "--prune=now"]);
    }
}

/// Whether the object store still has `commit` (a checkpoint whose objects were pruned cannot be restored).
pub fn has(f: &Folder, commit: &str) -> bool {
    out(f, None, Instant::now() + Duration::from_secs(30), &["cat-file", "-e", &format!("{commit}^{{commit}}")]).ok().flatten().is_some()
}

/// The paths that differ between two trees.
pub fn differing(f: &Folder, a: &str, b: &str) -> Result<Vec<String>, Refusal> {
    let raw = out_bytes(git(f, None), Instant::now() + Duration::from_secs(60), &["diff-tree", "-r", "-z", "--name-only", "--no-renames", a, b])?
        .ok_or_else(|| Refusal::Failed("git could not compare the checkpoints".into()))?;
    Ok(raw.split(|c| *c == 0).filter(|e| !e.is_empty()).map(|e| String::from_utf8_lossy(e).into_owned()).collect())
}

/// Writes the working files from tree `from` (what the folder holds now, checked by the caller) to tree `to`: added, edited,
/// deleted, renamed, mode-changed and binary files, in every subfolder. Only the temporary index is used; HEAD, the branch and the
/// repository's index stay as they were. A file that changed meanwhile makes git refuse, and nothing is written.
pub fn restore(f: &Folder, tmp: &Path, from: &str, to: &str, limits: &Limits) -> Result<(), Refusal> {
    let until = Instant::now() + limits.time.max(Duration::from_secs(60));
    ready(f, until)?;
    if index_locked(f, until)? {
        return Err(Refusal::IndexLocked);
    }
    let index = TempIndex::new(tmp)?;
    must(f, Some(&index.0), until, &["read-tree", from], "read the folder's checkpoint")?;
    // Stat data for the files as they are, so git knows they match `from`. It says 1 when something differs; the next step refuses.
    let _ = out(f, Some(&index.0), until, &["update-index", "-q", "--refresh"])?;
    must(f, Some(&index.0), until, &["read-tree", "-m", "-u", "--no-recurse-submodules", from, to], "put the files back (a file changed meanwhile, or is in the way)")?;
    Ok(())
}

/// What changed between two trees, file by file, without lines: statuses, renames, counts, binary files with their sizes.
pub fn files(f: &Folder, a: &str, b: &str) -> Result<Vec<DiffFile>, Refusal> {
    let until = Instant::now() + Duration::from_secs(60);
    let raw = out_bytes(git(f, None), until, &["diff-tree", "-r", "-z", "-M", "--raw", a, b])?.ok_or_else(|| Refusal::Failed("git could not compare the checkpoints".into()))?;
    let num = out_bytes(git(f, None), until, &["diff-tree", "-r", "-z", "-M", "--numstat", a, b])?.ok_or_else(|| Refusal::Failed("git could not count the changes".into()))?;
    let mut list = parse_raw(&raw);
    for (file, (added, removed)) in list.iter_mut().zip(parse_numstat(&num)) {
        match (added, removed) {
            (Some(x), Some(y)) => {
                file.0.added = x;
                file.0.removed = y;
            }
            _ => file.0.binary = true,
        }
    }
    let mut out = Vec::new();
    for (mut file, old_blob, new_blob) in list {
        if file.binary {
            // An all-zero id is the side where the file was not.
            let size = |blob: &str| if blob.trim_start_matches('0').is_empty() { None } else { self::out(f, None, until, &["cat-file", "-s", blob]).ok().flatten().and_then(|s| s.parse().ok()) };
            file.old_size = size(&old_blob);
            file.new_size = size(&new_blob);
        }
        if file.status == FileStatus::Submodule {
            file.binary = false;
        }
        out.push(file);
    }
    Ok(out)
}

/// `:old_mode new_mode old_blob new_blob status\0path\0[path2\0]`, each with its two blob ids.
fn parse_raw(raw: &[u8]) -> Vec<(DiffFile, String, String)> {
    let mut parts = raw.split(|c| *c == 0).map(|p| String::from_utf8_lossy(p).into_owned());
    let mut out = Vec::new();
    while let Some(head) = parts.next() {
        let Some(meta) = head.strip_prefix(':') else { continue };
        let f: Vec<&str> = meta.split(' ').collect();
        if f.len() < 5 {
            continue;
        }
        let (old_mode, new_mode, old_blob, new_blob, code) = (f[0], f[1], f[2], f[3], f[4]);
        let first = parts.next().unwrap_or_default();
        let letter = code.chars().next().unwrap_or('M');
        let (path, old_path) = if matches!(letter, 'R' | 'C') { (parts.next().unwrap_or_default(), Some(first)) } else { (first, None) };
        let status = if old_mode == "160000" || new_mode == "160000" {
            FileStatus::Submodule
        } else {
            match letter {
                'A' => FileStatus::Added,
                'D' => FileStatus::Deleted,
                'R' => FileStatus::Renamed,
                'C' => FileStatus::Copied,
                'T' => FileStatus::TypeChanged,
                _ => FileStatus::Modified,
            }
        };
        let modes = (old_mode != new_mode && old_mode != "000000" && new_mode != "000000").then(|| (old_mode.to_owned(), new_mode.to_owned()));
        let file = DiffFile {
            path,
            old_path,
            status,
            added: 0,
            removed: 0,
            binary: false,
            old_size: None,
            new_size: None,
            old_mode: modes.as_ref().map(|m| m.0.clone()),
            new_mode: modes.map(|m| m.1),
        };
        out.push((file, old_blob.to_owned(), new_blob.to_owned()));
    }
    out
}

/// `added\tremoved\tpath\0`, or for a rename `added\tremoved\t\0old\0new\0`; `-` for a binary file.
fn parse_numstat(raw: &[u8]) -> Vec<(Option<u32>, Option<u32>)> {
    let mut parts = raw.split(|c| *c == 0).map(|p| String::from_utf8_lossy(p).into_owned());
    let mut out = Vec::new();
    while let Some(line) = parts.next() {
        if line.is_empty() {
            continue;
        }
        let mut f = line.splitn(3, '\t');
        let added = f.next().and_then(|n| n.parse().ok());
        let removed = f.next().and_then(|n| n.parse().ok());
        if f.next().unwrap_or_default().is_empty() {
            // A rename: the old and new paths follow.
            parts.next();
            parts.next();
        }
        out.push((added, removed));
    }
    out
}

/// Counts for the run card: files, lines added and removed between two trees.
pub fn changes(f: &Folder, a: &str, b: &str) -> Result<Changes, Refusal> {
    let list = files(f, a, b)?;
    Ok(Changes { files: list.len() as u32, added: list.iter().map(|x| x.added).sum(), removed: list.iter().map(|x| x.removed).sum() })
}

/// One file's unified diff between two trees: its hunks' lines, each with its mark first. `paths`: the file, and where it was
/// before when it was renamed (so git sees the rename).
pub fn patch(f: &Folder, a: &str, b: &str, paths: &[&str]) -> Result<Vec<String>, Refusal> {
    let mut args = vec!["diff-tree", "-p", "-M", "--no-color", "--no-ext-diff", "-U3", a, b, "--"];
    args.extend(paths);
    let raw = out_bytes(git(f, None), Instant::now() + Duration::from_secs(60), &args)?.ok_or_else(|| Refusal::Failed("git could not show the changes".into()))?;
    let text = String::from_utf8_lossy(&raw);
    let mut lines = Vec::new();
    let mut in_hunks = false;
    for l in text.lines() {
        if l.starts_with("diff --git ") {
            in_hunks = false;
            continue;
        }
        if l.starts_with("@@") {
            in_hunks = true;
        }
        if in_hunks {
            lines.push(l.to_owned());
        }
    }
    Ok(lines)
}

/// Where a task's shadow repository lives: `<data>/undo/<short id>.git`.
pub fn shadow_dir(data_dir: &Path, task: &TaskId) -> PathBuf {
    data_dir.join(dfiles::UNDO).join(format!("{}.git", crate::workplace::short_id(task)))
}

/// Where temporary index files go: Neko's own folder, never the task's.
pub fn tmp_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(dfiles::UNDO).join("tmp")
}

/// The folder a task's checkpoints are of: its worktree in a repository, else its folder with a shadow repository. A plain folder
/// that is a git repository with no commits yet is refused rather than guessed at.
pub fn folder_for(data_dir: &Path, task: &TaskId, p: &Place) -> Result<Folder, Refusal> {
    if p.branch.is_some() && p.repo.is_some() {
        return Ok(Folder { kind: Kind::Repo, work: crate::workplace::worktree_root(data_dir, task), shadow: None });
    }
    let work = PathBuf::from(&p.workdir);
    if p.plain {
        let probe = Folder { kind: Kind::Repo, work: work.clone(), shadow: None };
        let until = Instant::now() + Duration::from_secs(10);
        if out(&probe, None, until, &["rev-parse", "--is-inside-work-tree"]).ok().flatten().as_deref() == Some("true") && head(&probe, until).ok().flatten().is_none() {
            return Err(Refusal::NoCommits);
        }
    }
    Ok(Folder { kind: Kind::Shadow, work, shadow: Some(shadow_dir(data_dir, task)) })
}

/// The size of a folder on disk, in bytes (files only, links not followed).
pub fn disk_bytes(dir: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.flatten()
        .map(|e| match e.file_type() {
            Ok(t) if t.is_dir() => disk_bytes(&e.path()),
            Ok(t) if t.is_file() => e.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_read_as_words() {
        assert_eq!(thousands(12_345), "12,345");
        assert_eq!(thousands(999), "999");
        assert_eq!(size_words(100_000_000), "100.0 MB");
        assert_eq!(size_words(1), "1 byte");
    }

    #[test]
    fn raw_and_numstat_lines_are_read_with_renames() {
        let raw = b":100644 100644 aaa bbb M\0src/a.rs\0:000000 100644 000 ccc A\0new.txt\0:100644 100644 ddd ddd R100\0old.md\0docs/new.md\0:100644 100755 eee eee M\0run.sh\0";
        let files = parse_raw(raw);
        assert_eq!(files.len(), 4);
        assert_eq!(files[2].0.path, "docs/new.md");
        assert_eq!(files[2].0.old_path.as_deref(), Some("old.md"));
        assert_eq!(files[2].0.status, FileStatus::Renamed);
        assert_eq!(files[3].0.new_mode.as_deref(), Some("100755"));
        assert_eq!(files[1].0.old_mode, None, "a new file has no mode change");
        let num = parse_numstat(b"3\t1\tsrc/a.rs\x00-\t-\tnew.txt\x000\t0\t\x00old.md\x00docs/new.md\x000\t0\trun.sh\x00");
        assert_eq!(num, vec![(Some(3), Some(1)), (None, None), (Some(0), Some(0)), (Some(0), Some(0))]);
    }
}
