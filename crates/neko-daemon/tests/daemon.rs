//! The real daemon binary on a temporary data folder: the token, loopback only, pushed events, one instance per folder, and a
//! clean shutdown. Every daemon a test starts is stopped by its PID when the test ends, pass or fail.

use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpStream, UdpSocket};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use neko_protocol::client::{Client, find};
use neko_protocol::{Change, ErrorCode, Reply, Request, ServerMessage, files};

fn temp_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    // The test's name keeps parallel tests apart; the time keeps runs apart.
    let dir = std::env::temp_dir().join(format!("neko-daemon-test-{name}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A daemon this test started, stopped (by its PID) and its folder removed on drop.
struct Daemon {
    child: Child,
    dir: PathBuf,
    keep_dir: bool,
}

fn spawn(dir: &Path) -> Child {
    Command::new(env!("CARGO_BIN_EXE_neko-daemon")).arg("--data-dir").arg(dir).stdout(Stdio::null()).stderr(Stdio::piped()).spawn().unwrap()
}

impl Daemon {
    fn start(name: &str) -> Daemon {
        Daemon::start_in(temp_dir(name))
    }

    fn start_in(dir: PathBuf) -> Daemon {
        let child = spawn(&dir);
        let d = Daemon { child, dir, keep_dir: false };
        d.wait_ready();
        d
    }

    fn wait_ready(&self) {
        let until = Instant::now() + Duration::from_secs(20);
        while Instant::now() < until {
            if let Ok((info, _)) = find(&self.dir)
                && info.pid == self.child.id()
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        panic!("the daemon did not start");
    }

    fn port(&self) -> u16 {
        find(&self.dir).unwrap().0.port
    }

    fn token(&self) -> String {
        find(&self.dir).unwrap().1
    }

    fn client(&self) -> Client {
        Client::connect(self.port(), &self.token(), "test").unwrap()
    }

    fn signal(&self, sig: i32) {
        // SAFETY: signals the child this test spawned, by its PID.
        unsafe { libc::kill(self.child.id() as i32, sig) };
    }

    /// Sends SIGTERM and waits for the exit status.
    fn stop(&mut self) -> std::process::ExitStatus {
        self.signal(libc::SIGTERM);
        let until = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(Instant::now() < until, "the daemon did not stop on SIGTERM");
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Daemon {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        if !self.keep_dir {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }
}

/// A raw HTTP request; returns the status line.
fn http(port: u16, lines: &[String]) -> String {
    let mut s = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
    write!(s, "{}\r\n\r\n", lines.join("\r\n")).unwrap();
    let mut out = String::new();
    let _ = s.read_to_string(&mut out);
    out.lines().next().unwrap_or("").to_owned()
}

#[test]
fn every_request_needs_the_token() {
    let d = Daemon::start("token");
    let (port, token) = (d.port(), d.token());
    let host = format!("Host: 127.0.0.1:{port}");
    assert_eq!(http(port, &["GET /v1/hello HTTP/1.1".into(), host.clone()]), "HTTP/1.1 401 Unauthorized");
    assert_eq!(http(port, &["GET /v1/hello HTTP/1.1".into(), host.clone(), "Authorization: Bearer nope".into()]), "HTTP/1.1 401 Unauthorized");
    assert_eq!(http(port, &["GET /v1/hello HTTP/1.1".into(), host.clone(), format!("Authorization: Bearer {token}")]), "HTTP/1.1 200 OK");
    assert_eq!(http(port, &["GET /v1/hello HTTP/1.1".into(), host, format!("Authorization: Bearer {token}"), "Origin: https://example.com".into()]), "HTTP/1.1 403 Forbidden");
    let refused = Client::connect(port, "wrong-token", "test").err().expect("a wrong token is refused");
    assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied, "{refused}");
    assert!(d.client().request(Request::ListTasks).is_ok(), "the right token works");
    // The token file is the owner's alone, and so are the folder and the address file.
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&d.dir.join(files::TOKEN)), 0o600);
    assert_eq!(mode(&d.dir.join(files::DAEMON)), 0o600);
    assert_eq!(mode(&d.dir), 0o700);
}

#[test]
fn it_listens_on_loopback_only() {
    let d = Daemon::start("loopback");
    let port = d.port();
    assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_ok());
    // This machine's address on its network, found without sending anything (a UDP "connect" only picks a route).
    let lan = UdpSocket::bind("0.0.0.0:0").and_then(|s| s.connect("192.0.2.1:9").map(|()| s)).and_then(|s| s.local_addr()).map(|a| a.ip());
    match lan {
        Ok(ip @ IpAddr::V4(v4)) if !v4.is_loopback() && !v4.is_unspecified() => {
            let reached = TcpStream::connect_timeout(&SocketAddr::new(ip, port), Duration::from_secs(2));
            assert!(reached.is_err(), "the daemon must not answer on {ip}");
        }
        _ => eprintln!("no network address on this machine; checked loopback only"),
    }
}

#[test]
fn changes_are_pushed_to_subscribed_screens() {
    let d = Daemon::start("events");
    let mut watcher = d.client();
    let Reply::Subscribed { revision } = watcher.request(Request::Subscribe).unwrap() else { panic!("subscribe") };
    assert_eq!(revision, 0);
    let mut writer = d.client();
    let Reply::Task { task } = writer.request(Request::CreateTask { words: "Fix the build".into() }).unwrap() else { panic!("create") };
    let event = watcher.next_event(Duration::from_secs(5)).unwrap().expect("an event");
    assert_eq!(event, ServerMessage::Event { revision: 1, change: Change::TaskCreated { task: task.clone() } });
    assert!(writer.next_event(Duration::from_millis(200)).unwrap().is_none(), "a screen that did not subscribe gets no events");
    let Reply::Tasks { tasks } = watcher.request(Request::ListTasks).unwrap() else { panic!("list") };
    assert_eq!(tasks, vec![task.clone()]);
    assert_eq!(watcher.request(Request::GetTask { task: task.id.clone() }).unwrap(), Reply::Task { task });
}

#[test]
fn bad_requests_get_reasons() {
    let d = Daemon::start("errors");
    let mut c = d.client();
    let empty = c.request(Request::CreateTask { words: "   ".into() }).unwrap();
    assert!(matches!(empty, Reply::Error { code: ErrorCode::InvalidWords, .. }), "{empty:?}");
    let missing = c.request(Request::GetTask { task: neko_protocol::TaskId("task_none".into()) }).unwrap();
    assert!(matches!(missing, Reply::Error { code: ErrorCode::NotFound, .. }), "{missing:?}");
}

#[test]
fn a_second_daemon_for_the_same_folder_is_refused() {
    let d = Daemon::start("second");
    let mut second = spawn(&d.dir);
    let status = second.wait().unwrap();
    assert_eq!(status.code(), Some(3), "the second daemon exits with 3");
    let mut why = String::new();
    second.stderr.take().unwrap().read_to_string(&mut why).unwrap();
    assert!(why.contains("another neko-daemon is running"), "{why}");
    assert_eq!(find(&d.dir).unwrap().0.pid, d.child.id(), "the first one's address is untouched");
    assert!(d.client().request(Request::ListTasks).is_ok(), "the first one still serves");
}

#[test]
fn it_shuts_down_cleanly_and_the_data_survives() {
    let mut d = Daemon::start("shutdown");
    let mut c = d.client();
    let Reply::Task { task } = c.request(Request::CreateTask { words: "Keep this".into() }).unwrap() else { panic!("create") };
    let status = d.stop();
    assert!(status.success(), "SIGTERM is a clean exit: {status:?}");
    assert!(!d.dir.join(files::DAEMON).exists(), "the address file is removed");
    assert!(!d.dir.join(format!("{}-wal", files::STORE)).exists() || std::fs::metadata(d.dir.join(format!("{}-wal", files::STORE))).unwrap().len() == 0, "the log was checkpointed");
    assert!(c.request(Request::ListTasks).is_err(), "the old connection was closed");
    d.keep_dir = true;
    let dir = d.dir.clone();
    drop(d);
    let again = Daemon::start_in(dir);
    let Reply::Tasks { tasks } = again.client().request(Request::ListTasks).unwrap() else { panic!("list") };
    assert_eq!(tasks, vec![task], "the task is still there after a restart");
}

#[test]
fn another_protocol_version_is_refused_at_hello() {
    let d = Daemon::start("version");
    let mut ws = {
        let mut req = tungstenite::client::IntoClientRequest::into_client_request(format!("ws://127.0.0.1:{}/v1/ws", d.port())).unwrap();
        req.headers_mut().insert("authorization", format!("Bearer {}", d.token()).parse().unwrap());
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, d.port())).unwrap();
        tungstenite::client::client(req, stream).unwrap().0
    };
    ws.send(tungstenite::Message::text(r#"{"id":0,"type":"list_tasks"}"#)).unwrap();
    let first: ServerMessage = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
    assert!(matches!(first, ServerMessage::Reply { reply: Reply::Error { code: ErrorCode::HelloFirst, .. }, .. }), "{first:?}");
    ws.send(tungstenite::Message::text(r#"{"id":1,"type":"hello","client":"old","protocol":99}"#)).unwrap();
    let reply: ServerMessage = serde_json::from_str(ws.read().unwrap().to_text().unwrap()).unwrap();
    assert!(matches!(reply, ServerMessage::Reply { id: 1, reply: Reply::Error { code: ErrorCode::ProtocolMismatch, .. } }), "{reply:?}");
    let closed = (0..3).map(|_| ws.read()).any(|r| r.is_err() || matches!(r, Ok(tungstenite::Message::Close(_))));
    assert!(closed, "the connection is closed after a mismatch");
}
