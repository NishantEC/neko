//! The Task view (docs/knowledge/app-shell.md, "The Task view"): the conversation with the agent, and the composer under it.
//!
//! The answer is the page: the agent's words sit on it with no bubble. Thinking and tool calls are quiet single lines that fold.
//! While a run works its steps show as they happen; once it ends they fold behind one "Worked for 2m 14s" line and its final
//! answer stays open. A run card shows only when the run changed files, has a branch or failed. Everything shares one reading
//! column: one left edge and one width. Nothing about money shows until the person opens the context ring in the composer.
//! Colour is for state only.

use std::rc::Rc;

use neko_protocol::{Part, PartBody, Permission, PermissionState, Pricing, Run, RunStatus, Task, Tone as NoticeTone, ToolState};
use rune::{El, Key, KeyEvent, Tx, col, el, row, text};
use rune_kit::{Markdown, Placement};
use rune_ui_style::{Recipe, tabular, weight};

use crate::ui::base::Shape;

use crate::state::{App, Book};
use crate::ui::agent::{FoldLine, PressTx, Status, caption, elapsed, icon_button, mark, mono, pre, return_presses, slab, tokens};
use crate::ui::badge::Badge;
use crate::ui::base::{ButtonVariant, Theme, Tone, glyph, paint};
use crate::ui::button::Button;
use crate::ui::confirmation::{Confirmation, Decision};
use crate::ui::context::{Context, Usage as Meter};
use crate::ui::conversation::Conversation;
use crate::ui::icons;
use crate::ui::message::{self, Message};
use crate::ui::prompt_input::{Phase, PromptInput};
use crate::ui::reasoning::Reasoning;
use crate::ui::response::{CARET, Response};
use crate::ui::shimmer::Shimmer;
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

/// The person's words, at the right; their copy button shows while the pointer is over them or the keyboard is in them.
fn person(t: &Theme, key: &str, words_key: &str, words: &str) -> El {
    let said = message::words(t, words.to_owned()).key(words_key.to_owned()).selectable(true);
    Message::user(key.to_owned(), "You").child(said).actions(row().child(copy(t, key, words.to_owned()).group_opacity(0.0, 1.0))).column(COLUMN).build(t).hover_group()
}

/// The agent's words as Markdown on the page, selectable. Built on rune-ui's `Response` look; Neko builds the Markdown itself only
/// to make it selectable (rune-ui's `Response` has no such switch yet). Only a run's final answer has a copy button.
fn answer(t: &Theme, key: &str, words: &str, streaming: bool, last: bool) -> El {
    let mut src = words.to_owned();
    if streaming {
        if !src.is_empty() && !src.ends_with(char::is_whitespace) {
            src.push(' ');
        }
        src.push_str(CARET);
    }
    let md = Markdown::new(src).streaming(streaming).reveal(if streaming && !t.reduced_motion { 0.18 } else { 0.0 }).selectable(!streaming).style(Response::style(t, COLUMN)).build(t.to_kit());
    let body = col().key(format!("{key}-answer")).role("document").label("Response").busy(streaming).w_full().min_w(0.0).child(md);
    let mut m = Message::assistant(key.to_owned(), AGENT).child(body).column(COLUMN);
    if last && !streaming {
        m = m.actions(row().child(copy(t, key, words.to_owned())));
    }
    m.build(t)
}

fn secs(p: &Part) -> f64 {
    p.ended_ms.map_or(0.0, |e| (e - p.at_ms).max(0) as f64 / 1000.0)
}

/// A duration worth showing: none under a second.
fn took(secs: f64) -> Option<String> {
    (secs >= 1.0).then(|| elapsed(secs.round()))
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

/// A tool call: its phrase, its mark, and quiet figures (a duration from a second up, a command's exit code, "Denied").
fn tool_line(s: &App, t: &Theme, key: &str, p: &Part, workdir: &str) -> Option<El> {
    let PartBody::Tool { name, summary, input, output, state, exit, .. } = &p.body else { return None };
    let status = match state {
        ToolState::Running => Status::Running,
        ToolState::Done => Status::Done,
        ToolState::Failed => Status::Error,
        ToolState::Denied => Status::Skipped,
    };
    let label = phrase(name, summary, input, *state == ToolState::Running, workdir, s.home.as_deref());
    let mut line = FoldLine::<App>::new(key.to_owned(), label, s.is_open(key)).lead(mark(t, status, t.control.icon));
    match (state, exit) {
        (ToolState::Denied, _) => line = line.meta("Denied"),
        (ToolState::Done, Some(code)) => line = line.meta(format!("exit {code}")),
        _ => {}
    }
    if *state != ToolState::Running
        && let Some(d) = p.ended_ms.and_then(|_| took(secs(p)))
    {
        line = line.meta(d);
    }
    let k = key.to_owned();
    line = line.on_toggle(move |s: &mut App| s.flip(&k));
    let mut body = vec![caption(t, if name == "Bash" { "Command" } else { "Input" }), pre(t, &format!("{key}-input"), input, t.ink(), 200.0)];
    match (state, output) {
        (ToolState::Failed, o) => {
            body.push(caption(t, "Error"));
            body.push(pre(t, &format!("{key}-error"), o.as_deref().unwrap_or("It did not finish."), t.c.danger.fg, 200.0));
        }
        (_, Some(o)) if !o.is_empty() => {
            body.push(caption(t, if *state == ToolState::Denied { "Why" } else { "Output" }));
            body.push(pre(t, &format!("{key}-output"), o, t.ink_muted(), 240.0));
        }
        _ => {}
    }
    Some(line.build(t, body))
}

/// A thought: nothing when it holds no words (Claude Code sends none today); "Thought for 12s" folding to its words otherwise.
fn thought(s: &App, t: &Theme, key: &str, p: &Part) -> Option<El> {
    let PartBody::Thinking { text: words, done } = &p.body else { return None };
    if words.trim().is_empty() {
        return None;
    }
    let k = key.to_owned();
    if !*done {
        return Some(Reasoning::new(key.to_owned(), true, secs(p), s.is_open(key)).text(words.clone()).on_toggle(move |s: &mut App| s.flip(&k)).build(t));
    }
    let label = took(secs(p)).map_or_else(|| "Thought".to_owned(), |d| format!("Thought for {d}"));
    let body = vec![text(words.clone()).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).selectable(true)];
    Some(FoldLine::<App>::new(key.to_owned(), label, s.is_open(key)).lead(glyph(&icons::SPARKLE, t.control.icon, t.ink_faint())).on_toggle(move |s: &mut App| s.flip(&k)).build(t, body))
}

/// A permission request: waiting, it asks; answered, it is one quiet line.
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
    if p.destructive {
        return destructive(t, &key, p);
    }
    let id = p.id.clone();
    let panel = Confirmation::<App>::new(key.clone(), p.title.clone(), decision)
        .detail(p.detail.clone())
        .on_decide(move |s: &mut App, d| {
            let answer = match d {
                Decision::AllowedOnce => neko_protocol::Decision::AllowOnce,
                Decision::AlwaysAllowed => neko_protocol::Decision::AlwaysAllow,
                _ => neko_protocol::Decision::Deny,
            };
            s.decide(&id, answer);
        })
        .build(t)
        .description(format!("{} Allow once, always allow for this task, or deny. Escape denies.", p.why));
    // Why Neko asks, under the request in the quiet ink (rune-ui's Confirmation has no place for it yet).
    col().key(format!("{key}-wrap")).w_full().gap(t.space.at(2.0)).child(panel).child(text(format!("Neko asks because {}", lower_first(&p.why))).key(format!("{key}-why")).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0).a11y_hidden())
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// A request that deletes or overwrites: no default answer and no "always" (rune-ui's Confirmation always offers both, so Neko draws
/// this one itself in the same look).
fn destructive(t: &Theme, key: &str, p: &Permission) -> El {
    let (allow, deny) = (p.id.clone(), p.id.clone());
    let answer = |suffix: &str, words: &str, d: neko_protocol::Decision, id: neko_protocol::PermissionId| {
        let id2 = id.clone();
        row()
            .on_key(move |s: &mut App, k: &KeyEvent, _: &mut Tx| return_presses(k, || s.decide(&id2, d)))
            .child(Button::<App>::new(format!("{key}-{suffix}"), words).variant(ButtonVariant::Secondary).on_press(move |s: &mut App| s.decide(&id, d)).build(t))
    };
    let head = row().w_full().gap(t.space.at(2.0)).center_y().child(glyph(&icons::SHIELD, t.control.icon, t.c.danger.fg)).child(text("Needs you").font(weight(t.ty.sm, t.ty.medium)).color(t.c.danger.fg));
    let body = col()
        .w_full()
        .padding(t.metrics.pad_md)
        .gap(t.space.at(3.0))
        .child(head)
        .child(text(p.title.clone()).font(weight(t.ty.md, t.ty.medium)).color(t.ink()).wrap().min_w(0.0))
        .child(text(p.why.clone()).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0))
        .child(pre(t, &format!("{key}-detail"), &p.detail, t.ink(), 160.0))
        .child(row().key(format!("{key}-answers")).gap(t.space.at(2.0)).flex_wrap().child(answer("allow", "Allow once", neko_protocol::Decision::AllowOnce, allow)).child(answer("deny", "Deny", neko_protocol::Decision::Deny, deny.clone())));
    slab(t, key.to_owned())
        .w_full()
        .role("alertdialog")
        .label(format!("Permission: {}", p.title))
        .description(format!("{} Allow once or deny. Escape denies.", p.why))
        .child(body)
        .on_key(move |s: &mut App, k: &KeyEvent, _: &mut Tx| {
            if k.key == Key::Escape {
                s.decide(&deny, neko_protocol::Decision::Deny);
                true
            } else {
                false
            }
        })
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

/// The run card, only when a run has something to show beyond time: files changed, a branch, or a failure. No money here.
fn run_card(t: &Theme, r: &Run) -> Option<El> {
    let changed = r.changes.filter(|c| c.files > 0);
    let failed = r.status == RunStatus::Failed;
    if changed.is_none() && r.branch.is_none() && !failed {
        return None;
    }
    let (word, tone) = match r.status {
        RunStatus::Running => ("Working", Tone::Info),
        RunStatus::Done => ("Done", Tone::Success),
        RunStatus::Failed => ("Failed", Tone::Danger),
        RunStatus::Stopped => ("Stopped", Tone::Neutral),
        RunStatus::Interrupted => ("Interrupted", Tone::Neutral),
    };
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
        facts = facts.child(fact(&icons::COMMIT, figures(b.clone())));
    }
    let head = row().w_full().gap(t.space.at(3.0)).center_y().child(Badge::new(format!("run-{}-status", r.id.0), word).tone(tone).dot().build(t)).child(facts);
    Some(paint(col().key(format!("run-{}", r.id.0)).role("group").label(format!("Run: {word}")).w_full().padding(t.metrics.inset).gap(t.space.at(3.0)).radius(t.radius.xl).round_as(t), &t.mat.slab.base).child(head))
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

/// Thinking and tool calls: quiet lines that sit together, closer than the words around them.
fn quiet_line(p: &Part) -> bool {
    matches!(p.body, PartBody::Thinking { .. } | PartBody::Tool { .. })
}

/// One part as drawn in a run: a step, words on the way (or the final answer when `last`), a request, a notice.
fn part_el(s: &App, t: &Theme, book: &Book, r: &Run, p: &Part, streaming: bool, last: bool) -> Option<El> {
    let key = format!("part-{}", p.seq);
    match &p.body {
        PartBody::Thinking { .. } => thought(s, t, &key, p),
        PartBody::Tool { .. } => tool_line(s, t, &key, p, &r.workdir),
        PartBody::Permission { permission } => book.permissions.get(permission).map(|perm| request(t, perm)),
        PartBody::Agent { text: words, .. } if words.trim().is_empty() => None,
        PartBody::Agent { text: words, .. } => Some(answer(t, &key, words, streaming, last)),
        PartBody::Notice { .. } => notice(t, p),
        PartBody::Person { .. } => None,
    }
}

/// Parts in order, the quiet lines gathered into tight groups.
fn lay_out(s: &App, t: &Theme, book: &Book, r: &Run, parts: &[&Part], streaming_at: Option<usize>, last_at: Option<usize>) -> Vec<El> {
    let mut out = Vec::new();
    let mut lines: Vec<El> = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        let Some(e) = part_el(s, t, book, r, p, Some(i) == streaming_at, Some(i) == last_at) else { continue };
        if quiet_line(p) {
            lines.push(e);
        } else {
            if !lines.is_empty() {
                out.push(col().w_full().gap(t.space.at(0.5)).children(std::mem::take(&mut lines)));
            }
            out.push(e);
        }
    }
    if !lines.is_empty() {
        out.push(col().w_full().gap(t.space.at(0.5)).children(lines));
    }
    out
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

/// One run. While it works: its steps and words as they come, and the live line. Once it ends: one "Worked for" line that the
/// person can open (its steps and words then follow it, on the same edge), the final answer, notices, and the card if it earns one.
fn run_block(s: &App, t: &Theme, book: &Book, r: &Run, parts: &[&Part]) -> Vec<El> {
    let last_words = parts.iter().rposition(|p| matches!(&p.body, PartBody::Agent { text, .. } if !text.trim().is_empty()));
    if r.status == RunStatus::Running {
        let streaming = parts.last().is_some_and(|p| matches!(&p.body, PartBody::Agent { done: false, .. }) || matches!(&p.body, PartBody::Thinking { done: false, text } if !text.trim().is_empty()));
        let mut out = lay_out(s, t, book, r, parts, if streaming { Some(parts.len() - 1) } else { None }, None);
        let waiting = parts.iter().any(|p| matches!(&p.body, PartBody::Permission { permission } if book.permissions.get(permission).is_some_and(|x| x.state == PermissionState::Pending)));
        let tool_running = parts.iter().any(|p| matches!(&p.body, PartBody::Tool { state: ToolState::Running, .. }));
        if !streaming && !waiting && !tool_running {
            out.push(live_line(t, r, book.thinking));
        }
        return out;
    }
    // The final answer is the words after the last step; notices stay in sight; everything else folds.
    let tail_from = parts.iter().rposition(|p| !matches!(p.body, PartBody::Agent { .. } | PartBody::Notice { .. })).map_or(0, |i| i + 1);
    let (work, tail) = parts.split_at(tail_from);
    let work: Vec<&Part> = work.iter().copied().filter(|p| part_el(s, t, book, r, p, false, false).is_some() && !matches!(p.body, PartBody::Notice { .. })).collect();
    let key = format!("worked-{}", r.id.0);
    let mut out = Vec::new();
    let lead = row().size(t.control.icon, t.control.icon).center().child(glyph(&icons::CLOCK, t.control.icon, t.ink_faint()));
    if work.is_empty() {
        // Nothing to open: one quiet line with the time.
        out.push(row().key(key).role("status").label(worked_words(r)).w_full().h(t.control.md).pl(t.space.at(1.5)).gap(t.metrics.gap).center_y().child(lead).child(text(worked_words(r)).font(t.ty.base).color(t.ink_muted()).a11y_hidden()));
    } else {
        let n = work.iter().filter(|p| quiet_line(p)).count();
        let k = key.clone();
        let mut line = FoldLine::<App>::new(key.clone(), worked_words(r), s.is_open(&key)).lead(lead).on_toggle(move |s: &mut App| s.flip(&k));
        if n > 0 {
            line = line.meta(format!("{n} step{}", if n == 1 { "" } else { "s" }));
        }
        out.push(line.build(t, Vec::new()));
        if s.is_open(&key) {
            out.push(col().key(format!("{key}-steps")).w_full().gap(t.space.at(4.0)).children(lay_out(s, t, book, r, &work, None, None)));
        }
    }
    let tail_last = tail.iter().rposition(|p| matches!(&p.body, PartBody::Agent { text, .. } if !text.trim().is_empty()));
    let _ = last_words;
    out.extend(lay_out(s, t, book, r, tail, None, tail_last));
    out.extend(run_card(t, r));
    out
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

/// Money in dollars, as rune-ui writes it.
fn money(usd: f64) -> String {
    crate::ui::agent::money(usd)
}

/// The context ring's popover: how full the context is, the task's tokens, and the cost as reported with how to read it. The only
/// place money shows. (rune-ui's Context draws a cost with no reading line and no "not reported"; Neko draws this panel itself.)
fn context_panel(t: &Theme, u: &TaskUsage, layer: &mut Vec<El>) {
    let trigger = "ctx-trigger";
    let share = if u.window == 0 { 0.0 } else { (u.context as f32 / u.window as f32).clamp(0.0, 1.0) };
    let line = |key: &str, name: &str, value: String| row().key(format!("ctx-{key}")).w_full().h(t.space.at(6.0)).center_y().child(text(name.to_owned()).font(t.ty.sm).color(t.ink_muted()).grow(1.0)).child(text(value).font(tabular(t.ty.sm)).color(t.c.floating.fg));
    let ink = if share > 0.95 { t.c.danger.solid } else if share > 0.8 { t.c.warning.solid } else { t.c.primary.bg };
    let bar = paint(row().w_full().h(6.0).radius(3.0), &t.mat.well.base).child(paint(el().w_frac(share).h(6.0).radius(3.0), &Recipe::new().fill(ink)));
    let of = format!("{} of {}{}", tokens(u.context), tokens(u.window), if u.window_assumed { " (window assumed)" } else { "" });
    let cost = u.cost.map_or_else(|| "Not reported".to_owned(), money);
    let how = match (u.cost, u.pricing) {
        (None, _) => format!("{} reported no cost.", u.reported_by),
        (Some(_), Pricing::ApiEquivalent) => format!("API-equivalent, as {} reports it. Not charged on a subscription.", u.reported_by),
        (Some(_), Pricing::Charged) => format!("As {} reports it.", u.reported_by),
        (Some(_), Pricing::Unknown) => format!("As {} reports it. Neko cannot tell whether it is charged.", u.reported_by),
    };
    let close = |s: &mut App| s.context_open = false;
    let panel = paint(col().key("ctx-panel").role("dialog").label("Context and usage").w(280.0).padding(t.metrics.pad_md - 2.0).gap(t.space.at(1.0)).radius(t.radius.lg).round_as(t), &t.mat.floating.base)
        .child(row().w_full().items_end().child(text("Context").font(weight(t.ty.base, t.ty.semibold)).color(t.c.floating.fg).grow(1.0)).child(text(format!("{:.0}%", share * 100.0)).font(tabular(t.ty.sm)).color(t.ink_muted())))
        .child(row().w_full().py(t.space.at(1.5)).child(bar))
        .child(text(of).key("ctx-used").font(tabular(t.ty.sm)).color(t.ink_muted()))
        .child(el().w_full().h(1.0).fill(t.c.line).a11y_hidden())
        .child(line("in", "Tokens in", tokens(u.input)))
        .child(line("out", "Tokens out", tokens(u.output)))
        .child(line("cached", "Cached", tokens(u.cached)))
        .child(el().w_full().h(1.0).fill(t.c.line).a11y_hidden())
        .child(line("cost", "Cost", cost))
        .child(text(how).key("ctx-how").font(t.ty.xs).color(t.ink_faint()).wrap().min_w(0.0))
        .focusable(true)
        .focus_return(trigger)
        .modal(true, close);
    layer.push(rune_kit::scrim("ctx-scrim", close));
    layer.push(rune_kit::popover::anchored(t.to_kit(), panel, trigger, Placement::TopStart, t.space.at(1.5)));
}

/// The context ring in the composer's footer: rune-ui's Context, showing how full the conversation's context is. Pressing it opens
/// Neko's own panel. None before any run reported usage.
fn context_ring(s: &App, t: &Theme, book: &Book, layer: &mut Vec<El>) -> Option<El> {
    let u = task_usage(book)?;
    // The meter's share is the context held over the window; its own panel is never shown (Neko's is), so only `input` and `limit`
    // matter here.
    let meter = Meter { input: u.context, output: 0, cache: 0, limit: u.window, cost: 0.0 };
    let mut discard = Vec::new();
    let face = Context::<App>::new("ctx", &meter, s.context_open).on_open(|s: &mut App, open| s.context_open = open).build(t, &mut discard);
    if s.context_open {
        context_panel(t, &u, layer);
    }
    Some(face.on_key(|s: &mut App, k: &KeyEvent, _: &mut Tx| return_presses(k, || s.context_open = !s.context_open)))
}

/// The open task: its conversation, and its composer.
pub fn task_view(s: &App, t: &Theme, layer: &mut Vec<El>) -> El {
    let Some(task) = s.open_task() else { return el() };
    let empty = Book::default();
    let book = s.book().unwrap_or(&empty);
    // The task's own words first (their text keyed "task-words", as in S0).
    let mut items = vec![person(t, "task-message", "task-words", &task.words)];
    if book.loaded && book.runs.is_empty() && book.parts.is_empty() && task.status == neko_protocol::TaskStatus::New {
        items.push(status_line(s, t, task));
    }
    for b in blocks(book) {
        match b {
            Block::Person(p) => items.push(person(t, &format!("part-{}", p.seq), &format!("part-{}-words", p.seq), p.body.text())),
            Block::Loose(p) => items.extend(notice(t, p)),
            Block::Run(r, parts) => items.extend(run_block(s, t, book, r, &parts)),
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

    #[test]
    fn a_duration_under_a_second_is_not_shown() {
        assert_eq!(took(0.4), None);
        assert_eq!(took(1.2), Some("1s".into()));
        assert_eq!(took(38.0), Some("38s".into()));
    }
}
