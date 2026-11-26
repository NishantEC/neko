//! The pieces of a task's conversation in Neko's design (docs/knowledge/design.md): the person's words in a soft well, each turn led
//! by its cat, steps on a quiet rail with neutral marks, a request led by the cat that asks, the live line, and the receipt a run
//! ends with. Built over rune-ui's FoldLine, Button and theme; rune-ui's own Tool, Reasoning, Confirmation and Task are not used
//! because their marks and headers carry colour and words this design gives to the cat (upstream asks in design.md).

use std::rc::Rc;

use neko_protocol::undo::{Action, Offer};
use neko_protocol::{DiffFile, Part, PartBody, Permission, PermissionState, Run, RunStatus, TaskStatus, ToolState};
use rune::{El, Key, KeyEvent, Transition, Tx, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size, tabular, weight};

use crate::faces::{portrait, portrait_in};
use crate::state::App;
use crate::ui::agent::{FoldLine, PressTx, caption, elapsed, icon_button, mono, pre, return_presses, took};
use crate::ui::base::{Shape, Theme, glyph};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::message::BLOCK_GAP;
use crate::view::{COLUMN, raised, well};

/// How a card that arrives settles in: up eight points and in, on the theme's settle (a fade alone under Reduce Motion).
pub fn settling(e: El) -> El {
    e.transition(Transition::offset(0.0, 8.0).combined(Transition::opacity()))
}

/// The person's words: a soft well at the right. Copy shows under them while the pointer is over them or the keyboard is in them,
/// and takes no room while hidden (it stays in the Tab order, as rune-ui's Message does it).
pub fn person(s: &App, t: &Theme, key: &str, words_key: &str, words: &str) -> El {
    let said = text(words.to_owned()).key(words_key.to_owned()).font(t.ty.md).color(t.ink()).wrap().min_w(0.0).selectable(true);
    let bubble = well(t, col().key(format!("{key}-bubble")).px(t.space.at(4.0)).py(t.space.at(2.5)).max_w(COLUMN * 0.8).min_w(0.0).radius(t.radius.xl).round_as(t)).child(said);
    let w = words.to_owned();
    let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(w.clone()));
    let copy = icon_button(t, &format!("copy-{key}"), "Copy", &icons::COPY, None, false, Some(press));
    let shown = s.reveal(key).shown();
    let slot = row().key(format!("{key}-actions-slot"));
    let slot = if shown { slot.pt(t.space.at(BLOCK_GAP)).w_full().justify_end().child(copy) } else { slot.absolute(0.0, 0.0).size(0.0, 0.0).clip_rounded(0.0).child(copy) };
    let (a, b) = (key.to_owned(), key.to_owned());
    col()
        .key(key.to_owned())
        .role("article")
        .label("You said")
        .w_full()
        .max_w(COLUMN)
        .items_end()
        .child(bubble)
        .child(slot)
        .on_hover_change(move |s: &mut App, inside: bool, _: &mut Tx| s.reveals.entry(a.clone()).or_default().hovered = inside)
        .on_focus_within_change(move |s: &mut App, within: bool, _: &mut Tx| s.reveals.entry(b.clone()).or_default().focused = within)
}

/// A neutral step mark: the state's shape in the third ink (a tick, a dash, a turning ring), never a colour; red only for a tool
/// that could not run.
fn mark(t: &Theme, st: ToolState) -> El {
    let i = t.control.icon;
    match st {
        ToolState::Running => crate::ui::base::spinner_in(t, i - 2.0, t.ink_muted(), "Running").a11y_hidden(),
        ToolState::Done => glyph(&icons::CHECK, i - 2.0, t.ink_faint()),
        ToolState::Failed => glyph(&icons::CIRCLE_X, i, t.c.danger.fg),
        ToolState::Denied => row().size(i, i).center().child(el().w(8.0).h(1.5).radius(0.75).fill(t.ink_faint())).a11y_hidden(),
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

/// A thought on the rail: "Thought for 4s" with a hollow dot, folding to its words. A thought with no words is no step: left out.
pub fn thought(s: &App, t: &Theme, key: &str, p: &Part) -> Option<El> {
    let PartBody::Thinking { text: words, done } = &p.body else { return None };
    if words.trim().is_empty() {
        return None;
    }
    let secs = p.ended_ms.map_or(0.0, |e| (e - p.at_ms).max(0) as f64 / 1000.0);
    let label = if !*done { "Thinking…".to_owned() } else if secs < 1.0 { "Thought".to_owned() } else { format!("Thought for {}", elapsed(secs.round())) };
    let open = s.is_open(key);
    let k = key.to_owned();
    let lead = row().size(t.control.icon, t.control.icon).center().child(el().size(7.0, 7.0).radius(3.5).border(1.2, t.ink_faint())).a11y_hidden();
    let body = if open { vec![text(words.clone()).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).selectable(true)] } else { Vec::new() };
    Some(FoldLine::<App>::new(key.to_owned(), label, open).lead(lead).on_toggle(move |s: &mut App| s.flip(&k)).build(t, body))
}

/// Steps in a row hang on a rail: a hairline in the margin, twelve points left of the reading edge; the steps keep the edge.
pub fn rail(t: &Theme, steps: Vec<El>) -> El {
    col().w_full().gap(1.0).ml(-t.space.at(3.0)).pl(t.space.at(3.0) - 1.0).border_sides([0.0, 0.0, 0.0, 1.0], t.c.line).children(steps)
}

/// The turn's head: the cat's portrait (32, ringed while it works), its name, and how the run stands. `waiting`: a request of this
/// run waits for the person, so the head says so (as the sidebar's row does) instead of "working". `undone`: its changes were
/// undone, so a finished run says "undone", not "done".
pub fn turn_head(t: &Theme, r: &Run, name: &str, face: &str, waiting: bool, undone: bool) -> El {
    let (st, doing) = match r.status {
        RunStatus::Running if waiting => (Some(TaskStatus::NeedsYou), "needs you"),
        RunStatus::Running => (Some(TaskStatus::Working), "working"),
        RunStatus::Failed => (Some(TaskStatus::Failed), "could not finish"),
        RunStatus::Stopped => (None, "stopped"),
        RunStatus::Interrupted => (None, "was interrupted"),
        // A run whose changes were undone is not "done": the files are as they were before it.
        RunStatus::Done if undone => (None, "undone"),
        RunStatus::Done => (None, "done"),
    };
    let ink = match r.status {
        RunStatus::Running if waiting => t.c.warning.fg,
        RunStatus::Running => t.c.accent.fg,
        RunStatus::Failed => t.c.danger.fg,
        _ => t.ink_faint(),
    };
    row()
        .key(format!("turn-{}-cat", r.id.0))
        .w_full()
        .gap(t.space.at(2.5))
        .center_y()
        // The portrait's ring sits in the margin, so the portrait itself starts on the reading edge.
        .ml(-(crate::faces::ringed(32.0) - 32.0) / 2.0)
        .child(portrait_in(t, face, 32.0, st, t.c.page.bg))
        .child(text(name.to_owned()).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()).a11y_hidden())
        .child(text(doing).font(t.ty.sm).color(ink).a11y_hidden())
}

/// The live line: what the cat is doing now, with the step it is on. The turn's head already shows who (portrait, name, ring), so
/// this line never repeats them: it is one quiet line with the turning mark a running step has. Its name for assistive technology
/// still says who ("miso is thinking").
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

/// A permission request, led by the cat that asks: its portrait with the needs-you dot, "miso asks", what it wants to do and why,
/// the exact command, and the three answers (two for what cannot be taken back). The same keys and words as rune-ui's
/// Confirmation: Tab reaches each answer, Return or Space gives it, Escape anywhere denies; the destructive form has no Always and
/// Return gives no answer. Answered, it is one quiet line in the third ink.
pub fn request(t: &Theme, p: &Permission, name: &str, face: &str) -> El {
    let key = format!("perm-{}", p.id.0);
    if p.state != PermissionState::Pending {
        let (icon, word) = match p.state {
            PermissionState::Allowed => (&icons::CHECK, "Allowed once"),
            PermissionState::AlwaysAllowed => (&icons::CHECK, "Always allowed"),
            PermissionState::Denied => (&icons::CLOSE, "Denied"),
            _ => (&icons::CLOCK, "Expired"),
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
            .child(glyph(icon, t.control.icon - 2.0, t.ink_faint()))
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
    let head = row()
        .w_full()
        .gap(t.space.at(3.0))
        .items_start()
        .child(portrait_in(t, face, 32.0, Some(TaskStatus::NeedsYou), crate::view::raised_fill(t)))
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
    let indent = crate::faces::ringed(32.0) + t.space.at(3.0);
    let mut detail = col().w_full().pl(indent).gap(t.space.at(3.0));
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

/// A file of a receipt: its path and its own counts, or its name alone while the counts are on their way.
pub struct ReceiptFile {
    pub path: String,
    pub counts: Option<(u32, u32)>,
    pub note: Option<&'static str>,
}

/// The receipt's files: from the run's own diff when it has come, else the files the run's steps edited (names only).
pub fn receipt_files(s: &App, r: &Run, parts: &[&Part]) -> Vec<ReceiptFile> {
    if let Some(d) = s.receipts.get(&r.id) {
        return d.files.iter().map(|f: &DiffFile| ReceiptFile { path: crate::review_view::file_label(f), counts: (!f.binary).then_some((f.added, f.removed)), note: f.binary.then_some("binary") }).collect();
    }
    let mut out: Vec<ReceiptFile> = Vec::new();
    for p in parts {
        if let PartBody::Tool { name, summary, state: ToolState::Done, .. } = &p.body
            && matches!(name.as_str(), "Edit" | "MultiEdit" | "Write" | "NotebookEdit")
        {
            let f = crate::task_view::show_path(summary, &r.workdir, s.home.as_deref());
            if !out.iter().any(|x| x.path == f) {
                out.push(ReceiptFile { path: f, counts: None, note: None });
            }
        }
    }
    out
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

/// How many files a receipt lists before "and N more".
const RECEIPT_FILES: usize = 5;

/// The receipt a run ends with, when it has something to show beyond time (files changed, a branch, a failure, or something undo
/// says): the cat and the sentence ("miso changed 3 files", +42 −7), the files each with its counts in a well, the quiet undo line,
/// and the branch, Review, Undo or Redo, Reveal in Finder and Copy branch.
#[allow(clippy::too_many_arguments)]
pub fn receipt(s: &App, t: &Theme, r: &Run, parts: &[&Part], o: &Offer, changed: Option<neko_protocol::Changes>, name: &str, face: &str) -> Option<El> {
    let failed = r.status == RunStatus::Failed;
    if changed.is_none() && r.branch.is_none() && !failed && o.line.is_none() && !o.review {
        return None;
    }
    let key = format!("run-{}", r.id.0);
    let files = |n: u32| format!("{n} file{}", if n == 1 { "" } else { "s" });
    let sentence = match (r.status, changed) {
        (RunStatus::Failed, _) => format!("{name} could not finish"),
        (RunStatus::Stopped, Some(c)) => format!("{name} stopped after changing {}", files(c.files)),
        (RunStatus::Stopped, None) => format!("{name} stopped"),
        (RunStatus::Interrupted, Some(c)) => format!("{name} was interrupted after changing {}", files(c.files)),
        (RunStatus::Interrupted, None) => format!("{name} was interrupted"),
        (_, Some(c)) if o.undone => format!("{name} changed {}, now undone", files(c.files)),
        (_, Some(c)) => format!("{name} changed {}", files(c.files)),
        _ => format!("{name} worked on a branch"),
    };
    let pad = t.space.at(1.5);
    let mut facts = row().key(format!("{key}-facts")).gap(t.space.at(2.0)).center_y();
    if let Some(c) = changed {
        facts = facts.label(format!("{}, {} lines added, {} removed", files(c.files), c.added, c.removed)).role("note").children(figures(t, c.added, c.removed, weight(t.ty.sm, t.ty.semibold)));
    }
    let head = row()
        .w_full()
        .px(t.space.at(2.5))
        .h(t.control.lg + 8.0)
        .gap(t.space.at(2.5))
        .center_y()
        .child(portrait(t, face, 24.0))
        .child(text(sentence.clone()).role("heading").level(3).font(weight(t.ty.base, t.ty.semibold)).color(if failed { t.c.danger.fg } else if o.undone { t.ink_muted() } else { t.ink() }).grow(1.0).truncate().min_w(0.0))
        .child(facts);
    let mut card = col().key(key.clone()).role("group").label(sentence).w_full().gap(0.0).child(head);
    let list = receipt_files(s, r, parts);
    let total = changed.map_or(list.len(), |c| (c.files as usize).max(list.len()));
    if total > 0 {
        let mut well_el = well(t, col().key(format!("{key}-files")).role("list").label("Files changed").w_full().py(t.space.at(1.0)).radius(t.radius.lg).round_as(t));
        let shown = list.len().min(if list.len() == RECEIPT_FILES + 1 { RECEIPT_FILES + 1 } else { RECEIPT_FILES });
        for (i, f) in list.iter().take(shown).enumerate() {
            let mut line = row()
                .role("listitem")
                .label(match f.counts {
                    Some((a, b)) => format!("{}, {a} added, {b} removed", f.path),
                    None => f.path.clone(),
                })
                .w_full()
                .h(t.control.md)
                .px(t.space.at(3.0))
                .gap(t.space.at(2.5))
                .center_y()
                .child(glyph(&icons::FILE, t.control.icon_sm, t.ink_faint()))
                .child(text(f.path.clone()).font(mono(t, t.ty.sm)).color(if o.undone { t.ink_muted() } else { t.ink() }).truncate_middle().min_w(0.0).grow(1.0).a11y_hidden());
            // Only the figures that say something: a file that only gained lines shows no "−0".
            if let Some((a, b)) = f.counts {
                line = line.children(file_figures(t, a, b));
            }
            if let Some(n) = f.note {
                line = line.child(text(n).font(t.ty.xs).color(t.ink_faint()).a11y_hidden());
            }
            if i > 0 {
                line = line.border_sides([0.5, 0.0, 0.0, 0.0], t.c.line);
            }
            well_el = well_el.child(line);
        }
        if total > shown {
            let words = if list.is_empty() { files(total as u32) } else { format!("and {} more", total - shown) };
            well_el = well_el.child(row().w_full().h(t.control.md).px(t.space.at(3.0)).center_y().border_sides([if list.is_empty() { 0.0 } else { 0.5 }, 0.0, 0.0, 0.0], t.c.line).child(text(words).font(t.ty.sm).color(t.ink_faint())));
        }
        card = card.child(well_el);
    }
    if let Some(line) = &o.line {
        card = card.child(text(line.clone()).key(format!("{key}-note")).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).px(t.space.at(2.5)).pt(t.space.at(2.5)));
    }
    let mut foot = row().key(format!("{key}-actions")).w_full().px(t.space.at(1.0)).pt(t.space.at(2.5)).pb(t.space.at(0.5)).gap(t.space.at(2.0)).center_y();
    if let Some(b) = &r.branch {
        foot = foot.child(row().pl(t.space.at(1.5)).gap(t.space.at(1.5)).center_y().min_w(0.0).shrink().child(glyph(&icons::COMMIT, t.control.icon_sm, t.ink_faint())).child(text(b.clone()).font(mono(t, t.ty.sm)).color(t.ink_muted()).truncate_middle().min_w(0.0).shrink()));
    }
    foot = foot.child(el().grow(1.0));
    if o.review {
        let (task, run) = (r.task.clone(), r.id.clone());
        foot = foot.child(Button::new(format!("{key}-review"), "Review").variant(ButtonVariant::Secondary).size(Size::Sm).icon(&icons::FILE_CODE).on_press(move |s: &mut App| s.open_review(&task, Some(run.clone()))).build(t));
    }
    if let Some((action, enabled)) = o.action {
        let run = r.id.clone();
        let (words, icon, redo) = match action {
            Action::Undo => ("Undo", &icons::RESTORE, false),
            Action::Redo => ("Redo", &icons::RETRY, true),
        };
        foot = foot.child(Button::new(format!("{key}-{}", words.to_lowercase()), words).variant(ButtonVariant::Secondary).size(Size::Sm).icon(icon).disabled(!enabled).on_press(move |s: &mut App| s.undo(&run, redo)).build(t));
    }
    if !r.workdir.is_empty() {
        let folder = r.workdir.clone();
        let reveal: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| {
            let _ = tx.reveal_path(&folder);
        });
        foot = foot.child(icon_button(t, &format!("{key}-reveal"), "Reveal in Finder", &icons::FOLDER_OPEN, None, false, Some(reveal)));
    }
    if let Some(b) = r.branch.clone() {
        let press: PressTx<App> = Rc::new(move |_: &mut App, tx: &mut Tx| tx.copy(b.clone()));
        foot = foot.child(icon_button(t, &format!("{key}-copy-branch"), "Copy branch", &icons::COPY, None, false, Some(press)));
    }
    let card = card.child(foot).padding(pad).radius(t.radius.lg + pad).round_as(t);
    Some(settling(raised(t, card)))
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
