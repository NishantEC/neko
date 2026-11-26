//! The first bytes of a connection: one HTTP/1.1 request head, checked before anything else happens.
//!
//! A request is served only when it names this daemon as its host (`127.0.0.1:<port>` or `localhost:<port>`, against DNS
//! rebinding), comes from no web page (no `Origin` header), and carries the token (`Authorization: Bearer …`). Then it is either the
//! plain hello (`GET /v1/hello`) or the WebSocket upgrade (`GET /v1/ws`).
//!
//! One more: a run's hook helper (`POST /v1/hook`) carries that run's own secret instead of the token; the run checks it.

use std::io::{self, Read, Write};
use std::net::TcpStream;

use neko_protocol::{HELLO_PATH, PROTOCOL_VERSION, WS_PATH};

use crate::files::same_token;

/// The longest request head read.
const MAX_HEAD: usize = 16 * 1024;

/// Where a run's hook helper asks (docs/knowledge/permissions.md).
pub const HOOK_PATH: &str = "/v1/hook";

/// The longest hook body read (a tool's input, as Claude Code sends it).
pub const MAX_HOOK_BODY: usize = 1024 * 1024;

/// What the head asked for, once it passed the checks.
#[derive(Debug, PartialEq, Eq)]
pub enum Asked {
    Hello,
    /// The WebSocket upgrade, with the client's `Sec-WebSocket-Key`.
    Upgrade(String),
    /// A hook helper's question: the secret it carries and the body's length. The run checks the secret.
    Hook { secret: String, length: usize },
}

/// Why a head was refused, as the status sent back.
#[derive(Debug, PartialEq, Eq)]
pub enum Refused {
    Bad,
    Forbidden,
    Unauthorized,
    NotFound,
}

/// Reads one request head (the bytes up to the blank line).
pub fn read_head(stream: &mut TcpStream) -> io::Result<Vec<u8>> {
    let mut head = Vec::with_capacity(1024);
    let mut byte = [0u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_HEAD {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "request head too long"));
        }
        // One byte at a time: nothing past the head is taken from the socket (the WebSocket reads the rest).
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "closed before the request head ended"));
        }
        head.push(byte[0]);
    }
    Ok(head)
}

/// Checks a request head against this daemon's `port` and `token`.
pub fn check(head: &[u8], port: u16, token: &str) -> Result<Asked, Refused> {
    let mut headers = [httparse::EMPTY_HEADER; 32];
    let mut req = httparse::Request::new(&mut headers);
    if !matches!(req.parse(head), Ok(httparse::Status::Complete(_))) {
        return Err(Refused::Bad);
    }
    let header = |name: &str| req.headers.iter().find(|h| h.name.eq_ignore_ascii_case(name)).and_then(|h| std::str::from_utf8(h.value).ok()).map(str::trim);
    let host_ok = header("host").is_some_and(|h| h == format!("127.0.0.1:{port}") || h == format!("localhost:{port}"));
    if !host_ok || header("origin").is_some() {
        return Err(Refused::Forbidden);
    }
    let bearer = header("authorization").and_then(|v| v.strip_prefix("Bearer ")).map(str::trim);
    if req.path == Some(HOOK_PATH) {
        if req.method != Some("POST") {
            return Err(Refused::NotFound);
        }
        let Some(secret) = bearer.filter(|b| !b.is_empty()) else { return Err(Refused::Unauthorized) };
        let length = header("content-length").and_then(|l| l.parse::<usize>().ok()).filter(|l| *l <= MAX_HOOK_BODY).ok_or(Refused::Bad)?;
        return Ok(Asked::Hook { secret: secret.to_owned(), length });
    }
    if !bearer.is_some_and(|b| same_token(b, token)) {
        return Err(Refused::Unauthorized);
    }
    if req.method != Some("GET") {
        return Err(Refused::NotFound);
    }
    match req.path {
        Some(HELLO_PATH) => Ok(Asked::Hello),
        Some(WS_PATH) => {
            let upgrade = header("upgrade").is_some_and(|u| u.eq_ignore_ascii_case("websocket"));
            match header("sec-websocket-key") {
                Some(key) if upgrade && !key.is_empty() => Ok(Asked::Upgrade(key.to_owned())),
                _ => Err(Refused::Bad),
            }
        }
        _ => Err(Refused::NotFound),
    }
}

pub fn respond(stream: &mut TcpStream, status: &str, content_type: &str, body: &str) -> io::Result<()> {
    write!(stream, "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n{body}", body.len())?;
    stream.flush()
}

pub fn refuse(stream: &mut TcpStream, why: &Refused) -> io::Result<()> {
    let status = match why {
        Refused::Bad => "400 Bad Request",
        Refused::Forbidden => "403 Forbidden",
        Refused::Unauthorized => "401 Unauthorized",
        Refused::NotFound => "404 Not Found",
    };
    respond(stream, status, "text/plain; charset=utf-8", status)
}

pub fn hello(stream: &mut TcpStream) -> io::Result<()> {
    let body = serde_json::json!({ "daemon": env!("CARGO_PKG_VERSION"), "protocol": PROTOCOL_VERSION }).to_string();
    respond(stream, "200 OK", "application/json", &body)
}

pub fn accept_upgrade(stream: &mut TcpStream, key: &str) -> io::Result<()> {
    let accept = tungstenite::handshake::derive_accept_key(key.as_bytes());
    write!(stream, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n")?;
    stream.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    fn head(lines: &[&str]) -> Vec<u8> {
        format!("{}\r\n\r\n", lines.join("\r\n")).into_bytes()
    }

    #[test]
    fn a_request_needs_the_host_no_origin_and_the_token() {
        let ok = ["GET /v1/hello HTTP/1.1", "Host: 127.0.0.1:4000", "Authorization: Bearer 0123456789abcdef0123456789abcdef"];
        assert_eq!(check(&head(&ok), 4000, TOKEN), Ok(Asked::Hello));
        assert_eq!(check(&head(&["GET /v1/hello HTTP/1.1", "Host: localhost:4000", ok[2]]), 4000, TOKEN), Ok(Asked::Hello));
        assert_eq!(check(&head(&ok[..2]), 4000, TOKEN), Err(Refused::Unauthorized), "no token");
        assert_eq!(check(&head(&[ok[0], ok[1], "Authorization: Bearer wrong"]), 4000, TOKEN), Err(Refused::Unauthorized));
        assert_eq!(check(&head(&[ok[0], "Host: evil.example.com:4000", ok[2]]), 4000, TOKEN), Err(Refused::Forbidden), "DNS rebinding");
        assert_eq!(check(&head(&[ok[0], ok[1], ok[2], "Origin: https://example.com"]), 4000, TOKEN), Err(Refused::Forbidden), "a web page");
        assert_eq!(check(&head(&["GET /other HTTP/1.1", ok[1], ok[2]]), 4000, TOKEN), Err(Refused::NotFound));
        assert_eq!(check(b"nonsense\r\n\r\n", 4000, TOKEN), Err(Refused::Bad));
    }

    #[test]
    fn a_hook_posts_with_its_runs_secret_and_a_length() {
        let base = ["POST /v1/hook HTTP/1.1", "Host: 127.0.0.1:4000", "Authorization: Bearer s3cret", "Content-Length: 12"];
        assert_eq!(check(&head(&base), 4000, TOKEN), Ok(Asked::Hook { secret: "s3cret".into(), length: 12 }));
        assert_eq!(check(&head(&base[..3]), 4000, TOKEN), Err(Refused::Bad), "no length");
        assert_eq!(check(&head(&[base[0], base[1], base[3]]), 4000, TOKEN), Err(Refused::Unauthorized), "no secret");
        assert_eq!(check(&head(&[base[0], "Host: evil.example.com:4000", base[2], base[3]]), 4000, TOKEN), Err(Refused::Forbidden));
        assert_eq!(check(&head(&[base[0], base[1], base[2], base[3], "Origin: https://example.com"]), 4000, TOKEN), Err(Refused::Forbidden), "a web page");
        assert_eq!(check(&head(&["GET /v1/hook HTTP/1.1", base[1], base[2]]), 4000, TOKEN), Err(Refused::NotFound));
    }

    #[test]
    fn the_upgrade_needs_its_key() {
        let base = ["GET /v1/ws HTTP/1.1", "Host: 127.0.0.1:4000", "Authorization: Bearer 0123456789abcdef0123456789abcdef", "Upgrade: websocket"];
        assert_eq!(check(&head(&base), 4000, TOKEN), Err(Refused::Bad));
        let mut with_key = base.to_vec();
        with_key.push("Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==");
        assert_eq!(check(&head(&with_key), 4000, TOKEN), Ok(Asked::Upgrade("dGhlIHNhbXBsZSBub25jZQ==".into())));
    }
}
