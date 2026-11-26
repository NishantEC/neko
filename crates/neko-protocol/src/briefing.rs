//! The Briefing (docs/knowledge/briefing.md): what happened while the person was away, built by the daemon from its own records. No
//! model writes it and nothing is sent anywhere: it is a derived view, like "Needs you", so it is the same after a restart.
//!
//! A briefing covers a **window**: from `from_ms` (inclusive) to `to_ms` (exclusive). The start is when the person last marked the
//! briefing read, or, the first time, 24 hours before now. Two windows that meet (mark read, then the next) share their edge and so
//! never show an item twice or lose one.

use serde::{Deserialize, Serialize};

use crate::rounds::RunKind;
use crate::undo::Offer;
use crate::{Changes, NeedsItem, RunId, TaskId, TaskStatus};

/// How long the first window is, when the person has never marked a briefing read.
pub const FIRST_WINDOW_MS: i64 = 24 * 3_600_000;
/// The most finished tasks a briefing lists; the rest are counted (the totals count them all).
pub const MAX_FINISHED: usize = 50;
/// The most round lines a briefing lists; the rest are counted.
pub const MAX_ROUND_LINES: usize = 40;
/// The longest a task's first sentence is, in characters.
pub const SENTENCE_CHARS: usize = 160;
/// The morning time before the person chooses one.
pub const DEFAULT_MORNING_AT: &str = "08:00";

/// The briefing's settings: whether the morning notification is posted and when (local time of the Mac).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefingSettings {
    pub morning_on: bool,
    /// "HH:MM", 24-hour.
    pub morning_at: String,
}

impl Default for BriefingSettings {
    fn default() -> Self {
        BriefingSettings { morning_on: true, morning_at: DEFAULT_MORNING_AT.to_owned() }
    }
}

/// How a finished task ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FinishedEnd {
    Done,
    /// It could not finish (a failure that is not the person's to fix; the ones they can fix are in Needs you).
    Failed,
    Stopped,
    /// The daemon stopped while it ran.
    Interrupted,
}

/// A task that finished in the window.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BriefingTask {
    pub task: TaskId,
    pub workspace: String,
    pub title: String,
    pub cat: String,
    pub cat_name: String,
    /// When its last run in the window ended.
    pub ended_ms: i64,
    pub end: FinishedEnd,
    /// The first sentence of the cat's final answer, in plain text; None when the run said nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub answer: Option<String>,
    /// What the run changed (its own changes when its checkpoints say, else the branch's). None: nothing known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changes: Option<Changes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    /// The run that ended, so Review and Undo can name it.
    pub run: RunId,
    /// What the run's card offers for it (Review, Undo or Redo), worked out from all the task's runs. `offer.undone`: its changes
    /// were undone since.
    pub offer: Offer,
}

/// A task running now.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefingWorking {
    pub task: TaskId,
    pub workspace: String,
    pub title: String,
    pub cat: String,
    pub cat_name: String,
    pub since_ms: i64,
}

/// What a round's due time came to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundOutcome {
    Ran,
    Skipped,
    Missed,
    /// It failed three times in a row and turned itself off.
    Braked,
}

impl From<RunKind> for RoundOutcome {
    fn from(k: RunKind) -> Self {
        match k {
            RunKind::Started => RoundOutcome::Ran,
            RunKind::Skipped => RoundOutcome::Skipped,
            RunKind::Missed => RoundOutcome::Missed,
        }
    }
}

/// One line about a round that was due in the window.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefingRound {
    pub round: String,
    pub name: String,
    pub workspace: String,
    pub cat: String,
    pub cat_name: String,
    pub outcome: RoundOutcome,
    /// When the daemon dealt with it (for a brake, when the round turned itself off).
    pub at_ms: i64,
    /// The time it was due (None for a brake).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub due_ms: Option<i64>,
    /// The task a run made, its title and where it stands now.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task: Option<TaskId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_status: Option<TaskStatus>,
    /// Why it was skipped or missed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The window's totals.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct BriefingTotals {
    /// Tasks that finished (all of them, also those past the list's cap).
    pub finished: u32,
    /// Files changed by those runs, with lines added and removed. A run that was undone since is not counted.
    pub files: u32,
    pub added: u32,
    pub removed: u32,
    /// What the runs that ended in the window reported they cost, as Claude Code reported it; None when none reported a cost.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    /// Some of that cost is an API-equivalent price, not money charged (a subscription login).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub api_equivalent: bool,
}

/// What happened while the person was away.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Briefing {
    /// The window: `from_ms` inclusive, `to_ms` exclusive.
    pub from_ms: i64,
    pub to_ms: i64,
    /// The window starts 24 hours back because no briefing was ever marked read.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub first: bool,
    /// When the person last marked a briefing read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_ms: Option<i64>,
    /// The next morning notification, when the morning time is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_morning_ms: Option<i64>,
    /// Everything waiting for the person now, whenever it began to wait (the Needs you list, whole).
    pub needs: Vec<NeedsItem>,
    /// Tasks that finished in the window, newest first (at most [`MAX_FINISHED`]).
    pub finished: Vec<BriefingTask>,
    /// Rounds that were due in the window, newest first (at most [`MAX_ROUND_LINES`]).
    pub rounds: Vec<BriefingRound>,
    /// How many round lines the window has in all: those that ran, and those that did not (skipped, missed, braked).
    pub rounds_ran: u32,
    pub rounds_not_run: u32,
    /// Tasks running now.
    pub working: Vec<BriefingWorking>,
    pub totals: BriefingTotals,
}

/// The numbers a headline is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct BriefingCounts {
    /// Everything waiting for the person.
    pub needs_you: u32,
    pub finished: u32,
    pub working: u32,
    /// Round lines that ran, and those that did not (skipped, missed or braked).
    pub rounds_ran: u32,
    pub rounds_not_run: u32,
    /// There is something new to say: a task finished, a round was due, or something began to wait inside the window. A request
    /// that has been waiting since before the window is not news (it is in Needs you), so after "Mark as read" the Home line and the
    /// morning notification are quiet.
    pub news: bool,
}

impl BriefingCounts {
    /// "3 finished, 1 needs you". When neither is there: what the rounds did, then what is working.
    pub fn headline(&self) -> String {
        let mut parts = Vec::new();
        if self.finished > 0 {
            parts.push(format!("{} finished", self.finished));
        }
        if self.needs_you > 0 {
            parts.push(format!("{} needs you", self.needs_you));
        }
        if parts.is_empty() {
            let plural = |n: u32| if n == 1 { "round" } else { "rounds" };
            if self.rounds_ran > 0 {
                parts.push(format!("{} {} ran", self.rounds_ran, plural(self.rounds_ran)));
            } else if self.rounds_not_run > 0 {
                parts.push(format!("{} {} did not run", self.rounds_not_run, plural(self.rounds_not_run)));
            }
        }
        if parts.is_empty() && self.working > 0 {
            parts.push(format!("{} still working", self.working));
        }
        parts.join(", ")
    }
}

impl Briefing {
    pub fn counts(&self) -> BriefingCounts {
        let new_needs = self.needs.iter().any(|n| n.since_ms >= self.from_ms);
        BriefingCounts {
            needs_you: self.needs.len() as u32,
            finished: self.totals.finished,
            working: self.working.len() as u32,
            rounds_ran: self.rounds_ran,
            rounds_not_run: self.rounds_not_run,
            news: self.totals.finished > 0 || self.rounds_ran + self.rounds_not_run > 0 || new_needs,
        }
    }

    /// Nothing new since the window began (a request that has long waited does not count).
    pub fn is_empty(&self) -> bool {
        !self.counts().news
    }

    /// Finished tasks past the list's cap.
    pub fn finished_more(&self) -> u32 {
        self.totals.finished.saturating_sub(self.finished.len() as u32)
    }

    /// Round lines past the list's cap.
    pub fn rounds_more(&self) -> u32 {
        (self.rounds_ran + self.rounds_not_run).saturating_sub(self.rounds.len() as u32)
    }

    /// Whether the content, apart from the moment it was made, is the same as another's.
    pub fn same_content(&self, other: &Briefing) -> bool {
        let mut a = self.clone();
        a.to_ms = other.to_ms;
        a == *other
    }
}

/// The first sentence of an answer, as plain text. Markdown's marks are dropped (emphasis, code spans and links keep their words),
/// a heading is passed over unless it is all there is, a list item or a quote loses its marker (a list item is a unit of its own),
/// a code block is skipped (prose before it counts; a block alone gives "Code"), the sentence ends at its first full stop,
/// question or exclamation mark followed by a space or the end, and one longer than [`SENTENCE_CHARS`] is cut at a word with an
/// ellipsis. Empty when the answer says nothing.
pub fn first_sentence(answer: &str) -> String {
    let mut in_code = false;
    let mut saw_code = false;
    let mut text = String::new();
    let mut heading: Option<String> = None;
    for raw in answer.lines() {
        let line = raw.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            in_code = !in_code;
            saw_code = true;
            continue;
        }
        if in_code {
            continue;
        }
        if line.is_empty() {
            if !text.is_empty() {
                break;
            }
            continue;
        }
        // A table row or a rule is not a sentence.
        if line.starts_with('|') || line.chars().all(|c| matches!(c, '-' | '=' | '*' | '_' | ' ')) {
            continue;
        }
        if line.starts_with('#') {
            if heading.is_none() {
                heading = Some(plain(line.trim_start_matches('#').trim()));
            }
            continue;
        }
        let (body, item) = strip_marks(line);
        if body.is_empty() {
            continue;
        }
        if item && !text.is_empty() {
            break;
        }
        if !text.is_empty() {
            text.push(' ');
        }
        text.push_str(&plain(&body));
        if item {
            break;
        }
    }
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.is_empty() {
        let heading = heading.unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
        return if !heading.is_empty() {
            cut(&sentence_end(&heading))
        } else if saw_code {
            "Code".to_owned()
        } else {
            String::new()
        };
    }
    cut(&sentence_end(&text))
}

/// A quote's or list item's marker taken off, and whether it was a list item.
fn strip_marks(line: &str) -> (String, bool) {
    let mut s = line.trim_start_matches('>').trim_start();
    let mut item = false;
    for marker in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if let Some(rest) = s.strip_prefix(marker) {
            s = rest.trim_start();
            item = true;
            break;
        }
    }
    let digits = s.chars().take_while(char::is_ascii_digit).count();
    if !item && (1..=3).contains(&digits) && let Some(rest) = s[digits..].strip_prefix(". ").or_else(|| s[digits..].strip_prefix(") ")) {
        s = rest.trim_start();
        item = true;
    }
    (s.to_owned(), item)
}

/// Emphasis, code spans, links and images reduced to their words.
fn plain(line: &str) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\\' if i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() => {
                out.push(chars[i + 1]);
                i += 2;
            }
            '*' | '`' => i += 1,
            // An underscore inside a word (snake_case) stays; one that frames words is emphasis.
            '_' => {
                if i > 0 && i + 1 < chars.len() && chars[i - 1].is_alphanumeric() && chars[i + 1].is_alphanumeric() {
                    out.push('_');
                }
                i += 1;
            }
            '~' if chars.get(i + 1) == Some(&'~') => i += 2,
            '!' if chars.get(i + 1) == Some(&'[') => i += 1,
            '[' => {
                // [words](target): keep the words.
                if let Some(close) = chars[i..].iter().position(|c| *c == ']')
                    && chars.get(i + close + 1) == Some(&'(')
                    && let Some(end) = chars[i + close..].iter().position(|c| *c == ')')
                {
                    out.extend(&chars[i + 1..i + close]);
                    i += close + end + 1;
                } else {
                    out.push('[');
                    i += 1;
                }
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The text up to and including its first sentence end.
fn sentence_end(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let closer = |c: Option<&char>| matches!(c, Some('"' | '\'' | ')' | '”' | '’'));
    for (i, c) in chars.iter().enumerate() {
        if !matches!(c, '.' | '?' | '!' | '…') {
            continue;
        }
        let next = chars.get(i + 1);
        // "v1.2" and "e.g." do not end a sentence: a full stop ends one only before a space (not after an abbreviation) or the
        // end; a question or exclamation mark ends one before a space or the end. A closing quote or bracket belongs to it.
        let at = if closer(next) { i + 2 } else { i + 1 };
        let ends = match chars.get(at) {
            None => true,
            Some(' ') => *c != '.' || !is_abbreviation(&chars[..=i]),
            _ => false,
        };
        if ends {
            return chars[..at].iter().collect();
        }
    }
    text.to_owned()
}

fn is_abbreviation(upto: &[char]) -> bool {
    let word: String = upto.iter().rev().take_while(|c| !c.is_whitespace()).collect::<Vec<_>>().into_iter().rev().collect();
    matches!(word.to_ascii_lowercase().as_str(), "e.g." | "i.e." | "etc." | "vs." | "approx." | "no.")
}

/// At most [`SENTENCE_CHARS`] characters, cut at a word with an ellipsis.
fn cut(s: &str) -> String {
    if s.chars().count() <= SENTENCE_CHARS {
        return s.to_owned();
    }
    let head: String = s.chars().take(SENTENCE_CHARS - 1).collect();
    let head = match head.rfind(' ') {
        Some(i) if i > SENTENCE_CHARS / 2 => head[..i].to_owned(),
        _ => head,
    };
    format!("{}…", head.trim_end_matches([' ', ',', ';', ':', '.']))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_sentence_of_plain_words() {
        assert_eq!(first_sentence("I renamed the label. Then I ran the tests."), "I renamed the label.");
        assert_eq!(first_sentence("Done! The build is green."), "Done!");
        assert_eq!(first_sentence("Which branch should it go on? I can do either."), "Which branch should it go on?");
        assert_eq!(first_sentence("Updated to v1.2 of the parser. Tests pass."), "Updated to v1.2 of the parser.");
        assert_eq!(first_sentence("Used e.g. the old helper. Done."), "Used e.g. the old helper.");
        assert_eq!(first_sentence("She said \"stop.\" Then left."), "She said \"stop.\"");
        assert_eq!(first_sentence("Two lines\nof one paragraph. And more."), "Two lines of one paragraph.");
    }

    #[test]
    fn markdown_is_plain_words() {
        assert_eq!(first_sentence("**Fixed** the `render_page` call in [the docs](https://example.com/a). More."), "Fixed the render_page call in the docs.");
        assert_eq!(first_sentence("## Summary\n\nThe job is finished."), "The job is finished.");
        assert_eq!(first_sentence("## Summary"), "Summary");
        assert_eq!(first_sentence("- Renamed three labels\n- Fixed a test"), "Renamed three labels");
        assert_eq!(first_sentence("Here is what changed\n- one\n- two"), "Here is what changed");
        assert_eq!(first_sentence("1. Install the package. 2. Run it."), "Install the package.");
        assert_eq!(first_sentence("> Quoted advice here. And more."), "Quoted advice here.");
        assert_eq!(first_sentence("Wrote ![diagram](a.png) for you."), "Wrote diagram for you.");
        assert_eq!(first_sentence("Moved it to ~/notes.md. Fine."), "Moved it to ~/notes.md.");
        assert_eq!(first_sentence("_Quietly_ done, ~~not~~ really."), "Quietly done, not really.");
    }

    #[test]
    fn a_code_block_is_skipped_and_prose_before_it_counts() {
        assert_eq!(first_sentence("Here is the fix:\n\n```rust\nfn main() {}\n```\n\nIt works."), "Here is the fix:");
        assert_eq!(first_sentence("```sh\ncargo test\n```\nAll tests pass."), "All tests pass.");
        assert_eq!(first_sentence("```sh\ncargo test\n```"), "Code");
    }

    #[test]
    fn very_long_and_empty_answers() {
        let long = format!("{} end.", "word ".repeat(80));
        let s = first_sentence(&long);
        assert!(s.chars().count() <= SENTENCE_CHARS && s.ends_with('…'), "{s}");
        assert!(!s.contains("  "));
        assert_eq!(first_sentence(""), "");
        assert_eq!(first_sentence("   \n\n  "), "");
        assert_eq!(first_sentence("No full stop at all"), "No full stop at all");
        let unbroken = "x".repeat(400);
        assert_eq!(first_sentence(&unbroken).chars().count(), SENTENCE_CHARS);
    }

    #[test]
    fn the_headline_names_what_matters() {
        let c = BriefingCounts { needs_you: 1, finished: 3, working: 2, rounds_ran: 1, rounds_not_run: 0, news: true };
        assert_eq!(c.headline(), "3 finished, 1 needs you");
        assert_eq!(BriefingCounts { finished: 1, news: true, ..BriefingCounts::default() }.headline(), "1 finished");
        assert_eq!(BriefingCounts { needs_you: 2, news: true, ..BriefingCounts::default() }.headline(), "2 needs you");
        assert_eq!(BriefingCounts { rounds_ran: 2, news: true, ..BriefingCounts::default() }.headline(), "2 rounds ran");
        assert_eq!(BriefingCounts { rounds_not_run: 1, news: true, ..BriefingCounts::default() }.headline(), "1 round did not run");
        assert_eq!(BriefingCounts { working: 1, ..BriefingCounts::default() }.headline(), "1 still working");
        assert_eq!(BriefingCounts::default().headline(), "");
    }
}
