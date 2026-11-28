//! Search (docs/knowledge/search.md): threads and kittens found by their titles and by what was said in them.
//!
//! The words are in two FTS5 tables the store keeps (`find_titles`, `find_words`: migration 14), each row numbered with the rowid of the
//! row it indexes. The person's text is parsed here: words (bare or in quotes) and the filters `project:`, `status:`, `cat:` and `since:`.
//! Every word is a prefix ("log" finds "login") and every word must be in the same title or the same message. A thread found by its
//! title is listed before one found by what was said in it (the thread's first words, then the parts). A title's matches are ranked by how well they fit and then by activity;
//! what was said comes newest first (a part's rowid grows with time), which lets SQLite stop after the first few hundred of a common
//! word's matches instead of ranking all of them.
//!
//! A search is a read. The daemon runs it on a connection of its own ([`Store::open_reader`]) on a thread of its own, so it never waits
//! for a run that is writing, and a run never waits for it.

use std::path::Path;

use neko_protocol::{Found, FoundBy, PartId, Task, TaskId, ThreadStatus};
use rusqlite::{Connection, params_from_iter, types::Value};

use crate::store::{Store, StoreError};

/// The most words of a query that count.
const MAX_WORDS: usize = 8;
/// How many candidates each of the two word searches keeps before filters that need a task's derived state are applied.
const CANDIDATES: usize = 300;
/// How many threads the recency scan reads (no words typed).
const SCAN: usize = 1000;
/// The most kittens in one answer, so threads are not crowded out.
const MAX_KITTENS: usize = 4;
/// The width of a message's snippet, in words around the match.
const SNIPPET_WORDS: i64 = 14;

/// A word of the query.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Word {
    /// A word typed bare: it matches as a prefix.
    Bare(String),
    /// Words in quotes: they match exactly, in order.
    Phrase(String),
}

/// What a thread's state may be for a `status:` filter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusFilter {
    Working,
    Waiting,
    Failed,
    Needs,
    Idle,
    Settled,
    Archived,
    Pinned,
}

impl StatusFilter {
    const NAMES: &'static str = "working, waiting, failed, needs, idle, settled, archived or pinned";

    fn of(word: &str) -> Option<StatusFilter> {
        Some(match word.to_lowercase().as_str() {
            "working" | "running" => StatusFilter::Working,
            "waiting" => StatusFilter::Waiting,
            "failed" => StatusFilter::Failed,
            "needs" | "needs-you" | "need" | "asking" => StatusFilter::Needs,
            "idle" | "done" => StatusFilter::Idle,
            "settled" => StatusFilter::Settled,
            "archived" => StatusFilter::Archived,
            "pinned" => StatusFilter::Pinned,
            _ => return None,
        })
    }

    fn holds(self, t: &Task) -> bool {
        match self {
            StatusFilter::Working => t.thread_status == ThreadStatus::Working,
            StatusFilter::Waiting => t.thread_status == ThreadStatus::Waiting,
            StatusFilter::Failed => t.thread_status == ThreadStatus::Failed,
            StatusFilter::Needs => t.needs_you,
            StatusFilter::Idle => t.thread_status == ThreadStatus::Idle,
            StatusFilter::Settled => t.settled,
            StatusFilter::Archived => t.archived,
            StatusFilter::Pinned => t.pinned,
        }
    }
}

/// The text a person typed, taken apart.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub words: Vec<Word>,
    /// Part of a project's name (`project:acme`); "none" and "general" mean No project.
    pub project: Option<String>,
    /// Any of these (`status:working|needs`).
    pub statuses: Vec<StatusFilter>,
    /// Part of a Cattain's name; threads have none now, so this reaches threads made before and by rounds.
    pub cat: Option<String>,
    /// Activity no earlier than this many milliseconds ago (`since:2d`).
    pub since_ms: Option<i64>,
    /// Why a filter could not be read, in words.
    pub problem: Option<String>,
}

impl Query {
    pub fn has_filters(&self) -> bool {
        self.project.is_some() || !self.statuses.is_empty() || self.cat.is_some() || self.since_ms.is_some()
    }
}

/// `2d`, `36h`, `30m`, `1w` as milliseconds.
fn span(value: &str) -> Option<i64> {
    let (n, unit) = value.split_at(value.len().checked_sub(1).filter(|&at| value.is_char_boundary(at))?);
    let n: i64 = n.parse().ok().filter(|n| (1..=100_000).contains(n))?;
    let unit = match unit.to_lowercase().as_str() {
        "m" => 60_000,
        "h" => 3_600_000,
        "d" => 86_400_000,
        "w" => 7 * 86_400_000,
        _ => return None,
    };
    Some(n * unit)
}

/// Takes the text apart. A filter with no value yet (`status:`) is dropped, so a half-typed filter is not searched for as a word.
pub fn parse(raw: &str) -> Query {
    let c: Vec<char> = raw.chars().collect();
    let mut q = Query::default();
    let mut i = 0;
    let quoted = |i: &mut usize| -> String {
        // c[*i] is the opening quote.
        *i += 1;
        let start = *i;
        while *i < c.len() && c[*i] != '"' {
            *i += 1;
        }
        let s: String = c[start..*i].iter().collect();
        *i = (*i + 1).min(c.len());
        s
    };
    while i < c.len() {
        if c[i].is_whitespace() {
            i += 1;
            continue;
        }
        if c[i] == '"' {
            let s = quoted(&mut i);
            if s.chars().any(char::is_alphanumeric) {
                q.words.push(Word::Phrase(s.split_whitespace().collect::<Vec<_>>().join(" ")));
            }
            continue;
        }
        let start = i;
        while i < c.len() && !c[i].is_whitespace() && c[i] != '"' {
            i += 1;
        }
        let tok: String = c[start..i].iter().collect();
        let key = tok.split_once(':').map(|(k, _)| k.to_lowercase()).filter(|k| ["project", "status", "cat", "since"].contains(&k.as_str()));
        let Some(key) = key else {
            if tok.chars().any(char::is_alphanumeric) {
                q.words.push(Word::Bare(tok));
            }
            continue;
        };
        let mut value = tok[key.len() + 1..].to_owned();
        // `project:"my app"`: the value goes on inside the quotes.
        if value.is_empty() && i < c.len() && c[i] == '"' {
            value = quoted(&mut i);
        }
        let value = value.trim().to_owned();
        if value.is_empty() {
            continue;
        }
        match key.as_str() {
            "project" => q.project = Some(value),
            "cat" => q.cat = Some(value),
            "status" => {
                for part in value.split(['|', ',']).filter(|p| !p.is_empty()) {
                    match StatusFilter::of(part) {
                        Some(s) => q.statuses.push(s),
                        None => q.problem = Some(format!("“{part}” is not a status. Try {}.", StatusFilter::NAMES)),
                    }
                }
            }
            _ => match span(&value) {
                Some(ms) => q.since_ms = Some(ms),
                None => q.problem = Some(format!("“since:{value}” needs a number and m, h, d or w, like since:2d.")),
            },
        }
    }
    q.words.truncate(MAX_WORDS);
    q
}

/// The FTS5 expression for the words: each in quotes (so nothing the person typed is read as FTS syntax), bare words as prefixes.
fn fts_expression(words: &[Word]) -> String {
    words
        .iter()
        .map(|w| match w {
            Word::Bare(s) => format!("\"{}\"*", s.replace('"', "\"\"")),
            Word::Phrase(s) => format!("\"{}\"", s.replace('"', "\"\"")),
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A line of `words`: its newlines and runs of spaces as single spaces.
fn one_line(words: &str) -> String {
    words.split_whitespace().collect::<Vec<_>>().join(" ")
}

struct Candidate {
    task: String,
    by: FoundBy,
    snippet: String,
    part: Option<String>,
    seq: u64,
}

impl Store {
    /// A connection for reading only, to the store at `path` as it is now: no migration is run, nothing can be written through it. The
    /// daemon opens one per search on the thread that answers it.
    pub fn open_reader(path: &Path) -> Result<Store, StoreError> {
        let conn = Connection::open(path)?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.pragma_update(None, "query_only", true)?;
        Ok(Store { conn })
    }

    /// The path of the database file this connection has open (None for an in-memory one).
    pub fn path(&self) -> Option<std::path::PathBuf> {
        self.conn.path().filter(|p| !p.is_empty()).map(std::path::PathBuf::from)
    }

    /// The threads and kittens `raw` finds: at most `limit`, titles first, then what was said. With no words, the most recent threads
    /// that no filter leaves out. Returns the results and, when a filter could not be read, why. `now_ms` is the clock for `since:`.
    pub fn search(&self, raw: &str, limit: usize, now_ms: i64) -> Result<(Vec<Found>, Option<String>), StoreError> {
        let q = parse(raw);
        if q.problem.is_some() {
            return Ok((Vec::new(), q.problem));
        }
        let limit = limit.clamp(1, neko_protocol::search::MAX_LIMIT as usize);
        let candidates = if q.words.is_empty() { self.scan(&q, now_ms)? } else { self.match_words(&q, now_ms)? };
        let mut found: Vec<Found> = Vec::new();
        let mut kittens = 0;
        for c in candidates {
            if found.len() >= limit {
                break;
            }
            if found.iter().any(|f| f.task.0 == c.task) {
                continue;
            }
            let Some(task) = self.get_task(&TaskId(c.task.clone()))? else { continue };
            if !q.statuses.is_empty() && !q.statuses.iter().any(|s| s.holds(&task)) {
                continue;
            }
            let kitten = task.subagent.is_some();
            if kitten {
                if kittens >= MAX_KITTENS.min(limit.div_ceil(3)) {
                    continue;
                }
                kittens += 1;
            }
            found.push(Found { task: task.id, kitten, by: c.by, snippet: c.snippet, part: c.part.map(PartId), seq: c.seq });
        }
        Ok((found, None))
    }

    /// The SQL that keeps only tasks (`t`) the filters allow, and what it binds; None when a filter can match nothing.
    fn filter_sql(&self, q: &Query, now_ms: i64) -> Result<Option<(String, Vec<Value>)>, StoreError> {
        let mut sql = String::new();
        let mut args: Vec<Value> = Vec::new();
        if let Some(p) = &q.project {
            let want = p.to_lowercase();
            let mut ids: Vec<String> = Vec::new();
            let mut st = self.conn.prepare_cached("SELECT id, name FROM workspaces")?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
            for r in rows {
                let (id, name) = r?;
                let none = id == crate::DEFAULT_WORKSPACE && ["none", "no project", "general"].iter().any(|n| n.contains(want.as_str()));
                if none || (id != crate::DEFAULT_WORKSPACE && name.to_lowercase().contains(&want)) {
                    ids.push(id);
                }
            }
            if ids.is_empty() {
                return Ok(None);
            }
            sql.push_str(&format!(" AND t.workspace_id IN ({})", vec!["?"; ids.len()].join(",")));
            args.extend(ids.into_iter().map(Value::Text));
        }
        if let Some(c) = &q.cat {
            sql.push_str(" AND t.cat_id IN (SELECT id FROM cats WHERE name_key LIKE ? ESCAPE '\\')");
            let like: String = c.to_lowercase().chars().flat_map(|ch| if matches!(ch, '%' | '_' | '\\') { vec!['\\', ch] } else { vec![ch] }).collect();
            args.push(Value::Text(format!("%{like}%")));
        }
        if let Some(ms) = q.since_ms {
            sql.push_str(" AND t.activity_ms >= ?");
            args.push(Value::Integer(now_ms.saturating_sub(ms)));
        }
        Ok(Some((sql, args)))
    }

    /// No words: threads by their last activity, newest first.
    fn scan(&self, q: &Query, now_ms: i64) -> Result<Vec<Candidate>, StoreError> {
        let Some((filters, args)) = self.filter_sql(q, now_ms)? else { return Ok(Vec::new()) };
        // With nothing typed and no filter this is the recent list: what is put away is not recent.
        let away = if !q.has_filters() { " AND t.archived_ms IS NULL AND t.settled_ms IS NULL" } else { "" };
        // A kitten is found by what is typed; by a filter alone it would crowd the list.
        let sql = format!("SELECT t.id FROM tasks t WHERE t.parent_id IS NULL{away}{filters} ORDER BY t.activity_ms DESC, t.id DESC LIMIT {SCAN}");
        let mut st = self.conn.prepare(&sql)?;
        let rows = st.query_map(params_from_iter(args), |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(Candidate { task: r?, by: FoundBy::Recent, snippet: String::new(), part: None, seq: 0 })).collect()
    }

    /// Words: the tasks whose title has them all, best first, then the messages that have them all.
    fn match_words(&self, q: &Query, now_ms: i64) -> Result<Vec<Candidate>, StoreError> {
        let Some((filters, args)) = self.filter_sql(q, now_ms)? else { return Ok(Vec::new()) };
        let expr = fts_expression(&q.words);
        let with = |expr: String, args: &[Value]| std::iter::once(Value::Text(expr)).chain(args.iter().cloned()).collect::<Vec<_>>();
        let mut out: Vec<Candidate> = Vec::new();
        // A title that has every word, best first.
        let titles = format!("SELECT t.id FROM find_titles JOIN tasks t ON t.rowid = find_titles.rowid WHERE find_titles MATCH ?{filters} ORDER BY bm25(find_titles), t.activity_ms DESC LIMIT {CANDIDATES}");
        let mut st = self.conn.prepare(&titles)?;
        let rows = st.query_map(params_from_iter(with(format!("title : ({expr})"), &args)), |r| r.get::<_, String>(0))?;
        for r in rows {
            out.push(Candidate { task: r?, by: FoundBy::Title, snippet: String::new(), part: None, seq: 0 });
        }
        // A thread's first words are the task's own, not a part: found here, with no part to open at.
        let first = format!("SELECT t.id, snippet(find_titles, 1, '', '', '…', {SNIPPET_WORDS}) FROM find_titles JOIN tasks t ON t.rowid = find_titles.rowid WHERE find_titles MATCH ?{filters} ORDER BY find_titles.rowid DESC LIMIT {CANDIDATES}");
        let mut st = self.conn.prepare(&first)?;
        let rows = st.query_map(params_from_iter(with(format!("words : ({expr})"), &args)), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for r in rows {
            let (task, snippet) = r?;
            out.push(Candidate { task, by: FoundBy::Message, snippet: one_line(&snippet), part: None, seq: 0 });
        }
        let said = format!(
            "SELECT p.task_id, p.id, p.seq, snippet(find_words, 0, '', '', '…', {SNIPPET_WORDS}) FROM find_words JOIN parts p ON p.rowid = find_words.rowid JOIN tasks t ON t.id = p.task_id WHERE find_words MATCH ?{filters} ORDER BY find_words.rowid DESC LIMIT {CANDIDATES}"
        );
        let mut st = self.conn.prepare(&said)?;
        let rows = st.query_map(params_from_iter(with(expr.clone(), &args)), |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?, r.get::<_, String>(3)?)))?;
        for r in rows {
            let (task, part, seq, snippet) = r?;
            out.push(Candidate { task, by: FoundBy::Message, snippet: one_line(&snippet), part: Some(part), seq: seq.max(0) as u64 });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_filters_and_quotes_are_taken_apart() {
        let q = parse(r#"fix "login redirect" project:acme status:needs|failed since:2d cat:miso"#);
        assert_eq!(q.words, [Word::Bare("fix".into()), Word::Phrase("login redirect".into())]);
        assert_eq!((q.project.as_deref(), q.cat.as_deref(), q.since_ms), (Some("acme"), Some("miso"), Some(2 * 86_400_000)));
        assert_eq!(q.statuses, [StatusFilter::Needs, StatusFilter::Failed]);
        assert_eq!(q.problem, None);
        assert_eq!(parse(r#"project:"my app" notes"#).project.as_deref(), Some("my app"));
    }

    #[test]
    fn a_half_typed_filter_is_not_a_word_and_a_wrong_one_says_why() {
        let q = parse("status: login");
        assert_eq!((q.words, q.statuses, q.problem), (vec![Word::Bare("login".into())], vec![], None));
        assert!(parse("status:sleepy").problem.unwrap().contains("not a status"));
        assert!(parse("since:soon").problem.unwrap().contains("since:2d"));
        // Punctuation alone is not a word; a colon in an ordinary word stays one.
        assert_eq!(parse("?! :: note:this").words, [Word::Bare("note:this".into())]);
    }

    #[test]
    fn nothing_typed_is_ever_read_as_search_syntax() {
        assert_eq!(fts_expression(&parse(r#"NEAR(a b) AND "x y" a"b"#).words), r#""NEAR(a"* "b)"* "AND"* "x y" "a"* "b""#);
    }
}
