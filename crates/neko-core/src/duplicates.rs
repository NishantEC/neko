//! The same problem filed twice becomes one ticket.
//!
//! Measured 2026-10-18: four Linear issues were imported twice two minutes
//! apart (two watches on the same connection, once titled `HEA-3006: …` and
//! once without the key), and one Sentry error was filed five times. Before a
//! ticket is created, an open ticket in the same workspace is looked for with
//! the same source key (a Linear key or Sentry short id, from a watched
//! source or a title prefix) or, in the same repository, the same title once
//! keys, case and punctuation are set aside. A match gets a note instead of a
//! twin. Finished tickets never match: filing again after one is done is a
//! regression or a retry, which is new work. Problems that are the same but
//! worded differently are left to the model triage (`crate::triage`).

use neko_protocol::workbench::{Snapshot, Task, TaskStatus};

/// Titles shorter than this (once normalized) are too generic to merge on.
const MIN_TITLE_CHARS: usize = 12;

/// Issue keys in `text`: `HEA-3006`, `CONNECT-PROD-P7W`. Upper-case words
/// joined by hyphens whose last part holds a digit; the first part has at
/// least two letters.
pub fn issue_keys(text: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for token in text.split(|c: char| !(c.is_ascii_alphanumeric() || c == '-')) {
        let token = token.trim_matches('-');
        let parts: Vec<&str> = token.split('-').collect();
        let well_formed = parts.len() >= 2
            && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()))
            && parts[0].len() >= 2
            && parts[0].chars().all(|c| c.is_ascii_uppercase())
            && parts.last().is_some_and(|p| p.chars().any(|c| c.is_ascii_digit()));
        if well_formed && !keys.iter().any(|k| k == token) {
            keys.push(token.to_owned());
        }
    }
    keys
}

/// The key a title starts with (`HEA-3006: Fix …`, `[HEA-3006] Fix …`).
fn leading_key(title: &str) -> Option<String> {
    let head = title.trim_start().trim_start_matches('[');
    let end = head.find(|c: char| !(c.is_ascii_alphanumeric() || c == '-')).unwrap_or(head.len());
    issue_keys(&head[..end]).into_iter().next()
}

/// Lower case, punctuation as spaces, a leading issue key removed.
pub fn normalized_title(title: &str) -> String {
    let mut title = title.trim();
    if let Some(key) = leading_key(title) {
        let at = title.find(&key).map_or(0, |i| i + key.len());
        title = title[at..].trim_start_matches([']', ':', ' ', '-', '—', '–', '|']);
    }
    title
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_lowercase().next().unwrap_or(c) } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn open(task: &Task) -> bool {
    !matches!(task.status, TaskStatus::Completed | TaskStatus::Cancelled | TaskStatus::Failed)
}

/// Source keys a ticket is known by: its watched sources' ids, its title's
/// leading key and its linked issue id.
fn keys_of(snapshot: &Snapshot, task: &Task) -> Vec<String> {
    let mut keys: Vec<String> = snapshot.mcp.sources.iter()
        .filter(|s| s.task_id.as_deref() == Some(task.id.as_str()))
        .map(|s| s.external_id.trim().to_ascii_uppercase())
        .collect();
    keys.extend(leading_key(&task.title));
    keys
}

/// An open ticket in `workspace_id` that a new ticket would duplicate.
/// `repository` is the new ticket's checkout when known; titles only match
/// within one repository (a ticket whose repository is not chosen yet
/// matches any). `source_key` is the watched source's id, if any.
pub fn duplicate_of(snapshot: &Snapshot, workspace_id: &str, repository: Option<&str>, title: &str, source_key: Option<&str>) -> Option<String> {
    let workspace = snapshot.workspaces.iter().find(|w| w.id == workspace_id)?;
    let mut wanted: Vec<String> = source_key.map(|k| k.trim().to_ascii_uppercase()).into_iter().filter(|k| !k.is_empty()).collect();
    wanted.extend(leading_key(title));
    let normalized = normalized_title(title);
    let candidates = snapshot.tasks.iter().filter(|t| t.workspace_id == workspace_id && open(t));
    // A shared source key is the strongest evidence; prefer it over a title.
    let by_key = candidates.clone().find(|t| {
        !wanted.is_empty() && keys_of(snapshot, t).iter().any(|k| wanted.contains(k))
    });
    if let Some(task) = by_key {
        return Some(task.id.clone());
    }
    if normalized.chars().count() < MIN_TITLE_CHARS {
        return None;
    }
    candidates
        .filter(|t| {
            let root = snapshot.root_for(t, workspace);
            let unknown = |r: &str| r == workspace.repository;
            repository.is_none_or(|r| r == root || unknown(r) || unknown(&root))
        })
        .find(|t| normalized_title(&t.title) == normalized)
        .map(|t| t.id.clone())
}

/// Records on `existing` that the same problem was filed again.
pub fn note_refiled(snapshot: &mut Snapshot, existing: &str, title: &str, from: &str) {
    if let Some(task) = snapshot.tasks.iter_mut().find(|t| t.id == existing) {
        let short: String = title.chars().take(200).collect();
        crate::workbench::append_event(task, "neko", &format!("Filed again {from}: “{short}”. Tracked here instead of opening a second ticket."));
        task.updated_at_ms = crate::workbench::now_ms();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::Workspace;

    fn snapshot() -> Snapshot {
        let mut s = Snapshot {
            workspaces: vec![Workspace { id: "w".into(), name: "Home".into(), repository: "/home".into(), instructions: String::new(), away_enabled: false }],
            ..Default::default()
        };
        for (id, title, status) in [
            ("linear", "HEA-3006: Coaches see only the reports shared for this review", TaskStatus::AwaitingApproval),
            ("sentry", "ErrorFromResponse: StreamChat error code 9: GetOrCreateChannel failed — Too many requests", TaskStatus::ReadyForReview),
            ("done", "TypeError: Converting circular structure to JSON", TaskStatus::Completed),
        ] {
            let mut task = crate::workbench::create_task("w".into(), None, title.into(), "g".into()).unwrap();
            task.id = id.into();
            task.status = status;
            s.tasks.push(task);
        }
        s.task_roots.insert("sentry".into(), "/athena".into());
        s
    }

    #[test]
    fn keys_come_from_linear_and_sentry_identifiers_only() {
        assert_eq!(issue_keys("HEA-3006: fix, see CONNECT-PROD-P7W and UTF-8"), vec!["HEA-3006", "CONNECT-PROD-P7W", "UTF-8"]);
        assert!(issue_keys("Fix the log-in page (E-mail)").is_empty());
        assert_eq!(normalized_title("HEA-3006: Coaches see only the reports"), "coaches see only the reports");
        assert_eq!(normalized_title("[HEA-3006] Coaches see ONLY the reports!"), "coaches see only the reports");
    }

    #[test]
    fn the_same_issue_filed_again_matches_the_open_ticket() {
        let s = snapshot();
        // Imported again without its key in the title (the real 10-16 pairs).
        assert_eq!(duplicate_of(&s, "w", None, "Coaches see only the reports shared for this review", None).as_deref(), Some("linear"));
        // Same key, different wording.
        assert_eq!(duplicate_of(&s, "w", None, "Reports for review calls", Some("hea-3006")).as_deref(), Some("linear"));
        // A source the open ticket came from.
        let mut s = s;
        s.mcp.sources.push(neko_protocol::mcp_host::SourceEvidence {
            task_id: Some("sentry".into()), id: "s".into(), responsibility_id: "r".into(), connection_id: "c".into(),
            external_id: "CONNECT-PROD-P7W".into(), revision: "1".into(), title: String::new(), description: String::new(),
            retrieved_ms: 0, receipt_ids: vec![], eligible: true,
        });
        assert_eq!(duplicate_of(&s, "w", None, "GetOrCreateChannel rate limit", Some("CONNECT-PROD-P7W")).as_deref(), Some("sentry"));
    }

    #[test]
    fn different_repositories_finished_tickets_and_generic_titles_do_not_merge() {
        let s = snapshot();
        let sentry_title = "ErrorFromResponse: StreamChat error code 9: GetOrCreateChannel failed — Too many requests";
        assert_eq!(duplicate_of(&s, "w", Some("/athena"), sentry_title, None).as_deref(), Some("sentry"));
        assert_eq!(duplicate_of(&s, "w", Some("/navihealth"), sentry_title, None), None, "same words, another repository");
        assert_eq!(duplicate_of(&s, "w", None, "TypeError: Converting circular structure to JSON", None), None, "a finished ticket is not reopened by a twin");
        assert_eq!(duplicate_of(&s, "w", None, "Fix bug", None), None);
        assert_eq!(duplicate_of(&s, "other", None, "Coaches see only the reports shared for this review", None), None);
    }
}
