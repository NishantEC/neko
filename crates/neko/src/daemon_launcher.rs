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
    let mut command = Command::new(&daemon_path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit());
    if let Err(e) = launch_and_reap(command) {
        eprintln!("neko: failed to spawn {}: {e}", daemon_path.display());
    }
}

/// Spawn inside the waiter so failure to create a thread cannot orphan a
/// previously spawned child. Waiting never blocks GPUI's foreground executor.
fn launch_and_reap(
    mut command: Command,
) -> std::io::Result<std::thread::JoinHandle<std::io::Result<std::process::ExitStatus>>> {
    std::thread::Builder::new()
        .name("neko-daemon-reaper".into())
        .spawn(move || {
            let result = command.spawn().and_then(|mut child| child.wait());
            if let Err(error) = &result {
                eprintln!("neko: daemon launch/wait failed: {error}");
            }
            result
        })
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_waiter_reaps_short_lived_child_and_preserves_status() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 7"]);
        let status = launch_and_reap(command).unwrap().join().unwrap().unwrap();
        assert_eq!(status.code(), Some(7));
    }
}
