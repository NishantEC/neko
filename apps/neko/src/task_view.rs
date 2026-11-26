//! The Task view (docs/knowledge/app-shell.md, "The Task view"): the conversation with the agent, and the composer under it.
//!
//! The answer is the page: the agent's words sit on it with no bubble. Thinking and tool calls are quiet single lines that fold.
//! While a run works its steps show as they happen; once it ends they fold behind one "Worked for 2m 14s" line and its final
//! answer stays open. A run card shows only when the run changed files, has a branch or failed. Everything shares one reading
//! column: one left edge and one width. Nothing about money shows until the person opens the context ring in the composer.
//! Colour is for state only.

use std::rc::Rc;

use neko_protocol::{Part, PartBody, PermissionState, Pricing, Run, RunStatus, Task, Tone as NoticeTone, ToolState};
use rune::{El, Tx, col, el, row, text};
use rune_ui_style::weight;

use crate::state::{App, Book};
use crate::turn;
use crate::ui::agent::{FoldLine, PressTx, elapsed, icon_button, mono};
use crate::ui::base::{Theme, glyph};
use crate::ui::context::{Context, Usage as Meter};
use crate::ui::conversation::Conversation;
use crate::ui::icons;
use crate::ui::message::{BLOCK_GAP, Message};
use crate::ui::prompt_input::{Phase, PromptInput};
use crate::ui::response::Response;
use crate::view::COLUMN;
use neko_protocol::undo::{self, Offer};

/// The context window assumed when the CLI did not report one.
pub const ASSUMED_WINDOW: u64 = 200_000;

/// The longest a step's line is, in characters, before its middle is cut (its whole text is in the fold).
const LINE_CHARS: usize = 72;

/// A copy button for `words`.
fn copy(t: &Theme, key: &str, words: String) -> El {
    let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(words.clone()));
    icon_button(t, &format!("copy-{key}"), "Copy", &icons::COPY, None, false, Some(press))
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

/// A notice: what happened, and what to do next.
fn notice(t: &Theme, p: &Part) -> Option<El> {
    let PartBody::Notice { tone, text: words, next, .. } = &p.body else { return None };
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

/// The quiet line before a run whose cat's rules changed since the task's last run (docs/knowledge/cats.md).
fn rules_changed(t: &Theme, p: &Part) -> Option<El> {
    let PartBody::RulesChanged { name, .. } = &p.body else { return None };
    let words = format!("{name}'s rules changed since the last run.");
    Some(
        row()
            .key(format!("rules-changed-{}", p.seq))
            .role("status")
            .label(words.clone())
            .w_full()
            .h(t.control.md)
            .pl(t.space.at(1.5))
            .gap(t.metrics.gap)
            .center_y()
            .child(row().size(t.control.icon, t.control.icon).center().child(glyph(&icons::INFO, t.control.icon, t.ink_faint())))
            .child(text(words).font(t.ty.sm).color(t.ink_muted()).a11y_hidden()),
    )
}

/// What undo offers for a run (docs/knowledge/undo.md), from all of its task's runs.
pub fn offer_of(book: &Book, r: &Run) -> Offer {
    book.runs.iter().position(|x| x.id == r.id).map(|i| undo::offers(&book.runs).swap_remove(i)).unwrap_or_default()
}

/// "From the round Docs check, due 09:00": a task a round made says so, with the round's name as a link to the Rounds page.
fn from_round(s: &App, t: &Theme, task: &Task) -> Option<El> {
    let r = task.round.as_ref()?;
    let said = match r.due_ms {
        Some(due) => format!("From the round {}, due {}", r.name, crate::rounds::when_at(due, s.now_ms, &*s.zone())),
        None => format!("From the round {}, run by hand", r.name),
    };
    let mut line = row().key("task-round").role("note").label(said.clone()).w_full().gap(t.space.at(2.0)).center_y().min_w(0.0);
    line = line.child(glyph(&crate::icons::ROUNDS, t.control.icon_sm, t.ink_faint()).a11y_hidden());
    line = line.child(text(said.clone()).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink().a11y_hidden());
    if s.round(&r.id).is_some() {
        line = line.child(crate::ui::button::Button::new("task-round-open", "Open round").variant(rune_ui_style::ButtonVariant::Link).size(rune_ui_style::Size::Sm).on_press(|s: &mut App| s.open_rounds()).build(t));
    }
    Some(line)
}

/// The band's run counter (`turn::counter`).
pub fn run_counter(s: &App, t: &Theme) -> Option<El> {
    turn::counter(s, t)
}

/// What a run itself changed: between its checkpoints when it has them, else what its branch changed (from before undo).
fn own_changes(r: &Run) -> Option<neko_protocol::Changes> {
    match &r.undo {
        Some(u) if u.ready() => u.changes,
        _ => r.changes,
    }
    .filter(|c| c.files > 0)
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
        // The rules line stands before its run, outside its fold.
        if let PartBody::RulesChanged { .. } = p.body {
            out.push(Block::Loose(p));
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

/// Who did a run: its own cat (a task keeps one), else the task's; its name and face.
pub struct Who {
    pub name: String,
    pub face: String,
}

fn who(s: &App, r: &Run, task: &Task) -> Who {
    let cat = s.cat(r.cat.as_deref().unwrap_or(&task.cat)).or_else(|| s.cat(&task.cat));
    cat.map_or_else(|| Who { name: "Neko".to_owned(), face: String::new() }, |c| Who { name: c.name.clone(), face: c.face.clone() })
}

/// One part as drawn in a run: a step, words on the way (`muted`, or the final answer when `last`), a request, a notice.
#[allow(clippy::too_many_arguments)]
fn part_el(s: &App, t: &Theme, book: &Book, r: &Run, w: &Who, p: &Part, streaming: bool, last: bool, muted: bool) -> Option<El> {
    let key = format!("part-{}", p.seq);
    match &p.body {
        PartBody::Thinking { .. } => turn::thought(s, t, &key, p),
        PartBody::Tool { .. } => turn::step(s, t, &key, p, &r.workdir),
        PartBody::Permission { permission } => book.permissions.get(permission).map(|perm| turn::request(t, perm, &w.name, &w.face)),
        PartBody::Agent { text: words, .. } if words.trim().is_empty() => None,
        PartBody::Agent { text: words, .. } => Some(answer(t, &key, words, streaming, last, muted)),
        PartBody::Notice { .. } => notice(t, p),
        PartBody::Person { .. } | PartBody::RulesChanged { .. } => None,
    }
}

/// Consecutive steps hang on one rail; everything else stands on the reading edge.
fn group_steps(t: &Theme, tagged: Vec<(bool, El)>) -> Vec<El> {
    let mut out = Vec::new();
    let mut steps: Vec<El> = Vec::new();
    for (step, e) in tagged {
        if step {
            steps.push(e);
            continue;
        }
        if !steps.is_empty() {
            out.push(turn::rail(t, std::mem::take(&mut steps)));
        }
        out.push(e);
    }
    if !steps.is_empty() {
        out.push(turn::rail(t, steps));
    }
    out
}

/// Parts in order. Words with a step after them are interim (`muted`); `last_at` is the final answer.
fn lay_out(s: &App, t: &Theme, book: &Book, r: &Run, w: &Who, parts: &[&Part], streaming_at: Option<usize>, last_at: Option<usize>) -> Vec<El> {
    let last_step = parts.iter().rposition(|p| quiet_line(p));
    let tagged: Vec<(bool, El)> = parts
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            let interim = last_step.is_some_and(|l| i < l) && Some(i) != last_at;
            part_el(s, t, book, r, w, p, Some(i) == streaming_at, Some(i) == last_at, interim).map(|e| (quiet_line(p), e))
        })
        .collect();
    group_steps(t, tagged)
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

/// One run, as one turn of the agent's (rune-ui's rhythm: its blocks [`BLOCK_GAP`] apart). While it works: its steps and words as
/// they come, and the live line. Once it ends: one "Worked for" fold that opens to its steps on the page's own edge (`indent(false)`),
/// the final answer, notices, and the card if it earns one.
fn run_block(s: &App, t: &Theme, book: &Book, r: &Run, parts: &[&Part], task: &Task) -> El {
    let w = who(s, r, task);
    let turn = Message::assistant(format!("turn-{}", r.id.0), w.name.clone()).column(COLUMN).child(turn::turn_head(t, r, &w.name, &w.face, r.status == RunStatus::Running && s.pending_request().is_some(), offer_of(book, r).undone));
    if r.status == RunStatus::Running {
        let streaming = parts.last().is_some_and(|p| matches!(&p.body, PartBody::Agent { done: false, .. }) || matches!(&p.body, PartBody::Thinking { done: false, text } if !text.trim().is_empty()));
        let mut out = lay_out(s, t, book, r, &w, parts, if streaming { Some(parts.len() - 1) } else { None }, None);
        let waiting = parts.iter().any(|p| matches!(&p.body, PartBody::Permission { permission } if book.permissions.get(permission).is_some_and(|x| x.state == PermissionState::Pending)));
        let tool_running = parts.iter().any(|p| matches!(&p.body, PartBody::Tool { state: ToolState::Running, .. }));
        if !streaming && !waiting && !tool_running {
            let steps = parts.iter().filter(|p| matches!(p.body, PartBody::Tool { .. })).count();
            out.push(turn::live_line(t, r, &w.name, book.thinking, steps));
        }
        return out.into_iter().fold(turn, Message::child).build(t);
    }
    // The final answer is the words after the last step; notices stay in sight; everything else folds.
    let tail_from = parts.iter().rposition(|p| !matches!(p.body, PartBody::Agent { .. } | PartBody::Notice { .. })).map_or(0, |i| i + 1);
    let (work, tail) = parts.split_at(tail_from);
    let work: Vec<&Part> = work.iter().copied().filter(|p| part_el(s, t, book, r, &w, p, false, false, false).is_some() && !matches!(p.body, PartBody::Notice { .. })).collect();
    let key = format!("worked-{}", r.id.0);
    let offer = offer_of(book, r);
    let mut out = Vec::new();
    let mark = el().size(7.0, 7.0).radius(3.5).border(1.2, t.ink_faint()).a11y_hidden();
    let lead = row().size(t.control.icon, t.control.icon).center().child(mark);
    if work.is_empty() {
        // Nothing to open: one quiet line with the time.
        let words = if offer.undone { format!("{} · undone", worked_words(r)) } else { worked_words(r) };
        out.push(row().key(key).role("status").label(words.clone()).w_full().h(t.control.md).pl(t.space.at(1.5)).gap(t.metrics.gap).center_y().child(lead).child(text(words).font(t.ty.base).color(t.ink_muted()).a11y_hidden()));
    } else {
        // The same count as the band's chip: tool calls (a thought is not a step).
        let n = turn::step_count(parts.iter().copied());
        let k = key.clone();
        let mut line = FoldLine::<App>::new(key.clone(), worked_words(r), s.is_open(&key)).lead(lead).indent(false).on_toggle(move |s: &mut App| s.flip(&k));
        // An undone run's steps stay readable, marked "undone".
        let steps = (n > 0).then(|| format!("{n} step{}", if n == 1 { "" } else { "s" }));
        let meta = match (steps, offer.undone) {
            (Some(st), true) => Some(format!("{st} · undone")),
            (None, true) => Some("undone".to_owned()),
            (st, false) => st,
        };
        if let Some(m) = meta {
            line = line.meta(m);
        }
        // Inside the fold every word is interim: quieter than the answer.
        let steps = if s.is_open(&key) { vec![col().key(format!("{key}-steps")).w_full().gap(t.space.at(BLOCK_GAP)).children(lay_out_folded(s, t, book, r, &w, &work))] } else { Vec::new() };
        out.push(line.build(t, steps));
    }
    let tail_last = tail.iter().rposition(|p| matches!(&p.body, PartBody::Agent { text, .. } if !text.trim().is_empty()));
    out.extend(lay_out(s, t, book, r, &w, tail, None, tail_last));
    out.extend(turn::receipt(s, t, r, parts, &offer, own_changes(r), &w.name, &w.face));
    out.into_iter().fold(turn, Message::child).build(t)
}

/// A finished run's folded work: steps, and words that are all interim.
fn lay_out_folded(s: &App, t: &Theme, book: &Book, r: &Run, w: &Who, work: &[&Part]) -> Vec<El> {
    let tagged = work.iter().filter_map(|p| part_el(s, t, book, r, w, p, false, false, true).map(|e| (quiet_line(p), e))).collect();
    group_steps(t, tagged)
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
    if let Some(r) = s.review.as_ref().filter(|r| r.task == task.id) {
        // A page of its own: the conversation it replaces goes at once (never a fade over Review), and this one fades in on a clean page.
        return crate::view::page_swap(col().key("task-review").w_full().grow(1.0).min_h(0.0).child(crate::review_view::review_view(s, t, r)));
    }
    let empty = Book::default();
    let book = s.book().unwrap_or(&empty);
    // The task's own words first (their text keyed "task-words", as in S0).
    let mut items = vec![turn::person(s, t, "task-message", "task-words", &task.words)];
    if book.loaded && book.runs.is_empty() && book.parts.is_empty() && task.status == neko_protocol::TaskStatus::New {
        items.push(status_line(s, t, task));
    }
    for b in blocks(book) {
        match b {
            Block::Person(p) => items.push(turn::person(s, t, &format!("part-{}", p.seq), &format!("part-{}-words", p.seq), p.body.text())),
            Block::Loose(p) => items.extend(notice(t, p).or_else(|| rules_changed(t, p))),
            Block::Run(r, parts) => items.push(run_block(s, t, book, r, &parts, task)),
        }
    }
    // One reading column: the conversation's content and the composer have the same width and left edge.
    let inset = t.metrics.inset;
    let conversation = Conversation::<App>::new("transcript", s.at_bottom).column(COLUMN + 2.0 * inset).children(items).on_at_bottom(|s: &mut App, at| s.at_bottom = at).build(t);
    let working = s.open_is_working();
    // While a request waits, Escape answers it no (`lib.rs`), so the composer does not take Escape as Stop then: the answer comes
    // first, and Stop comes back as soon as the request is answered.
    let asking = working && s.pending_request().is_some();
    let reply = s.reply_box().cloned().unwrap_or_default();
    let cat = s.cat_name(task);
    let mut composer = PromptInput::new("reply", &reply, |s: &mut App| s.reply_mut())
        .placeholder(if asking {
            format!("{cat} is waiting for your answer above. Escape denies it.")
        } else if working {
            format!("{cat} is working. Press Escape to stop.")
        } else if task.status == neko_protocol::TaskStatus::Asked {
            format!("Answer {cat}'s question")
        } else {
            format!("Reply to {cat}")
        })
        .spellcheck(true)
        .phase(if working && !asking { Phase::Streaming } else { Phase::Ready })
        .on_submit(|s: &mut App| s.follow_up())
        .on_stop(|s: &mut App| s.stop());
    if let Some(ring) = context_ring(s, t, book, layer) {
        composer = composer.tool(ring);
    }
    // The composer carries the cat's portrait at its left, in the margin, so the field keeps the reading edge.
    let mut composer = col().w_full().child(composer.build(t));
    // Where the margin has room for it (not in a narrow window, where the column fills the width).
    let margin = s.width.unwrap_or(1200.0) - if s.sidebar_shown() { crate::view::SIDEBAR + 2.0 * crate::sidebar::INSET } else { 0.0 } - COLUMN;
    if margin / 2.0 >= 32.0 + t.space.at(3.0) + inset {
        composer = composer.child(crate::faces::portrait(t, "reply-face", s.cat_face(&task.cat), 32.0).absolute(-(32.0 + t.space.at(3.0)), t.space.at(2.0)));
    }
    let mut foot = col().key("reply-area").w_full().max_w(COLUMN).gap(t.space.at(2.0)).child(composer);
    if let Some(line) = crate::view::slow_line(s, t) {
        foot = foot.child(line);
    }
    if let Some(n) = &s.notice {
        foot = foot.child(text(n.clone()).key("reply-notice").role("status").font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    // Where the task came from stays in sight above the conversation (which scrolls to its newest words), not in it.
    // It has the page's own ground and a hairline under it, and is drawn above the conversation, so nothing shows through it.
    let from = from_round(s, t, task).map(|line| {
        row().key("task-round-strip").w_full().justify_center().px(inset).pt(t.space.at(2.0)).pb(t.space.at(2.0)).fill(t.c.page.bg).border_sides([0.0, 0.0, 0.0, 1.0], t.c.line).paint_order(1).child(col().w_full().max_w(COLUMN).child(line))
    });
    let mut page = col().key("task").w_full().grow(1.0).min_h(0.0).items_center();
    if let Some(strip) = from {
        page = page.child(strip);
    }
    crate::view::page_swap(
        page
            .child(col().w_full().grow(1.0).min_h(0.0).child(conversation))
            .child(row().w_full().justify_center().px(inset).pt(t.space.at(2.0)).pb(t.space.at(5.0)).child(foot)),
    )
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
