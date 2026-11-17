//! The connection to the daemon: the daemon is the brain and the only writer, this client only
//! listens (docs/decisions/0002-rune-for-everything.md, P0).
//!
//! * **Two kinds of traffic, both listed.** The held snapshot connection here sends only the two
//!   reads in [`ReadRequest`]: the whole workbench once, then `WorkbenchIfChanged` held up to
//!   20 s, which is how the web UI stays current (`web/src/api.ts`). Everything else the app asks
//!   (the open task's runs, a message to an agent, a goal's actions, ...) is a [`Call`] in
//!   `write.rs`, one request the web UI sends too. `tests/allow_list.rs` names exactly the
//!   requests this crate may send, and no other `Request` or `Command` is named anywhere outside
//!   `link/`. The client never opens the database.
//! * **Reconnects with backoff** ([`backoff`]): 0.5 s, 1, 2, 4, 8, then every 10 s, reset by the
//!   first answer. It never gives up, because the daemon may simply not be running yet.
//! * **Calm errors** ([`calm`]): one sentence in the product's voice, never a raw OS message.
//!
//! The worker ([`run`]) is generic over [`Connector`] and a sleep function, so its rules are
//! tested with a fake transport and a recorded clock, and the real socket is tested against a
//! fake server in `link/unix.rs`.

pub mod http;
mod model;
mod pages;
mod prs;
mod reply;
mod setup;
mod unix;
pub mod write;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use neko_protocol::workbench::Command;
use neko_protocol::{Request, Response};

pub use model::*;
pub use pages::*;
pub use prs::*;
pub use reply::*;
pub use setup::*;
pub use write::Call;
pub use unix::UnixConnector;

/// How long the daemon holds `WorkbenchIfChanged` while nothing changes (the web UI's value).
pub const HOLD_MS: u64 = 20_000;
/// The least time between two asks, so a daemon that answers at once can never make a busy loop.
pub const MIN_SPACING: Duration = Duration::from_millis(250);

/// The only things this client asks of the daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadRequest {
    /// The whole workbench.
    Snapshot,
    /// The workbench unless it is still what `digest` names; held up to `wait_ms`.
    IfChanged { digest: String, wait_ms: u64 },
}

impl ReadRequest {
    pub fn wire(&self) -> Request {
        match self {
            ReadRequest::Snapshot => Request::Workbench(Command::Snapshot),
            ReadRequest::IfChanged { digest, wait_ms } => Request::WorkbenchIfChanged { digest: digest.clone(), wait_ms: *wait_ms },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// Nothing is listening: the daemon is not running (yet).
    NotRunning,
    /// The connection was refused or closed under us.
    Dropped,
    /// The daemon did not answer in time.
    Slow,
    /// The daemon answered something this client cannot read.
    Unreadable(String),
    /// The daemon said no, in its own words.
    Daemon(String),
}

impl LinkError {
    pub fn from_io(error: &io::Error) -> LinkError {
        use io::ErrorKind::*;
        match error.kind() {
            NotFound | ConnectionRefused | PermissionDenied | AddrNotAvailable => LinkError::NotRunning,
            TimedOut | WouldBlock => LinkError::Slow,
            InvalidData => LinkError::Unreadable(error.to_string()),
            _ => LinkError::Dropped,
        }
    }
}

/// The delay before attempt `n` (1 is the first retry): 0.5 s doubling to a ceiling of 10 s.
pub fn backoff(attempt: u32) -> Duration {
    let ms = 500u64.saturating_mul(1u64 << attempt.saturating_sub(1).min(5));
    Duration::from_millis(ms.min(10_000))
}

/// What the person reads when the link is down: what is true, then what happens next.
pub fn calm(error: &LinkError, retry_in: Duration) -> String {
    let retry = format!("Trying again in {} s.", retry_in.as_secs().max(1));
    match error {
        LinkError::NotRunning => format!("Neko isn't running. {retry}"),
        LinkError::Dropped => format!("The connection to Neko dropped. {retry}"),
        LinkError::Slow => format!("Neko is slow to answer. {retry}"),
        LinkError::Unreadable(_) => format!("Neko answered in a way this app doesn't understand. It may need an update. {retry}"),
        LinkError::Daemon(message) => format!("Neko said: {message} {retry}"),
    }
}

/// What the worker tells the app. (One per snapshot, moved once, so `Live` is not boxed.)
#[derive(Clone, Debug, PartialEq, Eq)]
#[allow(clippy::large_enum_variant)]
pub enum LinkEvent {
    Connecting,
    /// The workbench, new or changed.
    Live(Summary),
    /// The daemon is there and nothing changed (a held poll ran out): the link is fine.
    Quiet,
    /// The daemon pushed `ShowArtifact`: an agent asked every window to show this artifact now.
    ShowArtifact { id: String, version: Option<u32> },
    /// Another event the daemon pushes (`AttentionChanged`, `OpenTab`, `ThemeChanged`, `QuotaChanged`).
    Pushed(Pushed),
    Down { error: LinkError, attempt: u32, retry_in: Duration },
}

/// What the daemon pushes to every connection that this client acts on. `WebUpdated` (the web UI rebuilt itself) and the
/// others (`IconsUpdated`, `HotkeyChanged`, `ArtifactsChanged`, ...) are not in it: they are ignored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Pushed {
    /// How many agents are stopped waiting for a person.
    Attention { count: usize },
    /// Open this route (`tasks?task=<id>&view=full`, `artifact?id=<id>`, a plugin page's `<plugin>:<page>`, a page's id).
    OpenTab { route: String },
    /// A client committed a theme.
    Theme { id: String },
    /// A vendor's quota moved.
    Quota,
}

/// The event as this client acts on it, or None for the ones it ignores.
pub fn pushed(event: &neko_protocol::Event) -> Option<Pushed> {
    use neko_protocol::Event;
    match event {
        Event::AttentionChanged { count } => Some(Pushed::Attention { count: *count }),
        Event::OpenTab { route } => Some(Pushed::OpenTab { route: route.clone() }),
        Event::ThemeChanged { id } => Some(Pushed::Theme { id: id.clone() }),
        Event::AppearanceChanged { .. } => Some(Pushed::Theme { id: String::new() }),
        Event::QuotaChanged { .. } => Some(Pushed::Quota),
        _ => None,
    }
}

/// A connection that can ask the daemon one read request at a time.
pub trait Transport {
    /// Asks one read request. The daemon also pushes events to every connection, even while a
    /// held request waits: each one is handed to `on_event` as it arrives, not after the answer.
    fn ask(&mut self, request: &ReadRequest, on_event: &mut dyn FnMut(neko_protocol::Event)) -> Result<Response, LinkError>;
}

pub trait Connector {
    type Conn: Transport;
    fn connect(&self) -> Result<Self::Conn, LinkError>;
}

/// The worker loop. Returns only when `stop` is set. `emit` gets every event, `sleep` is how
/// time passes (a recording fake in tests).
pub fn run<C: Connector>(connector: &C, stop: &AtomicBool, emit: impl FnMut(LinkEvent), sleep: impl Fn(Duration)) {
    run_with(connector, stop, &Wanted::default(), emit, sleep)
}

/// [`run`], with each snapshot summarised for the task `wanted` names (the one open in the app).
pub fn run_with<C: Connector>(connector: &C, stop: &AtomicBool, wanted: &Wanted, mut emit: impl FnMut(LinkEvent), sleep: impl Fn(Duration)) {
    let mut attempt: u32 = 0;
    let mut first = true;
    while !stop.load(Ordering::Relaxed) {
        if first {
            emit(LinkEvent::Connecting);
            first = false;
        }
        let failure = match connector.connect() {
            Ok(mut conn) => hold(&mut conn, stop, wanted, &mut emit, &sleep, &mut attempt),
            Err(error) => error,
        };
        if stop.load(Ordering::Relaxed) {
            return;
        }
        attempt += 1;
        let retry_in = backoff(attempt);
        emit(LinkEvent::Down { error: failure, attempt, retry_in });
        sleep(retry_in);
    }
}

/// Keep one connection current until it fails; the error that ended it.
fn hold<T: Transport>(conn: &mut T, stop: &AtomicBool, wanted: &Wanted, emit: &mut impl FnMut(LinkEvent), sleep: &impl Fn(Duration), attempt: &mut u32) -> LinkError {
    let mut digest = String::new();
    while !stop.load(Ordering::Relaxed) {
        let request = if digest.is_empty() { ReadRequest::Snapshot } else { ReadRequest::IfChanged { digest: digest.clone(), wait_ms: HOLD_MS } };
        let asked = Instant::now();
        let mut on_event = |event: neko_protocol::Event| {
            if let neko_protocol::Event::ShowArtifact { id, version } = event {
                emit(LinkEvent::ShowArtifact { id, version });
            } else if let Some(pushed) = pushed(&event) {
                emit(LinkEvent::Pushed(pushed));
            }
        };
        match conn.ask(&request, &mut on_event) {
            Ok(Response::Workbench(snapshot)) => {
                *attempt = 0;
                digest = snapshot.digest.clone();
                emit(LinkEvent::Live(Summary::from_snapshot_for(&snapshot, wanted.get().as_deref())));
            }
            Ok(Response::WorkbenchUnchanged { .. }) => {
                *attempt = 0;
                emit(LinkEvent::Quiet);
            }
            Ok(Response::Error { message }) => return LinkError::Daemon(message),
            Ok(other) => return LinkError::Unreadable(format!("unexpected answer: {}", format!("{other:?}").chars().take(60).collect::<String>())),
            Err(error) => return error,
        }
        // A daemon that answers at once never makes a busy loop.
        let took = asked.elapsed();
        if took < MIN_SPACING {
            sleep(MIN_SPACING - took);
        }
    }
    LinkError::Dropped
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::{Snapshot, TaskStatus};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn only_two_requests_exist_and_neither_writes() {
        // `Request` has no `PartialEq`; its JSON is the wire form anyway.
        let json = |r: Request| serde_json::to_string(&r).unwrap();
        assert_eq!(json(ReadRequest::Snapshot.wire()), json(Request::Workbench(Command::Snapshot)));
        assert_eq!(
            json(ReadRequest::IfChanged { digest: "d".into(), wait_ms: 5 }.wire()),
            json(Request::WorkbenchIfChanged { digest: "d".into(), wait_ms: 5 })
        );
    }

    #[test]
    fn backoff_doubles_to_a_ceiling_and_never_overflows() {
        let secs: Vec<f32> = (1..=8).map(|n| backoff(n).as_secs_f32()).collect();
        assert_eq!(secs, vec![0.5, 1.0, 2.0, 4.0, 8.0, 10.0, 10.0, 10.0]);
        assert_eq!(backoff(0), Duration::from_millis(500));
        assert_eq!(backoff(u32::MAX), Duration::from_secs(10));
    }

    #[test]
    fn errors_are_one_calm_sentence_with_what_happens_next() {
        let line = calm(&LinkError::NotRunning, Duration::from_secs(4));
        assert_eq!(line, "Neko isn't running. Trying again in 4 s.");
        for error in [
            LinkError::NotRunning,
            LinkError::Dropped,
            LinkError::Slow,
            LinkError::Unreadable("EOF while parsing".into()),
            LinkError::Daemon("busy".into()),
        ] {
            let line = calm(&error, Duration::from_millis(500));
            assert!(line.ends_with("Trying again in 1 s."), "{line}");
            assert!(!line.contains("EOF while parsing") && !line.to_lowercase().contains("errno"), "no raw error text: {line}");
        }
    }

    #[test]
    fn io_errors_map_to_causes() {
        let k = |kind| LinkError::from_io(&io::Error::from(kind));
        assert_eq!(k(io::ErrorKind::NotFound), LinkError::NotRunning);
        assert_eq!(k(io::ErrorKind::ConnectionRefused), LinkError::NotRunning);
        assert_eq!(k(io::ErrorKind::TimedOut), LinkError::Slow);
        assert_eq!(k(io::ErrorKind::BrokenPipe), LinkError::Dropped);
        assert!(matches!(k(io::ErrorKind::InvalidData), LinkError::Unreadable(_)));
    }

    struct Fake {
        script: RefCell<VecDeque<Result<Response, LinkError>>>,
        asked: RefCell<Vec<ReadRequest>>,
    }

    struct Conn<'a>(&'a Fake);

    impl Transport for Conn<'_> {
        fn ask(&mut self, request: &ReadRequest, _on_event: &mut dyn FnMut(neko_protocol::Event)) -> Result<Response, LinkError> {
            self.0.asked.borrow_mut().push(request.clone());
            self.0.script.borrow_mut().pop_front().unwrap_or(Err(LinkError::Dropped))
        }
    }

    struct Dial<'a> {
        fake: &'a Fake,
        /// Connection attempts that fail before one succeeds.
        refusals: RefCell<u32>,
        connects: RefCell<u32>,
        stop: &'a AtomicBool,
        stop_after_connects: u32,
    }

    impl<'a> Connector for Dial<'a> {
        type Conn = Conn<'a>;
        fn connect(&self) -> Result<Conn<'a>, LinkError> {
            if *self.connects.borrow() >= self.stop_after_connects {
                self.stop.store(true, Ordering::Relaxed);
            }
            *self.connects.borrow_mut() += 1;
            let mut refusals = self.refusals.borrow_mut();
            if *refusals > 0 {
                *refusals -= 1;
                return Err(LinkError::NotRunning);
            }
            Ok(Conn(self.fake))
        }
    }

    fn snapshot(digest: &str) -> Response {
        Response::Workbench(Snapshot { digest: digest.into(), ..Snapshot::default() })
    }

    fn drive(script: Vec<Result<Response, LinkError>>, refusals: u32, stop_after_connects: u32) -> (Vec<LinkEvent>, Vec<Duration>, Vec<ReadRequest>) {
        let fake = Fake { script: RefCell::new(script.into()), asked: RefCell::new(Vec::new()) };
        let stop = AtomicBool::new(false);
        let dial = Dial { fake: &fake, refusals: RefCell::new(refusals), connects: RefCell::new(0), stop: &stop, stop_after_connects };
        let events = RefCell::new(Vec::new());
        let sleeps = RefCell::new(Vec::new());
        run(&dial, &stop, |e| events.borrow_mut().push(e), |d| sleeps.borrow_mut().push(d));
        (events.into_inner(), sleeps.into_inner(), fake.asked.into_inner())
    }

    #[test]
    fn asks_for_everything_once_then_holds_with_the_digest() {
        let (events, _, asked) = drive(vec![Ok(snapshot("d1")), Ok(Response::WorkbenchUnchanged { digest: "d1".into() }), Ok(snapshot("d2"))], 0, 1);
        assert_eq!(asked[0], ReadRequest::Snapshot);
        assert_eq!(asked[1], ReadRequest::IfChanged { digest: "d1".into(), wait_ms: HOLD_MS });
        assert_eq!(asked[2], ReadRequest::IfChanged { digest: "d1".into(), wait_ms: HOLD_MS });
        assert!(matches!(&events[1], LinkEvent::Live(s) if s.digest == "d1"));
        assert_eq!(events[2], LinkEvent::Quiet);
        assert!(matches!(&events[3], LinkEvent::Live(s) if s.digest == "d2"));
    }

    #[test]
    fn retries_a_missing_daemon_with_growing_waits_and_then_connects() {
        let (events, sleeps, _) = drive(vec![Ok(snapshot("d1"))], 3, 4);
        let downs: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                LinkEvent::Down { error, attempt, retry_in } => Some((error.clone(), *attempt, *retry_in)),
                _ => None,
            })
            .collect();
        assert_eq!(downs[0], (LinkError::NotRunning, 1, Duration::from_millis(500)));
        assert_eq!(downs[1], (LinkError::NotRunning, 2, Duration::from_secs(1)));
        assert_eq!(downs[2], (LinkError::NotRunning, 3, Duration::from_secs(2)));
        assert!(events.iter().any(|e| matches!(e, LinkEvent::Live(_))), "and then it is live");
        assert!(sleeps.starts_with(&[Duration::from_millis(500), Duration::from_secs(1), Duration::from_secs(2)]));
    }

    #[test]
    fn a_dropped_connection_starts_over_with_a_full_snapshot() {
        let (events, _, asked) = drive(vec![Ok(snapshot("d1")), Err(LinkError::Dropped), Ok(snapshot("d9"))], 0, 2);
        assert!(events.iter().any(|e| matches!(e, LinkEvent::Down { error: LinkError::Dropped, attempt: 1, .. })));
        // The digest of the dead connection is not trusted on the new one.
        assert_eq!(asked[2], ReadRequest::Snapshot);
    }

    #[test]
    fn an_answer_resets_the_backoff() {
        let (events, ..) = drive(vec![Err(LinkError::Dropped), Ok(snapshot("d1")), Err(LinkError::Dropped)], 0, 3);
        let attempts: Vec<u32> = events
            .iter()
            .filter_map(|e| match e {
                LinkEvent::Down { attempt, .. } => Some(*attempt),
                _ => None,
            })
            .collect();
        assert_eq!(attempts[..2], [1, 1], "a good answer in between starts the count again");
    }

    #[test]
    fn a_daemon_that_says_no_is_reported_in_its_words_and_retried() {
        let (events, ..) = drive(vec![Ok(Response::Error { message: "busy".into() })], 0, 1);
        assert!(events.iter().any(|e| matches!(e, LinkEvent::Down { error: LinkError::Daemon(m), .. } if m == "busy")));
    }

    #[test]
    fn a_goal_keeps_its_first_line_cut_short() {
        assert_eq!(first_line("\n  Fix the bug\nthen more"), "Fix the bug");
        assert_eq!(first_line(""), "");
        let long = "x".repeat(300);
        assert_eq!(first_line(&long).chars().count(), 140);
    }

    #[test]
    fn summaries_count_what_needs_the_person() {
        let task = |id: &str, status| TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: 0, goal: String::new(), wait: Wait::None, progress: None };
        let s = Summary {
            tasks: vec![
                task("a", TaskStatus::AwaitingApproval),
                task("b", TaskStatus::Building),
                task("c", TaskStatus::Planning),
                task("d", TaskStatus::ReadyForReview),
                task("e", TaskStatus::Completed),
            ],
            ..Summary::default()
        };
        assert_eq!((s.needs_you(), s.working(), s.in_review()), (2, 2, 1));
    }
}
