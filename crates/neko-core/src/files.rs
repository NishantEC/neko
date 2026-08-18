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
use std::time::Duration;

use neko_protocol::{Glyph, Icon, SearchItem};

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
    if NOISY_PATH_SUBSTRINGS.iter().any(|needle| path_str.contains(needle)) {
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
    "rs", "c", "h", "hpp", "cc", "cpp", "cxx", "m", "mm", "py", "js", "mjs", "cjs", "ts", "tsx", "jsx", "go", "rb",
    "java", "kt", "swift", "scala", "php", "cs", "sh", "bash", "zsh",
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
        .is_some_and(|ext| SOURCE_ARTIFACT_EXTENSIONS.iter().any(|candidate| candidate.eq_ignore_ascii_case(ext)))
}

fn default_scope_dirs() -> Vec<PathBuf> {
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
/// and what each protects against. Returns whatever was read before
/// whichever bound was hit first; `mdfind` itself is always killed before
/// returning, never left to run to completion in the background.
fn query_spotlight_paths(query: &str, dirs: &[PathBuf]) -> Vec<PathBuf> {
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

    let Ok(mut child) = command.spawn() else {
        return Vec::new();
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        return Vec::new();
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

    let paths = rx.recv_timeout(QUERY_TIMEOUT).unwrap_or_default();
    kill_and_reap(&mut child, reader);
    paths
}

fn kill_and_reap(child: &mut Child, reader: std::thread::JoinHandle<()>) {
    let _ = child.kill();
    let _ = child.wait();
    let _ = reader.join();
}

fn glyph_for(path: &Path) -> Glyph {
    if path.is_dir() { Glyph::Folder } else { Glyph::File }
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
        },
    }
}

/// The file-and-folder-search provider. Holds no persistent index of its
/// own — see this module's doc comment for why a live query per search is
/// the right shape here, unlike `AppsProvider`'s cached-and-watched index.
pub struct FileProvider {
    scope_dirs: Vec<PathBuf>,
}

impl FileProvider {
    pub fn new() -> Self {
        Self { scope_dirs: default_scope_dirs() }
    }

    /// A provider with an empty scope — always returns no candidates
    /// without ever shelling out to `mdfind`. For callers (daemon-level
    /// integration tests, mainly) that need a real `AppState`-shaped
    /// provider list but must stay hermetic and fast rather than depending
    /// on the test machine's own `~/Documents` contents.
    pub fn empty() -> Self {
        Self { scope_dirs: Vec::new() }
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

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let query = query.trim();
        if query.chars().count() < MIN_QUERY_LEN {
            return Vec::new();
        }
        query_spotlight_paths(query, &self.scope_dirs)
            .into_iter()
            .filter(|path| !is_noisy(path))
            .filter_map(|path| {
                let name = path.file_name()?.to_string_lossy().into_owned();
                let score = fuzzy_score(query, &name)?;
                let score = if is_source_artifact(&path) { score * SOURCE_ARTIFACT_DEMOTION } else { score };
                Some(build_candidate(score, path, name))
            })
            .take(MAX_CANDIDATES)
            .collect()
    }

    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        crate::launch::launch_app(Path::new(id)).map_err(|e| ProviderError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noisy_paths_are_excluded() {
        assert!(is_noisy(Path::new("/Users/x/Documents/proj/node_modules/leftpad/index.js")));
        assert!(is_noisy(Path::new("/Users/x/Documents/proj/target/debug/build")));
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
        assert!(is_noisy(Path::new("/Users/x/Documents/proj/env/lib/python3.8/site-packages/djangobower/finders.py")));
    }

    #[test]
    fn source_and_build_artifact_extensions_are_recognized() {
        assert!(is_source_artifact(Path::new("/Users/x/Documents/proj/terminal.rs")));
        assert!(is_source_artifact(Path::new("/Users/x/Documents/proj/terminal.h")));
        assert!(is_source_artifact(Path::new("/Users/x/Documents/proj/terminal.svg")));
        assert!(is_source_artifact(Path::new("/Users/x/Documents/proj/module.pyc")));
        assert!(is_source_artifact(Path::new("/Users/x/Documents/proj/Main.CLASS")), "extension match must be case-insensitive");
    }

    #[test]
    fn documents_images_and_folders_are_never_source_artifacts() {
        assert!(!is_source_artifact(Path::new("/Users/x/Documents/report.pdf")));
        assert!(!is_source_artifact(Path::new("/Users/x/Documents/photo.heic")));
        assert!(!is_source_artifact(Path::new("/Users/x/Documents/notes.txt")));
        assert!(!is_source_artifact(Path::new("/Users/x/Documents/project"))); // no extension
    }

    #[test]
    fn a_source_artifact_is_demoted_but_not_dropped() {
        let raw = fuzzy_score("terminal", "terminal.rs").unwrap();
        let score = if is_source_artifact(Path::new("terminal.rs")) { raw * SOURCE_ARTIFACT_DEMOTION } else { raw };
        assert!(score > 0.0, "a source-artifact match must still be a real, positive-scoring candidate");
        assert!(score < raw, "it must score lower than an equivalent non-artifact match would");
    }

    #[test]
    fn home_relative_parent_abbreviates_with_a_tilde() {
        let home = Path::new("/Users/testuser");
        assert_eq!(
            home_relative_parent_within(Path::new("/Users/testuser/Documents/report.pdf"), Some(home)).as_deref(),
            Some("~/Documents")
        );
        assert_eq!(
            home_relative_parent_within(Path::new("/Users/testuser/report.pdf"), Some(home)).as_deref(),
            Some("~")
        );
        assert_eq!(
            home_relative_parent_within(Path::new("/Volumes/External/report.pdf"), Some(home)).as_deref(),
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
        let provider = FileProvider { scope_dirs: default_scope_dirs() };
        assert_eq!(provider.search("a", 0).len(), 0);
        assert_eq!(provider.search("", 0).len(), 0);
    }

    #[test]
    fn an_empty_scope_returns_no_candidates() {
        let provider = FileProvider { scope_dirs: Vec::new() };
        assert_eq!(provider.search("readme", 0).len(), 0);
    }

    #[test]
    fn provider_search_sets_generic_row_fields() {
        let candidate = build_candidate(1.0, PathBuf::from("/Users/x/Documents/report.pdf"), "report.pdf".to_string());
        assert_eq!(candidate.item.kind, "file");
        assert_eq!(candidate.item.section_label, "Files");
        assert_eq!(candidate.item.action_label, "Open  ↵");
        assert_eq!(candidate.item.title, "report.pdf");
        assert_eq!(candidate.item.id, "/Users/x/Documents/report.pdf");
    }
}
