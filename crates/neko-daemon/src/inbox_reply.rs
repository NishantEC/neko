//! Answering a needs-you item: the one path both the Inbox button and the
//! phone use. An answer is either the item's default or the owner's words, and
//! it goes only where a typed answer would go:
//!
//! - a blocked goal's question: `GoalCommand::Answer` (the words become the
//!   goal's next direction; empty takes the default);
//! - "Extend by N turns?": `GoalCommand::Extend` (words must be a number of turns);
//! - "Raise today's ceiling to $X?": `SetDailyBudget` (words must be an amount);
//! - an item that names a task (`task:<id>`): the durable command queue,
//!   delivered after the current turn;
//! - any other routine item: kept in a small row of replies that the routine's
//!   next prompt shows as `memory.owner_replies`, and the item closes.
//!
//! Text is never run as a command. See docs/knowledge/phone-push.md.
use crate::routines::all_items_with_routine;
use crate::server::AppState;
use neko_core::inbox;
use neko_protocol::goals::{Goal, GoalCommand};
use neko_protocol::routines::NeedsYou;
use neko_protocol::workbench::Command;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer {
    /// Whatever the item offers if the owner only accepts.
    Default,
    /// The owner's own words (already cleaned by `inbox::answer_text`).
    Text(String),
}

/// Where an item's answer goes.
#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    GoalBlocked { goal_id: String, default: String },
    GoalExtend { goal_id: String, turns: u32, subject: String },
    Spend { to_cents: u32 },
    Task { task_id: String },
    Plain { routine_id: String },
}

/// What happened, in a line for the echo and the Inbox.
pub struct Done {
    pub summary: String,
}

const REPLIES_ROW: &str = "neko_owner_replies_v1:";
const KEEP_REPLIES: usize = 10;
const REPLY_MAX_AGE_MS: i64 = 7 * 86_400_000;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Reply {
    pub title: String,
    pub answer: String,
    pub at_ms: i64,
}

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, neko_core::Db> {
    state.db.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// What the owner answered to a routine's items lately (newest last), for the prompt.
pub fn replies(db: &neko_core::Db, routine_id: &str, now_ms: i64) -> Vec<Reply> {
    let all: Vec<Reply> = db.get_setting(&format!("{REPLIES_ROW}{routine_id}")).ok().flatten().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    all.into_iter().filter(|r| now_ms - r.at_ms < REPLY_MAX_AGE_MS).collect()
}

fn remember(db: &neko_core::Db, routine_id: &str, reply: Reply) -> Result<(), String> {
    let mut all = replies(db, routine_id, reply.at_ms);
    all.push(reply);
    let keep = all.len().saturating_sub(KEEP_REPLIES);
    all.drain(..keep);
    db.set_setting(&format!("{REPLIES_ROW}{routine_id}"), &serde_json::to_string(&all).map_err(|e| e.to_string())?).map_err(|e| e.to_string())
}

/// The open item with this `seq` and the routine that holds it.
pub fn find_open(db: &neko_core::Db, seq: u64) -> Result<(String, NeedsYou), String> {
    all_items_with_routine(db)?.into_iter().find(|(_, i)| i.seq == seq && !i.done).map(|(r, i)| (r, i)).ok_or_else(|| "That item is closed or gone".to_owned())
}

/// Where this item's answer goes, from its link and the state it points at.
pub fn target_of(db: &neko_core::Db, routine_id: &str, item: &NeedsYou) -> Target {
    let link = item.link.as_deref().unwrap_or("");
    if let Some(id) = link.strip_prefix("goal:") {
        if let Ok(Some(goal)) = neko_core::goals::load(db, id) {
            if let Some(blocked) = goal.blocked.as_ref().filter(|b| b.inbox_seq == Some(item.seq)) {
                return Target::GoalBlocked { goal_id: goal.id.clone(), default: blocked.default_answer.clone() };
            }
            if goal.extend_seq == Some(item.seq) {
                return Target::GoalExtend { goal_id: goal.id.clone(), turns: crate::workbench::goals::extend_by(&goal), subject: goal_name(&goal) };
            }
        }
    }
    if item.link.is_none() {
        if let Some(to_cents) = spend_raise(db, item.seq) {
            return Target::Spend { to_cents };
        }
    }
    if let Some(id) = link.strip_prefix("task:").filter(|id| !id.is_empty() && id.len() < 80 && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')) {
        return Target::Task { task_id: id.to_owned() };
    }
    Target::Plain { routine_id: routine_id.to_owned() }
}

fn goal_name(goal: &Goal) -> String {
    goal.ticket_id.clone().map_or_else(|| neko_core::goals::head(&goal.objective, 40), |t| t.chars().take(24).collect())
}

fn spend_raise(db: &neko_core::Db, seq: u64) -> Option<u32> {
    if neko_core::spend::notice(db).seq != Some(seq) {
        return None;
    }
    let snapshot = neko_core::workbench::load(db).ok()?;
    snapshot.daily_budget_cents.map(neko_core::spend::raise_to_cents)
}

/// What completing the item's reminder does, in a few words ("extend NK-14
/// by 10 turns"), or `None` when the item has no default.
pub fn action_label(db: &neko_core::Db, routine_id: &str, item: &NeedsYou) -> Option<String> {
    let default = item.default.as_deref().map(str::trim).filter(|d| !d.is_empty());
    match target_of(db, routine_id, item) {
        Target::GoalBlocked { default, .. } => Some(format!("answer \"{}\"", neko_core::goals::head(&default, 60))),
        Target::GoalExtend { turns, subject, .. } => Some(format!("extend {subject} by {turns} turns")),
        Target::Spend { to_cents } => Some(format!("raise today's ceiling to ${}", to_cents / 100)),
        Target::Task { .. } => default.map(|d| format!("tell the task \"{}\"", neko_core::goals::head(d, 60))),
        Target::Plain { .. } => default.map(|d| format!("reply \"{}\"", neko_core::goals::head(d, 60))),
    }
}

/// Applies an answer to the open item `seq`, closing it as answered. `phone`
/// says where it came from (the reason shown in Closed recently).
pub fn apply(state: &AppState, seq: u64, answer: Answer, phone: bool) -> Result<Done, String> {
    let (routine_id, item, target) = {
        let db = lock(state);
        let (routine_id, item) = find_open(&db, seq)?;
        let target = target_of(&db, &routine_id, &item);
        (routine_id, item, target)
    };
    let reason = if phone { inbox::ANSWERED_ON_PHONE } else { inbox::ANSWERED };
    let now = neko_core::workbench::now_ms();
    let summary = match (&target, &answer) {
        (Target::GoalBlocked { goal_id, .. }, _) => {
            // An empty answer takes the goal's default; the goal closes its own item.
            let text = if let Answer::Text(t) = &answer { t.clone() } else { String::new() };
            state.workbench.command(Command::Goals(GoalCommand::Answer { id: goal_id.clone(), text }))?;
            "answered the goal's question".to_owned()
        }
        (Target::GoalExtend { goal_id, turns, .. }, Answer::Default) => {
            state.workbench.command(Command::Goals(GoalCommand::Extend { id: goal_id.clone(), turns: *turns }))?;
            format!("extended by {turns} turns")
        }
        (Target::GoalExtend { goal_id, .. }, Answer::Text(text)) => {
            let turns = number_in(text).filter(|n| (1..=100).contains(n)).ok_or("To extend a goal, answer with a number of turns (1 to 100)")?;
            state.workbench.command(Command::Goals(GoalCommand::Extend { id: goal_id.clone(), turns }))?;
            format!("extended by {turns} turns")
        }
        (Target::Spend { to_cents }, Answer::Default) => raise_ceiling(state, seq, *to_cents)?,
        (Target::Spend { .. }, Answer::Text(text)) => {
            let cents = dollars_in(text).filter(|c| (1..=1_000_000).contains(c)).ok_or("To raise the ceiling, answer with an amount like $20")?;
            raise_ceiling(state, seq, cents)?
        }
        (Target::Task { task_id }, _) => {
            let text = match &answer {
                Answer::Text(t) => t.clone(),
                Answer::Default => item.default.clone().ok_or("No default to send")?,
            };
            let command_id = format!("answer-{seq}");
            state.workbench.command(Command::Crew(neko_protocol::crew::CrewCommand::Enqueue {
                task_id: task_id.clone(),
                command_id: Some(command_id),
                text: format!("Answer to \"{}\": {text}", item.title),
                mode: neko_protocol::crew::DeliveryMode::AfterTurn,
                kind: neko_protocol::crew::QueueKind::Answer,
            }))?;
            close(state, seq, reason, now)?;
            "sent to the task after its current turn".to_owned()
        }
        (Target::Plain { routine_id }, _) => {
            let text = match &answer {
                Answer::Text(t) => t.clone(),
                Answer::Default => item.default.clone().ok_or("This item has no default")?,
            };
            let db = lock(state);
            remember(&db, routine_id, Reply { title: item.title.clone(), answer: text, at_ms: now })?;
            drop(db);
            close(state, seq, reason, now)?;
            "recorded for the routine's next run".to_owned()
        }
    };
    let _ = routine_id;
    Ok(Done { summary })
}

fn close(state: &AppState, seq: u64, reason: &str, _now: i64) -> Result<(), String> {
    crate::routines::close_seqs(&lock(state), &[seq], reason)
}

fn raise_ceiling(state: &AppState, seq: u64, cents: u32) -> Result<String, String> {
    state.workbench.command(Command::SetDailyBudget { cents: Some(cents) })?;
    // The held state clears on the next gate look; close the item now.
    close(state, seq, inbox::ANSWERED, 0)?;
    Ok(format!("raised today's ceiling to ${}", cents / 100))
}

/// The first whole number in the words ("20", "add 20 turns").
fn number_in(text: &str) -> Option<u32> {
    text.split(|c: char| !c.is_ascii_digit()).find(|p| !p.is_empty() && p.len() < 6)?.parse().ok()
}

/// An amount in dollars as cents ("$20", "20 dollars", "12.50").
fn dollars_in(text: &str) -> Option<u32> {
    let token = text.split_whitespace().map(|w| w.trim_matches(|c: char| !c.is_ascii_digit() && c != '.')).find(|w| w.chars().any(|c| c.is_ascii_digit()))?;
    let value: f64 = token.parse().ok()?;
    (value.is_finite() && value > 0.0 && value <= 10_000.0).then(|| (value * 100.0).round() as u32)
}

/// The web's `AnswerNeedsYou`.
pub fn handle(state: &AppState, seq: u64, text: Option<String>) -> neko_protocol::Response {
    let answer = match text.as_deref().map(str::trim).filter(|t| !t.is_empty()) {
        None => Answer::Default,
        Some(words) => {
            let item = lock(state);
            match find_open(&item, seq).map(|(_, i)| inbox::answer_text(words, &i)) {
                Ok(Some(clean)) => Answer::Text(clean),
                Ok(None) => return neko_protocol::Response::Error { message: "That is not an answer".into() },
                Err(message) => return neko_protocol::Response::Error { message },
            }
        }
    };
    match apply(state, seq, answer, false) {
        Ok(done) => neko_protocol::Response::NeedsYouAnswered { summary: done.summary },
        Err(message) => neko_protocol::Response::Error { message },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_and_amounts_are_read_from_words() {
        assert_eq!(number_in("add 20 turns"), Some(20));
        assert_eq!(number_in("no"), None);
        assert_eq!(dollars_in("$20"), Some(2000));
        assert_eq!(dollars_in("make it 12.50 please"), Some(1250));
        assert_eq!(dollars_in("lots"), None);
    }
}
