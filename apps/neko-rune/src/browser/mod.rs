//! The live Browser tab of the context panel (docs/knowledge/rune-live-browser.md): the person WATCHES a page of a Chrome on
//! this Mac while an agent drives it. Nothing here controls, creates, closes or reads anything else.
//!
//! * [`discover`] finds the browser (loopback only); [`cdp`] is one thread with one browser-level websocket speaking a seven
//!   method allow-list ([`cdp::CDP_METHODS`]); this file is the model they share ([`BrowserView`]).
//! * **This talks to Chrome, not to the daemon.** Rune's request allow-list for the daemon (`link/`, `tests/allow_list.rs`) is
//!   unaffected: no `Call` and no daemon request is added, and no frame, title or address is ever sent to the daemon, an agent or a
//!   log. Frames live in memory only and are dropped the moment the tab is not on screen.

use std::collections::HashMap;

use serde_json::Value;

pub mod cdp;
pub mod discover;

/// How the connection to Chrome is going.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Status {
    /// Asking (or not asked yet).
    #[default]
    Looking,
    /// No browser answered on any address.
    None,
    Connected,
    /// The browser went away; the next try is at this time (ms since the epoch).
    Lost { retry_at_ms: i64 },
}

/// One page of the browser: a target of type "page" that is not the browser's own.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageInfo {
    pub target_id: String,
    pub title: String,
    pub url: String,
}

impl PageInfo {
    /// The address's host, for the top bar and the picker ("docs.acme.test"); the whole address when it has none.
    pub fn host(&self) -> String {
        url::Url::parse(&self.url).ok().and_then(|u| u.host_str().map(str::to_owned)).filter(|h| !h.is_empty()).unwrap_or_else(|| self.url.clone())
    }

    /// The page as a picker row: "title — host".
    pub fn label(&self) -> String {
        let title = if self.title.trim().is_empty() { self.host() } else { self.title.trim().to_owned() };
        format!("{title} — {}", self.host())
    }

    /// The address when it is one a person's own browser may open: `http` or `https`, nothing else.
    pub fn openable(&self) -> Option<String> {
        let u = url::Url::parse(&self.url).ok()?;
        matches!(u.scheme(), "http" | "https").then(|| u.to_string())
    }
}

/// Which page the tab shows.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Follow {
    /// The page that changed (its address or title) most recently.
    #[default]
    Newest,
    Page(String),
}

/// A decoded frame: RGBA pixels, kept only in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub rgba: Vec<u8>,
    pub w: u32,
    pub h: u32,
    pub at_ms: i64,
}

/// Pages the browser keeps for itself are not for watching.
const SKIPPED_SCHEMES: [&str; 3] = ["devtools://", "chrome://", "chrome-extension://"];

/// The page a CDP `targetInfo` describes, when it is one to watch: type "page", a real address (not the browser's own pages
/// and not `about:blank`).
pub fn watchable(info: &Value) -> Option<PageInfo> {
    if info.get("type").and_then(Value::as_str) != Some("page") {
        return None;
    }
    let url = info.get("url").and_then(Value::as_str)?;
    if url.is_empty() || url == "about:blank" || SKIPPED_SCHEMES.iter().any(|s| url.starts_with(s)) {
        return None;
    }
    Some(PageInfo { target_id: info.get("targetId").and_then(Value::as_str)?.to_owned(), title: info.get("title").and_then(Value::as_str).unwrap_or("").to_owned(), url: url.to_owned() })
}

/// What the Browser tab knows. The CDP thread writes it, the window reads it (behind one lock); only `follow` is the window's.
#[derive(Debug, Default)]
pub struct BrowserView {
    pub status: Status,
    pub pages: Vec<PageInfo>,
    pub follow: Follow,
    /// The page whose frames are wanted now (its session is attached or being attached).
    pub shown: Option<String>,
    /// The newest frame not yet taken by the window: an older one that was never shown is dropped, never queued.
    pub frame: Option<Frame>,
    /// Counts every change of `frame`, including its removal.
    pub frame_seq: u64,
    /// When the shown page closed (ms since the epoch); the tab says so for [`CLOSED_NOTE_MS`].
    pub closed_at_ms: Option<i64>,
    /// When each page last changed its address or title (or appeared): the larger is the newer.
    stamps: HashMap<String, u64>,
    next_stamp: u64,
}

/// How long "That page closed." stays on screen.
pub const CLOSED_NOTE_MS: i64 = 3000;

impl BrowserView {
    /// A target appeared or changed: a page to watch is added or updated, one that no longer is one (it went to a browser page)
    /// is dropped. It becomes the newest when it is new or its address or title changed.
    pub fn upsert(&mut self, info: &Value) {
        let Some(id) = info.get("targetId").and_then(Value::as_str) else { return };
        match watchable(info) {
            Some(page) => {
                let changed = match self.pages.iter_mut().find(|p| p.target_id == page.target_id) {
                    Some(old) => {
                        let changed = old.url != page.url || old.title != page.title;
                        *old = page.clone();
                        changed
                    }
                    None => {
                        self.pages.push(page.clone());
                        true
                    }
                };
                if changed {
                    self.next_stamp += 1;
                    self.stamps.insert(page.target_id, self.next_stamp);
                }
            }
            None => {
                self.pages.retain(|p| p.target_id != id);
                self.stamps.remove(id);
            }
        }
    }

    /// A target closed. When it was the shown page, its frame goes, the note starts and a chosen page that closed is followed
    /// no more (back to the newest). True when it was the shown one.
    pub fn destroyed(&mut self, id: &str, now_ms: i64) -> bool {
        self.pages.retain(|p| p.target_id != id);
        self.stamps.remove(id);
        let was_shown = self.shown.as_deref() == Some(id);
        if self.follow == Follow::Page(id.to_owned()) {
            self.follow = Follow::Newest;
        }
        if was_shown {
            self.shown = None;
            self.clear_frame();
            self.closed_at_ms = Some(now_ms);
        }
        was_shown
    }

    /// The page that changed last.
    pub fn newest(&self) -> Option<&PageInfo> {
        self.pages.iter().max_by_key(|p| self.stamps.get(&p.target_id).copied().unwrap_or(0))
    }

    /// The page to show now: the chosen one while it is there, else the newest.
    pub fn choose(&self) -> Option<String> {
        if let Follow::Page(id) = &self.follow
            && self.pages.iter().any(|p| &p.target_id == id)
        {
            return Some(id.clone());
        }
        self.newest().map(|p| p.target_id.clone())
    }

    /// The shown page's record.
    pub fn shown_page(&self) -> Option<&PageInfo> {
        self.shown.as_deref().and_then(|id| self.pages.iter().find(|p| p.target_id == id))
    }

    /// A frame arrived: it replaces any frame the window has not taken yet.
    pub fn set_frame(&mut self, frame: Frame) {
        self.frame = Some(frame);
        self.frame_seq += 1;
    }

    pub fn clear_frame(&mut self) {
        self.frame = None;
        self.frame_seq += 1;
    }

    /// Everything the connection gave, forgotten (the browser is gone, or the socket was closed).
    pub fn forget(&mut self) {
        self.pages.clear();
        self.stamps.clear();
        self.shown = None;
        self.clear_frame();
    }
}

/// `points` (logical size) at `scale`, rounded up to a multiple of 64 and kept within 64 to 3840: the size a screencast is
/// asked for.
pub fn screencast_side(points: f32, scale: f32) -> u32 {
    let px = (points.max(1.0) * scale.max(1.0)).ceil() as u32;
    (px.div_ceil(64) * 64).clamp(64, 3840)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn info(id: &str, kind: &str, url: &str, title: &str) -> Value {
        json!({"targetId": id, "type": kind, "url": url, "title": title})
    }

    #[test]
    fn only_real_pages_are_watchable() {
        assert!(watchable(&info("a", "page", "https://docs.acme.test/start", "Acme docs")).is_some());
        for (kind, url) in [("page", "about:blank"), ("page", ""), ("page", "devtools://devtools/bundled/x.html"), ("page", "chrome://settings"), ("page", "chrome-extension://abc/popup.html"), ("service_worker", "https://docs.acme.test/sw.js"), ("iframe", "https://docs.acme.test/f"), ("browser", "https://docs.acme.test")] {
            assert!(watchable(&info("a", kind, url, "t")).is_none(), "{kind} {url}");
        }
        let p = watchable(&info("a", "page", "https://docs.acme.test/start", "Acme docs")).unwrap();
        assert_eq!((p.host().as_str(), p.label().as_str()), ("docs.acme.test", "Acme docs — docs.acme.test"));
        assert_eq!(p.openable().as_deref(), Some("https://docs.acme.test/start"));
        let odd = PageInfo { target_id: "b".into(), title: "".into(), url: "file:///tmp/x.html".into() };
        assert_eq!(odd.openable(), None, "only http and https go to the person's browser");
    }

    #[test]
    fn the_newest_page_is_the_one_whose_address_or_title_changed_last() {
        let mut v = BrowserView::default();
        v.upsert(&info("a", "page", "https://docs.acme.test/", "Acme docs"));
        v.upsert(&info("b", "page", "https://board.acme.test/", "Asha's board"));
        v.upsert(&info("blank", "page", "about:blank", ""));
        assert_eq!(v.choose().as_deref(), Some("b"), "the last to appear");
        // Nothing changed in a: the same info again does not make it newer.
        v.upsert(&info("a", "page", "https://docs.acme.test/", "Acme docs"));
        assert_eq!(v.choose().as_deref(), Some("b"));
        // A new title does.
        v.upsert(&info("a", "page", "https://docs.acme.test/", "Acme docs (2)"));
        assert_eq!(v.choose().as_deref(), Some("a"));
        // A chosen page wins while it is there, and the newest is the fallback.
        v.follow = Follow::Page("b".into());
        assert_eq!(v.choose().as_deref(), Some("b"));
        v.follow = Follow::Page("gone".into());
        assert_eq!(v.choose().as_deref(), Some("a"));
        // A page that goes to a browser page is no longer one.
        v.upsert(&info("a", "page", "chrome://newtab", "New tab"));
        assert_eq!(v.pages.iter().map(|p| p.target_id.as_str()).collect::<Vec<_>>(), ["b"]);
    }

    #[test]
    fn a_closed_shown_page_says_so_and_a_followed_page_that_closed_is_followed_no_more() {
        let mut v = BrowserView::default();
        v.upsert(&info("a", "page", "https://docs.acme.test/", "Acme docs"));
        v.upsert(&info("b", "page", "https://board.acme.test/", "Asha's board"));
        v.follow = Follow::Page("a".into());
        v.shown = Some("a".into());
        v.set_frame(Frame { rgba: vec![0; 4], w: 1, h: 1, at_ms: 1 });
        assert!(v.destroyed("a", 5000));
        assert_eq!((v.follow.clone(), v.shown.clone(), v.closed_at_ms, v.frame.is_none()), (Follow::Newest, None, Some(5000), true));
        // Another page closing is not the shown one's.
        assert!(!v.destroyed("zzz", 6000));
        assert_eq!(v.closed_at_ms, Some(5000));
    }

    #[test]
    fn only_the_latest_frame_is_kept() {
        let mut v = BrowserView::default();
        for i in 0..3u8 {
            v.set_frame(Frame { rgba: vec![i; 4], w: 1, h: 1, at_ms: i64::from(i) });
        }
        assert_eq!((v.frame.as_ref().map(|f| f.rgba[0]), v.frame_seq), (Some(2), 3), "two frames never shown were dropped");
        v.clear_frame();
        assert_eq!((v.frame.is_none(), v.frame_seq), (true, 4));
    }

    #[test]
    fn a_screencast_is_asked_for_in_multiples_of_64_at_the_windows_scale() {
        assert_eq!(screencast_side(440.0, 2.0), 896);
        assert_eq!(screencast_side(400.0, 1.0), 448);
        assert_eq!(screencast_side(512.0, 1.0), 512);
        assert_eq!(screencast_side(0.0, 2.0), 64);
        assert_eq!(screencast_side(9000.0, 2.0), 3840);
    }
}
