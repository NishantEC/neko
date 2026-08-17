//! Brings up `neko-daemon` if it isn't already resident. The daemon's own
//! singleton check (`neko-daemon::server::bind_singleton`) makes this safe
//! to call unconditionally on every client launch: a redundant spawn just
//! exits immediately without disturbing the live instance.

use std::process::{Command, Stdio};

pub fn ensure_daemon_running() {
    let Some(daemon_path) = daemon_binary_path() else {
        eprintln!("neko: could not locate the neko-daemon binary next to neko");
        return;
    };
    let result = Command::new(&daemon_path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn();
    if let Err(e) = result {
        eprintln!("neko: failed to spawn {}: {e}", daemon_path.display());
    }
}

fn daemon_binary_path() -> Option<std::path::PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let sibling = current_exe.parent()?.join("neko-daemon");
    if sibling.exists() {
        Some(sibling)
    } else {
        // `cargo run` (dev loop) may not place both binaries in the same
        // directory in every workspace layout; fall back to PATH.
        Some(std::path::PathBuf::from("neko-daemon"))
    }
}
