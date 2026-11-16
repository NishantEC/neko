//! What the full views keep of the daemon's answers: the routines with their runs, the calendar's
//! week, the phone settings, the quota meters. Plain data; the requests that fetch and change them
//! are `Call`s in `write.rs`, the same ones the web UI's Inbox, Calendar, Settings and Usage pages send.

use std::collections::BTreeMap;

use neko_protocol::calendar::{CalendarEvent, CalendarStatus, NekoCalendarItem};
use neko_protocol::phone::{PhoneSettings, PhoneStatus};
use neko_protocol::routines::{RoutineView, ScopeProposal, SpendToday};

/// The `Routines` answer, whole: the routines (with state and recent runs), the standing orders per
/// workspace, the scopes found on disk, and the Inbox settings.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutinesData {
    pub views: Vec<RoutineView>,
    pub standing_orders: BTreeMap<String, String>,
    pub scopes: Vec<ScopeProposal>,
    pub owner_name: String,
    pub age_out_days: u32,
    pub routines_on: bool,
    pub spend_today: SpendToday,
}

/// The `CalendarEvents` answer for the days on screen.
#[derive(Clone, Debug, PartialEq)]
pub struct CalendarData {
    pub events: Vec<CalendarEvent>,
    pub neko: Vec<NekoCalendarItem>,
    pub status: CalendarStatus,
    pub from_ms: i64,
    pub to_ms: i64,
}

/// The `Phone` answer: the channels' settings and what the last push did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PhoneData {
    pub settings: PhoneSettings,
    pub status: PhoneStatus,
}

/// One vendor's quota meter, from the `usage` provider (the web's Usage page reads the same rows).
#[derive(Clone, Debug, PartialEq)]
pub struct QuotaRow {
    pub label: String,
    /// How much of the window is used, 0 to 1.
    pub used: f32,
    /// "62% left", "resets 14:00": the meter's own stat words.
    pub words: Vec<String>,
}
