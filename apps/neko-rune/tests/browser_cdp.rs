//! The live Browser tab's CDP client against a fake Chrome (docs/knowledge/rune-live-browser.md): a tiny HTTP answer for
//! `/json/version` and a websocket on one loopback port (`127.0.0.1:0`). The fake records every method it is sent.
//!
//! What is asserted: attach then `Page.startScreencast` on the attached session, an acknowledgement per frame, stop and detach the
//! moment the tab is hidden, the socket closed after the hidden delay, a page switch detaching the old session first, a lost
//! browser retried, a missing one reported, and that **no method outside `CDP_METHODS` is ever sent**.

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use neko_rune::browser::cdp::{CDP_METHODS, Client, Cmd, Config};
use neko_rune::browser::{BrowserView, Follow, Status};
use serde_json::{Value, json};
use tungstenite::Message;

#[derive(Clone, Debug)]
struct Rec {
    method: String,
    session: Option<String>,
    params: Value,
}

struct Fake {
    port: u16,
    log: Arc<Mutex<Vec<Rec>>>,
    inject: Sender<Value>,
    stop: Arc<AtomicBool>,
    connections: Arc<AtomicUsize>,
    closed: Arc<AtomicUsize>,
}

impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn jpeg(rgb: [u8; 3]) -> String {
    let img = image::RgbImage::from_pixel(32, 24, image::Rgb(rgb));
    let mut out = Vec::new();
    image::DynamicImage::ImageRgb8(img).write_to(&mut std::io::Cursor::new(&mut out), image::ImageFormat::Jpeg).unwrap();
    base64::engine::general_purpose::STANDARD.encode(out)
}

fn target(id: &str, kind: &str, url: &str, title: &str) -> Value {
    json!({ "targetId": id, "type": kind, "url": url, "title": title, "attached": false })
}

const FRAMES: usize = 3;
const COLOURS: [[u8; 3]; FRAMES] = [[200, 40, 40], [40, 200, 40], [40, 40, 200]];

fn fake() -> Fake {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (inject, rx) = mpsc::channel::<Value>();
    let rx = Arc::new(Mutex::new(rx));
    let log = Arc::new(Mutex::new(Vec::new()));
    let (stop, connections, closed) = (Arc::new(AtomicBool::new(false)), Arc::new(AtomicUsize::new(0)), Arc::new(AtomicUsize::new(0)));
    let f = Fake { port, log: log.clone(), inject, stop: stop.clone(), connections: connections.clone(), closed: closed.clone() };
    std::thread::spawn(move || {
        while !stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((stream, _)) => {
                    stream.set_nonblocking(false).unwrap();
                    let mut head = [0u8; 20];
                    let n = stream.peek(&mut head).unwrap_or(0);
                    if head[..n].starts_with(b"GET /json/version") {
                        answer_version(stream, port);
                    } else {
                        connections.fetch_add(1, Ordering::SeqCst);
                        let (log, rx, stop, closed) = (log.clone(), rx.clone(), stop.clone(), closed.clone());
                        std::thread::spawn(move || {
                            socket(stream, log, rx, stop);
                            closed.fetch_add(1, Ordering::SeqCst);
                        });
                    }
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    });
    f
}

fn answer_version(mut stream: TcpStream, port: u16) {
    let mut buf = [0u8; 2048];
    let _ = stream.read(&mut buf);
    let body = json!({ "Browser": "FakeChrome/1", "webSocketDebuggerUrl": format!("ws://127.0.0.1:{port}/devtools/browser/fake-1") }).to_string();
    let _ = stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()).as_bytes());
}

fn socket(stream: TcpStream, log: Arc<Mutex<Vec<Rec>>>, rx: Arc<Mutex<Receiver<Value>>>, stop: Arc<AtomicBool>) {
    let Ok(mut ws) = tungstenite::accept(stream) else { return };
    ws.get_ref().set_read_timeout(Some(Duration::from_millis(15))).unwrap();
    let mut sessions = 0;
    // Frames sent so far, by session.
    let mut sent: std::collections::HashMap<String, usize> = Default::default();
    let reply = |ws: &mut tungstenite::WebSocket<TcpStream>, v: Value| {
        let _ = ws.send(Message::text(v.to_string()));
    };
    while !stop.load(Ordering::SeqCst) {
        let injected: Vec<Value> = rx.lock().unwrap().try_iter().collect();
        for v in injected {
            if v.get("__close").is_some() {
                let _ = ws.close(None);
                return;
            }
            reply(&mut ws, v);
        }
        let text = match ws.read() {
            Ok(Message::Text(t)) => t.to_string(),
            Ok(Message::Close(_)) | Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => return,
            Ok(_) => continue,
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => continue,
            Err(_) => return,
        };
        let v: Value = serde_json::from_str(&text).unwrap();
        let method = v["method"].as_str().unwrap_or("").to_owned();
        let session = v["sessionId"].as_str().map(str::to_owned);
        log.lock().unwrap().push(Rec { method: method.clone(), session: session.clone(), params: v["params"].clone() });
        let id = v["id"].clone();
        match method.as_str() {
            "Target.setDiscoverTargets" => {
                reply(&mut ws, json!({ "id": id, "result": {} }));
                for info in [target("pg-docs", "page", "https://docs.acme.test/start", "Acme docs"), target("pg-new", "page", "about:blank", ""), target("pg-dev", "page", "devtools://devtools/bundled/inspector.html", "DevTools"), target("sw-1", "service_worker", "https://docs.acme.test/sw.js", "")] {
                    reply(&mut ws, json!({ "method": "Target.targetCreated", "params": { "targetInfo": info } }));
                }
            }
            "Target.attachToTarget" => {
                sessions += 1;
                reply(&mut ws, json!({ "id": id, "result": { "sessionId": format!("S{sessions}") } }));
            }
            "Page.startScreencast" => {
                reply(&mut ws, json!({ "id": id, "result": {} }));
                let s = session.clone().unwrap();
                sent.insert(s.clone(), 1);
                reply(&mut ws, json!({ "method": "Page.screencastFrame", "sessionId": s, "params": { "data": jpeg(COLOURS[0]), "metadata": {}, "sessionId": 1 } }));
            }
            "Page.screencastFrameAck" => {
                let s = session.clone().unwrap();
                let n = sent.entry(s.clone()).or_insert(0);
                if *n < FRAMES {
                    *n += 1;
                    let k = *n;
                    reply(&mut ws, json!({ "method": "Page.screencastFrame", "sessionId": s, "params": { "data": jpeg(COLOURS[k - 1]), "metadata": {}, "sessionId": k } }));
                }
            }
            _ => reply(&mut ws, json!({ "id": id, "result": {} })),
        }
    }
}

fn until(what: &str, mut f: impl FnMut() -> bool) {
    let start = Instant::now();
    while !f() {
        assert!(start.elapsed() < Duration::from_secs(8), "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

struct Run {
    client: Client,
    view: Arc<Mutex<BrowserView>>,
    wakes: Arc<AtomicUsize>,
}

fn start(bases: Vec<String>) -> Run {
    let view = Arc::new(Mutex::new(BrowserView::default()));
    let wakes = Arc::new(AtomicUsize::new(0));
    let w = wakes.clone();
    let cfg = Config { bases, wake: Arc::new(move || {
            w.fetch_add(1, Ordering::SeqCst);
        }), hidden_close: Duration::from_millis(500), retry_first: Duration::from_millis(40), retry_max: Duration::from_millis(160) };
    Run { client: Client::start(cfg, view.clone()), view, wakes }
}

fn methods(f: &Fake) -> Vec<String> {
    f.log.lock().unwrap().iter().map(|r| r.method.clone()).collect()
}

fn count(f: &Fake, method: &str) -> usize {
    methods(f).iter().filter(|m| *m == method).count()
}

fn no_method_outside_the_allow_list(f: &Fake) {
    for m in methods(f) {
        assert!(CDP_METHODS.contains(&m.as_str()), "the client sent {m}, which is not on the allow-list");
    }
}

#[test]
fn it_attaches_screencasts_acknowledges_each_frame_and_stops_and_detaches_on_hide() {
    let f = fake();
    let run = start(vec![format!("http://127.0.0.1:{}", f.port)]);
    run.client.send(Cmd::Watch { on: true, max: (640, 448) });

    until("three frames acknowledged", || count(&f, "Page.screencastFrameAck") >= FRAMES);
    let log = f.log.lock().unwrap().clone();
    // Discovery, then an attach that asks for a flattened session, then the screencast on the session Chrome answered with.
    assert_eq!(log[0].method, "Target.setDiscoverTargets");
    assert_eq!(log[0].params, json!({ "discover": true }));
    let attach = log.iter().find(|r| r.method == "Target.attachToTarget").unwrap();
    assert_eq!(attach.params, json!({ "targetId": "pg-docs", "flatten": true }), "only the real page was attached: not blank, not devtools, not a worker");
    let start_cast = log.iter().find(|r| r.method == "Page.startScreencast").unwrap();
    assert_eq!(start_cast.session.as_deref(), Some("S1"));
    assert_eq!(start_cast.params, json!({ "format": "jpeg", "quality": 70, "maxWidth": 640, "maxHeight": 448, "everyNthFrame": 1 }));
    // One acknowledgement per frame, with the frame's own number, on the session.
    let acks: Vec<_> = log.iter().filter(|r| r.method == "Page.screencastFrameAck").collect();
    assert_eq!(acks.iter().map(|a| a.params["sessionId"].as_i64().unwrap()).collect::<Vec<_>>(), [1, 2, 3]);
    assert!(acks.iter().all(|a| a.session.as_deref() == Some("S1")));
    until("the last frame shown", || run.view.lock().unwrap().frame.as_ref().is_some_and(|fr| fr.rgba[2] > 150));
    {
        let v = run.view.lock().unwrap();
        assert_eq!(v.status, Status::Connected);
        assert_eq!(v.pages.iter().map(|p| p.title.as_str()).collect::<Vec<_>>(), ["Acme docs"]);
        assert_eq!(v.shown.as_deref(), Some("pg-docs"));
        let fr = v.frame.as_ref().unwrap();
        assert_eq!((fr.w, fr.h, fr.rgba.len()), (32, 24, 32 * 24 * 4));
        assert!(fr.rgba[0] < 90 && fr.rgba[1] < 90, "the third frame is the blue one: {:?}", &fr.rgba[..4]);
    }
    assert!(run.wakes.load(Ordering::SeqCst) >= FRAMES, "the window was woken for the frames");

    // Hidden: stop and detach at once, the picture goes, the socket stays for a while.
    let before = methods(&f).len();
    run.client.send(Cmd::Watch { on: false, max: (640, 448) });
    until("stop and detach", || count(&f, "Target.detachFromTarget") == 1);
    let log = f.log.lock().unwrap().clone();
    assert_eq!(log[before].method, "Page.stopScreencast");
    assert_eq!(log[before].session.as_deref(), Some("S1"));
    assert_eq!(log[before + 1].method, "Target.detachFromTarget");
    assert_eq!(log[before + 1].params, json!({ "sessionId": "S1" }));
    assert!(run.view.lock().unwrap().frame.is_none() && run.view.lock().unwrap().shown.is_none());
    assert_eq!(f.closed.load(Ordering::SeqCst), 0, "the socket is still open right after hiding");
    until("the socket closed after the hidden delay", || f.closed.load(Ordering::SeqCst) == 1);

    // Shown again: a new look, a new attach.
    run.client.send(Cmd::Watch { on: true, max: (640, 448) });
    until("a second attach", || count(&f, "Target.attachToTarget") == 2);
    assert_eq!(f.connections.load(Ordering::SeqCst), 2);
    until("a frame again", || run.view.lock().unwrap().frame.is_some());
    no_method_outside_the_allow_list(&f);
    drop(run);
    // Dropping the client leaves Chrome as it was found: the screencast stopped and detached.
    until("a clean goodbye", || count(&f, "Target.detachFromTarget") == 2);
    no_method_outside_the_allow_list(&f);
}

#[test]
fn a_newer_page_is_followed_by_detaching_the_old_session_first_and_a_closed_page_is_noted() {
    let f = fake();
    let run = start(vec![format!("http://127.0.0.1:{}", f.port)]);
    run.client.send(Cmd::Watch { on: true, max: (640, 448) });
    until("the first page's screencast", || count(&f, "Page.startScreencast") == 1);

    f.inject.send(json!({ "method": "Target.targetCreated", "params": { "targetInfo": target("pg-board", "page", "https://board.acme.test/q3", "Asha's board") } })).unwrap();
    until("the second page's screencast", || count(&f, "Page.startScreencast") == 2);
    let log = f.log.lock().unwrap().clone();
    let pos = |m: &str, nth: usize| log.iter().enumerate().filter(|(_, r)| r.method == m).nth(nth).map(|(i, _)| i).unwrap();
    assert!(pos("Page.stopScreencast", 0) < pos("Target.detachFromTarget", 0) && pos("Target.detachFromTarget", 0) < pos("Target.attachToTarget", 1), "the old session is detached before the new page is attached");
    assert_eq!(log[pos("Target.attachToTarget", 1)].params["targetId"], "pg-board");
    assert_eq!(log[pos("Page.startScreencast", 1)].session.as_deref(), Some("S2"));

    // Choosing a page: back to the first, and the view says which.
    run.view.lock().unwrap().follow = Follow::Page("pg-docs".into());
    until("back on the first page", || count(&f, "Page.startScreencast") == 3 && run.view.lock().unwrap().shown.as_deref() == Some("pg-docs"));

    // The shown page closes: a note, no frame, and the choice is dropped (back to the newest, the board).
    f.inject.send(json!({ "method": "Target.targetDestroyed", "params": { "targetId": "pg-docs" } })).unwrap();
    until("the note", || run.view.lock().unwrap().closed_at_ms.is_some());
    until("following the newest again", || {
        let v = run.view.lock().unwrap();
        v.follow == Follow::Newest && v.shown.as_deref() == Some("pg-board")
    });
    until("the board's screencast", || count(&f, "Page.startScreencast") == 4);
    no_method_outside_the_allow_list(&f);
}

#[test]
fn a_lost_browser_is_retried_and_a_missing_one_is_reported() {
    let f = fake();
    let run = start(vec![format!("http://127.0.0.1:{}", f.port)]);
    run.client.send(Cmd::Watch { on: true, max: (320, 256) });
    until("connected with a frame", || run.view.lock().unwrap().frame.is_some());
    f.inject.send(json!({ "__close": true })).unwrap();
    until("lost", || matches!(run.view.lock().unwrap().status, Status::Lost { .. }));
    assert!(run.view.lock().unwrap().pages.is_empty(), "nothing of the old connection is kept");
    until("found again", || f.connections.load(Ordering::SeqCst) == 2 && run.view.lock().unwrap().status == Status::Connected);
    until("a frame again", || run.view.lock().unwrap().frame.is_some());
    no_method_outside_the_allow_list(&f);

    // Nothing answers on this one.
    let dead = TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let none = start(vec![format!("http://127.0.0.1:{dead}")]);
    none.client.send(Cmd::Watch { on: true, max: (320, 256) });
    until("no browser", || none.view.lock().unwrap().status == Status::None);
    // A tab that was never shown connects to nothing at all.
    let idle = start(vec![format!("http://127.0.0.1:{}", f.port)]);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(idle.view.lock().unwrap().status, Status::Looking);
    assert_eq!(f.connections.load(Ordering::SeqCst), 2, "a hidden tab opens no socket");
}
