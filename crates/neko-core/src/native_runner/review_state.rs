//! Host evidence for writable, independent reviewers. No model-produced
//! evidence participates in equality. Raw content also covers dirty files and
//! index flags that would hide edits from `git diff`.

use super::*;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};

const MAX_FILES: usize = 100_000;
const MAX_BYTES: u64 = 1024 * 1024 * 1024;

/// Opaque, host-observed repository identity, HEAD, index, control metadata
/// and source content. Compare snapshots before and after a reviewer with
/// `==`; any capture error must also reject the review.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewState {
    repository: PathBuf,
    git_dir: PathBuf,
    common_dir: PathBuf,
    head: String,
    head_tree: String,
    index: String,
    index_flags: Vec<(PathBuf, u32)>,
    config: [u8; 32],
    metadata: BTreeMap<PathBuf, Option<FileState>>,
    sources: BTreeMap<PathBuf, Option<FileState>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FileState {
    mode: u32,
    digest: [u8; 32],
}

// Used only while reading a file, never for before/after semantic equality.
// Formatters can rewrite identical bytes, and Git can refresh index stat data.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    mode: u32,
    size: u64,
    mtime: (i64, i64),
    ctime: (i64, i64),
}

impl From<&fs::Metadata> for Stamp {
    fn from(m: &fs::Metadata) -> Self {
        Self {
            device: m.dev(),
            inode: m.ino(),
            mode: m.mode(),
            size: m.len(),
            mtime: (m.mtime(), m.mtime_nsec()),
            ctime: (m.ctime(), m.ctime_nsec()),
        }
    }
}

struct Budget<'a> {
    cancel: &'a AtomicBool,
    start: Instant,
    bytes: u64,
    files: usize,
}

impl Budget<'_> {
    fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Acquire) {
            return Err("Review snapshot cancelled".into());
        }
        if self.start.elapsed() > Duration::from_secs(60)
            || self.bytes > MAX_BYTES
            || self.files > MAX_FILES
        {
            return Err("Review snapshot exceeded its time or size limit".into());
        }
        Ok(())
    }
}

/// Capture a stable repository state outside the agent sandbox, using the
/// hardened host Git adapter and no hooks, filters, textconv or external diff.
/// All tracked files (including staged deletions and skip-worktree entries)
/// plus nonignored untracked files are hashed. Ignored caches/dependencies
/// are omitted; changes to ignore rules and Git control metadata are not.
///
/// Two complete observations must agree. Unsupported submodules, special
/// files, unsafe metadata paths, races, cancellation and oversized evidence
/// fail closed. This is before/after integrity evidence, not a transactional
/// filesystem monitor or a security boundary against a full-access process.
pub fn capture_review_state(directory: &Path, cancel: &AtomicBool) -> Result<ReviewState, String> {
    let mut budget = Budget {
        cancel,
        start: Instant::now(),
        bytes: 0,
        files: 0,
    };
    budget.check()?;
    let directory = directory
        .canonicalize()
        .map_err(|e| format!("Review directory unavailable: {e}"))?;
    let first = capture_once(&directory, &mut budget)?;
    let second = capture_once(&directory, &mut budget)?;
    if first != second {
        return Err("Repository changed during review snapshot capture".into());
    }
    Ok(second)
}

fn absolute_git_path(
    directory: &Path,
    option: &str,
    cancel: &AtomicBool,
) -> Result<PathBuf, String> {
    let value = git_output(
        directory,
        &["rev-parse", "--path-format=absolute", option],
        cancel,
    )?;
    let path = PathBuf::from(value.trim_end_matches('\n'));
    if !path.is_absolute() || path.as_os_str().as_bytes().contains(&b'\n') {
        return Err("Invalid repository metadata path".into());
    }
    path.canonicalize()
        .map_err(|e| format!("Repository metadata unavailable: {e}"))
}

fn capture_once(directory: &Path, budget: &mut Budget<'_>) -> Result<ReviewState, String> {
    budget.check()?;
    let cancel = budget.cancel;
    let repository = absolute_git_path(directory, "--show-toplevel", cancel)?;
    if !directory.starts_with(&repository) {
        return Err("Review directory is outside the repository root".into());
    }
    let git_dir = absolute_git_path(&repository, "--absolute-git-dir", cancel)?;
    let common_dir = absolute_git_path(&repository, "--git-common-dir", cancel)?;
    let head = head(&repository, cancel)?;
    let head_tree = git_output(
        &repository,
        &["ls-tree", "-r", "-z", "--full-tree", "HEAD"],
        cancel,
    )?;
    let index = git_output(&repository, &["ls-files", "--stage", "-z"], cancel)?;
    let index_flags = semantic_index_flags(&git_output(
        &repository,
        &["ls-files", "--debug", "-z"],
        cancel,
    )?)?;
    let config = git_output(&repository, &["config", "--null", "--list"], cancel)?;
    let mut metadata = BTreeMap::new();
    // Linked worktrees store .git as a file and have their own HEAD/index/log.
    let dotgit = repository.join(".git");
    if fs::symlink_metadata(&dotgit)
        .map_err(|e| e.to_string())?
        .is_dir()
    {
        if dotgit.canonicalize().map_err(|e| e.to_string())? != git_dir {
            return Err("Repository .git directory changed identity".into());
        }
    } else {
        add_metadata(&dotgit, budget, &mut metadata)?;
    }
    for name in [
        "HEAD",
        "commondir",
        "gitdir",
        "config.worktree",
        "ORIG_HEAD",
        "logs/HEAD",
    ] {
        add_metadata(&git_dir.join(name), budget, &mut metadata)?;
    }
    // Validate raw index storage without comparing stat-cache bytes. The
    // staged manifest and semantic flags above are the equality evidence.
    check_ancestors(&git_dir.join("index"))?;
    fingerprint(&git_dir.join("index"), budget, false)?;
    for name in [
        "config",
        "packed-refs",
        "shallow",
        "info",
        "hooks",
        "refs/replace",
    ] {
        add_metadata(&common_dir.join(name), budget, &mut metadata)?;
    }
    // Sparse checkout and per-worktree excludes, if present.
    if common_dir != git_dir {
        add_metadata(&git_dir.join("info"), budget, &mut metadata)?;
    }
    let reference = git_output(
        &repository,
        &["rev-parse", "--symbolic-full-name", "HEAD"],
        cancel,
    )?;
    let reference = reference.trim();
    if reference != "HEAD" {
        validate_relative(Path::new(reference))?;
        if !reference.starts_with("refs/") {
            return Err("Invalid HEAD reference".into());
        }
        add_metadata(&common_dir.join(reference), budget, &mut metadata)?;
        add_metadata(
            &common_dir.join("logs").join(reference),
            budget,
            &mut metadata,
        )?;
    }
    // A split index keeps some entries in a separate file.
    for entry in fs::read_dir(&git_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if entry.file_name().as_bytes().starts_with(b"sharedindex.") {
            fingerprint(&entry.path(), budget, false)?;
        }
    }
    // Snapshot external ignore/attribute files as well as their config values.
    for name in ["core.excludesfile", "core.attributesfile"] {
        if config
            .split('\0')
            .any(|record| record.split_once('\n').is_some_and(|(key, _)| key == name))
        {
            let paths = git_output(
                &repository,
                &["config", "--path", "--get-all", name],
                cancel,
            )?;
            for path in paths.lines() {
                let path = Path::new(path);
                add_metadata(&repository.join(path), budget, &mut metadata)?;
            }
        }
    }
    // Git's implicit global ignore path need not appear in config.
    let xdg = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")));
    if let Some(xdg) = xdg.filter(|p| p.is_absolute()) {
        for name in ["ignore", "attributes"] {
            add_metadata(&xdg.join("git").join(name), budget, &mut metadata)?;
        }
    }
    let mut paths = BTreeSet::new();
    for listing in [&head_tree, &index] {
        for record in listing.split('\0').filter(|s| !s.is_empty()) {
            let (header, file) = record.split_once('\t').ok_or("Invalid Git file manifest")?;
            if header.starts_with("160000 ") || header.starts_with("040000 ") {
                return Err("Review snapshots require expanded source files; submodules and sparse directory entries are unsupported".into());
            }
            paths.insert(PathBuf::from(file));
        }
    }
    let untracked = git_output(
        &repository,
        &["ls-files", "--others", "--exclude-standard", "-z"],
        cancel,
    )?;
    paths.extend(
        untracked
            .split('\0')
            .filter(|s| !s.is_empty())
            .map(PathBuf::from),
    );
    let mut sources = BTreeMap::new();
    for path in paths {
        validate_relative(&path)?;
        check_ancestors(&repository.join(&path))?;
        sources.insert(
            path.clone(),
            fingerprint(&repository.join(&path), budget, true)?,
        );
    }
    budget.check()?;
    Ok(ReviewState {
        repository,
        git_dir,
        common_dir,
        head,
        head_tree,
        index,
        index_flags,
        config: Sha256::digest(config.as_bytes()).into(),
        metadata,
        sources,
    })
}

fn validate_relative(path: &Path) -> Result<(), String> {
    if path.as_os_str().is_empty()
        || path
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err("Unsafe repository source path".into());
    }
    Ok(())
}

// A tracked directory replaced by a symlink must not redirect host reads.
fn check_ancestors(path: &Path) -> Result<(), String> {
    for parent in path.ancestors().skip(1) {
        match fs::symlink_metadata(parent) {
            Ok(m) if !m.is_dir() => {
                return Err(format!("Unsafe snapshot ancestor: {}", parent.display()));
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(e) => return Err(format!("Cannot inspect snapshot ancestor: {e}")),
        }
    }
    Ok(())
}

fn add_metadata(
    path: &Path,
    budget: &mut Budget<'_>,
    out: &mut BTreeMap<PathBuf, Option<FileState>>,
) -> Result<(), String> {
    budget.check()?;
    check_ancestors(path)?;
    match fs::symlink_metadata(path) {
        Ok(m) if m.is_dir() => {
            // Directory times change when Git creates harmless lock files;
            // capture the actual control files instead.
            for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
                add_metadata(&entry.map_err(|e| e.to_string())?.path(), budget, out)?;
            }
        }
        _ => {
            out.insert(path.to_owned(), fingerprint(path, budget, false)?);
        }
    }
    Ok(())
}

fn fingerprint(
    path: &Path,
    budget: &mut Budget<'_>,
    allow_symlink: bool,
) -> Result<Option<FileState>, String> {
    budget.files += 1;
    budget.check()?;
    let before = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(format!(
                "Cannot inspect review file {}: {e}",
                path.display()
            ));
        }
    };
    let stamp = Stamp::from(&before);
    let mut hash = Sha256::new();
    if before.file_type().is_symlink() && allow_symlink {
        hash.update(
            fs::read_link(path)
                .map_err(|e| e.to_string())?
                .as_os_str()
                .as_bytes(),
        );
    } else if before.is_file() {
        // Do not follow a symlink or block on a FIFO substituted after lstat.
        let mut file = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| format!("Cannot read review file {}: {e}", path.display()))?;
        if Stamp::from(&file.metadata().map_err(|e| e.to_string())?) != stamp {
            return Err("Review file changed before reading".into());
        }
        let mut buffer = [0; 64 * 1024];
        loop {
            budget.check()?;
            let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            budget.bytes += n as u64;
            hash.update(&buffer[..n]);
        }
        if Stamp::from(&file.metadata().map_err(|e| e.to_string())?) != stamp {
            return Err("Review file changed while reading".into());
        }
    } else {
        return Err(format!(
            "Unsupported or redirected review file: {}",
            path.display()
        ));
    }
    if Stamp::from(&fs::symlink_metadata(path).map_err(|e| e.to_string())?) != stamp {
        return Err("Review file changed during capture".into());
    }
    budget.check()?;
    Ok(Some(FileState {
        mode: stamp.mode,
        digest: hash.finalize().into(),
    }))
}

fn semantic_index_flags(mut debug: &str) -> Result<Vec<(PathBuf, u32)>, String> {
    let mut result = Vec::new();
    while !debug.is_empty() {
        let (path, remaining) = debug
            .split_once('\0')
            .ok_or("Invalid index debug filename")?;
        debug = remaining;
        // Git --debug emits these five metadata lines after each NUL-terminated
        // name. Do not split on path newlines, or retain volatile stat fields.
        let mut flags = None;
        for expected in ["  ctime: ", "  mtime: ", "  dev: ", "  uid: ", "  size: "] {
            let (line, remaining) = debug
                .split_once('\n')
                .ok_or("Incomplete index debug metadata")?;
            if !line.starts_with(expected) {
                return Err("Unknown index debug metadata format".into());
            }
            if let Some((_, value)) = line.split_once("\tflags: ") {
                flags = Some(u32::from_str_radix(value, 16).map_err(|_| "Invalid index flags")?);
            }
            debug = remaining;
        }
        // CE_VALID (assume-unchanged), CE_INTENT_TO_ADD, CE_SKIP_WORKTREE.
        // Stage/object/mode are already covered by ls-files --stage. Ignore
        // cache bookkeeping such as CE_FSMONITOR_VALID and name-length bits.
        result.push((
            PathBuf::from(path),
            flags.ok_or("Missing index flags")? & 0x6000_8000,
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) {
        let result = git_command(repo).args(args).output().unwrap();
        assert!(
            result.status.success(),
            "{args:?}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }

    fn commit(repo: &Path) {
        git(
            repo,
            &[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.invalid",
                "commit",
                "--allow-empty",
                "-qm",
                "fixture",
            ],
        );
    }

    fn repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git(dir.path(), &["init", "-q"]);
        fs::write(dir.path().join(".gitignore"), "cache/\nnode_modules/\n").unwrap();
        fs::write(dir.path().join("source.rs"), "original\n").unwrap();
        git(dir.path(), &["add", "."]);
        commit(dir.path());
        dir
    }

    fn capture(repo: &Path) -> ReviewState {
        capture_review_state(repo, &AtomicBool::new(false)).unwrap()
    }

    #[test]
    fn review_state_allows_ignored_cache_writes_and_read_only_git() {
        let dir = repository();
        let repo = dir.path();
        fs::write(repo.join("source.rs"), "pre-existing dirty source\n").unwrap();
        fs::write(repo.join("untracked.rs"), "pre-existing untracked source\n").unwrap();
        let before = capture(repo);
        fs::create_dir(repo.join("cache")).unwrap();
        fs::write(repo.join("cache/test-results"), "passed\n").unwrap();
        fs::create_dir(repo.join("node_modules")).unwrap();
        fs::write(repo.join("node_modules/dependency.js"), "installed\n").unwrap();
        git_output(repo, &["status", "--porcelain"], &AtomicBool::new(false)).unwrap();
        task_patch(repo, "HEAD", &AtomicBool::new(false)).unwrap();
        assert_eq!(before, capture(repo));
        fs::write(repo.join("cache/test-results"), "updated\n").unwrap();
        assert_eq!(before, capture(repo));
    }

    #[test]
    fn review_state_allows_noop_formatter_rewrites_and_index_stat_refreshes() {
        let dir = repository();
        let repo = dir.path();
        let before = capture(repo);
        let raw_index = fs::read(repo.join(".git/index")).unwrap();
        // An in-place no-op formatter write changes timestamps but not code.
        fs::write(repo.join("source.rs"), "original\n").unwrap();
        assert_eq!(before, capture(repo));
        // Atomic replacement also changes inode, without changing source state.
        fs::write(repo.join("replacement"), "original\n").unwrap();
        fs::rename(repo.join("replacement"), repo.join("source.rs")).unwrap();
        assert_eq!(before, capture(repo));
        git(repo, &["update-index", "--refresh"]);
        assert_ne!(
            raw_index,
            fs::read(repo.join(".git/index")).unwrap(),
            "fixture refreshed index stat data"
        );
        assert_eq!(before, capture(repo));
    }

    #[test]
    fn review_state_distinguishes_intent_to_add_from_staged_empty_files() {
        let dir = repository();
        let repo = dir.path();
        fs::write(repo.join("empty"), "").unwrap();
        git(repo, &["add", "--intent-to-add", "empty"]);
        let intent = capture(repo);
        git(repo, &["add", "empty"]);
        let staged = capture(repo);
        assert_eq!(
            intent.index, staged.index,
            "both entries have the empty blob id"
        );
        assert_ne!(intent, staged, "intent-to-add is semantic index evidence");
    }

    #[test]
    fn review_state_allows_git_status_to_refresh_raw_index() {
        let dir = repository();
        let repo = dir.path();
        let before = capture(repo);
        let raw_index = fs::read(repo.join(".git/index")).unwrap();
        fs::write(repo.join("replacement"), "original\n").unwrap();
        fs::rename(repo.join("replacement"), repo.join("source.rs")).unwrap();
        let status = git_command(repo)
            .env("GIT_OPTIONAL_LOCKS", "1")
            .args(["status", "--porcelain"])
            .output()
            .unwrap();
        assert!(status.status.success());
        assert!(status.stdout.is_empty());
        assert_ne!(
            raw_index,
            fs::read(repo.join(".git/index")).unwrap(),
            "git status actually refreshed its index"
        );
        assert_eq!(before, capture(repo));
    }

    #[test]
    fn review_state_detects_mutations_to_preexisting_dirty_and_untracked_files() {
        let dir = repository();
        let repo = dir.path();
        fs::write(repo.join("source.rs"), "already dirty\n").unwrap();
        fs::write(repo.join("new.rs"), "untracked before\n").unwrap();
        let before = capture(repo);
        fs::write(repo.join("source.rs"), "reviewer modified dirty file\n").unwrap();
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        fs::write(repo.join("new.rs"), "untracked after\n").unwrap();
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        fs::remove_file(repo.join("new.rs")).unwrap();
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        fs::write(repo.join("added.rs"), "new source\n").unwrap();
        assert_ne!(before, capture(repo));
    }

    #[test]
    fn review_state_detects_index_only_and_head_only_mutations() {
        let dir = repository();
        let repo = dir.path();
        fs::write(repo.join("source.rs"), "dirty\n").unwrap();
        let before = capture(repo);
        git(repo, &["add", "source.rs"]);
        let staged = capture(repo);
        assert_ne!(
            before, staged,
            "staging changes evidence with identical working bytes"
        );
        commit(repo);
        let committed = capture(repo);
        assert_ne!(staged, committed);
        commit(repo);
        assert_ne!(
            committed,
            capture(repo),
            "empty commits also invalidate the review"
        );
    }

    #[test]
    fn review_state_reads_tracked_files_despite_index_visibility_flags() {
        for flag in ["--assume-unchanged", "--skip-worktree"] {
            let dir = repository();
            let repo = dir.path();
            let before = capture(repo);
            git(repo, &["update-index", flag, "source.rs"]);
            let hidden = capture(repo);
            assert_ne!(before, hidden, "index flag changes are evidence");
            fs::write(repo.join("source.rs"), "hidden edit\n").unwrap();
            assert_ne!(hidden, capture(repo), "raw content bypasses {flag}");
        }
    }

    #[test]
    fn review_state_detects_control_metadata_and_ignore_rule_changes() {
        let dir = repository();
        let repo = dir.path();
        let before = capture(repo);
        fs::write(repo.join(".git/info/exclude"), "hidden.rs\n").unwrap();
        fs::write(repo.join("hidden.rs"), "should not escape observation\n").unwrap();
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        git(repo, &["config", "core.filemode", "false"]);
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        fs::write(repo.join(".gitignore"), "cache/\n*.rs\n").unwrap();
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        fs::write(repo.join(".git/hooks/post-commit"), "changed\n").unwrap();
        assert_ne!(before, capture(repo));
    }

    #[test]
    fn review_state_supports_linked_worktrees_and_subdirectories() {
        let dir = repository();
        let root = tempfile::tempdir().unwrap();
        let task = root.path().join("task");
        git(
            dir.path(),
            &[
                "worktree",
                "add",
                "--detach",
                task.to_str().unwrap(),
                "HEAD",
            ],
        );
        let before = capture(&task);
        fs::create_dir(task.join("cache")).unwrap();
        fs::write(task.join("cache/dependency"), "cache\n").unwrap();
        assert_eq!(before, capture(&task.join("cache")));
        fs::write(task.join("source.rs"), "changed\n").unwrap();
        assert_ne!(before, capture(&task));
    }

    #[test]
    fn review_state_never_executes_repository_helpers() {
        let dir = repository();
        let repo = dir.path();
        fs::write(repo.join(".gitattributes"), "*.rs filter=evil diff=evil\n").unwrap();
        for key in [
            "filter.evil.clean",
            "filter.evil.process",
            "diff.evil.command",
            "diff.evil.textconv",
            "core.fsmonitor",
        ] {
            git(repo, &["config", key, "touch helper-ran; cat"]);
        }
        fs::write(repo.join("source.rs"), "changed\n").unwrap();
        let _ = capture(repo);
        assert!(!repo.join("helper-ran").exists());
    }

    #[test]
    fn review_state_detects_symlinks_modes_and_tracked_deletions() {
        let dir = repository();
        let repo = dir.path();
        std::os::unix::fs::symlink("source.rs", repo.join("alias")).unwrap();
        let before = capture(repo);
        fs::remove_file(repo.join("alias")).unwrap();
        std::os::unix::fs::symlink(".gitignore", repo.join("alias")).unwrap();
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        fs::set_permissions(repo.join("source.rs"), fs::Permissions::from_mode(0o755)).unwrap();
        assert_ne!(before, capture(repo));
        let before = capture(repo);
        fs::remove_file(repo.join("source.rs")).unwrap();
        assert_ne!(before, capture(repo));
    }

    #[test]
    fn review_state_rejects_symlinked_index_and_cancelled_capture() {
        let dir = repository();
        let repo = dir.path();
        let index = repo.join(".git/index");
        let moved = repo.join(".git/index-copy");
        fs::rename(&index, &moved).unwrap();
        std::os::unix::fs::symlink(&moved, &index).unwrap();
        assert!(
            capture_review_state(repo, &AtomicBool::new(false))
                .unwrap_err()
                .contains("redirected")
        );
        assert!(
            capture_review_state(repo, &AtomicBool::new(true))
                .unwrap_err()
                .contains("cancelled")
        );
    }
}
