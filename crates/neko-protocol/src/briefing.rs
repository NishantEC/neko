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
/// The most lines a Cattain's block quotes from each of its digest's "Needs you" and "New".
pub const DIGEST_LINES: usize = 3;
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
    /// Its cat and the cat's name; both empty for a thread that has none (version 14).
    #[serde(default)]
    pub cat: String,
    #[serde(default)]
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
    #[serde(default)]
    pub cat: String,
    #[serde(default)]
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

/// A Cattain whose DIGEST.md changed in the window (version 24): what its digest says under "Needs you" and "New", at most
/// [`DIGEST_LINES`] lines of each, in its own words ([`digest_lines`]).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefingCattain {
    pub cat: String,
    pub name: String,
    /// When its DIGEST.md last changed, as Neko saw it.
    pub changed_ms: i64,
    /// Its lines under "Needs you".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<String>,
    /// How many more lines it has there.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub needs_more: u32,
    /// Its lines under "New".
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new: Vec<String>,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub new_more: u32,
}

/// A Cattain an agent made by asking in the window (version 24), still at work: "New Cattain: yuzu (iOS work)", once.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BriefingNewCattain {
    pub cat: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purpose: Option<String>,
    pub made_ms: i64,
    /// The thread whose agent made it.
    pub made_in: TaskId,
}

fn is_zero(n: &u32) -> bool {
    *n == 0
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
    /// The Cattains whose DIGEST.md changed in the window, the most recent first (version 24).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cattains: Vec<BriefingCattain>,
    /// The Cattains agents made by asking in the window and still at work, the newest first (version 24).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub new_cattains: Vec<BriefingNewCattain>,
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
    /// There is something new to say: a task finished, a round was due, something began to wait inside the window, a Cattain's
    /// digest changed or a Cattain was made by asking. A request that has been waiting since before the window is not news (it is in
    /// Needs you), so after "Mark as read" New thread's line and the morning notification are quiet.
    pub news: bool,
    /// Cattains whose digest changed in the window (version 24).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub digests: u32,
    /// Cattains made by asking in the window (version 24).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub new_cattains: u32,
}

impl BriefingCounts {
    /// "3 finished, 1 needs you". When neither is there: the digests and new Cattains, then what the rounds did, then what is
    /// working.
    pub fn headline(&self) -> String {
        let mut parts = Vec::new();
        if self.finished > 0 {
            parts.push(format!("{} finished", self.finished));
        }
        if self.needs_you > 0 {
            parts.push(format!("{} needs you", self.needs_you));
        }
        if parts.is_empty() {
            if self.digests > 0 {
                parts.push(format!("{} {} updated", self.digests, if self.digests == 1 { "digest" } else { "digests" }));
            }
            if self.new_cattains > 0 {
                parts.push(format!("{} new {}", self.new_cattains, if self.new_cattains == 1 { "Cattain" } else { "Cattains" }));
            }
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
            news: self.totals.finished > 0 || self.rounds_ran + self.rounds_not_run > 0 || new_needs || !self.cattains.is_empty() || !self.new_cattains.is_empty(),
            digests: self.cattains.len() as u32,
            new_cattains: self.new_cattains.len() as u32,
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

/// Which of a digest's sections a line belongs to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DigestPart {
    NeedsYou,
    New,
    Other,
}

/// What a digest says under its "Needs you" and "New" headings (cats.md: a Cattain keeps DIGEST.md under Needs you, New and In
/// progress), each line in the Cattain's own words: a list item is a line (its marker off, a checkbox too; an item nested under
/// another is its detail and is passed over), the lines of a paragraph are one line, Markdown's marks are dropped as in
/// [`first_sentence`], a code block is skipped, and a line that only says there is nothing ("None", "Nothing new.") is not a line.
/// A heading is a `#` line, or a line that is only one of the three names (in bold, or with a colon). Each line is at most
/// [`SENTENCE_CHARS`] characters, cut at a word. A digest with neither heading gives nothing.
pub fn digest_lines(digest: &str) -> (Vec<String>, Vec<String>) {
    let (mut needs, mut new) = (Vec::new(), Vec::new());
    let mut part = DigestPart::Other;
    let mut in_code = false;
    // The line being gathered, and whether more of it may follow (a paragraph's next line, an item's lazy continuation).
    let mut open: Option<String> = None;
    let flush = |open: &mut Option<String>, part: DigestPart, needs: &mut Vec<String>, new: &mut Vec<String>| {
        let Some(line) = open.take() else { return };
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() || says_nothing(&line) {
            return;
        }
        match part {
            DigestPart::NeedsYou => needs.push(cut(&line)),
            DigestPart::New => new.push(cut(&line)),
            DigestPart::Other => {}
        }
    };
    for raw in digest.lines() {
        let line = raw.trim();
        if line.starts_with("```") || line.starts_with("~~~") {
            flush(&mut open, part, &mut needs, &mut new);
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        if line.is_empty() || line.starts_with('|') || line.chars().all(|c| matches!(c, '-' | '=' | '*' | '_' | ' ')) {
            flush(&mut open, part, &mut needs, &mut new);
            continue;
        }
        if let Some(p) = heading_of(line) {
            flush(&mut open, part, &mut needs, &mut new);
            part = p;
            continue;
        }
        let (body, item) = strip_marks(line);
        let indent = raw.len() - raw.trim_start().len();
        if item && indent >= 2 {
            // A nested item is the detail of the one above it.
            flush(&mut open, part, &mut needs, &mut new);
            continue;
        }
        if item {
            flush(&mut open, part, &mut needs, &mut new);
            open = Some(plain(&body));
        } else if let Some(o) = &mut open {
            o.push(' ');
            o.push_str(&plain(&body));
        } else {
            open = Some(plain(&body));
        }
    }
    flush(&mut open, part, &mut needs, &mut new);
    (needs, new)
}

/// The section a heading line starts, or None when the line is not a heading.
fn heading_of(line: &str) -> Option<DigestPart> {
    let hashed = line.starts_with('#');
    let bare = line.trim_start_matches('#').trim();
    let framed = bare.len() > 4 && (bare.starts_with("**") && bare.ends_with("**") || bare.starts_with("__") && bare.ends_with("__"));
    let words = plain(bare).trim().trim_end_matches(':').trim().to_lowercase();
    let colon = bare.ends_with(':') || bare.ends_with(":**");
    // "Needs you (2)" and "New · 3" keep their name.
    let name: String = words.split(['(', '·', '-', '—']).next().unwrap_or("").trim().to_owned();
    let part = match name.as_str() {
        "needs you" | "needs your attention" | "needs your decision" => Some(DigestPart::NeedsYou),
        "new" => Some(DigestPart::New),
        "in progress" => Some(DigestPart::Other),
        _ => None,
    };
    match (hashed, part) {
        (true, Some(p)) => Some(p),
        (true, None) => Some(DigestPart::Other),
        (false, Some(p)) if framed || colon => Some(p),
        _ => None,
    }
}

/// A line that only says there is nothing to say.
fn says_nothing(line: &str) -> bool {
    let l = line.trim().trim_end_matches(['.', '!']).trim().to_lowercase();
    matches!(l.as_str(), "none" | "none yet" | "(none)" | "nothing" | "nothing yet" | "nothing new" | "nothing right now" | "nothing needs you" | "n/a" | "—" | "-")
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
    fn a_digest_gives_its_needs_you_and_new_lines_in_its_own_words() {
        let digest = "# miso's digest\n\n## Needs you\n- Merge the three branches? `neko/4f2a-1`, `neko/4f2a-2` and `neko/4f2a-3` are ready.\n- **Decide** which [retry limit](https://example.com/r) to keep.\n\n## New\n- Fix test one: done.\n- Fix test two: done,\n  with a note on the flaky clock.\n  - a detail under it\n- Fix test three: done.\n- Fix test four: done.\n\n## In progress\n- Nothing else.\n";
        let (needs, new) = digest_lines(digest);
        assert_eq!(needs, vec!["Merge the three branches? neko/4f2a-1, neko/4f2a-2 and neko/4f2a-3 are ready.", "Decide which retry limit to keep."]);
        assert_eq!(new, vec!["Fix test one: done.", "Fix test two: done, with a note on the flaky clock.", "Fix test three: done.", "Fix test four: done."], "every line: the caller keeps three");
    }

    #[test]
    fn a_digest_heading_may_be_bold_or_carry_a_colon_or_a_count() {
        let (needs, new) = digest_lines("**Needs you**\nNothing right now.\n\nNew (2):\n1. Fixed the login test.\n2. Fixed the docs link.\n\nIn progress:\n- Port the backup script");
        assert!(needs.is_empty(), "a line that says nothing is not a line: {needs:?}");
        assert_eq!(new, vec!["Fixed the login test.", "Fixed the docs link."]);
        let (needs, new) = digest_lines("### NEEDS YOU:\n- [ ] Approve the release notes\n### new · 1\nA paragraph\nover two lines.\n```sh\ncargo test\n```\n");
        assert_eq!((needs, new), (vec!["Approve the release notes".to_owned()], vec!["A paragraph over two lines.".to_owned()]));
    }

    #[test]
    fn a_digest_without_the_headings_gives_nothing() {
        assert_eq!(digest_lines(""), (vec![], vec![]));
        assert_eq!(digest_lines("Merged the branch. Next: the docs.\n- New things are coming"), (vec![], vec![]));
        let long = format!("## New\n- {}", "word ".repeat(80));
        let (_, new) = digest_lines(&long);
        assert!(new[0].chars().count() <= SENTENCE_CHARS && new[0].ends_with('…'), "{new:?}");
    }

    #[test]
    fn the_headline_names_what_matters() {
        let c = BriefingCounts { needs_you: 1, finished: 3, working: 2, rounds_ran: 1, rounds_not_run: 0, news: true, digests: 1, new_cattains: 0 };
        assert_eq!(c.headline(), "3 finished, 1 needs you");
        assert_eq!(BriefingCounts { digests: 2, new_cattains: 1, rounds_ran: 1, news: true, ..BriefingCounts::default() }.headline(), "2 digests updated, 1 new Cattain");
        assert_eq!(BriefingCounts { digests: 1, news: true, ..BriefingCounts::default() }.headline(), "1 digest updated");
        assert_eq!(BriefingCounts { finished: 1, news: true, ..BriefingCounts::default() }.headline(), "1 finished");
        assert_eq!(BriefingCounts { needs_you: 2, news: true, ..BriefingCounts::default() }.headline(), "2 needs you");
        assert_eq!(BriefingCounts { rounds_ran: 2, news: true, ..BriefingCounts::default() }.headline(), "2 rounds ran");
        assert_eq!(BriefingCounts { rounds_not_run: 1, news: true, ..BriefingCounts::default() }.headline(), "1 round did not run");
        assert_eq!(BriefingCounts { working: 1, ..BriefingCounts::default() }.headline(), "1 still working");
        assert_eq!(BriefingCounts::default().headline(), "");
    }
}
