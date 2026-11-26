//! The Task view (docs/knowledge/app-shell.md, "The Task view"): the conversation with the agent, and the composer under it.
//!
//! The answer is the page: the agent's words sit on it with no bubble. Thinking and tool calls are quiet single lines that fold.
//! While a run works, its steps show as they happen; once it ends they fold behind one "Worked for 2m 14s" line, its last words stay
//! open, and a run card says what changed, how long it took and what it cost. A request for permission waits in the transcript.
//! Colour is for state only.

use std::rc::Rc;

use neko_protocol::{Part, PartBody, Permission, PermissionState, Pricing, Run, RunStatus, Task, Tone as NoticeTone, ToolState};
use rune::{El, Key, KeyEvent, Tx, col, el, row, text};
use rune_kit::Markdown;
use rune_ui_style::{tabular, weight};

use crate::ui::base::Shape;

use crate::state::{App, Book};
use crate::ui::agent::{self, FoldLine, PressTx, Status, elapsed, icon_button, mono, pre, return_presses, slab, tokens};
use crate::ui::badge::Badge;
use crate::ui::base::{ButtonVariant, Theme, Tone, glyph, paint};
use crate::ui::button::Button;
use crate::ui::confirmation::{Confirmation, Decision};
use crate::ui::conversation::Conversation;
use crate::ui::icons;
use crate::ui::message::{self, Message};
use crate::ui::prompt_input::{Phase, PromptInput};
use crate::ui::reasoning::Reasoning;
use crate::ui::response::{CARET, Response};
use crate::ui::shimmer::Shimmer;
use crate::ui::tool::{Tool, ToolCall};
use crate::view::COLUMN;

/// What the agent is called in this slice (named cats come later).
pub const AGENT: &str = "Neko";

/// A copy button for `words`.
fn copy(t: &Theme, key: &str, words: String) -> El {
    let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(words.clone()));
    icon_button(t, &format!("copy-{key}"), "Copy", &icons::COPY, None, false, Some(press))
}

/// The person's words, at the right, with a copy button under them.
fn person(t: &Theme, key: &str, words_key: &str, words: &str) -> El {
    let said = message::words(t, words.to_owned()).key(words_key.to_owned()).selectable(true);
    Message::user(key.to_owned(), "You").child(said).actions(row().child(copy(t, key, words.to_owned()))).column(COLUMN).build(t)
}

/// The agent's words as Markdown on the page, selectable. Built on rune-ui's `Response` look; Neko builds the Markdown itself only
/// to make it selectable (rune-ui's `Response` has no such switch yet).
fn answer(t: &Theme, key: &str, words: &str, streaming: bool) -> El {
    let mut src = words.to_owned();
    if streaming {
        if !src.is_empty() && !src.ends_with(char::is_whitespace) {
            src.push(' ');
        }
        src.push_str(CARET);
    }
    let md = Markdown::new(src).streaming(streaming).reveal(if streaming && !t.reduced_motion { 0.18 } else { 0.0 }).selectable(!streaming).style(Response::style(t, COLUMN)).build(t.to_kit());
    let body = col().key(format!("{key}-answer")).role("document").label("Response").busy(streaming).w_full().max_w(COLUMN).min_w(0.0).child(md);
    let mut m = Message::assistant(key.to_owned(), AGENT).child(body).column(COLUMN);
    if !streaming {
        m = m.actions(row().child(copy(t, key, words.to_owned())));
    }
    m.build(t)
}

fn secs(p: &Part) -> f64 {
    p.ended_ms.map_or(0.0, |e| (e - p.at_ms).max(0) as f64 / 1000.0)
}

fn tool_status(s: ToolState) -> Status {
    match s {
        ToolState::Running => Status::Running,
        ToolState::Done => Status::Done,
        ToolState::Failed => Status::Error,
    }
}

/// A step of a run: thinking, a tool call, a permission request, words on the way. None for parts drawn elsewhere.
fn step(s: &App, t: &Theme, task: &Task, book: &Book, p: &Part, last_words: bool) -> Option<El> {
    let key = format!("part-{}", p.seq);
    match &p.body {
        PartBody::Thinking { text: words, done } => {
            let k = key.clone();
            Some(Reasoning::new(key.clone(), !*done, secs(p), s.is_open(&key)).text(words.clone()).on_toggle(move |s: &mut App| s.flip(&k)).build(t))
        }
        PartBody::Tool { name, summary, input, output, state, .. } => {
            let call = ToolCall {
                name: name.clone(),
                summary: summary.clone(),
                status: tool_status(*state),
                secs: p.ended_ms.map(|_| secs(p)),
                input: input.clone(),
                output: if *state == ToolState::Failed { None } else { output.clone() },
                error: if *state == ToolState::Failed { Some(output.clone().unwrap_or_else(|| "It did not finish.".into())) } else { None },
            };
            let k = key.clone();
            Some(Tool::new(key.clone(), &call, s.is_open(&key)).on_toggle(move |s: &mut App| s.flip(&k)).build(t))
        }
        PartBody::Permission { permission } => book.permissions.get(permission).map(|perm| request(t, task, perm)),
        PartBody::Agent { text: words, done } => Some(answer(t, &key, words, !*done && last_words)),
        _ => None,
    }
}

/// A permission request: waiting, it asks; answered, it is one quiet line.
fn request(t: &Theme, _task: &Task, p: &Permission) -> El {
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
    col().key(format!("{key}-wrap")).w_full().gap(t.space.at(2.0)).child(panel).child(text(format!("Neko asks because {}", lower_first(&p.why))).key(format!("{key}-why")).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0).px(t.space.at(1.5)).a11y_hidden())
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
            .max_w(COLUMN)
            .gap(t.metrics.gap)
            .items_start()
            .px(t.space.at(1.5))
            .child(row().size(t.control.icon, t.ty.base.line_height.max(t.control.icon)).center().child(glyph(icon, t.control.icon, ink)))
            .child(lines),
    )
}

fn status_of(r: &Run) -> (Status, &'static str, Tone) {
    match r.status {
        RunStatus::Running => (Status::Running, "Working", Tone::Info),
        RunStatus::Done => (Status::Done, "Done", Tone::Success),
        RunStatus::Failed => (Status::Error, "Failed", Tone::Danger),
        RunStatus::Stopped => (Status::Skipped, "Stopped", Tone::Neutral),
        RunStatus::Interrupted => (Status::Skipped, "Interrupted", Tone::Neutral),
    }
}

/// Money in dollars, as rune-ui writes it.
fn money(usd: f64) -> String {
    agent::money(usd)
}

/// What a run cost, in words that never invent a figure: the CLI's own cost with how to read it, else its tokens, else nothing.
pub fn cost_words(r: &Run) -> Option<(String, String)> {
    let u = r.usage.as_ref()?;
    let tokens_line = format!("{} in · {} out{}", tokens(u.input_tokens), tokens(u.output_tokens), if u.cached_tokens > 0 { format!(" · {} cached", tokens(u.cached_tokens)) } else { String::new() });
    let by = if u.reported_by.is_empty() { "the agent".to_owned() } else { u.reported_by.clone() };
    match u.cost_usd {
        Some(c) => {
            let how = match u.pricing {
                Pricing::ApiEquivalent => format!("API-equivalent price as reported by {by}. Your subscription is not charged per call."),
                Pricing::Charged => format!("As reported by {by}."),
                Pricing::Unknown => format!("As reported by {by}."),
            };
            Some((money(c), format!("{tokens_line}. {how}")))
        }
        None if u.input_tokens + u.output_tokens > 0 => Some((format!("{} tokens", tokens(u.input_tokens + u.output_tokens)), format!("{tokens_line}, as reported by {by}. It reported no cost."))),
        None => None,
    }
}

/// The run card: how it ended, what changed, how long, what it cost.
fn run_card(t: &Theme, r: &Run, book: &Book) -> El {
    let (_, word, tone) = status_of(r);
    let secs = r.ended_ms.map_or(0.0, |e| (e - r.started_ms).max(0) as f64 / 1000.0);
    let fact = |icon, words: El| row().gap(t.space.at(1.5)).center_y().child(glyph(icon, t.control.icon_sm, t.ink_faint())).child(words);
    let figures = |s: String| text(s).font(tabular(t.ty.sm)).color(t.ink_muted());
    let mut facts = row().key(format!("run-{}-facts", r.id.0)).w_full().gap(t.space.at(5.0)).flex_wrap();
    if let Some(c) = r.changes {
        let changes = row()
            .gap(t.space.at(1.5))
            .child(figures(format!("{} file{}", c.files, if c.files == 1 { "" } else { "s" })))
            .children((c.files > 0).then(|| text(format!("+{}", c.added)).font(tabular(t.ty.sm)).color(t.c.success.fg)))
            .children((c.files > 0).then(|| text(format!("\u{2212}{}", c.removed)).font(tabular(t.ty.sm)).color(t.c.danger.fg)));
        facts = facts.child(fact(&icons::FILE, changes));
    }
    if let Some(b) = &r.branch {
        facts = facts.child(fact(&icons::COMMIT, figures(b.clone())));
    }
    facts = facts.child(fact(&icons::CLOCK, figures(elapsed(secs.round()))));
    let cost = cost_words(r);
    if let Some((figure, _)) = &cost {
        facts = facts.child(fact(&icons::TAG, figures(figure.clone())));
    }
    let head = row()
        .w_full()
        .gap(t.space.at(3.0))
        .center_y()
        .child(Badge::new(format!("run-{}-status", r.id.0), word).tone(tone).dot().build(t))
        .child(row().grow(1.0))
        .children(r.reason.clone().filter(|_| matches!(r.status, RunStatus::Stopped | RunStatus::Interrupted)).map(|why| text(why).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink()));
    let mut card = paint(col().key(format!("run-{}", r.id.0)).role("group").label(format!("Run: {word}")).w_full().max_w(COLUMN).padding(t.metrics.inset).gap(t.space.at(3.0)).radius(t.radius.xl).round_as(t), &t.mat.slab.base)
        .child(head)
        .child(facts);
    let mut quiet = Vec::new();
    if let Some((_, how)) = cost {
        quiet.push(how);
    }
    if book.runs.iter().filter(|x| x.usage.is_some()).count() > 1
        && let Some(total) = book.cost()
    {
        quiet.push(format!("This task so far: {}.", money(total)));
    }
    if !quiet.is_empty() {
        card = card.child(text(quiet.join(" ")).key(format!("run-{}-cost", r.id.0)).font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0));
    }
    card
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
    // A run that has said nothing yet still shows (working, or its card).
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

/// Thinking and tool calls: quiet lines that sit together, closer than the messages around them.
fn quiet_line(p: &Part) -> bool {
    matches!(p.body, PartBody::Thinking { .. } | PartBody::Tool { .. })
}

/// The quiet lines gathered so far, as one tight column.
fn group(t: &Theme, lines: &mut Vec<El>) -> Option<El> {
    if lines.is_empty() {
        return None;
    }
    Some(col().w_full().max_w(COLUMN).gap(t.space.at(0.5)).children(std::mem::take(lines)))
}

/// One run: live steps while it works; afterwards its steps behind "Worked for", its last words, and its card.
fn run_block(s: &App, t: &Theme, task: &Task, book: &Book, r: &Run, parts: &[&Part]) -> Vec<El> {
    let mut out = Vec::new();
    let running = r.status == RunStatus::Running;
    let last_words = parts.iter().rposition(|p| matches!(p.body, PartBody::Agent { .. }));
    if running {
        let mut lines = Vec::new();
        for (i, p) in parts.iter().enumerate() {
            let Some(e) = step(s, t, task, book, p, Some(i) == last_words).or_else(|| notice(t, p)) else { continue };
            if quiet_line(p) {
                lines.push(e);
            } else {
                out.extend(group(t, &mut lines));
                out.push(e);
            }
        }
        out.extend(group(t, &mut lines));
        let streaming = parts.last().is_some_and(|p| matches!(p.body, PartBody::Agent { done: false, .. } | PartBody::Thinking { done: false, .. }));
        let waiting = parts.iter().any(|p| matches!(&p.body, PartBody::Permission { permission } if book.permissions.get(permission).is_some_and(|x| x.state == PermissionState::Pending)));
        if !streaming && !waiting {
            out.push(row().key(format!("run-{}-working", r.id.0)).role("status").label("Neko is working").px(t.space.at(1.5)).gap(t.metrics.gap).center_y().child(glyph(&icons::SPARKLE, t.control.icon, t.ink_faint())).child(Shimmer::new(format!("run-{}-shimmer", r.id.0), "Working…").build(t).a11y_hidden()));
        }
        return out;
    }
    // The words after the last step stay open; everything before folds.
    let tail_from = parts.iter().rposition(|p| !matches!(p.body, PartBody::Agent { .. } | PartBody::Notice { .. })).map_or(0, |i| i + 1);
    let (steps, tail) = parts.split_at(tail_from);
    let steps: Vec<&&Part> = steps.iter().collect();
    if !steps.is_empty() {
        let key = format!("worked-{}", r.id.0);
        let secs = r.ended_ms.map_or(0.0, |e| (e - r.started_ms).max(0) as f64 / 1000.0);
        let n = steps.iter().filter(|p| !matches!(p.body, PartBody::Agent { .. })).count();
        let mut body: Vec<El> = Vec::new();
        if s.is_open(&key) {
            let mut lines = Vec::new();
            for p in &steps {
                let Some(e) = step(s, t, task, book, p, false).or_else(|| notice(t, p)) else { continue };
                if quiet_line(p) {
                    lines.push(e);
                } else {
                    body.extend(group(t, &mut lines));
                    body.push(e);
                }
            }
            body.extend(group(t, &mut lines));
        }
        let k = key.clone();
        let line = FoldLine::<App>::new(key.clone(), format!("Worked for {}", elapsed(secs.round())), s.is_open(&key))
            .lead(glyph(&icons::CLOCK, t.control.icon, t.ink_faint()))
            .meta(format!("{n} step{}", if n == 1 { "" } else { "s" }))
            .on_toggle(move |s: &mut App| s.flip(&k))
            .build(t, body);
        out.push(line);
    }
    for p in tail {
        if let Some(e) = step(s, t, task, book, p, false).or_else(|| notice(t, p)) {
            out.push(e);
        }
    }
    out.push(run_card(t, r, book));
    out
}

/// The open task: its conversation, and its composer.
pub fn task_view(s: &App, t: &Theme) -> El {
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
            Block::Run(r, parts) => items.extend(run_block(s, t, task, book, r, &parts)),
        }
    }
    let conversation = Conversation::<App>::new("transcript", s.at_bottom).column(COLUMN + 48.0).children(items).on_at_bottom(|s: &mut App, at| s.at_bottom = at).build(t);
    let working = s.open_is_working();
    let reply = s.reply_box().cloned().unwrap_or_default();
    let composer = PromptInput::new("reply", &reply, |s: &mut App| s.reply_mut())
        .placeholder(if working { "Neko is working. Press Escape to stop." } else { "Reply to Neko" })
        .spellcheck(true)
        .phase(if working { Phase::Streaming } else { Phase::Ready })
        .on_submit(|s: &mut App| s.follow_up())
        .on_stop(|s: &mut App| s.stop())
        .build(t);
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
        .child(row().w_full().justify_center().px(t.space.at(8.0)).pt(t.space.at(2.0)).pb(t.space.at(5.0)).child(foot))
}

/// S0's quiet line for a task no agent has started.
fn status_line(s: &App, t: &Theme, task: &Task) -> El {
    let added = crate::clock::added(task.created_ms, s.now_ms, s.utc_offset_s);
    row()
        .key("task-status")
        .role("status")
        .label(format!("No cat is working on this yet. Added {added}."))
        .w_full()
        .max_w(COLUMN)
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
