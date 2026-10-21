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
const MAX_STATIC_FILE: u64 = 32 * 1024 * 1024;
const MAX_MANIFEST: u64 = 64 * 1024;
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
    /// The built web app (`index.html` and assets).
    pub web_root: PathBuf,
    /// `<data dir>/plugins`, read again on every listing.
    pub plugins_dir: PathBuf,
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

fn read_request(stream: &TcpStream) -> Result<Req, HttpError> {
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
    if length > MAX_BODY {
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

fn handle(mut stream: TcpStream, port: u16, config: &Config, backend: &dyn Backend) {
    let req = match read_request(&stream) {
        Ok(req) => req,
        Err(HttpError(status, reason)) => return write_reply(&mut stream, Reply::text(status, reason)),
    };
    if let Err(rejection) = authorize(&req, port, &config.token) {
        let (status, reason) = rejection.status();
        return write_reply(&mut stream, Reply::text(status, reason));
    }
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
        redirect.extra.push(("Set-Cookie", format!("{COOKIE_NAME}={}; Path=/; HttpOnly; SameSite=Strict", config.token)));
        redirect
    } else if path == "/api/request" {
        if req.method != "POST" { Reply::text(405, "Method Not Allowed") } else { api_request(&req, backend) }
    } else if path == "/api/events" {
        if !get { Reply::text(405, "Method Not Allowed") } else { return events(stream, backend) }
    } else if path == "/api/plugins" {
        if !get { Reply::text(405, "Method Not Allowed") } else { Reply::json(&list_plugins(&config.plugins_dir)) }
    } else if let Some(rest) = path.strip_prefix("/plugins/") {
        if !get { Reply::text(405, "Method Not Allowed") } else { serve_plugin_file(&config.plugins_dir, rest) }
    } else if path.starts_with("/api/") {
        Reply::text(404, "Not Found")
    } else if !get {
        Reply::text(405, "Method Not Allowed")
    } else {
        serve_static(&config.web_root, path)
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

fn serve_static(root: &Path, path: &str) -> Reply {
    let Some(relative) = safe_relative(path) else { return Reply::text(400, "Bad Request") };
    let wanted = if relative.as_os_str().is_empty() { PathBuf::from("index.html") } else { relative.clone() };
    if let Some(file) = contained_file(root, &wanted) {
        return file_reply(&file);
    }
    // A client-side route (no extension) falls back to the app shell.
    if relative.extension().is_none() {
        if let Some(index) = contained_file(root, Path::new("index.html")) {
            return file_reply(&index);
        }
        return Reply::text(404, "The web app is not built. Run `pnpm build` in web/.");
    }
    Reply::text(404, "Not Found")
}

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
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
        let broken = |error: String| PluginInfo { id: id.clone(), title: id.clone(), entry: String::new(), version: 0, description: None, error: Some(error) };
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
            error: None,
            id,
        });
    }
    plugins.sort_by(|a, b| a.id.cmp(&b.id));
    plugins
}

#[cfg(test)]
#[path = "web_tests.rs"]
mod tests;
