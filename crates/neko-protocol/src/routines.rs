//! Routines: recurring agent runs, one scope each, that act and then report
//! what they did and what only the owner can do. Scopes are workspaces whose
//! MCP servers come from the owner's own config files on disk.
//! See docs/superpowers/specs/2026-10-29-neko-runs-the-day-design.md.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RoutineKind {
    Messages,
    Digest,
    PrSweep,
    /// Learns standing orders from what the owner did with the scope's work.
    Reflect,
    /// Keeps the Mac's disk healthy (personal scope only).
    MacCare,
    Custom,
}

/// Times are local. `Every` backs off while runs are quiet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Cadence {
    Every { minutes: u32, from_hour: u32, to_hour: u32 },
    Daily { hour: u32, minute: u32 },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Routine {
    pub id: String,
    pub workspace_id: String,
    pub kind: RoutineKind,
    pub name: String,
    pub instruction: String,
    pub cadence: Cadence,
    pub enabled: bool,
    /// Claude Code model alias; empty means `sonnet`.
    #[serde(default)]
    pub model: String,
    /// The Paseo schedule this routine replaces, paused after its first good run.
    #[serde(default)]
    pub paseo_schedule: Option<String>,
    /// May drive the owner's own Chrome (Claude in Chrome), signed in to his
    /// accounts: sign-in consent screens and web-only work.
    #[serde(default = "yes")]
    pub browser: bool,
    /// May see and operate Mac apps through Peekaboo (screenshots, the
    /// accessibility tree, clicks and typing, in the background when it can).
    #[serde(default = "yes")]
    pub computer: bool,
    /// The owner changed the instruction: Neko never rewrites it with a
    /// newer seeded one.
    #[serde(default)]
    pub customized: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct RoutineState {
    pub last_run_ms: Option<i64>,
    /// `None` means due now.
    pub next_due_ms: Option<i64>,
    pub quiet_streak: u32,
    /// Whatever the agent asked to be handed back next run.
    #[serde(default)]
    pub memory: serde_json::Value,
    pub running: bool,
    #[serde(default)]
    pub paseo_paused: bool,
    #[serde(default)]
    pub note: Option<String>,
    /// "Run now": runs once even if paused, or again right after a run in flight.
    #[serde(default)]
    pub run_requested: bool,
    /// Failed runs in a row; reset by a good one.
    #[serde(default)]
    pub failures: u32,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NeedsYou {
    /// Global and increasing, so a client can ask for what is new.
    pub seq: u64,
    pub title: String,
    pub detail: String,
    #[serde(default)]
    pub link: Option<String>,
    #[serde(default)]
    pub done: bool,
    /// When the item was raised. 0 on items from before these fields: the
    /// run's start stands in.
    #[serde(default)]
    pub created_ms: i64,
    /// The last time a run reported it again or the owner reopened it. Age
    /// is counted from here.
    #[serde(default)]
    pub touched_ms: i64,
    /// A time after which the item no longer matters (the routine's
    /// `expires`); it closes as "Expired".
    #[serde(default)]
    pub expires_ms: Option<i64>,
    /// When it closed, and why ("PR merged", "Expired", "Aged out",
    /// "Resolved by routine", "Done by you"). Set together with `done`.
    #[serde(default)]
    pub closed_ms: Option<i64>,
    #[serde(default)]
    pub reason: Option<String>,
    /// Reopened by the owner: a merged PR or a passed time no longer closes
    /// it again (only age-out and the owner do).
    #[serde(default)]
    pub kept: bool,
    /// The routine marked it as unable to wait for morning: it is pushed to
    /// the phone even during quiet hours.
    #[serde(default)]
    pub urgent: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineRun {
    pub id: String,
    pub routine_id: String,
    pub started_ms: i64,
    pub finished_ms: Option<i64>,
    pub ok: bool,
    pub quiet: bool,
    pub did: Vec<String>,
    pub needs_you: Vec<NeedsYou>,
    pub error: Option<String>,
    /// The agent's reply, kept for a failed run (bounded).
    #[serde(default)]
    pub text: String,
    /// Which agent did the run ("claude", "codex"); empty on old runs, which
    /// were all Claude Code.
    #[serde(default)]
    pub runtime: String,
    /// Said quietly in the run's detail, e.g. a fallback that ran without
    /// some of the scope's servers.
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeServer {
    pub name: String,
    /// The file that configures it.
    pub origin: String,
    /// "scope" (the top folder) or "repo".
    pub level: String,
    #[serde(default)]
    pub repo: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScopeProposal {
    pub name: String,
    pub folders: Vec<String>,
    pub servers: Vec<ScopeServer>,
    pub problems: Vec<String>,
    #[serde(default)]
    pub workspace_id: Option<String>,
    /// Found on disk by `neko_core::scopes`, not just an existing workspace.
    #[serde(default)]
    pub discovered: bool,
    /// The workspace's own instructions: what this scope's tools and accounts
    /// are, in the owner's words. Routines read them as scope notes.
    #[serde(default)]
    pub instructions: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RoutineView {
    pub routine: Routine,
    pub state: RoutineState,
    /// Newest first, at most 10.
    pub runs: Vec<RoutineRun>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineNotice {
    pub scope: String,
    pub routine: String,
    pub item: NeedsYou,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn routine_and_run_round_trip() {
        let routine = Routine {
            id: "r".into(),
            workspace_id: "w".into(),
            kind: RoutineKind::Messages,
            name: "Messages".into(),
            instruction: "Read Slack".into(),
            cadence: Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 },
            enabled: true,
            model: String::new(),
            paseo_schedule: Some("message-watch-adaptive".into()),
            browser: true,
            computer: true,
            customized: false,
        };
        let json = serde_json::to_string(&routine).unwrap();
        assert_eq!(serde_json::from_str::<Routine>(&json).unwrap(), routine);
        let run = RoutineRun {
            id: "x".into(),
            routine_id: "r".into(),
            started_ms: 1,
            finished_ms: Some(2),
            ok: true,
            quiet: false,
            did: vec!["Replied to Asha".into()],
            needs_you: vec![NeedsYou { seq: 3, title: "Sign in".into(), detail: "Jira".into(), link: None, done: false, ..Default::default() }],
            error: None,
            text: String::new(),
            ..Default::default()
        };
        let json = serde_json::to_string(&run).unwrap();
        assert_eq!(serde_json::from_str::<RoutineRun>(&json).unwrap(), run);
    }

    #[test]
    fn old_routine_gets_the_browser() {
        let json = r#"{"id":"r","workspace_id":"w","kind":"Messages","name":"M","instruction":"i","cadence":{"Daily":{"hour":9,"minute":0}},"enabled":true}"#;
        let routine = serde_json::from_str::<Routine>(json).unwrap();
        assert!(routine.browser && routine.computer);
    }

    #[test]
    fn old_state_without_new_fields_loads() {
        let state: RoutineState = serde_json::from_str(r#"{"last_run_ms":null,"next_due_ms":null,"quiet_streak":0,"running":false}"#).unwrap();
        assert!(!state.paseo_paused);
        assert!(state.memory.is_null());
    }
}
