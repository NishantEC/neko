//! The Canvas list's row actions (`web/src/canvas/ArtifactCard.tsx`'s `artifactMenu`): Download (the version is saved to the
//! Downloads folder and a toast says so), Open in browser (the daemon signs the web UI's address at the artifact's route, so
//! the page loads signed in), Copy link (the web UI's address, without the token, as a task's link is). And the bridge from the
//! embedded web panel (`web/src/canvas/ArtifactEmbed.tsx`): a comment's quote and an artifact's `data-neko-send` arrive as
//! messages from the page, accepted only from the signed daemon origin (`web_panel::decide`) and read by [`parse`].

use crate::app::App;
use crate::compose::HttpWork;
use crate::link::Call;

/// The longest text one click may offer (`MAX_SEND` in `web/src/canvas/artifacts.ts`).
pub const MAX_SEND: usize = 500;
/// The least time between two messages the page sends: a page that floods is ignored.
pub const MIN_GAP_MS: i64 = 1000;

/// What the embedded page says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Said {
    /// A comment on an element of the artifact: goes into the session's composer as a quotation.
    Quote(String),
    /// The text of a `data-neko-send` element: drafted into the composer for the person to send.
    Send(String),
}

/// The message a page posted through `window.ipc.postMessage`, or why not. `{"type":"quote"|"send","text":"…"}`, the text trimmed and
/// non-empty, at most [`MAX_SEND`] characters for a send (the web's `offerFromArtifact` refuses longer) and 2000 for a quote.
pub fn parse(body: &str) -> Option<Said> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let text = v.get("text")?.as_str()?.trim();
    if text.is_empty() {
        return None;
    }
    match v.get("type")?.as_str()? {
        "quote" if text.chars().count() <= 2000 => Some(Said::Quote(text.to_owned())),
        "send" if text.chars().count() <= MAX_SEND => Some(Said::Send(text.to_owned())),
        _ => None,
    }
}

/// Whether a message from the page at `address` is taken: only the web UI's own page on the signed daemon origin
/// (`http://127.0.0.1:<port>/`), never another site and never a page the artifact itself draws (`/artifact/<id>/<n>`, a frame the
/// agent wrote), which could otherwise speak as the person.
pub fn origin_ok(address: &str, daemon_port: u16) -> bool {
    use crate::web_panel::{Nav, decide};
    if decide(address, daemon_port) != Nav::Allow || address.starts_with("about:") {
        return false;
    }
    url::Url::parse(address).is_ok_and(|u| u.path() == "/")
}

/// The message a send becomes: the text, then the line that says which artifact drafted it (the web's `fromArtifactLine`).
pub fn from_artifact(text: &str, id: &str, version: u32, title: &str) -> String {
    let title: String = title.chars().map(|c| if matches!(c, '"' | '\n' | '\r' | ']') { '\'' } else { c }).take(80).collect();
    format!("{text}\n[from artifact {id} v{version}: \"{title}\"]")
}

/// A file name for a download: the artifact's title and the version file's extension.
pub fn download_name(title: &str, file: &str) -> String {
    let ext = file.rsplit_once('.').map(|(_, e)| e).filter(|e| !e.is_empty() && e.len() <= 12 && e.chars().all(|c| c.is_ascii_alphanumeric())).unwrap_or("txt");
    let stem: String = title.chars().map(|c| if c.is_control() || matches!(c, '/' | '\\' | ':') { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let stem = stem.trim_matches('.').trim();
    format!("{}.{ext}", if stem.is_empty() { "artifact" } else { stem })
}

impl App {
    /// The version a Canvas row stands for: the one the session shows, else the newest.
    fn canvas_version(&self, id: &str) -> Option<u32> {
        let chosen = self.selected.as_ref().and_then(|t| self.panel.sessions.get(t)).and_then(|sp| sp.versions.get(id).copied());
        chosen.or_else(|| self.panel.artifacts.get(id).and_then(|a| a.versions.iter().map(|v| v.n).max()))
    }

    /// "Download": the version is fetched over HTTP and saved under its own name in Downloads; a toast says where.
    pub fn artifact_download(&mut self, id: &str) {
        let Some(version) = self.canvas_version(id) else {
            self.show_toast("Still reading that artifact. Try again in a moment.");
            return;
        };
        let record = self.panel.artifacts.get(id);
        let file = record.and_then(|a| a.versions.iter().find(|v| v.n == version)).map(|v| v.file.clone()).unwrap_or_default();
        let name = download_name(record.map_or(id, |a| a.title.as_str()), &file);
        self.http_work(HttpWork::Download { id: id.to_owned(), version, name });
    }

    /// "Open in browser": the daemon signs the web UI's address at this artifact; it opens when the signed address arrives.
    pub fn artifact_open_in_browser(&mut self, id: &str) {
        let Some(version) = self.canvas_version(id) else {
            self.show_toast("Still reading that artifact. Try again in a moment.");
            return;
        };
        let route = format!("artifact/{id}/{version}");
        self.extra.browser_routes.push(route.clone());
        self.call(Call::SignedWebUrl { route });
    }

    /// The signed address of a route asked for the browser arrived: open it. True when it was one of those.
    pub fn browser_route_signed(&mut self, route: &str, url: &str) -> bool {
        let Some(at) = self.extra.browser_routes.iter().position(|r| r == route) else { return false };
        self.extra.browser_routes.remove(at);
        if (self.opener)(url).is_err() {
            self.show_toast("Couldn’t open your browser.");
        }
        true
    }

    /// The web UI's address of the artifact, without the token (what Copy link puts on the clipboard).
    pub fn artifact_link(&self, id: &str) -> Option<String> {
        let version = self.canvas_version(id);
        self.web.as_ref().map(|w| format!("http://{}/#/artifact?id={id}{}", w.display(), version.map(|v| format!("&v={v}")).unwrap_or_default()))
    }

    /// A message from the embedded page, already checked for its origin. Comments quote into the open session's composer; a send is drafted there too, never posted.
    pub fn page_said(&mut self, said: Said) {
        let now = (self.clock)();
        if now - self.extra.last_said_ms < MIN_GAP_MS && self.extra.last_said_ms != 0 {
            return;
        }
        self.extra.last_said_ms = now;
        match said {
            Said::Quote(text) => {
                let quoted: String = text.lines().map(|l| format!("> {l}")).collect::<Vec<_>>().join("\n");
                let current = self.composer.text.clone();
                let joined = if current.trim().is_empty() { format!("{quoted}\n\n") } else { format!("{}\n\n{quoted}\n\n", current.trim_end()) };
                self.composer.replace_text(joined);
                self.composer.focused = true;
            }
            Said::Send(text) => {
                let (id, version, title) = match self.panel_target() {
                    Some(crate::panel::Target::Artifact { id, version }) => {
                        let title = self.panel.artifacts.get(&id).map(|a| a.title.clone()).unwrap_or_default();
                        (id, version, title)
                    }
                    _ => return,
                };
                // Drafted, never posted: like the web, an artifact's offer goes into the composer and the person sends it.
                // An agent-made page must not be able to speak as the person.
                let message = from_artifact(&text, &id, version, &title);
                let current = self.composer.text.clone();
                let joined = if current.trim().is_empty() { message } else { format!("{}\n\n{message}", current.trim_end()) };
                self.composer.replace_text(joined);
                self.composer.focused = true;
                self.show_toast("Added to your message. Send it when ready.");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_a_type_and_a_trimmed_text_and_nothing_else_is_read() {
        assert_eq!(parse(r#"{"type":"send","text":"  Make the heading blue "}"#), Some(Said::Send("Make the heading blue".into())));
        assert_eq!(parse(r#"{"type":"quote","text":"[artifact a v1, element h1: \"x\"]"}"#), Some(Said::Quote("[artifact a v1, element h1: \"x\"]".into())));
        for bad in ["", "gesture", "{}", r#"{"type":"send"}"#, r#"{"type":"send","text":"   "}"#, r#"{"type":"send","text":5}"#, r#"{"type":"delete","text":"x"}"#, r#"["send","x"]"#, r#"{"type":"send","text":"#] {
            assert_eq!(parse(bad), None, "{bad}");
        }
        assert_eq!(parse(&format!(r#"{{"type":"send","text":"{}"}}"#, "x".repeat(MAX_SEND + 1))), None, "the web refuses a send over 500 characters");
        assert!(parse(&format!(r#"{{"type":"send","text":"{}"}}"#, "x".repeat(MAX_SEND))).is_some());
    }

    #[test]
    fn only_the_signed_daemon_pages_own_document_may_speak() {
        let port = 4747;
        assert!(origin_ok("http://127.0.0.1:4747/", port));
        assert!(origin_ok("http://127.0.0.1:4747/#/artifact/a/1?embed=1", port));
        assert!(!origin_ok("http://127.0.0.1:4747/artifact/a/1", port), "the frame an agent wrote is not the app");
        assert!(!origin_ok("http://127.0.0.1:4747/plugins/x/index.js", port));
        assert!(!origin_ok("http://127.0.0.1:4748/", port), "another port");
        assert!(!origin_ok("http://localhost:4747/", port), "another host");
        assert!(!origin_ok("https://127.0.0.1:4747/", port), "another scheme");
        assert!(!origin_ok("https://example.com/", port));
        assert!(!origin_ok("http://evil@127.0.0.1:4747/", port), "a user name");
        assert!(!origin_ok("about:blank", port) && !origin_ok("about:srcdoc", port));
        assert!(!origin_ok("file:///etc/passwd", port) && !origin_ok("javascript:alert(1)", port) && !origin_ok("", port));
    }

    #[test]
    fn a_send_ends_with_the_line_that_names_its_artifact() {
        assert_eq!(from_artifact("Make it blue", "abc", 2, "My \"page\"\n]"), "Make it blue\n[from artifact abc v2: \"My 'page'''\"]");
    }

    #[test]
    fn a_download_is_named_for_the_artifact_with_its_files_extension() {
        assert_eq!(download_name("Board: v2 / draft", "v2.excalidraw"), "Board v2 draft.excalidraw");
        assert_eq!(download_name("  ", "v1.html"), "artifact.html");
        assert_eq!(download_name("Notes", "v1"), "Notes.txt");
        assert_eq!(download_name("..", "v1.csv"), "artifact.csv");
    }

    #[test]
    fn a_page_that_floods_is_heard_once_a_second() {
        let mut app = App::headless();
        app.page_said(Said::Quote("one".into()));
        app.page_said(Said::Quote("two".into()));
        assert!(app.composer.text.contains("> one") && !app.composer.text.contains("two"));
        assert!(app.composer.text.ends_with("\n\n"));
    }
}

#[cfg(test)]
mod draft_only {
    /// An artifact's send offer is drafted, never posted: no call may be made from it.
    #[test]
    fn send_offers_never_call_the_daemon() {
        let src = include_str!("artifact_actions.rs");
        let body = &src[src.find("Said::Send(text) => {").unwrap()..src.find("#[cfg(test)]").unwrap()];
        for call in ["Call::Reply", "Call::Enqueue", "Call::Ask"] {
            assert!(!body.contains(call), "a send offer must only draft, found {call}");
        }
    }
}
