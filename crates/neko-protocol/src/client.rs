//! A screen's side of the link: finding the daemon, a blocking [`Client`] (tests, tools) and a threaded [`Link`] (the app).

use std::collections::VecDeque;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, TcpStream};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant};

use tungstenite::client::IntoClientRequest;
use tungstenite::{Message, WebSocket};

use crate::pump::{self, Flow, Out, Wake};
use crate::{ClientMessage, DaemonInfo, PROTOCOL_VERSION, Reply, Request, ServerMessage, WS_PATH, files};

/// How long a connection may take to open and answer Hello.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// The running daemon of `data_dir` as its files describe it: its address and the token. An error when it is not running (or
/// its files are unreadable); a stale `daemon.json` left by a crash is found out by connecting.
pub fn find(data_dir: &Path) -> io::Result<(DaemonInfo, String)> {
    let info: DaemonInfo = serde_json::from_slice(&std::fs::read(data_dir.join(files::DAEMON))?).map_err(io::Error::other)?;
    let token = std::fs::read_to_string(data_dir.join(files::TOKEN))?.trim().to_owned();
    Ok((info, token))
}

fn handshake(port: u16, token: &str) -> io::Result<WebSocket<TcpStream>> {
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, CONNECT_TIMEOUT)?;
    stream.set_nodelay(true)?;
    stream.set_read_timeout(Some(CONNECT_TIMEOUT))?;
    let mut req = format!("ws://127.0.0.1:{port}{WS_PATH}").into_client_request().map_err(io::Error::other)?;
    let bearer = format!("Bearer {token}").parse().map_err(io::Error::other)?;
    req.headers_mut().insert("authorization", bearer);
    match tungstenite::client::client(req, stream) {
        Ok((ws, _)) => Ok(ws),
        Err(tungstenite::HandshakeError::Failure(tungstenite::Error::Http(resp))) => {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, format!("the daemon refused the connection ({})", resp.status())))
        }
        Err(e) => Err(io::Error::other(e.to_string())),
    }
}

fn send(ws: &mut WebSocket<TcpStream>, msg: &ClientMessage) -> io::Result<()> {
    ws.send(Message::text(msg.to_json())).map_err(|e| io::Error::other(e.to_string()))
}

fn hello(client: &str) -> ClientMessage {
    ClientMessage { id: 0, request: Request::Hello { client: client.to_owned(), protocol: PROTOCOL_VERSION } }
}

fn check_hello(reply: Reply) -> io::Result<Reply> {
    match reply {
        Reply::Hello { .. } => Ok(reply),
        Reply::Error { message, .. } => Err(io::Error::new(io::ErrorKind::InvalidData, message)),
        other => Err(io::Error::other(format!("unexpected answer to hello: {other:?}"))),
    }
}

/// A blocking client: one request at a time, events kept until asked for.
pub struct Client {
    ws: WebSocket<TcpStream>,
    next: u64,
    events: VecDeque<ServerMessage>,
    pub hello: Reply,
}

impl Client {
    /// Connects to the daemon at `port` with `token` and says hello.
    pub fn connect(port: u16, token: &str, client: &str) -> io::Result<Client> {
        let mut ws = handshake(port, token)?;
        send(&mut ws, &hello(client))?;
        let mut c = Client { ws, next: 1, events: VecDeque::new(), hello: Reply::Subscribed { revision: 0 } };
        let reply = c.wait_reply(0, CONNECT_TIMEOUT)?;
        c.hello = check_hello(reply)?;
        Ok(c)
    }

    /// Sends a request and waits for its reply.
    pub fn request(&mut self, request: Request) -> io::Result<Reply> {
        let id = self.next;
        self.next += 1;
        send(&mut self.ws, &ClientMessage { id, request })?;
        self.wait_reply(id, CONNECT_TIMEOUT)
    }

    /// The next pushed event, waiting up to `timeout`.
    pub fn next_event(&mut self, timeout: Duration) -> io::Result<Option<ServerMessage>> {
        if let Some(e) = self.events.pop_front() {
            return Ok(Some(e));
        }
        match self.read(timeout)? {
            Some(m @ ServerMessage::Event { .. }) => Ok(Some(m)),
            Some(_) | None => Ok(None),
        }
    }

    fn read(&mut self, timeout: Duration) -> io::Result<Option<ServerMessage>> {
        self.ws.get_ref().set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
        match self.ws.read() {
            Ok(Message::Text(t)) => serde_json::from_str(t.as_str()).map(Some).map_err(io::Error::other),
            Ok(_) => Ok(None),
            Err(tungstenite::Error::Io(e)) if matches!(e.kind(), io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut) => Ok(None),
            Err(e) => Err(io::Error::new(io::ErrorKind::ConnectionAborted, e.to_string())),
        }
    }

    fn wait_reply(&mut self, id: u64, timeout: Duration) -> io::Result<Reply> {
        let until = Instant::now() + timeout;
        loop {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "the daemon did not answer"));
            }
            match self.read(left)? {
                Some(ServerMessage::Reply { id: got, reply }) if got == id => return Ok(reply),
                Some(e @ ServerMessage::Event { .. }) => self.events.push_back(e),
                Some(_) | None => {}
            }
        }
    }

    /// Closes the connection cleanly.
    pub fn close(mut self) {
        let _ = self.ws.close(None);
        let _ = self.ws.flush();
    }
}

/// What a [`Link`] hands its owner, on the link's own thread.
#[derive(Debug)]
pub enum LinkEvent {
    Message(ServerMessage),
    /// The connection ended; the words say why. Nothing more comes.
    Closed(String),
}

/// A connection served by its own thread: requests go out at once, replies and events come back through a callback.
pub struct Link {
    tx: Sender<Out>,
    wake: Wake,
    next: AtomicU64,
    /// The daemon's answer to hello.
    pub hello: Reply,
}

impl Link {
    /// Connects, says hello (blocking, up to [`CONNECT_TIMEOUT`]) and starts the link's thread.
    pub fn open(port: u16, token: &str, client: &str, mut on_event: impl FnMut(LinkEvent) + Send + 'static) -> io::Result<Link> {
        let mut ws = handshake(port, token)?;
        send(&mut ws, &hello(client))?;
        let until = Instant::now() + CONNECT_TIMEOUT;
        let reply = loop {
            if Instant::now() > until {
                return Err(io::Error::new(io::ErrorKind::TimedOut, "the daemon did not answer hello"));
            }
            match ws.read() {
                Ok(Message::Text(t)) => match serde_json::from_str::<ServerMessage>(t.as_str()) {
                    Ok(ServerMessage::Reply { id: 0, reply }) => break check_hello(reply)?,
                    Ok(_) => {}
                    Err(e) => return Err(io::Error::other(e)),
                },
                Ok(_) => {}
                Err(e) => return Err(io::Error::other(e.to_string())),
            }
        };
        ws.get_ref().set_read_timeout(None)?;
        ws.get_ref().set_nonblocking(true)?;
        let (wake, wake_rx) = pump::wake_pair()?;
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new().name("neko-link".into()).spawn(move || {
            let fd = ws.get_ref().as_raw_fd();
            let result = pump::run(&mut ws, fd, &wake_rx, &rx, |text| {
                match serde_json::from_str::<ServerMessage>(&text) {
                    Ok(m) => on_event(LinkEvent::Message(m)),
                    Err(e) => on_event(LinkEvent::Closed(format!("the daemon sent something this version cannot read: {e}"))),
                }
                Flow::Continue
            });
            on_event(LinkEvent::Closed(match result {
                Ok(()) => "the daemon closed the connection".to_owned(),
                Err(e) => e.to_string(),
            }));
        })?;
        Ok(Link { tx, wake, next: AtomicU64::new(1), hello: reply })
    }

    /// Sends a request; its reply carries the id returned.
    pub fn send(&self, request: Request) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed);
        let _ = self.tx.send(Out::Text(ClientMessage { id, request }.to_json()));
        self.wake.wake();
        id
    }

    /// Ends the connection.
    pub fn close(&self) {
        let _ = self.tx.send(Out::Close);
        self.wake.wake();
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.close();
    }
}
