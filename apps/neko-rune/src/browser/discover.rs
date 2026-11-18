//! Finding the Chrome to watch (docs/knowledge/rune-live-browser.md). Order: the `NEKO_BROWSER_CDP` address when it is set, else
//! `http://127.0.0.1:9333` (the shared Chrome agents on this Mac use) and then `http://127.0.0.1:9222`; the first whose
//! `GET /json/version` answers wins and names the browser's websocket (`webSocketDebuggerUrl`).
//!
//! **Loopback only.** The browser is on this Mac or it is not watched: a host other than `127.0.0.1`, `::1` or `localhost` is
//! refused, in the address given and in the websocket address Chrome answers with. Addresses are parsed with the `url` crate,
//! never matched as strings. The HTTP request is written by hand over `std::net::TcpStream` (300 ms to connect and to read).

use std::io::{self, Read, Write};
use std::net::{Ipv4Addr, Ipv6Addr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use serde_json::Value;
use url::{Host, Url};

/// The environment variable that names the endpoint.
pub const ENV: &str = "NEKO_BROWSER_CDP";
/// The ports looked at when it is not set, in order.
pub const PORTS: [u16; 2] = [9333, 9222];
/// How long a connect or a read may take.
pub const TIMEOUT: Duration = Duration::from_millis(300);
/// No answer is read past this many bytes.
const MAX_BODY: usize = 1 << 20;

/// Why an address was not used.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Not an address, or not an `http`/`ws` one.
    BadAddress,
    /// A host other than this Mac's loopback.
    NotLoopback,
    /// The answer did not name a websocket.
    NoSocket,
}

/// A Chrome found: where it answered, and the browser's websocket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    pub base: String,
    pub ws_url: String,
}

/// Whether a parsed host is this Mac's loopback: `127.0.0.1`, `::1` or `localhost`.
fn is_loopback(host: &Host<&str>) -> bool {
    match host {
        Host::Domain(d) => d.eq_ignore_ascii_case("localhost"),
        Host::Ipv4(ip) => *ip == Ipv4Addr::LOCALHOST,
        Host::Ipv6(ip) => *ip == Ipv6Addr::LOCALHOST,
    }
}

/// `raw` as an address on this Mac's loopback, over `http` or `ws` only (no TLS is built).
pub fn loopback_url(raw: &str) -> Result<Url, Refused> {
    let url = Url::parse(raw.trim()).map_err(|_| Refused::BadAddress)?;
    if !matches!(url.scheme(), "http" | "ws") || !url.username().is_empty() || url.password().is_some() {
        return Err(Refused::BadAddress);
    }
    match url.host() {
        Some(host) if is_loopback(&host) => Ok(url),
        Some(_) => Err(Refused::NotLoopback),
        None => Err(Refused::BadAddress),
    }
}

/// The addresses to try, in order: the variable's when it is set and not empty, else the two ports.
pub fn probe_order(env: Option<&str>) -> Vec<String> {
    match env.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) => vec![v.to_owned()],
        None => PORTS.iter().map(|p| format!("http://127.0.0.1:{p}")).collect(),
    }
}

/// The websocket address in a `/json/version` answer, checked to be on the loopback too.
pub fn parse_version(body: &str) -> Result<String, Refused> {
    let value: Value = serde_json::from_str(body).map_err(|_| Refused::NoSocket)?;
    let ws = value.get("webSocketDebuggerUrl").and_then(Value::as_str).ok_or(Refused::NoSocket)?;
    let url = loopback_url(ws)?;
    if url.scheme() != "ws" {
        return Err(Refused::BadAddress);
    }
    Ok(url.to_string())
}

/// `GET path` over plain HTTP/1.1 from a loopback `base`; the body when the answer is a 200.
pub fn http_get(base: &Url, path: &str) -> io::Result<String> {
    let port = base.port_or_known_default().ok_or_else(|| io::Error::other("no port"))?;
    let host = base.host_str().ok_or_else(|| io::Error::other("no host"))?;
    let mut last = io::Error::other("no address");
    let addrs: Vec<_> = (host.trim_matches(['[', ']']), port).to_socket_addrs()?.collect();
    for addr in addrs {
        match TcpStream::connect_timeout(&addr, TIMEOUT) {
            Ok(mut stream) => {
                stream.set_read_timeout(Some(TIMEOUT))?;
                stream.set_write_timeout(Some(TIMEOUT))?;
                let host_header = match base.host() {
                    Some(Host::Ipv6(ip)) => format!("[{ip}]:{port}"),
                    _ => format!("{host}:{port}"),
                };
                // One write, so the server sees the whole request at once.
                stream.write_all(format!("GET {path} HTTP/1.1\r\nHost: {host_header}\r\nAccept: application/json\r\nConnection: close\r\n\r\n").as_bytes())?;
                let mut raw = Vec::new();
                let mut chunk = [0u8; 8192];
                loop {
                    match stream.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            raw.extend_from_slice(&chunk[..n]);
                            if raw.len() > MAX_BODY {
                                return Err(io::Error::other("answer too long"));
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::WouldBlock || e.kind() == io::ErrorKind::TimedOut => {
                            // Slow past the limit: what came is used if it is whole.
                            if split_response(&raw).is_some_and(|r| r.complete) {
                                break;
                            }
                            return Err(e);
                        }
                        Err(e) => return Err(e),
                    }
                }
                return split_response(&raw).filter(|r| r.status == 200).map(|r| r.body).ok_or_else(|| io::Error::other("not a 200 answer"));
            }
            Err(e) => last = e,
        }
    }
    Err(last)
}

/// An HTTP answer, taken apart.
#[derive(Debug, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub body: String,
    /// The body is all there (its length matches, or it is chunked and ended).
    pub complete: bool,
}

/// Splits a raw HTTP/1.1 answer into its status and body; understands `Content-Length` and `chunked`.
pub fn split_response(raw: &[u8]) -> Option<Response> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n")?;
    let head = std::str::from_utf8(&raw[..split]).ok()?;
    let body = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status: u16 = lines.next()?.split_whitespace().nth(1)?.parse().ok()?;
    let mut length = None;
    let mut chunked = false;
    for line in lines {
        let Some((name, value)) = line.split_once(':') else { continue };
        let (name, value) = (name.trim().to_ascii_lowercase(), value.trim());
        if name == "content-length" {
            length = value.parse::<usize>().ok();
        } else if name == "transfer-encoding" && value.to_ascii_lowercase().contains("chunked") {
            chunked = true;
        }
    }
    if chunked {
        let (bytes, done) = dechunk(body);
        return Some(Response { status, body: String::from_utf8(bytes).ok()?, complete: done });
    }
    let complete = length.is_none_or(|n| body.len() >= n);
    let body = match length {
        Some(n) if body.len() >= n => &body[..n],
        _ => body,
    };
    Some(Response { status, body: String::from_utf8(body.to_vec()).ok()?, complete })
}

/// The bytes of a chunked body, and whether its last (empty) chunk was seen.
fn dechunk(mut rest: &[u8]) -> (Vec<u8>, bool) {
    let mut out = Vec::new();
    loop {
        let Some(line_end) = rest.windows(2).position(|w| w == b"\r\n") else { return (out, false) };
        let size = std::str::from_utf8(&rest[..line_end]).ok().and_then(|s| usize::from_str_radix(s.split(';').next().unwrap_or("").trim(), 16).ok());
        let Some(size) = size else { return (out, false) };
        rest = &rest[line_end + 2..];
        if size == 0 {
            return (out, true);
        }
        if rest.len() < size {
            out.extend_from_slice(rest);
            return (out, false);
        }
        out.extend_from_slice(&rest[..size]);
        rest = rest.get(size + 2..).unwrap_or(&[]);
    }
}

/// Asks one address: its `/json/version` must answer with a loopback websocket.
pub fn probe(base: &str) -> Result<Endpoint, Refused> {
    let url = loopback_url(base)?;
    let body = http_get(&url, "/json/version").map_err(|_| Refused::NoSocket)?;
    let ws_url = parse_version(&body)?;
    Ok(Endpoint { base: base.to_owned(), ws_url })
}

/// The first address that answers.
pub fn discover(bases: &[String]) -> Option<Endpoint> {
    bases.iter().find_map(|b| probe(b).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn only_this_macs_loopback_is_used() {
        for ok in ["http://127.0.0.1:9333", "http://localhost:9222", "http://[::1]:9333", "ws://127.0.0.1:9333/devtools/browser/x", "http://LOCALHOST:1"] {
            assert!(loopback_url(ok).is_ok(), "{ok}");
        }
        for (bad, why) in [
            ("http://example.com:9333", Refused::NotLoopback),
            ("http://192.168.1.20:9333", Refused::NotLoopback),
            ("http://0.0.0.0:9333", Refused::NotLoopback),
            ("http://127.0.0.2:9333", Refused::NotLoopback),
            ("http://localhost.example.com:9333", Refused::NotLoopback),
            ("http://127.0.0.1.example.com", Refused::NotLoopback),
            ("http://127.0.0.1@example.com:9333", Refused::BadAddress),
            ("http://user:pw@127.0.0.1:9333", Refused::BadAddress),
            ("https://127.0.0.1:9333", Refused::BadAddress),
            ("wss://127.0.0.1:9333", Refused::BadAddress),
            ("file:///tmp/x", Refused::BadAddress),
            ("nonsense", Refused::BadAddress),
        ] {
            assert_eq!(loopback_url(bad), Err(why), "{bad}");
        }
    }

    #[test]
    fn the_variable_wins_else_the_two_ports_in_order() {
        assert_eq!(probe_order(None), ["http://127.0.0.1:9333", "http://127.0.0.1:9222"]);
        assert_eq!(probe_order(Some("")), probe_order(None));
        assert_eq!(probe_order(Some(" http://127.0.0.1:1234 ")), ["http://127.0.0.1:1234"]);
    }

    #[test]
    fn a_version_answer_gives_its_socket_and_a_remote_socket_is_refused() {
        let ok = r#"{"Browser":"Chrome/200","webSocketDebuggerUrl":"ws://127.0.0.1:9333/devtools/browser/abc-123"}"#;
        assert_eq!(parse_version(ok).as_deref(), Ok("ws://127.0.0.1:9333/devtools/browser/abc-123"));
        let remote = r#"{"webSocketDebuggerUrl":"ws://203.0.113.9:9333/devtools/browser/abc"}"#;
        assert_eq!(parse_version(remote), Err(Refused::NotLoopback));
        let tls = r#"{"webSocketDebuggerUrl":"wss://127.0.0.1:9333/devtools/browser/abc"}"#;
        assert_eq!(parse_version(tls), Err(Refused::BadAddress));
        assert_eq!(parse_version(r#"{"Browser":"x"}"#), Err(Refused::NoSocket));
        assert_eq!(parse_version("not json"), Err(Refused::NoSocket));
    }

    #[test]
    fn http_answers_are_split_by_length_or_by_chunks() {
        let plain = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nContent-Type: application/json\r\n\r\nhello-and-more";
        assert_eq!(split_response(plain), Some(Response { status: 200, body: "hello".into(), complete: true }));
        let short = b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nhello";
        assert_eq!(split_response(short).map(|r| r.complete), Some(false));
        let chunked = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6;x=1\r\n world\r\n0\r\n\r\n";
        assert_eq!(split_response(chunked), Some(Response { status: 200, body: "hello world".into(), complete: true }));
        assert_eq!(split_response(b"HTTP/1.1 404 Not Found\r\n\r\n").map(|r| r.status), Some(404));
        assert_eq!(split_response(b"garbage"), None);
    }

    /// A one-shot HTTP server on a loopback port answering `answer`.
    fn serve(answer: String) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            if let Ok((mut s, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = s.read(&mut buf);
                let _ = s.write_all(answer.as_bytes());
            }
        });
        port
    }

    #[test]
    fn probing_reads_json_version_from_a_loopback_server() {
        let body = r#"{"webSocketDebuggerUrl":"ws://127.0.0.1:1/devtools/browser/zz"}"#;
        let port = serve(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
        let found = probe(&format!("http://127.0.0.1:{port}")).unwrap();
        assert_eq!(found.ws_url, "ws://127.0.0.1:1/devtools/browser/zz");
        // Nothing listens: no endpoint, and the second address is tried.
        let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
        let port2 = serve(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{body}", body.len()));
        let bases = vec![format!("http://127.0.0.1:{dead}"), format!("http://127.0.0.1:{port2}")];
        assert_eq!(discover(&bases).map(|e| e.base), Some(bases[1].clone()));
        assert_eq!(discover(&["http://example.com:9333".to_owned()]), None, "a remote address is never contacted");
    }
}
