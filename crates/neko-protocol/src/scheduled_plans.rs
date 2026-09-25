//! Neko-owned schedules. A saved/imported draft is paused until SetEnabled.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub workspace_id: Option<String>,
    pub rule: String,
    pub timezone: String,
    pub anchor_ms: i64,
    pub next_due_ms: Option<i64>,
    pub enabled: bool,
    /// Immutable import identity; reimport never overwrites a user-edited draft.
    pub source_id: Option<String>,
    pub last_task_id: Option<String>,
    pub last_result: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ScheduleCommand {
    List,
    /// Saving always pauses. Read-only/history fields in the input are ignored.
    Save {
        schedule: Schedule,
    },
    SetEnabled {
        id: String,
        enabled: bool,
    },
    Remove {
        id: String,
    },
    /// Enqueues a read-only plan; edits still need normal task approval.
    RunNow {
        id: String,
    },
}
