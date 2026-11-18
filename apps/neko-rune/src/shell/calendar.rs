//! Calendar from EventKit: a port of `CalendarBridge.swift`. Sends the person's macOS Calendar to the daemon for the Calendar page.
//!
//! * Only while the daemon says Calendar is switched on (`CalendarStatus.enabled`); macOS is asked for access, once, only then. While it
//!   is off the switch is looked at every 20 s.
//! * Every 5 minutes, and 2 seconds after the last `EKEventStoreChanged` of a burst, the events from local midnight 7 days ago to local
//!   midnight 35 days ahead (at most 2500 read) go as `SetCalendarEvents { events, source: "macOS Calendar", access }`; the daemon writes
//!   nothing when they are unchanged. Access is `Granted` for full access, `NotAsked` while undecided and `Denied` for everything else;
//!   access granted later in System Settings makes the next sync reset the store so it re-reads.
//! * A repeating event's occurrences share an identifier, so an event's id is `<identifier>@<start seconds>`. Its link is its own `http(s)`
//!   URL, else the first meeting link in its location or notes, else a location that is itself a web link.
//! * A duty (`shell::Duties::calendar`): only in `--app` mode, never while the Swift app runs (its `CalendarBridge` would be a second
//!   writer). Handed back, the loop goes quiet and forgets what it had scheduled; taken again, it starts with a cycle at once.
//! * Only from an app bundle that carries `NSCalendarsFullAccessUsageDescription` (`eventkit::usage_ready`); otherwise one log line and no thread.
//!
//! EventKit is behind [`CalendarSource`] and the daemon behind [`CalendarDaemon`], so the mapping and the schedule ([`Bridge`], `schedule::Runner`) are
//! tested with fakes. The EventKit one ([`native`]) is thin and is never run in a test.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use neko_protocol::calendar::{CalendarAccess, CalendarEvent};

use super::eventkit::Access;
use super::schedule::{self, Job};
use crate::clock;

/// How often events are sent (`CalendarBridge.interval`).
pub const INTERVAL: Duration = Duration::from_secs(300);
/// How often to look for the switch while Calendar is off (`idle`).
pub const IDLE: Duration = Duration::from_secs(20);
/// The most events read (the daemon keeps 2000; a little more is read so its cap decides).
pub const MAX_EVENTS: usize = 2500;
/// What the daemon is told the events came from.
pub const SOURCE: &str = "macOS Calendar";

const CONFERENCE_HOSTS: [&str; 12] = ["meet.google.com", "zoom.us", "zoom.com", "teams.microsoft.com", "teams.live.com", "webex.com", "whereby.com", "meet.jit.si", "chime.aws", "facetime.apple.com", "gotomeeting.com", "around.co"];

/// An event as EventKit gives it, before it is mapped. Times are seconds since 1970; a missing start is "now" and a missing end is the start.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RawEvent {
    /// `eventIdentifier`, else `calendarItemIdentifier`.
    pub identifier: String,
    pub calendar: String,
    /// `#RRGGBB` of the calendar's colour in sRGB, or empty.
    pub color: String,
    pub title: String,
    pub start: Option<f64>,
    pub end: Option<f64>,
    pub all_day: bool,
    pub location: Option<String>,
    pub url: Option<String>,
    pub notes: Option<String>,
}

/// What the bridge needs of EventKit.
pub trait CalendarSource {
    fn access(&mut self) -> Access;
    /// Shows the system prompt (only ever called while access is `NotAsked`) and waits for the answer.
    fn request_access(&mut self);
    /// Drops what the store cached (access was granted later in System Settings).
    fn reset(&mut self);
    /// The events between the two times (ms), at most `limit`.
    fn events(&mut self, from_ms: i64, to_ms: i64, limit: usize) -> Vec<RawEvent>;
    /// The web addresses found in free text (`NSDataDetector`'s links).
    fn links(&mut self, text: &str) -> Vec<String>;
}

/// What the bridge needs of the daemon.
pub trait CalendarDaemon {
    /// `CalendarStatus.enabled`; false when the daemon did not answer.
    fn enabled(&mut self) -> bool;
    fn send(&mut self, events: Vec<CalendarEvent>, access: CalendarAccess);
}

/// Local midnight 7 days ago to local midnight 35 days ahead, in ms (`CalendarPayload.window`).
pub fn window(now_ms: i64) -> (i64, i64) {
    let today = clock::day_start(now_ms);
    (clock::add_days(today, -7), clock::add_days(today, 35))
}

/// `#RRGGBB` from sRGB components in 0...1 (`CalendarPayload.hex`).
pub fn hex(r: f64, g: f64, b: f64) -> String {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", byte(r), byte(g), byte(b))
}

fn parsed_web(text: &str) -> Option<url::Url> {
    url::Url::parse(text).ok().filter(|u| matches!(u.scheme(), "http" | "https"))
}

fn is_meeting(address: &str) -> bool {
    let Some(host) = parsed_web(address).and_then(|u| u.host_str().map(str::to_lowercase)) else { return false };
    CONFERENCE_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// The event's own web URL, else the first meeting link in its location or notes, else a location that is itself a web link
/// (`CalendarPayload.link`). `find` lists the web addresses in a piece of text.
pub fn link(url: Option<&str>, location: Option<&str>, notes: Option<&str>, mut find: impl FnMut(&str) -> Vec<String>) -> Option<String> {
    if let Some(url) = url.filter(|u| parsed_web(u).is_some()) {
        return Some(url.to_owned());
    }
    let found: Vec<String> = [location, notes].into_iter().flatten().flat_map(&mut find).filter(|l| parsed_web(l).is_some()).collect();
    if let Some(meeting) = found.into_iter().find(|l| is_meeting(l)) {
        return Some(meeting);
    }
    let location = location?.trim();
    parsed_web(location).filter(|u| u.host().is_some()).map(|_| location.to_owned())
}

/// One event as the daemon takes it (`CalendarPayload.item` and `request`).
pub fn item(raw: &RawEvent, now_s: f64, find: impl FnMut(&str) -> Vec<String>) -> CalendarEvent {
    let start = raw.start.unwrap_or(now_s);
    let end = raw.end.unwrap_or(start);
    let location = raw.location.as_deref().map(str::trim).filter(|l| !l.is_empty()).map(str::to_owned);
    CalendarEvent {
        // A repeating event's occurrences share one identifier.
        id: format!("{}@{}", raw.identifier, start as i64),
        calendar: raw.calendar.clone(),
        color: raw.color.clone(),
        title: raw.title.clone(),
        start_ms: (start * 1000.0).round() as i64,
        end_ms: (end * 1000.0).round() as i64,
        all_day: raw.all_day,
        location,
        url: link(raw.url.as_deref(), raw.location.as_deref(), raw.notes.as_deref(), find),
    }
}

/// The bridge's decisions, `CalendarBridge`'s methods.
pub struct Bridge<S, D> {
    pub source: S,
    pub daemon: D,
    enabled: bool,
    last_access: Option<Access>,
}

impl<S: CalendarSource, D: CalendarDaemon> Bridge<S, D> {
    pub fn new(source: S, daemon: D) -> Bridge<S, D> {
        Bridge { source, daemon, enabled: false, last_access: None }
    }

    /// One pass of `start()`'s loop: asks whether Calendar is on; if so asks macOS once, sends, and waits [`INTERVAL`]; if not waits [`IDLE`].
    pub fn cycle(&mut self, now_ms: i64) -> Duration {
        self.enabled = self.daemon.enabled();
        if !self.enabled {
            return IDLE;
        }
        // EventKit shows its prompt only while access was never decided.
        if self.source.access() == Access::NotAsked {
            self.source.request_access();
        }
        self.sync(now_ms);
        INTERVAL
    }

    /// `sync()`: nothing unless Calendar is on.
    pub fn sync(&mut self, now_ms: i64) {
        if !self.enabled {
            return;
        }
        let access = self.source.access();
        // Access granted later in System Settings: the store must re-read.
        if access == Access::Granted && self.last_access.is_some_and(|last| last != Access::Granted) {
            self.source.reset();
        }
        self.last_access = Some(access);
        let mut events = Vec::new();
        if access == Access::Granted {
            let (from, to) = window(now_ms);
            let raw = self.source.events(from, to, MAX_EVENTS);
            let now_s = now_ms as f64 / 1000.0;
            events = raw.iter().map(|r| item(r, now_s, |text| self.source.links(text))).collect();
        }
        self.daemon.send(events, access.calendar());
    }
}

impl<S: CalendarSource, D: CalendarDaemon> Job for Bridge<S, D> {
    fn cycle(&mut self, now_ms: i64) -> Duration {
        Bridge::cycle(self, now_ms)
    }

    fn settled(&mut self, now_ms: i64) {
        self.sync(now_ms);
    }
}

/// The real daemon, over its socket.
pub struct SocketDaemon(pub PathBuf);

impl CalendarDaemon for SocketDaemon {
    fn enabled(&mut self) -> bool {
        crate::link::shell::calendar_enabled(&self.0).unwrap_or(false)
    }

    fn send(&mut self, events: Vec<CalendarEvent>, access: CalendarAccess) {
        let _ = crate::link::shell::set_calendar_events(&self.0, events, SOURCE, access);
    }
}

/// Starts the bridge on a thread of its own (EventKit's store is made there and never leaves it). `active` says whether Rune holds the duty now.
/// Does nothing outside an app bundle with the usage string: one line on stderr.
#[cfg(target_os = "macos")]
pub fn spawn(socket: PathBuf, active: Arc<AtomicBool>) {
    if let Err(why) = super::eventkit::native::bundle_check(super::eventkit::CALENDAR_USAGE_KEY) {
        eprintln!("neko-rune: --app: {why}, so Calendar is off");
        return;
    }
    let _ = std::thread::Builder::new().name("neko-calendar".into()).spawn(move || {
        let (tx, rx) = std::sync::mpsc::channel();
        let source = native::EkCalendar::new(tx);
        schedule::drive(Bridge::new(source, SocketDaemon(socket)), &active, &rx);
    });
}

#[cfg(not(target_os = "macos"))]
pub fn spawn(_socket: PathBuf, _active: Arc<AtomicBool>) {}

#[cfg(target_os = "macos")]
pub mod native {
    //! EventKit's side of [`CalendarSource`]. Thin on purpose: it reads and converts, it decides nothing.

    use std::sync::mpsc::Sender;

    use objc2::rc::Retained;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::NSColorSpace;
    use objc2_event_kit::{EKEntityType, EKEvent, EKEventStore};
    use objc2_foundation::{NSDataDetector, NSDate, NSMatchingOptions, NSObjectProtocol, NSRange, NSString, NSTextCheckingTypes};

    use super::{hex, CalendarSource, RawEvent};
    use crate::shell::eventkit::{native as ek, Access};

    pub struct EkCalendar {
        store: Retained<EKEventStore>,
        _observer: Retained<ProtocolObject<dyn NSObjectProtocol>>,
    }

    impl EkCalendar {
        pub fn new(changed: Sender<()>) -> EkCalendar {
            let store = unsafe { EKEventStore::new() };
            let observer = ek::observe(&store, changed);
            EkCalendar { store, _observer: observer }
        }
    }

    fn seconds(date: &NSDate) -> f64 {
        date.timeIntervalSince1970()
    }

    fn raw(event: &EKEvent) -> RawEvent {
        unsafe {
            let calendar = event.calendar();
            let color = calendar.as_ref().map(|c| {
                // `NSColor.usingColorSpace(.sRGB)`: the calendar's colour as sRGB components.
                match c.color().colorUsingColorSpace(&NSColorSpace::sRGBColorSpace()) {
                    Some(rgb) => hex(rgb.redComponent(), rgb.greenComponent(), rgb.blueComponent()),
                    None => String::new(),
                }
            });
            RawEvent {
                identifier: event.eventIdentifier().unwrap_or_else(|| event.calendarItemIdentifier()).to_string(),
                calendar: calendar.map(|c| c.title().to_string()).unwrap_or_default(),
                color: color.unwrap_or_default(),
                title: event.title().to_string(),
                start: Some(seconds(&event.startDate())),
                end: Some(seconds(&event.endDate())),
                all_day: event.isAllDay(),
                location: event.location().map(|s| s.to_string()),
                url: event.URL().and_then(|u| u.absoluteString()).map(|s| s.to_string()),
                notes: event.notes().map(|s| s.to_string()),
            }
        }
    }

    impl CalendarSource for EkCalendar {
        fn access(&mut self) -> Access {
            ek::authorization(EKEntityType::Event)
        }

        fn request_access(&mut self) {
            ek::request_access(&self.store, false);
        }

        fn reset(&mut self) {
            unsafe { self.store.reset() }
        }

        fn events(&mut self, from_ms: i64, to_ms: i64, limit: usize) -> Vec<RawEvent> {
            let from = NSDate::dateWithTimeIntervalSince1970(from_ms as f64 / 1000.0);
            let to = NSDate::dateWithTimeIntervalSince1970(to_ms as f64 / 1000.0);
            unsafe {
                let predicate = self.store.predicateForEventsWithStartDate_endDate_calendars(&from, &to, None);
                self.store.eventsMatchingPredicate(&predicate).iter().take(limit).map(|e| raw(&e)).collect()
            }
        }

        fn links(&mut self, text: &str) -> Vec<String> {
            // `NSDataDetector` with the link type, as Swift's `links(in:)`.
            const LINK: NSTextCheckingTypes = 1 << 5;
            let Ok(detector) = NSDataDetector::dataDetectorWithTypes_error(LINK) else { return Vec::new() };
            let string = NSString::from_str(text);
            let range = NSRange::new(0, string.length());
            detector.matchesInString_options_range(&string, NSMatchingOptions::empty(), range).iter().filter_map(|m| m.URL()).filter_map(|u| u.absoluteString()).map(|s| s.to_string()).collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shell::schedule::{Runner, Timers, RECHECK, SETTLE};
    use std::cell::RefCell;
    use std::rc::Rc;

    const MIN: i64 = 60_000;

    #[derive(Default)]
    struct World {
        enabled: bool,
        access: Option<Access>,
        asked: usize,
        resets: usize,
        reads: usize,
        sent: Vec<(Vec<CalendarEvent>, CalendarAccess)>,
        events: Vec<RawEvent>,
        window_seen: Option<(i64, i64, usize)>,
        /// What the fake link finder returns for a text.
        links: Vec<(String, Vec<String>)>,
        status_asks: usize,
    }

    #[derive(Clone, Default)]
    struct Fake(Rc<RefCell<World>>);

    impl CalendarSource for Fake {
        fn access(&mut self) -> Access {
            self.0.borrow().access.unwrap_or(Access::NotAsked)
        }
        fn request_access(&mut self) {
            let mut w = self.0.borrow_mut();
            w.asked += 1;
            w.access = Some(Access::Granted);
        }
        fn reset(&mut self) {
            self.0.borrow_mut().resets += 1;
        }
        fn events(&mut self, from_ms: i64, to_ms: i64, limit: usize) -> Vec<RawEvent> {
            let mut w = self.0.borrow_mut();
            w.reads += 1;
            w.window_seen = Some((from_ms, to_ms, limit));
            w.events.clone()
        }
        fn links(&mut self, text: &str) -> Vec<String> {
            self.0.borrow().links.iter().find(|(t, _)| t == text).map(|(_, l)| l.clone()).unwrap_or_default()
        }
    }

    impl CalendarDaemon for Fake {
        fn enabled(&mut self) -> bool {
            let mut w = self.0.borrow_mut();
            w.status_asks += 1;
            w.enabled
        }
        fn send(&mut self, events: Vec<CalendarEvent>, access: CalendarAccess) {
            self.0.borrow_mut().sent.push((events, access));
        }
    }

    fn bridge(world: &Fake) -> Bridge<Fake, Fake> {
        Bridge::new(world.clone(), world.clone())
    }

    fn event(id: &str, start: f64) -> RawEvent {
        RawEvent { identifier: id.into(), calendar: "Work".into(), color: "#112233".into(), title: "Standup".into(), start: Some(start), end: Some(start + 1800.0), ..RawEvent::default() }
    }

    #[test]
    fn the_window_is_local_midnight_seven_days_back_to_thirty_five_ahead() {
        let now = 1_800_000_000_000i64;
        let (from, to) = window(now);
        let today = clock::day_start(now);
        assert_eq!(from, clock::add_days(today, -7));
        assert_eq!(to, clock::add_days(today, 35));
        assert!(from < today && today < to && to - from > 41 * 23 * 3_600_000, "42 days");
    }

    #[test]
    fn the_hex_is_rounded_clamped_and_upper_case() {
        assert_eq!(hex(1.0, 0.0, 0.5), "#FF0080");
        assert_eq!(hex(-0.2, 1.4, 0.2), "#00FF33");
        assert_eq!(hex(0.0, 0.0, 0.0), "#000000");
    }

    #[test]
    fn the_id_carries_the_start_second_and_the_times_are_rounded_milliseconds() {
        let mut raw = event("ABC", 1_700_000_000.4);
        raw.all_day = true;
        raw.location = Some("  Room 4 \n".into());
        let e = item(&raw, 0.0, |_| vec![]);
        assert_eq!(e.id, "ABC@1700000000");
        assert_eq!((e.start_ms, e.end_ms), (1_700_000_000_400, 1_700_001_800_400));
        assert_eq!((e.calendar.as_str(), e.color.as_str(), e.title.as_str(), e.all_day), ("Work", "#112233", "Standup", true));
        assert_eq!(e.location.as_deref(), Some("Room 4"), "trimmed");
        assert!(e.url.is_none());
        // Two occurrences of one repeating event differ by start.
        assert_ne!(item(&event("R", 100.0), 0.0, |_| vec![]).id, item(&event("R", 86_500.0), 0.0, |_| vec![]).id);
    }

    #[test]
    fn a_missing_start_is_now_a_missing_end_is_the_start_and_an_empty_location_is_none() {
        let raw = RawEvent { identifier: "X".into(), location: Some("   ".into()), ..RawEvent::default() };
        let e = item(&raw, 1_700_000_100.0, |_| vec![]);
        assert_eq!((e.start_ms, e.end_ms), (1_700_000_100_000, 1_700_000_100_000));
        assert_eq!(e.id, "X@1700000100");
        assert!(e.location.is_none());
    }

    #[test]
    fn the_link_is_the_events_url_then_a_meeting_link_then_a_location_that_is_a_link() {
        let none = |_: &str| Vec::<String>::new();
        // Its own web URL wins, even over a meeting link.
        assert_eq!(link(Some("https://example.com/e/1"), Some("x"), Some("y"), |_| vec!["https://zoom.us/j/1".into()]).as_deref(), Some("https://example.com/e/1"));
        // A URL that is not http(s) is skipped.
        assert_eq!(link(Some("file:///etc/hosts"), None, None, none), None);
        assert_eq!(link(Some("mailto:a@example.com"), None, None, none), None);
        // The first meeting link in the location, then the notes.
        let find = |t: &str| match t {
            "Room" => vec!["https://example.com/room".to_owned()],
            "notes" => vec!["https://example.com/other".to_owned(), "https://acme.zoom.us/j/9".to_owned(), "https://meet.google.com/abc".to_owned()],
            _ => vec![],
        };
        assert_eq!(link(None, Some("Room"), Some("notes"), find).as_deref(), Some("https://acme.zoom.us/j/9"), "a subdomain of a conference host, the first of them");
        // No meeting link: a location that is itself a web link.
        assert_eq!(link(None, Some(" https://example.com/where \n"), None, |_| vec![]).as_deref(), Some("https://example.com/where"));
        assert_eq!(link(None, Some("Room 4"), Some("bring a laptop"), |_| vec![]), None);
        // A look-alike host is not a conference host.
        assert_eq!(link(None, None, Some("n"), |_| vec!["https://notzoom.us/j/1".into()]), None);
    }

    #[test]
    fn nothing_is_asked_or_sent_while_calendar_is_off_and_the_switch_is_looked_at_every_twenty_seconds() {
        let w = Fake::default();
        let mut b = bridge(&w);
        assert_eq!(b.cycle(0), IDLE);
        b.sync(0);
        let world = w.0.borrow();
        assert_eq!((world.asked, world.reads, world.sent.len()), (0, 0, 0), "macOS is not asked and no event is read while it is off");
        assert_eq!(world.status_asks, 1);
    }

    #[test]
    fn switched_on_it_asks_macos_once_reads_the_window_and_sends_granted_events() {
        let w = Fake::default();
        {
            let mut world = w.0.borrow_mut();
            world.enabled = true;
            world.events = vec![event("A", 1_700_000_000.0), event("B", 1_700_003_600.0)];
        }
        let mut b = bridge(&w);
        let now = 1_700_000_500_000;
        assert_eq!(b.cycle(now), INTERVAL);
        assert_eq!(b.cycle(now + 5 * MIN), INTERVAL);
        let world = w.0.borrow();
        assert_eq!(world.asked, 1, "the prompt is asked for once: only while access is undecided");
        assert_eq!(world.sent.len(), 2);
        let (events, access) = &world.sent[0];
        assert_eq!(*access, CalendarAccess::Granted);
        assert_eq!(events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["A@1700000000", "B@1700003600"]);
        let (from, to, limit) = world.window_seen.unwrap();
        assert_eq!((from, to), window(now + 5 * MIN));
        assert_eq!(limit, MAX_EVENTS);
    }

    #[test]
    fn denied_is_reported_as_denied_with_no_events_and_nothing_is_read() {
        let w = Fake::default();
        {
            let mut world = w.0.borrow_mut();
            world.enabled = true;
            world.access = Some(Access::Denied);
            world.events = vec![event("A", 1.0)];
        }
        let mut b = bridge(&w);
        b.cycle(0);
        let world = w.0.borrow();
        assert_eq!(world.asked, 0, "a decided answer is never asked again");
        assert_eq!(world.reads, 0);
        assert_eq!(world.sent, vec![(vec![], CalendarAccess::Denied)], "the daemon is told, so it drops what it had");
    }

    #[test]
    fn access_granted_later_resets_the_store_once() {
        let w = Fake::default();
        {
            let mut world = w.0.borrow_mut();
            world.enabled = true;
            world.access = Some(Access::Denied);
        }
        let mut b = bridge(&w);
        b.cycle(0);
        assert_eq!(w.0.borrow().resets, 0);
        w.0.borrow_mut().access = Some(Access::Granted);
        b.cycle(1);
        b.cycle(2);
        assert_eq!(w.0.borrow().resets, 1, "the first sync after the change re-reads, later ones do not");
        assert_eq!(w.0.borrow().sent.last().unwrap().1, CalendarAccess::Granted);
    }

    #[test]
    fn links_are_found_in_the_location_and_the_notes() {
        let w = Fake::default();
        {
            let mut world = w.0.borrow_mut();
            world.enabled = true;
            let mut e = event("L", 1_700_000_000.0);
            e.notes = Some("join: https://meet.google.com/abc-defg".into());
            world.events = vec![e];
            world.links = vec![("join: https://meet.google.com/abc-defg".into(), vec!["https://meet.google.com/abc-defg".into()])];
        }
        bridge(&w).cycle(0);
        assert_eq!(w.0.borrow().sent[0].0[0].url.as_deref(), Some("https://meet.google.com/abc-defg"));
    }

    #[test]
    fn the_loop_cycles_at_once_then_every_five_minutes_and_a_burst_settles_for_two_seconds() {
        let w = Fake::default();
        w.0.borrow_mut().enabled = true;
        let mut r = Runner::new(bridge(&w));
        let t0 = 10_000_000;
        assert_eq!(r.step(t0, true, false), RECHECK, "a cycle ran at once; the next is five minutes off, looked at every 5 s");
        assert_eq!(w.0.borrow().sent.len(), 1);
        assert_eq!(r.step(t0 + 4 * MIN, true, false), RECHECK);
        assert_eq!(w.0.borrow().sent.len(), 1, "nothing is sent between cycles");
        // A burst of changes: each one pushes the send back; one send comes two seconds after the last.
        assert_eq!(r.step(t0 + 4 * MIN + 100, true, true), SETTLE);
        assert_eq!(r.step(t0 + 4 * MIN + 1_000, true, true), SETTLE);
        assert_eq!(w.0.borrow().sent.len(), 1);
        assert_eq!(r.step(t0 + 4 * MIN + 2_999, true, false), Duration::from_millis(1));
        assert_eq!(w.0.borrow().sent.len(), 1, "two seconds after the LAST change, not the first");
        r.step(t0 + 4 * MIN + 3_000, true, false);
        assert_eq!(w.0.borrow().sent.len(), 2, "one send for the whole burst");
        // The five-minute cycle.
        r.step(t0 + 5 * MIN, true, false);
        assert_eq!(w.0.borrow().sent.len(), 3);
    }

    #[test]
    fn a_change_while_calendar_is_off_sends_nothing() {
        let w = Fake::default();
        let mut r = Runner::new(bridge(&w));
        r.step(0, true, false);
        r.step(1_000, true, true);
        r.step(3_500, true, false);
        let world = w.0.borrow();
        assert_eq!(world.sent.len(), 0);
        assert_eq!(world.reads, 0);
    }

    #[test]
    fn handed_back_the_loop_asks_and_sends_nothing_and_on_return_it_cycles_at_once() {
        let w = Fake::default();
        w.0.borrow_mut().enabled = true;
        let mut r = Runner::new(bridge(&w));
        r.step(0, true, false);
        assert_eq!((w.0.borrow().sent.len(), w.0.borrow().status_asks), (1, 1));
        // The Swift app started: nothing is asked, even a pending change is dropped.
        r.step(MIN, true, true);
        for t in [2 * MIN, 3 * MIN, 6 * MIN, 12 * MIN] {
            assert_eq!(r.step(t, false, t % 2 == 0), RECHECK);
        }
        assert_eq!((w.0.borrow().sent.len(), w.0.borrow().status_asks), (1, 1), "nothing since the hand-back: no status, no read, no send");
        assert_eq!(r.timers, Timers::default());
        // It quit: a cycle at once.
        r.step(13 * MIN, true, false);
        assert_eq!((w.0.borrow().sent.len(), w.0.borrow().status_asks), (2, 2));
    }
}
