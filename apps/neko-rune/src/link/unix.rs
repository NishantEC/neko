//! The real transport: the daemon's Unix socket with `neko_protocol`'s own framing (a 4-byte
//! length and a JSON body), as `neko-client` and the chat spike use it. The web UI reaches the
//! same requests through an HTTP route that wants a session cookie; a native client takes the
//! socket, which only its owner can open (mode 0600).

use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use neko_protocol::{Frame, Response, read_frame, write_frame};

use super::{Connector, LinkError, ReadRequest, Transport};

/// A little over the daemon's hold, so a quiet 20 s poll is answered before this gives up.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

pub struct UnixConnector {
    path: PathBuf,
}

impl UnixConnector {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        UnixConnector { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

pub struct UnixConn {
    stream: UnixStream,
    next_id: u64,
}

impl Connector for UnixConnector {
    type Conn = UnixConn;

    fn connect(&self) -> Result<UnixConn, LinkError> {
        let stream = UnixStream::connect(&self.path).map_err(|e| LinkError::from_io(&e))?;
        stream.set_read_timeout(Some(READ_TIMEOUT)).map_err(|e| LinkError::from_io(&e))?;
        stream.set_write_timeout(Some(WRITE_TIMEOUT)).map_err(|e| LinkError::from_io(&e))?;
        Ok(UnixConn { stream, next_id: 1 })
    }
}

impl Transport for UnixConn {
    fn ask(&mut self, request: &ReadRequest) -> Result<Response, LinkError> {
        let id = self.next_id;
        self.next_id += 1;
        write_frame(&mut self.stream, &Frame::Request { id, request: request.wire() }).map_err(|e| LinkError::from_io(&e))?;
        loop {
            match read_frame(&mut self.stream) {
                Ok(Some(Frame::Response { id: answered, response })) if answered == id => return Ok(response),
                // Events the daemon pushes to every connection, and answers to nothing we asked.
                Ok(Some(_)) => continue,
                Ok(None) => return Err(LinkError::Dropped),
                Err(e) => return Err(LinkError::from_io(&e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{LinkEvent, run};
    use neko_protocol::workbench::{Command, Snapshot};
    use neko_protocol::{Event, Request};
    use std::os::unix::net::UnixListener;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    fn socket_path(name: &str) -> PathBuf {
        // sockaddr_un paths are short on macOS, so keep it directly under the temp dir.
        std::env::temp_dir().join(format!("nrl-{name}-{}.sock", std::process::id()))
    }

    /// A stand-in daemon: answers a snapshot to `Snapshot`, holds `WorkbenchIfChanged` as the real
    /// one does, pushes an Event first, and records every request it saw.
    fn serve(listener: UnixListener, seen: Arc<Mutex<Vec<Request>>>, drop_after: usize) {
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let seen = seen.clone();
                std::thread::spawn(move || {
                    write_frame(&mut stream, &Frame::Event(Event::AttentionChanged { count: 2 })).ok();
                    let mut answered = 0;
                    while let Ok(Some(Frame::Request { id, request })) = read_frame(&stream) {
                        seen.lock().unwrap().push(request.clone());
                        if answered >= drop_after {
                            return; // the daemon goes away mid-conversation
                        }
                        answered += 1;
                        let response = match request {
                            Request::Workbench(Command::Snapshot) => Response::Workbench(Snapshot { digest: "one".into(), ..Snapshot::default() }),
                            Request::WorkbenchIfChanged { digest, .. } => Response::WorkbenchUnchanged { digest },
                            _ => Response::Error { message: "not for this test".into() },
                        };
                        write_frame(&mut stream, &Frame::Event(Event::AttentionChanged { count: 3 })).ok();
                        write_frame(&mut stream, &Frame::Response { id, response }).ok();
                    }
                });
            }
        });
    }

    #[test]
    fn talks_to_a_socket_skipping_pushed_events_and_reconnects_after_a_drop() {
        let path = socket_path("talk");
        let _ = std::fs::remove_file(&path);
        let listener = UnixListener::bind(&path).unwrap();
        let seen = Arc::new(Mutex::new(Vec::new()));
        serve(listener, seen.clone(), 2);

        let connector = UnixConnector::new(&path);
        let stop = AtomicBool::new(false);
        let mut events = Vec::new();
        run(
            &connector,
            &stop,
            |e| {
                let enough = events.iter().filter(|e| matches!(e, LinkEvent::Live(_))).count() >= 2;
                events.push(e);
                if enough {
                    stop.store(true, Ordering::Relaxed);
                }
            },
            |d| std::thread::sleep(d.min(Duration::from_millis(20))),
        );
        std::fs::remove_file(&path).ok();

        assert_eq!(events[0], LinkEvent::Connecting);
        assert!(matches!(&events[1], LinkEvent::Live(s) if s.digest == "one"));
        assert_eq!(events[2], LinkEvent::Quiet, "a held poll that runs out says the link is fine");
        assert!(events.iter().any(|e| matches!(e, LinkEvent::Down { error: LinkError::Dropped, attempt: 1, .. })), "the drop is noticed");
        assert!(events.iter().filter(|e| matches!(e, LinkEvent::Live(_))).count() >= 2, "and a full snapshot arrives on the new connection");

        let seen = seen.lock().unwrap();
        assert!(matches!(seen[0], Request::Workbench(Command::Snapshot)));
        assert!(seen.iter().all(|r| matches!(r, Request::Workbench(Command::Snapshot) | Request::WorkbenchIfChanged { .. })), "only reads: {seen:?}");
    }

    #[test]
    fn a_missing_socket_is_not_running() {
        let connector = UnixConnector::new(socket_path("missing"));
        assert!(matches!(connector.connect(), Err(LinkError::NotRunning)));
    }
}
