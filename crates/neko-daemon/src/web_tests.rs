//! Tests for the web bridge. Every rejection the module documents has its own
//! test, and each asserts that nothing reached the (fake) daemon.

use super::*;
use std::sync::Mutex;

const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

struct Fake {
    seen: Mutex<Vec<String>>,
    events: Mutex<Vec<mpsc::Sender<Event>>>,
}

impl Backend for Fake {
    fn request(&self, request: Request) -> Response {
        self.seen.lock().unwrap().push(format!("{request:?}"));
        match request {
            Request::Ping => Response::Pong,
            _ => Response::Error { message: "unexpected".into() },
        }
    }
    fn subscribe(&self) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel();
        self.events.lock().unwrap().push(tx);
        rx
    }
}

struct Harness {
    port: u16,
    backend: Arc<Fake>,
    dir: tempfile::TempDir,
}

fn start() -> Harness {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("web/assets")).unwrap();
    fs::write(dir.path().join("web/index.html"), "<html>app</html>").unwrap();
    fs::write(dir.path().join("web/assets/app.js"), "console.log(1)").unwrap();
    fs::write(dir.path().join("secret.txt"), "outside the web root").unwrap();
    fs::create_dir_all(dir.path().join("plugins")).unwrap();
    let listener = bind(0).unwrap();
    let port = listener.local_addr().unwrap().port();
    let backend = Arc::new(Fake { seen: Mutex::new(Vec::new()), events: Mutex::new(Vec::new()) });
    let config = Config { token: TOKEN.into(), web_root: dir.path().join("web"), plugins_dir: dir.path().join("plugins") };
    let served = backend.clone();
    std::thread::spawn(move || serve(listener, config, served));
    Harness { port, backend, dir }
}

struct Answer {
    status: u16,
    head: String,
    body: String,
}

/// One raw HTTP exchange; `lines` are the headers after the request line.
fn http(port: u16, method: &str, target: &str, lines: &[&str], body: &str) -> Answer {
    let mut raw = format!("{method} {target} HTTP/1.1\r\n");
    for line in lines {
        raw.push_str(line);
        raw.push_str("\r\n");
    }
    if !body.is_empty() {
        raw.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    stream.write_all(raw.as_bytes()).unwrap();
    let mut text = String::new();
    let _ = stream.read_to_string(&mut text);
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
    let status = head.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    Answer { status, head: head.to_string(), body: body.to_string() }
}

fn good(port: u16) -> Vec<String> {
    vec![format!("Host: 127.0.0.1:{port}"), format!("Authorization: Bearer {TOKEN}")]
}

fn get(port: u16, target: &str, extra: &[&str]) -> Answer {
    let mut lines = good(port);
    lines.extend(extra.iter().map(|s| s.to_string()));
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    http(port, "GET", target, &refs, "")
}

#[test]
fn listens_on_loopback_only() {
    let listener = bind(0).unwrap();
    let ip = listener.local_addr().unwrap().ip();
    assert!(ip.is_loopback());
    assert_eq!(ip.to_string(), "127.0.0.1");
}

#[test]
fn rejects_missing_wrong_and_partial_tokens() {
    let h = start();
    let host = format!("Host: 127.0.0.1:{}", h.port);
    assert_eq!(http(h.port, "GET", "/", &[&host], "").status, 401, "no token");
    assert_eq!(http(h.port, "GET", "/", &[&host, "Authorization: Bearer nope"], "").status, 401, "wrong bearer");
    assert_eq!(http(h.port, "GET", "/", &[&host, &format!("Authorization: Bearer {}", &TOKEN[..63])], "").status, 401, "truncated");
    assert_eq!(http(h.port, "GET", "/", &[&host, "Cookie: neko_token=nope"], "").status, 401, "wrong cookie");
    // The URL token is honoured on `/` only, never on an API path.
    assert_eq!(http(h.port, "GET", &format!("/api/plugins?token={TOKEN}"), &[&host], "").status, 401);
    assert_eq!(http(h.port, "POST", "/api/request", &[&host, "Content-Type: application/json"], "\"Ping\"").status, 401);
    assert!(h.backend.seen.lock().unwrap().is_empty(), "an unauthorised request must never reach the daemon");
}

#[test]
fn accepts_bearer_cookie_and_the_token_url_which_sets_the_cookie() {
    let h = start();
    let host = format!("Host: 127.0.0.1:{}", h.port);
    assert_eq!(get(h.port, "/", &[]).status, 200);
    assert_eq!(http(h.port, "GET", "/", &[&host, &format!("Cookie: other=1; neko_token={TOKEN}")], "").status, 200);
    let redirect = http(h.port, "GET", &format!("/?token={TOKEN}"), &[&host], "");
    assert_eq!(redirect.status, 302);
    assert!(redirect.head.contains(&format!("Set-Cookie: neko_token={TOKEN}; Path=/; HttpOnly; SameSite=Strict")), "{}", redirect.head);
    assert!(redirect.head.contains("Location: /"));
    // A stale cookie must not shadow a good URL token.
    assert_eq!(http(h.port, "GET", &format!("/?token={TOKEN}"), &[&host, "Cookie: neko_token=old"], "").status, 302);
    // And a wrong URL token sets nothing, whatever it contains.
    let bad = http(h.port, "GET", "/?token=evil%0d%0aSet-Cookie:x", &[&host], "");
    assert_eq!(bad.status, 401);
    assert!(!bad.head.contains("Set-Cookie"));
}

#[test]
fn rejects_a_foreign_host_even_with_a_valid_token() {
    let h = start();
    let auth = format!("Authorization: Bearer {TOKEN}");
    let other_port = format!("Host: 127.0.0.1:{}", h.port + 1);
    let suffix = format!("Host: localhost.evil.example:{}", h.port);
    for host in ["Host: evil.example", "Host: evil.example:4747", "Host: 127.0.0.1", other_port.as_str(), suffix.as_str(), "Host: 0.0.0.0:1"] {
        assert_eq!(http(h.port, "GET", "/", &[host, &auth], "").status, 403, "{host}");
    }
    assert_eq!(http(h.port, "GET", "/", &[&auth], "").status, 403, "no Host header");
    assert_eq!(http(h.port, "GET", "/", &[&format!("Host: localhost:{}", h.port), &auth], "").status, 200, "localhost is allowed");
    assert_eq!(http(h.port, "GET", "/", &[&format!("Host: LOCALHOST:{}", h.port), &auth], "").status, 200, "host names are case-insensitive");
    assert!(h.backend.seen.lock().unwrap().is_empty());
}

#[test]
fn rejects_a_cross_origin_request_even_with_a_valid_token() {
    let h = start();
    let other_port = format!("Origin: http://127.0.0.1:{}", h.port + 1);
    let https = format!("Origin: https://127.0.0.1:{}", h.port);
    let suffix = format!("Origin: http://localhost.evil.example:{}", h.port);
    for origin in ["Origin: https://evil.example", "Origin: http://evil.example", "Origin: null", other_port.as_str(), https.as_str(), suffix.as_str()] {
        let mut lines = good(h.port);
        lines.push(origin.to_string());
        lines.push("Content-Type: application/json".into());
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        assert_eq!(http(h.port, "POST", "/api/request", &refs, "\"Ping\"").status, 403, "{origin}");
        assert_eq!(http(h.port, "GET", "/api/events", &refs, "").status, 403, "{origin} on events");
    }
    assert!(h.backend.seen.lock().unwrap().is_empty(), "no cross-origin request may reach the daemon");
    // Same origin (what the served page itself sends) is fine.
    let mut lines = good(h.port);
    lines.push(format!("Origin: http://127.0.0.1:{}", h.port));
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(http(h.port, "POST", "/api/request", &refs, "\"Ping\"").status, 200);
}

#[test]
fn rejects_cross_site_fetch_metadata() {
    let h = start();
    assert_eq!(get(h.port, "/", &["Sec-Fetch-Site: cross-site"]).status, 403);
    assert_eq!(get(h.port, "/", &["Sec-Fetch-Site: same-site"]).status, 403, "another localhost app is same-site");
    assert_eq!(get(h.port, "/", &["Sec-Fetch-Site: same-origin"]).status, 200);
    assert_eq!(get(h.port, "/", &["Sec-Fetch-Site: none"]).status, 200);
}

#[test]
fn the_same_request_reaches_the_daemon_and_the_answer_comes_back() {
    let h = start();
    let mut lines = good(h.port);
    lines.push("Content-Type: application/json".into());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let answer = http(h.port, "POST", "/api/request", &refs, "\"Ping\"");
    assert_eq!(answer.status, 200);
    assert_eq!(answer.body, "\"Pong\"");
    assert_eq!(h.backend.seen.lock().unwrap().as_slice(), ["Ping"]);
    let garbage = http(h.port, "POST", "/api/request", &refs, "{\"Nope\":1}");
    assert!(garbage.body.contains("Error"), "{}", garbage.body);
    assert_eq!(h.backend.seen.lock().unwrap().len(), 1, "an unreadable request is not forwarded");
    assert_eq!(get(h.port, "/api/request", &[]).status, 405);
}

#[test]
fn bounds_request_size_and_refuses_chunked_bodies() {
    let h = start();
    let mut lines = good(h.port);
    lines.push("Content-Type: application/json".into());
    let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
    let too_big = format!("Content-Length: {}", MAX_BODY + 1);
    let mut with_length = refs.clone();
    with_length.push(&too_big);
    assert_eq!(http(h.port, "POST", "/api/request", &with_length, "").status, 413);
    let mut chunked = refs.clone();
    chunked.push("Transfer-Encoding: chunked");
    assert_eq!(http(h.port, "POST", "/api/request", &chunked, "").status, 501);
    let huge = format!("X-Pad: {}", "a".repeat(9000));
    let mut padded = refs.clone();
    padded.push(&huge);
    assert_eq!(http(h.port, "GET", "/", &padded, "").status, 431);
    assert_eq!(http(h.port, "GET", "http://evil.example/", &refs, "").status, 400, "absolute-form target");
    assert_eq!(http(h.port, "GET", "//evil.example/", &refs, "").status, 400, "network-path target");
}

#[test]
fn serves_the_app_with_a_spa_fallback_and_cannot_be_walked_out_of_the_root() {
    let h = start();
    let index = get(h.port, "/", &[]);
    assert_eq!((index.status, index.body.as_str()), (200, "<html>app</html>"));
    assert!(index.head.contains("text/html"));
    assert!(index.head.contains("Content-Security-Policy"));
    assert!(index.head.contains("Cache-Control: no-store"));
    assert_eq!(get(h.port, "/assets/app.js", &[]).status, 200);
    assert_eq!(get(h.port, "/some/client/route", &[]).body, "<html>app</html>", "extensionless paths fall back to the shell");
    assert_eq!(get(h.port, "/missing.js", &[]).status, 404);
    for evil in ["/../secret.txt", "/%2e%2e/secret.txt", "/assets/../../secret.txt", "/assets/%2e%2e%2f%2e%2e%2fsecret.txt", "/..%2fsecret.txt", "/%00", "/assets%5c..%5csecret.txt"] {
        let answer = get(h.port, evil, &[]);
        assert!(answer.status == 400 || answer.status == 404, "{evil} -> {}", answer.status);
        assert!(!answer.body.contains("outside the web root"), "{evil}");
    }
    // A symlink inside the root pointing outside is not followed.
    std::os::unix::fs::symlink(h.dir.path().join("secret.txt"), h.dir.path().join("web/link.txt")).unwrap();
    let linked = get(h.port, "/link.txt", &[]);
    assert_eq!(linked.status, 404);
    assert!(!linked.body.contains("outside the web root"));
}

#[test]
fn plugins_are_read_from_disk_on_every_listing_and_are_contained() {
    let h = start();
    let listing = |h: &Harness| get(h.port, "/api/plugins", &[]).body;
    assert_eq!(listing(&h), "[]");
    let plugin = h.dir.path().join("plugins/pr-board");
    fs::create_dir_all(&plugin).unwrap();
    fs::write(plugin.join("manifest.json"), r#"{"title":"PR board","entry":"index.js"}"#).unwrap();
    fs::write(plugin.join("index.js"), "export default () => {}").unwrap();
    let json: serde_json::Value = serde_json::from_str(&listing(&h)).unwrap();
    assert_eq!(json[0]["id"], "pr-board");
    assert_eq!(json[0]["title"], "PR board");
    assert_eq!(json[0]["entry"], "/plugins/pr-board/index.js");
    assert_eq!(get(h.port, "/plugins/pr-board/index.js", &[]).body, "export default () => {}");
    // A broken manifest is reported, not hidden.
    let broken = h.dir.path().join("plugins/bad");
    fs::create_dir_all(&broken).unwrap();
    fs::write(broken.join("manifest.json"), "{").unwrap();
    let json: serde_json::Value = serde_json::from_str(&listing(&h)).unwrap();
    assert!(json.as_array().unwrap().iter().any(|p| p["id"] == "bad" && p["error"].as_str().unwrap().contains("not valid JSON")));
    // An entry that points outside its plugin is refused.
    fs::write(broken.join("manifest.json"), r#"{"entry":"../pr-board/index.js"}"#).unwrap();
    let json: serde_json::Value = serde_json::from_str(&listing(&h)).unwrap();
    assert!(json.as_array().unwrap().iter().any(|p| p["id"] == "bad" && p["error"].is_string()));
    for evil in ["/plugins/../web/index.html", "/plugins/pr-board/../../secret.txt", "/plugins/%2e%2e/secret.txt", "/plugins/pr-board", "/plugins/", "/plugins/.hidden/x"] {
        let answer = get(h.port, evil, &[]);
        assert!(answer.status == 400 || answer.status == 404, "{evil} -> {}", answer.status);
    }
    assert_eq!(get(h.port, "/plugins/pr-board/index.js", &["Origin: https://evil.example"]).status, 403);
}

#[test]
fn streams_daemon_events_to_an_authorised_client() {
    let h = start();
    let mut stream = TcpStream::connect(("127.0.0.1", h.port)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(stream, "GET /api/events HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {TOKEN}\r\n\r\n", h.port).unwrap();
    let mut reader = BufReader::new(stream);
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    assert!(line.contains("200"), "{line}");
    for _ in 0..100 {
        if !h.backend.events.lock().unwrap().is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    h.backend.events.lock().unwrap()[0].send(Event::AttentionChanged { count: 3 }).unwrap();
    let mut seen = String::new();
    for _ in 0..40 {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        seen.push_str(&line);
        if line.starts_with("data:") {
            break;
        }
    }
    assert!(seen.contains("text/event-stream"), "{seen}");
    assert!(seen.contains(r#"data: {"AttentionChanged":{"count":3}}"#), "{seen}");
}

#[test]
fn the_token_is_32_random_bytes_kept_once_in_a_private_file() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let token = load_or_create_token(dir.path()).unwrap();
    assert_eq!(token.len(), 64);
    assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
    let path = dir.path().join("web-token");
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(load_or_create_token(dir.path()).unwrap(), token, "stable across restarts");
    // Loose permissions are repaired; a damaged file is replaced.
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(load_or_create_token(dir.path()).unwrap(), token);
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    fs::write(&path, "short").unwrap();
    let fresh = load_or_create_token(dir.path()).unwrap();
    assert_eq!(fresh.len(), 64);
    assert_ne!(fresh, token);
    let other = tempfile::tempdir().unwrap();
    assert_ne!(load_or_create_token(other.path()).unwrap(), fresh, "tokens are random");
}

#[test]
fn host_origin_and_comparison_helpers() {
    assert!(host_allowed(Some("127.0.0.1:4747"), 4747));
    assert!(!host_allowed(Some("127.0.0.1:4747"), 4748));
    assert!(!host_allowed(None, 4747));
    assert!(origin_allowed(None, 4747));
    assert!(!origin_allowed(Some("http://127.0.0.1:4747.evil.example"), 4747));
    assert!(constant_time_eq("abc", "abc"));
    assert!(!constant_time_eq("abc", "abcd"));
    assert!(!constant_time_eq("", "x"));
}
