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
            // An MCP tool (a plugin's) reads as its server's name and its own: `mcp__acme-docs__find_issue` is "acme-docs · find issue".
            let named = match neko_protocol::plugins::mcp_tool(other) {
                Some((server, tool)) => format!("{server} · {}", tool.replace('_', " ")),
                None => other.to_owned(),
            };
            format!("{named} {summary}").trim().to_owned()
        }
    };
    cut_middle(words.trim(), LINE_CHARS)
}

/// Neko's own tools in a person's words: "Started Kitten 1 \u{b7} Check links", "Asked about kittens", "Stopped Kitten 2". None for
/// any other tool (and for a Neko tool this app has no words for).
fn neko_phrase(name: &str, summary: &str, input: &str, output: Option<&str>, running: bool) -> Option<String> {
    let tool = name.strip_prefix("mcp__neko__")?;
    let v: serde_json::Value = serde_json::from_str(input).unwrap_or_default();
    // The input may have been cut for the fold, so the summary the daemon made stands in.
    let title = v["title"].as_str().map(str::to_owned).unwrap_or_else(|| if tool == "start_subagent" { summary.to_owned() } else { String::new() });
    let title = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let id = v["id"].as_str().map(str::to_owned).unwrap_or_else(|| summary.to_owned());
    let id = id.trim().trim_start_matches("Subagent").trim_start_matches("Kitten").trim().to_owned();
    let with = |head: String| if title.is_empty() { head } else { format!("{head} \u{b7} {title}") };
    Some(match tool {
        "start_subagent" => {
            let draft = v["draft"].as_bool().unwrap_or(false);
            // The number the answer gave it ("Subagent 1 (...) is started").
            let number = output.and_then(|o| o.strip_prefix("Subagent ")).and_then(|r| r.split_whitespace().next()).filter(|n| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit() || c == '.')).map(str::to_owned);
            match (running, number) {
                (true, _) => with(if draft { "Saving a kitten as a draft" } else { "Starting a kitten" }.to_owned()),
                (false, Some(n)) if draft => with(format!("Saved Kitten {n} as a draft")),
                (false, Some(n)) => with(format!("Started Kitten {n}")),
                (false, None) if output.is_some_and(|o| !o.is_empty()) => with("Could not start a kitten".to_owned()),
                (false, None) => with(if draft { "Saved a kitten as a draft" } else { "Started a kitten" }.to_owned()),
            }
        }
        "start_drafts" => if running { "Starting the drafts" } else { "Started the drafts" }.to_owned(),
        "subagents" if !id.is_empty() => if running { format!("Reading Kitten {id}'s report") } else { format!("Read Kitten {id}'s report") },
        "subagents" => if running { "Asking about kittens" } else { "Asked about kittens" }.to_owned(),
        "stop_subagent" if !id.is_empty() => if running { format!("Stopping Kitten {id}") } else { format!("Stopped Kitten {id}") },
        "stop_subagent" => if running { "Stopping a kitten" } else { "Stopped a kitten" }.to_owned(),
        // Questions go up: what was asked, and an answer passed down.
        "message_subagent" if !id.is_empty() => if running { format!("Writing to Kitten {id}") } else { format!("Wrote to Kitten {id}") },
        "message_subagent" => if running { "Writing to a kitten" } else { "Wrote to a kitten" }.to_owned(),
        "ask_parent" | "ask_person" => {
            let q = v["question"].as_str().map(|q| q.split_whitespace().collect::<Vec<_>>().join(" ")).unwrap_or_default();
            let whom = if tool == "ask_parent" { "the agent" } else { "you" };
            let head = match v["subagent"].as_str().filter(|n| !n.is_empty()) {
                Some(n) => format!("Passed Kitten {n}'s question to {whom}"),
                None => format!("Asked {whom}"),
            };
            if q.is_empty() { head } else { format!("{head} \u{b7} {q}") }
        }
        _ => return None,
    })
}

/// [`phrase`] for a tool part: Neko's own tools read as a person would say them, every other tool as [`phrase`] does.
pub fn step_phrase(name: &str, summary: &str, input: &str, output: Option<&str>, running: bool, workdir: &str, home: Option<&str>) -> String {
    match neko_phrase(name, summary, input, output, running) {
        Some(p) => cut_middle(p.trim(), LINE_CHARS),
        None => phrase(name, summary, input, running, workdir, home),
    }
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
            .key(kk(format!("notice-{}", p.seq)))
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
            .key(kk(format!("rules-changed-{}", p.seq)))
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
    // Words the person steered into a run while it worked (stored with no run, at a time the run was going) stand after the run's
    // block, not inside it: a run is one block, with one fold and one key.
    let mut held: Vec<&'a Part> = Vec::new();
    let inside = |p: &Part| book.runs.iter().any(|r| r.started_ms <= p.at_ms && r.ended_ms.is_none_or(|e| p.at_ms <= e));
    for p in &book.parts {
        if let PartBody::Person { .. } = p.body {
            if p.run.is_none() && inside(p) {
                held.push(p);
            } else {
                out.extend(held.drain(..).map(Block::Person));
                out.push(Block::Person(p));
            }
            continue;
        }
        let same_run = matches!((out.last(), p.run.as_ref()), (Some(Block::Run(r, _)), Some(id)) if &r.id == id);
        if !same_run {
            out.extend(held.drain(..).map(Block::Person));
        }
        // The rules line and the note that a report woke the thread stand before their run, outside its fold.
        if let PartBody::RulesChanged { .. } | PartBody::Wake { .. } = p.body {
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
    out.extend(held.drain(..).map(Block::Person));
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

/// Who did a run, in words. A thread's agent has no name or face (docs/knowledge/threads.md): `name` is empty, and what needs a
/// subject says "the agent". A subagent has its number.
pub struct Who {
    pub name: String,
}

impl Who {
    /// For a sentence's subject: "The agent", or the subagent's number.
    pub fn subject(&self) -> String {
        if self.name.is_empty() { "The agent".to_owned() } else { self.name.clone() }
    }
}

fn who(task: &Task) -> Who {
    Who { name: task.subagent_name().unwrap_or_default() }
}

/// One part as drawn in a run: a step, words on the way (`muted`, or the final answer when `last`), a request, a notice.
#[allow(clippy::too_many_arguments)]
fn part_el(s: &App, t: &Theme, book: &Book, r: &Run, w: &Who, p: &Part, streaming: bool, last: bool, muted: bool) -> Option<El> {
    let key = kk(format!("part-{}", p.seq));
    match &p.body {
        PartBody::Thinking { .. } => turn::thought(s, t, &key, p),
        PartBody::Tool { .. } => turn::step(s, t, &key, p, &r.workdir),
        PartBody::Permission { permission } => book.permissions.get(permission).map(|perm| turn::request(t, perm, &w.subject())),
        PartBody::Agent { text: words, .. } if words.trim().is_empty() => None,
        PartBody::Agent { text: words, .. } => Some(answer(t, &key, words, streaming, last, muted)),
        PartBody::Notice { .. } => notice(t, p),
        PartBody::ToSubagent { .. } => crate::subagents_view::to_kitten_note(s, t, p, &w.subject()),
        PartBody::Person { .. } | PartBody::RulesChanged { .. } | PartBody::Subagent { .. } | PartBody::Wake { .. } => None,
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
    let one = |i: usize, p: &Part| {
        let interim = last_step.is_some_and(|l| i < l) && Some(i) != last_at;
        part_el(s, t, book, r, w, p, Some(i) == streaming_at, Some(i) == last_at, interim).map(|e| (quiet_line(p), e))
    };
    let mut tagged: Vec<(bool, El)> = Vec::new();
    let mut i = 0;
    while i < parts.len() {
        let p = parts[i];
        if !matches!(p.body, PartBody::Subagent { .. }) {
            tagged.extend(one(i, p));
            i += 1;
            continue;
        }
        // Kittens started one after another are one card with a row each; only Neko's own tool steps (the calls that started
        // them) may stand between them. Those steps follow the card.
        let (mut group, mut last, mut j) = (vec![p], i, i + 1);
        while j < parts.len() {
            match &parts[j].body {
                PartBody::Subagent { .. } => {
                    group.push(parts[j]);
                    last = j;
                }
                PartBody::Tool { name, .. } if name.starts_with("mcp__neko__") => {}
                _ => break,
            }
            j += 1;
        }
        if let Some(card) = crate::subagents_view::card_stack(s, t, &group) {
            tagged.push((false, card));
        }
        for k in (i + 1..last).filter(|&k| !matches!(parts[k].body, PartBody::Subagent { .. })) {
            tagged.extend(one(k, parts[k]));
        }
        j = last + 1;
        i = j;
    }
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
    let w = who(task);
    // No turn has a head (a portrait and "done"): the words are the agent's, as in any agent app. What a run is doing is its live
    // line, then its fold; what a thread rests on is the line that ends the conversation.
    let turn = Message::assistant(format!("turn-{}", r.id.0), w.subject()).column(COLUMN);
    if r.status == RunStatus::Running {
        let streaming = parts.last().is_some_and(|p| matches!(&p.body, PartBody::Agent { done: false, .. }) || matches!(&p.body, PartBody::Thinking { done: false, text } if !text.trim().is_empty()));
        let mut out = lay_out(s, t, book, r, &w, parts, if streaming { Some(parts.len() - 1) } else { None }, None);
        let waiting = parts.iter().any(|p| matches!(&p.body, PartBody::Permission { permission } if book.permissions.get(permission).is_some_and(|x| x.state == PermissionState::Pending)));
        let tool_running = parts.iter().any(|p| matches!(&p.body, PartBody::Tool { state: ToolState::Running, .. }));
        if !streaming && !waiting && !tool_running {
            let steps = parts.iter().filter(|p| matches!(p.body, PartBody::Tool { .. })).count();
            out.push(turn::live_line(t, r, &w.subject(), book.thinking, steps));
        }
        return out.into_iter().fold(turn, Message::child).build(t);
    }
    // The final answer is the words after the last step; notices stay in sight; everything else folds.
    let tail_from = parts.iter().rposition(|p| !matches!(p.body, PartBody::Agent { .. } | PartBody::Notice { .. })).map_or(0, |i| i + 1);
    let (work, tail) = parts.split_at(tail_from);
    let work: Vec<&Part> = work.iter().copied().filter(|p| part_el(s, t, book, r, &w, p, false, false, false).is_some() && !matches!(p.body, PartBody::Notice { .. } | PartBody::ToSubagent { .. })).collect();
    let key = format!("worked-{}", r.id.0);
    let offer = offer_of(book, r);
    // The newest run of a thread that rests on its subagents says so on its own line, never "done": the thread is not.
    let resting = book.runs.last().is_some_and(|x| x.id == r.id).then(|| crate::subagents_view::waiting_words(s, task)).flatten();
    let mut out = Vec::new();
    let lead = crate::ui::agent::hollow_dot(t, t.control.icon);
    if work.is_empty() {
        // Nothing to open: one quiet line with the time.
        let mut words = if offer.undone { format!("{} · undone", worked_words(r)) } else { worked_words(r) };
        if let Some(w) = &resting {
            words = format!("{words} · {w}");
        }
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
        let meta = match (meta, &resting) {
            (Some(m), Some(w)) => Some(format!("{m} · {w}")),
            (None, Some(w)) => Some(w.clone()),
            (m, None) => m,
        };
        if let Some(m) = meta {
            line = line.meta(m);
        }
        // Inside the fold every word is interim: quieter than the answer.
        let steps = if s.is_open(&key) { vec![col().key(format!("{key}-steps")).w_full().gap(t.space.at(BLOCK_GAP)).children(lay_out_folded(s, t, book, r, &w, &work))] } else { Vec::new() };
        out.push(line.build(t, steps));
    }
    // The subagents it started stay in sight, as one card, outside the fold.
    let started: Vec<&Part> = parts.iter().copied().filter(|p| matches!(p.body, PartBody::Subagent { .. })).collect();
    out.extend(crate::subagents_view::card_stack(s, t, &started));
    // And what it wrote to them, each a quiet note: "The agent answered Kitten 1".
    out.extend(parts.iter().filter_map(|p| crate::subagents_view::to_kitten_note(s, t, p, &w.subject())));
    let tail_last = tail.iter().rposition(|p| matches!(&p.body, PartBody::Agent { text, .. } if !text.trim().is_empty()));
    out.extend(lay_out(s, t, book, r, &w, tail, None, tail_last));
    out.extend(turn::change_line(t, r, &offer, own_changes(r), task.subagent.is_some()));
    out.into_iter().fold(turn, Message::child).build(t)
}

/// Whether a part of a finished run is in the run's closed fold: words before the run's last step are interim and fold with the steps;
/// the words after it are the answer and stay on the page.
pub fn in_fold(book: &Book, p: &Part) -> bool {
    let Some(run) = &p.run else { return false };
    if book.runs.iter().find(|r| &r.id == run).is_none_or(|r| r.status == RunStatus::Running) {
        return false;
    }
    let parts: Vec<&Part> = book.parts.iter().filter(|x| x.run.as_ref() == Some(run) && !matches!(x.body, PartBody::Person { .. })).collect();
    let tail_from = parts.iter().rposition(|p| !matches!(p.body, PartBody::Agent { .. } | PartBody::Notice { .. })).map_or(0, |i| i + 1);
    parts.iter().position(|x| x.id == p.id).is_some_and(|i| i < tail_from)
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
    // The ring sits at the composer's left, so its breakdown opens from the trigger's start (the default hangs out over the sidebar);
    // a window the model did not report says so.
    let mut ring = Context::<App>::new("ctx", &meter, s.context_open).placement(rune_kit::Placement::TopStart).limit_assumed(u.window_assumed).on_open(|s: &mut App, open| s.context_open = open);
    if let Some(n) = cost_note(&u) {
        ring = ring.note(n);
    }
    Some(ring.build(t, layer))
}

thread_local! {
    /// The conversation being built is a kitten's tab in the right pane, beside its thread's own: every key that is not unique to the
    /// task (a part's place in its transcript) takes a prefix, so no two elements of the window share one.
    static IN_PANE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// A key that is only unique within one task's transcript: prefixed when the conversation is in the right pane.
pub fn kk(key: String) -> String {
    if IN_PANE.with(std::cell::Cell::get) { format!("pane-{key}") } else { key }
}

/// A task's conversation as the blocks of one column: the person's words, then each run, note and card in order, and what the thread
/// waits on at the end.
fn items_of(s: &App, t: &Theme, task: &Task, book: &Book) -> Vec<El> {
    // The task's own words first (their text keyed "task-words", as in S0).
    let mut items = vec![turn::person(s, t, "task-message", "task-words", &task.words)];
    if book.loaded && book.runs.is_empty() && book.parts.is_empty() && task.status == neko_protocol::TaskStatus::New && task.subagent.is_none() {
        items.push(status_line(s, t, task));
    }
    for b in blocks(book) {
        match b {
            // In a kitten's conversation every message after its brief is its parent's (an answer, or more to do).
            Block::Person(p) if task.subagent.is_some() => items.push(turn::person_from(s, t, &format!("part-{}", p.seq), &format!("part-{}-words", p.seq), p.body.text(), &crate::subagents::asked_whom(s.parent_of(task)))),
            Block::Person(p) => items.push(turn::person(s, t, &format!("part-{}", p.seq), &format!("part-{}-words", p.seq), p.body.text())),
            Block::Loose(p) => items.extend(notice(t, p).or_else(|| rules_changed(t, p)).or_else(|| crate::subagents_view::wake_note(s, t, p)).or_else(|| crate::subagents_view::card_stack(s, t, &[p]))),
            Block::Run(r, parts) => items.push(run_block(s, t, book, r, &parts, task)),
        }
    }
    // A thread resting on its subagents ends on what it waits for; one that asked, on its question.
    items.extend(crate::subagents_view::waiting_line(s, t, task));
    items.extend(crate::subagents_view::question_block(s, t, task, book.runs.last()));
    items
}

/// A kitten's conversation for its tab in the right pane: the same blocks as the main view, with keys of their own.
pub fn kitten_items(s: &App, t: &Theme, task: &Task, book: &Book) -> Vec<El> {
    IN_PANE.with(|c| c.set(true));
    let mut items = items_of(s, t, task, book);
    IN_PANE.with(|c| c.set(false));
    // Its first words are the brief, which the tab folds above: not said twice.
    if !items.is_empty() {
        items.remove(0);
    }
    items
}

/// The open task: its conversation, and its composer.
pub fn task_view(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let Some(task) = s.open_task() else { return el() };
    let empty = Book::default();
    let book = s.book().unwrap_or(&empty);
    let items = items_of(s, t, task, book);
    // One reading column: the conversation's content and the composer have the same width and left edge.
    let inset = t.metrics.inset;
    let conversation = Conversation::<App>::new("transcript", s.at_bottom).column(COLUMN + 2.0 * inset).children(items).on_at_bottom(|s: &mut App, at| s.at_bottom = at).build(t);
    let working = s.open_is_working();
    // While a request waits, Escape answers it no (`lib.rs`), so the composer does not take Escape as Stop then: the answer comes
    // first, and Stop comes back as soon as the request is answered.
    let asking = working && s.pending_request().is_some();
    let reply = s.reply_box().cloned().unwrap_or_default();
    // While the agent works the button is Stop for an empty box and Send, in the mode Return uses, for words (`composer.rs`).
    let has_words = !reply.text.trim().is_empty();
    let mut composer = PromptInput::new("reply", &reply, |s: &mut App| s.reply_mut())
        .placeholder(crate::composer::placeholder(s, task, asking))
        .spellcheck(true)
        .phase(if working && !asking && !has_words { Phase::Streaming } else { Phase::Ready })
        .on_submit(|s: &mut App| s.follow_up())
        .on_stop(|s: &mut App| s.stop());
    if let Some(ring) = context_ring(s, t, book, layer) {
        composer = composer.tool(ring);
    }
    if let Some(chip) = crate::composer::mode_chip(s, t, layer) {
        composer = composer.tool(chip);
    }
    // The composer stands alone: who will answer is already in the band and in its placeholder ("Reply to miso"). The keys that choose
    // how words reach a working agent are read by the wrapper, before the text field sees them.
    let composer = col().key("reply-block").w_full().on_key_preview(crate::composer::key_preview).child(composer.build(t));
    let mut foot = col().key("reply-area").w_full().max_w(COLUMN).gap(t.space.at(2.0));
    // A subagent is read, not talked to: no composer, a quiet line and the way back (and Stop while it works).
    if task.subagent.is_some() {
        foot = foot.child(crate::subagents_view::readonly_foot(s, t, task));
    } else {
        // Above the box: what waits to be read or sent, then the one thin line of what still runs.
        if let Some(w) = crate::composer::waiting(s, t) {
            foot = foot.child(w);
        }
        if let Some(bar) = crate::composer::activity_bar(s, t) {
            foot = foot.child(bar);
        }
        foot = foot.child(composer);
        if let Some(h) = crate::composer::key_hints(s, t) {
            foot = foot.child(h);
        }
    }
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
        .label(format!("The agent has not started yet. Added {added}."))
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .justify_end()
        .pr(t.space.at(2.0))
        .child(glyph(&icons::CIRCLE, t.control.icon_sm, t.ink_faint()))
        .child(text("The agent has not started yet").font(t.ty.sm).color(t.ink_muted()).a11y_hidden())
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
        assert_eq!(phrase("mcp__acme__find_issue", "ENG-123", "{}", false, w, home), "acme · find issue ENG-123");
        assert_eq!(phrase("mcp__acme-docs__search", "", "{}", false, w, home), "acme-docs · search");
        assert_eq!(show_path("/Users/examplex/a", w, home), "/Users/examplex/a", "a sibling of home is not home");
        // Neko's own tools read as words, not as a server and a tool name.
        let start = "{\n  \"title\": \"Check links\"\n}";
        let said = "Subagent 1 (\"Check links\") is started. It is working now.";
        assert_eq!(step_phrase("mcp__neko__start_subagent", "Check links", start, Some(said), false, w, home), "Started Kitten 1 \u{b7} Check links");
        assert_eq!(step_phrase("mcp__neko__start_subagent", "Check links", start, None, true, w, home), "Starting a kitten \u{b7} Check links");
        assert_eq!(step_phrase("mcp__neko__start_subagent", "Check links", "{\n  \"prompt\": \"A long brief that was cut", Some("Subagent 1.2 (\"Check links\") is started."), false, w, home), "Started Kitten 1.2 \u{b7} Check links", "a cut input: the summary stands in");
        let draft = "{\"draft\": true, \"title\": \"Later\"}";
        assert_eq!(step_phrase("mcp__neko__start_subagent", "Later", draft, Some("Subagent 3 (\"Later\") is saved as a draft."), false, w, home), "Saved Kitten 3 as a draft \u{b7} Later");
        assert_eq!(step_phrase("mcp__neko__start_subagent", "Later", "{}", Some("Placement own_copy needs the project to be a git repository."), false, w, home), "Could not start a kitten \u{b7} Later");
        assert_eq!(step_phrase("mcp__neko__subagents", "", "{}", Some("Subagent 1 \u{b7} x"), false, w, home), "Asked about kittens");
        assert_eq!(step_phrase("mcp__neko__subagents", "2", "{\"id\": \"2\"}", None, false, w, home), "Read Kitten 2's report");
        assert_eq!(step_phrase("mcp__neko__stop_subagent", "2", "{\"id\": \"Subagent 2\"}", None, false, w, home), "Stopped Kitten 2");
        assert_eq!(step_phrase("mcp__neko__start_drafts", "", "{}", None, false, w, home), "Started the drafts");
        assert_eq!(step_phrase("mcp__acme__find_issue", "ENG-123", "{}", None, false, w, home), "acme \u{b7} find issue ENG-123", "a plugin keeps its own reading");
        let long = phrase("Bash", "", &format!("git status --short && git log --oneline -5 --decorate && {}", "x ".repeat(40)), false, w, home);
        assert_eq!(long.chars().count(), LINE_CHARS);
        assert!(long.starts_with("git status --short") && long.contains('…'), "{long}");
    }
}
