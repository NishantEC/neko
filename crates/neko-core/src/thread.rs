//! What a thread is doing, for a sidebar row (docs/knowledge/threads.md). Derived here and nowhere else, from what the store holds,
//! so every screen sees the same answer after a restart. [`thread_status`] and [`needs_you`] are the one place a later status
//! (Waiting on subagents) is added.

use neko_protocol::{LATEST_LINE_CHARS, TaskStatus, ThreadStatus};

/// Working, Failed or Idle. Needs you is a flag beside it ([`needs_you`]), not a value.
///
/// - **Working:** a run is going (the agent is at work, also while it waits for the person to answer a request).
/// - **Failed:** the last run failed and the person has not opened the thread since (`last_read_ms` is older than the activity
///   that ended the run).
/// - **Idle:** anything else.
pub fn thread_status(status: TaskStatus, last_read_ms: i64, activity_ms: i64) -> ThreadStatus {
    match status {
        TaskStatus::Working | TaskStatus::NeedsYou => ThreadStatus::Working,
        TaskStatus::Failed if last_read_ms < activity_ms => ThreadStatus::Failed,
        _ => ThreadStatus::Idle,
    }
}

/// Whether something waits for the person in the thread: a request (the run is waiting on it), a run that ended with a question,
/// or a failure the person can act on (`actionable_failure`: not signed in, the usage limit, the folder or the CLI missing). The
/// same three things `needs::derive` lists.
pub fn needs_you(status: TaskStatus, actionable_failure: bool) -> bool {
    match status {
        TaskStatus::NeedsYou | TaskStatus::Asked => true,
        TaskStatus::Failed => actionable_failure,
        _ => false,
    }
}

/// The last line of some words, for a row's second line: Markdown's marks at the start of the line (heading, list, quote) and
/// round the words (`*`, `` ` ``) are dropped, spaces collapse, and it is cut at [`LATEST_LINE_CHARS`] with an ellipsis. Empty when
/// the words have no line with letters in it.
pub fn latest_line(text: &str) -> String {
    let line = text.lines().rev().map(str::trim).find(|l| l.chars().any(char::is_alphanumeric)).unwrap_or("");
    let mut s = line.trim_start_matches(['#', '>', '-', '*', '+', ' ']).to_owned();
    // An ordered list's "3. ".
    if let Some(i) = s.find(". ")
        && i > 0
        && i <= 3
        && s[..i].chars().all(|c| c.is_ascii_digit())
    {
        s = s[i + 2..].to_owned();
    }
    let s: String = s.chars().filter(|c| !matches!(c, '*' | '`')).collect();
    cut(&s.split_whitespace().collect::<Vec<_>>().join(" "))
}

/// A failure's words in one line, cut like [`latest_line`].
pub fn failure_line(text: &str) -> String {
    cut(&text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn cut(s: &str) -> String {
    if s.chars().count() <= LATEST_LINE_CHARS {
        return s.to_owned();
    }
    let head: String = s.chars().take(LATEST_LINE_CHARS - 1).collect();
    let head = match head.rfind(' ') {
        Some(i) if i > LATEST_LINE_CHARS / 2 => &head[..i],
        _ => &head,
    };
    format!("{}…", head.trim_end())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_status_is_working_while_a_run_goes_and_failed_until_opened() {
        assert_eq!(thread_status(TaskStatus::Working, 0, 5), ThreadStatus::Working);
        assert_eq!(thread_status(TaskStatus::NeedsYou, 0, 5), ThreadStatus::Working);
        assert_eq!(thread_status(TaskStatus::Failed, 4, 5), ThreadStatus::Failed);
        assert_eq!(thread_status(TaskStatus::Failed, 5, 5), ThreadStatus::Idle, "opened after it failed");
        for s in [TaskStatus::New, TaskStatus::Done, TaskStatus::Stopped, TaskStatus::Interrupted, TaskStatus::Asked] {
            assert_eq!(thread_status(s, 0, 5), ThreadStatus::Idle, "{s:?}");
        }
    }

    #[test]
    fn needs_you_is_a_flag_for_a_request_a_question_or_a_failure_to_act_on() {
        assert!(needs_you(TaskStatus::NeedsYou, false));
        assert!(needs_you(TaskStatus::Asked, false));
        assert!(needs_you(TaskStatus::Failed, true));
        assert!(!needs_you(TaskStatus::Failed, false));
        assert!(!needs_you(TaskStatus::Working, false));
        assert!(!needs_you(TaskStatus::Done, true));
    }

    #[test]
    fn the_latest_line_is_the_last_line_with_words_in_it() {
        assert_eq!(latest_line("I looked.\n\nRenamed the label in **three** places.\n\n"), "Renamed the label in three places.");
        assert_eq!(latest_line("Plan:\n- read `a.rs`\n- 3 files changed"), "3 files changed");
        assert_eq!(latest_line("## Done\n\n1. Fixed the redirect"), "Fixed the redirect");
        assert_eq!(latest_line("\n \n"), "");
        let long = format!("{} end", "word ".repeat(60));
        let l = latest_line(&long);
        assert!(l.chars().count() <= LATEST_LINE_CHARS && l.ends_with('…'), "{l}");
    }
}
