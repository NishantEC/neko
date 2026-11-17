//! Deep links: `neko-rune --open <route>` hands the route to the Rune window that is already running and exits (the web's routes:
//! `tasks?task=<id>`, `setup/tools`, `pull-requests`, `settings`, an artifact, a plugin page). With no window running the route opens
//! in the one this launch starts. The route is carried over a Unix socket in the person's own cache folder (mode 0600, the peer must be
//! the same user): Rune's `single_instance` and `on_open_url` forward only on Windows and Linux, and macOS hands links to a bundled app
//! through its URL scheme, which waits for the bundle (phase P5), so `neko://` is not registered yet. "Copy link" still gives the web URL.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// What `forward` says when nothing is listening: the only answer that lets a launch open a window of its own.
pub const NO_WINDOW: &str = "No Neko window is running.";

/// The longest route taken (the daemon's own limit for `OpenTab` is the same order).
pub const MAX_ROUTE: usize = 300;

/// The route named by `--open <route>` or `--open=<route>`, as typed.
pub fn route_arg(args: &[String]) -> Option<String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--open" {
            return it.next().cloned();
        }
        if let Some(v) = a.strip_prefix("--open=") {
            return Some(v.to_owned());
        }
    }
    None
}

/// A route this client will open: a leading `#/` or `/` dropped; letters, digits and `- _ : / ? = & . % ~`; no `..`, no scheme, no spaces.
/// The same set the daemon's `OpenTab` takes, so a route that reaches a window is one the web UI would open too.
pub fn clean_route(route: &str) -> Option<String> {
    let route = route.trim().trim_start_matches("#/").trim_start_matches('/');
    if route.is_empty() || route.len() > MAX_ROUTE || route.contains("..") || route.contains("//") {
        return None;
    }
    if !route.chars().all(|c| c.is_ascii_alphanumeric() || "-_:/?=&.%~".contains(c)) {
        return None;
    }
    // A colon is only a plugin page's `<plugin>:<page>` (letters, digits, - and _); anything else with one would read as a scheme.
    let path = route.split('?').next().unwrap_or(route);
    if let Some((plugin, page)) = path.split_once(':') {
        let id = |s: &str| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
        if !id(plugin) || !id(page) {
            return None;
        }
    }
    Some(route.to_owned())
}

/// Where the running window listens: `NEKO_RUNE_OPEN_SOCKET`, else `~/Library/Caches/neko/rune-open.sock`.
pub fn socket_path() -> PathBuf {
    std::env::var_os("NEKO_RUNE_OPEN_SOCKET").map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
        home.join("Library/Caches/neko/rune-open.sock")
    })
}

/// Hands `route` to the running window. Ok when it answered "ok"; Err says why not (nothing is listening, or it refused).
pub fn forward(route: &str, socket: &Path) -> Result<(), String> {
    let route = clean_route(route).ok_or("That is not a route in Neko.")?;
    let mut stream = UnixStream::connect(socket).map_err(|_| NO_WINDOW.to_owned())?;
    let _ = stream.set_read_timeout(Some(Duration::from_secs(3)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(3)));
    stream.write_all(format!("{route}\n").as_bytes()).map_err(|_| "The running window did not take it.".to_owned())?;
    let mut answer = String::new();
    BufReader::new(stream).read_line(&mut answer).map_err(|_| "The running window did not answer.".to_owned())?;
    if answer.trim() == "ok" { Ok(()) } else { Err(answer.trim().to_owned()) }
}

/// The user id of the process on the other end of `stream`.
fn peer_uid(stream: &UnixStream) -> Option<u32> {
    use std::os::fd::AsRawFd;
    let (mut uid, mut gid) = (0 as libc::uid_t, 0 as libc::gid_t);
    // SAFETY: `getpeereid` writes two integers through valid pointers for the open descriptor of `stream`.
    let rc = unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) };
    (rc == 0).then_some(uid)
}

fn serve_one(stream: UnixStream, on_route: &(dyn Fn(String) + Send + Sync)) {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
    // SAFETY: `geteuid` has no arguments and cannot fail.
    let me = unsafe { libc::geteuid() };
    let mut writer = match stream.try_clone() {
        Ok(w) => w,
        Err(_) => return,
    };
    if peer_uid(&stream) != Some(me) {
        let _ = writer.write_all(b"refused\n");
        return;
    }
    let mut line = String::new();
    // At most a route and a newline: a longer line is refused, never buffered.
    let mut reader = BufReader::new(stream).take_line(MAX_ROUTE + 8);
    if reader.read_line(&mut line).is_err() {
        return;
    }
    match clean_route(&line) {
        Some(route) => {
            on_route(route);
            let _ = writer.write_all(b"ok\n");
        }
        None => {
            let _ = writer.write_all(b"refused\n");
        }
    }
}

trait TakeLine: Sized {
    fn take_line(self, max: usize) -> std::io::Take<Self>;
}

impl<R: std::io::Read> TakeLine for BufReader<R> {
    fn take_line(self, max: usize) -> std::io::Take<Self> {
        std::io::Read::take(self, max as u64)
    }
}

/// Starts listening for routes. A socket left by a window that died is replaced; one a live window holds is left alone (Err).
/// `on_route` runs on the listener's thread for each accepted route.
pub fn listen(socket: &Path, on_route: impl Fn(String) + Send + Sync + 'static) -> Result<(), String> {
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    if socket.exists() {
        if UnixStream::connect(socket).is_ok() {
            return Err("Another Neko window already listens.".to_owned());
        }
        let _ = std::fs::remove_file(socket);
    }
    // Bound under a umask that leaves the socket reachable by the person alone from its first moment, then set explicitly.
    // SAFETY: `umask` only changes this process's file-creation mask and cannot fail.
    let old = unsafe { libc::umask(0o177) };
    let bound = UnixListener::bind(socket);
    // SAFETY: as above; the previous mask is put back.
    unsafe { libc::umask(old) };
    let listener = bound.map_err(|e| e.to_string())?;
    std::fs::set_permissions(socket, std::fs::Permissions::from_mode(0o600)).map_err(|e| e.to_string())?;
    let on_route: std::sync::Arc<dyn Fn(String) + Send + Sync> = std::sync::Arc::new(on_route);
    std::thread::Builder::new()
        .name("neko-open".into())
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                serve_one(stream, &*on_route);
            }
        })
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_route_comes_from_open_in_either_spelling() {
        assert_eq!(route_arg(&args(&["--open", "tasks?task=t1"])).as_deref(), Some("tasks?task=t1"));
        assert_eq!(route_arg(&args(&["nolink", "--open=setup/tools"])).as_deref(), Some("setup/tools"));
        assert_eq!(route_arg(&args(&["--open"])), None);
        assert_eq!(route_arg(&args(&["nolink"])), None);
    }

    #[test]
    fn only_a_route_in_neko_is_taken() {
        assert_eq!(clean_route("#/tasks?task=abc-1&view=full").as_deref(), Some("tasks?task=abc-1&view=full"));
        assert_eq!(clean_route("/setup/tools").as_deref(), Some("setup/tools"));
        assert_eq!(clean_route("pull-requests").as_deref(), Some("pull-requests"));
        assert_eq!(clean_route("my-plugin:board").as_deref(), Some("my-plugin:board"));
        assert_eq!(clean_route("javascript:void"), None, "a colon is only a plugin page's");
        assert_eq!(clean_route("a:b:c"), None);
        for bad in ["", "   ", "../etc/passwd", "a//b", "https://example.com", "javascript:alert(1)", "tasks task", "tasks\nsettings", "a;b", "<script>", &"x".repeat(MAX_ROUTE + 1)] {
            // `https://example.com` has `//`, `javascript:alert(1)` has parentheses.
            assert_eq!(clean_route(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn a_route_reaches_the_running_window_and_nothing_else_does() {
        let dir = std::env::temp_dir().join(format!("neko-open-{}", std::process::id()));
        let sock = dir.join("open.sock");
        let (tx, rx) = mpsc::channel();
        let tx = std::sync::Mutex::new(tx);
        listen(&sock, move |route| {
            let _ = tx.lock().unwrap().send(route);
        })
        .unwrap();
        assert_eq!(std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777, 0o600, "only the person can reach it");
        assert_eq!(forward("#/tasks?task=t9", &sock), Ok(()));
        assert_eq!(rx.recv_timeout(Duration::from_secs(2)).unwrap(), "tasks?task=t9");
        assert!(forward("https://example.com", &sock).is_err(), "refused before it is sent");
        // A line that is not a route is refused by the listener too.
        let mut raw = UnixStream::connect(&sock).unwrap();
        raw.write_all(b"../../etc/passwd\n").unwrap();
        let mut answer = String::new();
        BufReader::new(raw).read_line(&mut answer).unwrap();
        assert_eq!(answer.trim(), "refused");
        assert!(rx.try_recv().is_err());
        // A second window does not take the socket from the first.
        assert!(listen(&sock, |_| {}).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn with_no_window_running_nothing_is_listening() {
        let sock = std::env::temp_dir().join(format!("neko-open-none-{}.sock", std::process::id()));
        assert_eq!(forward("tasks", &sock), Err(NO_WINDOW.to_owned()));
    }
}
