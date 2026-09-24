//! What Neko has learned about the user. Visible, editable, and bounded.
//!
//! Memory shapes how Neko works (preferences, conventions, past decisions). It
//! is never authority: prompts present it as the user's stated preferences,
//! which cannot grant tools, permissions or publication.
use crate::Db;
use neko_protocol::workbench::{MemoryEntry, MemoryKind};

const SETTING: &str = "neko_memory_v1";
pub const MAX_ENTRIES: usize = 300;
/// A separate bounded allowance means a full user memory page cannot prevent
/// cancelling a running task. Older automatic decisions remain in task events.
pub const MAX_DECISIONS: usize = 64;
pub const MAX_TEXT: usize = 500;
/// Memory included in any one prompt.
pub const PROMPT_BUDGET: usize = 4 * 1024;

pub fn load(db: &Db) -> Result<Vec<MemoryEntry>, String> {
    match db.get_setting(SETTING).map_err(|e| e.to_string())? {
        None => Ok(Vec::new()),
        Some(json) => serde_json::from_str(&json).map_err(|e| format!("Cannot read Neko memory: {e}")),
    }
}

fn save(db: &Db, entries: &[MemoryEntry]) -> Result<(), String> {
    let json = serde_json::to_string(entries).map_err(|e| e.to_string())?;
    db.set_setting(SETTING, &json).map_err(|e| e.to_string())
}

fn clean(text: &str) -> Result<String, String> {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        return Err("A memory needs some text".into());
    }
    if text.len() > MAX_TEXT {
        return Err(format!("Keep a memory under {MAX_TEXT} characters"));
    }
    Ok(text)
}

/// Add (empty id) or replace an entry. Workspace notes must name a workspace;
/// profile entries never do. Returns the saved entry.
pub fn upsert(db: &Db, mut entry: MemoryEntry, known_workspaces: &[String]) -> Result<MemoryEntry, String> {
    entry.text = clean(&entry.text)?;
    match (entry.kind, &entry.workspace_id) {
        (MemoryKind::Profile, _) => entry.workspace_id = None,
        (MemoryKind::Workspace, None) => return Err("Choose which workspace this note is about".into()),
        _ => {}
    }
    if let Some(id) = &entry.workspace_id {
        if !known_workspaces.iter().any(|w| w == id) {
            return Err("That workspace no longer exists".into());
        }
    }
    let now = crate::now_unix_ms();
    let mut entries = load(db)?;
    if entry.id.is_empty() {
        // The same fact twice is one memory.
        if let Some(existing) = entries.iter_mut().find(|e| e.agent_profile_id == entry.agent_profile_id && e.kind == entry.kind && e.workspace_id == entry.workspace_id && e.text.eq_ignore_ascii_case(&entry.text) && (entry.kind != MemoryKind::Decision || e.source == entry.source)) {
            existing.updated_at_ms = now;
            let saved = existing.clone();
            save(db, &entries)?;
            return Ok(saved);
        }
        if entries.iter().filter(|e| !e.id.starts_with("decision-")).count() >= MAX_ENTRIES {
            return Err(format!("Memory is full ({MAX_ENTRIES} entries). Delete some on the Memory page"));
        }
        entry.id = crate::workbench::new_id();
        entry.created_at_ms = now;
        entry.updated_at_ms = now;
        entries.push(entry.clone());
    } else {
        let existing = entries.iter_mut().find(|e| e.id == entry.id).ok_or("That memory no longer exists")?;
        if existing.agent_profile_id != entry.agent_profile_id { return Err("A memory cannot be moved between agents".into()); }
        entry.created_at_ms = existing.created_at_ms;
        entry.updated_at_ms = now;
        *existing = entry.clone();
    }
    save(db, &entries)?;
    Ok(entry)
}

/// Host-authored account of a successful user action, committed in the same
/// transaction as that action. Never stores an inferred preference or reason.
pub fn record_decision(db: &Db, mut entry: MemoryEntry) -> Result<(), String> {
    entry.text = clean(&entry.text)?;
    if entry.kind != MemoryKind::Decision || entry.workspace_id.is_none() {
        return Err("Ticket decisions require their exact workspace".into());
    }
    let mut entries = load(db)?;
    if entries.iter().any(|e| e.source == entry.source && e.agent_profile_id == entry.agent_profile_id && e.text == entry.text) { return Ok(()); }
    while entries.iter().filter(|e| e.id.starts_with("decision-")).count() >= MAX_DECISIONS {
        let index = entries.iter().enumerate().filter(|(_, e)| e.id.starts_with("decision-")).min_by_key(|(_, e)| e.created_at_ms).map(|(i, _)| i).ok_or("Decision history unavailable")?;
        entries.remove(index);
    }
    entry.id = format!("decision-{}", crate::workbench::new_id());
    entry.created_at_ms = crate::now_unix_ms();
    entry.updated_at_ms = entry.created_at_ms;
    entries.push(entry);
    save(db, &entries)
}

pub fn delete(db: &Db, id: &str) -> Result<(), String> {
    let mut entries = load(db)?;
    let before = entries.len();
    entries.retain(|e| e.id != id);
    if entries.len() == before {
        return Err("That memory no longer exists".into());
    }
    save(db, &entries)
}

/// Removing a workspace's notes when the workspace itself is gone.
pub fn retain_workspaces(db: &Db, known_workspaces: &[String]) -> Result<(), String> {
    let mut entries = load(db)?;
    let before = entries.len();
    entries.retain(|e| e.workspace_id.as_ref().is_none_or(|w| known_workspaces.contains(w)));
    if entries.len() != before { save(db, &entries)?; }
    Ok(())
}

/// Format already profile-filtered memory. Production callers must use
/// `agent_profiles::context`, which enforces ownership and explicit sharing.
/// This layer only narrows workspace scope and applies the fixed text budget.
pub(crate) fn for_prompt(entries: &[MemoryEntry], scope: Option<&str>) -> String {
    let mut relevant: Vec<&MemoryEntry> = entries
        .iter()
        .filter(|e| match &e.workspace_id {
            None => true,
            Some(w) => scope == Some(w.as_str()),
        })
        .collect();
    relevant.sort_by_key(|e| std::cmp::Reverse(e.updated_at_ms));
    let mut lines = Vec::new();
    let mut used = 0;
    for e in relevant {
        let label = match e.kind {
            MemoryKind::Profile => "about the user",
            MemoryKind::Workspace => "this workspace",
            MemoryKind::Decision => "past decision",
        };
        let line = format!("- ({label}) {}", e.text);
        used += line.len() + 1;
        if used > PROMPT_BUDGET {
            break;
        }
        lines.push(line);
    }
    if lines.is_empty() { "(nothing yet)".into() } else { lines.join("\n") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(kind: MemoryKind, ws: Option<&str>, text: &str) -> MemoryEntry {
        MemoryEntry { agent_profile_id: neko_protocol::agent_profiles::default_profile_id(), id: String::new(), kind, workspace_id: ws.map(Into::into), text: text.into(), source: "user".into(), created_at_ms: 0, updated_at_ms: 0 }
    }

    #[test]
    fn add_dedupe_edit_delete() {
        let db = Db::open_in_memory().unwrap();
        let ws = vec!["w".to_string()];
        let a = upsert(&db, entry(MemoryKind::Profile, Some("w"), "  Prefers   small PRs "), &ws).unwrap();
        assert_eq!(a.text, "Prefers small PRs");
        assert_eq!(a.workspace_id, None, "profile memory is never workspace-bound");
        upsert(&db, entry(MemoryKind::Profile, None, "prefers small prs"), &ws).unwrap();
        assert_eq!(load(&db).unwrap().len(), 1, "same fact is one memory");
        let edited = upsert(&db, MemoryEntry { text: "Prefers PRs under 300 lines".into(), ..a.clone() }, &ws).unwrap();
        assert_eq!(edited.id, a.id);
        delete(&db, &a.id).unwrap();
        assert!(load(&db).unwrap().is_empty());
        assert!(delete(&db, &a.id).is_err());
    }

    #[test]
    fn workspace_notes_need_a_real_workspace_and_text() {
        let db = Db::open_in_memory().unwrap();
        let ws = vec!["w".to_string()];
        assert!(upsert(&db, entry(MemoryKind::Workspace, None, "x"), &ws).is_err());
        assert!(upsert(&db, entry(MemoryKind::Workspace, Some("gone"), "x"), &ws).is_err());
        assert!(upsert(&db, entry(MemoryKind::Profile, None, "   "), &ws).is_err());
        assert!(upsert(&db, entry(MemoryKind::Profile, None, &"x".repeat(MAX_TEXT + 1)), &ws).is_err());
    }

    #[test]
    fn prompts_get_profile_plus_only_the_scoped_workspace() {
        let entries = vec![
            MemoryEntry { updated_at_ms: 1, ..entry(MemoryKind::Profile, None, "likes pytest") },
            MemoryEntry { updated_at_ms: 2, ..entry(MemoryKind::Workspace, Some("a"), "hme uses make test") },
            MemoryEntry { updated_at_ms: 3, ..entry(MemoryKind::Workspace, Some("b"), "tcc secret convention") },
        ];
        let text = for_prompt(&entries, Some("a"));
        assert!(text.contains("likes pytest") && text.contains("make test"));
        assert!(!text.contains("tcc secret"));
        assert!(!for_prompt(&entries, None).contains("make test"));
        assert_eq!(for_prompt(&[], None), "(nothing yet)");
    }

    #[test]
    fn memory_is_bounded() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..MAX_ENTRIES {
            upsert(&db, entry(MemoryKind::Profile, None, &format!("fact {i}")), &[]).unwrap();
        }
        assert!(upsert(&db, entry(MemoryKind::Profile, None, "one more"), &[]).unwrap_err().contains("full"));
        let many: Vec<MemoryEntry> = (0..400).map(|i| entry(MemoryKind::Profile, None, &format!("{i} {}", "x".repeat(400)))).collect();
        assert!(for_prompt(&many, None).len() <= PROMPT_BUDGET);
    }

    #[test]
    fn deleting_a_workspace_drops_its_notes() {
        let db = Db::open_in_memory().unwrap();
        let ws = vec!["a".to_string(), "b".to_string()];
        upsert(&db, entry(MemoryKind::Workspace, Some("a"), "a note"), &ws).unwrap();
        upsert(&db, entry(MemoryKind::Profile, None, "me"), &ws).unwrap();
        retain_workspaces(&db, &["b".to_string()]).unwrap();
        let left = load(&db).unwrap();
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].text, "me");
    }
    #[test]
    fn bounded_decision_history_does_not_consume_user_memory_capacity() {
        let db = Db::open_in_memory().unwrap();
        for i in 0..MAX_ENTRIES { upsert(&db, entry(MemoryKind::Profile, None, &format!("fact {i}")), &[]).unwrap(); }
        for i in 0..(MAX_DECISIONS + 2) {
            let mut decision = entry(MemoryKind::Decision, Some("w"), "Cancelled this task.");
            decision.source = format!("ticket:{i}:cancellation:0");
            record_decision(&db, decision).unwrap();
        }
        let memories = load(&db).unwrap();
        assert_eq!(memories.len(), MAX_ENTRIES + MAX_DECISIONS);
        assert_eq!(memories.iter().filter(|m| m.kind == MemoryKind::Profile).count(), MAX_ENTRIES);
    }
}
