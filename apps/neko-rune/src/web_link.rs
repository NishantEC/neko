//! "Open the web UI": the way out of a placeholder, so nobody is stuck while the native app has
//! no screen for something yet.
//!
//! The daemon serves the web UI on 127.0.0.1 and wants the secret it keeps in
//! `<data dir>/web-token` (`crates/neko-daemon/src/web.rs`, `WebHome.swift` does the same). The
//! address is read on the link thread, never in a frame or a handler, and arrives in the app's
//! state; the token is only read, never written, rotated or logged.

use std::io;
use std::path::Path;

/// The daemon's default web port (`web::DEFAULT_PORT`).
pub const DEFAULT_PORT: u16 = 4747;

#[derive(Clone, PartialEq, Eq)]
pub struct WebTarget {
    pub port: u16,
    token: String,
}

// The token is a secret: never printed, not even by `{:?}` in a log or a test failure.
impl std::fmt::Debug for WebTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WebTarget").field("port", &self.port).field("token", &"<hidden>").finish()
    }
}

/// `NEKO_WEB_PORT` as the daemon reads it: unset or unparsable is the default, `0` is off.
pub fn configured_port(env: Option<&str>) -> Option<u16> {
    match env.map(str::trim).map(str::parse::<u16>) {
        None | Some(Err(_)) => Some(DEFAULT_PORT),
        Some(Ok(0)) => None,
        Some(Ok(port)) => Some(port),
    }
}

impl WebTarget {
    /// Reads `<dir>/web-token`. None when the bridge is off, the daemon has not written its
    /// token yet, or the file is not a 64-digit hex token.
    pub fn discover(data_dir: &Path, port_env: Option<&str>) -> Option<WebTarget> {
        let port = configured_port(port_env)?;
        let token = std::fs::read_to_string(data_dir.join("web-token")).ok()?;
        let token = token.trim();
        (token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit())).then(|| WebTarget { port, token: token.to_owned() })
    }

    /// A target from known parts (scenes and tests).
    pub fn new(port: u16, token: &str) -> WebTarget {
        WebTarget { port, token: token.to_owned() }
    }

    /// `http://127.0.0.1:<port>/?token=…`, then the web UI's hash route for `page` (`inbox`,
    /// `tasks`, none for home). The daemon answers the token URL with a cookie and a redirect to
    /// `/`, and the fragment survives the redirect.
    pub fn url(&self, page: Option<&str>) -> String {
        format!("http://127.0.0.1:{}/?token={}{}", self.port, self.token, page.map(|p| format!("#/{p}")).unwrap_or_default())
    }

    /// The same address without the token, safe to show.
    pub fn display(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }
}

/// Opens a URL in the default browser. macOS's `open`, with the URL as its one argument.
pub fn open_url(url: &str) -> io::Result<()> {
    std::process::Command::new("/usr/bin/open").arg(url).spawn().map(|_| ())
}

/// Shows a file in Finder: macOS's `open -R`, with the path as its one argument after the flag.
pub fn reveal_in_finder(path: &str) -> io::Result<()> {
    std::process::Command::new("/usr/bin/open").arg("-R").arg(path).spawn().map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("neko-rune-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_port_follows_the_daemons_rules() {
        assert_eq!(configured_port(None), Some(4747));
        assert_eq!(configured_port(Some("5000")), Some(5000));
        assert_eq!(configured_port(Some(" 5000 ")), Some(5000));
        assert_eq!(configured_port(Some("nope")), Some(4747));
        assert_eq!(configured_port(Some("0")), None, "0 turns the bridge off");
    }

    #[test]
    fn a_valid_token_makes_the_url_to_the_same_place() {
        let dir = temp("ok");
        std::fs::write(dir.join("web-token"), format!("{TOKEN}\n")).unwrap();
        let target = WebTarget::discover(&dir, Some("4750")).unwrap();
        assert_eq!(target.url(None), format!("http://127.0.0.1:4750/?token={TOKEN}"));
        assert_eq!(target.url(Some("schedules")), format!("http://127.0.0.1:4750/?token={TOKEN}#/schedules"));
        assert!(!target.display().contains(TOKEN), "what is shown never carries the secret");
        assert!(!format!("{target:?}").contains(TOKEN), "nor does a debug print");
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_missing_short_or_odd_token_is_no_target() {
        let dir = temp("bad");
        assert!(WebTarget::discover(&dir, None).is_none(), "no file yet");
        std::fs::write(dir.join("web-token"), "short").unwrap();
        assert!(WebTarget::discover(&dir, None).is_none());
        std::fs::write(dir.join("web-token"), "z".repeat(64)).unwrap();
        assert!(WebTarget::discover(&dir, None).is_none(), "not hex");
        std::fs::write(dir.join("web-token"), TOKEN).unwrap();
        assert!(WebTarget::discover(&dir, Some("0")).is_none(), "bridge off");
        std::fs::remove_dir_all(dir).ok();
    }
}
