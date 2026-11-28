//! Timers and watches (docs/knowledge/waits.md): what a thread's agent asks Neko to wake it for, after its turn has ended. A timer
//! wakes it at a time; a watch wakes it when files in its folder change, or when a check command succeeds. The daemon keeps them and
//! wakes the thread with `--resume`; a screen draws the ones still waiting on a thread ([`crate::Task::waits`]) and may stop one.

use serde::{Deserialize, Serialize};

/// A day, in milliseconds.
pub const DAY_MS: i64 = 24 * 3_600_000;
/// How far ahead a timer may be set: a week.
pub const TIMER_MAX_MS: i64 = 7 * DAY_MS;
/// The soonest a timer may be set for. Sooner than this the agent waits inside its own turn.
pub const TIMER_MIN_MS: i64 = 60_000;
/// The longest a watch lasts; it then ends with a wake that says so. A local Mac sleeps and shuts down, so a week, not a month.
pub const WATCH_MAX_MS: i64 = 7 * DAY_MS;
/// The most timers one thread may have set at once.
pub const TIMERS_PER_THREAD: usize = 4;
/// The most watches one thread may have at once, and in all.
pub const WATCHES_PER_THREAD: usize = 4;
pub const WATCHES_IN_ALL: usize = 32;

/// What a thread waits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaitKind {
    /// A time (`wake_me`).
    Timer,
    /// Files in its folder changing (`watch_files`).
    Files,
    /// A check command exiting 0 (`watch_command`).
    Command,
}

/// One timer or watch a thread waits on (version 24). Only the ones still waiting are sent; one that fired, ended or was stopped is
/// gone from [`crate::Task::waits`].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wait {
    pub id: String,
    pub kind: WaitKind,
    /// What the agent said it is for, in its own words.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    /// A timer: when it wakes the thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<i64>,
    /// A watch: when it ends by itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until_ms: Option<i64>,
    /// A files watch: the paths or patterns, as the agent gave them (relative to the thread's folder).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub paths: Vec<String>,
    /// A command watch: the command, and how often it is run.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub every_ms: Option<i64>,
    /// A command watch: when it was last run, and its exit code then.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_exit: Option<i32>,
    pub created_ms: i64,
}

impl Wait {
    /// Whether it is a watch (files or a command), not a timer.
    pub fn is_watch(&self) -> bool {
        self.kind != WaitKind::Timer
    }

    /// What it watches, in a few words for a line: the paths ("src/**, Cargo.toml"), or the command.
    pub fn subject(&self) -> String {
        match self.kind {
            WaitKind::Timer => String::new(),
            WaitKind::Files => self.paths.join(", "),
            WaitKind::Command => self.command.clone().unwrap_or_default(),
        }
    }
}

/// What woke a thread, for the centred note before the run that read it ([`crate::PartBody::EventWake`], version 24).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    /// A timer it set came due.
    Timer,
    /// Files it watched changed.
    Files,
    /// A check command it watched succeeded.
    Command,
    /// A watch ended by itself (a week passed).
    WatchEnded,
    /// A standing round posted its words into a Cattain's own thread.
    Round,
}

/// The longest a watch's paths may be listed: at most this many.
pub const PATHS_MAX: usize = 16;

/// Whether `path` (relative, `/`-separated) matches `pattern`: `*` is any run of characters inside one part, `?` one character, `**`
/// any number of whole parts. A pattern with no wildcard also matches everything under it, so a folder's name watches the folder.
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern = pattern.trim_matches('/');
    let path = path.trim_matches('/');
    if !pattern.contains(['*', '?']) {
        return path == pattern || pattern.is_empty() || path.starts_with(&format!("{pattern}/"));
    }
    let p: Vec<&str> = pattern.split('/').collect();
    let s: Vec<&str> = path.split('/').collect();
    parts_match(&p, &s)
}

fn parts_match(p: &[&str], s: &[&str]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(&"**") => (0..=s.len()).any(|i| parts_match(&p[1..], &s[i..])),
        Some(seg) => !s.is_empty() && segment_match(seg.as_bytes(), s[0].as_bytes()) && parts_match(&p[1..], &s[1..]),
    }
}

fn segment_match(p: &[u8], s: &[u8]) -> bool {
    match p.first() {
        None => s.is_empty(),
        Some(b'*') => (0..=s.len()).any(|i| segment_match(&p[1..], &s[i..])),
        Some(b'?') => !s.is_empty() && segment_match(&p[1..], &s[1..]),
        Some(c) => s.first() == Some(c) && segment_match(&p[1..], &s[1..]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_match_as_a_shell_and_git_read_them() {
        assert!(matches("src/**", "src/a.rs") && matches("src/**", "src/deep/b.rs") && matches("src/**", "src"));
        assert!(matches("*.md", "README.md") && !matches("*.md", "docs/a.md"), "one star stays in one part");
        assert!(matches("**/*.md", "docs/a.md") && matches("**/*.md", "a.md"));
        assert!(matches("build.log", "build.log") && !matches("build.log", "build.log.old"));
        assert!(matches("out", "out/report.txt"), "a folder's name watches what is in it");
        assert!(matches("a?c.txt", "abc.txt") && !matches("a?c.txt", "ac.txt"));
        assert!(!matches("src/**", "srcx/a.rs"));
    }
}
