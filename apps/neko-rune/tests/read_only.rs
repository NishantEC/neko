//! The daemon is the only writer. This client may not write to it or to its database, and the
//! way it is kept that way is checked here, by reading the source, so a later screen cannot
//! quietly add a write.
//!
//! P0 is read-only. The first writes (P1: send a message, approve a task) will come through a
//! reviewed `link` addition and this test will say so out loud.

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

#[test]
fn only_the_link_module_names_daemon_requests() {
    for (path, text) in crate_sources() {
        let in_link = path.components().any(|c| c.as_os_str() == "link");
        if in_link {
            continue;
        }
        let code = production(&text);
        for forbidden in ["Request::", "Command::", "write_frame", "Frame::Request"] {
            assert_eq!(bare(code, forbidden), 0, "{} names `{forbidden}`: daemon requests belong in src/link/", path.display());
        }
    }
}

#[test]
fn the_link_sends_only_the_two_read_requests() {
    let (_, link) = crate_sources().into_iter().find(|(p, _)| p.ends_with("link/mod.rs")).unwrap();
    let code = production(&link);
    let requests: Vec<&str> = code.lines().filter(|l| bare(l, "Request::") + bare(l, "Command::") > 0).collect();
    assert_eq!(requests.len(), 2, "exactly the two read requests: {requests:?}");
    assert!(requests.iter().any(|l| l.contains("Command::Snapshot")));
    assert!(requests.iter().any(|l| l.contains("WorkbenchIfChanged")));
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
fn nothing_in_the_client_writes_files() {
    // `web-token` is read, never written; the client has no files of its own in P0.
    for (path, text) in crate_sources() {
        let code = production(&text);
        for forbidden in ["fs::write", "File::create", "OpenOptions", "fs::remove", "fs::rename"] {
            assert!(!code.contains(forbidden), "{} uses `{forbidden}`", path.display());
        }
    }
}
