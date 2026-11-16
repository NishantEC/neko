//! The daemon is the only writer. This client may ask it for the requests named here and for no
//! other, and may not touch its database. The way it is kept that way is checked by reading the
//! source, so a later screen cannot quietly add a write: adding a request means changing this
//! list in the same commit, where a reviewer sees it.
//!
//! P0 sent two reads. P1 adds the reads a session view needs (the open task's runs and one run's
//! diff, the unseen marks, the Inbox, the digest, the model list) and the writes the web UI's own
//! buttons make, with the same fields (docs/decisions/0002-rune-for-everything.md).

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Requests that only fetch.
const READS: &[&str] = &[
    "Command::Snapshot",
    "Request::AgentModels",
    "Request::CheckpointDiff",
    "Request::MorningDigest",
    "Request::Routines",
    "Request::TaskCheckpoints",
    "Request::UnseenTasks",
    "Request::WorkbenchIfChanged",
];

/// Requests that change something, each one a button the web UI has. Wrappers (`Request::Workbench`
/// carries a `Command`; `Command::Crew` carries a `CrewCommand`) are listed too.
const WRITES: &[&str] = &[
    // The envelopes.
    "Request::Workbench",
    "Command::Crew",
    "Command::Checkpoints",
    "Command::Goals",
    // A message to a running agent, and taking it back.
    "CrewCommand::Enqueue",
    "CrewCommand::CancelQueued",
    // A task between turns, and what the person decides about it.
    "Command::ReplyToTask",
    "Command::ApproveTask",
    "Command::CompleteTask",
    "Command::CancelTask",
    "Command::StartTask",
    // A goal.
    "GoalCommand::Pause",
    "GoalCommand::Resume",
    "GoalCommand::Extend",
    "GoalCommand::MarkDone",
    "GoalCommand::Stop",
    "GoalCommand::Answer",
    "GoalCommand::Create",
    // Runs.
    "CheckpointCommand::Undo",
    "CheckpointCommand::Redo",
    // The Inbox and the digest.
    "Request::AnswerNeedsYou",
    "Request::MarkDigestRead",
    // Seen and unread.
    "Request::MarkTaskSeen",
    // A new session, and its runtime.
    "Command::CreateTask",
    "Command::SetConversationRuntime",
    // Neko's own conversation.
    "Command::SendMessage",
    "Command::CancelChat",
    "Command::DecideChatTool",
    "Command::DecideChatAction",
];

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

const PREFIXES: [&str; 5] = ["Request::", "Command::", "CrewCommand::", "CheckpointCommand::", "GoalCommand::"];

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
        for sneaky in ["Request as", "Command as", "Request::*", "Command::*", "CrewCommand as", "GoalCommand as", "CheckpointCommand as", "use neko_protocol::*"] {
            assert!(!production(&text).contains(sneaky), "{} uses `{sneaky}`, which hides requests from the allow-list scan", path.display());
        }
        // Everything after the first `#[cfg(test)]` is unscanned, so it must be the one test module.
        if let Some((_, rest)) = text.split_once("#[cfg(test)]") {
            assert!(rest.trim_start().starts_with("mod tests") && !rest.contains("#[cfg(test)]"), "{}: `#[cfg(test)]` must only guard the closing `mod tests`", path.display());
        }
    }
}

#[test]
fn the_link_sends_exactly_the_allow_listed_requests() {
    let mut used = BTreeSet::new();
    for (path, text) in crate_sources().into_iter().filter(|(p, _)| p.components().any(|c| c.as_os_str() == "link")) {
        let _ = path;
        used.extend(named(production(&text)));
    }
    let allowed: BTreeSet<String> = READS.iter().chain(WRITES).map(|s| (*s).to_owned()).collect();
    let extra: Vec<_> = used.difference(&allowed).collect();
    let missing: Vec<_> = allowed.difference(&used).collect();
    assert!(extra.is_empty(), "requests the client sends that nobody reviewed (add them to this list on purpose): {extra:?}");
    assert!(missing.is_empty(), "allow-listed requests the client no longer sends (remove them from this list): {missing:?}");
}

#[test]
fn the_reads_do_not_write_and_the_list_has_no_destructive_commands() {
    // Nothing that deletes or rewrites history, settings or other people's data is on the list.
    for name in READS.iter().chain(WRITES) {
        for forbidden in ["Delete", "ClearFinished", "CancelAll", "StartOver", "Save", "Set", "Update", "RollBack", "Merge", "Close", "Publish", "Skills", "Mcp", "Connect", "Clear"] {
            let tail = name.rsplit("::").next().unwrap_or(name);
            // The one `Set` the web's composer also sends: pinning a runtime on a task's conversation.
            if *name == "Command::SetConversationRuntime" {
                continue;
            }
            assert!(!tail.starts_with(forbidden), "{name} looks like a request this client must not send");
        }
    }
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
        let is_attachments = path.ends_with("attachments.rs");
        for forbidden in ["fs::write", "File::create", "OpenOptions", "fs::remove", "fs::rename", "fs::copy", "File::options", "create_dir"] {
            assert!(is_attachments || !code.contains(forbidden), "{} uses `{forbidden}`", path.display());
        }
    }
    let attachments = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/attachments.rs")).unwrap();
    let code = production(&attachments);
    assert!(code.contains("composer-attachments") && !code.contains("support_dir") && !code.contains("neko_protocol"), "pasted images go to the client's cache, never the daemon's data folder");
}
