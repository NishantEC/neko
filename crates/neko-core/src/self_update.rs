//! The installed app keeping itself current from the published repository:
//! deciding what a range of published commits needs (a new web UI, a
//! reinstalled app, or nothing), managing the updater's own clone, building
//! and swapping the web UI, and reading what the detached installer reported.
//! The daemon's `self_update` thread drives this; everything here is either
//! pure or works on paths it is given, so it is tested on temp dirs and local
//! bare repositories (never GitHub).
//!
//! See `docs/knowledge/neko-updates-itself.md` for the design and why the
//! daemon hands its own replacement to a detached script.

use neko_protocol::workbench::{SelfUpdate, UpdatePlan};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// Safety-critical paths. Since 2026-11-13 the use copy follows the published
/// `main`, so a range touching these applies like any other (switch auto-update
/// off in Settings → Neko updates to make everything wait for a click); the
/// list is what the plan and the notice name so the owner can see it. A
/// trailing `/` means the whole folder.
///
/// Keep this the one list. It covers the rulebook's defaults, the code that
/// decides authority, approval, review and publishing, the web bridge's access
/// control, tool authority, and this updater itself (including the scripts it
/// runs), so a change can never quietly loosen the check that let it in.
pub const SENSITIVE: &[&str] = &[
    // Rulebook defaults every agent works to.
    "crates/neko-core/src/rulebook.rs",
    // Approval, authority and task state transitions.
    "crates/neko-core/src/workbench.rs",
    "crates/neko-daemon/src/workbench.rs",
    "crates/neko-daemon/src/workbench/replies.rs",
    "crates/neko-daemon/src/workbench/responsibilities.rs",
    "crates/neko-core/src/chief_of_staff.rs",
    "crates/neko-core/src/supervision.rs",
    // Review: verdicts, evidence and repair.
    "crates/neko-core/src/native_runner/review_state.rs",
    "crates/neko-core/src/verification.rs",
    "crates/neko-daemon/src/workbench/review_repair.rs",
    // Publishing and merging.
    "crates/neko-core/src/native_runner/publish.rs",
    "crates/neko-daemon/src/workbench/publish.rs",
    "crates/neko-core/src/pull_requests.rs",
    "crates/neko-daemon/src/workbench/pull_requests.rs",
    // Tool authority (MCP host, bridge, credentials, chat tools).
    "crates/neko-core/src/mcp_host/",
    "crates/neko-daemon/src/mcp_host.rs",
    "crates/neko-daemon/src/mcp_host/",
    // The web bridge's access control.
    "crates/neko-daemon/src/web.rs",
    // This updater, and what it runs.
    "crates/neko-core/src/self_update.rs",
    "crates/neko-daemon/src/self_update.rs",
    "scripts/self-update.sh",
    "scripts/install-native.sh",
    "scripts/build-native.sh",
    "scripts/check.sh",
    // Patched dependencies.
    "patches/",
];

/// Whether `path` (repository-relative, `/`-separated) is safety-critical.
pub fn is_sensitive(path: &str) -> bool {
    SENSITIVE.iter().any(|entry| if entry.ends_with('/') { path.starts_with(entry) } else { path == *entry })
}

/// What a set of changed paths needs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Classification {
    /// The web UI must be rebuilt.
    pub web: bool,
    /// The app (daemon and native client) must be reinstalled.
    pub native: bool,
    /// Safety-critical paths among them.
    pub sensitive: Vec<String>,
}

fn is_doc(path: &str) -> bool {
    path.ends_with(".md")
}

/// Sorts changed paths into web, native and safety-critical. Documentation
/// changes nothing that runs.
pub fn classify(paths: &[String]) -> Classification {
    let mut out = Classification::default();
    for path in paths {
        let path = path.trim();
        if path.is_empty() { continue; }
        if is_sensitive(path) { out.sensitive.push(path.to_owned()); }
        if is_doc(path) { continue; }
        if path.starts_with("web/") { out.web = true; }
        if ["crates/", "native/", "scripts/", "patches/"].iter().any(|p| path.starts_with(p))
            || matches!(path, "Cargo.toml" | "Cargo.lock")
        {
            out.native = true;
        }
    }
    out.sensitive.sort();
    out.sensitive.dedup();
    out
}

/// What to do with the commits `from..to`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Nothing that runs changed.
    Nothing,
    /// Apply it now (web) and when idle (native).
    Queue(UpdatePlan),
    /// Wait for the owner's click.
    Pending(UpdatePlan),
}

/// Decides one update. `approved` is the owner's click (Apply or Update now):
/// it lets switched-off parts through. Safety-critical paths are named in the
/// plan but do not hold it back: the source is the published `main`.
pub fn decide(from: Option<&str>, to: &str, paths: &[String], task_id: Option<String>, approved: bool, settings: &SelfUpdate, now_ms: i64) -> Decision {
    let class = classify(paths);
    if !class.web && !class.native {
        return Decision::Nothing;
    }
    let mut plan = UpdatePlan {
        from: from.map(str::to_owned), to: to.to_owned(), task_id, web: class.web, native: class.native,
        sensitive: class.sensitive, reason: String::new(), approved, at_ms: now_ms,
    };
    if approved {
        return Decision::Queue(plan);
    }
    let blocked: Vec<&str> = [(plan.web && !settings.auto_ui, "the web UI"), (plan.native && !settings.auto_native, "the app")]
        .into_iter().filter(|(off, _)| *off).map(|(_, what)| what).collect();
    if !blocked.is_empty() {
        plan.reason = format!("Auto-update is off for {}.", blocked.join(" and "));
        return Decision::Pending(plan);
    }
    Decision::Queue(plan)
}

/// A pending update that waited only because auto-update was off may go once
/// it is switched back on.
pub fn releasable(plan: &UpdatePlan, settings: &SelfUpdate) -> bool {
    (!plan.web || settings.auto_ui) && (!plan.native || settings.auto_native)
}

/// A short list of paths for a sentence: the first three, then an ellipsis.
pub fn short_list(paths: &[String]) -> String {
    let mut shown: Vec<&str> = paths.iter().take(3).map(String::as_str).collect();
    if paths.len() > 3 { shown.push("…"); }
    shown.join(", ")
}

pub fn short(commit: &str) -> &str {
    &commit[..commit.len().min(8)]
}

/// The reinstall waits until Neko is idle and the Mac says building is fine.
/// Pure, so each reason is tested on its own; the first that applies is said.
#[derive(Debug, Clone, Default)]
pub struct IdleInputs {
    pub workers: usize,
    pub publishing: usize,
    pub chat_running: bool,
    /// `~/.agents/bin/machine-ok`'s WAIT reason (or the steward's hold).
    pub machine_wait: Option<String>,
    /// The clone's `HEAD` is the commit being installed.
    pub checkout_at_target: bool,
    /// Tracked files in the clone have uncommitted changes.
    pub checkout_dirty: bool,
}

pub fn idle_gate(inputs: &IdleInputs) -> Result<(), String> {
    if inputs.workers > 0 {
        return Err(format!("Waiting for {} running task{} to finish before reinstalling.", inputs.workers, if inputs.workers == 1 { "" } else { "s" }));
    }
    if inputs.publishing > 0 {
        return Err("Waiting for a publish to finish before reinstalling.".into());
    }
    if inputs.chat_running {
        return Err("Waiting for Neko's reply to finish before reinstalling.".into());
    }
    if let Some(reason) = &inputs.machine_wait {
        return Err(format!("Waiting for the Mac to settle before building ({reason})."));
    }
    if !inputs.checkout_at_target {
        return Err("Waiting: the update source is not on the commit to install.".into());
    }
    if inputs.checkout_dirty {
        return Err("Waiting: the update source has uncommitted changes.".into());
    }
    Ok(())
}

/// `~/.agents/bin/machine-ok`: exit 0 is fine, anything else is the reason to
/// wait. A Mac without the script is never held.
pub fn machine_wait() -> Option<String> {
    let home = std::env::var_os("HOME")?;
    let script = Path::new(&home).join(".agents/bin/machine-ok");
    if !script.is_file() { return None; }
    match Command::new(&script).stdin(Stdio::null()).output() {
        Ok(output) if output.status.success() => None,
        Ok(output) => Some(crate::machine::parse_comfort_wait(&String::from_utf8_lossy(&output.stdout))
            .unwrap_or_else(|| "the shared machine check says wait".into())),
        Err(_) => None,
    }
}

// ---------------------------------------------------------------------------
// The use copy: a clean clone the updater owns
// ---------------------------------------------------------------------------

/// The project's public repository, used when the app has not told the daemon
/// another one (Settings → About, "Updates come from"). Same value as
/// `Updates.defaultRepository` in the native app.
pub const DEFAULT_REPOSITORY: &str = "NishantEC/neko"; // privacy-ok: the project's own public repository

/// A look at the published repository at most this often (milliseconds).
pub const CHECK_EVERY_MS: i64 = 10 * 60 * 1000;
const LS_REMOTE_TIMEOUT: Duration = Duration::from_secs(30);
const FETCH_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CLONE_TIMEOUT: Duration = Duration::from_secs(15 * 60);

/// `owner/name` from `owner/name`, a github.com URL (with or without `.git`)
/// or an ssh remote; `None` for anything else. Mirrors
/// `Updates.normalizedRepository` in the native app.
pub fn normalize_repository(text: &str) -> Option<String> {
    let mut value = text.trim();
    for prefix in ["https://github.com/", "http://github.com/", "git@github.com:", "github.com/"] {
        if let Some(rest) = value.strip_prefix(prefix) { value = rest; break; }
    }
    let value = value.trim_end_matches('/');
    let value = value.strip_suffix(".git").unwrap_or(value);
    let parts: Vec<&str> = value.split('/').collect();
    let allowed = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    (parts.len() == 2 && parts.iter().all(|p| !p.is_empty() && p.chars().all(allowed))).then(|| value.to_owned())
}

/// Where the use copy is cloned from, and where it lives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// An https URL without credentials (tests and development may name a
    /// local bare repository).
    pub remote: String,
    /// `<data dir>/source`: a clean clone nobody else writes to.
    pub dir: PathBuf,
}

impl Source {
    /// The configured source: the app's setting (or the public repository),
    /// over https and with no credentials. `NEKO_UPDATE_REMOTE` and
    /// `NEKO_UPDATE_SOURCE_DIR` override the remote and the clone path (tests
    /// and development; never set in a normal install).
    pub fn configured(data_dir: &Path, repository: &str) -> Source {
        let repository = normalize_repository(repository).unwrap_or_else(|| DEFAULT_REPOSITORY.to_owned());
        Source {
            remote: std::env::var("NEKO_UPDATE_REMOTE").ok().filter(|r| !r.is_empty())
                .unwrap_or_else(|| format!("https://github.com/{repository}.git")),
            dir: std::env::var_os("NEKO_UPDATE_SOURCE_DIR").map(PathBuf::from).filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| data_dir.join("source")),
        }
    }
}

/// Cargo's build output for updates: the user's cache folder, not the data
/// folder (it is several GB and can be rebuilt). `NEKO_UPDATE_TARGET_DIR`
/// overrides.
pub fn build_cache_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("NEKO_UPDATE_TARGET_DIR").filter(|d| !d.is_empty()) {
        return PathBuf::from(dir);
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    home.join("Library/Caches/neko/target")
}

fn no_cancel() -> AtomicBool {
    AtomicBool::new(false)
}

fn git(checkout: &Path, args: &[&str]) -> Result<String, String> {
    crate::native_runner::git_output(checkout, args, &no_cancel()).map(|s| s.trim().to_owned())
}

fn git_net(directory: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
    crate::native_runner::publish::git_limited(directory, args, &no_cancel(), timeout).map(|s| s.trim().to_owned())
}

/// What `main` points at on the remote: one `ls-remote`, 30 seconds at most.
pub fn remote_head(source: &Source) -> Result<String, String> {
    let from = source.dir.parent().filter(|p| p.is_dir()).map(Path::to_path_buf).unwrap_or_else(std::env::temp_dir);
    let out = git_net(&from, &["ls-remote", &source.remote, "refs/heads/main"], LS_REMOTE_TIMEOUT)?;
    out.split_whitespace().next().filter(|h| h.len() >= 40 && h.bytes().all(|b| b.is_ascii_hexdigit())).map(str::to_owned)
        .ok_or_else(|| "the repository has no main branch".to_owned())
}

/// Brings the clone to the remote's `main`, creating it when it is missing:
/// clone (single branch, no tags) or fetch, then `reset --hard` and
/// `clean -fdx` keeping only `node_modules` and the Swift `.build` cache
/// (nobody else writes in the clone, so nothing else is worth keeping).
/// Never commits and never pushes. Returns the commit it is at.
pub fn sync_clone(source: &Source) -> Result<String, String> {
    let dir = &source.dir;
    if !dir.join(".git").exists() {
        if dir.exists() && fs::read_dir(dir).map(|mut d| d.next().is_some()).unwrap_or(true) {
            return Err(format!("{} exists and is not a clone; move it away", dir.display()));
        }
        let parent = dir.parent().ok_or("the update source has no parent folder")?;
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        git_net(parent, &["clone", "--quiet", "--single-branch", "--branch", "main", "--no-tags", &source.remote, &dir.to_string_lossy()], CLONE_TIMEOUT)?;
    } else {
        if git(dir, &["config", "--get", "remote.origin.url"]).ok().as_deref() != Some(source.remote.as_str()) {
            // The person pointed Neko at another repository.
            git(dir, &["remote", "set-url", "origin", &source.remote])?;
        }
        git_net(dir, &["fetch", "--quiet", "--no-tags", "origin", "+refs/heads/main:refs/remotes/origin/main"], FETCH_TIMEOUT)?;
    }
    reset_clone(dir)
}

/// Makes the clone's tree exactly `origin/main` again, without the network.
pub fn reset_clone(dir: &Path) -> Result<String, String> {
    git(dir, &["checkout", "--quiet", "--force", "-B", "main", "refs/remotes/origin/main"])?;
    git(dir, &["reset", "--quiet", "--hard", "refs/remotes/origin/main"])?;
    git(dir, &["clean", "--quiet", "-fdx", "-e", "node_modules", "-e", ".build"])?;
    head(dir)
}

pub fn head(checkout: &Path) -> Result<String, String> {
    git(checkout, &["rev-parse", "--verify", "HEAD^{commit}"])
}

pub fn changed_paths(checkout: &Path, from: &str, to: &str) -> Result<Vec<String>, String> {
    Ok(git(checkout, &["diff", "--name-only", "--no-renames", from, to])?
        .lines().map(str::to_owned).filter(|l| !l.is_empty()).collect())
}

/// Every path of the tree at `commit`: what "apply it once" classifies when
/// there is no starting commit to compare with.
pub fn all_paths(checkout: &Path, commit: &str) -> Result<Vec<String>, String> {
    Ok(git(checkout, &["ls-tree", "-r", "--name-only", commit])?.lines().map(str::to_owned).filter(|l| !l.is_empty()).collect())
}

/// The paths a plan from `from` (or from nothing) to `to` changes.
pub fn paths_between(checkout: &Path, from: Option<&str>, to: &str) -> Result<Vec<String>, String> {
    match from {
        Some(from) => changed_paths(checkout, from, to),
        None => all_paths(checkout, to),
    }
}

/// The tree object of `commit` in `repository`, if that repository has it.
pub fn tree_of(repository: &Path, commit: &str) -> Option<String> {
    git(repository, &["rev-parse", "--verify", "--quiet", &format!("{commit}^{{tree}}")]).ok().filter(|t| !t.is_empty())
}

/// Where a running app starts from. Pure over what the clone knows:
/// - the recorded `applied` commit, if it is in the published history;
/// - else the app's own commit, if it is in the published history;
/// - else (the app was built from commits that are not public, such as a
///   rewritten history) `head` when the app's tree equals `head`'s: it is
///   the same files, so it is adopted as already applied;
/// - else `None`: nothing to compare with, apply `head` once.
pub fn starting_point(clone: &Path, head: &str, applied: Option<&str>, bundle: Option<&str>, bundle_tree: Option<&str>) -> Option<String> {
    if let Some(applied) = applied.filter(|a| is_ancestor(clone, a, head)) {
        return Some(applied.to_owned());
    }
    let bundle = bundle?;
    if is_ancestor(clone, bundle, head) {
        return Some(bundle.to_owned());
    }
    (bundle_tree.is_some() && bundle_tree == tree_of(clone, head).as_deref()).then(|| head.to_owned())
}

/// The full commit a build label (a short hex commit) names, if the checkout
/// knows it. Anything that is not hex is not a commit label.
pub fn resolve_commit(checkout: &Path, label: &str) -> Option<String> {
    if label.len() < 7 || !label.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    git(checkout, &["rev-parse", "--verify", "--quiet", &format!("{label}^{{commit}}")]).ok().filter(|c| !c.is_empty())
}

/// Whether `ancestor` is reachable from `descendant`.
pub fn is_ancestor(checkout: &Path, ancestor: &str, descendant: &str) -> bool {
    git(checkout, &["merge-base", "--is-ancestor", ancestor, descendant]).is_ok()
}

/// Tracked files (optionally under one folder) have uncommitted changes.
pub fn dirty(checkout: &Path, under: Option<&str>) -> bool {
    let mut args = vec!["status", "--porcelain", "--untracked-files=no"];
    if let Some(folder) = under { args.extend(["--", folder]); }
    git(checkout, &args).map(|s| !s.is_empty()).unwrap_or(true)
}

// ---------------------------------------------------------------------------
// The web UI: build, smoke check, atomic swap
// ---------------------------------------------------------------------------

/// `<data>/web-live` (what is served), `web-live.prev` (the one before) and
/// `web-live.next` (a build being checked). `web-live` and `web-live.prev`
/// are symlinks into `<data>/web-builds/`, so a swap is one `rename`.
pub struct WebPaths {
    pub live: PathBuf,
    pub prev: PathBuf,
    pub next: PathBuf,
    pub builds: PathBuf,
}

pub fn web_paths(data_dir: &Path) -> WebPaths {
    WebPaths {
        live: data_dir.join("web-live"),
        prev: data_dir.join("web-live.prev"),
        next: data_dir.join("web-live.next"),
        builds: data_dir.join("web-builds"),
    }
}

/// The live web UI, when there is a usable one.
pub fn live_web_root(data_dir: &Path) -> Option<PathBuf> {
    let live = web_paths(data_dir).live;
    live.join("index.html").is_file().then_some(live)
}

/// The folder name `web-live` points at (`web-builds/<label>`), when a live UI
/// is served. Both the updater and `scripts/ship-web.sh` label a build with
/// the short commit it was built from.
pub fn live_web_label(data_dir: &Path) -> Option<String> {
    live_web_root(data_dir)?;
    let target = fs::read_link(web_paths(data_dir).live).ok()?;
    target.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// The previous web UI, kept so a page still holding the old `index.html`
/// can load the chunks it names.
pub fn previous_web_root(data_dir: &Path) -> Option<PathBuf> {
    let prev = web_paths(data_dir).prev;
    prev.join("index.html").is_file().then_some(prev)
}

/// A build is servable: `index.html` exists, loads at least one script, and
/// every `/assets/…` file it names exists and is not empty.
pub fn smoke_check(dir: &Path) -> Result<(), String> {
    let index = fs::read_to_string(dir.join("index.html")).map_err(|_| "the build has no index.html".to_owned())?;
    let referenced = asset_references(&index);
    if !referenced.iter().any(|r| r.ends_with(".js")) {
        return Err("index.html loads no script".into());
    }
    for reference in &referenced {
        let relative = reference.trim_start_matches('/');
        if relative.split('/').any(|p| p == ".." || p.is_empty()) {
            return Err(format!("index.html names an unsafe path: {reference}"));
        }
        let file = dir.join(relative);
        match fs::metadata(&file) {
            Ok(meta) if meta.is_file() && meta.len() > 0 => {}
            _ => return Err(format!("index.html names {reference}, which the build does not have")),
        }
    }
    Ok(())
}

/// Every `src="/…"` and `href="/…"` in the page that points into `/assets/`.
fn asset_references(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for attribute in ["src=\"", "href=\""] {
        let mut rest = html;
        while let Some(at) = rest.find(attribute) {
            rest = &rest[at + attribute.len()..];
            let Some(end) = rest.find('"') else { break };
            let value = &rest[..end];
            if value.starts_with("/assets/") { out.push(value.to_owned()); }
            rest = &rest[end..];
        }
    }
    out
}

fn replace_symlink(link: &Path, target: &Path) -> Result<(), String> {
    let tmp = link.with_extension(format!("tmp-{}", std::process::id()));
    let _ = fs::remove_file(&tmp);
    std::os::unix::fs::symlink(target, &tmp).map_err(|e| format!("cannot link {}: {e}", link.display()))?;
    // `rename` over an existing symlink replaces it atomically.
    fs::rename(&tmp, link).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("cannot swap {}: {e}", link.display())
    })
}

/// Moves a checked build (`web-live.next`) into `web-builds/<label>` and
/// points `web-live` at it in one atomic rename; the old target becomes
/// `web-live.prev`. Builds that are neither are deleted.
pub fn swap_in(data_dir: &Path, label: &str) -> Result<(), String> {
    let paths = web_paths(data_dir);
    smoke_check(&paths.next)?;
    if label.is_empty() || label.contains(['/', '.']) {
        return Err("bad build label".into());
    }
    fs::create_dir_all(&paths.builds).map_err(|e| e.to_string())?;
    let destination = paths.builds.join(label);
    if destination.exists() {
        fs::remove_dir_all(&destination).map_err(|e| e.to_string())?;
    }
    fs::rename(&paths.next, &destination).map_err(|e| format!("cannot keep the build: {e}"))?;
    let old = fs::read_link(&paths.live).ok();
    let relative = Path::new("web-builds").join(label);
    replace_symlink(&paths.live, &relative)?;
    match &old {
        Some(old) if *old != relative => replace_symlink(&paths.prev, old)?,
        _ => {}
    }
    prune_builds(data_dir);
    Ok(())
}

/// Points `web-live` back at `web-live.prev`, or removes it (the app bundle's
/// UI is served) when there is none.
pub fn roll_back_web(data_dir: &Path) -> Result<(), String> {
    let paths = web_paths(data_dir);
    match fs::read_link(&paths.prev) {
        Ok(prev) => {
            replace_symlink(&paths.live, &prev)?;
            let _ = fs::remove_file(&paths.prev);
        }
        Err(_) => {
            let _ = fs::remove_file(&paths.live);
        }
    }
    prune_builds(data_dir);
    Ok(())
}

/// Stops serving any live web UI (the app bundle's is served again).
pub fn clear_web(data_dir: &Path) {
    let paths = web_paths(data_dir);
    let _ = fs::remove_file(&paths.live);
    let _ = fs::remove_file(&paths.prev);
    prune_builds(data_dir);
}

fn prune_builds(data_dir: &Path) {
    let paths = web_paths(data_dir);
    let keep: Vec<PathBuf> = [&paths.live, &paths.prev].iter()
        .filter_map(|l| fs::read_link(l).ok()).map(|t| data_dir.join(t)).collect();
    let Ok(entries) = fs::read_dir(&paths.builds) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if !keep.contains(&path) {
            let _ = fs::remove_dir_all(&path);
        }
    }
}

const WEB_BUILD_TIMEOUT: Duration = Duration::from_secs(15 * 60);

fn tool_path() -> Option<std::ffi::OsString> {
    let mut dirs = crate::native_runner::cli_workers::agent_directories();
    if let Some(path) = std::env::var_os("PATH") { dirs.extend(std::env::split_paths(&path)); }
    std::env::join_paths(dirs).ok()
}

fn pnpm() -> Option<PathBuf> {
    let path = tool_path()?;
    std::env::split_paths(&path).map(|d| d.join("pnpm")).find(|p| p.is_file())
}

/// Runs `pnpm` in `<checkout>/web` with a time limit; the error is the tail
/// of what it printed.
fn run_pnpm(checkout: &Path, args: &[&str]) -> Result<(), String> {
    let pnpm = pnpm().ok_or("pnpm was not found")?;
    let log = tempfile_path("neko-web-build");
    let file = fs::File::create(&log).map_err(|e| e.to_string())?;
    let mut command = Command::new(pnpm);
    command.current_dir(checkout.join("web")).args(args)
        .stdin(Stdio::null()).stdout(file.try_clone().map_err(|e| e.to_string())?).stderr(file)
        .env("CI", "false");
    if let Some(path) = tool_path() { command.env("PATH", path); }
    let mut child = command.spawn().map_err(|e| format!("cannot run pnpm: {e}"))?;
    let started = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? { break status; }
        if started.elapsed() > WEB_BUILD_TIMEOUT {
            let _ = child.kill();
            let _ = child.wait();
            let _ = fs::remove_file(&log);
            return Err("the web build took longer than 15 minutes".into());
        }
        std::thread::sleep(Duration::from_millis(250));
    };
    let output = fs::read_to_string(&log).unwrap_or_default();
    let _ = fs::remove_file(&log);
    if status.success() { return Ok(()); }
    let tail: Vec<&str> = output.lines().rev().filter(|l| !l.trim().is_empty()).take(6).collect();
    Err(format!("pnpm {} failed: {}", args.first().unwrap_or(&""), tail.into_iter().rev().collect::<Vec<_>>().join(" / ")))
}

fn tempfile_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}-{}", std::process::id(), crate::workbench::now_ms()))
}

/// Builds the web app from the checkout into `<data>/web-live.next`
/// (installing dependencies first when the lockfile changed), then smoke
/// checks it. Nothing that is served is touched.
pub fn build_web(checkout: &Path, data_dir: &Path, install_dependencies: bool) -> Result<(), String> {
    let next = web_paths(data_dir).next;
    let _ = fs::remove_dir_all(&next);
    fs::create_dir_all(data_dir).map_err(|e| e.to_string())?;
    if install_dependencies || !checkout.join("web/node_modules").is_dir() {
        run_pnpm(checkout, &["install", "--frozen-lockfile", "--prefer-offline"])?;
    }
    let out = next.to_string_lossy().into_owned();
    run_pnpm(checkout, &["run", "build", "--outDir", &out, "--emptyOutDir"])?;
    smoke_check(&next)
}

// ---------------------------------------------------------------------------
// The native app
// ---------------------------------------------------------------------------

/// The commit the running app was built from: `NekoGitCommit` in the bundle's
/// `Info.plist` (`scripts/build-native.sh` stamps it).
pub fn bundle_commit() -> Option<String> {
    let exe = std::env::current_exe().ok()?;
    let plist = exe.parent()?.parent()?.join("Info.plist");
    plist_commit(&plist)
}

pub fn plist_commit(plist: &Path) -> Option<String> {
    let text = fs::read_to_string(plist).ok()?;
    let key = text.find("<key>NekoGitCommit</key>")?;
    let rest = &text[key..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")?;
    let commit = rest[start..start + end].trim();
    (commit.len() >= 7 && commit.bytes().all(|b| b.is_ascii_hexdigit())).then(|| commit.to_owned())
}

/// The folder the running app was built in (`NekoSourcePath`). Read only:
/// when the app's commit is not in the published history, its tree is looked
/// up there to see whether the app matches the published `main`.
pub fn bundle_source_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let text = fs::read_to_string(exe.parent()?.parent()?.join("Info.plist")).ok()?;
    let rest = &text[text.find("<key>NekoSourcePath</key>")?..];
    let start = rest.find("<string>")? + "<string>".len();
    let end = rest[start..].find("</string>")?;
    Some(PathBuf::from(rest[start..start + end].trim())).filter(|p| p.join(".git").exists())
}

/// What `scripts/self-update.sh` writes to `<data>/self-update/result.json`
/// when it is done (successfully or not).
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct InstallResult {
    pub commit: String,
    /// `updated`, `failed` (nothing was replaced) or `rolled_back`.
    pub status: String,
    #[serde(default)]
    pub reason: String,
}

pub fn result_path(data_dir: &Path) -> PathBuf {
    data_dir.join("self-update/result.json")
}

/// Reads and removes the installer's report, if it has written one.
pub fn take_result(data_dir: &Path) -> Option<InstallResult> {
    let path = result_path(data_dir);
    let text = fs::read_to_string(&path).ok()?;
    let _ = fs::remove_file(&path);
    serde_json::from_str(text.trim()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn paths_sort_into_web_native_and_safety_critical() {
        let c = classify(&paths(&["web/src/pages/Home.tsx"]));
        assert!(c.web && !c.native && c.sensitive.is_empty());
        let c = classify(&paths(&["crates/neko-core/src/triage.rs", "Cargo.lock"]));
        assert!(!c.web && c.native && c.sensitive.is_empty());
        let c = classify(&paths(&["native/NekoKit/Sources/NekoNative/WebHome.swift", "scripts/run-dev.sh"]));
        assert!(c.native && !c.web);
        let c = classify(&paths(&["AGENTS.md", "docs/design/x.md", "web/README.md"]));
        assert_eq!(c, Classification::default(), "documentation changes nothing that runs");
        let c = classify(&paths(&["crates/neko-daemon/src/web.rs", "web/src/api.ts", "crates/neko-core/src/mcp_host/bridge.rs"]));
        assert!(c.web && c.native);
        assert_eq!(c.sensitive, paths(&["crates/neko-core/src/mcp_host/bridge.rs", "crates/neko-daemon/src/web.rs"]));
    }

    #[test]
    fn the_safety_critical_list_covers_authority_review_publish_and_the_updater_itself() {
        for path in [
            "crates/neko-core/src/rulebook.rs",
            "crates/neko-core/src/native_runner/publish.rs",
            "crates/neko-daemon/src/workbench/publish.rs",
            "crates/neko-core/src/native_runner/review_state.rs",
            "crates/neko-core/src/mcp_host/capability.rs",
            "crates/neko-daemon/src/web.rs",
            "crates/neko-core/src/self_update.rs",
            "crates/neko-daemon/src/self_update.rs",
            "scripts/self-update.sh",
            "scripts/install-native.sh",
            "crates/neko-core/src/workbench.rs",
        ] {
            assert!(is_sensitive(path), "{path} must need a click");
        }
        for path in ["crates/neko-core/src/triage.rs", "web/src/App.tsx", "crates/neko-core/src/mcp_hostile.rs", "crates/neko-daemon/src/web_tests.rs"] {
            assert!(!is_sensitive(path), "{path} is ordinary");
        }
        // Every listed file exists, so a rename cannot silently drop a guard.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        for entry in SENSITIVE {
            assert!(root.join(entry).exists(), "{entry} is listed but missing");
        }
    }

    #[test]
    fn decisions_follow_the_settings_and_the_owners_click() {
        let on = SelfUpdate::default();
        let web = paths(&["web/src/App.tsx"]);
        assert!(matches!(decide(None, "abc", &web, None, false, &on, 1), Decision::Queue(p) if p.web && !p.native));
        assert_eq!(decide(None, "abc", &paths(&["README.md"]), None, false, &on, 1), Decision::Nothing);
        // Safety-critical paths are named in the plan but no longer hold it back.
        let critical = paths(&["crates/neko-daemon/src/web.rs"]);
        let Decision::Queue(plan) = decide(Some("a"), "b", &critical, None, false, &on, 1) else { panic!("applies on its own") };
        assert!(plan.native && plan.sensitive == critical && !plan.approved);
        assert!(matches!(decide(Some("a"), "b", &critical, None, true, &on, 1), Decision::Queue(p) if p.approved));
        let off = SelfUpdate { auto_native: false, ..SelfUpdate::default() };
        let Decision::Pending(plan) = decide(Some("a"), "b", &critical, None, false, &off, 1) else { panic!("auto-update off waits") };
        assert!(releasable(&plan, &on) && !releasable(&plan, &off));
        let off = SelfUpdate { auto_native: false, ..SelfUpdate::default() };
        let Decision::Pending(plan) = decide(None, "b", &paths(&["crates/neko-core/src/triage.rs"]), None, false, &off, 1) else { panic!() };
        assert!(plan.reason.contains("the app"));
        assert!(!releasable(&plan, &off) && releasable(&plan, &on), "switching auto-update back on releases it");
    }

    #[test]
    fn the_reinstall_waits_for_idle_and_a_settled_mac() {
        let ready = IdleInputs { checkout_at_target: true, ..IdleInputs::default() };
        assert!(idle_gate(&ready).is_ok());
        let cases = [
            (IdleInputs { workers: 2, ..ready.clone() }, "2 running tasks"),
            (IdleInputs { publishing: 1, ..ready.clone() }, "publish"),
            (IdleInputs { chat_running: true, ..ready.clone() }, "reply"),
            (IdleInputs { machine_wait: Some("load 18".into()), ..ready.clone() }, "load 18"),
            (IdleInputs { checkout_at_target: false, ..ready.clone() }, "not on the commit"),
            (IdleInputs { checkout_dirty: true, ..ready.clone() }, "uncommitted"),
        ];
        for (inputs, said) in cases {
            let reason = idle_gate(&inputs).unwrap_err();
            assert!(reason.contains(said), "{reason}");
        }
    }

    fn fake_build(dir: &Path, script: &str) {
        fs::create_dir_all(dir.join("assets")).unwrap();
        fs::write(dir.join("index.html"), format!(r#"<html><head><script type="module" src="/assets/{script}"></script><link rel="stylesheet" href="/assets/index.css"></head></html>"#)).unwrap();
        fs::write(dir.join("assets").join(script), "console.log(1)").unwrap();
        fs::write(dir.join("assets/index.css"), "body{}").unwrap();
    }

    #[test]
    fn the_smoke_check_refuses_a_page_whose_assets_are_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(smoke_check(dir.path()).is_err(), "no index.html");
        fake_build(dir.path(), "index-a.js");
        smoke_check(dir.path()).unwrap();
        fs::remove_file(dir.path().join("assets/index.css")).unwrap();
        assert!(smoke_check(dir.path()).unwrap_err().contains("index.css"));
        fs::write(dir.path().join("index.html"), "<html></html>").unwrap();
        assert!(smoke_check(dir.path()).unwrap_err().contains("no script"));
    }

    #[test]
    fn a_swap_is_atomic_keeps_the_previous_build_and_rolls_back() {
        let data = tempfile::tempdir().unwrap();
        let p = web_paths(data.path());
        assert_eq!(live_web_root(data.path()), None);
        // A broken build is refused and nothing is served from it.
        fs::create_dir_all(&p.next).unwrap();
        assert!(swap_in(data.path(), "one").is_err());
        assert_eq!(live_web_root(data.path()), None);
        fake_build(&p.next, "index-1.js");
        swap_in(data.path(), "one").unwrap();
        assert!(live_web_root(data.path()).unwrap().join("assets/index-1.js").is_file());
        assert!(!p.next.exists());
        fake_build(&p.next, "index-2.js");
        swap_in(data.path(), "two").unwrap();
        assert!(p.live.join("assets/index-2.js").is_file());
        assert!(previous_web_root(data.path()).unwrap().join("assets/index-1.js").is_file());
        fake_build(&p.next, "index-3.js");
        swap_in(data.path(), "three").unwrap();
        let kept: Vec<_> = fs::read_dir(&p.builds).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        assert_eq!(kept.len(), 2, "only live and previous are kept: {kept:?}");
        roll_back_web(data.path()).unwrap();
        assert!(p.live.join("assets/index-2.js").is_file(), "back to the one before");
        roll_back_web(data.path()).unwrap();
        assert_eq!(live_web_root(data.path()), None, "nothing left: the bundle's UI is served");
    }

    #[test]
    fn the_bundle_commit_and_the_installer_report_are_read() {
        let dir = tempfile::tempdir().unwrap();
        let plist = dir.path().join("Info.plist");
        fs::write(&plist, "<dict><key>CFBundleIdentifier</key><string>dev.neko.launcher</string><key>NekoGitCommit</key><string>9acc5352f3</string></dict>").unwrap();
        assert_eq!(plist_commit(&plist).as_deref(), Some("9acc5352f3"));
        fs::write(&plist, "<dict><key>NekoGitCommit</key><string>not a sha</string></dict>").unwrap();
        assert_eq!(plist_commit(&plist), None);
        assert_eq!(take_result(dir.path()), None);
        fs::create_dir_all(dir.path().join("self-update")).unwrap();
        fs::write(result_path(dir.path()), r#"{"commit":"abc","status":"rolled_back","reason":"health check failed"}"#).unwrap();
        let result = take_result(dir.path()).unwrap();
        assert_eq!((result.status.as_str(), result.reason.as_str()), ("rolled_back", "health check failed"));
        assert_eq!(take_result(dir.path()), None, "read once");
    }

    fn run(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git").current_dir(dir).args(args)
            .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t").env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
            .output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    }

    #[test]
    fn repositories_are_normalized_like_the_native_apps_setting() {
        for (input, want) in [
            ("acme/neko", Some("acme/neko")),
            ("https://github.com/acme/neko.git", Some("acme/neko")),
            ("git@github.com:acme/neko.git", Some("acme/neko")),
            ("github.com/acme/neko/", Some("acme/neko")),
            ("acme", None), ("a/b/c", None), ("https://evil.example/acme/neko", None), ("", None), ("acme/ne ko", None),
        ] {
            assert_eq!(normalize_repository(input).as_deref(), want, "{input}");
        }
    }

    #[test]
    fn the_source_is_https_with_no_credentials_and_the_clone_lives_in_the_data_folder() {
        let data = Path::new("/tmp/neko-data");
        let source = Source::configured(data, "");
        if std::env::var_os("NEKO_UPDATE_REMOTE").is_none() {
            assert_eq!(source.remote, format!("https://github.com/{DEFAULT_REPOSITORY}.git"));
            assert!(!source.remote.contains('@'));
        }
        if std::env::var_os("NEKO_UPDATE_SOURCE_DIR").is_none() {
            assert_eq!(source.dir, data.join("source"));
        }
        // The build output is never inside the data folder.
        assert!(!build_cache_dir().starts_with(data));
    }

    #[test]
    fn the_clone_is_made_moved_to_origin_main_and_cleaned_but_keeps_its_dependencies() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        let bare = root.join("public.git");
        let work = root.join("work");
        run(&root, &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
        run(&root, &["init", "-q", "-b", "main", work.to_str().unwrap()]);
        run(&work, &["remote", "add", "origin", bare.to_str().unwrap()]);
        fs::write(work.join("a.txt"), "one").unwrap();
        run(&work, &["add", "-A"]);
        run(&work, &["commit", "-q", "-m", "one"]);
        run(&work, &["push", "-q", "origin", "main"]);
        let source = Source { remote: bare.to_string_lossy().into_owned(), dir: root.join("data/source") };
        let one = run(&work, &["rev-parse", "HEAD"]);
        assert_eq!(remote_head(&source).unwrap(), one);
        assert_eq!(sync_clone(&source).unwrap(), one);
        // Litter, local edits and a stray commit are all undone; dependencies stay.
        fs::create_dir_all(source.dir.join("web/node_modules")).unwrap();
        fs::write(source.dir.join("web/node_modules/dep.js"), "x").unwrap();
        fs::write(source.dir.join("stray.txt"), "x").unwrap();
        fs::write(source.dir.join("a.txt"), "edited").unwrap();
        run(&source.dir, &["commit", "-q", "-am", "a commit nobody should make"]);
        fs::write(work.join("a.txt"), "two").unwrap();
        run(&work, &["commit", "-q", "-am", "two"]);
        run(&work, &["push", "-q", "origin", "main"]);
        let two = run(&work, &["rev-parse", "HEAD"]);
        assert_eq!(sync_clone(&source).unwrap(), two);
        assert_eq!(fs::read_to_string(source.dir.join("a.txt")).unwrap(), "two");
        assert!(!source.dir.join("stray.txt").exists());
        assert!(source.dir.join("web/node_modules/dep.js").is_file());
        assert!(!dirty(&source.dir, None));
        assert!(remote_head(&Source { remote: root.join("none.git").to_string_lossy().into_owned(), dir: source.dir.clone() }).is_err());
        // A folder in the way that is not a clone is never overwritten.
        let blocked = Source { dir: root.join("blocked"), ..source.clone() };
        fs::create_dir_all(&blocked.dir).unwrap();
        fs::write(blocked.dir.join("mine.txt"), "x").unwrap();
        assert!(sync_clone(&blocked).unwrap_err().contains("not a clone"));
        assert!(blocked.dir.join("mine.txt").exists());
    }

    #[test]
    fn a_running_app_starts_from_what_is_applied_what_is_running_or_a_matching_tree() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().canonicalize().unwrap();
        run(&dir, &["init", "-q", "-b", "main"]);
        fs::write(dir.join("a.txt"), "1").unwrap();
        run(&dir, &["add", "-A"]);
        run(&dir, &["commit", "-q", "-m", "1"]);
        let one = run(&dir, &["rev-parse", "HEAD"]);
        fs::write(dir.join("a.txt"), "2").unwrap();
        run(&dir, &["commit", "-q", "-am", "2"]);
        let two = run(&dir, &["rev-parse", "HEAD"]);
        let unknown = "f".repeat(40);
        assert_eq!(starting_point(&dir, &two, Some(&one), Some(&two), None).as_deref(), Some(one.as_str()), "applied wins");
        assert_eq!(starting_point(&dir, &two, None, Some(&one), None).as_deref(), Some(one.as_str()), "then the app's commit");
        assert_eq!(starting_point(&dir, &two, Some(&unknown), Some(&one), None).as_deref(), Some(one.as_str()));
        let tree = tree_of(&dir, &two).unwrap();
        assert_eq!(starting_point(&dir, &two, None, Some(&unknown), Some(&tree)).as_deref(), Some(two.as_str()), "same files: adopted");
        assert_eq!(starting_point(&dir, &two, None, Some(&unknown), Some("0".repeat(40).as_str())), None, "different files: apply once");
        assert_eq!(starting_point(&dir, &two, None, None, None), None);
        assert_eq!(paths_between(&dir, None, &two).unwrap(), vec!["a.txt".to_owned()]);
    }
}
