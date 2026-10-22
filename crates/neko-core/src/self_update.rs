//! Neko writing itself over: deciding what a landed change to Neko's own
//! checkout needs (a new web UI, a reinstalled app, or the owner's click),
//! building and swapping the web UI, and reading what the detached installer
//! reported. The daemon's `self_update` thread drives this; everything here is
//! either pure or works on paths it is given, so it is tested on temp dirs.
//!
//! See AGENTS.md, "Neko updates itself", for the design and why the daemon
//! hands its own replacement to a detached script.

use neko_protocol::workbench::{SelfUpdate, UpdatePlan};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

/// Safety-critical paths: a change touching any of these is never applied on
/// its own; it waits for the owner's click (Settings → System, or the chat
/// action `apply_update`). A trailing `/` means the whole folder.
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
/// it lets safety-critical changes and switched-off parts through.
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
    if !plan.sensitive.is_empty() {
        plan.reason = format!("It changes safety-critical code ({}), so it waits for your click.", short_list(&plan.sensitive));
        return Decision::Pending(plan);
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
/// it is switched back on. Safety-critical ones never do.
pub fn releasable(plan: &UpdatePlan, settings: &SelfUpdate) -> bool {
    plan.sensitive.is_empty() && (!plan.web || settings.auto_ui) && (!plan.native || settings.auto_native)
}

fn short_list(paths: &[String]) -> String {
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
    /// The checkout's `HEAD` is the commit being installed.
    pub checkout_at_target: bool,
    /// Tracked files in the checkout have uncommitted changes.
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
        return Err("Waiting: the neko checkout is not on the commit to install.".into());
    }
    if inputs.checkout_dirty {
        return Err("Waiting: the neko checkout has uncommitted changes.".into());
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
// The checkout
// ---------------------------------------------------------------------------

/// Neko's own checkout: a `land_on_main` repository that holds the daemon and
/// the web app. `NEKO_SELF_CHECKOUT` overrides (tests and development).
pub fn find_checkout<'a>(candidates: impl IntoIterator<Item = &'a String>) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("NEKO_SELF_CHECKOUT").map(PathBuf::from) {
        return looks_like_neko(&dir).then_some(dir);
    }
    candidates.into_iter().map(PathBuf::from).find(|dir| looks_like_neko(dir))
}

pub fn looks_like_neko(dir: &Path) -> bool {
    dir.join("crates/neko-daemon/Cargo.toml").is_file() && dir.join("web/package.json").is_file() && dir.join(".git").exists()
}

fn git(checkout: &Path, args: &[&str]) -> Result<String, String> {
    crate::native_runner::git_output(checkout, args, &AtomicBool::new(false)).map(|s| s.trim().to_owned())
}

pub fn main_head(checkout: &Path) -> Result<String, String> {
    git(checkout, &["rev-parse", "--verify", "refs/heads/main^{commit}"])
}

pub fn head(checkout: &Path) -> Result<String, String> {
    git(checkout, &["rev-parse", "--verify", "HEAD^{commit}"])
}

pub fn changed_paths(checkout: &Path, from: &str, to: &str) -> Result<Vec<String>, String> {
    Ok(git(checkout, &["diff", "--name-only", "--no-renames", from, to])?
        .lines().map(str::to_owned).filter(|l| !l.is_empty()).collect())
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
        let critical = paths(&["crates/neko-daemon/src/web.rs"]);
        let Decision::Pending(plan) = decide(Some("a"), "b", &critical, Some("t".into()), false, &on, 1) else { panic!("must wait") };
        assert!(plan.reason.contains("safety-critical") && plan.native && plan.task_id.as_deref() == Some("t"));
        assert!(!releasable(&plan, &on), "a click is the only way through");
        assert!(matches!(decide(Some("a"), "b", &critical, None, true, &on, 1), Decision::Queue(p) if p.approved));
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
}
