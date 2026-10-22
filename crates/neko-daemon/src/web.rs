//! The daemon's web bridge: the same requests and events the Unix socket
//! carries, over HTTP on loopback, plus the built web app and user plugins.
//!
//! **Why this exists.** The SwiftUI client cannot be seen or clicked by the
//! agents Neko runs, and every change to it needs a rebuild. A web interface
//! the daemon serves can be read, driven and edited by those agents (a file
//! changes, the page reloads). The daemon stays the only authority: this
//! module owns *transport and access control only* and hands every request to
//! the same `handle_request` the socket uses (through [`Backend`]), so there
//! is no second code path to keep in step.
//!
//! **Security is the first job here, not an afterthought**: the daemon can run
//! full-access agents, so anything that can POST to it can run code as the
//! user. A page open in any browser tab can aim requests at `127.0.0.1`, so a
//! loopback bind alone is not a boundary. Every request must pass, in order:
//!
//! 1. `Host` is exactly `127.0.0.1:<port>` or `localhost:<port>` (DNS
//!    rebinding: an attacker's name that resolves to 127.0.0.1 keeps *its own*
//!    Host header, so this alone defeats it);
//! 2. a present `Origin` is one of those same two origins (a cross-site page
//!    always sends `Origin` on `POST`, and on `fetch`/`EventSource`);
//! 3. a present `Sec-Fetch-Site` is `same-origin` or `none` (defence in depth);
//! 4. a 32-byte random token, as `Authorization: Bearer` or the `neko_token`
//!    cookie that `GET /?token=` sets (constant-time compare).
//!
//! The listener is bound to `127.0.0.1` by [`bind`] and nowhere else; there is
//! no parameter that could widen it.
//!
//! No framework: std sockets and a deliberately small HTTP/1.1 subset
//! (bounded header and body sizes, no chunked uploads, `Connection: close`),
//! so nothing is added to the dependency tree.

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::Duration;

use neko_protocol::{Event, Request, Response};

/// Default port; `NEKO_WEB_PORT` overrides it and `0` turns the bridge off.
pub const DEFAULT_PORT: u16 = 4747;
/// Name of the cookie `GET /?token=` sets.
pub const COOKIE_NAME: &str = "neko_token";

const MAX_HEADER_LINE: u64 = 8 * 1024;
const MAX_HEADERS: usize = 64;
const MAX_BODY: usize = 8 * 1024 * 1024;
/// The one route that takes a larger body: a file attached in the composer
/// (the native composer's own limit is also 32 MB).
const MAX_ATTACHMENT: usize = 32 * 1024 * 1024;
const MAX_STATIC_FILE: u64 = 32 * 1024 * 1024;
const MAX_MANIFEST: u64 = 64 * 1024;
/// One stored value, one namespace's key count, and the custom stylesheet.
const MAX_STORAGE_VALUE: usize = 1024 * 1024;
const MAX_STORAGE_KEYS: usize = 256;
const MAX_CUSTOM_CSS: u64 = 1024 * 1024;
const MAX_CONNECTIONS: usize = 96;
const HEADER_TIMEOUT: Duration = Duration::from_secs(10);
const BODY_TIMEOUT: Duration = Duration::from_secs(30);
const KEEPALIVE: Duration = Duration::from_secs(15);

/// What the bridge needs from the daemon. Kept as a trait so the transport and
/// its security can be tested without a database.
pub trait Backend: Send + Sync + 'static {
    /// The same function the socket calls; a request that streams partial
    /// answers (search) is answered with its final one.
    fn request(&self, request: Request) -> Response;
    /// A stream of every event the daemon broadcasts from now on.
    fn subscribe(&self) -> mpsc::Receiver<Event>;
}

pub struct Config {
    /// The secret every request must carry.
    pub token: String,
    /// The built web app (`index.html` and assets) the app shipped with.
    pub web_root: PathBuf,
    /// Neko's data folder, when a web UI Neko rebuilt itself
    /// (`<data>/web-live`, see `neko_core::self_update`) may be preferred.
    pub live_web: Option<PathBuf>,
    /// `<data dir>/plugins`, read again on every listing.
    pub plugins_dir: PathBuf,
    /// `<data dir>/plugin-data`: one folder per page or plugin, JSON values a
    /// page keeps through `host.storage` (see `storage_route`).
    pub plugin_data_dir: PathBuf,
    /// `<data dir>/custom.css`: the person's own styles, linked after the app's.
    pub custom_css: PathBuf,
    /// `<data dir>/attachments`: where files attached in the web composer are
    /// kept, the same folder the native composer writes to.
    pub attachments_dir: PathBuf,
}

// ---------------------------------------------------------------------------
// Token
// ---------------------------------------------------------------------------

/// Reads `<dir>/web-token`, or creates it: 32 random bytes as 64 hex digits,
/// mode 0600, written once. A file with the wrong length or looser permissions
/// is repaired rather than trusted (a token another user could read is no
/// token).
pub fn load_or_create_token(dir: &Path) -> io::Result<String> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    fs::create_dir_all(dir)?;
    let path = dir.join("web-token");
    if let Ok(existing) = fs::read_to_string(&path) {
        let existing = existing.trim().to_string();
        if existing.len() == 64 && existing.bytes().all(|b| b.is_ascii_hexdigit()) {
            let mode = fs::metadata(&path)?.permissions().mode() & 0o777;
            if mode != 0o600 {
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
            }
            return Ok(existing);
        }
        let _ = fs::remove_file(&path);
    }
    let mut bytes = [0u8; 32];
    fs::File::open("/dev/urandom")?.read_exact(&mut bytes)?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let mut file = fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&path)?;
    file.write_all(token.as_bytes())?;
    Ok(token)
}

// ---------------------------------------------------------------------------
// Access control (pure, so each rejection is testable on its own)
// ---------------------------------------------------------------------------

/// Why a request was refused before it reached any route.
#[derive(Debug, PartialEq, Eq)]
pub enum Rejection {
    BadHost,
    BadOrigin,
    CrossSite,
    Unauthorized,
}

impl Rejection {
    fn status(&self) -> (u16, &'static str) {
        match self {
            Rejection::Unauthorized => (401, "Unauthorized"),
            _ => (403, "Forbidden"),
        }
    }
}

pub fn host_allowed(host: Option<&str>, port: u16) -> bool {
    let Some(host) = host else { return false };
    let host = host.trim().to_ascii_lowercase();
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

pub fn origin_allowed(origin: Option<&str>, port: u16) -> bool {
    let Some(origin) = origin else { return true };
    let origin = origin.trim().to_ascii_lowercase();
    origin == format!("http://127.0.0.1:{port}") || origin == format!("http://localhost:{port}")
}

/// Both comparisons look at every byte, so response timing says nothing about
/// how much of a guess was right.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());
    let mut diff = (a.len() ^ b.len()) as u8;
    for i in 0..a.len().max(b.len()) {
        diff |= a.get(i).copied().unwrap_or(0) ^ b.get(i).copied().unwrap_or(0xff);
    }
    diff == 0
}

fn bearer(authorization: Option<&str>) -> Option<&str> {
    let value = authorization?.trim();
    let (scheme, rest) = value.split_once(' ')?;
    scheme.eq_ignore_ascii_case("bearer").then(|| rest.trim())
}

fn cookie_token(cookie: Option<&str>) -> Option<&str> {
    cookie?.split(';').filter_map(|pair| pair.trim().split_once('=')).find(|(name, _)| *name == COOKIE_NAME).map(|(_, v)| v)
}

/// Paths anyone on this Mac may read: what a browser needs to offer
/// "Install Neko" (the web manifest and the app icons).
pub fn is_public_asset(path: &str) -> bool {
    path == "/manifest.webmanifest" || (path.starts_with("/icons/") && !path.contains("..") && path.matches('/').count() == 2)
}

/// How long the sign-in cookie lasts: a year, so a browser tab or an installed
/// Neko window keeps working across restarts. Rotating `web-token` ends it.
pub const COOKIE_MAX_AGE_SECS: u64 = 365 * 24 * 60 * 60;

/// The whole gate, in the order that matters (see the module doc).
pub fn authorize(req: &Req, port: u16, token: &str) -> Result<(), Rejection> {
    if !host_allowed(req.header("host"), port) {
        return Err(Rejection::BadHost);
    }
    if !origin_allowed(req.header("origin"), port) {
        return Err(Rejection::BadOrigin);
    }
    if let Some(site) = req.header("sec-fetch-site") {
        if !matches!(site.trim(), "same-origin" | "none") {
            return Err(Rejection::CrossSite);
        }
    }
    // The install manifest and its icons carry nothing secret, and a browser
    // fetches them without the cookie (manifests default to no credentials),
    // so they skip the token but keep every check above.
    if req.method == "GET" && is_public_asset(&req.path) {
        return Ok(());
    }
    // The query token is accepted on `GET /` only: that is the one request a
    // shell makes before it has a cookie, and keeping it off every other
    // path keeps the secret out of logs and `Referer`-style leaks.
    let from_query = (req.method == "GET" && req.path == "/").then(|| query_param(&req.query, "token")).flatten();
    // Any presented credential that matches is enough: a stale cookie from an
    // earlier token must not shadow a fresh `?token=`.
    let presented = [bearer(req.header("authorization")), cookie_token(req.header("cookie")), from_query];
    let ok = presented.iter().flatten().fold(false, |ok, candidate| constant_time_eq(candidate, token) | ok);
    if ok { Ok(()) } else { Err(Rejection::Unauthorized) }
}

// ---------------------------------------------------------------------------
// HTTP
// ---------------------------------------------------------------------------

pub struct Req {
    pub method: String,
    /// Path without the query string, still percent-encoded.
    pub path: String,
    pub query: String,
    headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Req {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }
}

struct HttpError(u16, &'static str);

/// Reads one request. `gate` sees the request line and headers *before* any
/// body is read, so a caller that is not allowed in never makes the server
/// buffer a body (the attachment route takes up to 32 MB).
fn read_request(stream: &TcpStream, gate: &dyn Fn(&Req) -> Result<(), HttpError>) -> Result<Req, HttpError> {
    let _ = stream.set_read_timeout(Some(HEADER_TIMEOUT));
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    let read_line = |reader: &mut BufReader<&TcpStream>, line: &mut String| -> Result<(), HttpError> {
        line.clear();
        let n = (&mut *reader).take(MAX_HEADER_LINE).read_line(line).map_err(|_| HttpError(408, "Request Timeout"))?;
        if n == 0 { return Err(HttpError(400, "Bad Request")); }
        if !line.ends_with('\n') { return Err(HttpError(431, "Request Header Fields Too Large")); }
        Ok(())
    };
    read_line(&mut reader, &mut line)?;
    let mut parts = line.trim_end().split(' ');
    let (method, target, version) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
    if parts.next().is_some() || method.is_empty() || !version.starts_with("HTTP/1.") {
        return Err(HttpError(400, "Bad Request"));
    }
    // Origin-form only: an absolute-form target would let the request line
    // name a different host than the Host header does.
    if !target.starts_with('/') || target.starts_with("//") {
        return Err(HttpError(400, "Bad Request"));
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let (method, path, query) = (method.to_string(), path.to_string(), query.to_string());
    let mut headers: Vec<(String, String)> = Vec::new();
    loop {
        read_line(&mut reader, &mut line)?;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() { break; }
        if headers.len() >= MAX_HEADERS { return Err(HttpError(431, "Request Header Fields Too Large")); }
        let Some((name, value)) = trimmed.split_once(':') else { return Err(HttpError(400, "Bad Request")) };
        headers.push((name.trim().to_ascii_lowercase(), value.trim().to_string()));
    }
    let mut req = Req { method, path, query, headers, body: Vec::new() };
    // Smuggling needs two framings; refuse the one this server does not read.
    if req.header("transfer-encoding").is_some() {
        return Err(HttpError(501, "Not Implemented"));
    }
    let length = match req.header("content-length") {
        None => 0,
        Some(v) => v.parse::<usize>().map_err(|_| HttpError(400, "Bad Request"))?,
    };
    gate(&req)?;
    let limit = if req.path == "/api/attachments" {
        MAX_ATTACHMENT
    } else if req.path.starts_with("/api/storage/") {
        MAX_STORAGE_VALUE
    } else {
        MAX_BODY
    };
    if length > limit {
        return Err(HttpError(413, "Payload Too Large"));
    }
    if length > 0 {
        let _ = stream.set_read_timeout(Some(BODY_TIMEOUT));
        let mut body = vec![0u8; length];
        reader.read_exact(&mut body).map_err(|_| HttpError(408, "Request Timeout"))?;
        req.body = body;
    }
    Ok(req)
}

struct Reply {
    status: u16,
    reason: &'static str,
    content_type: &'static str,
    extra: Vec<(&'static str, String)>,
    body: Vec<u8>,
}

impl Reply {
    fn new(status: u16, reason: &'static str, content_type: &'static str, body: impl Into<Vec<u8>>) -> Self {
        Reply { status, reason, content_type, extra: Vec::new(), body: body.into() }
    }
    fn text(status: u16, reason: &'static str) -> Self {
        Self::new(status, reason, "text/plain; charset=utf-8", format!("{status} {reason}\n"))
    }
    fn json(value: &impl serde::Serialize) -> Self {
        match serde_json::to_vec(value) {
            Ok(bytes) => Self::new(200, "OK", "application/json", bytes),
            Err(_) => Self::text(500, "Internal Server Error"),
        }
    }
}

fn write_head(stream: &mut TcpStream, status: u16, reason: &str, content_type: &str, extra: &[(&str, String)], length: Option<usize>) -> io::Result<()> {
    let mut head = format!("HTTP/1.1 {status} {reason}\r\nContent-Type: {content_type}\r\n");
    if let Some(length) = length {
        head.push_str(&format!("Content-Length: {length}\r\n"));
    }
    // Pages and plugins change under the user's feet by design; never cache.
    head.push_str("Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nX-Frame-Options: SAMEORIGIN\r\n");
    head.push_str("Content-Security-Policy: default-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'none'; frame-ancestors 'self'\r\n");
    for (name, value) in extra {
        head.push_str(&format!("{name}: {value}\r\n"));
    }
    head.push_str("Connection: close\r\n\r\n");
    stream.write_all(head.as_bytes())
}

fn write_reply(stream: &mut TcpStream, reply: Reply) {
    let _ = stream.set_write_timeout(Some(Duration::from_secs(10)));
    if write_head(stream, reply.status, reply.reason, reply.content_type, &reply.extra, Some(reply.body.len())).is_ok() {
        let _ = stream.write_all(&reply.body);
    }
    let _ = stream.flush();
    let _ = stream.shutdown(Shutdown::Both);
}

// ---------------------------------------------------------------------------
// Server
// ---------------------------------------------------------------------------

/// Binds `127.0.0.1:port`. There is deliberately no address parameter.
pub fn bind(port: u16) -> io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}

/// The port from `NEKO_WEB_PORT`; `None` when the bridge is switched off.
pub fn configured_port() -> Option<u16> {
    match std::env::var("NEKO_WEB_PORT") {
        Ok(value) => match value.trim().parse::<u16>() {
            Ok(0) => None,
            Ok(port) => Some(port),
            Err(_) => Some(DEFAULT_PORT),
        },
        Err(_) => Some(DEFAULT_PORT),
    }
}

/// Where the built web app lives: `NEKO_WEB_DIR`; inside an installed app
/// (`Contents/Resources/web` beside `Contents/MacOS`); else this checkout's
/// `web/dist`.
pub fn default_web_root() -> PathBuf {
    if let Some(dir) = std::env::var_os("NEKO_WEB_DIR").map(PathBuf::from) {
        return dir;
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bundled) = exe.parent().and_then(Path::parent).map(|c| c.join("Resources/web")) {
            if bundled.join("index.html").is_file() { return bundled; }
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist")
}

pub fn serve(listener: TcpListener, config: Config, backend: Arc<dyn Backend>) {
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
    let config = Arc::new(config);
    let active = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            std::thread::sleep(Duration::from_millis(50));
            continue;
        };
        if active.fetch_add(1, Ordering::AcqRel) >= MAX_CONNECTIONS {
            active.fetch_sub(1, Ordering::AcqRel);
            write_reply(&mut stream, Reply::text(503, "Service Unavailable"));
            continue;
        }
        let (config, backend, active) = (config.clone(), backend.clone(), active.clone());
        std::thread::spawn(move || {
            handle(stream, port, &config, backend.as_ref());
            active.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

const SIGN_IN_REASON: &str = "Sign in";

/// Shown to a browser that has no sign-in cookie yet. Opening Neko from its
/// menu bar item (or the quick panel) signs this browser in.
pub const SIGN_IN_PAGE: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="color-scheme" content="dark light"><title>Neko</title>
<style>body{font:15px/1.5 -apple-system,system-ui,sans-serif;display:grid;place-items:center;min-height:100vh;margin:0}main{max-width:420px;padding:24px}h1{font-size:20px;margin:0 0 8px}p{opacity:.75;margin:0}</style></head>
<body><main><h1>Open Neko from your Mac</h1><p>Click the Neko icon in the menu bar and choose <b>Open Neko</b>. That signs this browser in, and it stays signed in.</p></main></body></html>"#;

fn handle(mut stream: TcpStream, port: u16, config: &Config, backend: &dyn Backend) {
    // Authorization happens between the headers and the body.
    let gate = |req: &Req| authorize(req, port, &config.token).map_err(|rejection| {
        // A person opening the page without the cookie gets told how to sign
        // in, rather than a bare 401.
        if rejection == Rejection::Unauthorized && req.method == "GET" && !req.path.starts_with("/api/") {
            return HttpError(401, SIGN_IN_REASON);
        }
        let (status, reason) = rejection.status();
        HttpError(status, reason)
    });
    let req = match read_request(&stream, &gate) {
        Ok(req) => req,
        Err(HttpError(401, SIGN_IN_REASON)) => return write_reply(&mut stream, Reply::new(401, "Unauthorized", "text/html; charset=utf-8", SIGN_IN_PAGE)),
        Err(HttpError(status, reason)) => return write_reply(&mut stream, Reply::text(status, reason)),
    };
    route(stream, req, config, backend);
}

fn route(mut stream: TcpStream, req: Req, config: &Config, backend: &dyn Backend) {
    let path = req.path.as_str();
    let get = req.method == "GET";
    // `GET /?token=…` (already verified by `authorize`) hands over a cookie —
    // built from the server's own token, never from what the URL said — and
    // redirects, which takes the secret out of the address bar and history.
    let reply = if get && path == "/" && query_param(&req.query, "token").is_some() {
        let mut redirect = Reply::new(302, "Found", "text/plain; charset=utf-8", "");
        redirect.extra.push(("Location", "/".into()));
        redirect.extra.push(("Set-Cookie", format!("{COOKIE_NAME}={}; Path=/; Max-Age={COOKIE_MAX_AGE_SECS}; HttpOnly; SameSite=Strict", config.token)));
        redirect
    } else if path == "/api/request" {
        if req.method != "POST" { Reply::text(405, "Method Not Allowed") } else { api_request(&req, backend) }
    } else if path == "/api/events" {
        if !get { Reply::text(405, "Method Not Allowed") } else { return events(stream, backend) }
    } else if path == "/api/plugins" {
        if !get { Reply::text(405, "Method Not Allowed") } else { Reply::json(&list_plugins(&config.plugins_dir)) }
    } else if path == "/api/plugin-folder" {
        if !get { Reply::text(405, "Method Not Allowed") } else { Reply::json(&serde_json::json!({ "path": config.plugins_dir.to_string_lossy() })) }
    } else if let Some(rest) = path.strip_prefix("/api/plugins/") {
        plugin_route(&config.plugins_dir, &req.method, rest)
    } else if let Some(rest) = path.strip_prefix("/api/storage/") {
        storage_route(&config.plugin_data_dir, &req.method, rest, &req.body)
    } else if path == "/api/customisation" {
        if !get { Reply::text(405, "Method Not Allowed") } else { Reply::json(&customisation_info(config)) }
    } else if path == "/custom.css" {
        if !get { Reply::text(405, "Method Not Allowed") } else { custom_css_reply(&config.custom_css) }
    } else if path == "/api/files" {
        if !get { Reply::text(405, "Method Not Allowed") } else { mention_files(&req.query, backend) }
    } else if path == "/api/attachments" {
        if req.method != "POST" { Reply::text(405, "Method Not Allowed") } else { store_attachment(&config.attachments_dir, req.header("x-neko-filename"), &req.body) }
    } else if let Some(rest) = path.strip_prefix("/plugins/") {
        if !get { Reply::text(405, "Method Not Allowed") } else { serve_plugin_file(&config.plugins_dir, rest) }
    } else if path.starts_with("/api/") {
        Reply::text(404, "Not Found")
    } else if !get {
        Reply::text(405, "Method Not Allowed")
    } else {
        serve_static(&web_roots(config), path)
    };
    write_reply(&mut stream, reply);
}

fn query_param<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').filter_map(|pair| pair.split_once('=')).find(|(n, _)| *n == name).map(|(_, v)| v)
}

fn api_request(req: &Req, backend: &dyn Backend) -> Reply {
    match serde_json::from_slice::<Request>(&req.body) {
        Ok(request) => Reply::json(&backend.request(request)),
        Err(error) => Reply::json(&Response::Error { message: format!("Neko couldn’t read this request. ({error})") }),
    }
}

/// Server-sent events: one `daemon` event per broadcast `Event`, a comment
/// line every 15s so a dead peer is noticed by the failed write.
fn events(mut stream: TcpStream, backend: &dyn Backend) {
    let receiver = backend.subscribe();
    let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
    if write_head(&mut stream, 200, "OK", "text/event-stream", &[], None).is_err() { return; }
    if stream.write_all(b"retry: 2000\n\n").is_err() { return; }
    loop {
        let chunk = match receiver.recv_timeout(KEEPALIVE) {
            Ok(event) => match serde_json::to_string(&event) {
                Ok(json) => format!("event: daemon\ndata: {json}\n\n"),
                Err(_) => continue,
            },
            Err(mpsc::RecvTimeoutError::Timeout) => ": keepalive\n\n".to_string(),
            Err(mpsc::RecvTimeoutError::Disconnected) => return,
        };
        if stream.write_all(chunk.as_bytes()).and_then(|()| stream.flush()).is_err() { return; }
    }
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

fn percent_decode(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = path.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// A relative path made only of ordinary components: no `..`, no absolute
/// root, no backslash, no NUL, no hidden file.
fn safe_relative(path: &str) -> Option<PathBuf> {
    let decoded = percent_decode(path)?;
    if decoded.contains(['\0', '\\']) { return None; }
    let mut out = PathBuf::new();
    for part in decoded.split('/') {
        if part.is_empty() || part == "." { continue; }
        if part == ".." || part.starts_with('.') { return None; }
        out.push(part);
    }
    Some(out)
}

/// Joins and canonicalizes, then proves the result is still inside `root`
/// (this is what stops a symlink from leading out).
fn contained_file(root: &Path, relative: &Path) -> Option<PathBuf> {
    let root = root.canonicalize().ok()?;
    let file = root.join(relative).canonicalize().ok()?;
    (file.starts_with(&root) && file.is_file()).then_some(file)
}

fn mime(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" | "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "ico" => "image/x-icon",
        "webmanifest" => "application/manifest+json",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

fn file_reply(file: &Path) -> Reply {
    match fs::metadata(file) {
        Ok(meta) if meta.len() <= MAX_STATIC_FILE => match fs::read(file) {
            Ok(bytes) => Reply::new(200, "OK", mime(file), bytes),
            Err(_) => Reply::text(404, "Not Found"),
        },
        _ => Reply::text(404, "Not Found"),
    }
}

/// Where static files come from, in order: the live UI Neko rebuilt itself,
/// the one before it (a page still holding the old `index.html` can load the
/// chunks it names), then the UI the app shipped with. Read per request, so a
/// swap takes effect at once and a removed live UI falls back to the bundle.
pub fn web_roots(config: &Config) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(data) = &config.live_web {
        roots.extend(neko_core::self_update::live_web_root(data));
        if !roots.is_empty() {
            roots.extend(neko_core::self_update::previous_web_root(data));
        }
    }
    roots.push(config.web_root.clone());
    roots
}

fn serve_static(roots: &[PathBuf], path: &str) -> Reply {
    let Some(relative) = safe_relative(path) else { return Reply::text(400, "Bad Request") };
    let wanted = if relative.as_os_str().is_empty() { PathBuf::from("index.html") } else { relative.clone() };
    // The page itself always comes from the first root, so a tab never mixes
    // an old shell with new chunks; other files may come from any of them.
    let searched = if wanted == Path::new("index.html") { &roots[..roots.len().min(1)] } else { roots };
    if let Some(file) = searched.iter().find_map(|root| contained_file(root, &wanted)) {
        return file_reply(&file);
    }
    // A client-side route (no extension) falls back to the app shell.
    if relative.extension().is_none() {
        if let Some(index) = roots.first().and_then(|root| contained_file(root, Path::new("index.html"))) {
            return file_reply(&index);
        }
        return Reply::text(404, "The web app is not built. Run `pnpm build` in web/.");
    }
    Reply::text(404, "Not Found")
}

/// Presence of this file in a plugin's folder switches the plugin off.
const DISABLED_MARKER: &str = ".disabled";

fn valid_plugin_id(name: &str) -> bool {
    !name.is_empty() && name.len() <= 64 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn serve_plugin_file(plugins: &Path, rest: &str) -> Reply {
    let Some(relative) = safe_relative(rest) else { return Reply::text(400, "Bad Request") };
    let id_ok = relative.components().next().and_then(|c| c.as_os_str().to_str()).is_some_and(valid_plugin_id);
    if !id_ok || relative.components().count() < 2 { return Reply::text(404, "Not Found"); }
    match contained_file(plugins, &relative) {
        Some(file) => file_reply(&file),
        None => Reply::text(404, "Not Found"),
    }
}

#[derive(serde::Serialize)]
struct PluginInfo {
    id: String,
    title: String,
    /// URL of the ES module the page imports.
    entry: String,
    /// Changes whenever the module file does, so the page reloads it.
    version: u64,
    /// The manifest's own `"version"`, for people ("0.2.0").
    #[serde(skip_serializing_if = "Option::is_none")]
    manifest_version: Option<String>,
    /// A `.disabled` marker file sits in the plugin folder: listed, not loaded.
    disabled: bool,
    /// Absolute path of the plugin's folder.
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    /// The sidebar group the manifest asks for: `main`, `work` or `system`.
    #[serde(skip_serializing_if = "Option::is_none")]
    group: Option<String>,
}

/// Reads `<plugins>/*/manifest.json` now (never cached). A broken plugin is
/// listed with its error rather than hidden, so whoever is editing it (often
/// an agent) can see why it did not load.
fn list_plugins(dir: &Path) -> Vec<PluginInfo> {
    let mut plugins = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return plugins };
    for entry in entries.flatten() {
        let Some(id) = entry.file_name().to_str().map(str::to_string) else { continue };
        if !valid_plugin_id(&id) || !entry.path().is_dir() { continue; }
        let disabled = entry.path().join(DISABLED_MARKER).exists();
        let path = entry.path().to_string_lossy().into_owned();
        let broken = |error: String| PluginInfo { id: id.clone(), title: id.clone(), entry: String::new(), version: 0, manifest_version: None, disabled, path: path.clone(), description: None, error: Some(error), group: None };
        let manifest_path = entry.path().join("manifest.json");
        let manifest = match fs::File::open(&manifest_path).and_then(|f| {
            let mut text = String::new();
            f.take(MAX_MANIFEST).read_to_string(&mut text)?;
            Ok(text)
        }) {
            Ok(text) => text,
            Err(_) => { plugins.push(broken("manifest.json is missing".into())); continue; }
        };
        let json: serde_json::Value = match serde_json::from_str(&manifest) {
            Ok(json) => json,
            Err(error) => { plugins.push(broken(format!("manifest.json is not valid JSON: {error}"))); continue; }
        };
        let module = json.get("entry").and_then(|v| v.as_str()).unwrap_or("index.js");
        let Some(relative) = safe_relative(module).filter(|p| !p.as_os_str().is_empty()) else {
            plugins.push(broken(format!("entry “{module}” is not a plain relative path")));
            continue;
        };
        let Some(file) = contained_file(&entry.path(), &relative) else {
            plugins.push(broken(format!("entry “{module}” does not exist")));
            continue;
        };
        let version = fs::metadata(&file).and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0);
        plugins.push(PluginInfo {
            title: json.get("title").and_then(|v| v.as_str()).unwrap_or(&id).to_string(),
            description: json.get("description").and_then(|v| v.as_str()).map(str::to_string),
            entry: format!("/plugins/{id}/{}", relative.to_string_lossy()),
            version,
            manifest_version: json.get("version").and_then(|v| v.as_str()).map(str::to_string),
            group: json.get("group").and_then(|v| v.as_str()).filter(|g| matches!(*g, "main" | "work" | "system")).map(str::to_string),
            disabled,
            path,
            error: None,
            id,
        });
    }
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    plugins
}

// ---------------------------------------------------------------------------
// Managing plugins: enable, disable, remove
// ---------------------------------------------------------------------------

/// `POST /api/plugins/<name>/enable|disable` and `DELETE /api/plugins/<name>`.
/// The name must be a plain id (letters, digits, `-`, `_`), so no path
/// component can climb out of the plugins folder, and the folder itself must
/// be a real directory inside it (a symlink is never followed or removed).
/// Every success answers with the fresh listing.
fn plugin_route(plugins: &Path, method: &str, rest: &str) -> Reply {
    let (name, action) = match rest.split_once('/') {
        Some((name, action)) => (name, Some(action)),
        None => (rest, None),
    };
    if !valid_plugin_id(name) {
        return Reply::text(400, "Bad Request");
    }
    if !matches!(action, None | Some("enable") | Some("disable")) {
        return Reply::text(404, "Not Found");
    }
    let allowed = match action {
        None => method == "DELETE",
        Some(_) => method == "POST",
    };
    if !allowed {
        return Reply::text(405, "Method Not Allowed");
    }
    let folder = plugins.join(name);
    match fs::symlink_metadata(&folder) {
        Ok(meta) if meta.is_dir() => {}
        _ => return Reply::text(404, "Not Found"),
    }
    let done = match action {
        Some("enable") => match fs::remove_file(folder.join(DISABLED_MARKER)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e),
        },
        Some(_) => fs::write(folder.join(DISABLED_MARKER), b"switched off from Neko's Plugins page\n"),
        None => fs::remove_dir_all(&folder),
    };
    match done {
        Ok(()) => Reply::json(&list_plugins(plugins)),
        Err(_) => Reply::text(500, "Internal Server Error"),
    }
}

// ---------------------------------------------------------------------------
// Storage for pages: `host.storage`
// ---------------------------------------------------------------------------

/// A storage key: letters, digits, `.`, `_`, `-`; no leading dot; 1-128 bytes.
fn valid_storage_key(key: &str) -> bool {
    !key.is_empty() && key.len() <= 128 && !key.starts_with('.') && key.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The file a key is kept in. `.json` is appended, so a key can never name a
/// dotfile or the folder's own bookkeeping.
fn storage_file(namespace: &Path, key: &str) -> PathBuf {
    namespace.join(format!("{key}.json"))
}

/// Keys of one namespace, sorted. Only plain files named `<key>.json`.
fn storage_keys(namespace: &Path) -> Vec<(String, u64)> {
    let mut keys = Vec::new();
    let Ok(entries) = fs::read_dir(namespace) else { return keys };
    for entry in entries.flatten() {
        let Some(name) = entry.file_name().to_str().map(str::to_string) else { continue };
        let Some(key) = name.strip_suffix(".json").filter(|k| valid_storage_key(k)) else { continue };
        match entry.metadata() {
            Ok(meta) if meta.is_file() => keys.push((key.to_string(), meta.len())),
            _ => {}
        }
    }
    keys.sort();
    keys
}

/// `GET /api/storage/<id>` lists keys; `GET|PUT|DELETE /api/storage/<id>/<key>`
/// reads, writes (a JSON body) and removes one value. `<id>` follows the plugin
/// id rule, so a namespace is one flat folder under `plugin-data` and nothing
/// can climb out of it; a symlinked namespace or value is refused, never
/// followed. Values are bounded (1 MiB, 256 keys) and written atomically.
/// The same Host/Origin/token gates as every other route apply before this runs.
fn storage_route(root: &Path, method: &str, rest: &str, body: &[u8]) -> Reply {
    let (id, key) = match rest.split_once('/') {
        Some((id, key)) => (id, Some(key)),
        None => (rest, None),
    };
    if !valid_plugin_id(id) || key.is_some_and(|k| !valid_storage_key(k)) {
        return Reply::text(400, "Bad Request");
    }
    let namespace = root.join(id);
    let namespace_state = fs::symlink_metadata(&namespace);
    if namespace_state.as_ref().is_ok_and(|m| !m.is_dir()) {
        return Reply::text(404, "Not Found");
    }
    let Some(key) = key else {
        if method != "GET" { return Reply::text(405, "Method Not Allowed"); }
        let keys: Vec<_> = storage_keys(&namespace).into_iter().map(|(key, size)| serde_json::json!({ "key": key, "size": size })).collect();
        return Reply::json(&serde_json::json!({ "keys": keys }));
    };
    let file = storage_file(&namespace, key);
    match method {
        "GET" => match fs::symlink_metadata(&file) {
            Ok(meta) if meta.is_file() && meta.len() <= MAX_STORAGE_VALUE as u64 => match fs::read(&file) {
                Ok(bytes) => Reply::new(200, "OK", "application/json", bytes),
                Err(_) => Reply::text(404, "Not Found"),
            },
            _ => Reply::text(404, "Not Found"),
        },
        "PUT" => {
            if body.len() > MAX_STORAGE_VALUE { return Reply::text(413, "Payload Too Large"); }
            if serde_json::from_slice::<serde_json::Value>(body).is_err() { return Reply::text(400, "Bad Request"); }
            if namespace_state.is_err() && fs::create_dir_all(&namespace).is_err() { return Reply::text(500, "Internal Server Error"); }
            if fs::symlink_metadata(&file).is_ok_and(|m| !m.is_file()) { return Reply::text(404, "Not Found"); }
            if !file.exists() && storage_keys(&namespace).len() >= MAX_STORAGE_KEYS { return Reply::text(413, "Payload Too Large"); }
            let temp = namespace.join(format!(".{key}.tmp"));
            match fs::write(&temp, body).and_then(|()| fs::rename(&temp, &file)) {
                Ok(()) => Reply::json(&serde_json::json!({ "ok": true })),
                Err(_) => {
                    let _ = fs::remove_file(&temp);
                    Reply::text(500, "Internal Server Error")
                }
            }
        }
        "DELETE" => match fs::symlink_metadata(&file) {
            Ok(meta) if meta.is_file() => match fs::remove_file(&file) {
                Ok(()) => Reply::json(&serde_json::json!({ "ok": true })),
                Err(_) => Reply::text(500, "Internal Server Error"),
            },
            _ => Reply::text(404, "Not Found"),
        },
        _ => Reply::text(405, "Method Not Allowed"),
    }
}

// ---------------------------------------------------------------------------
// The person's own stylesheet: `<data dir>/custom.css`
// ---------------------------------------------------------------------------

fn custom_css_version(file: &Path) -> u64 {
    fs::metadata(file).and_then(|m| m.modified()).ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64).unwrap_or(0)
}

/// Where things a person customises live, and when `custom.css` last changed
/// (so the shell reloads it by itself).
fn customisation_info(config: &Config) -> serde_json::Value {
    serde_json::json!({
        "plugins_dir": config.plugins_dir.to_string_lossy(),
        "css_path": config.custom_css.to_string_lossy(),
        "css_exists": config.custom_css.is_file(),
        "css_version": custom_css_version(&config.custom_css),
    })
}

/// `GET /custom.css`: an empty sheet when there is none (a `<link>` should not
/// error), a plain file only (a symlink is not followed), at most 1 MiB.
fn custom_css_reply(file: &Path) -> Reply {
    let css = |bytes: Vec<u8>| Reply::new(200, "OK", "text/css; charset=utf-8", bytes);
    match fs::symlink_metadata(file) {
        Ok(meta) if meta.is_file() && meta.len() <= MAX_CUSTOM_CSS => css(fs::read(file).unwrap_or_default()),
        Ok(meta) if meta.is_file() => Reply::text(413, "Payload Too Large"),
        _ => css(Vec::new()),
    }
}

// ---------------------------------------------------------------------------
// File mentions (`@` in the composer)
// ---------------------------------------------------------------------------

const MENTION_FILES: usize = 5;
const MENTION_TIMEOUT: Duration = Duration::from_millis(1500);
const MENTION_NOISE: [&str; 8] = ["/node_modules/", "/.git/", "/target/", "/.build/", "/DerivedData/", "/Pods/", "/Library/", "/."];

/// Letters, digits and `._- ` only: what is left once nothing can break out of
/// the Spotlight predicate (the native composer filters the same way).
fn mention_pattern(query: &str) -> String {
    query.chars().filter(|c| c.is_alphanumeric() || "._- ".contains(*c)).collect()
}

/// "Documents › neko" for `/Users/me/Documents/neko/a.rs`: the last two folders
/// of the parent, relative to home, as the native composer shows them.
fn mention_location(path: &str, home: &str) -> String {
    let parent = Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let relative = parent.strip_prefix(home).unwrap_or(&parent).trim_matches('/').to_string();
    if relative.is_empty() {
        return "Home".into();
    }
    let parts: Vec<&str> = relative.split('/').collect();
    parts[parts.len().saturating_sub(2)..].join(" › ")
}

/// `GET /api/files?q=<start of a file name>&workspace=<id>`: up to five files
/// whose names start with `q` in the folders of that workspace (or of every
/// workspace). The same Spotlight query the native composer runs, resolved
/// here from the daemon's own snapshot so the browser never names a folder.
fn mention_files(query: &str, backend: &dyn Backend) -> Reply {
    let none = || Reply::json(&serde_json::json!({ "files": [] }));
    let q = percent_decode(&query_param(query, "q").unwrap_or("").replace('+', " ")).unwrap_or_default();
    let workspace = percent_decode(query_param(query, "workspace").unwrap_or("")).unwrap_or_default();
    let pattern = mention_pattern(&q);
    if q.chars().count() < 2 || pattern.is_empty() || pattern.len() > 64 {
        return none();
    }
    let Response::Workbench(snapshot) = backend.request(Request::Workbench(neko_protocol::workbench::Command::Snapshot)) else { return none() };
    let mut folders: Vec<String> = Vec::new();
    for workspace_record in snapshot.workspaces.iter().filter(|w| workspace.is_empty() || w.id == workspace) {
        let listed = snapshot.workspace_folders.get(&workspace_record.id).cloned().unwrap_or_default();
        let listed = if listed.is_empty() { vec![workspace_record.repository.clone()] } else { listed };
        folders.extend(listed.into_iter().filter(|f| !f.is_empty() && Path::new(f).is_dir()));
    }
    folders.dedup();
    folders.truncate(6);
    if folders.is_empty() {
        return none();
    }
    let mut command = std::process::Command::new("/usr/bin/mdfind");
    for folder in &folders {
        command.arg("-onlyin").arg(folder);
    }
    command.arg(format!("kMDItemFSName == '{pattern}*'cd")).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
    let Ok(mut child) = command.spawn() else { return none() };
    let Some(mut out) = child.stdout.take() else { return none() };
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = out.read_to_string(&mut text);
        let _ = tx.send(text);
    });
    let text = match rx.recv_timeout(MENTION_TIMEOUT) {
        Ok(text) => text,
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            return none();
        }
    };
    let _ = child.wait();
    let home = std::env::var("HOME").unwrap_or_default();
    let files: Vec<serde_json::Value> = text
        .lines()
        .filter(|path| !MENTION_NOISE.iter().any(|noise| path.contains(noise)))
        .take(MENTION_FILES)
        .map(|path| {
            let name = Path::new(path).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            serde_json::json!({ "path": path, "name": name, "location": mention_location(path, &home) })
        })
        .collect();
    Reply::json(&serde_json::json!({ "files": files }))
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

fn sniff_image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("jpg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() > 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

/// Letters, digits and `/` stay; everything else is `%XX` (what the native
/// composer's `URL.absoluteString` produces for a path).
fn percent_encode_path(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'/' | b'-' | b'.' | b'_' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Keeps a file the browser sent under `<attachments>/<sha256>[.ext]` and
/// answers with the markdown reference the native composer puts in a message
/// (`![Image](file://…)` for an image, `[name](file://…)` otherwise), so the
/// daemon and the agents see exactly what they see from the native app. The
/// name only ever becomes a label and an extension; the path is the hash.
fn store_attachment(dir: &Path, filename: Option<&str>, body: &[u8]) -> Reply {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    if body.is_empty() {
        return Reply::text(400, "Bad Request");
    }
    let decoded = filename.and_then(percent_decode).unwrap_or_default();
    let base = decoded.rsplit(['/', '\\']).next().unwrap_or("").trim();
    let name: String = base.chars().filter(|c| !c.is_control()).take(120).collect();
    let name = if name.is_empty() || name.starts_with('.') { "file".to_string() } else { name };
    let image = sniff_image_extension(body);
    let extension: String = match image {
        Some(ext) => ext.to_string(),
        None => Path::new(&name).extension().and_then(|e| e.to_str()).unwrap_or("")
            .to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).take(12).collect(),
    };
    let hash: String = Sha256::digest(body).iter().map(|b| format!("{b:02x}")).collect();
    let file_name = if extension.is_empty() { hash } else { format!("{hash}.{extension}") };
    if fs::create_dir_all(dir).is_err() {
        return Reply::text(500, "Internal Server Error");
    }
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let path = dir.join(&file_name);
    if !path.exists() {
        let written = fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&path)
            .and_then(|mut file| file.write_all(body));
        if written.is_err() {
            let _ = fs::remove_file(&path);
            return Reply::text(500, "Internal Server Error");
        }
    }
    let absolute = path.to_string_lossy().into_owned();
    let reference = if image.is_some() {
        format!("![Image](file://{absolute})")
    } else {
        let label = name.replace('\\', "\\\\").replace('[', "\\[").replace(']', "\\]");
        format!("[{label}](file://{})", percent_encode_path(&absolute))
    };
    Reply::json(&serde_json::json!({ "name": name, "reference": reference, "is_image": image.is_some() }))
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
