//! One WebSocket connection driven by one thread, with nothing waking it while nothing happens.
//!
//! The thread blocks in `poll(2)` on two descriptors: the socket, and the read end of a pipe ([`wake_pair`]). Whoever wants to send
//! puts a message in the outbox and calls [`Wake::wake`]; the thread wakes, writes it and goes back to sleep. A quiet connection
//! costs no CPU at all. Both ends of the link use this: the daemon for each screen, and a screen for its link to the daemon.

use std::io::{self, Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError};

use tungstenite::{Message, WebSocket};

/// What the outbox holds.
#[derive(Debug)]
pub enum Out {
    Text(String),
    /// Close the connection cleanly and end the pump.
    Close,
}

/// What the handler of an incoming text wants next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    Continue,
    Stop,
}

/// The sending side of the wake pipe: cheap to clone, safe from any thread.
#[derive(Clone, Debug)]
pub struct Wake(Arc<OwnedFd>);

/// The pump's side of the wake pipe.
#[derive(Debug)]
pub struct WakeRx(OwnedFd);

/// A non-blocking pipe: [`Wake`] writes a byte, the pump reads it and looks at its outbox.
pub fn wake_pair() -> io::Result<(Wake, WakeRx)> {
    let mut fds = [0; 2];
    // SAFETY: `fds` is two ints, as pipe(2) wants.
    if unsafe { libc::pipe(fds.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    for fd in fds {
        // SAFETY: fd was just returned by pipe(2) and is open.
        unsafe {
            libc::fcntl(fd, libc::F_SETFL, libc::fcntl(fd, libc::F_GETFL) | libc::O_NONBLOCK);
            libc::fcntl(fd, libc::F_SETFD, libc::FD_CLOEXEC);
        }
    }
    // SAFETY: both descriptors are open and owned by nothing else.
    let (r, w) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
    Ok((Wake(Arc::new(w)), WakeRx(r)))
}

impl Wake {
    /// Wakes the pump. A full pipe already holds a wake, so a failed write changes nothing.
    pub fn wake(&self) {
        let byte = [1u8];
        // SAFETY: writes one byte from a live buffer to an open descriptor.
        let _ = unsafe { libc::write(self.0.as_raw_fd(), byte.as_ptr().cast(), 1) };
    }
}

impl WakeRx {
    fn drain(&self) {
        let mut buf = [0u8; 64];
        // SAFETY: reads into a live buffer of its own length from an open non-blocking descriptor.
        while unsafe { libc::read(self.0.as_raw_fd(), buf.as_mut_ptr().cast(), buf.len()) } > 0 {}
    }
}

fn io_err(e: tungstenite::Error) -> io::Error {
    match e {
        tungstenite::Error::Io(e) => e,
        other => io::Error::other(other.to_string()),
    }
}

fn would_block(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::Io(io) if io.kind() == io::ErrorKind::WouldBlock)
}

fn closed(e: &tungstenite::Error) -> bool {
    matches!(e, tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed)
}

/// Runs the connection until either side closes it, the outbox says [`Out::Close`] or is dropped, or `on_text` says
/// [`Flow::Stop`] (after what it queued is sent). `socket` is the stream's descriptor; it must already be non-blocking.
pub fn run<S: Read + Write>(ws: &mut WebSocket<S>, socket: RawFd, wake: &WakeRx, outbox: &Receiver<Out>, mut on_text: impl FnMut(String) -> Flow) -> io::Result<()> {
    let mut stop = false;
    loop {
        loop {
            match outbox.try_recv() {
                Ok(Out::Text(t)) => match ws.write(Message::text(t)) {
                    Ok(()) => {}
                    // Buffered: the flush below finishes it when the socket can take it.
                    Err(e) if would_block(&e) => {}
                    Err(e) if closed(&e) => return Ok(()),
                    Err(e) => return Err(io_err(e)),
                },
                Ok(Out::Close) | Err(TryRecvError::Disconnected) => {
                    let _ = ws.close(None);
                    let _ = ws.flush();
                    return Ok(());
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        let want_write = match ws.flush() {
            Ok(()) => false,
            Err(e) if would_block(&e) => true,
            Err(e) if closed(&e) => return Ok(()),
            Err(e) => return Err(io_err(e)),
        };
        if stop {
            // What the last handler queued (its reply) has been written above; now close.
            let _ = ws.close(None);
            let _ = ws.flush();
            return Ok(());
        }
        let mut fds = [
            libc::pollfd { fd: socket, events: libc::POLLIN | if want_write { libc::POLLOUT } else { 0 }, revents: 0 },
            libc::pollfd { fd: wake.0.as_raw_fd(), events: libc::POLLIN, revents: 0 },
        ];
        // SAFETY: two live pollfd structs; -1 waits until one is ready.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), 2, -1) };
        if n < 0 {
            let e = io::Error::last_os_error();
            if e.kind() == io::ErrorKind::Interrupted {
                continue;
            }
            return Err(e);
        }
        if fds[1].revents != 0 {
            wake.drain();
        }
        if fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
            loop {
                match ws.read() {
                    Ok(Message::Text(t)) => {
                        if on_text(t.as_str().to_owned()) == Flow::Stop {
                            stop = true;
                            break;
                        }
                    }
                    Ok(_) => {}
                    Err(e) if would_block(&e) => break,
                    Err(e) if closed(&e) => return Ok(()),
                    Err(e) => return Err(io_err(e)),
                }
            }
        }
    }
}
