//! Reaching the owner on their phone when something needs them and they are
//! away from the Mac. Two channels, both off until the person turns them on:
//! Apple Reminders (the installed app owns EventKit, the daemon decides) and
//! ntfy. See docs/knowledge/phone-push.md. Every field has a serde default so
//! stored rows and older clients still load.
use serde::{Deserialize, Serialize};

/// What the person chose. Stored as one row; `ntfy_topic` is a secret that
/// is generated on first use and shown only to the owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhoneSettings {
    /// Apple Reminders: a reminder in the "Neko" list per needs-you item.
    pub reminders: bool,
    pub ntfy: bool,
    pub ntfy_server: String,
    pub ntfy_topic: String,
    /// Send only the title to the ntfy server (it sees the text).
    pub titles_only: bool,
    pub quiet_enabled: bool,
    pub quiet_from_hour: u32,
    pub quiet_to_hour: u32,
}

pub const DEFAULT_NTFY_SERVER: &str = "https://ntfy.sh";

impl Default for PhoneSettings {
    fn default() -> Self {
        Self {
            reminders: false,
            ntfy: false,
            ntfy_server: DEFAULT_NTFY_SERVER.into(),
            ntfy_topic: String::new(),
            titles_only: true,
            quiet_enabled: true,
            quiet_from_hour: 23,
            quiet_to_hour: 8,
        }
    }
}

/// Whether the app may use Reminders (`NotAsked` until the person turns the
/// channel on).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RemindersAccess {
    #[default]
    NotAsked,
    Granted,
    Denied,
}

/// What the Settings page shows, one line each.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhoneStatus {
    pub reminders_access: RemindersAccess,
    pub last_push_ms: Option<i64>,
    /// "Reminders" or "ntfy".
    pub last_channel: String,
    pub last_error: Option<String>,
    /// Needs-you items waiting for quiet hours to end.
    pub held: u32,
    /// `ntfy://host/topic`, empty without a topic.
    pub subscribe_link: String,
}

/// One reminder the app should create.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReminderNew {
    pub seq: u64,
    pub title: String,
    pub notes: String,
    pub url: Option<String>,
    /// Unique per queued reminder; echoed back so a summary (seq 0) is not
    /// created again on the next cycle.
    pub key: String,
}

/// What the app should do next. `enabled` false with an empty `delete` means
/// leave Reminders alone.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReminderPlan {
    pub enabled: bool,
    pub create: Vec<ReminderNew>,
    /// `calendarItemIdentifier`s of reminders to remove.
    pub delete: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReminderCreated {
    pub seq: u64,
    pub id: String,
    pub key: String,
}
