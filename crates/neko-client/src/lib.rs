//! The client SDK: a persistent, auto-reconnecting connection to
//! `neko-daemon` over its Unix socket. The `neko` app crate is the only
//! thing that should depend on this — nothing here knows about GPUI, and
//! nothing in `neko` should reach past this crate to `neko-protocol`'s
//! framing directly (see `AGENTS.md`'s crate-boundary rule).

use std::collections::HashMap;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_core::Stream;
use neko_protocol::{Event, Frame, Request, Response, read_frame, write_frame};

#[derive(Debug, Clone)]
pub enum ClientError {
    /// No live connection right now — the daemon may be starting, or a
    /// reconnect is in flight. Callers driving a search-as-you-type UI
    /// should just retry on the next keystroke rather than surface this as
    /// a hard failure.
    NotConnected,
    /// The connection dropped after the request was sent but before a
    /// reply arrived.
    Disconnected,
}

impl std::fmt::Display for ClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClientError::NotConnected => write!(f, "not connected to neko-daemon"),
            ClientError::Disconnected => write!(f, "disconnected from neko-daemon before a reply arrived"),
        }
    }
}

impl std::error::Error for ClientError {}

struct Shared {
    next_id: AtomicU64,
    pending: Mutex<HashMap<u64, futures_channel::oneshot::Sender<Response>>>,
    /// Requests whose reply can arrive in more than one frame — today,
    /// exactly `Request::Search` (see `Response::ends_request`). Kept in a
    /// second map rather than generalizing `pending`, so the ordinary
    /// one-request-one-response path stays a `oneshot` with no per-response
    /// allocation and no way for a caller to accidentally await a second
    /// frame that will never come.
    pending_streams: Mutex<HashMap<u64, futures_channel::mpsc::UnboundedSender<Response>>>,
    write_stream: Mutex<Option<UnixStream>>,
    event_tx: std_mpsc::Sender<Event>,
    /// Mirrors whether `run_supervisor` currently holds a live socket —
    /// the one client-visible connection-state signal callers need to stop
    /// a dead daemon from being a silent no-op (see `AGENTS.md`,
    /// "Reliability / error states"). `NekoClient::is_connected` is a
    /// plain poll, not a push channel: the caller (`neko`'s summon loop)
    /// already polls something else (`Event`) on a short, fixed interval,
    /// so a second thing to poll from the same loop is the smaller
    /// addition — no new channel, no new wire concept.
    connected: AtomicBool,
}

#[derive(Clone)]
pub struct NekoClient {
    shared: Arc<Shared>,
}

impl NekoClient {
    /// Connects in the background (retrying with a short backoff) and
    /// returns immediately — the daemon does not need to already be
    /// listening. `events` is where `Event::HotkeyChanged` and similar
    /// server-pushed messages arrive; poll it from the same loop the caller
    /// already uses for other periodic work (this crate stays runtime-agnostic
    /// rather than assuming GPUI's executor).
    pub fn connect(socket_path: PathBuf) -> (Self, std_mpsc::Receiver<Event>) {
        let (event_tx, event_rx) = std_mpsc::channel();
        let shared = Arc::new(Shared {
            next_id: AtomicU64::new(1),
            pending: Mutex::new(HashMap::new()),
            pending_streams: Mutex::new(HashMap::new()),
            write_stream: Mutex::new(None),
            event_tx,
            connected: AtomicBool::new(false),
        });

        let supervisor_shared = shared.clone();
        std::thread::spawn(move || run_supervisor(socket_path, supervisor_shared));

        (Self { shared }, event_rx)
    }

    /// Send a request and asynchronously await its matching response.
    /// Resolves immediately with `Err(ClientError::NotConnected)` if there
    /// is no live connection right now — this deliberately does not block
    /// or buffer, so a caller wired to "every keystroke" naturally retries.
    pub fn request(
        &self,
        request: Request,
    ) -> impl std::future::Future<Output = Result<Response, ClientError>> + 'static {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = futures_channel::oneshot::channel();

        let send_result = {
            let mut stream_guard = self.shared.write_stream.lock().unwrap();
            match stream_guard.as_mut() {
                Some(stream) => write_frame(stream, &Frame::Request { id, request }),
                None => Err(std::io::Error::new(
                    std::io::ErrorKind::NotConnected,
                    "no live connection",
                )),
            }
        };

        if send_result.is_ok() {
            self.shared.pending.lock().unwrap().insert(id, tx);
        }

        async move {
            if send_result.is_err() {
                return Err(ClientError::NotConnected);
            }
            rx.await.map_err(|_| ClientError::Disconnected)
        }
    }

    /// Send a request whose reply may arrive in more than one frame, and
    /// get every frame in order as they land — today that means
    /// `Request::Search`, which answers the fast providers immediately and
    /// the slow ones in a follow-up (see `Response::SearchResults`'s own
    /// doc comment, and `AGENTS.md`'s "Two-phase search" section).
    ///
    /// The stream ends after the frame that satisfies
    /// `Response::ends_request`, or immediately if there is no live
    /// connection — a caller wired to "every keystroke" gets `None` on the
    /// first poll and naturally retries on the next one, exactly as
    /// [`NekoClient::request`]'s own `Err(NotConnected)` already behaves.
    /// **A caller must drain until `None`** rather than stopping at the
    /// first frame it likes the look of: the correlation entry is retired
    /// by the reader thread when the final frame arrives, but the
    /// caller-side task is what decides whether anything is still waiting
    /// on it.
    pub fn request_streaming(&self, request: Request) -> ResponseStream {
        let id = self.shared.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = futures_channel::mpsc::unbounded();

        let send_result = {
            let mut stream_guard = self.shared.write_stream.lock().unwrap();
            match stream_guard.as_mut() {
                Some(stream) => write_frame(stream, &Frame::Request { id, request }),
                None => Err(std::io::Error::new(std::io::ErrorKind::NotConnected, "no live connection")),
            }
        };

        if send_result.is_ok() {
            self.shared.pending_streams.lock().unwrap().insert(id, tx);
        }
        // On a send failure `tx` drops here, so the returned stream is
        // already finished — the caller sees `None` on its first poll.
        ResponseStream { rx }
    }

    /// Whether the reconnect supervisor currently has a live socket open to
    /// `neko-daemon`. `false` both before the very first connection and
    /// for as long as the daemon stays unreachable after one drops — a
    /// caller that wants to distinguish "never connected yet" from "was
    /// connected, then lost it" needs its own first-response bookkeeping
    /// (`request`'s own `Ok`/`Err` already gives it that), since this is
    /// deliberately just the one plain, poll-anytime signal.
    pub fn is_connected(&self) -> bool {
        self.shared.connected.load(Ordering::Relaxed)
    }
}

/// The frames of one streaming request, in arrival order. Ends after the
/// response that satisfies `Response::ends_request`, or when the connection
/// drops mid-request.
pub struct ResponseStream {
    rx: futures_channel::mpsc::UnboundedReceiver<Response>,
}

impl ResponseStream {
    /// The next frame, or `None` once this request is finished.
    ///
    /// Hand-rolled over `futures_core::Stream` rather than exposing the
    /// trait (or pulling in `futures-util` for `StreamExt::next`): callers
    /// only ever want this one operation, and a plain inherent `async fn`
    /// keeps `ResponseStream` from leaking a public trait dependency out of
    /// this crate's API.
    pub async fn next(&mut self) -> Option<Response> {
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut self.rx).poll_next(cx)).await
    }
}

fn run_supervisor(socket_path: PathBuf, shared: Arc<Shared>) {
    let mut backoff = Duration::from_millis(50);
    const MAX_BACKOFF: Duration = Duration::from_secs(2);

    loop {
        match UnixStream::connect(&socket_path) {
            Ok(stream) => {
                backoff = Duration::from_millis(50);
                let reader_stream = match stream.try_clone() {
                    Ok(s) => s,
                    Err(_) => {
                        std::thread::sleep(backoff);
                        continue;
                    }
                };
                *shared.write_stream.lock().unwrap() = Some(stream);
                shared.connected.store(true, Ordering::Relaxed);

                // Blocks until the connection drops (EOF or an error) —
                // that's the resume point for the outer reconnect loop.
                read_until_disconnected(reader_stream, &shared);

                *shared.write_stream.lock().unwrap() = None;
                shared.connected.store(false, Ordering::Relaxed);
                // Any request that was mid-flight when the connection died
                // resolves now: dropping these senders turns their
                // `rx.await` into `Err(Canceled)`, i.e. `ClientError::Disconnected`.
                shared.pending.lock().unwrap().clear();
                // Same for streaming requests — dropping the sender ends
                // the stream, so a caller awaiting the *second* frame of a
                // two-phase search stops waiting instead of hanging on a
                // frame that can no longer arrive.
                shared.pending_streams.lock().unwrap().clear();
            }
            Err(_) => {
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
    }
}

fn read_until_disconnected(stream: UnixStream, shared: &Arc<Shared>) {
    loop {
        match read_frame(&stream) {
            Ok(Some(Frame::Response { id, response })) => {
                let ends_request = response.ends_request();
                // A plain `request()` caller wants exactly one answer: the
                // final one. Drop any partial frame for such a request
                // *without* retiring its correlation entry, so a caller
                // that used `request(Request::Search { .. })` still gets
                // the complete result set and simply never learns the
                // daemon answered in two parts. Only `request_streaming`
                // opts into seeing the partial.
                {
                    let mut pending = shared.pending.lock().unwrap();
                    if pending.contains_key(&id) {
                        if !ends_request {
                            continue;
                        }
                        if let Some(tx) = pending.remove(&id) {
                            let _ = tx.send(response);
                        }
                        continue;
                    }
                }
                // A streaming request's entry is retired only once a frame
                // says it ends the request — that rule lives in
                // `Response::ends_request`, on the protocol type, rather
                // than being re-derived from the payload shape here.
                let mut streams = shared.pending_streams.lock().unwrap();
                if let Some(tx) = streams.get(&id) {
                    let _ = tx.unbounded_send(response);
                    if ends_request {
                        streams.remove(&id);
                    }
                }
            }
            Ok(Some(Frame::Event(event))) => {
                let _ = shared.event_tx.send(event);
            }
            Ok(Some(Frame::Request { .. })) => {
                // The daemon never sends `Request` frames; ignore rather
                // than tear down the connection over a protocol surprise.
            }
            Ok(None) | Err(_) => return,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    fn temp_socket_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "neko-client-test-{}-{}.sock",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn request_before_any_connection_exists_returns_not_connected_immediately() {
        let path = temp_socket_path();
        let (client, _events) = NekoClient::connect(path);
        let result = futures::executor::block_on(client.request(Request::Ping));
        assert!(matches!(result, Err(ClientError::NotConnected)));
    }

    #[test]
    fn a_ping_round_trips_once_a_daemon_like_listener_is_up() {
        let path = temp_socket_path();
        let listener = UnixListener::bind(&path).unwrap();

        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            loop {
                match read_frame(&stream) {
                    Ok(Some(Frame::Request { id, request: Request::Ping })) => {
                        let mut writer = stream.try_clone().unwrap();
                        write_frame(&mut writer, &Frame::Response { id, response: Response::Pong }).unwrap();
                    }
                    _ => return,
                }
            }
        });

        let (client, _events) = NekoClient::connect(path);
        // The reconnect supervisor polls on a short backoff; give it a
        // moment to dial in before asserting on the round trip.
        std::thread::sleep(Duration::from_millis(200));
        let result = futures::executor::block_on(client.request(Request::Ping));
        assert!(matches!(result, Ok(Response::Pong)));
    }

    /// A daemon-like listener that answers a `Search` the way the real one
    /// does for a query with a deferred provider: a partial frame, a beat,
    /// then the complete one — both on the same request id.
    fn spawn_two_phase_search_listener(listener: UnixListener) {
        std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            loop {
                match read_frame(&stream) {
                    Ok(Some(Frame::Request { id, request: Request::Search { .. } })) => {
                        let mut writer = stream.try_clone().unwrap();
                        write_frame(
                            &mut writer,
                            &Frame::Response {
                                id,
                                response: Response::SearchResults { items: Vec::new(), complete: false },
                            },
                        )
                        .unwrap();
                        std::thread::sleep(Duration::from_millis(20));
                        write_frame(
                            &mut writer,
                            &Frame::Response {
                                id,
                                response: Response::SearchResults { items: Vec::new(), complete: true },
                            },
                        )
                        .unwrap();
                    }
                    _ => return,
                }
            }
        });
    }

    #[test]
    fn a_streaming_request_yields_every_frame_then_ends() {
        let path = temp_socket_path();
        let _ = std::fs::remove_file(&path);
        spawn_two_phase_search_listener(UnixListener::bind(&path).unwrap());

        let (client, _events) = NekoClient::connect(path);
        assert!(wait_until(|| client.is_connected(), Duration::from_secs(2)));

        let mut stream =
            client.request_streaming(Request::Search { query: "do".into(), limit: 8, provider: None });
        futures::executor::block_on(async {
            assert!(
                matches!(stream.next().await, Some(Response::SearchResults { complete: false, .. })),
                "the partial frame must reach the caller, not be swallowed"
            );
            assert!(matches!(stream.next().await, Some(Response::SearchResults { complete: true, .. })));
            assert!(stream.next().await.is_none(), "the stream ends after the frame that ends the request");
        });
    }

    #[test]
    fn a_plain_request_for_a_two_phase_search_still_resolves_with_the_complete_answer() {
        // Callers that predate streaming (and the `query_probe` example)
        // must keep working unchanged: the partial frame is dropped, not
        // mistaken for the answer, and the correlation entry survives to
        // receive the real one.
        let path = temp_socket_path();
        let _ = std::fs::remove_file(&path);
        spawn_two_phase_search_listener(UnixListener::bind(&path).unwrap());

        let (client, _events) = NekoClient::connect(path);
        assert!(wait_until(|| client.is_connected(), Duration::from_secs(2)));

        let result = futures::executor::block_on(
            client.request(Request::Search { query: "do".into(), limit: 8, provider: None }),
        );
        assert!(
            matches!(result, Ok(Response::SearchResults { complete: true, .. })),
            "expected the complete frame, got {result:?}"
        );
    }

    #[test]
    fn a_streaming_request_with_no_live_connection_ends_immediately() {
        let path = temp_socket_path();
        let (client, _events) = NekoClient::connect(path);
        let mut stream =
            client.request_streaming(Request::Search { query: "do".into(), limit: 8, provider: None });
        assert!(futures::executor::block_on(stream.next()).is_none());
    }

    #[test]
    fn is_connected_reflects_the_daemon_dying_without_any_request_being_made() {
        // The exact defect this exists to fix (`AGENTS.md`, "Reliability /
        // error states"): a caller must be able to learn the daemon died
        // even if nothing ever sends another request afterward — no
        // keystroke, no poll-triggered search, nothing.
        let path = temp_socket_path();
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();

        let (client, _events) = NekoClient::connect(path.clone());
        assert!(!client.is_connected(), "not connected before the daemon-like listener ever accepts");

        let (stream, _) = listener.accept().unwrap();
        assert!(
            wait_until(|| client.is_connected(), Duration::from_secs(2)),
            "connected once the supervisor's dial-in succeeds"
        );

        // Simulates the daemon dying: close the accepted end, which
        // delivers EOF to the client's reader thread with no request in
        // flight and no further keystroke to provoke a failure.
        drop(stream);
        drop(listener);
        assert!(
            wait_until(|| !client.is_connected(), Duration::from_secs(2)),
            "disconnected the moment the socket closes, proactively"
        );
    }

    /// Polls `condition` with a short interval instead of a single fixed
    /// sleep — this test asserts on the reconnect supervisor's own
    /// background-thread timing, and a fixed sleep tight enough to be fast
    /// is also tight enough to flake under real system load (seen once in
    /// a full `cargo test --workspace` run on a busy multi-agent machine, a
    /// generous fixed sleep still isn't a guarantee). A 2s budget is far
    /// past the supervisor's own 50ms initial backoff either way.
    fn wait_until(mut condition: impl FnMut() -> bool, timeout: Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if condition() {
                return true;
            }
            if std::time::Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
