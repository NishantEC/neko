//! "Needs you" (docs/knowledge/needs-you.md): what waits for the person, derived from what the store holds. Nothing here is stored
//! of its own, so the list is the same after a restart and every screen sees the same one.
//!
//! Four things count, and nothing else (the fourth, a plugin waiting for approval, belongs to no task: docs/knowledge/plugins.md):
//!
//! - a permission request still pending;
//! - a run that ended with a question (the task is `asked`);
//! - a run that failed for a reason the person can act on (not signed in, the usage limit, the folder missing, the CLI missing):
//!   the task is `failed` and its newest error notice says which.
//!
//! The list is oldest first, grouped by workspace: the workspace whose item has waited longest comes first, and inside it the
//! items wait in order.

use std::collections::HashMap;

use neko_protocol::{Approval, FailureKind, Need, NeedsItem, PartBody, RunStatus, Task, TaskId, TaskStatus, Tone};

use crate::{Store, StoreError};

/// The longest a question shown in the list (and a notification) is, in characters.
pub const QUESTION_CHARS: usize = 200;

/// Everything waiting for the person now, oldest first, grouped by workspace.
pub fn derive(store: &Store) -> Result<Vec<NeedsItem>, StoreError> {
    let tasks: HashMap<_, Task> = store.all_tasks()?.into_iter().map(|t| (t.id.clone(), t)).collect();
    let names: HashMap<String, String> = store.list_cats()?.into_iter().map(|c| (c.id, c.name)).collect();
    // A thread with no cat has neither an id nor a name here (version 14).
    let cat_name = |t: &Task| t.cat.as_ref().map(|c| names.get(c).cloned().unwrap_or_else(|| "Neko".to_owned())).unwrap_or_default();
    let cat_id = |t: &Task| t.cat.clone().unwrap_or_default();
    // A subagent's request is shown under its thread's title and says which subagent asks.
    let shown = |t: &Task| t.subagent.as_ref().and_then(|s| tasks.get(&s.root)).map_or_else(|| t.title.clone(), |r| r.title.clone());
    let who = |t: &Task| t.subagent.as_ref().map(|s| s.number.clone());
    let mut items = Vec::new();
    for p in store.pending_permissions()? {
        let Some(t) = tasks.get(&p.task) else { continue };
        items.push(NeedsItem { id: format!("perm:{}", p.id.0), task: t.id.clone(), workspace: t.workspace.clone(), title: shown(t), cat: cat_id(t), cat_name: cat_name(t), since_ms: p.asked_ms, need: Need::Permission { permission: p }, subagent: who(t) });
    }
    // A subagent's question or failure is its thread's to read (its report wakes it): only its requests wait for the person.
    for t in tasks.values().filter(|t| t.subagent.is_none()) {
        match t.status {
            TaskStatus::Asked => {
                let Some(run) = store.runs_of(&t.id)?.pop().filter(|r| r.status == RunStatus::Done) else { continue };
                let parts = store.parts_of(&t.id)?;
                let Some(words) = parts.iter().rev().find_map(|p| match &p.body {
                    PartBody::Agent { text, .. } if p.run.as_ref() == Some(&run.id) && !text.trim().is_empty() => Some(text.as_str()),
                    _ => None,
                }) else {
                    continue;
                };
                let since = run.ended_ms.unwrap_or(run.started_ms);
                items.push(NeedsItem { id: format!("question:{}", run.id.0), task: t.id.clone(), workspace: t.workspace.clone(), title: t.title.clone(), cat: cat_id(t), cat_name: cat_name(t), since_ms: since, need: Need::Question { run: run.id, question: question_of(words) }, subagent: None });
            }
            TaskStatus::Failed => {
                // The newest error notice says why the last attempt failed.
                let Some(part) = store.newest_error_notice(&t.id)? else { continue };
                let Some(kind) = failure_of(&part) else { continue };
                let PartBody::Notice { text, next, .. } = &part.body else { continue };
                items.push(NeedsItem {
                    id: format!("failure:{}", part.id.0),
                    task: t.id.clone(),
                    workspace: t.workspace.clone(),
                    title: t.title.clone(),
                    cat: cat_id(t),
                    cat_name: cat_name(t),
                    since_ms: part.at_ms,
                    need: Need::Failure { failure: kind, text: text.clone(), next: next.clone(), command: next.as_deref().and_then(command_of) },
                    subagent: None,
                });
            }
            _ => {}
        }
    }
    // A plugin waiting to be approved (or approved again) belongs to no task or workspace: its group is the empty one.
    for p in store.plugins()? {
        if !matches!(p.approval, Approval::Waiting | Approval::Changed) {
            continue;
        }
        let since_ms = store.plugin_row(&p.id)?.map_or(p.created_ms, |r| r.manifest_ms);
        items.push(NeedsItem { id: format!("plugin:{}", p.id), task: TaskId(String::new()), workspace: String::new(), title: p.name.clone(), cat: String::new(), cat_name: String::new(), since_ms, need: Need::Plugin { ask: crate::plugins::ask_of(&p) }, subagent: None });
    }
    Ok(order(items))
}

/// What kind of failure an error notice reports, when it is one the person can act on (not signed in, the usage limit, the CLI or
/// the folder missing). `None` for any other; used by [`derive`] and by a task's `needs_you` flag, so the two agree.
pub fn failure_of(part: &neko_protocol::Part) -> Option<FailureKind> {
    match &part.body {
        PartBody::Notice { tone: Tone::Error, text, failure, .. } => failure.or_else(|| written_before_6(text)),
        _ => None,
    }
}

/// Oldest first, grouped by workspace: groups in the order of their oldest item, items in the order they began to wait.
pub fn order(mut items: Vec<NeedsItem>) -> Vec<NeedsItem> {
    let mut first: HashMap<String, i64> = HashMap::new();
    for i in &items {
        let e = first.entry(i.workspace.clone()).or_insert(i.since_ms);
        *e = (*e).min(i.since_ms);
    }
    items.sort_by(|a, b| (first[&a.workspace], &a.workspace, a.since_ms, &a.id).cmp(&(first[&b.workspace], &b.workspace, b.since_ms, &b.id)));
    items
}

/// What a run's last words ask, in a line or two: their last paragraph, without Markdown's marks, cut to the sentences at its end
/// that fit [`QUESTION_CHARS`].
pub fn question_of(words: &str) -> String {
    let para = words.split("\n\n").map(str::trim).filter(|p| !p.is_empty()).last().unwrap_or("");
    let plain: String = para.replace("**", "").replace("__", "").replace('`', "");
    let line = plain.lines().map(|l| l.trim().trim_start_matches(['#', '>', '-', '*']).trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join(" ");
    let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= QUESTION_CHARS {
        return line;
    }
    // The sentences at the end that fit.
    let mut start = None;
    for (i, w) in chars.windows(2).enumerate() {
        if matches!(w[0], '.' | '?' | '!' | ':') && w[1] == ' ' && chars.len() - (i + 2) <= QUESTION_CHARS {
            start = Some(i + 2);
            break;
        }
    }
    match start {
        Some(s) => chars[s..].iter().collect(),
        None => format!("…{}", chars[chars.len() - (QUESTION_CHARS - 1)..].iter().collect::<String>()),
    }
}

/// The one line to copy in what to do next: its first `code` span ("Run `claude auth login` in Terminal").
pub fn command_of(next: &str) -> Option<String> {
    let (_, rest) = next.split_once('`')?;
    let (code, _) = rest.split_once('`')?;
    Some(code.trim().to_owned()).filter(|c| !c.is_empty())
}

/// A notice Neko wrote before version 6 carries no kind: its own words say which it was.
fn written_before_6(text: &str) -> Option<FailureKind> {
    if text.ends_with(" is not signed in.") {
        Some(FailureKind::NotSignedIn)
    } else if text.ends_with(" has reached its usage limit.") {
        Some(FailureKind::UsageLimit)
    } else if text.ends_with(" is not installed on this Mac.") {
        Some(FailureKind::NotInstalled)
    } else if text.starts_with("The folder ") && text.ends_with(" is gone.") {
        Some(FailureKind::FolderMissing)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_question_is_its_last_paragraph_in_a_line_or_two() {
        assert_eq!(question_of("I looked.\n\nShould I rename **all four**, or keep `Settings`?"), "Should I rename all four, or keep Settings?");
        assert_eq!(question_of("- Which branch\n  should it go on?"), "Which branch should it go on?");
        let long = format!("{} Which one should I keep?", "This is a long sentence about the many things I found in the folder.".repeat(4));
        let q = question_of(&long);
        assert!(q.chars().count() <= QUESTION_CHARS && q.ends_with("Which one should I keep?"), "{q}");
        let one = format!("{}?", "word ".repeat(80));
        let q = question_of(&one);
        assert!(q.starts_with('…') && q.chars().count() == QUESTION_CHARS, "{q}");
    }

    #[test]
    fn the_line_to_copy_is_the_first_code_span() {
        assert_eq!(command_of("Run `claude auth login` in Terminal, then send a follow-up.").as_deref(), Some("claude auth login"));
        assert_eq!(command_of("Send a follow-up once the limit has reset to carry on."), None);
    }

    #[test]
    fn notices_from_before_version_6_still_say_their_kind() {
        assert_eq!(written_before_6("Claude Code is not signed in."), Some(FailureKind::NotSignedIn));
        assert_eq!(written_before_6("Claude Code has reached its usage limit."), Some(FailureKind::UsageLimit));
        assert_eq!(written_before_6("Claude Code is not installed on this Mac."), Some(FailureKind::NotInstalled));
        assert_eq!(written_before_6("The folder /Users/example/notes is gone."), Some(FailureKind::FolderMissing));
        assert_eq!(written_before_6("Claude Code stopped unexpectedly (exit code 3)."), None);
    }
}
