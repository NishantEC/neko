//! The Calendar: the person's events as macOS Calendar (EventKit) has them,
//! sent by the installed app, and Neko's own dated work beside them. See
//! docs/knowledge/calendar.md.
use serde::{Deserialize, Serialize};

/// One event from macOS Calendar. Times are Unix milliseconds; an all-day
/// event spans local midnight to local midnight as EventKit reports it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarEvent {
    /// EventKit's identifier for this occurrence (a repeating event's
    /// occurrences share an event id, so the app adds the start time).
    pub id: String,
    /// The calendar's title ("Work", "Family", an account's name).
    pub calendar: String,
    /// The calendar's colour as `#RRGGBB`, or empty when it has none.
    pub color: String,
    pub title: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub all_day: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// The event's own URL, or a conference link found in it (Meet, Zoom,
    /// Teams, Webex). Always `http(s)`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// Whether Neko may read the person's calendars.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarAccess {
    /// No installed Neko has asked yet (or none has reported).
    #[default]
    NotAsked,
    Granted,
    /// Denied, restricted, or write-only: Neko cannot read events.
    Denied,
}

pub fn granted() -> CalendarAccess {
    CalendarAccess::Granted
}

/// What the Calendar knows about where its events came from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CalendarStatus {
    pub access: CalendarAccess,
    /// Who sent the events ("macOS Calendar").
    pub source: String,
    pub last_sync_ms: Option<i64>,
    /// Events stored.
    pub events: usize,
    /// More events were sent than are kept (2000 or 1 MB).
    pub truncated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NekoItemKind {
    /// A task, on the day it finished (ready for review, done or failed).
    Task,
    /// A scheduled plan's coming runs.
    Schedule,
    /// A routine's coming runs.
    Routine,
}

/// A piece of Neko's own work that has a date, for the all-day row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NekoCalendarItem {
    pub id: String,
    pub kind: NekoItemKind,
    pub title: String,
    pub at_ms: i64,
    /// True when only the day matters (tasks); false when `at_ms` is a time.
    pub all_day: bool,
    /// For tasks: `TaskStatus` as text ("Completed"). Empty otherwise.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// One short line: "Daily at 09:00", "Ready for review".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub detail: String,
    /// Where the item opens in the web app, without `#/` ("tasks?task=t-1").
    pub route: String,
}
