//! neko's own preferences — the settings a person changes about *neko*,
//! as opposed to `crate::settings`, which searches macOS's System Settings
//! panes. The two are deliberately separate providers with separate ids
//! (`"preference"` here, `"settings"` there): they answer different
//! questions and a query for one should not be diluted by the other.
//!
//! **Why this is a provider at all, rather than a preferences window.**
//! Every other surface in this app is a mode over a provider's list
//! (`AGENTS.md`, "Commands and modes"), and a preferences screen is a list
//! of settings. Modelling it the same way means the row rendering, the
//! section header, the `⌘K` menu and the footer verb all come for free, and
//! typing "hotkey" in the root list can find the setting directly rather
//! than only finding a container to open.
//!
//! **The three settings and where each one is actually performed:**
//!
//! | Setting | Row behaviour | Performed by |
//! | --- | --- | --- |
//! | Summon Hotkey | enters `preference.hotkey` | the **client** — a live OS registration needs the client's run loop (`AGENTS.md`, "The daemon/client split") |
//! | Launch at Login | toggles in place | the **daemon** — [`set_launch_at_login`] below |
//! | Search Folders | enters `preference.folders` | the **daemon** — [`set_search_folders`], read by `crate::files` |
//!
//! Only the middle one is an ordinary `Provider::activate`. The other two
//! carry `SearchItem::enters_mode`, which the client handles without ever
//! building a `Request::Activate` — the same path `commands.rs`'s rows take.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use neko_protocol::{Glyph, Icon, ItemAction, SearchItem};

use crate::provider::{Provider, ProviderError};
use crate::search::{Candidate, fuzzy_score};

const LAUNCH_AT_LOGIN_KEY: &str = "launch_at_login";
const SEARCH_FOLDERS_KEY: &str = "file_search_scope";

/// Row ids. Stable strings, not indices — they are what
/// `Request::Activate`'s `id` carries back, so reordering [`ROWS`] must
/// never change which setting a row activates.
pub const ROW_HOTKEY: &str = "hotkey";
pub const ROW_LAUNCH_AT_LOGIN: &str = "launch-at-login";
pub const ROW_SEARCH_FOLDERS: &str = "search-folders";
pub const ROW_AGENTS_ENABLED: &str = "agents-enabled";
pub const ROW_AGENTS_INCLUDE_IDLE: &str = "agents-include-idle";

/// The mode a row enters, for the two settings that need their own screen.
pub const MODE_HOTKEY: &str = "preference.hotkey";
pub const MODE_FOLDERS: &str = "preference.folders";

/// One preference row's fixed, compiled-in description. Aliases exist for
/// the same reason `commands.rs`'s do: `fuzzy_score` is a strict in-order
/// subsequence match, so "shortcut" never falls out of "Summon Hotkey" on
/// its own, and "startup" never falls out of "Launch at Login".
struct RowSpec {
    id: &'static str,
    title: &'static str,
    aliases: &'static [&'static str],
    subtitle: &'static str,
    enters_mode: Option<&'static str>,
    action_label: &'static str,
    glyph: Glyph,
}

const ROWS: &[RowSpec] = &[
    RowSpec {
        id: ROW_HOTKEY,
        title: "Summon Hotkey",
        aliases: &["Summon Hotkey", "Hotkey", "Shortcut", "Keyboard Shortcut", "Summon"],
        subtitle: "The key combination that opens neko",
        enters_mode: Some(MODE_HOTKEY),
        action_label: "Change  ↵",
        glyph: Glyph::Sliders,
    },
    RowSpec {
        id: ROW_LAUNCH_AT_LOGIN,
        title: "Launch at Login",
        aliases: &["Launch at Login", "Login", "Startup", "Start at Login", "Autostart"],
        subtitle: "Start neko automatically when you log in",
        enters_mode: None,
        action_label: "Toggle  ↵",
        glyph: Glyph::Sliders,
    },
    RowSpec {
        id: ROW_SEARCH_FOLDERS,
        title: "Search Folders",
        aliases: &["Search Folders", "Folders", "File Search Scope", "Directories", "Scope"],
        subtitle: "Where file search looks",
        enters_mode: Some(MODE_FOLDERS),
        action_label: "Edit  ↵",
        glyph: Glyph::Folder,
    },
    RowSpec {
        id: ROW_AGENTS_ENABLED,
        title: "Show Agents",
        aliases: &["Show Agents", "Agents", "Coding Agents", "Paseo"],
        subtitle: "List running coding agents in search results",
        enters_mode: None,
        action_label: "Toggle  ↵",
        glyph: Glyph::Agent,
    },
    RowSpec {
        id: ROW_AGENTS_INCLUDE_IDLE,
        title: "Include Idle Agents",
        aliases: &["Include Idle Agents", "Idle Agents", "Idle"],
        subtitle: "Match agents that are not currently running",
        enters_mode: None,
        action_label: "Toggle  ↵",
        glyph: Glyph::Agent,
    },
];

// ---------------------------------------------------------------------
// Persisted values
// ---------------------------------------------------------------------

pub fn get_launch_at_login(db: &crate::Db) -> rusqlite::Result<bool> {
    Ok(db.get_setting(LAUNCH_AT_LOGIN_KEY)?.as_deref() == Some("true"))
}

/// Flips the persisted flag **and** reconciles the real LaunchAgent on
/// disk. The two are set together on purpose: a persisted `true` with no
/// agent installed would render as "On" while doing nothing at all, which
/// is exactly the class of silent lie `AGENTS.md`'s "verified, not trusted"
/// readbacks exist to prevent elsewhere in this codebase.
pub fn set_launch_at_login(db: &crate::Db, enabled: bool) -> Result<(), String> {
    if enabled {
        launch_agent::install()?;
    } else {
        launch_agent::remove()?;
    }
    db.set_setting(LAUNCH_AT_LOGIN_KEY, if enabled { "true" } else { "false" })
        .map_err(|e| format!("couldn't save the setting: {e}"))
}

/// The folders file search looks in. Falls back to
/// [`crate::files::default_scope_dirs`] when nothing has been persisted —
/// so an untouched install behaves exactly as it did before this setting
/// existed, and "reset to defaults" is just deleting the key.
pub fn get_search_folders(db: &crate::Db) -> Vec<PathBuf> {
    let stored = db.get_setting(SEARCH_FOLDERS_KEY).ok().flatten();
    let Some(raw) = stored else {
        return crate::files::default_scope_dirs();
    };
    match serde_json::from_str::<Vec<String>>(&raw) {
        // A persisted-but-empty list is a real choice ("search nothing"),
        // not a missing value, so it is honoured rather than falling back.
        Ok(paths) => paths.into_iter().map(PathBuf::from).collect(),
        // A corrupted cosmetic setting degrades to the default rather than
        // erroring, the same rule `themes.rs` applies to an unknown theme id.
        Err(_) => crate::files::default_scope_dirs(),
    }
}

pub fn set_search_folders(db: &crate::Db, folders: &[PathBuf]) -> Result<(), String> {
    let as_strings: Vec<String> = folders.iter().map(|p| p.display().to_string()).collect();
    let json = serde_json::to_string(&as_strings).map_err(|e| e.to_string())?;
    db.set_setting(SEARCH_FOLDERS_KEY, &json)
        .map_err(|e| format!("couldn't save the setting: {e}"))
}

/// Adds one folder, rejecting anything that isn't a real directory. Returns
/// the new list. Adding a folder already in the list is a silent no-op
/// rather than an error — the person's intent ("this folder should be
/// searched") is already satisfied.
pub fn add_search_folder(db: &crate::Db, folder: &str) -> Result<Vec<PathBuf>, String> {
    let expanded = expand_tilde(folder);
    if !expanded.is_dir() {
        return Err(format!("not a folder: {}", expanded.display()));
    }
    let mut folders = get_search_folders(db);
    if !folders.iter().any(|f| f == &expanded) {
        folders.push(expanded);
        set_search_folders(db, &folders)?;
    }
    Ok(folders)
}

pub fn remove_search_folder(db: &crate::Db, folder: &str) -> Result<Vec<PathBuf>, String> {
    let target = PathBuf::from(folder);
    let mut folders = get_search_folders(db);
    folders.retain(|f| f != &target);
    set_search_folders(db, &folders)?;
    Ok(folders)
}

/// `~/Documents` → `/Users/me/Documents`. Typed paths are the only way to
/// add a folder (there is no native folder picker — see this module's doc
/// comment), and `~` is what a person actually types.
pub fn expand_tilde(raw: &str) -> PathBuf {
    let trimmed = raw.trim();
    if let Some(rest) = trimmed.strip_prefix("~/")
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home).join(rest);
    }
    if trimmed == "~"
        && let Some(home) = std::env::var_os("HOME")
    {
        return PathBuf::from(home);
    }
    PathBuf::from(trimmed)
}

// ---------------------------------------------------------------------
// Launch at login
// ---------------------------------------------------------------------

/// Launch-at-login via a `~/Library/LaunchAgents` plist, **not**
/// `SMAppService`.
///
/// `SMAppService::mainApp` is the modern API and would be the right choice
/// for a packaged app — but it registers *the calling app's bundle*, and
/// neko today is a bare Mach-O binary (`target/release/neko`), not a
/// `.app`. There is no bundle for it to register. A LaunchAgent plist has
/// no such requirement, so it is what actually works for how this app is
/// currently built and run. **If neko is ever packaged as a `.app`, this
/// should move to `SMAppService`** — the plist route needs the binary to
/// stay at the same path, which a real installed app would guarantee and a
/// `cargo build` output directory does not.
pub mod launch_agent {
    use super::*;

    pub const LABEL: &str = "com.neko.launcher";

    pub fn plist_path() -> Option<PathBuf> {
        let home = std::env::var_os("HOME")?;
        Some(PathBuf::from(home).join("Library/LaunchAgents").join(format!("{LABEL}.plist")))
    }

    /// The client binary to launch — the sibling `neko` next to whichever
    /// binary is running this code (in the real daemon, `neko-daemon`).
    /// Launching the *client* is correct and the daemon is deliberately not
    /// launched directly: the client spawns the daemon itself on startup
    /// (`neko`'s `daemon_launcher`), so starting the client starts both,
    /// while starting the daemon alone would leave no window to summon.
    pub fn client_binary_path() -> Result<PathBuf, String> {
        let exe = std::env::current_exe().map_err(|e| format!("couldn't locate neko: {e}"))?;
        let dir = exe.parent().ok_or_else(|| "couldn't locate neko's directory".to_string())?;
        let candidate = dir.join("neko");
        if candidate.is_file() {
            return Ok(candidate);
        }
        // Already running as the client itself (a test harness, or a future
        // build where the two are one binary).
        if exe.file_name().is_some_and(|n| n == "neko") {
            return Ok(exe);
        }
        Err(format!("couldn't find the neko binary next to {}", dir.display()))
    }

    pub fn is_installed() -> bool {
        plist_path().is_some_and(|p| p.is_file())
    }

    pub fn install() -> Result<(), String> {
        let binary = client_binary_path()?;
        let path = plist_path().ok_or_else(|| "no HOME set".to_string())?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| format!("couldn't create {}: {e}", parent.display()))?;
        }
        std::fs::write(&path, plist_contents(&binary)).map_err(|e| format!("couldn't write {}: {e}", path.display()))?;
        // Best-effort: the plist alone is enough for the *next* login, which
        // is what the setting promises. `bootstrap` only additionally makes
        // it live in this session, and legitimately fails if it is already
        // loaded — never a reason to report the setting as failed.
        let _ = std::process::Command::new("/bin/launchctl").arg("load").arg("-w").arg(&path).output();
        Ok(())
    }

    pub fn remove() -> Result<(), String> {
        let path = plist_path().ok_or_else(|| "no HOME set".to_string())?;
        let _ = std::process::Command::new("/bin/launchctl").arg("unload").arg("-w").arg(&path).output();
        match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            // Already absent is the desired end state, not a failure.
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("couldn't remove {}: {e}", path.display())),
        }
    }

    /// Built by hand rather than through a plist serializer: this is a
    /// fixed five-key document, and the one dynamic value is a path.
    pub fn plist_contents(binary: &Path) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <false/>
</dict>
</plist>
"#,
            xml_escape(&binary.display().to_string())
        )
    }

    fn xml_escape(raw: &str) -> String {
        raw.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }
}

// ---------------------------------------------------------------------
// The provider
// ---------------------------------------------------------------------

pub struct PreferencesProvider {
    db: Arc<Mutex<crate::Db>>,
}

impl PreferencesProvider {
    pub fn new(db: Arc<Mutex<crate::Db>>) -> Self {
        Self { db }
    }

    fn accessory_for(&self, id: &str) -> Option<String> {
        let db = self.db.lock().unwrap();
        match id {
            // Read live rather than cached at construction: a rebind from
            // the Summon Hotkey screen persists through the daemon, and this
            // row must show the new combination the very next time it is
            // rendered, not after a restart.
            ROW_HOTKEY => Some(crate::hotkey::get_hotkey(&db).ok()?.combo.display()),
            ROW_LAUNCH_AT_LOGIN => {
                Some(if get_launch_at_login(&db).unwrap_or(false) { "On" } else { "Off" }.to_string())
            }
            ROW_SEARCH_FOLDERS => {
                let n = get_search_folders(&db).len();
                Some(if n == 1 { "1 folder".to_string() } else { format!("{n} folders") })
            }
            ROW_AGENTS_ENABLED => {
                Some(if crate::agents::agents_enabled(&db) { "On" } else { "Off" }.to_string())
            }
            ROW_AGENTS_INCLUDE_IDLE => {
                Some(if crate::agents::include_idle(&db) { "On" } else { "Off" }.to_string())
            }
            _ => None,
        }
    }
}

impl Provider for PreferencesProvider {
    fn id(&self) -> &'static str {
        "preference"
    }

    fn section_label(&self) -> &'static str {
        "Preferences"
    }

    /// A settings row is only ever wanted in answer to asking for it.
    /// "Launch at Login" sitting in the root list before anything has been
    /// typed is noise — and was exactly that, live, before this existed.
    fn answers_empty_root_query(&self) -> bool {
        false
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        ROWS.iter()
            .filter_map(|row| {
                // An empty query lists every setting, because the
                // Preferences window loads its values with exactly that
                // (scoped) request. Keeping these rows out of the *root*
                // list is `answers_empty_root_query` below — a different
                // question, asked in a different place.
                let score = if query.trim().is_empty() {
                    Some(1.0)
                } else {
                    row.aliases
                        .iter()
                        .filter_map(|alias| fuzzy_score(query, alias))
                        .fold(None, |best: Option<f32>, s| Some(best.map_or(s, |b| b.max(s))))
                }?;
                Some(Candidate {
                    score,
                    item: SearchItem {
                        id: row.id.to_string(),
                        kind: "preference".to_string(),
                        title: row.title.to_string(),
                        subtitle: Some(row.subtitle.to_string()),
                        icon: Icon::Glyph(row.glyph),
                        section_label: "Preferences".to_string(),
                        action_label: row.action_label.to_string(),
                        badge: None,
                        accessory: self.accessory_for(row.id),
                        enters_mode: row.enters_mode.map(str::to_string),
                        group_label: None,
                        actions: Vec::new(),
                        source: None,
                        meter: None,
                        keeps_open: false,
                        preview: None,
                    },
                })
            })
            .collect()
    }

    /// Only [`ROW_LAUNCH_AT_LOGIN`] is a real daemon-side action; the other
    /// two rows carry `enters_mode` and are handled entirely client-side,
    /// so reaching them here means the client's own check was bypassed.
    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        match id {
            ROW_LAUNCH_AT_LOGIN => {
                let db = self.db.lock().unwrap();
                let now = get_launch_at_login(&db).unwrap_or(false);
                set_launch_at_login(&db, !now).map_err(ProviderError)
            }
            ROW_AGENTS_ENABLED => {
                let db = self.db.lock().unwrap();
                let now = crate::agents::agents_enabled(&db);
                crate::agents::set_agents_enabled(&db, !now)
                    .map_err(|e| ProviderError(format!("couldn't save the setting: {e}")))
            }
            ROW_AGENTS_INCLUDE_IDLE => {
                let db = self.db.lock().unwrap();
                let now = crate::agents::include_idle(&db);
                crate::agents::set_include_idle(&db, !now)
                    .map_err(|e| ProviderError(format!("couldn't save the setting: {e}")))
            }
            ROW_HOTKEY | ROW_SEARCH_FOLDERS => {
                Err(ProviderError(format!("{id} opens its own screen; it has no direct action")))
            }
            other => Err(ProviderError(format!("no such preference: {other}"))),
        }
    }
}

// ---------------------------------------------------------------------
// The folder-scope list (mode-only)
// ---------------------------------------------------------------------

/// The list behind the `preference.folders` mode: one row per configured
/// search folder, each removable.
///
/// **Registered as a mode-only provider** (`AppState::mode_providers`), not
/// alongside the root-list providers — a folder path is not something a
/// person searching the root list wants back, and unlike every other
/// provider these rows only make sense inside their own screen.
pub struct FolderScopeProvider {
    db: Arc<Mutex<crate::Db>>,
}

impl FolderScopeProvider {
    pub fn new(db: Arc<Mutex<crate::Db>>) -> Self {
        Self { db }
    }
}

impl Provider for FolderScopeProvider {
    fn id(&self) -> &'static str {
        "folder-scope"
    }

    fn section_label(&self) -> &'static str {
        "Search Folders"
    }

    fn search(&self, query: &str, _now_unix_ms: i64) -> Vec<Candidate> {
        let folders = {
            let db = self.db.lock().unwrap();
            get_search_folders(&db)
        };
        folders
            .into_iter()
            .map(|folder| {
                let display = folder.display().to_string();
                let name = folder
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| display.clone());
                // The query in this mode is a *path being typed to add*, not
                // a filter — so it must never hide existing rows. Scoring is
                // flat and the list is always whole.
                let _ = query;
                Candidate {
                    score: 1.0,
                    item: SearchItem {
                        id: display.clone(),
                        kind: "folder-scope".to_string(),
                        title: name,
                        subtitle: Some(display),
                        icon: Icon::Glyph(Glyph::Folder),
                        section_label: "Search Folders".to_string(),
                        action_label: "Remove  ↵".to_string(),
                        badge: None,
                        accessory: None,
                        enters_mode: None,
                        group_label: None,
                        actions: vec![ItemAction {
                            id: "remove".to_string(),
                            label: "Remove Folder".to_string(),
                            destructive: true,
                        }],
                        source: None,
                        meter: None,
                        keeps_open: false,
                        preview: None,
                    },
                }
            })
            .collect()
    }

    /// The primary action on a folder row is removing it — there is nothing
    /// else to do to a configured path from here.
    fn activate(&self, id: &str) -> Result<(), ProviderError> {
        let db = self.db.lock().unwrap();
        remove_search_folder(&db, id).map(|_| ()).map_err(ProviderError)
    }

    fn perform_action(&self, id: &str, action_id: &str) -> Result<(), ProviderError> {
        match action_id {
            // `id` is a path being *typed*, not one of this provider's own
            // rows — the one action here whose subject does not already
            // exist in the list. Validation lives in `add_search_folder`, so
            // a typo comes back as a message the panel can show inline
            // rather than silently adding a folder that is not there.
            "add" => add_search_folder(&self.db.lock().unwrap(), id).map(|_| ()).map_err(ProviderError),
            "remove" => self.activate(id),
            other => Err(ProviderError(format!("no action '{other}' on this row"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> crate::Db {
        crate::Db::open_in_memory().expect("in-memory db")
    }

    #[test]
    fn search_folders_falls_back_to_the_built_in_scope_when_nothing_is_persisted() {
        let db = db();
        assert_eq!(get_search_folders(&db), crate::files::default_scope_dirs());
    }

    #[test]
    fn a_persisted_empty_list_is_honoured_rather_than_falling_back() {
        let db = db();
        set_search_folders(&db, &[]).unwrap();
        assert!(get_search_folders(&db).is_empty(), "an explicit empty scope is a real choice");
    }

    #[test]
    fn a_corrupted_scope_setting_degrades_to_the_default_instead_of_erroring() {
        let db = db();
        db.set_setting(SEARCH_FOLDERS_KEY, "not json at all").unwrap();
        assert_eq!(get_search_folders(&db), crate::files::default_scope_dirs());
    }

    #[test]
    fn adding_a_path_that_is_not_a_directory_is_rejected_and_changes_nothing() {
        let db = db();
        set_search_folders(&db, &[]).unwrap();
        let err = add_search_folder(&db, "/definitely/not/a/real/directory").unwrap_err();
        assert!(err.contains("not a folder"), "got: {err}");
        assert!(get_search_folders(&db).is_empty());
    }

    #[test]
    fn adding_the_same_folder_twice_does_not_duplicate_it() {
        let db = db();
        set_search_folders(&db, &[]).unwrap();
        let tmp = std::env::temp_dir();
        add_search_folder(&db, &tmp.display().to_string()).unwrap();
        let after = add_search_folder(&db, &tmp.display().to_string()).unwrap();
        assert_eq!(after.len(), 1, "a repeat add is a no-op, not an error and not a duplicate");
    }

    #[test]
    fn removing_a_folder_drops_exactly_that_one() {
        let db = db();
        let tmp = std::env::temp_dir();
        set_search_folders(&db, &[tmp.clone(), PathBuf::from("/usr")]).unwrap();
        let after = remove_search_folder(&db, &tmp.display().to_string()).unwrap();
        assert_eq!(after, vec![PathBuf::from("/usr")]);
    }

    #[test]
    fn tilde_expands_against_home_and_a_bare_path_is_left_alone() {
        let home = std::env::var("HOME").expect("HOME");
        assert_eq!(expand_tilde("~/Documents"), PathBuf::from(&home).join("Documents"));
        assert_eq!(expand_tilde("~"), PathBuf::from(&home));
        assert_eq!(expand_tilde("/usr/local"), PathBuf::from("/usr/local"));
        assert_eq!(expand_tilde("  /usr/local  "), PathBuf::from("/usr/local"));
    }

    #[test]
    fn the_launch_agent_plist_names_the_binary_and_is_well_formed() {
        let contents = launch_agent::plist_contents(Path::new("/tmp/neko"));
        assert!(contents.contains("<string>com.neko.launcher</string>"));
        assert!(contents.contains("<string>/tmp/neko</string>"));
        assert!(contents.contains("<key>RunAtLoad</key>"));
    }

    #[test]
    fn an_ampersand_in_the_binary_path_is_escaped_rather_than_breaking_the_plist() {
        let contents = launch_agent::plist_contents(Path::new("/tmp/a&b/neko"));
        assert!(contents.contains("/tmp/a&amp;b/neko"), "got: {contents}");
    }

    #[test]
    fn every_preference_row_is_searchable_by_a_word_a_person_would_actually_type() {
        let provider = PreferencesProvider::new(Arc::new(Mutex::new(db())));
        for (query, expected) in
            [("shortcut", ROW_HOTKEY), ("startup", ROW_LAUNCH_AT_LOGIN), ("folders", ROW_SEARCH_FOLDERS)]
        {
            let found = provider.search(query, 0);
            let top = found
                .iter()
                .max_by(|a, b| a.score.partial_cmp(&b.score).unwrap())
                .unwrap_or_else(|| panic!("{query} matched nothing"));
            assert_eq!(top.item.id, expected, "query {query:?} should find {expected}");
        }
    }

    #[test]
    fn an_empty_query_still_lists_every_setting_because_the_window_loads_that_way() {
        let provider = PreferencesProvider::new(Arc::new(Mutex::new(db())));
        assert_eq!(provider.search("", 0).len(), ROWS.len());
        // Keeping them out of the *root* list is a separate question, asked
        // of the provider rather than of the query.
        assert!(!provider.answers_empty_root_query());
    }

    #[test]
    fn the_two_screen_rows_carry_a_mode_and_the_toggle_row_does_not() {
        let provider = PreferencesProvider::new(Arc::new(Mutex::new(db())));
        let rows = provider.search("", 0);
        let by_id = |id: &str| rows.iter().find(|c| c.item.id == id).unwrap().item.clone();
        assert_eq!(by_id(ROW_HOTKEY).enters_mode.as_deref(), Some(MODE_HOTKEY));
        assert_eq!(by_id(ROW_SEARCH_FOLDERS).enters_mode.as_deref(), Some(MODE_FOLDERS));
        assert_eq!(by_id(ROW_LAUNCH_AT_LOGIN).enters_mode, None);
    }

    #[test]
    fn activating_a_row_that_owns_a_screen_errors_rather_than_silently_doing_nothing() {
        let provider = PreferencesProvider::new(Arc::new(Mutex::new(db())));
        assert!(provider.activate(ROW_HOTKEY).is_err());
        assert!(provider.activate(ROW_SEARCH_FOLDERS).is_err());
        assert!(provider.activate("no-such-row").is_err());
    }

    #[test]
    fn the_folder_scope_list_shows_every_configured_folder_regardless_of_the_typed_query() {
        let db = Arc::new(Mutex::new(db()));
        set_search_folders(&db.lock().unwrap(), &[PathBuf::from("/usr"), PathBuf::from("/tmp")]).unwrap();
        let provider = FolderScopeProvider::new(db);
        // The query in this mode is a path being typed to *add*, so it must
        // not filter the existing list away underneath it.
        assert_eq!(provider.search("", 0).len(), 2);
        assert_eq!(provider.search("/Users/someone/Projects", 0).len(), 2);
    }
}
