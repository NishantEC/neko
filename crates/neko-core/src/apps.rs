//! Installed-application discovery.
//!
//! **Source of truth: Spotlight's metadata index, not a hard-coded
//! directory list.** [`query_spotlight_app_bundles`] shells out to
//! `mdfind` — the documented CLI front end to the same `mds`/`mdworker`
//! store the system Spotlight UI itself queries for its own "Top Hit"
//! application results. This was chosen over linking `LaunchServices`
//! or `CoreServices`'s `MDQuery`/`NSMetadataQuery` APIs directly: there is
//! no public LaunchServices call that enumerates "every registered
//! application" (`LSCopyApplicationURLsForBundleIdentifier` needs a bundle
//! id already in hand; `LSCopyApplicationURLsForURL` is scoped to a
//! content type) — checked against the actual SDK header
//! (`LaunchServices.framework/Headers/LSInfo.h`) rather than assumed.
//! `NSMetadataQuery`/raw `MDQuery` would work but need a persistently
//! pumped `CFRunLoop` on a dedicated thread to deliver live-update
//! notifications; `mdfind` gets the same live signal (see
//! [`watch_applications`]) for a fraction of the unsafe surface, at the
//! cost of a process spawn per query — cheap here since queries are rare
//! (only on a real Spotlight-reported change) and each returns in well
//! under 100ms on a machine with ~100 indexed apps.
//!
//! **Why it's unioned with a plain directory scan of three specific
//! paths.** Verified on the dev machine: `mdfind` returns zero results
//! under `/System/Applications`, `/System/Applications/Utilities`, or
//! `/System/Library/CoreServices/Applications` — every built-in app
//! (Mail, Calculator, Terminal, ~80 bundles) is invisible to it — even
//! though `mdls` proves Spotlight *has* metadata for each one
//! individually. `mdutil -s /` explains why: `Indexing disabled` for the
//! whole read-only system volume those paths live on (Apple's sealed
//! system volume, introduced for SIP; it's immutable between OS updates,
//! so it needs no live index at all). Real Spotlight's UI still shows
//! these because it additionally consults Launch Services' own bundle
//! registration, not just the content-metadata store `mdfind` queries —
//! see the API-surface note above for why this module reaches for a
//! static scan of those three paths instead of chasing that path too.
//! Because the volume is sealed, those three paths only change on an OS
//! update, which reboots the machine (and this daemon with it), so
//! scanning them once at startup — not live — is correct, not a
//! shortcut.
//!
//! **Fallback of last resort.** If `mdfind` itself can't be run at all
//! (missing binary, sandboxed, non-zero exit), [`scan_applications`]
//! additionally does a full recursive scan of the classic third-party
//! locations (`/Applications`, `~/Applications`) so the launcher still
//! shows real applications rather than silently going empty. This is not
//! the common path — every measurement in this module's evidence was
//! taken with `mdfind` working normally — and it does not engage just
//! because Spotlight indexing happens to be disabled for a *user's* apps
//! specifically (an empty-but-successful `mdfind` run is trusted as
//! accurate, per "the metadata index must be the primary source"); that
//! narrow case is a known, honest limitation, not silently masked.

use std::collections::HashSet;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use neko_protocol::{Icon, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppEntry {
    /// The bundle id when the app has one, otherwise the `.app` path as a
    /// string — either way, stable across a single scan and usable as the
    /// protocol `SearchItem::id` / `launches.app_id`.
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

/// A launched-recently boost, tapering over roughly two weeks, plus a small
/// per-launch frequency term. Recency dominates frequency: a thing you used
/// once yesterday should usually beat a thing you used fifty times last
/// year.
/// How many apps an empty root query suggests. Short deliberately — see
/// `AppsProvider::search`.
pub const SUGGESTION_COUNT: usize = 5;

fn recency_boost(last_launched_at_unix_ms: i64, launch_count: i64, now_unix_ms: i64) -> f32 {
    let age_ms = (now_unix_ms - last_launched_at_unix_ms).max(0) as f32;
    let age_days = age_ms / (1000.0 * 60.0 * 60.0 * 24.0);
    let half_life_days = 5.0;
    let recency = 8.0 * 0.5f32.powf(age_days / half_life_days);
    let frequency = (launch_count as f32).ln_1p() * 0.5;
    recency + frequency
}

/// The application-search provider: matches by fuzzy-scoring each indexed
/// app's own name, boosted by how recently and how often it's been
/// launched. Owns no storage of its own — `apps` is the live index
/// `watch_applications` keeps current, `db` is where launches are recorded
/// — both shared `Arc`s so this provider and the daemon's own background
/// threads (icon extraction, the Spotlight watcher) see the same state.
pub struct AppsProvider {
    apps: Arc<RwLock<Vec<AppEntry>>>,
    db: Arc<Mutex<crate::Db>>,
}

impl AppsProvider {
    pub fn new(apps: Arc<RwLock<Vec<AppEntry>>>, db: Arc<Mutex<crate::Db>>) -> Self {
        Self { apps, db }
    }
}

impl Provider for AppsProvider {
    fn id(&self) -> &'static str {
        "app"
    }

    fn section_label(&self) -> &'static str {
        "Applications"
    }

    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate> {
        let suggesting = query.trim().is_empty();
        // **"Suggested" with nothing typed, "Applications" once there is.**
        // The rows are the same rows; what differs is what they *are*. With
        // no query the order is pure frecency (`recency_boost` over
        // `last_launched_at` and `launch_count`, recorded by `activate`), so
        // the list is a suggestion — "what you reach for" — rather than a
        // set of matches. Once something is typed it is an answer to that
        // query, and calling it Applications is the honest label.
        let section = if suggesting { "Suggested" } else { "Applications" };
        let apps = self.apps.read().unwrap();
        let recency = self.db.lock().unwrap().recency().unwrap_or_default();
        let mut scored: Vec<Candidate> = apps
            .iter()
            .filter_map(|app| {
                let mut score = fuzzy_score(query, &app.name)?;
                if let Some(&(last, count)) = recency.get(&app.id) {
                    score += recency_boost(last, count, now_unix_ms);
                }
                let icon = crate::icons::cached_icon_path(&app.id)
                    .filter(|p| p.exists())
                    .map(|p| Icon::Image(p.to_string_lossy().into_owned()))
                    .unwrap_or(Icon::Placeholder);
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: app.id.clone(),
                        kind: "app".to_string(),
                        title: app.name.clone(),
                        subtitle: None,
                        icon,
                        section_label: section.to_string(),
                        action_label: "Open  ↵".to_string(),
                        badge: None,
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: crate::provider::path_actions(),
                        source: None,
                        meter: None,
                        keeps_open: false,
                        preview_markdown: false,
                        preview: None,
                    },
                })
            })
            .collect::<Vec<_>>();

        if !suggesting {
            return scored;
        }
        // **A suggestion list is short on purpose.** With nothing typed the
        // ordering is pure frecency, so the tail is not "more suggestions",
        // it is every app on the machine in a slightly arbitrary order —
        // which is noise, not depth. Sorted here rather than left to
        // `search::allocate` because the cap has to be applied to the *best*
        // few, and allocate only ever sees what this returns.
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(SUGGESTION_COUNT);
        scored
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        let app_path = {
            let apps = self.apps.read().unwrap();
            apps.iter().find(|a| a.id == id).map(|a| a.path.clone())
        };
        let Some(app_path) = app_path else {
            return Err(ProviderError(format!("no such app: {id}")));
        };
        crate::launch::launch_app(&app_path).map_err(|e| ProviderError(e.to_string()))?;
        let _ = self.db.lock().unwrap().record_launch(id, crate::now_unix_ms());
        Ok(())
    }

    /// An app's id is its bundle id, not its path, so this has to look the
    /// bundle up the same way `activate` does before it can reveal it.
    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        let app_path = {
            let apps = self.apps.read().unwrap();
            apps.iter().find(|a| a.id == id).map(|a| a.path.clone())
        };
        let Some(app_path) = app_path else {
            return Err(ProviderError(format!("no such app: {id}")));
        };
        crate::provider::perform_path_action(&app_path, action)
    }
}

/// The locations on the sealed, read-only system volume that get a plain
/// *recursive* scan — see this module's doc comment for why they need a
/// plain scan rather than a Spotlight query, and why that scan doesn't need
/// to be live. **Deliberately excludes `/System/Library/CoreServices`
/// itself** (only its `Applications` subdirectory) — see
/// [`CORE_SERVICES_ALLOWED_APPS`] for why that directory gets a named
/// allowlist instead of a recursive scan.
fn sealed_system_directories() -> Vec<PathBuf> {
    vec![
        PathBuf::from("/System/Applications"),
        PathBuf::from("/System/Applications/Utilities"),
        PathBuf::from("/System/Library/CoreServices/Applications"),
    ]
}

/// `Finder.app`, `Installer.app`, `Siri.app`, `Game Center.app`, and
/// `Screen Time.app` all live loose directly in
/// `/System/Library/CoreServices` — one level up from
/// `.../CoreServices/Applications`, a different, smaller directory of minor
/// utilities (About This Mac, Archive Utility, Keychain Access, …) that
/// [`sealed_system_directories`] already scans recursively and safely.
///
/// The same top-level directory also holds ~112 macOS background
/// agents/daemons (`Dock.app`, `ControlCenter.app`, `PowerChime.app`,
/// `CoreLocationAgent.app`, `SystemUIServer.app`, …), and a first version of
/// this fix scanned the whole directory on the assumption that
/// `read_app_bundle`'s existing `LSBackgroundOnly` check would drop them —
/// it doesn't: checked with `PlistBuddy` against a wide sample
/// (`Dock.app`, `ControlCenter.app`, `SystemUIServer.app`, `loginwindow.app`,
/// `NotificationCenter.app`, `WindowManager.app`, `Spotlight.app`,
/// `System Events.app`, `WiFiAgent.app`, `OBEXAgent.app`, `iCloud.app`,
/// `BluetoothUIServer.app`, `CoreServicesUIAgent.app`, `rcd.app`,
/// `PowerChime.app`, `CoreLocationAgent.app`), **none of them set
/// `LSBackgroundOnly`**, so a full scan of this directory landed 259 apps
/// instead of the expected ~152 — 107 unwanted background agents, not 5.
///
/// No single static `Info.plist`/Launch-Services signal was found that
/// cleanly separates the 5 wanted bundles from the ~112 agents on this OS
/// build — every candidate checked draws the line in the wrong place:
/// - `LSUIElement` (Dock-icon visibility): **wrong**. `Siri.app` and
///   `Game Center.app` — both wanted — set `LSUIElement=true`, identically
///   to `Dock.app`/`ControlCenter.app`/`WindowManager.app` and most of the
///   other agents.
/// - `CFBundleIconFile`/`CFBundleIconName` presence: **wrong**. `Dock.app`,
///   `ControlCenter.app`, `Automator Installer.app`, `iCloud+.app`, and
///   many other agents all carry a real icon asset just like the wanted
///   five do.
/// - `lsregister -dump`'s bundle flags (`has-display-name`, `ui-element`,
///   `is-containerized`, …): **wrong**. `Game Center.app`'s flag set
///   (`has-display-name ui-element`) is byte-identical to
///   `Dock.app`/`ControlCenter.app`/`WindowManager.app`'s.
/// - A `launchd` registration under `/System/Library/LaunchAgents` (agents
///   are launchd-managed services, real apps aren't): **wrong in both
///   directions**. `Finder.app` and `Installer.app` (wanted) *do* have a
///   `LaunchAgents` entry; `PowerChime.app`/`CoreLocationAgent.app`
///   (unwanted) do *not*.
/// - Presence of a compiled `.nib`/`.storyboardc` (a real window to show):
///   **wrong**. `PowerChime.app` ships 3 nibs and is still a background
///   chime player with no launchable window; `Screen Time.app` (wanted)
///   ships none.
/// - `LSApplicationCategoryType` (App Store category): only 2 of the 5
///   wanted bundles set it at all (`Finder`, `Screen Time`) — too sparse to
///   build a rule on.
///
/// Per the brief's own fallback: a small, explicit, named allowlist of
/// exactly the 5 verified-wanted bundles, checked directly by path rather
/// than discovered by a recursive scan. This is not a denylist of the 112
/// unwanted names (which would be fragile across OS versions) — it's the 5
/// names the captain actually asked for, still filtered through
/// `read_app_bundle`'s ordinary checks (nesting, `CFBundlePackageType`,
/// `LSBackgroundOnly`) like every other entry in the index.
const CORE_SERVICES_ALLOWED_APPS: &[&str] =
    &["Finder.app", "Installer.app", "Siri.app", "Game Center.app", "Screen Time.app"];

fn core_services_root() -> PathBuf {
    PathBuf::from("/System/Library/CoreServices")
}

/// Reads exactly the [`CORE_SERVICES_ALLOWED_APPS`] bundles, never a
/// recursive walk of their parent directory — see that constant's doc
/// comment for why.
fn scan_core_services_allowlist(seen_ids: &mut HashSet<String>, out: &mut Vec<AppEntry>) {
    let root = core_services_root();
    for name in CORE_SERVICES_ALLOWED_APPS {
        if let Some(app) = read_app_bundle(&root.join(name))
            && seen_ids.insert(app.id.clone())
        {
            out.push(app);
        }
    }
}

/// The classic third-party locations, scanned only when `mdfind` itself is
/// unusable — see this module's doc comment.
fn fallback_directories() -> Vec<PathBuf> {
    let mut dirs = sealed_system_directories();
    dirs.push(PathBuf::from("/Applications"));
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Applications"));
    }
    dirs
}

/// Deep enough to find real-world nested installs (verified case:
/// `~/Applications/CrossOver/Steam/Steam.app`, two levels deep) without
/// risking runaway recursion through a symlink cycle in an untrusted
/// directory tree.
const MAX_SCAN_DEPTH: u8 = 6;

/// A plain recursive directory walk, used only for the sealed-system paths
/// and the last-resort fallback — never the primary path for anywhere
/// Spotlight can see. See this module's doc comment.
fn scan_dir(dir: &Path, depth: u8, seen_ids: &mut HashSet<String>, out: &mut Vec<AppEntry>) {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "app") {
            if let Some(app) = read_app_bundle(&path)
                && seen_ids.insert(app.id.clone())
            {
                out.push(app);
            }
        } else if depth < MAX_SCAN_DEPTH && path.is_dir() {
            scan_dir(&path, depth + 1, seen_ids, out);
        }
    }
}

/// `kMDItemContentType`, not `kMDItemContentTypeTree`: on this machine the
/// tree variant matched dozens of unrelated files (stray `.service` files
/// with a seemingly mis-inferred type tree) that the direct-type predicate
/// does not — verified by diffing both queries' output. Every one of the
/// direct-type query's own results ends in `.app`, so no further UTI
/// filtering is needed.
const APP_BUNDLE_PREDICATE: &str = "kMDItemContentType == 'com.apple.application-bundle'";

/// One-shot query against Spotlight's live metadata index. `None` means
/// `mdfind` itself could not be run (see [`fallback_directories`]);
/// `Some(vec![])` is a trusted, successful "no matches" — see this
/// module's doc comment on why that is not treated as a failure.
fn query_spotlight_app_bundles() -> Option<Vec<PathBuf>> {
    let output = Command::new("mdfind").arg(APP_BUNDLE_PREDICATE).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    Some(
        text.lines()
            .map(PathBuf::from)
            .filter(|p| p.extension().is_some_and(|ext| ext == "app"))
            .collect(),
    )
}

/// Build (or rebuild) the full application index: Spotlight's metadata
/// index unioned with a plain scan of the sealed system directories (and,
/// only if the Spotlight query itself failed, the classic third-party
/// directories too). See this module's doc comment for the full
/// reasoning. Called once at daemon startup and again, from
/// [`watch_applications`], every time Spotlight reports a real change —
/// so it must stay cheap, and does: one `mdfind` process (sub-100ms on a
/// ~100-app machine) plus a handful of directory reads.
pub fn scan_applications() -> Vec<AppEntry> {
    let spotlight_paths = query_spotlight_app_bundles();
    let scan_dirs = if spotlight_paths.is_some() {
        sealed_system_directories()
    } else {
        fallback_directories()
    };

    let mut seen_ids = HashSet::new();
    let mut entries = Vec::new();
    for dir in scan_dirs {
        scan_dir(&dir, 0, &mut seen_ids, &mut entries);
    }
    scan_core_services_allowlist(&mut seen_ids, &mut entries);
    for path in spotlight_paths.unwrap_or_default() {
        if let Some(app) = read_app_bundle(&path)
            && seen_ids.insert(app.id.clone())
        {
            entries.push(app);
        }
    }
    entries
}

/// The bundle's on-screen name: `CFBundleDisplayName`, then `CFBundleName`,
/// then the `.app` filename's own stem — falling through past any
/// candidate that's *present but blank*, not just past a missing key. The
/// 147-app Spotlight-backed index surfaced a real bundle with
/// `CFBundleDisplayName` present and set to an empty (or whitespace-only)
/// string; the old `.and_then(|v| v.as_string()).unwrap_or_else(...)` chain
/// treated that as "found" (`as_string` returns `Some("")`, so the
/// `unwrap_or_else` fallback never ran), producing a row with a real icon
/// and a completely empty title. `None` only when every candidate,
/// including the filename stem, is blank — an entry with truly no usable
/// name anywhere is excluded rather than shown with an empty title.
fn bundle_display_name(dict: &plist::Dictionary, path: &Path) -> Option<String> {
    let from_key = |key: &str| {
        dict.get(key)
            .and_then(|v| v.as_string())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    from_key("CFBundleDisplayName").or_else(|| from_key("CFBundleName")).or_else(|| {
        path.file_stem()
            .map(|s| s.to_string_lossy().trim().to_owned())
            .filter(|s| !s.is_empty())
    })
}

/// Reads `path`'s `Info.plist` and returns `None` both for a bundle that
/// fails to parse *and* for one that parses fine but isn't something a
/// person would launch — see `is_nested_or_noisy` and the two Info.plist
/// checks below for the actual rule.
fn read_app_bundle(path: &Path) -> Option<AppEntry> {
    if is_nested_or_noisy(path) {
        return None;
    }

    let plist_path = path.join("Contents/Info.plist");
    let value: plist::Value = plist::from_file(&plist_path).ok()?;
    let dict = value.as_dictionary()?;

    // Real app bundles are `CFBundlePackageType == "APPL"`; installer
    // payloads, plugins, and other bundle kinds that happen to carry a
    // `.app` extension are not. Many legitimate small apps omit the key
    // entirely, so absence is not itself a signal — only a *wrong* value
    // is. `"FNDR"` is also accepted: `/System/Library/CoreServices/
    // Finder.app`'s own `Info.plist` carries that legacy four-char OSType
    // value (predating the `"APPL"` convention) instead of `"APPL"` — a
    // real, verified fact about Finder specifically (every other loose app
    // in that same directory — Installer, Siri, Game Center, Screen Time —
    // uses `"APPL"`), not a guess or a broadened filter.
    if let Some(pkg_type) = dict.get("CFBundlePackageType").and_then(|v| v.as_string())
        && pkg_type != "APPL"
        && pkg_type != "FNDR"
    {
        return None;
    }

    // `LSBackgroundOnly` — not `LSUIElement` — is the real signal for
    // "not something a person launches directly". `LSUIElement` only means
    // "no Dock icon"; Raycast, Rectangle, Tailscale, Docker, and
    // Amphetamine all set it and are all meant to be launchable (verified
    // against the actual installed bundles on the dev machine — every one
    // of the five sets `LSUIElement` and none set `LSBackgroundOnly`).
    // `LSBackgroundOnly` means the app has no user-facing UI surface at
    // all — nothing a launch could bring forward — which is exactly true
    // of the helper bundles this task named (verified: `~/Applications/
    // Claude Code URL Handler.app`, a URL-scheme-handler stub with no
    // window of its own, sets `LSBackgroundOnly` and does not set
    // `LSUIElement`).
    if dict
        .get("LSBackgroundOnly")
        .and_then(|v| v.as_boolean())
        .unwrap_or(false)
    {
        return None;
    }

    let name = bundle_display_name(dict, path)?;

    let id = dict
        .get("CFBundleIdentifier")
        .and_then(|v| v.as_string())
        .map(str::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    Some(AppEntry {
        id,
        name,
        path: path.to_path_buf(),
    })
}

/// Path-only checks, applied before any Info.plist read: a bundle nested
/// inside another `.app` or a `.framework` is a helper, plugin, XPC
/// service, or login item, never something launched on its own — verified
/// against the dev machine's own Spotlight results, which included exactly
/// this shape for e.g. `MobileDevice.framework`'s embedded helper/updater
/// `.app`s. The remaining substrings are Xcode/CI build products, installer
/// staging directories, and Script Editor's own template stubs — all
/// explicitly called out in the brief, and all present in this machine's
/// own Spotlight results before this filter was added (verified: several
/// `DerivedData`/`ios/build` products, and every stub under `Script
/// Editor/Templates` — that one is *not* caught by the nesting check above
/// since, on this machine at least, the templates live loose in a shared
/// `Templates` directory rather than inside `Script Editor.app` itself).
fn is_nested_or_noisy(path: &Path) -> bool {
    let nested = path.ancestors().skip(1).any(|p| {
        matches!(
            p.extension().and_then(|ext| ext.to_str()),
            Some("app") | Some("framework")
        )
    });
    if nested {
        return true;
    }

    const NOISY_SUBSTRINGS: &[&str] = &[
        "/DerivedData/",
        "/ios/build/",
        "/android/build/",
        "/.build/",
        "/private/var/folders/",
        "/private/tmp/",
        "/.Trash/",
        "PKInstallSandbox",
        "/Script Editor/Templates/",
    ];
    let path_str = path.to_string_lossy();
    NOISY_SUBSTRINGS.iter().any(|needle| path_str.contains(needle))
}

/// Spawns a background thread that keeps the application index live:
/// installs, moves, and removals are reflected without a daemon restart,
/// for as long as Spotlight is willing to report them. See this module's
/// doc comment for the API choice; the mechanics are documented inline
/// below since they carry real, previously-unobvious constraints.
pub fn watch_applications(on_change: impl Fn(Vec<AppEntry>) + Send + 'static) {
    std::thread::spawn(move || loop {
        if let Err(e) = run_live_watch(&on_change) {
            eprintln!("neko-core: spotlight live watch ended ({e}), retrying in 5s");
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}

fn run_live_watch(on_change: &impl Fn(Vec<AppEntry>)) -> std::io::Result<()> {
    // `mdfind -live` only ever reports a *count* of matches on each
    // change ("Query update: N matches" — confirmed against `mdfind`'s own
    // man page, and by direct experiment: it never re-prints the path
    // list on an update) — so it's used purely as a wake-up signal here;
    // the actual updated list still comes from a fresh
    // `query_spotlight_app_bundles` call, same as the initial scan.
    //
    // `NSUnbufferedIO=YES` is not optional: `mdfind` fully buffers stdout
    // once it isn't a TTY, so without it a real, already-indexed change
    // sat invisible on the read end for over 60 seconds in direct testing
    // (confirmed live by a concurrent one-shot `mdfind` call finding the
    // change within 3 seconds while `-live`'s own pipe stayed silent) —
    // "live" from Spotlight's side, silently stalled from this daemon's.
    let mut child = Command::new("mdfind")
        .arg("-live")
        .arg(APP_BUNDLE_PREDICATE)
        .env("NSUnbufferedIO", "YES")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| std::io::Error::other("mdfind -live: no stdout"))?;

    let (tx, rx) = mpsc::channel::<()>();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            if line.starts_with("Query update:") {
                let _ = tx.send(());
            }
        }
    });

    // Blocked on `recv`/`recv_timeout` the entire time nothing has
    // changed — and the reader thread above is blocked on a pipe read the
    // entire time `mdfind` hasn't printed anything — so this loop cannot
    // spin the CPU. A burst of filesystem churn (an installer touching
    // many files at once) is coalesced into a single rescan by draining
    // any further signals that land within 300ms of the first, rather
    // than rescanning once per line.
    while rx.recv().is_ok() {
        while rx.recv_timeout(Duration::from_millis(300)).is_ok() {}
        on_change(scan_applications());
    }

    let _ = child.kill();
    let _ = reader.join();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_bundle(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "neko-apps-test-{}-{}-{name}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn write_bundle(dir: &Path, plist_body: &str) {
        std::fs::create_dir_all(dir.join("Contents")).unwrap();
        std::fs::write(
            dir.join("Contents/Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>{plist_body}</dict></plist>"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn sealed_system_directories_are_the_three_recursively_scanned_paths() {
        // `/System/Library/CoreServices` itself is deliberately NOT here —
        // see `CORE_SERVICES_ALLOWED_APPS`'s doc comment for why that one
        // directory gets a named allowlist instead of a recursive scan.
        let dirs = sealed_system_directories();
        assert!(dirs.contains(&PathBuf::from("/System/Applications")));
        assert!(dirs.contains(&PathBuf::from("/System/Applications/Utilities")));
        assert!(dirs.contains(&PathBuf::from("/System/Library/CoreServices/Applications")));
        assert!(!dirs.contains(&PathBuf::from("/System/Library/CoreServices")));
    }

    #[test]
    fn fallback_directories_include_the_standard_third_party_locations() {
        let dirs = fallback_directories();
        assert!(dirs.contains(&PathBuf::from("/Applications")));
    }

    #[test]
    fn scanning_the_real_machine_finds_finder() {
        // A real, non-mocked scan against this machine's actual
        // CoreServices allowlist (the part of `scan_applications` that
        // never depends on `mdfind` being available in a test
        // environment). Asserts Finder specifically, not an OR against
        // "System Settings" — the OR version passed the entire time Finder
        // itself was absent from the scan, a false-negative-blind test.
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        scan_core_services_allowlist(&mut seen, &mut out);
        assert!(
            out.iter().any(|a| a.name == "Finder"),
            "expected to find Finder, found: {:?}",
            out.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn scanning_the_real_machine_excludes_a_verified_background_agent() {
        // `PowerChime.app`: confirmed by hand (`PlistBuddy`) to set
        // neither `LSBackgroundOnly` nor a distinguishing static signal
        // this module could filter on generally — see
        // `CORE_SERVICES_ALLOWED_APPS`'s doc comment for the full
        // investigation. The allowlist keeps it out simply by never
        // naming it, not by any property check.
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        scan_core_services_allowlist(&mut seen, &mut out);
        assert!(
            !out.iter().any(|a| a.name == "PowerChime"),
            "PowerChime must not appear in the CoreServices allowlist scan, found: {:?}",
            out.iter().map(|a| &a.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn core_services_allowlist_yields_exactly_the_five_wanted_apps() {
        let mut seen = HashSet::new();
        let mut out = Vec::new();
        scan_core_services_allowlist(&mut seen, &mut out);
        let mut names: Vec<&str> = out.iter().map(|a| a.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec!["Finder", "Game Center", "Installer", "Screen Time", "Siri"],
            "expected exactly the five verified-wanted CoreServices apps"
        );
    }

    #[test]
    fn an_ordinary_app_is_kept() {
        let dir = temp_bundle("Ordinary.app");
        write_bundle(
            &dir,
            r#"<key>CFBundleIdentifier</key><string>com.neko.ordinary</string>
               <key>CFBundlePackageType</key><string>APPL</string>
               <key>CFBundleName</key><string>Ordinary</string>"#,
        );
        let app = read_app_bundle(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(app.unwrap().name, "Ordinary");
    }

    #[test]
    fn a_blank_display_name_falls_back_to_bundle_name() {
        // `CFBundleDisplayName` present but empty — the real shape of the
        // captain's icon-with-no-title row. `as_string()` returns
        // `Some("")` for this, which the old `.unwrap_or_else` chain
        // treated as "found" and never fell through.
        let dir = temp_bundle("BlankDisplayName.app");
        write_bundle(
            &dir,
            r#"<key>CFBundleIdentifier</key><string>com.neko.blankdisplay</string>
               <key>CFBundlePackageType</key><string>APPL</string>
               <key>CFBundleDisplayName</key><string></string>
               <key>CFBundleName</key><string>RealName</string>"#,
        );
        let app = read_app_bundle(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(app.unwrap().name, "RealName");
    }

    #[test]
    fn a_whitespace_only_display_name_falls_back_to_the_filename_stem() {
        let dir = temp_bundle("WhitespaceName.app");
        write_bundle(
            &dir,
            r#"<key>CFBundleIdentifier</key><string>com.neko.whitespace</string>
               <key>CFBundlePackageType</key><string>APPL</string>
               <key>CFBundleDisplayName</key><string>   </string>"#,
        );
        let app = read_app_bundle(&dir);
        let expected_stem = dir.file_stem().unwrap().to_string_lossy().into_owned();
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(app.unwrap().name, expected_stem);
    }

    #[test]
    fn no_usable_name_anywhere_returns_none_rather_than_a_blank_title() {
        // Every candidate blank, including the filename stem itself — a
        // row with an icon and no title is never acceptable, so a bundle
        // this nameless is dropped rather than indexed with an empty name.
        // Tested against the pure helper directly: a real path whose own
        // `file_stem()` is blank isn't constructible through a temp
        // directory (any real final path component is non-empty), but an
        // empty `Path` reproduces the same "no candidate anywhere" case
        // `bundle_display_name` has to handle.
        let mut dict = plist::Dictionary::new();
        dict.insert("CFBundleDisplayName".to_string(), plist::Value::String(String::new()));
        dict.insert("CFBundleName".to_string(), plist::Value::String("   ".to_string()));
        assert_eq!(bundle_display_name(&dict, Path::new("")), None);
    }

    #[test]
    fn ls_ui_element_alone_does_not_exclude_a_menu_bar_app() {
        // The exact trap the brief calls out: Raycast, Rectangle,
        // Tailscale, Docker, and Amphetamine all set this, and are all
        // meant to stay launchable.
        let dir = temp_bundle("MenuBarApp.app");
        write_bundle(
            &dir,
            r#"<key>CFBundleIdentifier</key><string>com.neko.menubar</string>
               <key>CFBundlePackageType</key><string>APPL</string>
               <key>CFBundleName</key><string>MenuBarApp</string>
               <key>LSUIElement</key><true/>"#,
        );
        let app = read_app_bundle(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(app.is_some(), "an LSUIElement app must not be filtered out");
    }

    #[test]
    fn ls_background_only_excludes_a_helper() {
        // The real shape of `~/Applications/Claude Code URL Handler.app`
        // on the dev machine: a URL-scheme-handler stub with no UI at all.
        let dir = temp_bundle("URLHandler.app");
        write_bundle(
            &dir,
            r#"<key>CFBundleIdentifier</key><string>com.neko.handler</string>
               <key>CFBundlePackageType</key><string>APPL</string>
               <key>CFBundleName</key><string>URL Handler</string>
               <key>LSBackgroundOnly</key><true/>"#,
        );
        let app = read_app_bundle(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(app.is_none(), "an LSBackgroundOnly bundle must be filtered out");
    }

    #[test]
    fn a_non_application_package_type_is_excluded() {
        let dir = temp_bundle("Plugin.app");
        write_bundle(
            &dir,
            r#"<key>CFBundleIdentifier</key><string>com.neko.plugin</string>
               <key>CFBundlePackageType</key><string>BNDL</string>
               <key>CFBundleName</key><string>Plugin</string>"#,
        );
        let app = read_app_bundle(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(app.is_none());
    }

    #[test]
    fn a_bundle_nested_inside_another_app_is_excluded() {
        let host = temp_bundle("Host.app");
        let helper = host.join("Contents/Library/LoginItems/Helper.app");
        write_bundle(
            &helper,
            r#"<key>CFBundleIdentifier</key><string>com.neko.helper</string>
               <key>CFBundlePackageType</key><string>APPL</string>
               <key>CFBundleName</key><string>Helper</string>"#,
        );
        let app = read_app_bundle(&helper);
        std::fs::remove_dir_all(&host).unwrap();
        assert!(app.is_none(), "a bundle nested inside another .app must be filtered out");
    }

    #[test]
    fn a_bundle_nested_inside_a_framework_is_excluded() {
        let root = temp_bundle("Framework.framework");
        let helper = root.join("Versions/A/Helper.app");
        write_bundle(
            &helper,
            r#"<key>CFBundleIdentifier</key><string>com.neko.frameworkhelper</string>
               <key>CFBundlePackageType</key><string>APPL</string>
               <key>CFBundleName</key><string>FrameworkHelper</string>"#,
        );
        let app = read_app_bundle(&helper);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(app.is_none());
    }

    #[test]
    fn a_derived_data_build_product_is_excluded() {
        assert!(is_nested_or_noisy(&PathBuf::from(
            "/Users/x/Library/Developer/Xcode/DerivedData/Foo-abc/Build/Products/Debug/Foo.app"
        )));
        assert!(is_nested_or_noisy(&PathBuf::from("/Users/x/project/ios/build/Debug/Foo.app")));
    }

    #[test]
    fn a_script_editor_template_stub_is_excluded() {
        // Verified present in this machine's own raw Spotlight results and
        // *not* caught by the nesting check: these live loose in a shared
        // `Templates` directory, not inside `Script Editor.app` itself.
        assert!(is_nested_or_noisy(&PathBuf::from(
            "/Library/Application Support/Script Editor/Templates/Droplets/Recursive File Processing Droplet.app"
        )));
    }

    fn provider_with(apps: Vec<AppEntry>) -> AppsProvider {
        AppsProvider::new(Arc::new(RwLock::new(apps)), Arc::new(Mutex::new(crate::Db::open_in_memory().unwrap())))
    }

    #[test]
    fn provider_search_matches_by_fuzzy_name() {
        let provider = provider_with(vec![app_entry("a", "Safari"), app_entry("b", "Notes")]);
        let results = provider.search("saf", 1_000_000);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].item.id, "a");
        assert_eq!(results[0].item.kind, "app");
        assert_eq!(results[0].item.section_label, "Applications");
        // With nothing typed the same rows are a frecency-ranked suggestion,
        // and say so.
        let suggested = provider.search("", 0);
        assert!(suggested.iter().all(|c| c.item.section_label == "Suggested"));
        assert!(
            suggested.len() <= SUGGESTION_COUNT,
            "a suggestion list is short on purpose — the tail is every app on the machine, not more suggestions"
        );
        assert_eq!(results[0].item.action_label, "Open  ↵");
    }

    #[test]
    fn provider_search_boosts_recently_launched_apps_on_a_tied_fuzzy_score() {
        let db = crate::Db::open_in_memory().unwrap();
        db.record_launch("b", 1_000_000).unwrap();
        let provider =
            AppsProvider::new(Arc::new(RwLock::new(vec![app_entry("a", "Finder"), app_entry("b", "Finder")])), Arc::new(Mutex::new(db)));
        let results = provider.search("find", 1_000_000 + 1000);
        let best = results.iter().max_by(|a, b| a.score.total_cmp(&b.score)).unwrap();
        assert_eq!(best.item.id, "b");
    }

    #[test]
    fn provider_activate_launches_and_records_a_launch() {
        // No real `.app` bundle exists at this path, so `launch_app` (which
        // shells to `/usr/bin/open`) fails — this test only exercises the
        // "no such app" branch, not a real launch, to stay hermetic.
        let provider = provider_with(vec![app_entry("a", "Nonexistent")]);
        assert!(provider.activate("does-not-exist").is_err());
    }

    fn app_entry(id: &str, name: &str) -> AppEntry {
        AppEntry {
            id: id.to_string(),
            name: name.to_string(),
            path: PathBuf::from(format!("/Applications/{name}.app")),
        }
    }
}

#[cfg(test)]
mod suggestion_tests {
    use super::*;
    use crate::provider::Provider;

    /// The cap must keep the *best* few, not the first few the index happens
    /// to yield — otherwise "suggested" would mean "alphabetically early".
    #[test]
    fn an_empty_query_suggests_the_most_used_apps_not_an_arbitrary_slice() {
        let db = crate::Db::open_in_memory().unwrap();
        let apps: Vec<AppEntry> = (0..12)
            .map(|i| AppEntry {
                id: format!("/Applications/App{i}.app"),
                name: format!("App{i}"),
                path: std::path::PathBuf::from(format!("/Applications/App{i}.app")),
            })
            .collect();
        // The last three are the ones actually launched, so they are the
        // ones a suggestion list has to surface.
        for i in [9, 10, 11] {
            for _ in 0..5 {
                db.record_launch(&format!("/Applications/App{i}.app"), 1_000_000 + i as i64).unwrap();
            }
        }
        let provider = AppsProvider::new(
            std::sync::Arc::new(std::sync::RwLock::new(apps)),
            std::sync::Arc::new(std::sync::Mutex::new(db)),
        );

        let suggested = provider.search("", 2_000_000);
        assert_eq!(suggested.len(), SUGGESTION_COUNT);
        let ids: Vec<&str> = suggested.iter().map(|c| c.item.id.as_str()).collect();
        for i in [9, 10, 11] {
            let wanted = format!("/Applications/App{i}.app");
            assert!(ids.contains(&wanted.as_str()), "a launched app must be suggested, got {ids:?}");
        }
    }
}
