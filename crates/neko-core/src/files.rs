//! File and folder search — the third provider, and the proof that the
//! `Provider` seam (`provider.rs`) genuinely supports "just another result
//! type in the same fast list" rather than being designed in the abstract.
//! Source of truth is the same one `apps.rs` already established for
//! application discovery: Spotlight's own metadata index via `mdfind`, not
//! a hand-rolled directory walk — see that module's doc comment for the
//! full case for `mdfind` over linking a metadata-query API directly.
//!
//! **Prefix matching, not the app provider's fuzzy subsequence matching —
//! measured, not guessed.** The natural first attempt, `kMDItemFSName ==
//! '*query*'cd` (query anywhere in the filename, the same shape
//! `apps.rs` uses for `com.apple.application-bundle`), was tested directly
//! against this machine's real `~/Documents` — which, like the captain's
//! own machine per the launch brief, holds real development repositories
//! with large `node_modules`/`target` trees. A single-character leading-
//! wildcard query (`*n*'cd`) matched 55,191 files and took **~13 seconds**
//! to complete; a two-character one was still multiple seconds. Spotlight
//! can serve a *prefix* query (`'query*'cd`, no leading wildcard) from its
//! index directly — the same query re-run as `'n*'cd` returned in well
//! under a second. A leading wildcard forces Spotlight to fall back to a
//! slower per-item comparison it can't use its index for. This is a real,
//! load-bearing trade-off, not a minor tuning knob: file search here only
//! matches names that *start with* the query, not names that merely
//! *contain* it — "mtg" will not match "meeting-notes.txt" the way a
//! fuzzy app-name search would. Reasonable for a launcher (Spotlight's own
//! UI makes the same trade for its live-typing results) and the only way
//! this provider stays fast against a directory tree of unknown size.
//!
//! **Bounded regardless of how broad the query turns out to be.**
//! Even a fast-to-start prefix query can still be broad (a 2-character
//! prefix matched thousands of files in testing). [`query_spotlight_paths`]
//! never waits for `mdfind` to finish on its own: a background reader
//! thread (the same spawn-plus-channel shape `apps.rs`'s `run_live_watch`
//! already uses) stops reading after [`MAX_RAW_RESULTS`] lines, and the
//! caller gives it at most [`QUERY_TIMEOUT`] wall-clock time via
//! `recv_timeout` either way, killing the child process the moment either
//! limit is hit. A query that would otherwise run for seconds is capped to
//! a bounded, predictable latency — see `AGENTS.md`'s "Provider
//! abstraction" section for the actual measured numbers against a real,
//! repository-heavy `~/Documents`.
//!
//! **Scope: the captain's own documents and common working locations, not
//! the whole disk.** `~/Documents`, `~/Desktop`, `~/Downloads` — the
//! classic "things a person put here on purpose" locations, the same shape
//! Spotlight's own Finder integration highlights by default. Not
//! configurable in this task (no settings UI exists for it yet — see
//! `AGENTS.md` for the seam a follow-up would add, mirroring
//! `clipboard_history_enabled`'s daemon-setting pattern). Noisy
//! subdirectories that happen to live inside those locations (`node_modules`,
//! `target`, `.git`, build output, ...) are filtered out of the results
//! after the query, in [`is_noisy`] — `mdfind` has no directory-exclusion
//! flag, so this can't be pushed into the query itself, only applied to
//! what comes back.
//!
//! **Source/build artifacts are demoted, not excluded.** A `.rs`/`.h`/`.py`/
//! `.svg` sitting in an ordinary, non-hidden project folder used to be a
//! fully eligible result for any short generic query, competing on equal
//! footing with a genuinely-wanted document. [`SOURCE_ARTIFACT_DEMOTION`]
//! scales such a match's score down instead of dropping it — a developer
//! does sometimes want to find a source file by name, so it still shows
//! up, just no longer crowds out a document/PDF/image match for the same
//! query. See [`is_source_artifact`] for exactly what counts.
//!
//! **The apparent-duplicate investigation (launch brief: "settle it,
//! either way, with evidence").** A prior audit saw what looked like two
//! identical `finders.py` rows and couldn't resolve, from a `/tmp` test
//! fixture, whether that was a real duplicate-emission bug or two
//! legitimately different files. Settled here against the real machine:
//! `mdfind -onlyin ~/Documents -onlyin ~/Desktop -onlyin ~/Downloads
//! "kMDItemFSName == 'finders.py*'cd"` returns exactly two lines,
//! `.../django/contrib/staticfiles/finders.py` and
//! `.../djangobower/finders.py` — two different real files at two
//! different real paths, each a distinct Django-ecosystem package shipping
//! its own module of that name inside the same vendored virtualenv. Not a
//! duplicate-emission bug: [`query_spotlight_paths`] has no dedup logic
//! because it never receives the same path twice from `mdfind` in the
//! first place. The row *was* already distinguishable — [`home_relative_parent`]
//! puts each file's own parent directory in the subtitle, so the two rows
//! read `finders.py — ~/.../staticfiles` and `finders.py — ~/.../djangobower`,
//! not two blank-subtitle duplicates — but both were also exactly the kind
//! of vendored-dependency noise `node_modules`/`vendor` are already
//! filtered for, which is the more useful fix: `/site-packages/` is now in
//! `NOISY_PATH_SUBSTRINGS`, so neither shows up for a query this generic
//! any more. Raw `mdfind` output preserved in
//! `docs/evidence/ranking-before-after.md`.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use neko_protocol::{Glyph, Icon, SearchItem};

use crate::cancel::Cancel;
use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

/// Below this, a prefix query is either meaningless (empty) or, per this
/// module's doc comment, broad enough to be both slow and useless (the
/// measured single-character case matched 55k+ files). Two characters is
/// the same practical floor most launchers use for live filename search.
const MIN_QUERY_LEN: usize = 2;

/// How many raw `mdfind` result lines to read before cutting the query off
/// — deliberately more than [`MAX_CANDIDATES`] so noisy-path filtering has
/// real candidates left to work with, not so many that a broad query's
/// bound stops being tight.
const MAX_RAW_RESULTS: usize = 40;

/// The final number of candidates a single search can hand back, after
/// noise filtering and re-scoring — the panel only ever shows a handful of
/// any one section, so there is no benefit to keeping more.
const MAX_CANDIDATES: usize = 10;

/// Hard wall-clock ceiling on one `mdfind` invocation, independent of
/// [`MAX_RAW_RESULTS`] — the backstop for a query shape this module's own
/// measurements didn't anticipate. Measured directly (not guessed): even
/// with `NSUnbufferedIO` fixing the buffering bug described above, *time
/// to first streamed result* for a broad 2-4 character prefix against this
/// machine's real, repository-heavy `~/Documents`/`~/Desktop`/`~/Downloads`
/// varied from under 100ms to a little over 1.2s across repeated runs of
/// the same query — real variance in how long Spotlight's own query
/// planning takes for a broad predicate, not something this module can
/// smooth out. 900ms cut off real, useful results often enough in that
/// range to be a regression, not just a rare tail; 1.5s gives real
/// headroom while staying well short of "the whole request visibly hangs."
const QUERY_TIMEOUT: Duration = Duration::from_millis(1500);

const NOISY_PATH_SUBSTRINGS: &[&str] = &[
    "/node_modules/",
    "/target/",
    "/.git/",
    "/DerivedData/",
    "/.build/",
    "/build/",
    "/dist/",
    "/Pods/",
    "/.venv/",
    "/venv/",
    "/vendor/",
    "/.Trash/",
    // Python's `node_modules` equivalent — vendored third-party package
    // code a `pip`/virtualenv install unpacks, regardless of what the venv
    // root directory itself happens to be named (".venv", "venv", "env",
    // or anything else a project picked, unlike the fixed substrings
    // above). Confirmed live against a real query on this machine's actual
    // `~/Documents`: querying "finders.py" returned two hits with the same
    // filename — `django/contrib/staticfiles/finders.py` and
    // `djangobower/finders.py` — both under a virtualenv literally named
    // `env` (not matched by "/venv/" or "/.venv/" above), both genuinely
    // different files, not a duplicate-emission bug (see this module's
    // "the apparent-duplicate investigation" doc section below) — but both
    // exactly the kind of vendored dependency noise `node_modules`/`vendor`
    // are already filtered for. See `docs/evidence/ranking-before-after.md`
    // for the raw `mdfind` output this was verified against.
    "/site-packages/",
];

/// True for a path that's noise for a "find something I saved" search: a
/// build artefact, a dependency tree, or anything hidden (a dotfile or a
/// path with a hidden ancestor directory) — mirrors `apps.rs`'s own
/// `is_nested_or_noisy` in spirit, a different concrete list since the
/// noise here is build/dependency output rather than nested app bundles.
/// `.app` bundles are excluded too: they're already covered by
/// `AppsProvider`, and showing the same bundle twice (once as an app, once
/// as a folder) would be confusing, not additive.
fn is_noisy(path: &Path) -> bool {
    if path.extension().is_some_and(|ext| ext == "app") {
        return true;
    }
    let path_str = path.to_string_lossy();
    if NOISY_PATH_SUBSTRINGS
        .iter()
        .any(|needle| path_str.contains(needle))
    {
        return true;
    }
    path.components()
        .any(|c| c.as_os_str().to_string_lossy().starts_with('.'))
}

/// Extensions of compiled/source-code artefacts — incidental to a codebase
/// rather than something a person searches for by name the way they'd
/// search for a document, image, or PDF. Deliberately **not** excluded
/// outright (the launch brief: "a developer does sometimes want them") —
/// see [`SOURCE_ARTIFACT_DEMOTION`], applied in [`FileProvider::search`]
/// instead of a hard filter here. Scoped to source/compiled-code and
/// project-metadata files specifically; document formats (`.pdf`, `.docx`,
/// `.pages`, `.key`, `.numbers`, `.txt`, `.md`), images (`.png`, `.jpg`,
/// `.heic`, ...) and plain folders are never in this list and are never
/// demoted.
const SOURCE_ARTIFACT_EXTENSIONS: &[&str] = &[
    // Compiled/bytecode output — never what "find this by name" means.
    "pyc", "pyo", "o", "obj", "class", "so", "dylib", "a", "rlib",
    // Source code, across the languages this codebase's own machines and
    // the captain's repos are most likely to contain.
    "rs", "c", "h", "hpp", "cc", "cpp", "cxx", "m", "mm", "py", "js", "mjs", "cjs", "ts", "tsx",
    "jsx", "go", "rb", "java", "kt", "swift", "scala", "php", "cs", "sh", "bash", "zsh",
    // Markup/data formats that are almost always project source, not a
    // document a person saved for its own sake — an SVG in a repo is an
    // icon asset, not artwork someone's looking for by filename.
    "svg", "css", "scss",
];

/// Multiplies a source-artifact candidate's [`fuzzy_score`] down instead of
/// dropping it — large enough to reliably lose to a same-scoring
/// non-artifact match (a document, image, or folder), small enough that an
/// exact, high-confidence source-file match still competes normally
/// against everything else, per this task's launch brief: "do not exclude
/// source files outright."
///
/// **`0.85`, not the more aggressive `0.5` first tried — caught live, not
/// assumed.** A real query against the captain's own machine ("code",
/// alongside `AppsProvider`'s own real, unboosted `fuzzy_score` matches
/// for "Xcode" ≈ 8.9, "Cloudflare WARP"/"Cloudless Voice" ≈ 10.7 — none of
/// these get [`crate::search`]'s app-category bonus, since none of them
/// are a genuine prefix match on "code") showed the interaction a smaller
/// demotion factor misses: at `0.5`, three clearly-relevant source files
/// ("CodexAdapter.ts", "CodexDriver.ts", "CodexProvider.ts", each
/// `fuzzy_score` ≈ 13.7, a real prefix match on "Code") demoted to ≈ 6.85
/// — *below* those three unrelated, merely-coincidental app matches — so
/// the file section lost real, wanted results to app-search noise that
/// the demotion feature was never meant to promote. `0.85` keeps every
/// one of those three real files comfortably above all three of those
/// real scattered-app scores (≈ 11.6 vs. ≤ 10.7) while still cutting a
/// source file's competitive weight by 15% against a same-scoring
/// document — enough to consistently lose a contested slot to one
/// (`docs/evidence/ranking-before-after.md` has the full before/after
/// numbers for both factors).
const SOURCE_ARTIFACT_DEMOTION: f32 = 0.85;

/// See [`SOURCE_ARTIFACT_EXTENSIONS`]. Extension comparison is
/// case-insensitive (`README.MD`-style all-caps extensions are common
/// enough on real filesystems to be worth not missing).
fn is_source_artifact(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            SOURCE_ARTIFACT_EXTENSIONS
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(ext))
        })
}

pub fn default_scope_dirs() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME") else {
        return Vec::new();
    };
    let home = PathBuf::from(home);
    ["Documents", "Desktop", "Downloads"]
        .into_iter()
        .map(|dir| home.join(dir))
        .filter(|p| p.is_dir())
        .collect()
}

/// The `~` (or `~/relative/path`) form of `path`'s parent directory, for
/// the row's subtitle — falls back to the parent's own full path when it
/// isn't under `$HOME` (a scope directory reached via a symlink elsewhere,
/// for instance).
fn home_relative_parent(path: &Path) -> Option<String> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    home_relative_parent_within(path, home.as_deref())
}

/// The pure part of [`home_relative_parent`], with `home` passed in rather
/// than read from the environment — testable without mutating the
/// process-wide `HOME` variable, which would race every other test in this
/// crate that also reads it (`cargo test` runs a crate's tests in parallel
/// by default).
fn home_relative_parent_within(path: &Path, home: Option<&Path>) -> Option<String> {
    let parent = path.parent()?;
    Some(match home {
        Some(home) if parent == home => "~".to_string(),
        Some(home) if parent.starts_with(home) => {
            format!("~/{}", parent.strip_prefix(home).ok()?.display())
        }
        _ => parent.display().to_string(),
    })
}

/// Escapes the two characters that would otherwise break out of the
/// predicate's own single-quoted string literal.
fn escape_predicate_literal(query: &str) -> String {
    query.replace('\\', "\\\\").replace('\'', "\\'")
}

/// One bounded `mdfind` query — see this module's doc comment for why both
/// bounds ([`MAX_RAW_RESULTS`] lines, [`QUERY_TIMEOUT`] wall-clock) exist
/// and what each protects against, and [`run_bounded_child`] for the third
/// bound this task added (`cancel`: the captain typed another character and
/// this query's answer is no longer wanted). Returns whatever was read
/// before whichever bound was hit first; `mdfind` itself is always killed
/// before returning, never left to run to completion in the background.
fn query_spotlight_paths(query: &str, dirs: &[PathBuf], cancel: &Cancel) -> Vec<PathBuf> {
    if dirs.is_empty() {
        return Vec::new();
    }
    let predicate = format!("kMDItemFSName == '{}*'cd", escape_predicate_literal(query));
    let mut command = Command::new("mdfind");
    for dir in dirs {
        command.arg("-onlyin").arg(dir);
    }
    // Same fix `apps.rs`'s `run_live_watch` already needed for `mdfind
    // -live`, for the identical reason: `mdfind` fully buffers stdout once
    // it isn't a TTY, so without this a fast, large result set can sit
    // invisible on the read end well past this query's own deadline even
    // though `mdfind` itself would have finished quickly — confirmed live
    // here the same way `apps.rs` confirmed it for `-live` (a query that
    // completed in well under a second at the shell produced zero results
    // through this reader before `NSUnbufferedIO` was added).
    command
        .arg(predicate)
        .env("NSUnbufferedIO", "YES")
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    run_bounded_child(command, cancel).0
}

/// `NEKO_FILE_SEARCH_DELAY_MS` — verification-only, unset by default, never
/// read in normal operation. Same pattern (and same reasoning) as
/// `neko-daemon`'s own `NEKO_ICON_EXTRACT_DELAY_MS`: real hardware answers
/// a prefix query against a real corpus in tens of milliseconds, which is
/// genuinely too fast to land a screenshot *between* the fast providers'
/// results and this one's, even though that intermediate state is exactly
/// what the two-phase search exists to produce and therefore exactly what
/// has to be shown to be believed.
///
/// Stretching this provider out is the only way to capture that window
/// without synthetic input or a doctored screenshot. It also makes the
/// cancellation path live-verifiable rather than only unit-tested: with a
/// multi-second delay set, a superseded keystroke visibly abandons its
/// query instead of holding a thread.
fn verification_delay() -> Option<Duration> {
    std::env::var("NEKO_FILE_SEARCH_DELAY_MS")
        .ok()?
        .parse::<u64>()
        .ok()
        .map(Duration::from_millis)
}

/// Sleeps for `delay` in [`CANCEL_POLL_INTERVAL`] slices, returning `false`
/// if `cancel` fired before it elapsed. `None` returns `true` immediately —
/// the normal, un-delayed path.
fn sleep_unless_cancelled(delay: Option<Duration>, cancel: &Cancel) -> bool {
    let Some(delay) = delay else { return true };
    let deadline = std::time::Instant::now() + delay;
    while std::time::Instant::now() < deadline {
        if cancel.is_cancelled() {
            return false;
        }
        std::thread::sleep(CANCEL_POLL_INTERVAL);
    }
    !cancel.is_cancelled()
}

/// How often [`run_bounded_child`] wakes to re-check `cancel` while waiting
/// on the reader thread. Small enough that a superseded query's child dies
/// within a keystroke's own interval rather than lingering, large enough
/// that the wait is still a blocked thread rather than a spin — the whole
/// point of cancelling is to stop competing with the *next* query for
/// Spotlight's own query planner, so a cancellation that arrives 100ms late
/// would defeat most of the benefit.
const CANCEL_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Runs `command` to at most three bounds — [`MAX_RAW_RESULTS`] lines,
/// [`QUERY_TIMEOUT`] wall-clock, and `cancel` — killing and reaping the
/// child before returning under every one of them, including the ordinary
/// "it finished on its own" path.
///
/// Returns the lines read *and the reaped child's own exit status*, which
/// exists purely so a test can prove the third bound did what it claims:
/// a cancelled run's status carries `SIGKILL` in `ExitStatusExt::signal()`,
/// which is direct evidence the child process was killed rather than
/// merely abandoned with its output discarded. [`query_spotlight_paths`]
/// itself discards the status.
fn run_bounded_child(
    mut command: Command,
    cancel: &Cancel,
) -> (Vec<PathBuf>, Option<std::process::ExitStatus>) {
    let Ok(mut child) = command.spawn() else {
        return (Vec::new(), None);
    };
    let Some(stdout) = child.stdout.take() else {
        let status = kill_and_reap(&mut child, None);
        return (Vec::new(), status);
    };

    let (tx, rx) = mpsc::channel::<Vec<PathBuf>>();
    let reader = std::thread::spawn(move || {
        let mut paths = Vec::new();
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            paths.push(PathBuf::from(line));
            if paths.len() >= MAX_RAW_RESULTS {
                break;
            }
        }
        let _ = tx.send(paths);
    });

    // The wait `recv_timeout(QUERY_TIMEOUT)` used to be, sliced so `cancel`
    // is actually observable: a single long blocking wait cannot notice a
    // token that flips halfway through it.
    let deadline = std::time::Instant::now() + QUERY_TIMEOUT;
    let paths = loop {
        if cancel.is_cancelled() {
            break Vec::new();
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            break Vec::new();
        }
        match rx.recv_timeout(remaining.min(CANCEL_POLL_INTERVAL)) {
            Ok(paths) => break paths,
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break Vec::new(),
        }
    };
    let status = kill_and_reap(&mut child, Some(reader));
    (paths, status)
}

/// Kills `child` and waits for it, returning its reaped status. `reader`,
/// when present, is joined *after* the kill — killing the child closes its
/// stdout, which is what unblocks a reader thread still sitting in
/// `BufRead::lines`; joining first would deadlock for as long as the child
/// stayed alive.
fn kill_and_reap(
    child: &mut Child,
    reader: Option<std::thread::JoinHandle<()>>,
) -> Option<std::process::ExitStatus> {
    let _ = child.kill();
    let status = child.wait().ok();
    if let Some(reader) = reader {
        let _ = reader.join();
    }
    status
}

fn glyph_for(path: &Path) -> Glyph {
    if path.is_dir() {
        Glyph::Folder
    } else {
        Glyph::File
    }
}

fn build_candidate(score: f32, path: PathBuf, name: String) -> Candidate {
    let id = path.to_string_lossy().into_owned();
    Candidate {
        score,
        item: SearchItem {
            id,
            kind: "file".to_string(),
            title: name,
            subtitle: home_relative_parent(&path),
            icon: Icon::Glyph(glyph_for(&path)),
            section_label: "Files".to_string(),
            // Enter opens the item — `/usr/bin/open` on a file launches it
            // in its default app, on a folder opens it in Finder, matching
            // exactly what "Open" already means for `AppsProvider` and
            // what Spotlight's own Enter key does. A separate "reveal in
            // Finder" action would be a real secondary action (matching
            // Raycast's ⌘-Enter) but there is no secondary-action affordance
            // anywhere in this panel yet — see `AGENTS.md`'s "Provider
            // abstraction" section for why this wasn't added speculatively.
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
            speaker: None,
            images: Vec::new(),
            preview: None,
        },
    }
}

/// The file-and-folder-search provider. Holds no persistent index of its
/// own — see this module's doc comment for why a live query per search is
/// the right shape here, unlike `AppsProvider`'s cached-and-watched index.
pub struct FileProvider {
    scope: Scope,
}

/// Where this provider's search scope comes from.
///
/// `Configured` re-reads the persisted list (`crate::preferences::
/// get_search_folders`) on every query rather than caching it, so a folder
/// added or removed in the Search Folders screen takes effect on the very
/// next keystroke with no daemon restart and no invalidation message. The
/// cost is one point query against a two-column KV table per search —
/// microseconds against SQLite's page cache, and far below the `mdfind`
/// round-trip this same call is about to make anyway.
enum Scope {
    Fixed(Vec<PathBuf>),
    Configured(Arc<Mutex<crate::Db>>),
}

impl FileProvider {
    pub fn new() -> Self {
        Self {
            scope: Scope::Fixed(default_scope_dirs()),
        }
    }

    /// The real daemon's constructor: scope follows the persisted setting.
    pub fn with_db(db: Arc<Mutex<crate::Db>>) -> Self {
        Self {
            scope: Scope::Configured(db),
        }
    }

    fn scope_dirs(&self) -> Vec<PathBuf> {
        match &self.scope {
            Scope::Fixed(dirs) => dirs.clone(),
            Scope::Configured(db) => {
                let db = db.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                crate::preferences::get_search_folders(&db)
                    .into_iter()
                    .filter(|d| d.is_dir())
                    .collect()
            }
        }
    }

    /// A provider with an empty scope — always returns no candidates
    /// without ever shelling out to `mdfind`. For callers (daemon-level
    /// integration tests, mainly) that need a real `AppState`-shaped
    /// provider list but must stay hermetic and fast rather than depending
    /// on the test machine's own `~/Documents` contents.
    pub fn empty() -> Self {
        Self {
            scope: Scope::Fixed(Vec::new()),
        }
    }
}

impl Default for FileProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for FileProvider {
    fn id(&self) -> &'static str {
        "file"
    }

    fn section_label(&self) -> &'static str {
        "Files"
    }

    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate> {
        self.search_cancellable(query, now_unix_ms, &Cancel::never())
    }

    /// True only when this provider will actually shell out to `mdfind`
    /// for `query` — below [`MIN_QUERY_LEN`], or with no scope directory to
    /// search (`FileProvider::empty`, or a home directory with no
    /// `Documents`/`Desktop`/`Downloads`), `search` returns instantly with
    /// nothing and there is nothing worth deferring. Getting this wrong in
    /// the "yes" direction is not merely wasteful: it would make the daemon
    /// answer every such query in two frames, the second one identical to
    /// the first. See [`Provider::defers_for`].
    fn defers_for(&self, query: &str) -> bool {
        !self.scope_dirs().is_empty() && query.trim().chars().count() >= MIN_QUERY_LEN
    }

    fn search_cancellable(
        &self,
        query: &str,
        _now_unix_ms: i64,
        cancel: &Cancel,
    ) -> Vec<Candidate> {
        let query = query.trim();
        if query.chars().count() < MIN_QUERY_LEN {
            return Vec::new();
        }
        if !sleep_unless_cancelled(verification_delay(), cancel) {
            return Vec::new();
        }
        query_spotlight_paths(query, &self.scope_dirs(), cancel)
            .into_iter()
            .filter(|path| !is_noisy(path))
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                let score = fuzzy_score(query, &name)?;
                let score = if is_source_artifact(&path) {
                    score * SOURCE_ARTIFACT_DEMOTION
                } else {
                    score
                };
                Some(build_candidate(score, path, name))
            })
            .take(MAX_CANDIDATES)
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        crate::launch::launch_app(Path::new(id)).map_err(|e| ProviderError(e.to_string()))
    }

    /// The file's own path is its id, so both actions are direct.
    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        crate::provider::perform_path_action(Path::new(id), action)
    }
}

// ---------------------------------------------------------------- Inside documents

/// Content search is slower than a name prefix and can match many files, so
/// it waits longer and keeps whatever arrived when the deadline passes.
const DOCUMENT_TIMEOUT: Duration = Duration::from_secs(4);

fn run_streaming_child(mut command: Command, cancel: &Cancel, timeout: Duration) -> Vec<PathBuf> {
    let Ok(mut child) = command.spawn() else { return Vec::new() };
    let Some(stdout) = child.stdout.take() else {
        kill_and_reap(&mut child, None);
        return Vec::new();
    };
    let found = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let (sink, finished) = (found.clone(), done.clone());
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let mut paths = sink.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            paths.push(PathBuf::from(line));
            if paths.len() >= MAX_RAW_RESULTS {
                break;
            }
        }
        finished.store(true, std::sync::atomic::Ordering::Release);
    });
    let deadline = std::time::Instant::now() + timeout;
    while !done.load(std::sync::atomic::Ordering::Acquire) && std::time::Instant::now() < deadline && !cancel.is_cancelled() {
        std::thread::sleep(CANCEL_POLL_INTERVAL);
    }
    kill_and_reap(&mut child, None);
    if cancel.is_cancelled() {
        return Vec::new();
    }
    let paths = found.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone();
    paths
}


/// Words people use around a document name that never help find it.
const DOCUMENT_FILLER: &[&str] = &[
    "my", "the", "a", "an", "find", "show", "open", "where", "is", "are", "that", "this", "file", "files",
    "doc", "docs", "document", "documents", "pc", "mac", "computer", "laptop", "please", "me", "for", "of", "in", "on",
    "copy", "scan", "scanned", "latest", "old", "new",
];

/// Names for the same document. Each group is searched as alternatives.
const DOCUMENT_SYNONYMS: &[&[&str]] = &[
    &["resume", "cv", "curriculum", "résumé"],
    &["aadhaar", "aadhar", "adhar", "uidai", "आधार"],
    &["licence", "license", "driving"],
    &["passport"],
    &["invoice", "bill"],
    &["receipt"],
    &["payslip", "salary", "pay"],
    &["statement", "bank"],
    &["offer", "appointment"],
    &["insurance", "policy"],
    &["pan", "income-tax"],
];

/// The distinct search terms for a natural document request, each with its
/// synonyms: "find my CV" → [[resume, cv, curriculum, résumé]].
pub fn document_terms(query: &str) -> Vec<Vec<String>> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    for word in query.split(|c: char| c.is_whitespace() || matches!(c, ',' | '?' | '!' | '"')) {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '-').to_lowercase();
        if word.chars().count() < 2 || DOCUMENT_FILLER.contains(&word.as_str()) {
            continue;
        }
        let group: Vec<String> = DOCUMENT_SYNONYMS
            .iter()
            .find(|g| g.contains(&word.as_str()))
            .map(|g| g.iter().map(|s| s.to_string()).collect())
            .unwrap_or_else(|| vec![word.clone()]);
        if !groups.contains(&group) && groups.len() < 4 {
            groups.push(group);
        }
    }
    groups
}

/// Every term group must match the name or the indexed text, by word prefix.
fn document_predicate(groups: &[Vec<String>]) -> String {
    groups
        .iter()
        .map(|group| {
            let alternatives: Vec<String> = group
                .iter()
                .flat_map(|term| {
                    let term = escape_predicate_literal(term);
                    [format!("kMDItemTextContent == '{term}*'cdw"), format!("kMDItemDisplayName == '{term}*'cdw")]
                })
                .collect();
            format!("({})", alternatives.join(" || "))
        })
        .collect::<Vec<_>>()
        .join(" && ")
}

/// "Search inside documents": Spotlight's indexed text plus names, for
/// natural requests like "my driving licence" or "aadhaar". A separate mode
/// so the tuned filename search in the root list is untouched. It does not
/// OCR scans Spotlight hasn't indexed.
pub struct DocumentProvider {
    files: FileProvider,
}

impl DocumentProvider {
    pub fn with_db(db: Arc<Mutex<crate::Db>>) -> Self {
        Self { files: FileProvider::with_db(db) }
    }
    pub fn empty() -> Self {
        Self { files: FileProvider::empty() }
    }
}

impl Provider for DocumentProvider {
    fn id(&self) -> &'static str {
        "document"
    }

    fn section_label(&self) -> &'static str {
        "Documents"
    }

    fn search(&self, query: &str, now_unix_ms: i64) -> Vec<Candidate> {
        self.search_cancellable(query, now_unix_ms, &Cancel::never())
    }

    fn search_cancellable(&self, query: &str, _now_unix_ms: i64, cancel: &Cancel) -> Vec<Candidate> {
        let groups = document_terms(query);
        let dirs = self.files.scope_dirs();
        if groups.is_empty() || dirs.is_empty() {
            return Vec::new();
        }
        let mut command = Command::new("mdfind");
        for dir in &dirs {
            command.arg("-onlyin").arg(dir);
        }
        command.arg(document_predicate(&groups)).env("NSUnbufferedIO", "YES").stdout(Stdio::piped()).stderr(Stdio::null());
        let names: Vec<String> = groups.iter().flatten().cloned().collect();
        let mut found: Vec<(f32, std::time::SystemTime, Candidate)> = run_streaming_child(command, cancel, DOCUMENT_TIMEOUT)
            .into_iter()
            .filter(|path| !is_noisy(path) && path.is_file())
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                let lower = name.to_lowercase();
                // Name matches first, content-only matches after; never a source artifact.
                let score = if names.iter().any(|n| lower.contains(n.as_str())) { 2.0 } else { 1.0 };
                if is_source_artifact(&path) {
                    return None;
                }
                let modified = std::fs::metadata(&path).and_then(|m| m.modified()).unwrap_or(std::time::UNIX_EPOCH);
                let mut candidate = build_candidate(score, path, name);
                candidate.item.section_label = "Documents".into();
                Some((score, modified, candidate))
            })
            .collect();
        found.sort_by(|a, b| b.0.total_cmp(&a.0).then(b.1.cmp(&a.1)));
        found.into_iter().take(MAX_CANDIDATES).map(|(_, _, c)| c).collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        self.files.activate(id)
    }

    fn perform_action(&self, id: &str, action: &str) -> Result<(), ProviderError> {
        self.files.perform_action(id, action)
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;

    #[test]
    fn natural_document_requests_become_synonym_groups() {
        assert_eq!(document_terms("find my CV"), vec![vec!["resume", "cv", "curriculum", "résumé"]]);
        assert_eq!(document_terms("aadhar card")[0][0], "aadhaar");
        assert_eq!(document_terms("my driving license scan").len(), 1, "driving and license are one document");
        assert!(document_terms("the file on my pc").is_empty());
        let predicate = document_predicate(&document_terms("o'brien invoice"));
        assert!(predicate.contains("kMDItemTextContent == 'o\\'brien*'cdw"), "{predicate}");
        assert!(predicate.contains(") && ("), "every term must match");
    }

    /// A stand-in for `mdfind` with the same observable shape this module
    /// depends on: it streams one result line immediately, then stays alive
    /// far longer than [`QUERY_TIMEOUT`]. `sleep` is invoked directly (no
    /// `sh -c` wrapper) so the process this test's `Child` handle refers to
    /// is genuinely the long-running one, not a shell that may or may not
    /// `exec` away from under it.
    fn long_running_child_that_emits_one_line() -> Command {
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("echo /tmp/neko-cancel-fixture.txt; exec sleep 30")
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        command
    }

    #[test]
    fn a_cancelled_query_abandons_the_child_immediately_and_kills_it() {
        // The defect this proves fixed: before `Cancel` existed, a query
        // superseded by the next keystroke still held this thread for the
        // full `QUERY_TIMEOUT` and left its `mdfind` child running for that
        // whole window, competing with the query the captain actually
        // wanted. `SIGKILL` in the reaped status is direct evidence the
        // child process was killed, not merely abandoned with its output
        // thrown away.
        let cancel = Cancel::new();
        let canceller = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            canceller.cancel();
        });

        let started = std::time::Instant::now();
        let (paths, status) = run_bounded_child(long_running_child_that_emits_one_line(), &cancel);
        let elapsed = started.elapsed();

        assert!(
            elapsed < QUERY_TIMEOUT / 2,
            "a cancelled query must return promptly, not run out its own {QUERY_TIMEOUT:?} bound (took {elapsed:?})"
        );
        assert!(
            paths.is_empty(),
            "a cancelled query contributes nothing, even if a line had already streamed in"
        );
        assert_eq!(
            status.and_then(|s| s.signal()),
            Some(9),
            "the child process must be SIGKILLed, not left running to complete on its own"
        );
    }

    #[test]
    fn a_query_cancelled_before_it_starts_never_waits_at_all() {
        // The common real shape: the supersede arrives while this request
        // is still queued behind its own thread spawn. Nothing should block
        // for any measurable time.
        let cancel = Cancel::new();
        cancel.cancel();
        let started = std::time::Instant::now();
        let (paths, status) = run_bounded_child(long_running_child_that_emits_one_line(), &cancel);
        assert!(started.elapsed() < Duration::from_millis(500));
        assert!(paths.is_empty());
        assert_eq!(status.and_then(|s| s.signal()), Some(9));
    }

    #[test]
    fn an_uncancelled_query_still_returns_the_lines_the_child_streamed() {
        // The cancellation plumbing must not change the ordinary path: a
        // child that finishes on its own still hands back everything it
        // printed.
        let mut command = Command::new("/bin/sh");
        command
            .arg("-c")
            .arg("echo /tmp/one.txt; echo /tmp/two.txt")
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let (paths, _status) = run_bounded_child(command, &Cancel::never());
        assert_eq!(
            paths,
            vec![PathBuf::from("/tmp/one.txt"), PathBuf::from("/tmp/two.txt")]
        );
    }

    #[test]
    fn the_file_provider_only_defers_once_the_query_is_long_enough_to_query_spotlight() {
        let provider = FileProvider {
            scope: Scope::Fixed(vec![PathBuf::from("/tmp")]),
        };
        assert!(
            !provider.defers_for(""),
            "an empty query never reaches mdfind"
        );
        assert!(
            !provider.defers_for("a"),
            "a single character is below MIN_QUERY_LEN"
        );
        assert!(
            !provider.defers_for("  a  "),
            "whitespace does not count toward the minimum"
        );
        assert!(
            provider.defers_for("do"),
            "two characters is the point mdfind actually runs"
        );
    }

    #[test]
    fn a_verification_delay_is_abandoned_the_moment_the_query_is_superseded() {
        let cancel = Cancel::new();
        let canceller = cancel.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(30));
            canceller.cancel();
        });
        let started = std::time::Instant::now();
        let completed = sleep_unless_cancelled(Some(Duration::from_secs(30)), &cancel);
        assert!(
            !completed,
            "a cancelled delay reports that it did not elapse"
        );
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "and returns without waiting the rest of it out"
        );
    }

    #[test]
    fn no_verification_delay_configured_means_no_wait_at_all() {
        let started = std::time::Instant::now();
        assert!(sleep_unless_cancelled(None, &Cancel::never()));
        assert!(started.elapsed() < Duration::from_millis(50));
    }

    #[test]
    fn a_provider_with_no_scope_directories_never_defers_however_long_the_query() {
        // Otherwise the daemon would answer every such query in two frames,
        // the second one byte-identical to the first.
        assert!(!FileProvider::empty().defers_for("documents"));
    }

    #[test]
    fn noisy_paths_are_excluded() {
        assert!(is_noisy(Path::new(
            "/Users/x/Documents/proj/node_modules/leftpad/index.js"
        )));
        assert!(is_noisy(Path::new(
            "/Users/x/Documents/proj/target/debug/build"
        )));
        assert!(is_noisy(Path::new("/Users/x/Documents/proj/.git/HEAD")));
        assert!(is_noisy(Path::new("/Users/x/Documents/.hidden/file.txt")));
        assert!(is_noisy(Path::new("/Users/x/Documents/Safari.app")));
    }

    #[test]
    fn an_ordinary_document_is_not_noisy() {
        assert!(!is_noisy(Path::new("/Users/x/Documents/report.pdf")));
        assert!(!is_noisy(Path::new("/Users/x/Documents/project/README.md")));
    }

    #[test]
    fn a_vendored_python_dependency_is_noisy() {
        assert!(is_noisy(Path::new(
            "/Users/x/Documents/proj/env/lib/python3.8/site-packages/django/contrib/staticfiles/finders.py"
        )));
        assert!(is_noisy(Path::new(
            "/Users/x/Documents/proj/env/lib/python3.8/site-packages/djangobower/finders.py"
        )));
    }

    #[test]
    fn source_and_build_artifact_extensions_are_recognized() {
        assert!(is_source_artifact(Path::new(
            "/Users/x/Documents/proj/terminal.rs"
        )));
        assert!(is_source_artifact(Path::new(
            "/Users/x/Documents/proj/terminal.h"
        )));
        assert!(is_source_artifact(Path::new(
            "/Users/x/Documents/proj/terminal.svg"
        )));
        assert!(is_source_artifact(Path::new(
            "/Users/x/Documents/proj/module.pyc"
        )));
        assert!(
            is_source_artifact(Path::new("/Users/x/Documents/proj/Main.CLASS")),
            "extension match must be case-insensitive"
        );
    }

    #[test]
    fn documents_images_and_folders_are_never_source_artifacts() {
        assert!(!is_source_artifact(Path::new(
            "/Users/x/Documents/report.pdf"
        )));
        assert!(!is_source_artifact(Path::new(
            "/Users/x/Documents/photo.heic"
        )));
        assert!(!is_source_artifact(Path::new(
            "/Users/x/Documents/notes.txt"
        )));
        assert!(!is_source_artifact(Path::new("/Users/x/Documents/project"))); // no extension
    }

    #[test]
    fn a_source_artifact_is_demoted_but_not_dropped() {
        let raw = fuzzy_score("terminal", "terminal.rs").unwrap();
        let score = if is_source_artifact(Path::new("terminal.rs")) {
            raw * SOURCE_ARTIFACT_DEMOTION
        } else {
            raw
        };
        assert!(
            score > 0.0,
            "a source-artifact match must still be a real, positive-scoring candidate"
        );
        assert!(
            score < raw,
            "it must score lower than an equivalent non-artifact match would"
        );
    }

    #[test]
    fn home_relative_parent_abbreviates_with_a_tilde() {
        let home = Path::new("/Users/testuser");
        assert_eq!(
            home_relative_parent_within(
                Path::new("/Users/testuser/Documents/report.pdf"),
                Some(home)
            )
            .as_deref(),
            Some("~/Documents")
        );
        assert_eq!(
            home_relative_parent_within(Path::new("/Users/testuser/report.pdf"), Some(home))
                .as_deref(),
            Some("~")
        );
        assert_eq!(
            home_relative_parent_within(Path::new("/Volumes/External/report.pdf"), Some(home))
                .as_deref(),
            Some("/Volumes/External")
        );
    }

    #[test]
    fn predicate_literals_are_escaped() {
        assert_eq!(escape_predicate_literal("it's"), "it\\'s");
        assert_eq!(escape_predicate_literal(r"back\slash"), r"back\\slash");
    }

    #[test]
    fn a_query_shorter_than_the_minimum_returns_no_candidates_without_querying() {
        let provider = FileProvider {
            scope: Scope::Fixed(default_scope_dirs()),
        };
        assert_eq!(provider.search("a", 0).len(), 0);
        assert_eq!(provider.search("", 0).len(), 0);
    }

    #[test]
    fn an_empty_scope_returns_no_candidates() {
        let provider = FileProvider {
            scope: Scope::Fixed(Vec::new()),
        };
        assert_eq!(provider.search("readme", 0).len(), 0);
    }

    #[test]
    fn provider_search_sets_generic_row_fields() {
        let candidate = build_candidate(
            1.0,
            PathBuf::from("/Users/x/Documents/report.pdf"),
            "report.pdf".to_string(),
        );
        assert_eq!(candidate.item.kind, "file");
        assert_eq!(candidate.item.section_label, "Files");
        assert_eq!(candidate.item.action_label, "Open  ↵");
        assert_eq!(candidate.item.title, "report.pdf");
        assert_eq!(candidate.item.id, "/Users/x/Documents/report.pdf");
    }
}
