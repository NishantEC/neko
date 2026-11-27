//! The pieces of a task's conversation in Neko's design (docs/knowledge/design.md): the person's words in a soft well, each turn led
//! by its cat, steps on a quiet rail with neutral marks, a request led by the cat that asks, the live line, and the receipt a run
//! ends with. Built over rune-ui's FoldLine, Button and theme; rune-ui's own Tool, Reasoning, Confirmation and Task are not used
//! because their marks and headers carry colour and words this design gives to the cat (upstream asks in design.md).

use std::rc::Rc;

use neko_protocol::undo::{Action, Offer};
use neko_protocol::{Part, PartBody, Permission, PermissionState, Run, RunStatus, ToolState};
use rune::{El, Key, KeyEvent, Transition, Tx, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size, tabular, weight};

use crate::state::App;
use crate::ui::agent::{FoldLine, Marks, PressTx, Status, caption, elapsed, icon_button, mark_with, pre, return_presses, took};
use crate::ui::reasoning::Reasoning;
use crate::ui::base::{Shape, Theme, glyph};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::message::{Bubble, Message};
use crate::view::{COLUMN, raised, well};

/// How a card that arrives settles in: up eight points and in, on the theme's settle (a fade alone under Reduce Motion).
pub fn settling(e: El) -> El {
    e.transition(Transition::offset(0.0, 8.0).combined(Transition::opacity()))
}

/// The person's words: rune-ui's `Message` with the well bubble at the right. Copy shows under them while the pointer is over them or
/// the keyboard is in them, and takes no room while hidden (it stays in the Tab order).
pub fn person(s: &App, t: &Theme, key: &str, words_key: &str, words: &str) -> El {
    let (key, words_key) = (crate::task_view::kk(key.to_owned()), crate::task_view::kk(words_key.to_owned()));
    let (key, words_key) = (key.as_str(), words_key.as_str());
    let said = text(words.to_owned()).key(words_key.to_owned()).font(t.ty.md).color(t.ink()).wrap().min_w(0.0).selectable(true);
    let w = words.to_owned();
    let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(w.clone()));
    let copy = icon_button(t, &format!("copy-{key}"), "Copy", &icons::COPY, None, false, Some(press));
    let at = key.to_owned();
    Message::user(key.to_owned(), "You")
        .bubble(Bubble::Well)
        .column(COLUMN)
        .child(said)
        .actions(copy)
        .reveal_actions(s.reveal(key), move |s: &mut App| s.reveals.entry(at.clone()).or_default())
        .build(t)
}

/// A neutral step mark: rune-ui's `Marks::Neutral` (a tick and a dash in the third ink, a turning ring in the second), red only for a
/// tool that could not run. A tick and a turning ring are two points under the icon's size, so a step stays quiet.
fn mark(t: &Theme, st: ToolState) -> El {
    let i = t.control.icon;
    match st {
        ToolState::Running => mark_with(t, Status::Running, i - 2.0, Marks::Neutral).a11y_hidden(),
        ToolState::Done => mark_with(t, Status::Done, i - 2.0, Marks::Neutral),
        ToolState::Failed => mark_with(t, Status::Error, i, Marks::Neutral),
        ToolState::Denied => mark_with(t, Status::Denied, i, Marks::Neutral),
    }
}

/// A tool call on the rail: the phrase, its time or its note, folding to its input and output.
pub fn step(s: &App, t: &Theme, key: &str, p: &Part, workdir: &str) -> Option<El> {
    let PartBody::Tool { name, summary, input, output, state, exit, .. } = &p.body else { return None };
    let label = crate::task_view::phrase(name, summary, input, *state == ToolState::Running, workdir, s.home.as_deref());
    let open = s.is_open(key);
    let k = key.to_owned();
    let mut line = FoldLine::<App>::new(key.to_owned(), label, open).lead(mark(t, *state)).truncate_middle(true).on_toggle(move |s: &mut App| s.flip(&k));
    match (state, exit) {
        (ToolState::Denied, _) => line = line.meta("Denied".to_owned()),
        (ToolState::Failed, _) => line = line.meta("Could not run".to_owned()),
        (ToolState::Done, Some(code)) => line = line.meta(format!("exit {code}")),
        (ToolState::Done, None) => {
            if let Some(d) = p.ended_ms.map(|e| (e - p.at_ms).max(0) as f64 / 1000.0).and_then(took) {
                line = line.meta(d);
            }
        }
        _ => {}
    }
    let mut body = Vec::new();
    if open {
        if !input.is_empty() {
            body.push(caption(t, "Input"));
            body.push(pre(t, &format!("{key}-input"), input, t.ink(), 200.0));
        }
        let failed = *state == ToolState::Failed;
        match output.as_ref().filter(|o| !o.is_empty()) {
            Some(o) => {
                body.push(caption(t, if failed { "Error" } else { "Output" }));
                body.push(pre(t, &format!("{key}-output"), o, if failed { t.c.danger.fg } else { t.ink_muted() }, 240.0));
            }
            None if failed => body.push(text("It did not finish.").font(t.ty.sm).color(t.c.danger.fg)),
            None => {}
        }
    }
    Some(line.build(t, body))
}

/// A thought on the rail: rune-ui's `Reasoning` with the hollow dot ("Thought for 4s", folding to its words). A thought with no words
/// is no step: left out.
pub fn thought(s: &App, t: &Theme, key: &str, p: &Part) -> Option<El> {
    let PartBody::Thinking { text: words, done } = &p.body else { return None };
    if words.trim().is_empty() {
        return None;
    }
    let secs = p.ended_ms.map_or(0.0, |e| (e - p.at_ms).max(0) as f64 / 1000.0);
    let k = key.to_owned();
    Some(Reasoning::<App>::new(key.to_owned(), !*done, secs, s.is_open(key)).text(words.clone()).icon(None).on_toggle(move |s: &mut App| s.flip(&k)).build(t))
}

/// Steps in a row hang on a rail: a hairline in the margin, twelve points left of the reading edge; the steps keep the edge.
pub fn rail(t: &Theme, steps: Vec<El>) -> El {
    col().w_full().gap(1.0).ml(-t.space.at(3.0)).pl(t.space.at(3.0) - 1.0).border_sides([0.0, 0.0, 0.0, 1.0], t.c.line).children(steps)
}

/// The live line: what the agent is doing now, with the step it is on: one quiet line with the turning mark a running step has. Its
/// name for assistive technology says who ("The agent is thinking").
pub fn live_line(t: &Theme, r: &Run, name: &str, thinking: bool, steps: usize) -> El {
    let doing = if thinking { "thinking" } else { "working" };
    let label = format!("{name} is {doing}");
    let seen = if thinking { "Thinking…" } else { "Working…" };
    let words = if steps > 0 { format!("{seen} · step {}", steps + 1) } else { seen.to_owned() };
    row()
        .key(format!("run-{}-working", r.id.0))
        .role("status")
        .label(label)
        .w_full()
        .h(t.control.lg)
        // A step's own inset and gap (rune-ui's FoldLine), so the mark and the words sit under the steps' marks and words.
        .pl(t.space.at(1.5))
        .gap(t.metrics.gap)
        .center_y()
        .child(row().size(t.control.icon, t.control.icon).center().child(mark(t, ToolState::Running)))
        .child(text(words).font(t.ty.base).color(t.ink_muted()).a11y_hidden())
}

/// A permission request, led by a small amber mark: "The agent asks" (or "Kitten 1 asks"), what it wants to do and why, the exact
/// command, and the three answers (two for what cannot be taken back). The same keys and words as rune-ui's
/// Confirmation: Tab reaches each answer, Return or Space gives it, Escape anywhere denies; the destructive form has no Always and
/// Return gives no answer. Answered, it is one quiet line in the third ink.
pub fn request(t: &Theme, p: &Permission, name: &str) -> El {
    let key = format!("perm-{}", p.id.0);
    if p.state != PermissionState::Pending {
        // rune-ui's `Confirmation::answered_marks(Marks::Neutral)` draws the same line with the mark at the icon's full size; a step's
        // tick here is two points smaller, so the answered line's words line up with the steps' (upstream ask: a mark size).
        let small = t.control.icon - 2.0;
        let (mark, word) = match p.state {
            PermissionState::Allowed => (mark_with(t, Status::Done, small, Marks::Neutral), "Allowed once"),
            PermissionState::AlwaysAllowed => (mark_with(t, Status::Done, small, Marks::Neutral), "Always allowed"),
            PermissionState::Denied => (mark_with(t, Status::Denied, t.control.icon, Marks::Neutral), "Denied"),
            _ => (glyph(&icons::CLOCK, small, t.ink_faint()), "Expired"),
        };
        return row()
            .key(key)
            .role("status")
            .label(format!("{}: {word}", p.title))
            .w_full()
            .h(t.control.md)
            .pl(t.space.at(1.5))
            .gap(t.metrics.gap)
            .center_y()
            .child(mark)
            .child(text(word).font(weight(t.ty.base, t.ty.medium)).color(t.ink_muted()).a11y_hidden())
            .child(text(p.title.clone()).font(t.ty.base).color(t.ink_faint()).truncate().min_w(0.0).a11y_hidden());
    }
    let destructive = p.destructive;
    let id = p.id.clone();
    let decide = Rc::new(move |s: &mut App, d: neko_protocol::Decision| s.decide(&id, d));
    let answer = |suffix: &str, words: &str, v: ButtonVariant, d: neko_protocol::Decision| {
        let f = decide.clone();
        let g = decide.clone();
        let b = Button::<App>::new(format!("{key}-{suffix}"), words).variant(v).on_press(move |s: &mut App| f(s, d)).build(t);
        let mut holder = row();
        if !destructive {
            holder = holder.on_key(move |s: &mut App, k: &KeyEvent, _: &mut Tx| return_presses(k, || g(s, d)));
        }
        holder.child(b)
    };
    let mut answers = row().key(format!("{key}-answers")).gap(t.space.at(2.0)).flex_wrap().child(answer(
        "allow",
        "Allow once",
        if destructive { ButtonVariant::Destructive } else { ButtonVariant::Default },
        neko_protocol::Decision::AllowOnce,
    ));
    if !destructive {
        answers = answers.child(answer("always", "Always allow", ButtonVariant::Secondary, neko_protocol::Decision::AlwaysAllow));
    }
    answers = answers.child(answer("deny", "Deny", ButtonVariant::Ghost, neko_protocol::Decision::Deny));
    let asks = if destructive { format!("{name} asks to do something that cannot be undone") } else { format!("{name} asks") };
    // A small mark leads it: amber for what needs the person, red when what is asked cannot be taken back.
    let mark_ink = if destructive { t.c.danger.fg } else { t.c.warning.fg };
    let head = row()
        .w_full()
        .gap(t.space.at(2.5))
        .items_start()
        .child(row().key(format!("{key}-mark")).size(t.control.icon, t.control.icon).center().child(el().size(8.0, 8.0).radius(4.0).fill(mark_ink)).a11y_hidden())
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .pt(2.0)
                .child(text(asks).font(weight(t.ty.sm, t.ty.semibold)).color(if destructive { t.c.danger.fg } else { t.c.warning.fg }).wrap().min_w(0.0).a11y_hidden())
                .child(text(p.title.clone()).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).wrap().min_w(0.0)),
        );
    let mut body = col().w_full().gap(t.space.at(3.0)).child(head);
    let mut detail = col().w_full().pl(t.control.icon + t.space.at(2.5)).gap(t.space.at(3.0));
    if !p.why.is_empty() {
        detail = detail.child(text(format!("{name} asks because {}", lower_first(&p.why))).key(format!("{key}-why")).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0));
    }
    if !p.detail.is_empty() {
        detail = detail.child(pre(t, &format!("{key}-detail"), &p.detail, t.ink(), 160.0));
    }
    body = body.child(detail.child(answers));
    let how = if destructive { "Allow once or deny. Escape denies." } else { "Allow once, always allow, or deny. Escape denies." };
    let edge = if destructive { t.c.danger.solid } else { t.c.warning.solid };
    let card = col().key(key).role("alertdialog").label(format!("Permission: {}", p.title)).description(how).w_full().padding(t.space.at(4.0)).radius(t.radius.xl + 4.0).round_as(t);
    let f = decide.clone();
    settling(raised(t, card).border(1.0, edge.alpha(if t.dark { 0.45 } else { 0.4 })).child(body)).on_key(move |s: &mut App, k: &KeyEvent, _: &mut Tx| match k.key {
        Key::Escape => {
            f(s, neko_protocol::Decision::Deny);
            true
        }
        // The destructive form keeps Return from reaching any default button: only a deliberate press answers it.
        Key::Enter => destructive,
        _ => false,
    })
}

pub fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    c.next().map(|f| f.to_lowercase().collect::<String>() + c.as_str()).unwrap_or_default()
}

/// Plus and minus figures, in green and red, tabular.
fn figures(t: &Theme, added: u32, removed: u32, style: rune::TextStyle) -> [El; 2] {
    [text(format!("+{added}")).font(tabular(style)).color(t.c.success.fg).a11y_hidden(), text(format!("\u{2212}{removed}")).font(tabular(style)).color(t.c.danger.fg).a11y_hidden()]
}

/// A file's own figures in the receipt's form (the small step, green and red, only the side that says something): a file that
/// only gained lines shows no "\u{2212}0". Review's file headers use the same.
pub fn file_figures(t: &Theme, added: u32, removed: u32) -> Vec<El> {
    let [plus, minus] = figures(t, added, removed, t.ty.xs);
    let mut v = Vec::new();
    if added > 0 {
        v.push(plus);
    }
    if removed > 0 {
        v.push(minus);
    }
    v
}

/// The quiet line a run that changed files ends its fold with: `Changed 3 files · +19 −7 · Review · Undo` (the full receipt, with the
/// files, the folder and the branch, is in the right pane's Overview). A failed run says so first, in red; a stopped or interrupted
/// one says how far it got; an undone run says "now undone" and offers Redo. A run that changed nothing and did not fail has no line,
/// only the one quiet sentence undo sometimes has. A kitten's line has no buttons: its changes are read, and undone through its thread.
pub fn change_line(t: &Theme, r: &Run, o: &Offer, changed: Option<neko_protocol::Changes>, kitten: bool) -> Option<El> {
    let failed = r.status == RunStatus::Failed;
    let key = format!("run-{}", r.id.0);
    let note = |line: &String| text(line.clone()).key(format!("{key}-note")).role("note").font(t.ty.sm).color(t.ink_faint()).wrap().min_w(0.0);
    if changed.is_none() && !failed {
        return o.line.as_ref().map(note);
    }
    let files = |n: u32| format!("{n} file{}", if n == 1 { "" } else { "s" });
    let words = match (r.status, changed) {
        (RunStatus::Failed, Some(c)) => format!("Could not finish \u{b7} changed {}", files(c.files)),
        (RunStatus::Failed, None) => "Could not finish".to_owned(),
        (RunStatus::Stopped, Some(c)) => format!("Stopped after changing {}", files(c.files)),
        (RunStatus::Interrupted, Some(c)) => format!("Interrupted after changing {}", files(c.files)),
        (_, Some(c)) if o.undone => format!("Changed {}, now undone", files(c.files)),
        (_, Some(c)) => format!("Changed {}", files(c.files)),
        _ => "Finished".to_owned(),
    };
    let ink = if failed { t.c.danger.fg } else if o.undone { t.ink_faint() } else { t.ink_muted() };
    let dot = || text("\u{b7}").font(t.ty.sm).color(t.ink_faint()).mx(t.space.at(1.0)).a11y_hidden();
    let mut line = row().key(key.clone()).role("group").label(words.clone()).w_full().min_h(t.control.md).gap(0.0).center_y().flex_wrap().child(text(words).font(t.ty.sm).color(ink).a11y_hidden());
    if let Some(c) = changed {
        let mut facts = row().key(format!("{key}-facts")).gap(t.space.at(1.5)).label(format!("{} lines added, {} removed", c.added, c.removed)).role("note").children(figures(t, c.added, c.removed, weight(t.ty.sm, t.ty.semibold)));
        if o.undone {
            facts = facts.opacity(0.6);
        }
        line = line.child(dot()).child(facts);
    }
    if !kitten {
        if o.review {
            let (task, run) = (r.task.clone(), r.id.clone());
            line = line.child(dot()).child(Button::new(format!("{key}-review"), "Review").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(move |s: &mut App| s.open_review(&task, Some(run.clone()))).build(t));
        }
        if let Some((action, enabled)) = o.action {
            let run = r.id.clone();
            let (words, redo) = match action {
                Action::Undo => ("Undo", false),
                Action::Redo => ("Redo", true),
            };
            line = line.child(dot()).child(Button::new(format!("{key}-{}", words.to_lowercase()), words).variant(ButtonVariant::Ghost).size(Size::Sm).disabled(!enabled).on_press(move |s: &mut App| s.undo(&run, redo)).build(t));
        }
    }
    let mut out = col().w_full().gap(0.0).child(line);
    if let Some(l) = &o.line {
        out = out.child(note(l));
    }
    Some(out)
}

/// How many steps a run took: its tool calls. The band's chip and the run's own fold both count this, so they agree.
pub fn step_count<'a>(parts: impl Iterator<Item = &'a Part>) -> usize {
    parts.filter(|p| matches!(p.body, PartBody::Tool { .. })).count()
}

/// The band's run counter: while a run works, the step it is on; once it ends, how many steps and how long.
pub fn counter(s: &App, t: &Theme) -> Option<El> {
    let book = s.book()?;
    let r = book.runs.last()?;
    let n = step_count(book.parts.iter().filter(|p| p.run.as_ref() == Some(&r.id)));
    let running = r.status == RunStatus::Running;
    let words = if running {
        format!("Step {}", n + 1)
    } else {
        let secs = r.ended_ms.map_or(0.0, |e| (e - r.started_ms).max(0) as f64 / 1000.0).round().max(1.0);
        // A run with no steps has only its time to show: "0 steps · 9s" says nothing a person wants.
        if n == 0 { elapsed(secs) } else { format!("{n} step{} · {}", if n == 1 { "" } else { "s" }, elapsed(secs)) }
    };
    let label = if running { format!("Working, step {}", n + 1) } else { words.clone() };
    let mut pill = well(t, row().key("run-counter").role("status").label(label).h(24.0).px(t.space.at(2.5)).gap(t.space.at(1.5)).center_y().radius(12.0));
    if running {
        let mut dot = el().size(6.0, 6.0).radius(3.0).fill(t.c.ring);
        if !t.reduced_motion {
            dot = dot.looping(rune::LoopSpec::opacity(1.4, 0.0, 0.3, 1.0));
        }
        pill = pill.child(dot.a11y_hidden());
    }
    Some(pill.child(text(words).font(tabular(weight(t.ty.xs, t.ty.semibold))).color(if running { t.c.accent.fg } else { t.ink_muted() }).a11y_hidden()))
}
