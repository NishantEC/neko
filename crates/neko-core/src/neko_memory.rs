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
        Some(json) => {
            serde_json::from_str(&json).map_err(|e| format!("Cannot read Neko memory: {e}"))
        }
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
    if let Some(kind) = sensitive(&text) {
        return Err(format!("Neko doesn’t keep {kind} in memory. Store them in Keychain or a password manager instead"));
    }
    Ok(text)
}

/// Why a text must never become a memory, or None when it is fine.
///
/// Memory is sent to models and kept on disk, so it refuses actual secret
/// values whoever proposes them: the user, a learning job or a decision.
/// It targets values (a key, a card number, "password is ..."), so a
/// preference such as "never commit API keys" is still allowed.
pub fn sensitive(text: &str) -> Option<&'static str> {
    let lower = text.to_ascii_lowercase();
    if lower.contains("-----begin") && lower.contains("private key") {
        return Some("private keys");
    }
    for word in text.split(|c: char| c.is_whitespace() || matches!(c, '"' | '\'' | '\x60' | ',' | ';' | '(' | ')' | '<' | '>')) {
        if looks_like_credential(word) {
            return Some("keys and tokens");
        }
    }
    if has_card_number(text) {
        return Some("card numbers");
    }
    if has_government_id(text) {
        return Some("government ID numbers");
    }
    if has_labelled_secret(&lower) {
        return Some("passwords, codes and keys");
    }
    None
}

fn looks_like_credential(word: &str) -> bool {
    let word = word.trim_matches(|c: char| matches!(c, '.' | ':' | '='));
    let after_eq = word.rsplit_once('=').map_or(word, |(_, value)| value);
    const PREFIXES: &[(&str, usize)] = &[
        ("sk-", 20), ("sk_live_", 16), ("rk_live_", 16), ("ghp_", 30), ("gho_", 30), ("ghs_", 30),
        ("github_pat_", 30), ("glpat-", 20), ("xoxb-", 20), ("xoxp-", 20), ("xoxa-", 20),
        ("akia", 20), ("aiza", 35), ("lin_api_", 30), ("npm_", 30),
    ];
    let lower = after_eq.to_ascii_lowercase();
    if PREFIXES.iter().any(|(prefix, min)| lower.starts_with(prefix) && after_eq.len() >= *min) {
        return true;
    }
    // JSON Web Tokens: three base64url segments, the first a JSON header.
    if after_eq.starts_with("eyJ") && after_eq.matches('.').count() == 2 && after_eq.len() >= 40 {
        return true;
    }
    // A long random token: mixed case and digits, no path or URL structure.
    after_eq.len() >= 32
        && !after_eq.contains('/')
        && after_eq.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        && after_eq.chars().any(|c| c.is_ascii_uppercase())
        && after_eq.chars().any(|c| c.is_ascii_lowercase())
        && after_eq.chars().any(|c| c.is_ascii_digit())
}

/// 13–19 digits, optionally grouped by spaces or dashes, passing Luhn.
fn has_card_number(text: &str) -> bool {
    let chars: Vec<char> = text.chars().collect();
    let mut start = 0;
    while start < chars.len() {
        if !chars[start].is_ascii_digit() || (start > 0 && chars[start - 1].is_ascii_digit()) {
            start += 1;
            continue;
        }
        let mut digits = Vec::new();
        let mut i = start;
        while i < chars.len() && (chars[i].is_ascii_digit() || (matches!(chars[i], ' ' | '-') && i + 1 < chars.len() && chars[i + 1].is_ascii_digit() && !digits.is_empty())) {
            if let Some(d) = chars[i].to_digit(10) {
                digits.push(d);
            }
            i += 1;
        }
        if (13..=19).contains(&digits.len()) && luhn(&digits) {
            return true;
        }
        start = i.max(start + 1);
    }
    false
}

fn luhn(digits: &[u32]) -> bool {
    let sum: u32 = digits.iter().rev().enumerate().map(|(i, &d)| if i % 2 == 1 { let x = d * 2; if x > 9 { x - 9 } else { x } } else { d }).sum();
    sum % 10 == 0 && digits.iter().any(|&d| d != 0)
}

/// US SSN (123-45-6789) and Aadhaar (1234 5678 9012) shapes.
fn has_government_id(text: &str) -> bool {
    let groups = |sep: char, sizes: &[usize]| {
        text.split(|c: char| !(c.is_ascii_digit() || c == sep)).any(|token| {
            let parts: Vec<&str> = token.split(sep).collect();
            parts.len() == sizes.len() && parts.iter().zip(sizes).all(|(p, n)| p.len() == *n && p.bytes().all(|b| b.is_ascii_digit()))
        })
    };
    groups('-', &[3, 2, 4]) || {
        let words: Vec<&str> = text.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_ascii_digit())).collect();
        words.windows(3).any(|w| w.iter().all(|p| p.len() == 4 && p.bytes().all(|b| b.is_ascii_digit())))
            && !words.windows(4).any(|w| w.iter().all(|p| p.len() == 4 && p.bytes().all(|b| b.is_ascii_digit())))
    }
}

/// "password is hunter2", "OTP: 482913", "api key = abc...".
fn has_labelled_secret(lower: &str) -> bool {
    const LABELS: &[&str] = &[
        "password", "passcode", "passwd", "pin", "pin code", "otp", "one-time code", "one time code",
        "2fa code", "verification code", "security code", "cvv", "api key", "api token", "access token",
        "secret key", "private key", "seed phrase", "recovery phrase", "token",
    ];
    for label in LABELS {
        for (at, _) in lower.match_indices(label) {
            let before_ok = at == 0 || !lower.as_bytes()[at - 1].is_ascii_alphanumeric();
            let rest = &lower[at + label.len()..];
            if !before_ok || rest.as_bytes().first().is_some_and(u8::is_ascii_alphanumeric) {
                continue;
            }
            let rest = rest.trim_start();
            let value = if let Some(v) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=')) {
                v
            } else if let Some(v) = rest.strip_prefix("is ") {
                v
            } else {
                continue;
            };
            let value = value.trim_start().trim_start_matches(['"', '\'', '\x60']);
            let token: String = value.chars().take_while(|c| !c.is_whitespace()).collect();
            let token = token.trim_end_matches(['.', ',', '"', '\'', '\x60']);
            // A value, not a description: "is required", "is stored in Keychain".
            const WORDS: &[&str] = &["required", "stored", "kept", "saved", "needed", "in", "the", "a", "an", "not", "never", "always", "managed", "set", "missing", "optional", "secret", "private", "rotated", "changed", "expired", "valid", "invalid"];
            if token.len() >= 4 && !WORDS.contains(&token) && (token.chars().any(|c| c.is_ascii_digit()) || token.len() >= 6) {
                return true;
            }
        }
    }
    false
}


/// Add (empty id) or replace an entry. Workspace notes must name a workspace;
/// profile entries never do. Returns the saved entry.
pub fn upsert(
    db: &Db,
    mut entry: MemoryEntry,
    known_workspaces: &[String],
) -> Result<MemoryEntry, String> {
    entry.text = clean(&entry.text)?;
    match (entry.kind, &entry.workspace_id) {
        (MemoryKind::Profile, _) => entry.workspace_id = None,
        (MemoryKind::Workspace, None) => {
            return Err("Choose which workspace this note is about".into());
        }
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
        if let Some(existing) = entries.iter_mut().find(|e| {
            e.agent_profile_id == entry.agent_profile_id
                && e.kind == entry.kind
                && e.workspace_id == entry.workspace_id
                && e.text.eq_ignore_ascii_case(&entry.text)
                && (entry.kind != MemoryKind::Decision || e.source == entry.source)
        }) {
            existing.updated_at_ms = now;
            let saved = existing.clone();
            save(db, &entries)?;
            return Ok(saved);
        }
        if entries
            .iter()
            .filter(|e| !e.id.starts_with("decision-"))
            .count()
            >= MAX_ENTRIES
        {
            return Err(format!(
                "Memory is full ({MAX_ENTRIES} entries). Delete some on the Memory page"
            ));
        }
        entry.id = crate::workbench::new_id();
        entry.created_at_ms = now;
        entry.updated_at_ms = now;
        entries.push(entry.clone());
    } else {
        let existing = entries
            .iter_mut()
            .find(|e| e.id == entry.id)
            .ok_or("That memory no longer exists")?;
        if existing.agent_profile_id != entry.agent_profile_id {
            return Err("A memory cannot be moved between agents".into());
        }
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
    if entries.iter().any(|e| {
        e.source == entry.source
            && e.agent_profile_id == entry.agent_profile_id
            && e.text == entry.text
    }) {
        return Ok(());
    }
    while entries
        .iter()
        .filter(|e| e.id.starts_with("decision-"))
        .count()
        >= MAX_DECISIONS
    {
        let index = entries
            .iter()
            .enumerate()
            .filter(|(_, e)| e.id.starts_with("decision-"))
            .min_by_key(|(_, e)| e.created_at_ms)
            .map(|(i, _)| i)
            .ok_or("Decision history unavailable")?;
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
    entries.retain(|e| {
        e.workspace_id
            .as_ref()
            .is_none_or(|w| known_workspaces.contains(w))
    });
    if entries.len() != before {
        save(db, &entries)?;
    }
    Ok(())
}


/// Short, stable handle for one memory in a prompt, e.g. "m:3f9c2a".
pub fn tag(entry: &MemoryEntry) -> String {
    let id: String = entry.id.chars().filter(char::is_ascii_alphanumeric).take(6).collect();
    format!("m:{id}")
}

/// Memories a reply named by tag, in the order named, without duplicates.
pub fn resolve_tags<'a>(entries: &'a [MemoryEntry], tags: &[String]) -> Vec<&'a MemoryEntry> {
    let mut found: Vec<&MemoryEntry> = Vec::new();
    for wanted in tags {
        let wanted = wanted.trim().trim_matches(|c| c == '[' || c == ']');
        if let Some(entry) = entries.iter().find(|e| tag(e) == wanted && !e.id.is_empty()) {
            if !found.iter().any(|f| f.id == entry.id) {
                found.push(entry);
            }
        }
    }
    found
}

const STOPWORDS: &[&str] = &[
    "the", "and", "for", "that", "this", "with", "from", "into", "your", "you", "are", "was", "were", "have",
    "has", "had", "not", "but", "can", "will", "should", "would", "could", "about", "what", "when", "where",
    "which", "who", "how", "why", "all", "any", "our", "out", "use", "uses", "using", "make", "made", "then",
    "than", "them", "they", "their", "there", "here", "just", "also", "like", "want", "need", "please", "it's",
    "its", "too", "very", "some", "more", "most", "only", "always", "never", "now", "new", "get", "got", "one",
];

/// Lowercase distinctive words: three or more letters, not a stopword, with
/// a light plural/verb trim so "tests" matches "test".
pub(crate) fn words(text: &str) -> Vec<String> {
    let mut out: Vec<String> = text
        .to_lowercase()
        .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
        .map(|w| w.trim_matches(|c| c == '-' || c == '_'))
        .filter(|w| w.chars().count() >= 3 && !STOPWORDS.contains(w))
        .map(|w| {
            let w = w.strip_suffix("ing").filter(|s| s.len() >= 4).unwrap_or(w);
            let w = w.strip_suffix("es").filter(|s| s.len() >= 4).unwrap_or(w);
            w.strip_suffix('s').filter(|s| s.len() >= 3).unwrap_or(w).to_owned()
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

pub(crate) fn overlap(wanted: &[String], text: &str) -> usize {
    words(text).iter().filter(|w| wanted.contains(w)).count()
}

/// Memory requests handled by code, instantly and without a model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatIntent {
    Remember(String),
    Forget(String),
    ForgetEverything,
    List,
}

pub fn chat_intent(message: &str) -> Option<ChatIntent> {
    let text = message.trim();
    let lower = text.to_lowercase();
    let bare = lower.trim_end_matches(['?', '!', '.', ' ']);
    const LIST: &[&str] = &[
        "what do you remember", "what do you remember about me", "what do you know about me",
        "show my memories", "show memories", "list memories", "list my memories", "what have you remembered",
    ];
    if LIST.contains(&bare) {
        return Some(ChatIntent::List);
    }
    if matches!(bare, "forget everything" | "forget all" | "forget everything about me" | "clear your memory" | "clear memory") {
        return Some(ChatIntent::ForgetEverything);
    }
    let rest = |prefixes: &[&str]| {
        prefixes.iter().find_map(|p| lower.strip_prefix(p).map(|_| text[p.len()..].trim().trim_end_matches('.').trim().to_owned()))
    };
    if let Some(fact) = rest(&["remember that ", "remember: ", "please remember that ", "note that i ", "from now on, ", "from now on "]) {
        if lower.starts_with("note that i ") {
            return (!fact.is_empty()).then(|| ChatIntent::Remember(format!("I {fact}")));
        }
        return (!fact.is_empty()).then(|| ChatIntent::Remember(capitalize(&fact)));
    }
    if let Some(query) = rest(&["forget that ", "forget about ", "please forget ", "forget "]) {
        // "forget it" and friends are conversational, not a memory request.
        if !query.is_empty() && !matches!(query.to_lowercase().as_str(), "it" | "that" | "this" | "about it") {
            return Some(ChatIntent::Forget(query));
        }
    }
    None
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map(|c| c.to_uppercase().collect::<String>() + chars.as_str()).unwrap_or_default()
}

/// The memories a "forget …" request most plausibly means: the best word
/// overlap, requiring at least half of the request's distinctive words.
pub fn forget_candidates<'a>(entries: &'a [MemoryEntry], query: &str) -> Vec<&'a MemoryEntry> {
    let wanted = words(query);
    if wanted.is_empty() {
        return Vec::new();
    }
    let lower = query.to_lowercase();
    let exact: Vec<&MemoryEntry> = entries.iter().filter(|e| e.text.to_lowercase().contains(&lower)).collect();
    if !exact.is_empty() {
        return exact;
    }
    let scored: Vec<(usize, &MemoryEntry)> = entries.iter().map(|e| (overlap(&wanted, &e.text), e)).collect();
    let best = scored.iter().map(|(s, _)| *s).max().unwrap_or(0);
    if best == 0 || best * 2 < wanted.len() {
        return Vec::new();
    }
    scored.into_iter().filter(|(s, _)| *s == best).map(|(_, e)| e).collect()
}


/// Format already profile-filtered memory. Production callers must use
/// `agent_profiles::context`, which enforces ownership and explicit sharing.
/// This layer only narrows workspace scope and applies the fixed text budget.
pub(crate) fn for_prompt(entries: &[MemoryEntry], scope: Option<&str>) -> String {
    for_prompt_about(entries, scope, None)
}

/// With `about` (the request or task goal), only memories sharing a
/// distinctive word with it are included, plus standing facts about the user,
/// so the past is not dragged into every request. Each line carries a tag
/// such as [m:ab12cd] so a reply can say which memories it relied on.
pub(crate) fn for_prompt_about(entries: &[MemoryEntry], scope: Option<&str>, about: Option<&str>) -> String {
    let wanted = about.map(words);
    let mut relevant: Vec<(usize, &MemoryEntry)> = entries
        .iter()
        .filter(|e| match &e.workspace_id {
            None => true,
            Some(w) => scope == Some(w.as_str()),
        })
        // Entries saved before secret refusal existed never reach a model.
        .filter(|e| sensitive(&e.text).is_none())
        .map(|e| (wanted.as_ref().map_or(0, |w| overlap(w, &e.text)), e))
        .filter(|(score, e)| wanted.is_none() || *score > 0 || e.kind == MemoryKind::Profile)
        .collect();
    relevant.sort_by_key(|(score, e)| (std::cmp::Reverse(*score), std::cmp::Reverse(e.updated_at_ms)));
    let mut lines = Vec::new();
    let mut used = 0;
    for (_, e) in relevant {
        let label = match e.kind {
            MemoryKind::Profile => "about the user",
            MemoryKind::Workspace => "this workspace",
            MemoryKind::Decision => "past decision",
        };
        let line = format!("- [{}] ({label}) {}", tag(e), e.text);
        used += line.len() + 1;
        if used > PROMPT_BUDGET {
            break;
        }
        lines.push(line);
    }
    if lines.is_empty() {
        "(nothing yet)".into()
    } else {
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mem(id: &str, kind: MemoryKind, text: &str, at: i64) -> MemoryEntry {
        let mut e = entry(kind, if kind == MemoryKind::Profile { None } else { Some("w") }, text);
        e.id = id.into();
        e.updated_at_ms = at;
        e
    }

    #[test]
    fn prompts_carry_only_relevant_memory_with_tags() {
        let entries = vec![
            mem("aaaaaa11", MemoryKind::Profile, "Prefers short answers", 1),
            mem("bbbbbb22", MemoryKind::Workspace, "Invoices go in the Finance folder", 2),
            mem("cccccc33", MemoryKind::Workspace, "Run tests with pnpm test", 3),
            mem("dddddd44", MemoryKind::Decision, "Ruled out Redis for caching", 4),
        ];
        let text = for_prompt_about(&entries, Some("w"), Some("Fix the failing tests in CI"));
        assert!(text.contains("[m:cccccc] (this workspace) Run tests"), "{text}");
        assert!(text.contains("Prefers short answers"), "standing facts about the user always apply");
        assert!(!text.contains("Invoices") && !text.contains("Redis"), "{text}");
        assert!(text.find("Run tests").unwrap() < text.find("Prefers").unwrap(), "relevant first");
        let all = for_prompt(&entries, Some("w"));
        assert!(all.contains("Invoices") && all.contains("Redis"), "no request means no narrowing");
        let used = resolve_tags(&entries, &["m:cccccc".into(), "[m:cccccc]".into(), "m:zzzzzz".into()]);
        assert_eq!(used.len(), 1);
        assert_eq!(used[0].id, "cccccc33");
    }

    #[test]
    fn chat_memory_verbs_are_recognised_by_code() {
        assert_eq!(chat_intent("remember that my manager is Priya."), Some(ChatIntent::Remember("My manager is Priya".into())));
        assert_eq!(chat_intent("From now on, invoices go in Finance"), Some(ChatIntent::Remember("Invoices go in Finance".into())));
        assert_eq!(chat_intent("What do you remember about me?"), Some(ChatIntent::List));
        assert_eq!(chat_intent("forget that I use Redis"), Some(ChatIntent::Forget("I use Redis".into())));
        assert_eq!(chat_intent("Forget everything"), Some(ChatIntent::ForgetEverything));
        assert_eq!(chat_intent("forget it"), None);
        assert_eq!(chat_intent("remember to call mom"), None, "a reminder, not a memory");
        assert_eq!(chat_intent("Fix the remember-me checkbox"), None);
        let entries = vec![
            mem("aaaaaa11", MemoryKind::Profile, "Uses Redis for queues", 1),
            mem("bbbbbb22", MemoryKind::Profile, "Uses Redis for caching", 2),
            mem("cccccc33", MemoryKind::Profile, "Uses Postgres for storage", 3),
        ];
        assert_eq!(forget_candidates(&entries, "postgres").len(), 1);
        assert_eq!(forget_candidates(&entries, "redis").len(), 2, "ambiguous: both match");
        assert_eq!(forget_candidates(&entries, "the redis caching thing").len(), 1);
        assert!(forget_candidates(&entries, "kubernetes").is_empty());
    }

    #[test]
    fn secret_values_are_refused_but_preferences_about_them_are_kept() {
        for secret in [
            "my openai key is sk-proj-abcdefghijklmnopqrstuvwxyz123456",
            "github token ghp_abcdefghijklmnopqrstuvwxyz0123456789",
            "card 4242 4242 4242 4242 for the test account",
            "wifi password is hunter22",
            "OTP: 482913",
            "api key = Zq81jdLmnPq",
            "SSN 123-45-6789",
            "aadhaar 2345 6789 0123",
            "use AKIAIOSFODNN7EXAMPLE for staging",
            "jwt eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxMjM0NTY3ODkwIn0.dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
            "-----BEGIN OPENSSH PRIVATE KEY----- abc",
        ] {
            assert!(sensitive(secret).is_some(), "should refuse: {secret}");
            assert!(clean(secret).unwrap_err().contains("doesn’t keep"), "{secret}");
        }
        for fine in [
            "Never commit API keys; store them in Keychain",
            "The password is stored in 1Password",
            "Prefer pnpm over npm in this repo",
            "Base branch is main; commit 3f9c2a1b7e8d4c6a9b0e1f2a3b4c5d6e7f8a9b0c is the release",
            "Call 4111 when the build breaks",
            "Phone OTP flow lives in auth/otp.rs",
            "Run tests with cargo test --workspace -- --test-threads=4",
            "The token is required for the staging API",
        ] {
            assert_eq!(sensitive(fine), None, "should keep: {fine}");
        }
    }

    fn entry(kind: MemoryKind, ws: Option<&str>, text: &str) -> MemoryEntry {
        MemoryEntry {
            agent_profile_id: neko_protocol::agent_profiles::default_profile_id(),
            id: String::new(),
            kind,
            workspace_id: ws.map(Into::into),
            text: text.into(),
            source: "user".into(),
            created_at_ms: 0,
            updated_at_ms: 0,
        }
    }

    #[test]
    fn add_dedupe_edit_delete() {
        let db = Db::open_in_memory().unwrap();
        let ws = vec!["w".to_string()];
        let a = upsert(
            &db,
            entry(MemoryKind::Profile, Some("w"), "  Prefers   small PRs "),
            &ws,
        )
        .unwrap();
        assert_eq!(a.text, "Prefers small PRs");
        assert_eq!(
            a.workspace_id, None,
            "profile memory is never workspace-bound"
        );
        upsert(
            &db,
            entry(MemoryKind::Profile, None, "prefers small prs"),
            &ws,
        )
        .unwrap();
        assert_eq!(load(&db).unwrap().len(), 1, "same fact is one memory");
        let edited = upsert(
            &db,
            MemoryEntry {
                text: "Prefers PRs under 300 lines".into(),
                ..a.clone()
            },
            &ws,
        )
        .unwrap();
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
        assert!(
            upsert(
                &db,
                entry(MemoryKind::Profile, None, &"x".repeat(MAX_TEXT + 1)),
                &ws
            )
            .is_err()
        );
    }

    #[test]
    fn prompts_get_profile_plus_only_the_scoped_workspace() {
        let entries = vec![
            MemoryEntry {
                updated_at_ms: 1,
                ..entry(MemoryKind::Profile, None, "likes pytest")
            },
            MemoryEntry {
                updated_at_ms: 2,
                ..entry(MemoryKind::Workspace, Some("a"), "hme uses make test")
            },
            MemoryEntry {
                updated_at_ms: 3,
                ..entry(MemoryKind::Workspace, Some("b"), "tcc secret convention")
            },
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
            upsert(
                &db,
                entry(MemoryKind::Profile, None, &format!("fact {i}")),
                &[],
            )
            .unwrap();
        }
        assert!(
            upsert(&db, entry(MemoryKind::Profile, None, "one more"), &[])
                .unwrap_err()
                .contains("full")
        );
        let many: Vec<MemoryEntry> = (0..400)
            .map(|i| {
                entry(
                    MemoryKind::Profile,
                    None,
                    &format!("{i} {}", "x".repeat(400)),
                )
            })
            .collect();
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
        for i in 0..MAX_ENTRIES {
            upsert(
                &db,
                entry(MemoryKind::Profile, None, &format!("fact {i}")),
                &[],
            )
            .unwrap();
        }
        for i in 0..(MAX_DECISIONS + 2) {
            let mut decision = entry(MemoryKind::Decision, Some("w"), "Cancelled this task.");
            decision.source = format!("ticket:{i}:cancellation:0");
            record_decision(&db, decision).unwrap();
        }
        let memories = load(&db).unwrap();
        assert_eq!(memories.len(), MAX_ENTRIES + MAX_DECISIONS);
        assert_eq!(
            memories
                .iter()
                .filter(|m| m.kind == MemoryKind::Profile)
                .count(),
            MAX_ENTRIES
        );
    }
}
