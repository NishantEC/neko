//! The Inbox's "needs you" items stay few without the owner cleaning them:
//! each one closes itself when its pull request is merged or closed, when
//! the time it was tied to has passed, or when nothing has touched it for a
//! while, and a repeat of an open item refreshes it instead of adding
//! another. Pure rules live here; the daemon's `routines` module applies
//! them to the stored runs. A closed item keeps its reason.
//! See docs/knowledge/neko-runs-the-day.md.
use neko_protocol::routines::NeedsYou;

pub const DAY_MS: i64 = 24 * 60 * 60 * 1000;
/// Days without activity before an item closes ("Aged out"). 0 turns it off.
pub const DEFAULT_AGE_OUT_DAYS: u32 = 7;
pub const MAX_AGE_OUT_DAYS: u32 = 90;
/// How long a closed item stays listed under "Closed recently".
pub const RECENT_CLOSED_DAYS: i64 = 3;

pub const PR_MERGED: &str = "PR merged";
pub const PR_CLOSED: &str = "PR closed";
pub const EXPIRED: &str = "Expired";
pub const AGED_OUT: &str = "Aged out";
pub const BY_ROUTINE: &str = "Resolved by routine";
pub const BY_YOU: &str = "Done by you";
pub const ON_PHONE: &str = "Done on your phone";
/// Closed because the owner answered it (typed on the phone or tapped the default).
pub const ANSWERED_ON_PHONE: &str = "Answered on your phone";
pub const ANSWERED: &str = "Answered by you";

/// One open item as the prompt lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenItem {
    pub title: String,
    pub age_days: i64,
    /// ISO time, when the item has one.
    pub expires: Option<String>,
}

/// A time as a routine writes it: RFC 3339, the same without an offset
/// (UTC), a date and time without seconds, or a bare date (its last second).
pub fn parse_time_ms(text: &str) -> Option<i64> {
    use chrono::{NaiveDate, NaiveDateTime, TimeZone, Utc};
    let text = text.trim();
    if let Ok(t) = chrono::DateTime::parse_from_rfc3339(text) {
        return Some(t.timestamp_millis());
    }
    for format in ["%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d %H:%M"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(text, format) {
            return Some(Utc.from_utc_datetime(&t).timestamp_millis());
        }
    }
    let day = NaiveDate::parse_from_str(text, "%Y-%m-%d").ok()?;
    Some(Utc.from_utc_datetime(&day.and_hms_opt(23, 59, 59)?).timestamp_millis())
}

pub fn iso_from_ms(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)).unwrap_or_default()
}

/// A pull request an item names.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrRef {
    pub owner: String,
    pub repo: String,
    pub number: u64,
}

impl PrRef {
    pub fn key(&self) -> String {
        format!("{}/{}#{}", self.owner, self.repo, self.number)
    }
}

fn name_ok(s: &str) -> bool {
    !s.is_empty() && s.len() <= 100 && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The pull requests an item's link or detail names, as a pull URL or
/// `owner/repo#12`; at most three, in order of appearance.
pub fn pr_refs(item: &NeedsYou) -> Vec<PrRef> {
    let mut found: Vec<PrRef> = Vec::new();
    let mut add = |r: PrRef| {
        if found.len() < 3 && !found.contains(&r) {
            found.push(r);
        }
    };
    let text = format!("{} {}", item.link.as_deref().unwrap_or_default(), item.detail);
    for token in text.split(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | '[' | ']' | '<' | '>' | ',' | ';' | '"' | '\'' | '`')) {
        let token = token.trim_end_matches(['.', ':', '!', '?']);
        if let Some(rest) = token.split("github.com/").nth(1) {
            let mut parts = rest.split('/');
            if let (Some(owner), Some(repo), Some("pull"), Some(n)) = (parts.next(), parts.next(), parts.next(), parts.next()) {
                let digits: String = n.chars().take_while(char::is_ascii_digit).collect();
                if let (true, true, Ok(number)) = (name_ok(owner), name_ok(repo), digits.parse::<u64>()) {
                    add(PrRef { owner: owner.into(), repo: repo.into(), number });
                }
            }
        } else if let Some((left, right)) = token.split_once('#') {
            if let Some((owner, repo)) = left.split_once('/') {
                if let (true, true, Ok(number)) = (name_ok(owner), name_ok(repo), right.parse::<u64>()) {
                    add(PrRef { owner: owner.into(), repo: repo.into(), number });
                }
            }
        }
    }
    found
}

/// What two titles must share to be the same item: the normalised title
/// (`duplicates::normalized_title`) behind its leading issue key, so two
/// tickets with one wording stay two items; the lower-cased text when nothing
/// letter-like is left.
pub fn title_key(title: &str) -> String {
    let key = crate::duplicates::normalized_title(title);
    if key.is_empty() {
        return title.trim().to_lowercase();
    }
    match crate::duplicates::leading_key(title) {
        Some(lead) => format!("{} {key}", lead.to_lowercase()),
        None => key,
    }
}

/// Words that say "this is a setup failure" and the filler around them. What
/// is left of the title names the connection, so "Enable Google Calendar API"
/// and "Google Calendar needs re-auth" count as one.
const SETUP_MARKERS: &[&str] = &["enable", "enabled", "enabling", "reauth", "auth", "authenticate", "authentication", "reauthenticate", "authorize", "authorise", "authorization", "login", "credentials", "token", "tokens", "expired", "reconnect", "sign"];
const SETUP_FILLER: &[&str] = &["re", "in", "out", "signed", "log", "needs", "need", "required", "missing", "api", "access", "grant", "connect", "renew", "fix", "to", "the", "for", "on", "your", "a", "an", "is", "are", "again", "not", "failed", "failing", "error", "account", "of", "and", "with", "setup", "set", "up", "expire", "expires", "has", "have", "been", "please", "mcp", "server", "connection"];

/// The connection a setup-failure title names, or `None` for any other item.
pub fn setup_key(title: &str) -> Option<String> {
    let normalized = crate::duplicates::normalized_title(title);
    let words: Vec<&str> = normalized.split_whitespace().collect();
    if words.is_empty() || words.len() > 8 || !words.iter().any(|w| SETUP_MARKERS.contains(w)) {
        return None;
    }
    let mut rest: Vec<&str> = words.into_iter().filter(|w| !SETUP_MARKERS.contains(w) && !SETUP_FILLER.contains(w)).collect();
    if rest.is_empty() {
        return None;
    }
    rest.sort_unstable();
    rest.dedup();
    Some(rest.join(" "))
}

/// Whether `new` repeats `old`. Titles match within a scope; a setup failure
/// naming the same connection matches across scopes too.
pub fn same_item(new_title: &str, old_title: &str, same_scope: bool) -> bool {
    if same_scope && title_key(new_title) == title_key(old_title) {
        return true;
    }
    matches!((setup_key(new_title), setup_key(old_title)), (Some(a), Some(b)) if a == b)
}

/// The last moment something happened to the item; `run_started_ms` stands
/// in for items stored before these fields existed.
pub fn last_activity_ms(item: &NeedsYou, run_started_ms: i64) -> i64 {
    [item.touched_ms, item.created_ms].into_iter().max().filter(|t| *t > 0).unwrap_or(run_started_ms)
}

pub fn age_days(item: &NeedsYou, run_started_ms: i64, now: i64) -> i64 {
    ((now - last_activity_ms(item, run_started_ms)) / DAY_MS).max(0)
}

pub fn close(item: &mut NeedsYou, reason: &str, now: i64) {
    item.done = true;
    item.closed_ms = Some(now);
    item.reason = Some(reason.to_string());
}

pub fn reopen(item: &mut NeedsYou, now: i64) {
    item.done = false;
    item.closed_ms = None;
    item.reason = None;
    item.touched_ms = now;
    item.kept = true;
}

/// A routine reported an open item again: newer words replace the old, and
/// its age starts over.
pub fn refresh(old: &mut NeedsYou, new: &NeedsYou, now: i64) {
    if !new.detail.trim().is_empty() {
        old.detail = new.detail.clone();
    }
    if new.link.is_some() {
        old.link = new.link.clone();
    }
    if new.default.is_some() {
        old.default = new.default.clone();
    }
    if new.expires_ms.is_some() {
        old.expires_ms = new.expires_ms;
        // A new time is a new deadline, even on an item the owner reopened.
        old.kept = false;
    }
    old.touched_ms = now;
}

/// The most an answer, a question or a default may say.
pub const MAX_ANSWER: usize = 1000;

/// An escalation is one question: everything after its first question mark
/// is dropped (a title with no question mark is left as it is). A second
/// question belongs in a second item, or nowhere.
pub fn one_question(title: &str) -> String {
    let title = title.trim();
    match title.find('?') {
        Some(at) => title[..=at].trim().to_owned(),
        None => title.to_owned(),
    }
}

/// A default the owner can accept: one line of plain words, bounded.
pub fn clean_default(text: &str) -> Option<String> {
    let line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let line: String = line.chars().filter(|c| !c.is_control()).take(300).collect();
    line.chars().any(char::is_alphanumeric).then_some(line)
}

/// What the owner wrote on lines that start with `>` in a reminder's notes
/// (the lines are joined with a space), or `None` when there is none.
pub fn reply_in_notes(notes: &str) -> Option<String> {
    let notes = unify_breaks(notes);
    let lines: Vec<&str> = notes.lines().filter_map(|l| l.trim_start().strip_prefix('>')).map(str::trim).filter(|l| !l.is_empty()).collect();
    (!lines.is_empty()).then(|| lines.join(" "))
}

/// Every kind of line break the Reminders text can carry (Swift's `isNewline`) as `\n`, so a
/// `>` after a carriage return or a Unicode separator is a line start here too.
fn unify_breaks(text: &str) -> String {
    text.replace("\r\n", "\n").replace(['\r', '\u{b}', '\u{c}', '\u{85}', '\u{2028}', '\u{2029}'], "\n")
}

/// Neutralises a `>` at the start of a line in text Neko writes into a
/// reminder, so only the owner's own lines count as replies.
pub fn without_reply_marks(text: &str) -> String {
    unify_breaks(text).lines().map(|l| if l.trim_start().starts_with('>') { l.replacen('>', "\u{203a}", 1) } else { l.to_owned() }).collect::<Vec<_>>().join("\n")
}

/// Whether `text` is an answer to `item` and what it is, cleaned. Not an
/// answer: nothing to read in it, or only the item's own words quoted back.
/// The result only ever goes where a typed answer would go; it is never run.
pub fn answer_text(text: &str, item: &NeedsYou) -> Option<String> {
    let cleaned: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    let cleaned: String = cleaned.chars().take(MAX_ANSWER).collect();
    if !cleaned.chars().any(char::is_alphanumeric) {
        return None;
    }
    let key = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase();
    let mine = key(&cleaned);
    let quoted = std::iter::once(item.title.as_str()).chain(item.detail.lines()).any(|l| !l.trim().is_empty() && key(l) == mine);
    (!quoted).then_some(cleaned)
}

/// Why an open item should close now for time alone: its `expires` passed
/// (unless the owner reopened it) or `age_out_days` went by without activity.
pub fn time_reason(item: &NeedsYou, run_started_ms: i64, now: i64, age_out_days: u32) -> Option<&'static str> {
    if item.done {
        return None;
    }
    if !item.kept && item.expires_ms.is_some_and(|t| t <= now) {
        return Some(EXPIRED);
    }
    if age_out_days > 0 && now - last_activity_ms(item, run_started_ms) >= i64::from(age_out_days) * DAY_MS {
        return Some(AGED_OUT);
    }
    None
}

/// Whether a closed item still shows under "Closed recently".
pub fn recently_closed(item: &NeedsYou, now: i64) -> bool {
    item.done && item.closed_ms.is_some_and(|t| now - t <= RECENT_CLOSED_DAYS * DAY_MS)
}

/// What a pull request's state closes an item as, once every pull request it
/// names is merged or closed. `state` answers for one reference with its
/// state and when it ended. A pull request that ended before `since_ms` (the
/// item's start) does not count: the item only mentions it, so it stays for
/// age-out to handle.
pub fn pr_reason(refs: &[PrRef], since_ms: i64, state: &dyn Fn(&PrRef) -> Option<(String, i64)>) -> Option<&'static str> {
    if refs.is_empty() {
        return None;
    }
    let mut all_merged = true;
    for r in refs {
        let (state, ended_ms) = state(r)?;
        if ended_ms < since_ms - PR_SLACK_MS {
            return None;
        }
        match state.as_str() {
            "MERGED" => {}
            "CLOSED" => all_merged = false,
            _ => return None,
        }
    }
    Some(if all_merged { PR_MERGED } else { PR_CLOSED })
}

/// Clock differences between GitHub and this Mac.
const PR_SLACK_MS: i64 = 5 * 60 * 1000;

#[cfg(test)]
mod tests {
    use super::*;

    fn item(title: &str, detail: &str, link: Option<&str>) -> NeedsYou {
        NeedsYou { seq: 1, title: title.into(), detail: detail.into(), link: link.map(Into::into), ..Default::default() }
    }

    #[test]
    fn times_parse_in_the_forms_a_routine_writes() {
        let t = parse_time_ms("2026-11-14T08:00:00Z").unwrap();
        assert_eq!(parse_time_ms("2026-11-14T08:00:00+00:00"), Some(t));
        assert_eq!(parse_time_ms("2026-11-14T08:00:00"), Some(t));
        assert_eq!(parse_time_ms("2026-11-14 08:00"), Some(t));
        assert_eq!(parse_time_ms("2026-11-14T10:00:00+02:00"), Some(t));
        assert_eq!(parse_time_ms("2026-11-14"), parse_time_ms("2026-11-14T23:59:59Z"));
        assert_eq!(parse_time_ms("tomorrow"), None);
    }

    #[test]
    fn pull_requests_are_found_in_links_and_details() {
        let by_link = item("Review", "", Some("https://github.com/acme/widgets/pull/12/files"));
        assert_eq!(pr_refs(&by_link), vec![PrRef { owner: "acme".into(), repo: "widgets".into(), number: 12 }]);
        let by_text = item("Fix", "CI fails on acme/widgets#7, see (https://github.com/acme/api/pull/9).", None);
        let keys: Vec<String> = pr_refs(&by_text).iter().map(PrRef::key).collect();
        assert_eq!(keys, vec!["acme/widgets#7", "acme/api#9"]);
        assert!(pr_refs(&item("Issue", "see acme/widgets/issues/3 and #4", Some("https://github.com/acme/widgets/issues/3"))).is_empty());
    }

    #[test]
    fn setup_failures_naming_one_connection_are_one_item() {
        assert!(same_item("Enable Google Calendar API", "Enable Google Calendar API", false));
        assert!(same_item("Re-auth Slack", "Slack token expired", false));
        assert!(same_item("Enable the Google Calendar API", "Google Calendar needs re-auth", false));
        assert!(!same_item("Enable Google Calendar API", "Enable Gmail API", false));
        assert!(!same_item("Reply to Sam", "Reply to Sam", false), "plain titles match within a scope only");
        assert!(same_item("Reply to Sam", "reply to sam!", true));
        assert_eq!(setup_key("Reply to Sam about the launch"), None);
        assert_ne!(title_key("ABC-1: Review the fix"), title_key("ABC-2: Review the fix"), "two tickets, one wording");
        assert_eq!(title_key("ABC-1: Review the fix"), title_key("ABC-1 - review the fix!"));
    }

    #[test]
    fn time_closes_expired_and_stale_items() {
        let now = 100 * DAY_MS;
        let mut a = item("a", "", None);
        a.created_ms = now - 8 * DAY_MS;
        a.touched_ms = a.created_ms;
        assert_eq!(time_reason(&a, 0, now, 7), Some(AGED_OUT));
        assert_eq!(time_reason(&a, 0, now, 0), None, "0 days turns age-out off");
        a.touched_ms = now - DAY_MS;
        assert_eq!(time_reason(&a, 0, now, 7), None);
        a.expires_ms = Some(now - 1);
        assert_eq!(time_reason(&a, 0, now, 7), Some(EXPIRED));
        a.kept = true;
        assert_eq!(time_reason(&a, 0, now, 7), None, "a reopened item is not expired again");
        // Items from before the fields existed are aged from their run.
        assert_eq!(time_reason(&item("old", "", None), now - 9 * DAY_MS, now, 7), Some(AGED_OUT));
    }

    #[test]
    fn pull_request_state_closes_only_when_all_are_done() {
        let r = |n| PrRef { owner: "o".into(), repo: "r".into(), number: n };
        let states = |n: &PrRef| Some((match n.number { 1 => "MERGED", 2 => "CLOSED", _ => "OPEN" }.to_string(), 1_000_000));
        assert_eq!(pr_reason(&[r(1)], 0, &states), Some(PR_MERGED));
        assert_eq!(pr_reason(&[r(1), r(2)], 0, &states), Some(PR_CLOSED));
        assert_eq!(pr_reason(&[r(1), r(3)], 0, &states), None);
        assert_eq!(pr_reason(&[r(1), r(4)], 0, &|n: &PrRef| (n.number == 1).then(|| ("MERGED".to_string(), 1_000_000))), None, "unknown stays open");
        assert_eq!(pr_reason(&[], 0, &states), None);
        assert_eq!(pr_reason(&[r(1)], 2_000_000, &states), None, "a pull request that ended before the item was raised is only mentioned");
        assert_eq!(pr_reason(&[r(1)], 1_000_000 + 60_000, &states), Some(PR_MERGED), "clock slack");
    }

    #[test]
    fn refresh_and_reopen_restart_age() {
        let mut old = item("t", "old words", Some("https://a"));
        old.created_ms = 1;
        old.touched_ms = 1;
        refresh(&mut old, &item("t", "newer words", None), 500);
        assert_eq!((old.detail.as_str(), old.link.as_deref(), old.touched_ms), ("newer words", Some("https://a"), 500));
        close(&mut old, BY_YOU, 600);
        assert!(old.done && old.reason.as_deref() == Some(BY_YOU));
        assert!(recently_closed(&old, 600 + DAY_MS) && !recently_closed(&old, 600 + 4 * DAY_MS));
        reopen(&mut old, 700);
        assert!(!old.done && old.kept && old.closed_ms.is_none() && old.touched_ms == 700);
    }

    #[test]
    fn an_escalation_is_one_question_with_a_default() {
        assert_eq!(one_question("Which database? And which region?"), "Which database?");
        assert_eq!(one_question("  Sign in to the tracker  "), "Sign in to the tracker");
        assert_eq!(clean_default("  Use the\n primary  "), Some("Use the primary".to_owned()));
        assert_eq!(clean_default("  ...  "), None, "nothing to read is no default");
        assert_eq!(clean_default(""), None);
    }

    #[test]
    fn only_lines_that_start_with_greater_than_are_a_reply() {
        assert_eq!(reply_in_notes("why\n\n> use staging\n>and retry\nneko:4"), Some("use staging and retry".to_owned()));
        assert_eq!(reply_in_notes("  >  indented works too"), Some("indented works too".to_owned()));
        assert_eq!(reply_in_notes("a > b is not a reply\nneko:4"), None);
        assert_eq!(reply_in_notes(">\n> \n"), None);
        // What Neko writes into a reminder never starts a reply line.
        assert_eq!(reply_in_notes(&without_reply_marks("> quoted by an agent\nplain\n   > indented quote")), None);
    }

    #[test]
    fn what_is_not_an_answer_is_dropped_and_answers_are_cleaned() {
        let item = NeedsYou { title: "Reply to Sam?".into(), detail: "Sam asked about Friday\nSecond line".into(), ..Default::default() };
        assert_eq!(answer_text("  Friday\nworks\u{7} ", &item), Some("Friday works".to_owned()));
        assert_eq!(answer_text("...", &item), None);
        assert_eq!(answer_text("", &item), None);
        assert_eq!(answer_text("reply to sam?", &item), None, "the question quoted back");
        assert_eq!(answer_text("Sam asked about  Friday", &item), None, "a line of the detail quoted back");
        assert_eq!(answer_text(&"x".repeat(5000), &item).map(|a| a.len()), Some(MAX_ANSWER));
    }

    #[test]
    fn refresh_keeps_the_default_the_item_had() {
        let mut old = item("t", "d", None);
        old.default = Some("Say yes".into());
        refresh(&mut old, &item("t", "newer", None), 5);
        assert_eq!(old.default.as_deref(), Some("Say yes"));
        let mut newer = item("t", "newer", None);
        newer.default = Some("Say no".into());
        refresh(&mut old, &newer, 6);
        assert_eq!(old.default.as_deref(), Some("Say no"));
    }
}
