//! The watch-only Chrome DevTools Protocol client: one thread, one browser-level websocket, flatten mode (docs/knowledge/rune-live-browser.md).
//!
//! `Target.setDiscoverTargets` keeps the page list current; `Target.attachToTarget {flatten: true}` gives a session for the page
//! shown; `Page.startScreencast` on that session streams JPEG frames, each decoded here, handed to the window as the latest frame
//! (older ones never shown are dropped) and acknowledged. **Only the seven methods of [`CDP_METHODS`] are ever sent**: the one
//! send function ([`Engine::send`]) refuses and logs the name of any other. No `Browser.close`, `Target.createTarget`/`closeTarget`,
//! `Input.*`, `Runtime.*`, `Network.*`, `Storage.*` or `DOM.*`.
//!
//! Lifecycle: connected only while the window asks (`Cmd::Watch { on: true }`: the Browser tab is the visible panel tab and the
//! window is active). On `on: false` the screencast is stopped and the session detached at once; the socket is closed after
//! `Config::hidden_close` (60 s). A browser that goes away is `Status::Lost`; the retry is after 1 s, doubling to 30 s.
//!
//! Nothing here logs a page's title or address; a frame is never written anywhere.

use std::collections::HashMap;
use std::io;
use std::net::TcpStream;
use std::sync::mpsc::{self, RecvTimeoutError, TryRecvError};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use serde_json::{Value, json};
use tungstenite::{Error as WsError, Message, WebSocket};

use super::discover::{self, Endpoint};
use super::{BrowserView, Frame, Status};

/// The only methods this client sends.
pub const CDP_METHODS: [&str; 7] = ["Target.setDiscoverTargets", "Target.getTargets", "Target.attachToTarget", "Target.detachFromTarget", "Page.startScreencast", "Page.stopScreencast", "Page.screencastFrameAck"];

/// Whether `method` may be sent.
pub fn is_allowed(method: &str) -> bool {
    CDP_METHODS.contains(&method)
}

/// A CDP command as JSON text, or None (and a line in the log naming the method) when the method is not on the allow-list.
pub fn command(id: u64, method: &str, params: Value, session: Option<&str>) -> Option<String> {
    if !is_allowed(method) {
        eprintln!("neko-rune: browser: refused to send CDP method {method}");
        return None;
    }
    let mut message = json!({ "id": id, "method": method, "params": params });
    if let Some(session) = session {
        message["sessionId"] = json!(session);
    }
    Some(message.to_string())
}

/// No frame is held larger than this on a side.
const MAX_SIDE: u32 = 4096;

/// A screencast frame's base64 JPEG as RGBA pixels and their size; None when it is not a JPEG or is absurdly large.
pub fn decode_frame(data: &str) -> Option<(Vec<u8>, u32, u32)> {
    let bytes = base64::engine::general_purpose::STANDARD.decode(data.trim()).ok()?;
    let img = image::load_from_memory_with_format(&bytes, image::ImageFormat::Jpeg).ok()?;
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE {
        return None;
    }
    Some((img.to_rgba8().into_raw(), w, h))
}

/// What the window asks of the thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmd {
    /// Watch (or stop), asking the screencast for at most `max` pixels.
    Watch { on: bool, max: (u32, u32) },
    /// Look for a browser again now ("Look again").
    Look,
    /// End the thread.
    Stop,
}

/// How the thread behaves; `standard` is the app's, tests shorten the times and name their own address.
#[derive(Clone)]
pub struct Config {
    /// The addresses to look at, in order (`discover::probe_order`).
    pub bases: Vec<String>,
    /// Called after every change of the shared [`BrowserView`], from the thread: it must be cheap and coalesce.
    pub wake: Arc<dyn Fn() + Send + Sync>,
    /// Hidden this long, the websocket is closed.
    pub hidden_close: Duration,
    pub retry_first: Duration,
    pub retry_max: Duration,
}

impl Config {
    pub fn standard(wake: Arc<dyn Fn() + Send + Sync>) -> Config {
        Config { bases: discover::probe_order(std::env::var(discover::ENV).ok().as_deref()), wake, hidden_close: Duration::from_secs(60), retry_first: Duration::from_secs(1), retry_max: Duration::from_secs(30) }
    }
}

/// The thread, from the window's side. Dropping it ends the thread (stopping the screencast and detaching first).
pub struct Client {
    tx: mpsc::Sender<Cmd>,
    join: Option<JoinHandle<()>>,
}

impl Client {
    pub fn start(config: Config, view: Arc<Mutex<BrowserView>>) -> Client {
        let (tx, rx) = mpsc::channel();
        let join = std::thread::Builder::new().name("neko-browser".into()).spawn(move || Engine::new(config, view).run(rx)).ok();
        Client { tx, join }
    }

    pub fn send(&self, cmd: Cmd) {
        let _ = self.tx.send(cmd);
    }
}

impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.tx.send(Cmd::Stop);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn now_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64)
}

/// The session of the page being watched.
struct Attached {
    target_id: String,
    /// None until Chrome answers the attach.
    session: Option<String>,
    /// The size the screencast was started with, once it was.
    started: Option<(u32, u32)>,
}

struct Engine {
    cfg: Config,
    view: Arc<Mutex<BrowserView>>,
    ws: Option<WebSocket<TcpStream>>,
    next_id: u64,
    /// Attach commands waiting for their answer: command id to target id.
    pending: HashMap<u64, String>,
    attached: Option<Attached>,
    /// A page whose attach failed is left alone until then.
    attach_hold: Option<Instant>,
    watch_on: bool,
    max: (u32, u32),
    hidden_since: Option<Instant>,
    /// A look was made since the tab was last shown (or "Look again" asked).
    tried: bool,
    /// The browser was reached and went away: retry with a backoff.
    lost: bool,
    backoff: Duration,
    retry_at: Option<Instant>,
}

const IDLE: Duration = Duration::from_millis(100);
const READ_SLICE: Duration = Duration::from_millis(50);

impl Engine {
    fn new(cfg: Config, view: Arc<Mutex<BrowserView>>) -> Engine {
        let backoff = cfg.retry_first;
        Engine { cfg, view, ws: None, next_id: 0, pending: HashMap::new(), attached: None, attach_hold: None, watch_on: false, max: (64, 64), hidden_since: Some(Instant::now()), tried: false, lost: false, backoff, retry_at: None }
    }

    /// Changes the shared view and tells the window.
    fn update<R>(&self, f: impl FnOnce(&mut BrowserView) -> R) -> R {
        let out = f(&mut self.view.lock().unwrap_or_else(PoisonError::into_inner));
        (self.cfg.wake)();
        out
    }

    fn read_view<R>(&self, f: impl FnOnce(&BrowserView) -> R) -> R {
        f(&self.view.lock().unwrap_or_else(PoisonError::into_inner))
    }

    fn run(mut self, rx: mpsc::Receiver<Cmd>) {
        loop {
            let first = if self.ws.is_none() {
                match rx.recv_timeout(IDLE) {
                    Ok(c) => Some(c),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            } else {
                match rx.try_recv() {
                    Ok(c) => Some(c),
                    Err(TryRecvError::Empty) => None,
                    Err(TryRecvError::Disconnected) => break,
                }
            };
            let mut stop = false;
            for cmd in first.into_iter().chain(std::iter::from_fn(|| rx.try_recv().ok())) {
                stop |= !self.command(cmd);
            }
            if stop {
                break;
            }
            if self.watch_on {
                self.show_step();
            } else {
                self.hide_step();
            }
            if self.ws.is_some() {
                self.read_one();
            }
        }
        // The window is gone: leave Chrome as it was found.
        self.detach_current();
        self.close_socket();
    }

    /// False when the thread should end.
    fn command(&mut self, cmd: Cmd) -> bool {
        match cmd {
            Cmd::Watch { on, max } => {
                if on && !self.watch_on {
                    self.tried = false;
                }
                self.watch_on = on;
                self.max = max;
                self.hidden_since = if on { None } else { self.hidden_since.or_else(|| Some(Instant::now())) };
            }
            Cmd::Look => {
                self.tried = false;
                self.retry_at = None;
            }
            Cmd::Stop => return false,
        }
        true
    }

    /// Hidden: nothing streams, and after a while nothing is connected.
    fn hide_step(&mut self) {
        self.detach_current();
        if self.ws.is_some() && self.hidden_since.is_some_and(|t| t.elapsed() >= self.cfg.hidden_close) {
            self.close_socket();
            self.update(|v| {
                v.status = Status::Looking;
                v.forget();
            });
            self.tried = false;
        }
    }

    fn show_step(&mut self) {
        if self.ws.is_none() {
            let due = if self.lost { self.retry_at.is_none_or(|t| Instant::now() >= t) } else { !self.tried };
            if due {
                self.connect();
            }
        }
        if self.ws.is_some() {
            self.reconcile();
        }
    }

    fn connect(&mut self) {
        self.tried = true;
        if !self.lost {
            self.update(|v| v.status = Status::Looking);
        }
        let Some(endpoint) = discover::discover(&self.cfg.bases) else {
            if self.lost {
                self.schedule_retry();
            } else {
                self.update(|v| v.status = Status::None);
            }
            return;
        };
        match open(&endpoint) {
            Some(ws) => {
                self.ws = Some(ws);
                self.lost = false;
                self.backoff = self.cfg.retry_first;
                self.retry_at = None;
                self.update(|v| {
                    v.forget();
                    v.status = Status::Connected;
                });
                self.send("Target.setDiscoverTargets", json!({ "discover": true }), None);
            }
            None => {
                self.lost = true;
                self.schedule_retry();
            }
        }
    }

    fn schedule_retry(&mut self) {
        let wait = self.backoff;
        self.backoff = (wait * 2).min(self.cfg.retry_max);
        self.retry_at = Some(Instant::now() + wait);
        let at = now_ms() + wait.as_millis() as i64;
        self.update(|v| v.status = Status::Lost { retry_at_ms: at });
    }

    /// The socket broke or Chrome closed it.
    fn lose(&mut self) {
        self.ws = None;
        self.pending.clear();
        self.attached = None;
        self.lost = true;
        self.backoff = self.cfg.retry_first;
        self.update(|v| v.forget());
        if self.watch_on {
            self.schedule_retry();
        } else {
            // Hidden: nothing is tried until the tab is shown again, then at once.
            self.retry_at = None;
            self.update(|v| v.status = Status::Looking);
        }
    }

    fn close_socket(&mut self) {
        if let Some(mut ws) = self.ws.take() {
            let _ = ws.close(None);
            let _ = ws.flush();
        }
        self.pending.clear();
        self.attached = None;
    }

    /// The one place a command leaves: the allow-list is checked here. Returns its id.
    fn send(&mut self, method: &str, params: Value, session: Option<&str>) -> Option<u64> {
        self.next_id += 1;
        let text = command(self.next_id, method, params, session)?;
        let ws = self.ws.as_mut()?;
        match ws.send(Message::text(text)) {
            Ok(()) => Some(self.next_id),
            // Queued by the socket, flushed on the next read or write.
            Err(WsError::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Some(self.next_id),
            Err(_) => {
                self.lose();
                None
            }
        }
    }

    /// Makes the attached page the one `choose` names.
    fn reconcile(&mut self) {
        let wanted = self.read_view(BrowserView::choose);
        let current = self.attached.as_ref().map(|a| (a.target_id.clone(), a.session.clone(), a.started));
        match (current, wanted) {
            (Some((target, session, started)), Some(w)) if target == w => {
                let Some(session) = session else { return };
                let max = self.max;
                match started {
                    None => self.start(&session, max),
                    // A bigger area than the screencast was asked for: ask again.
                    Some(used) if max.0 > used.0 || max.1 > used.1 => {
                        self.send("Page.stopScreencast", json!({}), Some(&session));
                        self.start(&session, max);
                    }
                    Some(_) => {}
                }
            }
            (Some(_), other) => {
                self.detach_current();
                if let Some(w) = other {
                    self.attach(w);
                }
            }
            (None, Some(w)) => self.attach(w),
            (None, None) => {
                if self.read_view(|v| v.shown.is_some()) {
                    self.update(|v| v.shown = None);
                }
            }
        }
    }

    fn attach(&mut self, target: String) {
        if self.attach_hold.is_some_and(|t| Instant::now() < t) {
            return;
        }
        let Some(id) = self.send("Target.attachToTarget", json!({ "targetId": target, "flatten": true }), None) else { return };
        self.pending.insert(id, target.clone());
        self.attached = Some(Attached { target_id: target.clone(), session: None, started: None });
        self.update(|v| {
            v.shown = Some(target);
            v.clear_frame();
        });
    }

    fn start(&mut self, session: &str, max: (u32, u32)) {
        let params = json!({ "format": "jpeg", "quality": 70, "maxWidth": max.0, "maxHeight": max.1, "everyNthFrame": 1 });
        if self.send("Page.startScreencast", params, Some(session)).is_some()
            && let Some(a) = self.attached.as_mut()
        {
            a.started = Some(max);
        }
    }

    /// Stops the screencast and detaches the session, at once.
    fn detach_current(&mut self) {
        let Some(a) = self.attached.take() else { return };
        if let Some(session) = a.session {
            if a.started.is_some() {
                self.send("Page.stopScreencast", json!({}), Some(&session));
            }
            self.send("Target.detachFromTarget", json!({ "sessionId": session }), None);
        }
        self.update(|v| {
            v.shown = None;
            v.clear_frame();
        });
    }

    fn read_one(&mut self) {
        let Some(ws) = self.ws.as_mut() else { return };
        match ws.read() {
            Ok(Message::Text(text)) => self.on_message(&text),
            Ok(Message::Close(_)) => self.lose(),
            Ok(_) => {}
            Err(WsError::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => {}
            Err(_) => self.lose(),
        }
    }

    fn on_message(&mut self, text: &str) {
        let Ok(message) = serde_json::from_str::<Value>(text) else { return };
        if let Some(id) = message.get("id").and_then(Value::as_u64) {
            self.on_reply(id, &message);
            return;
        }
        let params = &message["params"];
        match message.get("method").and_then(Value::as_str) {
            Some("Target.targetCreated" | "Target.targetInfoChanged") => {
                let info = params["targetInfo"].clone();
                self.update(|v| v.upsert(&info));
            }
            Some("Target.targetDestroyed") => {
                let Some(id) = params["targetId"].as_str() else { return };
                if self.attached.as_ref().is_some_and(|a| a.target_id == id) {
                    self.attached = None;
                }
                let id = id.to_owned();
                self.update(|v| v.destroyed(&id, now_ms()));
            }
            Some("Target.detachedFromTarget") => {
                let session = params["sessionId"].as_str();
                if session.is_some() && self.attached.as_ref().and_then(|a| a.session.as_deref()) == session {
                    self.attached = None;
                    self.attach_hold = Some(Instant::now() + Duration::from_secs(1));
                    self.update(|v| {
                        v.shown = None;
                        v.clear_frame();
                    });
                }
            }
            Some("Page.screencastFrame") => self.on_frame(&message),
            _ => {}
        }
    }

    fn on_reply(&mut self, id: u64, message: &Value) {
        let Some(target) = self.pending.remove(&id) else { return };
        let session = message["result"]["sessionId"].as_str().map(str::to_owned);
        let current = self.attached.as_ref().is_some_and(|a| a.target_id == target && a.session.is_none());
        match (session, current) {
            (Some(session), true) => {
                if let Some(a) = self.attached.as_mut() {
                    a.session = Some(session);
                }
            }
            // The page was dropped while Chrome answered: let the stray session go.
            (Some(session), false) => {
                self.send("Target.detachFromTarget", json!({ "sessionId": session }), None);
            }
            // The attach failed (the page is closing): leave it alone for a moment.
            (None, _) => {
                if current {
                    self.attached = None;
                    self.update(|v| v.shown = None);
                }
                self.attach_hold = Some(Instant::now() + Duration::from_secs(1));
            }
        }
    }

    fn on_frame(&mut self, message: &Value) {
        let session = message["sessionId"].as_str();
        if session.is_none() || self.attached.as_ref().and_then(|a| a.session.as_deref()) != session {
            return;
        }
        let params = &message["params"];
        if let Some((rgba, w, h)) = params["data"].as_str().and_then(decode_frame) {
            self.update(|v| v.set_frame(Frame { rgba, w, h, at_ms: now_ms() }));
        }
        // Acknowledged either way, so Chrome keeps sending.
        if let (Some(ack), Some(session)) = (params["sessionId"].as_i64(), session) {
            let session = session.to_owned();
            self.send("Page.screencastFrameAck", json!({ "sessionId": ack }), Some(&session));
        }
    }
}

/// Opens the browser's websocket on the loopback address `endpoint` named.
fn open(endpoint: &Endpoint) -> Option<WebSocket<TcpStream>> {
    use std::net::ToSocketAddrs;
    let url = discover::loopback_url(&endpoint.ws_url).ok()?;
    let host = url.host_str()?.trim_matches(['[', ']']).to_owned();
    let addrs: Vec<_> = (host.as_str(), url.port_or_known_default()?).to_socket_addrs().ok()?.collect();
    let stream = addrs.iter().find_map(|a| TcpStream::connect_timeout(a, Duration::from_secs(1)).ok())?;
    stream.set_nodelay(true).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).ok()?;
    let (ws, _) = tungstenite::client(url.as_str(), stream).ok()?;
    // From here a read gives up after a slice, so the thread hears the window between messages.
    ws.get_ref().set_read_timeout(Some(READ_SLICE)).ok()?;
    Some(ws)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_outside_the_allow_list_is_ever_written() {
        for method in CDP_METHODS {
            assert!(command(1, method, json!({}), None).is_some(), "{method}");
        }
        for method in ["Browser.close", "Target.closeTarget", "Target.createTarget", "Input.dispatchMouseEvent", "Input.dispatchKeyEvent", "Runtime.evaluate", "Network.enable", "Storage.getCookies", "DOM.getDocument", "Page.navigate", "Page.captureScreenshot", "Target.setAutoAttach", "", "target.attachtotarget", "Page.startScreencast "] {
            assert_eq!(command(1, method, json!({}), Some("s")), None, "{method:?}");
        }
        assert_eq!(CDP_METHODS.len(), 7);
        let text = command(7, "Page.startScreencast", json!({"format": "jpeg"}), Some("sess-1")).unwrap();
        let v: Value = serde_json::from_str(&text).unwrap();
        assert_eq!((v["id"].as_u64(), v["method"].as_str(), v["sessionId"].as_str()), (Some(7), Some("Page.startScreencast"), Some("sess-1")));
    }

    #[test]
    fn a_jpeg_frame_decodes_to_rgba_and_junk_does_not() {
        let img = image::RgbaImage::from_fn(48, 32, |x, y| image::Rgba([(x * 5) as u8, (y * 7) as u8, 128, 255]));
        let mut jpeg = Vec::new();
        image::DynamicImage::ImageRgba8(img).to_rgb8().write_to(&mut std::io::Cursor::new(&mut jpeg), image::ImageFormat::Jpeg).unwrap();
        let data = base64::engine::general_purpose::STANDARD.encode(&jpeg);
        let (rgba, w, h) = decode_frame(&data).unwrap();
        assert_eq!((w, h, rgba.len()), (48, 32, 48 * 32 * 4));
        assert_eq!(rgba[3], 255);
        assert!(decode_frame("not base64 !!").is_none());
        assert!(decode_frame(&base64::engine::general_purpose::STANDARD.encode(b"plain text")).is_none());
    }
}
