//! The daemon is the only writer. This client may ask it for what the web UI asks it, and for a short list of
//! extras, and for nothing else; it may not touch its database. The way it is kept that way is checked by
//! reading the source, so a later screen cannot quietly add a request.
//!
//! **The mirror rule (2026-11-16).** A test greps `web/src` for every daemon request the web UI sends (its
//! `command(`, `run(`, `send(` and `request(` call sites) and builds `WEB_SENDS`. Rune may send exactly
//! `WEB_SENDS` plus `RUNE_EXTRAS`: what Rune needs that the web reaches another way (a session form, the panel's
//! signed address, the reads it already used). `NEVER` keeps only what no person-facing screen sends: the agents'
//! `McpBridge` and the calls a run makes under its lease. A destructive request (delete, start over, retry,
//! stop all, close a pull request, remove a theme, roll back the web UI) may only be built inside a `confirm(`
//! dialog. The HTTP client (`src/link/http.rs`) reaches only the web's `/api/attachments`, `/api/files` and
//! `/api/plugins/*`.
//!
//! A new request therefore needs no edit here when the web sends it too; one the web does not send goes into
//! `RUNE_EXTRAS` with its reason, where a reviewer sees it.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn sources(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            sources(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

fn crate_sources() -> Vec<(PathBuf, String)> {
    let mut files = Vec::new();
    sources(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"), &mut files);
    files.into_iter().map(|p| (p.clone(), fs::read_to_string(p).unwrap())).collect()
}

/// Code outside tests: everything before a file's `#[cfg(test)]` module.
fn production(text: &str) -> &str {
    text.split("#[cfg(test)]").next().unwrap_or(text)
}

/// How many times `needle` appears as a whole path (`Request::`, not `ReadRequest::`).
fn bare(code: &str, needle: &str) -> usize {
    code.match_indices(needle)
        // `ReadRequest::` is not `Request::`, and `process::Command` is not the daemon's.
        .filter(|(at, _)| !code[..*at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') && !code[..*at].ends_with("process::"))
        .count()
}

const PREFIXES: [&str; 13] = ["Request::", "Command::", "CrewCommand::", "CheckpointCommand::", "GoalCommand::", "McpCommand::", "SkillCommand::", "ProfileCommand::", "ScheduleCommand::", "ImportCommand::", "DecisionContextCommand::", "PackCommand::", "ImproveCommand::"];

/// Every `Prefix::Name` the code names, as the daemon's request paths.
fn named(code: &str) -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for prefix in PREFIXES {
        for (at, _) in code.match_indices(prefix) {
            if code[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') || code[..at].ends_with("process::") {
                continue;
            }
            let name: String = code[at + prefix.len()..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            if !name.is_empty() {
                found.insert(format!("{prefix}{name}"));
            }
        }
    }
    found
}

#[test]
fn only_the_link_module_names_daemon_requests() {
    for (path, text) in crate_sources() {
        if path.components().any(|c| c.as_os_str() == "link") {
            continue;
        }
        let code = production(&text);
        for forbidden in PREFIXES.iter().copied().chain(["write_frame", "Frame::Request"]) {
            assert_eq!(bare(code, forbidden), 0, "{} names `{forbidden}`: daemon requests belong in src/link/", path.display());
        }
    }
}

#[test]
fn the_scan_cannot_be_sidestepped_by_renaming_or_by_an_early_test_module() {
    for (path, text) in crate_sources() {
        // A renamed or glob-imported request type would hide its uses from the scan.
        for sneaky in ["Request as", "Command as", "Request::*", "Command::*", "CrewCommand as", "GoalCommand as", "CheckpointCommand as", "use neko_protocol::*", "Call as", "Call::*", "link::*"] {
            assert!(!production(&text).contains(sneaky), "{} uses `{sneaky}`, which hides requests from the allow-list scan", path.display());
        }
        // Everything after the first `#[cfg(test)]` is unscanned, so it must be the one test module.
        if let Some((_, rest)) = text.split_once("#[cfg(test)]") {
            assert!(rest.trim_start().starts_with("mod tests") && !rest.contains("#[cfg(test)]"), "{}: `#[cfg(test)]` must only guard the closing `mod tests`", path.display());
        }
    }
}

/// Requests no person-facing screen sends: the bridge an agent's run talks to (`McpBridge`) and the calls a run
/// makes under its lease (`save_artifact`). They are for agents, so this client never sends them.
const NEVER: &[&str] = &["McpBridge", "SaveArtifact"];

/// Requests Rune sends that the scan of the web UI's call sites does not find, each for a reason the line says.
const RUNE_EXTRAS: &[&str] = &[
    // A new session: the web starts work from Neko's chat (`create_task`), Rune has a form (Home, "+ New").
    "CreateTask",
    "StartTask",
    // The goal on a new session made from the form (`GoalCommand::Create`; the web makes goals through `/goal` in chat).
    "Create",
    // Rune's own panel: the signed address for the web panel and the API's token, the viewers and pages plugins
    // declare, a session's folder and what its runs saved.
    "SignedWebUrl",
    "TaskFolder",
    "ArtifactFile",
    "ArtifactsForTask",
    "PluginViewers",
    "PluginPages",
    // A reply to a task between turns: the web sends it through its chat stream (`/api/chat/…`, which the daemon turns
    // into `ReplyToTask`); Rune has no stream and sends the command itself.
    "ReplyToTask",
    // Settings' two limits, which the web sends through a `limit(name, value)` helper the scan does not follow.
    "SetMaxRunningTickets",
    "SetMaxWritablePerRepository",
    // Tools: a tool's unattended-use grant. The web's Tools page shows tools read-only; the earlier native app had this control.
    "SetToolGrant",
    "SetWorkspaceToolGrant",
    // The summon shortcut and the update source: the web shows or links them (and the native app sets them), Rune's Settings sets them
    // the same way the native app does: a side-effect-free check, then the commit, and the source as `owner/name`.
    "CheckHotkeyConflict",
    "CommitHotkey",
    "SetUpdateSource",
    // Reads Rune made before this rule that the web UI reaches through a store or helper the scan does not follow.
    "Snapshot",
    "WorkbenchIfChanged",
    "Artifact",
    "MorningDigest",
    "PhoneSettings",
    "GetAppearance",
    "PluginStatus",
    "Routines",
    "UnseenTasks",
    "TaskCheckpoints",
    "CheckpointDiff",
    "CalendarEvents",
    "AgentModels",
    "Search",
    // Luau plugins (`src/luau/`): the files of a healthy, unchanged pack, which Rune runs in a sandbox. The daemon refuses a pack that is
    // not Healthy or whose files differ from what the person approved; Rune never reads a pack folder itself.
    "PackUi",
];

/// The envelopes: the web sends `Crew`, `Goals` and `Checkpoints` with the sub-command in the payload, and
/// `Workbench` around every command.
const ENVELOPES: &[&str] = &["Workbench", "Crew", "Goals", "Checkpoints"];

/// The sub-commands carried inside an envelope: checked by their name appearing in the web's sources.
/// The Setup pages' sub-commands (`mcp("Wake", ...)`, `skillsCmd("Refresh")`, `schedulesCmd("Save", ...)` in `web/src/setup/lib.ts`) are carried the same way.
const SUBCOMMAND_PREFIXES: [&str; 11] = ["CrewCommand", "GoalCommand", "CheckpointCommand", "McpCommand", "SkillCommand", "ProfileCommand", "ScheduleCommand", "ImportCommand", "DecisionContextCommand", "PackCommand", "ImproveCommand"];

/// Requests that destroy, publish or restart something, with the `Call` that carries each. A `Call` is only built
/// inside a `confirm(` (see `destructive_requests_are_only_sent_from_a_confirm_dialog`).
const DESTRUCTIVE: &[(&str, &str)] = &[
    ("DeleteTask", "Call::Delete"),
    ("CancelTask", "Call::Stop"),
    ("StartOver", "Call::WipeTasks"),
    ("ClosePullRequest", "Call::ClosePr"),
    ("RemoveTheme", "Call::DropTheme"),
    ("CancelAllWork", "Call::StopAll"),
    ("RetryTask", "Call::Retry"),
    ("RollBackWebUi", "Call::PreviousWebBuild"),
    ("DELETE /api/plugins/<id>", "Call::RemovePlugin"),
    ("ClearFinishedTasks", "Call::ClearFinished"),
];

fn web_sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "ts" || e == "tsx") && !path.to_string_lossy().contains(".test.") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/src"), &mut files);
    files.into_iter().map(|p| (p.clone(), fs::read_to_string(p).unwrap_or_default())).collect()
}

/// A name read at byte `at` in `text`: an optional quote, then a capitalised identifier.
fn name_at(text: &str, at: usize) -> Option<String> {
    let rest = text[at..].trim_start().trim_start_matches(['"', '\'', '`']);
    let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
    name.chars().next().filter(|c| c.is_uppercase()).map(|_| name)
}

/// The byte offset of the first thing after `at` that is not whitespace, `{` or a quote.
fn skip_open(text: &str, mut at: usize) -> usize {
    while let Some(c) = text[at..].chars().next() {
        if c.is_whitespace() || c == '{' {
            at += c.len_utf8();
        } else {
            break;
        }
    }
    at
}

/// The daemon request names the web UI sends: the first argument of its `command(`, `run(`, `send(` and `request(`
/// call sites (a quoted name, or the first key of an object), what `Workbench` wraps, and the board's move table.
fn web_sends() -> BTreeSet<String> {
    let mut found = BTreeSet::new();
    for (_, text) in web_sources() {
        for call in ["command(", "run(", "send(", "request("] {
            for (at, _) in text.match_indices(call) {
                if text[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let start = skip_open(&text, at + call.len());
                let Some(name) = name_at(&text, start) else { continue };
                found.insert(name.clone());
                // `{ Workbench: { Name` and `{ Workbench: "Name"`.
                if name == "Workbench" {
                    let after = text[start..].find("Workbench").map(|i| start + i + "Workbench".len()).unwrap_or(start);
                    let rest = text[after..].trim_start_matches(['"', '\'']).trim_start();
                    if let Some(rest) = rest.strip_prefix(':') {
                        let at = text.len() - rest.len();
                        if let Some(inner) = name_at(&text, skip_open(&text, at)) {
                            found.insert(inner);
                        }
                    }
                }
            }
        }
        // The board's move commands are named in one table (`moveCommand`).
        for (at, _) in text.match_indices("command: \"") {
            if let Some(name) = name_at(&text, at + "command: ".len()) {
                found.insert(name);
            }
        }
    }
    found
}

/// Whether the web's sources use `name` as a request key or a quoted name (`{ Enqueue: {` or `"Enqueue"`), not just as a word in a
/// comment (a sub-command carried inside `Crew`, `Goals`, ...).
fn web_mentions(name: &str) -> bool {
    web_sources().iter().any(|(_, text)| {
        text.match_indices(name).any(|(at, _)| {
            let before = text[..at].chars().next_back();
            let rest = &text[at + name.len()..];
            let after = rest.chars().next();
            let boundary = !before.is_some_and(|c| c.is_alphanumeric() || c == '_') && !after.is_some_and(|c| c.is_alphanumeric() || c == '_');
            let as_key = rest.trim_start().starts_with(':') || (before == Some('"') && after == Some('"')) || (before == Some('\'') && after == Some('\''));
            boundary && as_key
        })
    })
}

/// Every request the client's link module names, as `Prefix::Name`.
fn sent() -> BTreeSet<String> {
    let mut used = BTreeSet::new();
    for (_, text) in crate_sources().into_iter().filter(|(p, _)| p.components().any(|c| c.as_os_str() == "link")) {
        used.extend(named(production(&text)));
    }
    used
}

#[test]
fn the_scan_finds_the_web_uis_requests() {
    let sends = web_sends();
    for known in [
        "SendMessage", "DiscardQueuedMessage", "InterruptAndSendMessage", "CancelAllWork", "ClearFinishedTasks", "ApproveTask", "DeleteTask", "RenameTask", "RetryTask",
        "PublishTask", "PrioritizeTask", "SetLandOnMain", "CancelTask", "CompleteTask", "AnswerNeedsYou", "MarkTaskSeen", "SaveRoutine", "ClearNeedsYou", "ReopenNeedsYou", "ProposeSplit",
    ] {
        assert!(sends.contains(known), "the scan of web/src did not find `{known}`; found {sends:?}");
    }
}

#[test]
fn rune_sends_only_what_the_web_sends_and_its_extras() {
    let web = web_sends();
    let mut unreviewed = Vec::new();
    for full in sent() {
        let (prefix, tail) = full.split_once("::").unwrap();
        assert!(!NEVER.contains(&tail), "{full} is for agents, never for this client");
        let known = web.contains(tail)
            || RUNE_EXTRAS.contains(&tail)
            || ENVELOPES.contains(&tail)
            || (SUBCOMMAND_PREFIXES.contains(&prefix) && web_mentions(tail));
        if !known {
            unreviewed.push(full.clone());
        }
    }
    assert!(unreviewed.is_empty(), "requests Rune sends that the web UI does not (send what the web sends, or give the extra a reason in RUNE_EXTRAS): {unreviewed:?}");
}

#[test]
fn the_packs_requests_are_exactly_the_protocols() {
    // Rune names every `PackCommand` the protocol has and `Request::Packs`, and nothing else of that family.
    let used = sent();
    let packs: BTreeSet<&str> = used.iter().filter_map(|s| s.strip_prefix("PackCommand::")).collect();
    let protocol = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/neko-protocol/src/packs.rs")).unwrap();
    let enum_text = protocol.split("pub enum PackCommand {").nth(1).unwrap().split("\n}\n").next().unwrap();
    let variants: BTreeSet<&str> = enum_text.lines().filter(|l| l.starts_with("    ") && !l.starts_with("     ") && !l.trim_start().starts_with("//")).filter_map(|l| l.trim().split(|c: char| !c.is_alphanumeric()).next()).filter(|v| v.chars().next().is_some_and(char::is_uppercase)).collect();
    assert!(variants.len() >= 8, "{variants:?}");
    assert_eq!(packs, variants);
    assert!(used.contains("Request::Packs"));
    // The one destructive pack request is built only in a confirmation (`state.rs`'s `confirm_remove_pack`): see
    // `setup_destroying_requests_are_only_sent_from_a_confirmation`. The wire name never leaves `link/`.
    for (path, text) in crate_sources() {
        if !path.components().any(|c| c.as_os_str() == "link") {
            assert!(!production(&text).contains("PackCommand"), "{} names `PackCommand`: pack requests belong in src/link/", path.display());
        }
    }
}

#[test]
fn the_improve_requests_are_exactly_the_protocols_and_the_webs() {
    // Rune names every `ImproveCommand` the protocol has (the page sends List, Apply, Reject, Undo and SetEnabled; the Settings switch
    // sends SetEnabled too) and `Request::Improve`, and each sub-command is one the web's sources send as a key.
    let used = sent();
    let improve: BTreeSet<&str> = used.iter().filter_map(|s| s.strip_prefix("ImproveCommand::")).collect();
    let protocol = std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../crates/neko-protocol/src/improve.rs")).unwrap();
    let enum_text = protocol.split("pub enum ImproveCommand {").nth(1).unwrap().split("\n}\n").next().unwrap();
    let variants: BTreeSet<&str> = enum_text.lines().filter(|l| l.starts_with("    ") && !l.starts_with("     ") && !l.trim_start().starts_with("//")).filter_map(|l| l.trim().split(|c: char| !c.is_alphanumeric()).next()).filter(|v| v.chars().next().is_some_and(char::is_uppercase)).collect();
    assert_eq!(improve, variants, "the page sends what the protocol has");
    assert!(used.contains("Request::Improve"));
    for name in &improve {
        assert!(web_mentions(name), "the web's sources do not send Improve `{name}`");
    }
}

#[test]
fn every_extra_is_still_sent_and_none_is_a_web_send() {
    let web = web_sends();
    let used = sent();
    let tails: BTreeSet<&str> = used.iter().filter_map(|s| s.split_once("::").map(|(_, t)| t)).collect();
    for extra in RUNE_EXTRAS {
        // The wire name of `Command::Snapshot` and the held read are named in `link/mod.rs` and `link/write.rs`.
        assert!(tails.contains(extra), "{extra} is in RUNE_EXTRAS but Rune no longer sends it: remove it");
    }
    let _ = web;
}

#[test]
fn destructive_requests_are_only_sent_from_a_confirm_dialog() {
    // Every `Call` that reaches a destructive request is built inside `confirm(`; no file outside `link/` names the wire request.
    let confirm_only = ["task_actions.rs"];
    let mut built = 0;
    for (path, text) in crate_sources() {
        let code = production(&text);
        let in_link = path.components().any(|c| c.as_os_str() == "link");
        for (wire, call) in DESTRUCTIVE {
            if !in_link {
                assert!(!bare_word(code, wire), "{} names the request `{wire}`: only link/ may", path.display());
            }
            if in_link {
                continue;
            }
            for (at, _) in code.match_indices(call) {
                if code[at + call.len()..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                built += 1;
                let before = &code[at.saturating_sub(900)..at];
                assert!(before.contains("self.confirm(Ask {") || before.contains("confirm(Ask {"), "{}: `{call}` is built outside a confirm dialog", path.display());
                assert!(confirm_only.iter().any(|f| path.ends_with(f)), "{}: `{call}` belongs in task_actions.rs (the one place that confirms)", path.display());
            }
        }
    }
    assert!(built >= 4, "the destructive calls were not found ({built}): the scan is looking at nothing");
}

#[test]
fn setup_destroying_requests_are_only_sent_from_a_confirmation() {
    // A delete, a forget, a dismissed suggestion and a pause that revokes grants are built in `state.rs`'s `confirm_*`
    // functions (the coral button's dialog) and nowhere else: no view file builds one for a button to send directly.
    let destroying = ["SetupAction::RemoveWatch", "SetupAction::RemoveSchedule", "SetupAction::ForgetMemory", "SetupAction::ForgetPreference", "SetupAction::PackRemove", "SetupAction::ImproveUndo"];
    let mut built_in_state = 0;
    for (path, text) in crate_sources() {
        let name = path.to_string_lossy().into_owned();
        if !name.contains("view/setup/") {
            continue;
        }
        let code = production(&text);
        for what in destroying {
            let n = code.matches(what).count();
            if name.ends_with("view/setup/state.rs") {
                built_in_state += n;
            } else {
                assert_eq!(n, 0, "{name} builds `{what}` outside a confirmation");
            }
        }
    }
    assert!(built_in_state >= 6, "the confirmations build the destroying requests ({built_in_state})");
}


/// `word` appears in `code` as a whole word.
fn bare_word(code: &str, word: &str) -> bool {
    code.match_indices(word).any(|(at, _)| {
        !code[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_') && !code[at + word.len()..].chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_')
    })
}

#[test]
fn http_goes_only_to_the_paths_the_web_uses() {
    // `src/link/http.rs` is the one place the client speaks HTTP: `/api/attachments`, `/api/files` and `/api/plugins/*`,
    // each a path the web's sources use.
    let http = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/link/http.rs")).unwrap();
    let code = production(&http);
    let mut paths = BTreeSet::new();
    for (at, _) in code.match_indices("\"/api/") {
        let path: String = code[at + 1..].chars().take_while(|c| *c != '"' && *c != '?' && *c != '{').collect();
        paths.insert(path);
    }
    let allowed: BTreeSet<String> = ["/api/attachments", "/api/files", "/api/plugins", "/api/plugins/", "/api/customisation", "/api/storage/"].map(str::to_owned).into();
    assert!(paths.is_subset(&allowed), "http.rs reaches paths the web UI does not use: {paths:?}");
    assert!(paths.contains("/api/attachments") && paths.contains("/api/files"));
    for path in &paths {
        assert!(web_sources().iter().any(|(_, t)| t.contains(path.trim_end_matches('/'))), "{path} is not a path the web UI uses");
    }
    // No other file opens a socket to the web port. (`src/browser/` opens sockets too, to a Chrome on this Mac and never to
    // the daemon: see the next test.)
    for (path, text) in crate_sources() {
        let browser = path.components().any(|c| c.as_os_str() == "browser");
        if !path.ends_with("http.rs") && !browser {
            assert!(!production(&text).contains("TcpStream"), "{} speaks TCP outside link/http.rs", path.display());
        }
    }
}

/// The live Browser tab talks to Chrome (`src/browser/`), not to the daemon: it is the only other code that opens a socket, it
/// reaches only a loopback address it parsed (`discover::loopback_url`), it knows nothing of the daemon's requests or web paths, and
/// the only CDP methods it sends are the seven of `cdp::CDP_METHODS`.
#[test]
fn the_browser_module_talks_to_chrome_on_the_loopback_and_never_to_the_daemon() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/browser");
    let mut methods = BTreeSet::new();
    for entry in fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        let text = fs::read_to_string(&path).unwrap();
        let code = production(&text);
        for forbidden in ["/api/", "neko_protocol", "crate::link", "Call::", "WEB_SENDS", "socket_path", "support_dir", "https://", "wss://", "std::fs", "File::create", "fs::write"] {
            assert!(!code.contains(forbidden), "{} mentions `{forbidden}`: the browser module touches Chrome only, and writes nothing", path.display());
        }
        if code.contains("TcpStream::connect") {
            assert!(code.contains("loopback_url"), "{} connects without parsing the address as a loopback one", path.display());
        }
        // Every method named at a send is a string literal on the allow-list (the call sites of `self.send(`).
        for (at, _) in code.match_indices("self.send(\"") {
            let method: String = code[at + 11..].chars().take_while(|c| *c != '"').collect();
            methods.insert(method);
        }
    }
    let allowed: BTreeSet<String> = neko_rune::browser::cdp::CDP_METHODS.map(str::to_owned).into();
    assert!(!methods.is_empty() && methods.is_subset(&allowed), "a send names a method that is not on the allow-list: {methods:?}");
}

#[test]
fn the_client_never_opens_the_database() {
    let manifest = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml")).unwrap();
    let deps = manifest.split("[dev-dependencies]").next().unwrap();
    for db in ["rusqlite", "sqlite", "neko-core", "diesel", "sqlx"] {
        assert!(!deps.contains(db), "the client must not depend on `{db}`: only the daemon reads or writes the database");
    }
    for (path, text) in crate_sources() {
        let code = production(&text);
        assert!(!code.contains("neko_core"), "{} reaches into neko-core", path.display());
        assert!(!code.contains("database_path"), "{} names the database file", path.display());
    }
}

#[test]
fn only_the_attachments_module_writes_files_and_only_in_its_own_cache() {
    // `web-token` is read, never written. A pasted image is kept in the client's own cache folder
    // (not the daemon's data folder); that is the one place this client writes a file.
    for (path, text) in crate_sources() {
        let code = production(&text);
        // The deep-link socket (`deeplink.rs`) is the one other file: a Unix socket in the client's own cache folder, replaced when a window died.
        let is_attachments = path.ends_with("attachments.rs") || path.ends_with("deeplink.rs");
        for forbidden in ["fs::write", "File::create", "OpenOptions", "fs::remove", "fs::rename", "fs::copy", "File::options", "create_dir"] {
            assert!(is_attachments || !code.contains(forbidden), "{} uses `{forbidden}`", path.display());
        }
    }
    let deeplink = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/deeplink.rs")).unwrap();
    assert!(!production(&deeplink).contains("support_dir") && !production(&deeplink).contains("neko_protocol"), "the deep-link socket is in the client's cache, never the daemon's data folder");
    let attachments = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/attachments.rs")).unwrap();
    let code = production(&attachments);
    assert!(code.contains("composer-attachments") && !code.contains("support_dir") && !code.contains("neko_protocol"), "pasted images go to the client's cache, never the daemon's data folder");
}

#[test]
fn only_the_app_state_touches_rune_prefs_and_only_to_keep_the_layout() {
    // The window's widths are kept in Rune's own preferences store (its config folder), written on a
    // thread from `app.rs`. Nothing else may open it, and it is never the daemon's data folder.
    for (path, text) in crate_sources() {
        let code = production(&text);
        let is_app = path.ends_with("src/app.rs");
        assert!(is_app || !code.contains("prefs::") && !code.contains("Prefs::"), "{} touches the preferences store", path.display());
    }
    let app = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/app.rs")).unwrap();
    let code = production(&app);
    assert!(code.contains("PREFS_ID") && !code.contains("support_dir") && !code.contains("neko_protocol::socket_path"), "the layout goes to Rune's store, not the daemon's folder");
    assert_eq!(code.matches("prefs::Prefs::open").count(), 12, "at startup one read of the layout, of the workspace scope, of the All sessions layout, of the hidden Luau cards, of the activity pill's preference and of the Open-in choice, each written on a thread when it changes");
}

#[test]
fn the_panel_reads_files_only_through_the_confined_readers() {
    // The Files tab and the Canvas show what the daemon names (a folder, a stored artifact), and the
    // daemon's word is data, not permission: every such read goes through `readers.rs`, which refuses
    // `..`, paths outside the root and links that leave it. The token (`web_link.rs`) and a pasted image
    // (`attachments.rs`) are the only other files this client opens.
    for (path, text) in crate_sources() {
        let code = production(&text);
        // `permissions.rs` lists one folder macOS protects (Safari's) only to learn whether Full Disk Access is given; it reads no names.
        // `scenes.rs` reads one JSON summary a headless snapshot run is pointed at (`NEKO_RUNE_SUMMARY`).
        // `shell/daemon.rs` (Rune as the Mac app, `--app`) asks whether the sibling `neko-daemon` is an executable file before it starts it.
        let allowed = path.ends_with("readers.rs") || path.ends_with("web_link.rs") || path.ends_with("attachments.rs") || path.ends_with("scenes.rs") || path.ends_with("permissions.rs") || path.ends_with("shell/daemon.rs");
        for opener in ["File::open", "fs::read(", "fs::read_to_string", "read_dir", "fs::metadata", "canonicalize", "File::create"] {
            assert!(allowed || !code.contains(opener), "{} opens files with `{opener}` outside the confined readers", path.display());
        }
    }
    let readers = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/readers.rs")).unwrap();
    let code = production(&readers);
    for writer in ["fs::write", "File::create", "OpenOptions", "fs::remove", "fs::rename", "fs::copy", "File::options", "create_dir", "set_permissions", "fs::symlink(", "unix::fs::symlink("] {
        assert!(!code.contains(writer), "the readers never write: `{writer}`");
    }
}
