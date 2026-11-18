//! The requests the Mac-app duties make on their own threads (`src/shell/`, docs/knowledge/rune-as-the-app.md), each a one-shot
//! read on a fresh connection like every other `ask`: `Ping` (the daemon watch; the web's Diagnostics sends it too), `RoutineNotifications` (the notifier; `RoutineNotifier.swift` sends it) and
//! `CancelAllWork` (the uninstaller's first step, after the person confirmed), and the two EventKit bridges' (`CalendarStatus` and `SetCalendarEvents`,
//! `CalendarBridge.swift`'s; `PhoneRemindersSync` and `PhoneRemindersDone`, `PhoneBridge.swift`'s). Only `CancelAllWork` and the three that store
//! something (`SetCalendarEvents`, `PhoneRemindersSync`'s plan, `PhoneRemindersDone`) change anything in the daemon.

use std::path::Path;

use neko_protocol::calendar::{CalendarAccess, CalendarEvent};
use neko_protocol::routines::RoutineNotice;
use neko_protocol::workbench::Command;
use neko_protocol::{Request, Response};

use super::ask;

/// What a `Ping` came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ping {
    /// The daemon answered `Pong`.
    Pong,
    /// Something answered, but not with `Pong`.
    Other,
    /// Nothing answered: not running, or not reachable.
    Down,
}

/// `Ping` on `socket` (the daemon's own answer, as `NativeLifecycle.swift` asks).
pub fn ping(socket: &Path) -> Ping {
    match ask(socket, Request::Ping) {
        Ok(Response::Pong) => Ping::Pong,
        Ok(_) => Ping::Other,
        Err(_) => Ping::Down,
    }
}
/// The needs-you items after `after`, and where the daemon's list now stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notices {
    pub items: Vec<RoutineNotice>,
    pub latest: u64,
}

/// `RoutineNotifications { after }`. None when the daemon did not answer with them (`RoutineNotifier.poll` returns at once then).
pub fn routine_notifications(socket: &Path, after: u64) -> Option<Notices> {
    match ask(socket, Request::RoutineNotifications { after }) {
        Ok(Response::RoutineNotifications { items, latest }) => Some(Notices { items, latest }),
        _ => None,
    }
}

/// `CancelAllWork`: the uninstaller stops all work first (`Uninstaller.confirmAndRun`). Only ever called after the confirmation said yes.
pub fn cancel_all_work(socket: &Path) -> bool {
    ask(socket, Request::Workbench(Command::CancelAllWork)).is_ok()
}

/// `CalendarStatus`: has the person switched Calendar on? None when the daemon did not answer with the status (`CalendarPayload.isEnabled`
/// reads false then).
pub fn calendar_enabled(socket: &Path) -> Option<bool> {
    match ask(socket, Request::CalendarStatus) {
        Ok(Response::CalendarStatus(status)) => Some(status.enabled),
        _ => None,
    }
}

/// `SetCalendarEvents` (`CalendarPayload.request`): the events, where they came from and whether access was granted. True when the daemon answered.
pub fn set_calendar_events(socket: &Path, events: Vec<CalendarEvent>, source: &str, access: CalendarAccess) -> bool {
    ask(socket, Request::SetCalendarEvents { events, source: source.to_owned(), access }).is_ok()
}
