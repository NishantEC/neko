//! The Task view (docs/knowledge/app-shell.md, "The Task view"): the conversation with the agent, and the composer under it.
//!
//! The answer is the page: the agent's words sit on it with no bubble. Thinking and tool calls are quiet single lines that fold.
//! While a run works its steps show as they happen; once it ends they fold behind one "Worked for 2m 14s" line and its final
//! answer stays open. A run card shows only when the run changed files, has a branch or failed. Everything shares one reading
//! column: one left edge and one width. Nothing about money shows until the person opens the context ring in the composer.
//! Colour is for state only.

use std::rc::Rc;

use neko_protocol::{Part, PartBody, Permission, PermissionState, Pricing, Run, RunStatus, Task, Tone as NoticeTone, ToolState};
use rune::{El, Tx, col, el, row, text};
use rune_ui_style::{tabular, weight};

use crate::ui::base::Shape;

use crate::state::{App, Book};
use crate::ui::agent::{FoldLine, PressTx, Status, elapsed, icon_button, mono};
use crate::ui::badge::Badge;
use crate::ui::base::{Theme, Tone, glyph, paint};
use crate::ui::confirmation::{Confirmation, Decision};
use crate::ui::context::{Context, Usage as Meter};
use crate::ui::conversation::Conversation;
use crate::ui::icons;
use crate::ui::message::{self, BLOCK_GAP, Message};
use crate::ui::prompt_input::{Phase, PromptInput};
use crate::ui::reasoning::Reasoning;
use crate::ui::response::Response;
use crate::ui::shimmer::Shimmer;
use crate::ui::task as card;
use crate::ui::tool::{Tool, ToolCall};
use crate::view::COLUMN;

/// What the agent is called in this slice (named cats come later).
pub const AGENT: &str = "Neko";

/// The context window assumed when the CLI did not report one.
pub const ASSUMED_WINDOW: u64 = 200_000;

/// The longest a step's line is, in characters, before its middle is cut (its whole text is in the fold).
const LINE_CHARS: usize = 72;

/// A copy button for `words`.
fn copy(t: &Theme, key: &str, words: String) -> El {
    let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(words.clone()));
    icon_button(t, &format!("copy-{key}"), "Copy", &icons::COPY, None, false, Some(press))
}

/// The person's words, at the right; their copy button shows only while the pointer is over them or the keyboard is in them, and
/// takes no room while hidden (rune-ui's `reveal_actions`).
fn person(s: &App, t: &Theme, key: &str, words_key: &str, words: &str) -> El {
    let said = message::words(t, words.to_owned()).key(words_key.to_owned()).selectable(true);
    let k = key.to_owned();
    Message::user(key.to_owned(), "You")
        .child(said)
        .actions(row().child(copy(t, key, words.to_owned())))
        .reveal_actions(s.reveal(key), move |s: &mut App| s.reveals.entry(k.clone()).or_default())
        .column(COLUMN)
        .build(t)
}

/// The agent's words as Markdown on the page: rune-ui's `Response`, selectable once whole. Interim words (`muted`) are in the second
/// ink, quieter than the answer. Only a run's final answer (`last`) has a copy button, right under it.
fn answer(t: &Theme, key: &str, words: &str, streaming: bool, last: bool, muted: bool) -> El {
    let body = Response::new(format!("{key}-answer"), words).streaming(streaming).selectable(!streaming).muted(muted).column(COLUMN).build(t);
    if last && !streaming {
        return col().key(key.to_owned()).w_full().gap(t.space.at(BLOCK_GAP)).child(body).child(row().child(copy(t, key, words.to_owned())));
    }
    body
}

fn secs(p: &Part) -> f64 {
    p.ended_ms.map_or(0.0, |e| (e - p.at_ms).max(0) as f64 / 1000.0)
}

/// `s` with its middle cut to fit `max` characters.
pub fn cut_middle(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_owned();
    }
    let keep = max.saturating_sub(1);
    let head = keep.div_ceil(2);
    let tail = keep - head;
    format!("{}…{}", chars[..head].iter().collect::<String>(), chars[chars.len() - tail..].iter().collect::<String>())
}

/// A path as a person reads it: relative inside the task's folder, `~/…` under their home, else whole.
pub fn show_path(path: &str, workdir: &str, home: Option<&str>) -> String {
    let w = workdir.trim_end_matches('/');
    if !w.is_empty()
        && let Some(rest) = path.strip_prefix(w).and_then(|r| r.strip_prefix('/'))
        && !rest.is_empty()
    {
        return rest.to_owned();
    }
    if let Some(h) = home.map(|h| h.trim_end_matches('/')).filter(|h| !h.is_empty())
        && let Some(rest) = path.strip_prefix(h)
        && (rest.is_empty() || rest.starts_with('/'))
    {
        return format!("~{rest}");
    }
    path.to_owned()
}

/// One line for a command: its lines joined, spaces collapsed.
fn one_line(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What a step did, in a short human phrase: a shell step is its command; other tools read as words ("Read src/app.rs",
/// "Edited notes.md", "Searched for “Settings”").
pub fn phrase(name: &str, summary: &str, input: &str, running: bool, workdir: &str, home: Option<&str>) -> String {
    let path = || show_path(summary, workdir, home);
    let tense = |now: &str, done: &str| if running { now.to_owned() } else { done.to_owned() };
    let words = match name {
        "Bash" => one_line(if input.trim().is_empty() { summary } else { input }),
        "Read" | "NotebookRead" => format!("{} {}", tense("Reading", "Read"), path()),
        "Edit" | "MultiEdit" | "NotebookEdit" => format!("{} {}", tense("Editing", "Edited"), path()),
        "Write" => format!("{} {}", tense("Writing", "Wrote"), path()),
        "Grep" => format!("{} “{summary}”", tense("Searching for", "Searched for")),
        "Glob" => format!("{} {summary}", tense("Finding", "Found")),
        "LS" => format!("{} {}", tense("Listing", "Listed"), path()),
        "WebFetch" => format!("{} {summary}", tense("Fetching", "Fetched")),
        "WebSearch" => format!("{} “{summary}”", tense("Searching the web for", "Searched the web for")),
        "Task" | "Agent" => format!("{} {summary}", tense("Asking a helper:", "Asked a helper:")),
        "TodoWrite" => tense("Updating the plan", "Updated the plan"),
        other => {
            // An MCP tool reads as its own name and its server's: `mcp__acme__find_issue` is "find issue (acme)".
            let named = match other.strip_prefix("mcp__").and_then(|r| r.split_once("__")) {
                Some((server, tool)) => format!("{} ({server})", tool.replace('_', " ")),
                None => other.to_owned(),
            };
            format!("{named} {summary}").trim().to_owned()
        }
    };
    cut_middle(words.trim(), LINE_CHARS)
}

/// A tool call: rune-ui's `Tool`, its line Neko's phrase, its note a command's exit code, a step the person denied neutral.
fn tool_line(s: &App, t: &Theme, key: &str, p: &Part, workdir: &str) -> Option<El> {
    let PartBody::Tool { name, summary, input, output, state, exit, .. } = &p.body else { return None };
    let status = match state {
        ToolState::Running => Status::Running,
        ToolState::Done => Status::Done,
        ToolState::Failed => Status::Error,
        ToolState::Denied => Status::Denied,
    };
    let failed = *state == ToolState::Failed;
    let call = ToolCall {
        name: name.clone(),
        summary: summary.clone(),
        status,
        secs: (*state != ToolState::Running && p.ended_ms.is_some()).then(|| secs(p)),
        input: input.clone(),
        output: if failed { None } else { output.clone().filter(|o| !o.is_empty()) },
        error: failed.then(|| output.clone().filter(|o| !o.is_empty()).unwrap_or_else(|| "It did not finish.".to_owned())),
    };
    let mut tool = Tool::<App>::new(key.to_owned(), &call, s.is_open(key)).label(phrase(name, summary, input, *state == ToolState::Running, workdir, s.home.as_deref()));
    if let (ToolState::Done, Some(code)) = (state, exit) {
        tool = tool.note(format!("exit {code}"));
    }
    let k = key.to_owned();
    Some(tool.on_toggle(move |s: &mut App| s.flip(&k)).build(t))
}

/// A thought: rune-ui's `Reasoning` ("Thinking…" live, "Thought for 12s" folding to its words). A thought with no words is no step:
/// it is left out, so it neither counts nor folds (Claude Code sends none today; the live line says "Thinking…").
fn thought(s: &App, t: &Theme, key: &str, p: &Part) -> Option<El> {
    let PartBody::Thinking { text: words, done } = &p.body else { return None };
    if words.trim().is_empty() {
        return None;
    }
    let k = key.to_owned();
    Some(Reasoning::new(key.to_owned(), !*done, secs(p), s.is_open(key)).text(words.clone()).on_toggle(move |s: &mut App| s.flip(&k)).build(t))
}

/// A permission request: rune-ui's `Confirmation`, with why Neko asks under it; a destructive one has no default answer and no
/// "Always". Answered, it is one quiet line.
fn request(t: &Theme, p: &Permission) -> El {
    let key = format!("perm-{}", p.id.0);
    let decision = match p.state {
        PermissionState::Pending => Decision::Pending,
        PermissionState::Allowed => Decision::AllowedOnce,
        PermissionState::AlwaysAllowed => Decision::AlwaysAllowed,
        PermissionState::Denied => Decision::Denied,
        PermissionState::Expired => Decision::Expired,
    };
    if decision != Decision::Pending {
        return Confirmation::<App>::new(key, p.title.clone(), decision).build(t);
    }
    let id = p.id.clone();
    Confirmation::<App>::new(key, p.title.clone(), decision)
        .detail(p.detail.clone())
        .why(format!("Neko asks because {}", lower_first(&p.why)))
        .destructive(p.destructive)
        .on_decide(move |s: &mut App, d| {
            let answer = match d {
                Decision::AllowedOnce => neko_protocol::Decision::AllowOnce,
                Decision::AlwaysAllowed => neko_protocol::Decision::AlwaysAllow,
                _ => neko_protocol::Decision::Deny,
            };
            s.decide(&id, answer);
        })
        .build(t)
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// A notice: what happened, and what to do next.
fn notice(t: &Theme, p: &Part) -> Option<El> {
    let PartBody::Notice { tone, text: words, next } = &p.body else { return None };
    let (icon, ink) = match tone {
        NoticeTone::Error => (&icons::CIRCLE_X, t.c.danger.fg),
        NoticeTone::Warning => (&icons::WARNING, t.c.warning.fg),
        NoticeTone::Info => (&icons::INFO, t.ink_faint()),
    };
    let label = match next {
        Some(n) => format!("{words} {n}"),
        None => words.clone(),
    };
    let mut lines = col().gap(t.space.at(1.0)).min_w(0.0).shrink().child(text(words.clone()).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).wrap().min_w(0.0).selectable(true));
    if let Some(n) = next {
        lines = lines.child(text(n.clone()).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).selectable(true));
    }
    Some(
        row()
            .key(format!("notice-{}", p.seq))
            .role("status")
            .label(label)
            .w_full()
            .gap(t.metrics.gap)
            .items_start()
            .child(row().size(t.control.icon, t.ty.base.line_height.max(t.control.icon)).center().child(glyph(icon, t.control.icon, ink)))
            .child(lines),
    )
}

/// The run card, only when a run has something to show beyond time: files changed, a branch, or a failure. rune-ui's `Task` with
/// only the facts Neko has (no checks, no money, no time: the run's line has it).
fn run_card(t: &Theme, r: &Run) -> Option<El> {
    let changed = r.changes.filter(|c| c.files > 0);
    let failed = r.status == RunStatus::Failed;
    if changed.is_none() && r.branch.is_none() && !failed {
        return None;
    }
    let status = match r.status {
        RunStatus::Running => Status::Running,
        RunStatus::Done => Status::Done,
        RunStatus::Failed => Status::Error,
        // rune-ui's statuses have no "Stopped": its pill would say "Skipped" (an upstream ask). Neko keeps its own pill for these.
        RunStatus::Stopped | RunStatus::Interrupted => return Some(stopped_card(t, r, changed)),
    };
    // A short title, not the task's again (the band has it): what the card is about.
    let title = if failed { "Could not finish" } else if changed.is_some() { "Changes" } else { "Branch" };
    let mut card = card::Run::new(title, status);
    if let Some(c) = changed {
        card = card.changes(c.files as usize, c.added as usize, c.removed as usize);
    }
    if let Some(b) = &r.branch {
        card = card.branch(b.clone());
    }
    Some(card::Task::<App>::new(format!("run-{}", r.id.0), &card).build(t))
}

/// A stopped or interrupted run's card, in the look of rune-ui's `Task` with the right word on its pill.
fn stopped_card(t: &Theme, r: &Run, changed: Option<neko_protocol::Changes>) -> El {
    let word = if r.status == RunStatus::Stopped { "Stopped" } else { "Interrupted" };
    let fact = |icon, words: El| row().gap(t.space.at(1.5)).center_y().child(glyph(icon, t.control.icon_sm, t.ink_faint())).child(words);
    let figures = |s: String| text(s).font(tabular(t.ty.sm)).color(t.ink_muted());
    let mut facts = row().key(format!("run-{}-facts", r.id.0)).grow(1.0).min_w(0.0).gap(t.space.at(5.0)).flex_wrap().center_y();
    if let Some(c) = changed {
        let changes = row()
            .gap(t.space.at(1.5))
            .child(figures(format!("{} file{}", c.files, if c.files == 1 { "" } else { "s" })))
            .child(text(format!("+{}", c.added)).font(tabular(t.ty.sm)).color(t.c.success.fg))
            .child(text(format!("\u{2212}{}", c.removed)).font(tabular(t.ty.sm)).color(t.c.danger.fg));
        facts = facts.child(fact(&icons::FILE, changes));
    }
    if let Some(b) = &r.branch {
        facts = facts.child(fact(&icons::COMMIT, text(b.clone()).font(mono(t, t.ty.sm)).color(t.ink_muted()).truncate_middle().min_w(0.0).shrink()));
    }
    let head = row().w_full().gap(t.space.at(3.0)).center_y().child(Badge::new(format!("run-{}-status", r.id.0), word).tone(Tone::Neutral).dot().build(t)).child(facts);
    paint(col().key(format!("run-{}", r.id.0)).role("group").label(format!("Run: {word}")).w_full().padding(t.metrics.inset).gap(t.space.at(3.0)).radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(head)
}

/// What the transcript is made of, in order.
enum Block<'a> {
    Person(&'a Part),
    Run(&'a Run, Vec<&'a Part>),
    Loose(&'a Part),
}

fn blocks<'a>(book: &'a Book) -> Vec<Block<'a>> {
    let mut out: Vec<Block<'a>> = Vec::new();
    for p in &book.parts {
        if let PartBody::Person { .. } = p.body {
            out.push(Block::Person(p));
            continue;
        }
        let Some(run) = p.run.as_ref().and_then(|id| book.runs.iter().find(|r| &r.id == id)) else {
            out.push(Block::Loose(p));
            continue;
        };
        match out.last_mut() {
            Some(Block::Run(r, parts)) if r.id == run.id => parts.push(p),
            _ => out.push(Block::Run(run, vec![p])),
        }
    }
    // A run that has said nothing yet still shows (working, or its line).
    for r in &book.runs {
        if !out.iter().any(|b| matches!(b, Block::Run(x, _) if x.id == r.id)) {
            let at = out.iter().position(|b| match b {
                Block::Person(p) | Block::Loose(p) => p.at_ms > r.started_ms,
                Block::Run(x, _) => x.started_ms > r.started_ms,
            });
            out.insert(at.unwrap_or(out.len()), Block::Run(r, Vec::new()));
        }
    }
    out
}

/// Thinking and tool calls: the steps a finished run folds and counts.
fn quiet_line(p: &Part) -> bool {
    matches!(p.body, PartBody::Thinking { .. } | PartBody::Tool { .. })
}

/// One part as drawn in a run: a step, words on the way (`muted`, or the final answer when `last`), a request, a notice.
fn part_el(s: &App, t: &Theme, book: &Book, r: &Run, p: &Part, streaming: bool, last: bool, muted: bool) -> Option<El> {
    let key = format!("part-{}", p.seq);
    match &p.body {
        PartBody::Thinking { .. } => thought(s, t, &key, p),
        PartBody::Tool { .. } => tool_line(s, t, &key, p, &r.workdir),
        PartBody::Permission { permission } => book.permissions.get(permission).map(|perm| request(t, perm)),
        PartBody::Agent { text: words, .. } if words.trim().is_empty() => None,
        PartBody::Agent { text: words, .. } => Some(answer(t, &key, words, streaming, last, muted)),
        PartBody::Notice { .. } => notice(t, p),
        PartBody::Person { .. } => None,
    }
}

/// Parts in order. Words with a step after them are interim (`muted`); `last_at` is the final answer.
fn lay_out(s: &App, t: &Theme, book: &Book, r: &Run, parts: &[&Part], streaming_at: Option<usize>, last_at: Option<usize>) -> Vec<El> {
    let last_step = parts.iter().rposition(|p| quiet_line(p));
    parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let interim = last_step.is_some_and(|l| i < l) && Some(i) != last_at;
            part_el(s, t, book, r, p, Some(i) == streaming_at, Some(i) == last_at, interim)
        })
        .collect()
}

/// What a finished run's line says: how long it worked, or how it ended.
fn worked_words(r: &Run) -> String {
    let secs = r.ended_ms.map_or(0.0, |e| (e - r.started_ms).max(0) as f64 / 1000.0).round().max(1.0);
    let d = elapsed(secs);
    match r.status {
        RunStatus::Stopped => format!("Stopped after {d}"),
        RunStatus::Interrupted => format!("Interrupted after {d}"),
        _ => format!("Worked for {d}"),
    }
}

/// The live line: "Thinking…" while the agent thinks and nothing else arrives, "Working…" while it waits on something else.
fn live_line(t: &Theme, r: &Run, thinking: bool) -> El {
    let words = if thinking { "Thinking…" } else { "Working…" };
    row()
        .key(format!("run-{}-working", r.id.0))
        .role("status")
        .label(if thinking { "Neko is thinking" } else { "Neko is working" })
        .w_full()
        .h(t.control.md)
        .pl(t.space.at(1.5))
        .gap(t.metrics.gap)
        .center_y()
        .child(row().size(t.control.icon, t.control.icon).center().child(glyph(&icons::SPARKLE, t.control.icon, t.ink_faint())))
        .child(Shimmer::new(format!("run-{}-shimmer", r.id.0), words).build(t).a11y_hidden())
}

/// One run, as one turn of the agent's (rune-ui's rhythm: its blocks [`BLOCK_GAP`] apart). While it works: its steps and words as
/// they come, and the live line. Once it ends: one "Worked for" fold that opens to its steps on the page's own edge (`indent(false)`),
/// the final answer, notices, and the card if it earns one.
fn run_block(s: &App, t: &Theme, book: &Book, r: &Run, parts: &[&Part]) -> El {
    let turn = Message::assistant(format!("turn-{}", r.id.0), AGENT).column(COLUMN);
    if r.status == RunStatus::Running {
        let streaming = parts.last().is_some_and(|p| matches!(&p.body, PartBody::Agent { done: false, .. }) || matches!(&p.body, PartBody::Thinking { done: false, text } if !text.trim().is_empty()));
        let mut out = lay_out(s, t, book, r, parts, if streaming { Some(parts.len() - 1) } else { None }, None);
        let waiting = parts.iter().any(|p| matches!(&p.body, PartBody::Permission { permission } if book.permissions.get(permission).is_some_and(|x| x.state == PermissionState::Pending)));
        let tool_running = parts.iter().any(|p| matches!(&p.body, PartBody::Tool { state: ToolState::Running, .. }));
        if !streaming && !waiting && !tool_running {
            out.push(live_line(t, r, book.thinking));
        }
        return out.into_iter().fold(turn, Message::child).build(t);
    }
    // The final answer is the words after the last step; notices stay in sight; everything else folds.
    let tail_from = parts.iter().rposition(|p| !matches!(p.body, PartBody::Agent { .. } | PartBody::Notice { .. })).map_or(0, |i| i + 1);
    let (work, tail) = parts.split_at(tail_from);
    let work: Vec<&Part> = work.iter().copied().filter(|p| part_el(s, t, book, r, p, false, false, false).is_some() && !matches!(p.body, PartBody::Notice { .. })).collect();
    let key = format!("worked-{}", r.id.0);
    let mut out = Vec::new();
    let lead = row().size(t.control.icon, t.control.icon).center().child(glyph(&icons::CLOCK, t.control.icon, t.ink_faint()));
    if work.is_empty() {
        // Nothing to open: one quiet line with the time.
        out.push(row().key(key).role("status").label(worked_words(r)).w_full().h(t.control.md).pl(t.space.at(1.5)).gap(t.metrics.gap).center_y().child(lead).child(text(worked_words(r)).font(t.ty.base).color(t.ink_muted()).a11y_hidden()));
    } else {
        let n = work.iter().filter(|p| quiet_line(p)).count();
        let k = key.clone();
        let mut line = FoldLine::<App>::new(key.clone(), worked_words(r), s.is_open(&key)).lead(lead).indent(false).on_toggle(move |s: &mut App| s.flip(&k));
        if n > 0 {
            line = line.meta(format!("{n} step{}", if n == 1 { "" } else { "s" }));
        }
        // Inside the fold every word is interim: quieter than the answer.
        let steps = if s.is_open(&key) { vec![col().key(format!("{key}-steps")).w_full().gap(t.space.at(BLOCK_GAP)).children(lay_out_folded(s, t, book, r, &work))] } else { Vec::new() };
        out.push(line.build(t, steps));
    }
    let tail_last = tail.iter().rposition(|p| matches!(&p.body, PartBody::Agent { text, .. } if !text.trim().is_empty()));
    out.extend(lay_out(s, t, book, r, tail, None, tail_last));
    out.extend(run_card(t, r));
    out.into_iter().fold(turn, Message::child).build(t)
}

/// A finished run's folded work: steps, and words that are all interim.
fn lay_out_folded(s: &App, t: &Theme, book: &Book, r: &Run, work: &[&Part]) -> Vec<El> {
    work.iter().filter_map(|p| part_el(s, t, book, r, p, false, false, true)).collect()
}

/// What the task's runs used, for the context ring: the context the newest run held against its window, and the tokens and cost
/// over every run, as reported.
pub struct TaskUsage {
    pub context: u64,
    pub window: u64,
    pub window_assumed: bool,
    pub input: u64,
    pub output: u64,
    pub cached: u64,
    pub cost: Option<f64>,
    pub pricing: Pricing,
    pub reported_by: String,
}

pub fn task_usage(book: &Book) -> Option<TaskUsage> {
    let with: Vec<&neko_protocol::Usage> = book.runs.iter().filter_map(|r| r.usage.as_ref()).collect();
    let newest = with.last()?;
    let held = with.iter().rev().find(|u| u.context_tokens.is_some()).copied();
    let window = held.and_then(|u| u.context_window).or(newest.context_window);
    Some(TaskUsage {
        context: held.and_then(|u| u.context_tokens).unwrap_or(0),
        window: window.unwrap_or(ASSUMED_WINDOW),
        window_assumed: window.is_none(),
        input: with.iter().map(|u| u.input_tokens).sum(),
        output: with.iter().map(|u| u.output_tokens).sum(),
        cached: with.iter().map(|u| u.cached_tokens).sum(),
        cost: with.iter().filter_map(|u| u.cost_usd).fold(None, |a, c| Some(a.unwrap_or(0.0) + c)),
        pricing: newest.pricing,
        reported_by: if newest.reported_by.is_empty() { "the agent".to_owned() } else { newest.reported_by.clone() },
    })
}

/// The line under the cost, only for a subscription login, where the cost the CLI reports is not money charged.
pub fn cost_note(u: &TaskUsage) -> Option<String> {
    (u.cost.is_some() && u.pricing == Pricing::ApiEquivalent).then(|| format!("API-equivalent, as {} reports it. Not charged on a subscription.", u.reported_by))
}

/// The context ring in the composer's footer: rune-ui's Context (the ring face and its popover: context used and window, tokens in,
/// out and cached, the cost when reported, and the note for a subscription). The only place money shows. None before any run
/// reported usage.
fn context_ring(s: &App, t: &Theme, book: &Book, layer: &mut Vec<El>) -> Option<El> {
    let u = task_usage(book)?;
    let meter = Meter { context: u.context, limit: u.window, input: u.input, output: u.output, cache: u.cached, cost: u.cost };
    let mut ring = Context::<App>::new("ctx", &meter, s.context_open).on_open(|s: &mut App, open| s.context_open = open);
    if let Some(n) = cost_note(&u) {
        ring = ring.note(n);
    }
    Some(ring.build(t, layer))
}

/// The open task: its conversation, and its composer.
pub fn task_view(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let Some(task) = s.open_task() else { return el() };
    let empty = Book::default();
    let book = s.book().unwrap_or(&empty);
    // The task's own words first (their text keyed "task-words", as in S0).
    let mut items = vec![person(s, t, "task-message", "task-words", &task.words)];
    if book.loaded && book.runs.is_empty() && book.parts.is_empty() && task.status == neko_protocol::TaskStatus::New {
        items.push(status_line(s, t, task));
    }
    for b in blocks(book) {
        match b {
            Block::Person(p) => items.push(person(s, t, &format!("part-{}", p.seq), &format!("part-{}-words", p.seq), p.body.text())),
            Block::Loose(p) => items.extend(notice(t, p)),
            Block::Run(r, parts) => items.push(run_block(s, t, book, r, &parts)),
        }
    }
    // One reading column: the conversation's content and the composer have the same width and left edge.
    let inset = t.metrics.inset;
    let conversation = Conversation::<App>::new("transcript", s.at_bottom).column(COLUMN + 2.0 * inset).children(items).on_at_bottom(|s: &mut App, at| s.at_bottom = at).build(t);
    let working = s.open_is_working();
    let reply = s.reply_box().cloned().unwrap_or_default();
    let mut composer = PromptInput::new("reply", &reply, |s: &mut App| s.reply_mut())
        .placeholder(if working { "Neko is working. Press Escape to stop." } else { "Reply to Neko" })
        .spellcheck(true)
        .phase(if working { Phase::Streaming } else { Phase::Ready })
        .on_submit(|s: &mut App| s.follow_up())
        .on_stop(|s: &mut App| s.stop());
    if let Some(ring) = context_ring(s, t, book, layer) {
        composer = composer.tool(ring);
    }
    let composer = composer.build(t);
    let mut foot = col().key("reply-area").w_full().max_w(COLUMN).gap(t.space.at(2.0)).child(composer);
    if let Some(n) = &s.notice {
        foot = foot.child(text(n.clone()).key("reply-notice").role("status").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    col()
        .key("task")
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .items_center()
        .child(col().w_full().grow(1.0).min_h(0.0).child(conversation))
        .child(row().w_full().justify_center().px(inset).pt(t.space.at(2.0)).pb(t.space.at(5.0)).child(foot))
}

/// S0's quiet line for a task no agent has started.
fn status_line(s: &App, t: &Theme, task: &Task) -> El {
    let added = crate::clock::added(task.created_ms, s.now_ms, s.utc_offset_s);
    row()
        .key("task-status")
        .role("status")
        .label(format!("No cat is working on this yet. Added {added}."))
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .justify_end()
        .pr(t.space.at(2.0))
        .child(glyph(&icons::CIRCLE, t.control.icon_sm, t.ink_faint()))
        .child(text("No cat is working on this yet").font(t.ty.sm).color(t.ink_muted()).a11y_hidden())
        .child(text(format!("· Added {added}")).font(t.ty.sm).color(t.ink_faint()).a11y_hidden())
}

/// A monospaced line the person can copy (an install or sign-in command).
pub fn command_line(t: &Theme, key: &str, words: &str) -> El {
    row()
        .key(key.to_owned())
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .child(text(words.to_owned()).font(mono(t, t.ty.sm)).color(t.ink()).selectable(true).truncate().min_w(0.0).shrink())
        .child(copy(t, key, words.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_read_as_phrases_with_short_paths() {
        let w = "/Users/example/Library/Application Support/neko/worktrees/x7k2m9qa";
        let home = Some("/Users/example");
        assert_eq!(phrase("Bash", "cargo test", "cargo test\n  --all", false, w, home), "cargo test --all", "a command is itself, on one line");
        assert_eq!(phrase("Read", &format!("{w}/src/app.rs"), "{}", false, w, home), "Read src/app.rs");
        assert_eq!(phrase("Read", "src/app.rs", "{}", false, w, home), "Read src/app.rs");
        assert_eq!(phrase("Edit", "/Users/example/notes.md", "{}", false, w, home), "Edited ~/notes.md");
        assert_eq!(phrase("Edit", "notes.md", "{}", true, w, home), "Editing notes.md");
        assert_eq!(phrase("Grep", "Settings", "{}", false, w, home), "Searched for “Settings”");
        assert_eq!(phrase("mcp__acme__find_issue", "ENG-123", "{}", false, w, home), "find issue (acme) ENG-123");
        assert_eq!(show_path("/Users/examplex/a", w, home), "/Users/examplex/a", "a sibling of home is not home");
        let long = phrase("Bash", "", &format!("git status --short && git log --oneline -5 --decorate && {}", "x ".repeat(40)), false, w, home);
        assert_eq!(long.chars().count(), LINE_CHARS);
        assert!(long.starts_with("git status --short") && long.contains('…'), "{long}");
    }
}
