//! The one place this client speaks HTTP to the daemon: the web UI's own `/api/attachments`, `/api/files` and
//! `/api/plugins/*` (what the web composer uses to keep a file, to find a file to mention, and what its Plugins
//! page uses to list, enable and disable). Everything else goes over the socket (`write.rs`).
//!
//! The daemon's web server wants a token. It is the one `SignedWebUrl` answers with (`http://127.0.0.1:<port>/?token=…`),
//! sent as `Authorization: Bearer …`, which is how the daemon accepts a credential on anything but `GET /`. The token
//! is a secret: it is never printed, not even by `{:?}`. `tests/allow_list.rs` reads this file and fails when a path
//! appears here that the web's sources do not use.
//!
//! The client is a plain HTTP/1.1 request on a fresh loopback connection (`Connection: close`), so there is nothing
//! to keep alive and no dependency to carry. Calls block: they run on a thread, never in a frame.

use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::time::Duration;

/// The route `SignedWebUrl` is asked for when only the token is wanted.
pub const ROUTE: &str = "api";

/// The most a file may be (the web's `MAX_ATTACHMENT_BYTES`).
pub const MAX_ATTACHMENT_BYTES: usize = 32 * 1024 * 1024;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone, PartialEq, Eq)]
pub struct Http {
    port: u16,
    token: String,
}

// The token is a secret: never printed.
impl std::fmt::Debug for Http {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Http").field("port", &self.port).field("token", &"<hidden>").finish()
    }
}

/// Why a call did not work, in a sentence for the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HttpError(pub String);

impl std::fmt::Display for HttpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A file the daemon kept: the markdown reference that goes into the message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Kept {
    pub name: String,
    pub reference: String,
    pub is_image: bool,
}

/// A file the `@` popover can mention.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionFile {
    pub path: String,
    pub name: String,
    pub location: String,
}

/// One plugin as `/api/plugins` lists it (the web's `PluginListing`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PluginListing {
    pub id: String,
    pub title: String,
    pub entry: String,
    /// The entry file's mtime: changes when the module does.
    pub version: u64,
    /// The manifest's own "version" (for people).
    pub manifest_version: String,
    /// A `.disabled` marker file is in the plugin's folder: listed, never loaded.
    pub disabled: bool,
    /// Absolute path of the plugin's folder.
    pub path: String,
    pub description: String,
    /// Why the daemon could not read the manifest or entry.
    pub error: String,
    /// The viewers its manifest declares: (id, title, kinds).
    pub viewers: Vec<(String, String, Vec<String>)>,
    pub fences: Vec<String>,
}

/// Where the person's customisations live (`/api/customisation`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Customisation {
    pub plugins_dir: String,
    pub css_path: String,
    pub css_exists: bool,
}

struct Reply {
    status: u16,
    body: Vec<u8>,
}

impl Http {
    /// From the address `SignedWebUrl` answered with: the port and the token. None when it has neither.
    pub fn from_signed_url(url: &str) -> Option<Http> {
        let rest = url.strip_prefix("http://127.0.0.1:").or_else(|| url.strip_prefix("http://localhost:"))?;
        let (port, rest) = rest.split_once(['/', '?'])?;
        let port: u16 = port.parse().ok()?;
        let query = rest.trim_start_matches('/').strip_prefix('?').unwrap_or(rest);
        let token = query.split(['&', '#']).find_map(|pair| pair.strip_prefix("token="))?;
        (token.len() == 64 && token.chars().all(|c| c.is_ascii_hexdigit())).then(|| Http { port, token: token.to_owned() })
    }

    /// A client from known parts (tests).
    pub fn new(port: u16, token: &str) -> Http {
        Http { port, token: token.to_owned() }
    }

    fn send(&self, method: &str, target: &str, headers: &[(&str, String)], body: &[u8]) -> Result<Reply, HttpError> {
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let mut stream = TcpStream::connect_timeout(&addr, TIMEOUT).map_err(|_| HttpError("Neko's web address isn't answering.".into()))?;
        let _ = stream.set_read_timeout(Some(TIMEOUT));
        let _ = stream.set_write_timeout(Some(TIMEOUT));
        let mut head = format!("{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nConnection: close\r\nContent-Length: {}\r\n", self.port, self.token, body.len());
        for (name, value) in headers {
            head.push_str(&format!("{name}: {value}\r\n"));
        }
        head.push_str("\r\n");
        stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(body)).map_err(|_| HttpError("Couldn’t send that to Neko.".into()))?;
        let mut raw = Vec::new();
        stream.read_to_end(&mut raw).map_err(|_| HttpError("Neko didn’t answer in time.".into()))?;
        parse(&raw)
    }

    /// `POST /api/attachments`: keeps a file under the daemon's attachments folder and answers with the reference.
    /// Exactly the web's request: the bytes as `application/octet-stream`, the name percent-encoded in `x-neko-filename`.
    pub fn upload(&self, name: &str, bytes: &[u8]) -> Result<Kept, HttpError> {
        if bytes.is_empty() {
            return Err(HttpError(format!("{name} is empty.")));
        }
        if bytes.len() > MAX_ATTACHMENT_BYTES {
            return Err(HttpError("Choose a file under 32 MB.".into()));
        }
        let reply = self.send("POST", "/api/attachments", &[("Content-Type", "application/octet-stream".into()), ("x-neko-filename", percent_encode(name))], bytes)?;
        if reply.status != 200 {
            return Err(HttpError(format!("The daemon refused the file ({}).", reply.status)));
        }
        let json: serde_json::Value = serde_json::from_slice(&reply.body).map_err(|_| HttpError("The daemon's answer wasn’t readable.".into()))?;
        let reference = json["reference"].as_str().ok_or_else(|| HttpError("The daemon's answer wasn’t readable.".into()))?.to_owned();
        Ok(Kept { name: json["name"].as_str().unwrap_or(name).to_owned(), reference, is_image: json["is_image"].as_bool().unwrap_or(false) })
    }

    /// `GET /api/files?q=…&workspace=…`: up to five files in the workspace's folders whose names start with `q`.
    pub fn files(&self, query: &str, workspace: &str) -> Result<Vec<MentionFile>, HttpError> {
        let target = format!("/api/files?q={}&workspace={}", percent_encode(query), percent_encode(workspace));
        let reply = self.send("GET", &target, &[], &[])?;
        if reply.status != 200 {
            return Err(HttpError(format!("Couldn’t look for files ({}).", reply.status)));
        }
        let json: serde_json::Value = serde_json::from_slice(&reply.body).map_err(|_| HttpError("The daemon's answer wasn’t readable.".into()))?;
        Ok(json["files"]
            .as_array()
            .map(|files| {
                files
                    .iter()
                    .filter_map(|f| {
                        Some(MentionFile { path: f["path"].as_str()?.to_owned(), name: f["name"].as_str().unwrap_or_default().to_owned(), location: f["location"].as_str().unwrap_or_default().to_owned() })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// `GET /api/plugins`: the plugins the daemon found, with everything the web's Customisations page shows of each.
    pub fn plugins(&self) -> Result<Vec<PluginListing>, HttpError> {
        let reply = self.send("GET", "/api/plugins", &[], &[])?;
        if reply.status != 200 {
            return Err(HttpError(format!("Couldn’t list the plugins ({}).", reply.status)));
        }
        let json: serde_json::Value = serde_json::from_slice(&reply.body).map_err(|_| HttpError("The daemon's answer wasn’t readable.".into()))?;
        let list = json.as_array().or_else(|| json["plugins"].as_array());
        let text = |v: &serde_json::Value, k: &str| v[k].as_str().unwrap_or_default().to_owned();
        Ok(list
            .map(|plugins| {
                plugins
                    .iter()
                    .filter_map(|p| {
                        let id = p["id"].as_str().or_else(|| p["name"].as_str())?.to_owned();
                        Some(PluginListing {
                            title: p["title"].as_str().or_else(|| p["name"].as_str()).unwrap_or_default().to_owned(),
                            entry: text(p, "entry"),
                            version: p["version"].as_u64().unwrap_or(0),
                            manifest_version: text(p, "manifest_version"),
                            disabled: p["disabled"].as_bool().unwrap_or(false) || p["enabled"].as_bool() == Some(false),
                            path: text(p, "path"),
                            description: text(p, "description"),
                            error: text(p, "error"),
                            viewers: p["viewers"]
                                .as_array()
                                .map(|v| v.iter().map(|v| (text(v, "id"), text(v, "title"), v["kinds"].as_array().map(|k| k.iter().filter_map(|k| k.as_str().map(str::to_owned)).collect()).unwrap_or_default())).collect())
                                .unwrap_or_default(),
                            fences: p["fences"].as_array().map(|f| f.iter().filter_map(|f| f.as_str().map(str::to_owned)).collect()).unwrap_or_default(),
                            id,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }

    /// `GET /api/customisation`: the plugins folder and `custom.css`.
    pub fn customisation(&self) -> Result<Customisation, HttpError> {
        let reply = self.send("GET", "/api/customisation", &[], &[])?;
        if reply.status != 200 {
            return Err(HttpError(format!("Couldn’t read where the customisations live ({}).", reply.status)));
        }
        let json: serde_json::Value = serde_json::from_slice(&reply.body).map_err(|_| HttpError("The daemon's answer wasn’t readable.".into()))?;
        Ok(Customisation { plugins_dir: json["plugins_dir"].as_str().unwrap_or_default().to_owned(), css_path: json["css_path"].as_str().unwrap_or_default().to_owned(), css_exists: json["css_exists"].as_bool().unwrap_or(false) })
    }

    /// `DELETE /api/plugins/<id>`: deletes the plugin's folder. Only sent after a confirmation.
    pub fn remove_plugin(&self, id: &str) -> Result<(), HttpError> {
        let reply = self.send("DELETE", &format!("/api/plugins/{}", percent_encode(id)), &[], &[])?;
        match reply.status {
            200 | 204 => Ok(()),
            404 => Err(HttpError("That plugin is no longer installed.".into())),
            _ => Err(HttpError(format!("The daemon answered {}.", reply.status))),
        }
    }

    /// `GET /plugins/<id>/manifest.json`: the manifest as written, as the web's details panel reads it.
    pub fn manifest(&self, id: &str) -> Result<String, HttpError> {
        let reply = self.send("GET", &format!("/plugins/{}/manifest.json", percent_encode(id)), &[], &[])?;
        if reply.status != 200 {
            return Err(HttpError(format!("Could not read manifest.json: HTTP {}", reply.status)));
        }
        let text = String::from_utf8_lossy(&reply.body).into_owned();
        Ok(serde_json::from_str::<serde_json::Value>(&text).ok().and_then(|v| serde_json::to_string_pretty(&v).ok()).unwrap_or(text))
    }

    /// `GET /artifact/<id>/<n>?download=1`: one version of an artifact, as the web's Download link fetches it.
    pub fn artifact_bytes(&self, id: &str, version: u32) -> Result<Vec<u8>, HttpError> {
        let reply = self.send("GET", &format!("/artifact/{}/{version}?download=1", percent_encode(id)), &[], &[])?;
        if reply.status != 200 {
            return Err(HttpError(format!("The daemon refused the download ({}).", reply.status)));
        }
        Ok(reply.body)
    }

    /// `POST /api/plugins/<id>/enable` or `/disable` (the web's Plugins page switch).
    pub fn set_plugin_enabled(&self, id: &str, enabled: bool) -> Result<(), HttpError> {
        let target = format!("/api/plugins/{}/{}", percent_encode(id), if enabled { "enable" } else { "disable" });
        let reply = self.send("POST", &target, &[], &[])?;
        if reply.status == 200 { Ok(()) } else { Err(HttpError(format!("The daemon refused ({}).", reply.status))) }
    }
}

/// `encodeURIComponent`: everything but letters, digits and `-_.!~*'()` becomes `%XX`.
pub fn percent_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'!' | b'~' | b'*' | b'\'' | b'(' | b')') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The status and body of a whole HTTP/1.1 answer read to the end.
fn parse(raw: &[u8]) -> Result<Reply, HttpError> {
    let split = raw.windows(4).position(|w| w == b"\r\n\r\n").ok_or_else(|| HttpError("Neko's answer wasn’t readable.".into()))?;
    let head = String::from_utf8_lossy(&raw[..split]);
    let status = head.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).ok_or_else(|| HttpError("Neko's answer wasn’t readable.".into()))?;
    let mut body = raw[split + 4..].to_vec();
    if head.lines().any(|l| l.to_ascii_lowercase().starts_with("transfer-encoding:") && l.to_ascii_lowercase().contains("chunked")) {
        body = unchunk(&body);
    }
    Ok(Reply { status, body })
}

fn unchunk(mut rest: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    while let Some(line_end) = rest.windows(2).position(|w| w == b"\r\n") {
        let size = usize::from_str_radix(String::from_utf8_lossy(&rest[..line_end]).split(';').next().unwrap_or("").trim(), 16).unwrap_or(0);
        rest = &rest[line_end + 2..];
        if size == 0 || rest.len() < size {
            break;
        }
        out.extend_from_slice(&rest[..size]);
        rest = rest.get(size + 2..).unwrap_or(&[]);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader};
    use std::net::TcpListener;

    const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn the_port_and_token_come_from_the_signed_address() {
        let http = Http::from_signed_url(&format!("http://127.0.0.1:4747/?token={TOKEN}#/api")).unwrap();
        assert_eq!(http, Http::new(4747, TOKEN));
        assert!(Http::from_signed_url("http://127.0.0.1:4747/").is_none());
        assert!(Http::from_signed_url("https://example.com/?token=x").is_none());
        assert!(!format!("{http:?}").contains(TOKEN), "the token is never printed");
    }

    #[test]
    fn names_are_encoded_as_the_web_does() {
        assert_eq!(percent_encode("my file (1).png"), "my%20file%20(1).png");
        assert_eq!(percent_encode("é"), "%C3%A9");
    }

    /// A one-request server: answers `status`/`body` and hands back the request it read.
    fn serve(status: &str, body: &'static str) -> (u16, std::thread::JoinHandle<(String, Vec<u8>)>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let status = status.to_owned();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut head = String::new();
            let mut length = 0usize;
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" || line.is_empty() {
                    break;
                }
                if let Some(v) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                    length = v.trim().parse().unwrap();
                }
                head.push_str(&line);
            }
            let mut got = vec![0u8; length];
            reader.read_exact(&mut got).unwrap();
            let mut stream = stream;
            write!(stream, "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).unwrap();
            (head, got)
        });
        (port, handle)
    }

    #[test]
    fn an_upload_is_the_webs_request_and_reads_the_reference() {
        let (port, server) = serve("200 OK", r#"{"name":"a b.txt","reference":"[a b.txt](file:///x/a%20b.txt)","is_image":false}"#);
        let kept = Http::new(port, TOKEN).upload("a b.txt", b"hello").unwrap();
        assert_eq!((kept.name.as_str(), kept.reference.as_str(), kept.is_image), ("a b.txt", "[a b.txt](file:///x/a%20b.txt)", false));
        let (head, body) = server.join().unwrap();
        assert!(head.starts_with("POST /api/attachments HTTP/1.1"), "{head}");
        assert!(head.contains(&format!("Authorization: Bearer {TOKEN}")) && head.contains("x-neko-filename: a%20b.txt") && head.contains("application/octet-stream"), "{head}");
        assert_eq!(body, b"hello");
    }

    #[test]
    fn files_are_asked_for_by_name_and_workspace() {
        let (port, server) = serve("200 OK", r#"{"files":[{"path":"/p/readme.md","name":"readme.md","location":"~/p"}]}"#);
        let files = Http::new(port, TOKEN).files("read", "w 1").unwrap();
        assert_eq!(files, vec![MentionFile { path: "/p/readme.md".into(), name: "readme.md".into(), location: "~/p".into() }]);
        let (head, _) = server.join().unwrap();
        assert!(head.starts_with("GET /api/files?q=read&workspace=w%201 HTTP/1.1"), "{head}");
    }

    #[test]
    fn a_refusal_is_one_calm_sentence_and_an_empty_or_huge_file_never_leaves() {
        let (port, server) = serve("401 Unauthorized", "no");
        let error = Http::new(port, TOKEN).upload("x.png", b"1").unwrap_err();
        assert!(error.0.contains("401") && !error.0.contains(TOKEN));
        server.join().unwrap();
        let http = Http::new(1, TOKEN);
        assert!(http.upload("x", b"").unwrap_err().0.contains("empty"));
        assert!(http.upload("x", &vec![0u8; MAX_ATTACHMENT_BYTES + 1]).unwrap_err().0.contains("32 MB"));
    }
}
