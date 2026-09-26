//! One client, and the newest one wins.
//!
//! **This exists because two clients is a silent, expensive failure.** Launch
//! at login starts a client; rebuilding and relaunching during development
//! starts another; both open a panel, both install a menu bar item, and both
//! attempt the summon hotkey — of which exactly one registration receives the
//! keypress. Which one is not observable from the outside, so a captain
//! pressing ⌥Space can be looking at a build from yesterday while today's
//! sits behind it, and a fix that was measured and shipped simply does not
//! appear. That happened twice in one session, and cost a round trip each
//! time; `AGENTS.md` had recorded it as something to *remember*, which is the
//! kind of mitigation that fails the third time too.
//!
//! ## The newest wins, deliberately
//!
//! The daemon's own [`bind_singleton`](../../neko_daemon/server/fn.bind_singleton.html)
//! makes a *redundant* start a quiet no-op — right for a resident process
//! nobody launches on purpose. The client is the opposite: it is launched on
//! purpose, and a person launching one is asking for the one they just
//! launched. So a second client tells the first to quit and takes over. For a
//! double-click that is indistinguishable from "activate the existing one";
//! for a rebuild it is the whole point.
//!
//! ## Evidence runs never participate
//!
//! An isolated evidence client taking over the captain's real one would be a
//! far worse bug than the one this fixes — see `evidence.rs`'s own standing
//! safety rule. [`take_over`] returns immediately for any evidence run, so a
//! throwaway client can neither be quit by, nor quit, anything.

use std::io::Read as _;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set when another client has asked this one to stand down.
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Whether a newer client has taken over. Cleared by the read, like every
/// other flag `main.rs`'s 20ms poll consumes.
pub fn take_quit_request() -> bool {
    QUIT_REQUESTED.swap(false, Ordering::Relaxed)
}

/// Where the running client advertises itself. Beside the daemon's own socket
/// and the database, because they are all the same per-user install — and
/// because an isolated `HOME` therefore gets its own, which is what keeps
/// evidence runs and the real client from ever meeting.
pub fn client_socket_path() -> PathBuf {
    neko_protocol::support_dir().join("client.sock")
}

/// Becomes the one client: asks any existing one to quit, then listens.
///
/// Returns whether this process now holds the socket. `false` means something
/// is wrong with the path itself (an unwritable directory, a socket held by
/// something that will not yield) — in which case the client still runs, just
/// without the guarantee. Refusing to start would be a worse failure than the
/// one being prevented.
pub fn take_over() -> bool {
    if crate::evidence::evidence_run_active() {
        return false;
    }
    let path = client_socket_path();
    if let Some(parent) = path.parent()
        && std::fs::create_dir_all(parent).is_err()
    {
        return false;
    }
    // A live client answers; a stale file does not. Either way the file has
    // to go before this process can bind, and the connect attempt is what
    // tells the two apart — the same distinction `bind_singleton` draws.
    if path.exists() {
        if UnixStream::connect(&path).is_ok() {
            // The connection *is* the message: the listener below quits on
            // any accept, so there is nothing to write and no reply to wait
            // for. Dropping the stream closes it immediately.
            wait_for_release(&path);
        }
        let _ = std::fs::remove_file(&path);
    }
    let Ok(listener) = UnixListener::bind(&path) else {
        return false;
    };
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            // Any connection at all means a newer client is starting. Read to
            // EOF first so the caller's own `wait_for_release` sees this
            // process acknowledge before it proceeds.
            if let Ok(mut stream) = stream {
                let mut sink = [0u8; 1];
                let _ = stream.read(&mut sink);
            }
            QUIT_REQUESTED.store(true, Ordering::Relaxed);
        }
    });
    true
}

/// How long to wait for the previous client to let go of the socket.
///
/// Bounded rather than open-ended: a predecessor wedged badly enough to never
/// exit must not stop its replacement from starting. Overshooting the wait
/// only costs startup latency in a case that should not happen; giving up
/// early costs the guarantee, so this is generous.
const RELEASE_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);
const RELEASE_POLL: std::time::Duration = std::time::Duration::from_millis(25);

fn wait_for_release(path: &std::path::Path) {
    let deadline = std::time::Instant::now() + RELEASE_TIMEOUT;
    while std::time::Instant::now() < deadline {
        // Gone, or no longer answering: the predecessor is on its way out.
        if !path.exists() || UnixStream::connect(path).is_err() {
            return;
        }
        std::thread::sleep(RELEASE_POLL);
    }
}

/// Removes this client's socket on the way out, so the next start sees a
/// clean path rather than having to prove a stale file dead.
pub fn release() {
    let _ = std::fs::remove_file(client_socket_path());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_client_socket_sits_beside_the_daemons_own() {
        // Per-`HOME`, which is exactly what keeps an isolated evidence client
        // from ever being able to quit the captain's real one.
        let path = client_socket_path();
        assert!(path.to_string_lossy().contains("Application Support/neko"));
        assert!(path.ends_with("client.sock"));
        assert_ne!(
            path,
            neko_protocol::socket_path(),
            "not the daemon's socket"
        );
    }

    #[test]
    fn a_quit_request_is_consumed_exactly_once() {
        // Read by the same 20ms poll every other flag rides; one that stayed
        // set would re-quit forever.
        assert!(!take_quit_request());
        QUIT_REQUESTED.store(true, Ordering::Relaxed);
        assert!(take_quit_request());
        assert!(!take_quit_request());
    }

    #[test]
    fn waiting_on_a_path_nothing_holds_returns_at_once() {
        let path = std::env::temp_dir().join("neko-singleton-absent.sock");
        let _ = std::fs::remove_file(&path);
        let started = std::time::Instant::now();
        wait_for_release(&path);
        assert!(
            started.elapsed() < RELEASE_TIMEOUT,
            "did not sit out the timeout"
        );
    }
}
