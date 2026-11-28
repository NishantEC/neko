//! Kittens in the conversation (docs/knowledge/app-shell.md and design.md, "Kittens in the app"): the card where a thread started
//! them, the note where one's report woke the thread, the line that says what the thread waits on, the read-only foot of a
//! subagent opened on its own, and the band's breadcrumb. The words and the state are `subagents.rs`.

use std::rc::Rc;

use neko_protocol::{Part, PartBody, Task, TaskId, TaskStatus};
use rune::{El, TextEdit, Tx, col, el, row, text};
use rune_ui_style::{ButtonVariant, weight};

use crate::state::App;
use crate::subagents::{kitten_word, name, names_sentence, order_words, place_words, question_line, wake_line};
use crate::ui::agent::{FoldLine, Marks, PressTx, mark_with, mono};
use crate::ui::base::{Shape, SizeExt, Theme, glyph, hit_area, paint_state, recipes, states};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::response::Response;
use crate::ui::textarea::Textarea;
use crate::view::{COLUMN, raised};

/// A card row's height: two lines and air.
const ROW_H: f32 = 56.0;

/// The still ring: the working ring held still, in the second ink. A thread that waits is not busy, and must not look as if it were.
pub fn still_ring(t: &Theme, size: f32) -> El {
    let ink = t.ink_muted();
    let track = rune::icon(&icons::RING, size).absolute(0.0, 0.0).color(ink.alpha(0.3));
    let arc = rune::icon(&icons::ARC, size).absolute(0.0, 0.0).color(ink);
    el().size(size, size).role("img").label("Waiting").child(track).child(arc)
}

/// The still ring in `ink` (the sidebar's mark takes its colour from the row's surface).
pub fn still_ring_in(size: f32, ink: rune::Color) -> El {
    let track = rune::icon(&icons::RING, size).absolute(0.0, 0.0).color(ink.alpha(0.3));
    let arc = rune::icon(&icons::ARC, size).absolute(0.0, 0.0).color(ink);
    el().size(size, size).child(track).child(arc)
}

/// A task's mark by its status, `size` across: the kit's neutral marks, and the still ring for Waiting.
pub fn status_glyph(t: &Theme, st: TaskStatus, size: f32) -> El {
    match st {
        TaskStatus::Waiting => still_ring(t, size),
        other => mark_with(t, crate::view::status_mark(other).0, size, Marks::Neutral),
    }
}

/// The ink a card's status word is in: work in the identity colour, a request amber, a failure red, the rest quiet.
fn word_ink(t: &Theme, st: TaskStatus) -> rune::Color {
    match st {
        TaskStatus::Working => t.c.accent.fg,
        TaskStatus::NeedsYou | TaskStatus::Asked => t.c.warning.fg,
        TaskStatus::Failed => t.c.danger.fg,
        _ => t.ink_muted(),
    }
}

/// A secondary button whose visible words are short and whose name for assistive technology says more ("Open", "Open Kitten 1").
fn named_button(t: &Theme, variant: ButtonVariant, key: &str, words: &str, name: &str, press: PressTx<App>) -> El {
    let size = crate::ui::base::Size::Sm;
    let parts = recipes::button(t, variant);
    let st = states(None, false, false);
    let ink = parts.label.resolve(st);
    let pad = size.pad(t);
    let g = press.clone();
    let mut face = row().key(key.to_owned()).role("button").label(name.to_owned()).focusable(true).h(size.height(t)).pl(pad).pr(pad).center().radius(size.radius(t)).round_as(t);
    face = paint_state(face, &parts.root, st).child(text(words.to_owned()).font(size.text(t, t.ty.medium)).color(ink).a11y_hidden());
    face = face.on_press_tx(move |s: &mut App, tx: &mut Tx| g(s, tx));
    let slop = t.control.slop(size.height(t));
    let hit = hit_area::<App>(key, face, 0.0, slop, None);
    if slop > 0.0 { hit.on_press_tx(move |s: &mut App, tx: &mut Tx| press(s, tx)) } else { hit }
}

/// What a subagent's card knows: its task (if the window has it), its words.
struct Row<'a> {
    number: &'a str,
    title: &'a str,
    id: &'a TaskId,
}

/// What a card's second line says before the kitten's own last words: where it works, and where it stands in line.
fn under_words(task: Option<&Task>) -> String {
    let Some(task) = task else { return "Shared folder".to_owned() };
    match order_words(task) {
        Some(after) => format!("{} \u{b7} {after}", place_words(task)),
        None => place_words(task),
    }
}

/// One row of a card: `Kitten 1`, its title, its state at the right; where it works (and "After Kitten 1") and its last line under;
/// Open. A draft is a different row: its prompt to edit, Start and Discard.
fn card_row(s: &App, t: &Theme, r: &Row, first: bool) -> El {
    let task = s.tasks.iter().find(|x| &x.id == r.id);
    if task.is_some_and(|x| x.subagent.as_ref().is_some_and(|o| o.draft)) {
        return draft_row(s, t, r, first, task);
    }
    let st = task.map_or(TaskStatus::New, |x| x.status);
    let who = name(r.number);
    let word = task.map_or_else(|| crate::subagents::card_word(st), kitten_word);
    let discarded = task.and_then(|x| x.subagent.as_ref()).is_some_and(|o| o.discarded);
    let last = if discarded { None } else { s.subagent_line(r.id) };
    let place = under_words(task);
    // While it waits on an answer, its question is what the card says under its name.
    let asking = task.and_then(|x| question_line(x, s.parent_of(x)));
    // A question still with the agents above waits on them, not on the person: the still ring and the quiet ink, never amber.
    let with_agent = task.and_then(|x| x.subagent.as_ref()?.question.as_ref()).is_some_and(|q| q.with != neko_protocol::QuestionWith::Person);
    let head = row()
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .child(text(who.clone()).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()).a11y_hidden())
        .child(text(r.title.to_owned()).font(t.ty.base).color(t.ink_muted()).truncate().min_w(0.0).grow(1.0).a11y_hidden())
        .child(if discarded { el().size(0.0, 0.0) } else if with_agent { still_ring(t, t.control.icon - 2.0) } else { status_glyph(t, st, t.control.icon - 2.0) })
        .child(text(word).font(weight(t.ty.sm, t.ty.medium)).color(if with_agent { t.ink_muted() } else { word_ink(t, st) }).a11y_hidden());
    let mut under = row().w_full().gap(t.space.at(1.5)).center_y();
    if let Some(q) = &asking {
        under = under.child(text(q.clone()).key(format!("subagent-question-{}", r.number)).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink().a11y_hidden());
    } else {
        under = under.child(text(place.clone()).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink().a11y_hidden());
        if let Some(l) = &last {
            under = under.child(text("\u{b7}").font(t.ty.sm).color(t.ink_faint()).a11y_hidden()).child(text(l.clone()).key(format!("subagent-line-{}", r.number)).font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0).shrink().a11y_hidden());
        }
    }
    let id = r.id.clone();
    // Open puts its conversation in the right pane as a tab; with Command held it takes the main view. A discarded draft never ran: it
    // has nothing to open.
    let open = (!discarded).then(|| {
        named_button(
            t,
            ButtonVariant::Secondary,
            &format!("subagent-open-{}", r.number),
            "Open",
            &format!("Open {who}"),
            Rc::new(move |s: &mut App, tx: &mut Tx| if tx.mods().cmd { s.open_kitten_main(&id) } else { s.open_kitten_tab(&id) }),
        )
    });
    let said = match (&asking, &last) {
        (Some(q), _) => format!("{who}: {}, {word}. {q}", r.title),
        (None, Some(l)) => format!("{who}: {}, {word}. {place}. {l}", r.title),
        (None, None) => format!("{who}: {}, {word}. {place}.", r.title),
    };
    let mut line = row()
        .key(format!("subagent-card-{}", r.number))
        .role("listitem")
        .label(said)
        .w_full()
        .h(ROW_H)
        .px(t.space.at(3.0))
        .gap(t.space.at(3.0))
        .center_y()
        .child(col().grow(1.0).min_w(0.0).gap(2.0).child(head).child(under));
    if let Some(o) = open {
        line = line.child(o);
    }
    if !first {
        line = line.border_sides([0.5, 0.0, 0.0, 0.0], t.c.line);
    }
    line
}

/// A draft kitten's row: the thread wrote it down and the person starts it. Its prompt is theirs to change, in a text box of its own;
/// Start sends it as it stands; Discard throws it away. It is not waited on and it has nothing to open yet.
fn draft_row(s: &App, t: &Theme, r: &Row, first: bool, task: Option<&Task>) -> El {
    let who = name(r.number);
    let place = under_words(task);
    let head = row()
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .child(text(who.clone()).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()).a11y_hidden())
        .child(text(r.title.to_owned()).font(t.ty.base).color(t.ink_muted()).truncate().min_w(0.0).grow(1.0).a11y_hidden())
        .child(text("Draft").font(weight(t.ty.sm, t.ty.medium)).color(t.ink_muted()).a11y_hidden());
    let under = text(place.clone()).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).a11y_hidden();
    let id = r.id.clone();
    let fallback = TextEdit::new(task.map_or("", |x| x.words.as_str()));
    let edit = s.draft_edits.get(r.id).unwrap_or(&fallback);
    let eid = id.clone();
    let prompt = Textarea::new(format!("draft-prompt-{}", r.number), edit, move |s: &mut App| s.draft_edit(&eid)).name(format!("Prompt for {who}")).placeholder("What it should do, in full: it sees nothing else.").rows(2, 6).build(t);
    let (sid, did) = (id.clone(), id.clone());
    let start = named_button(t, ButtonVariant::Secondary, &format!("draft-start-{}", r.number), "Start", &format!("Start {who}"), Rc::new(move |s: &mut App, _: &mut Tx| s.start_draft(&sid)));
    let discard = named_button(t, ButtonVariant::Ghost, &format!("draft-discard-{}", r.number), "Discard", &format!("Discard {who}"), Rc::new(move |s: &mut App, _: &mut Tx| s.discard_draft(&did)));
    let buttons = row().w_full().gap(t.space.at(2.0)).center_y().justify_end().child(discard).child(start);
    let mut line = col()
        .key(format!("subagent-card-{}", r.number))
        .role("listitem")
        .label(format!("{who}: {}, Draft. {place}.", r.title))
        .w_full()
        .px(t.space.at(3.0))
        .py(t.space.at(2.5))
        .gap(t.space.at(2.0))
        .child(col().w_full().gap(2.0).child(head).child(under))
        .child(prompt)
        .child(buttons);
    if !first {
        line = line.border_sides([0.5, 0.0, 0.0, 0.0], t.c.line);
    }
    line
}

/// Kitten parts that follow each other, as one raised card with a row each.
pub fn card_stack(s: &App, t: &Theme, parts: &[&Part]) -> Option<El> {
    let rows: Vec<Row> = parts
        .iter()
        .filter_map(|p| match &p.body {
            PartBody::Subagent { subagent, number, title } => Some(Row { number, title, id: subagent }),
            _ => None,
        })
        .collect();
    let first = rows.first()?;
    let label = if rows.len() == 1 { "Kitten".to_owned() } else { "Kittens".to_owned() };
    let mut card = col().key(format!("subagent-cards-{}", first.number)).role("list").label(label).w_full().radius(t.radius.lg + 2.0).round_as(t).clip_rounded(t.radius.lg + 2.0);
    for (i, r) in rows.iter().enumerate() {
        card = card.child(card_row(s, t, r, i == 0));
    }
    // A little air above and below: the card is the one raised thing between lines of steps and words.
    Some(crate::turn::settling(raised(t, card)).my(t.space.at(1.0)))
}

impl App {
    /// Settings: ask before a thread's first kitten.
    pub fn set_ask_first_subagent(&mut self, on: bool) {
        if let Some((_, s)) = &self.runtimes {
            let settings = neko_protocol::Settings { ask_first_subagent: on, ..s.clone() };
            self.send(neko_protocol::Request::SetSettings { settings });
        }
    }
}

/// The note where a subagent's report woke the thread: a centred line with a hairline each side, and under it the report, folded.
pub fn wake_note(s: &App, t: &Theme, p: &Part) -> Option<El> {
    let PartBody::Wake { number, status, summary, files, branch, question, .. } = &p.body else { return None };
    let said = wake_line(number, status);
    let mut note = col().w_full().gap(0.0).child(centred_note(t, &crate::task_view::kk(format!("wake-{}", p.seq)), &said));
    let asked = question.as_deref().filter(|_| status == "asked");
    if summary.trim().is_empty() && files.is_empty() && branch.is_none() && asked.is_none() {
        return Some(note);
    }
    let key = crate::task_view::kk(format!("wake-report-{}", p.seq));
    let open = s.is_open(&key);
    let k = key.clone();
    let title = if asked.is_some() { "The question".to_owned() } else { format!("Report from {}", name(number)) };
    let fold = FoldLine::<App>::new(key.clone(), title, open).lead(crate::ui::agent::hollow_dot(t, t.control.icon)).on_toggle(move |s: &mut App| s.flip(&k));
    let mut body = Vec::new();
    if open {
        let mut inner = col().key(format!("{key}-body")).w_full().gap(t.space.at(3.0)).pl(t.space.at(1.5));
        if let Some(q) = asked {
            inner = inner.child(text(q.to_owned()).key(format!("{key}-question")).role("note").font(t.ty.base).color(t.ink()).wrap().min_w(0.0).selectable(true));
        }
        if !summary.trim().is_empty() && asked.is_none_or(|q| q.trim() != summary.trim()) {
            inner = inner.child(Response::new(format!("{key}-summary"), summary.clone()).selectable(true).muted(true).column(COLUMN).build(t));
        }
        if !files.is_empty() {
            let mut list = col().key(format!("{key}-files")).role("list").label("Files changed").w_full().gap(2.0);
            for f in files {
                list = list.child(row().role("listitem").label(f.clone()).w_full().gap(t.space.at(2.0)).center_y().child(glyph(&icons::FILE, t.control.icon_sm, t.ink_faint())).child(text(f.clone()).font(mono(t, t.ty.sm)).color(t.ink()).truncate_middle().min_w(0.0).shrink().a11y_hidden()));
            }
            inner = inner.child(list);
        }
        if let Some(b) = branch {
            inner = inner.child(row().key(format!("{key}-branch")).role("note").label(format!("Branch {b}")).w_full().gap(t.space.at(2.0)).center_y().child(glyph(&icons::COMMIT, t.control.icon_sm, t.ink_faint())).child(text(b.clone()).font(mono(t, t.ty.sm)).color(t.ink_muted()).truncate_middle().min_w(0.0).shrink().a11y_hidden()));
        }
        body.push(inner);
    }
    note = note.child(fold.build(t, body));
    Some(note)
}

/// A centred xs line in the second ink with a hairline on each side: how the conversation says something happened between turns.
pub fn centred_note(t: &Theme, key: &str, said: &str) -> El {
    let hair = || el().grow(1.0).h(1.0).fill(t.c.line);
    row()
        .key(key.to_owned())
        .role("note")
        .label(said.to_owned())
        .w_full()
        .h(t.control.md)
        .gap(t.space.at(3.0))
        .center_y()
        .child(hair())
        .child(text(said.to_owned()).font(t.ty.xs).color(t.ink_muted()).a11y_hidden())
        .child(hair())
}

/// The note where the agent wrote to one of its kittens: "The agent answered Kitten 1" (it was waiting on a question) or "The agent
/// wrote to Kitten 1", and under it what it said, folded. `subject` is who wrote ("The agent", or "Kitten 1" for a kitten's own).
pub fn to_kitten_note(s: &App, t: &Theme, p: &Part, subject: &str) -> Option<El> {
    let PartBody::ToSubagent { number, text: words, answer } = &p.body else { return None };
    let said = format!("{subject} {} {}", if *answer { "answered" } else { "wrote to" }, name(number));
    let key = crate::task_view::kk(format!("to-kitten-{}", p.seq));
    let open = s.is_open(&key);
    let k = key.clone();
    let fold = FoldLine::<App>::new(key.clone(), if *answer { "The answer".to_owned() } else { "What it said".to_owned() }, open).lead(crate::ui::agent::hollow_dot(t, t.control.icon)).on_toggle(move |s: &mut App| s.flip(&k));
    let body = if open { vec![col().key(format!("{key}-body")).w_full().pl(t.space.at(1.5)).child(Response::new(format!("{key}-words"), words.clone()).selectable(true).muted(true).column(COLUMN).build(t))] } else { Vec::new() };
    Some(col().w_full().gap(0.0).child(centred_note(t, &crate::task_view::kk(format!("to-kitten-line-{}", p.seq)), &said)).child(fold.build(t, body)))
}

/// The question a conversation ends on while it waits for an answer, led by a small amber mark: a thread's that asked the person ("Kitten
/// 1 asks (via the agent)", or "The agent asks"), and a kitten's tab while it waits on its parent ("Kitten 1 asks the agent"). Who has it
/// is the quiet line under it. None when nothing is asked with Neko's tools.
pub fn question_block(s: &App, t: &Theme, task: &Task, last: Option<&neko_protocol::Run>) -> Option<El> {
    if task.status != TaskStatus::Asked {
        return None;
    }
    let (head, words, under) = match &task.subagent {
        Some(of) => {
            let q = of.question.as_ref()?;
            let whom = crate::subagents::asked_whom(s.parent_of(task));
            let under = match q.with {
                neko_protocol::QuestionWith::Parent => format!("Waiting for {whom} to answer."),
                neko_protocol::QuestionWith::Up => format!("{} passed it up.", capital(&whom)),
                neko_protocol::QuestionWith::Person => format!("{} passed it to you in Needs you.", capital(&whom)),
            };
            (format!("{} asks {whom}", name(&of.number)), q.text.clone(), Some(under))
        }
        None => {
            let a = last?.ask.as_ref().filter(|a| a.to == neko_protocol::AskTo::Person)?;
            let (kitten, via) = match a.chain.split_first() {
                Some((k, rest)) => (Some(k.as_str()), rest.to_vec()),
                None => (None, Vec::new()),
            };
            let head = if kitten.is_some() { crate::subagents::question_says(kitten, &via) } else { "The agent asks".to_owned() };
            let under = a.unanswered.then(|| "The agent did not answer it, so Neko passed it to you.".to_owned());
            (head, a.text.clone(), under)
        }
    };
    let label = match &under {
        Some(u) => format!("{head}: {words} {u}"),
        None => format!("{head}: {words}"),
    };
    let mark = row().size(t.control.icon, t.ty.base.line_height.max(t.control.icon)).center().child(el().size(8.0, 8.0).radius(4.0).fill(t.c.warning.fg)).a11y_hidden();
    let mut lines = col()
        .min_w(0.0)
        .shrink()
        .gap(t.space.at(1.0))
        .child(text(head).font(weight(t.ty.sm, t.ty.semibold)).color(t.c.warning.fg).a11y_hidden())
        .child(text(words).key(crate::task_view::kk("question-words".into())).font(t.ty.md).color(t.ink()).wrap().min_w(0.0).selectable(true));
    if let Some(u) = under {
        lines = lines.child(text(u).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0).a11y_hidden());
    }
    Some(row().key(crate::task_view::kk("question-block".into())).role("note").label(label).w_full().pl(t.space.at(1.5)).py(t.space.at(1.0)).gap(t.metrics.gap).items_start().child(mark).child(lines))
}

fn capital(s: &str) -> String {
    let mut c = s.chars();
    c.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + c.as_str())
}

/// What a thread that rests on its subagents waits on, in words ("Waiting on Kitten 1 and Kitten 2"): None when it is not resting.
/// The run that ended before it says this on its own line, so it never reads "done" while its kittens work.
pub fn waiting_words(s: &App, task: &Task) -> Option<String> {
    if task.status != TaskStatus::Waiting || task.subagent.is_some() {
        return None;
    }
    let names: Vec<String> = s.live_subagents(&task.id).iter().filter_map(|x| x.subagent_name()).collect();
    // Resting on its timers and watches alone: they say it (`waits.rs`).
    if names.is_empty() && !task.waits.is_empty() {
        return None;
    }
    Some(if names.is_empty() { "Reading the reports".to_owned() } else { format!("Waiting on {}", names_sentence(&names)) })
}

/// The conversation's last line while the open thread waits: what it waits on, each name a link that opens that subagent.
pub fn waiting_line(s: &App, t: &Theme, task: &Task) -> Option<El> {
    if task.status != TaskStatus::Waiting || task.subagent.is_some() {
        return None;
    }
    let live = s.live_subagents(&task.id);
    if live.is_empty() && !task.waits.is_empty() {
        return None;
    }
    let names: Vec<String> = live.iter().filter_map(|x| x.subagent_name()).collect();
    let sentence = if names.is_empty() { "Reading the reports".to_owned() } else { format!("Waiting on {}", names_sentence(&names)) };
    let mut line = row().key("waiting-line").role("status").label(sentence).w_full().min_h(t.control.lg).pl(t.space.at(1.5)).gap(0.0).center_y().flex_wrap().child(row().size(t.control.icon, t.control.icon).mr(t.metrics.gap).center().child(still_ring(t, t.control.icon - 2.0).a11y_hidden()));
    let words = |w: String| text(w).font(t.ty.base).color(t.ink_muted()).a11y_hidden();
    if live.is_empty() {
        return Some(line.child(words("Reading the reports".to_owned())));
    }
    line = line.child(words("Waiting on ".to_owned()));
    let n = live.len();
    for (i, x) in live.iter().enumerate() {
        let Some(num) = x.subagent.as_ref().map(|o| o.number.clone()) else { continue };
        let id = x.id.clone();
        if i > 0 {
            line = line.child(words(if i == n - 1 { " and ".to_owned() } else { ", ".to_owned() }));
        }
        line = line.child(link(t, &format!("waiting-link-{num}"), &name(&num), move |s: &mut App| s.open_kitten_tab(&id)));
    }
    Some(line)
}

/// What stands where the composer would: a subagent is read, not talked to. A quiet line, Back to its thread, and Stop while it works.
pub fn readonly_foot(s: &App, t: &Theme, task: &Task) -> El {
    let who = task.subagent_name().unwrap_or_default();
    let thread = s.thread_of(task).map_or_else(|| "its thread".to_owned(), |x| x.title.clone());
    let parent = s.parent_of(task);
    let back_to = match (parent, s.thread_of(task)) {
        (Some(p), Some(th)) if p.id != th.id => p.subagent_name().unwrap_or_else(|| p.title.clone()),
        _ => thread.clone(),
    };
    let said = format!("This is {who}'s own conversation, to read. Say what you want changed in {thread}.");
    let mut buttons = row().w_full().gap(t.space.at(2.0)).center_y().flex_wrap();
    buttons = buttons.child(Button::<App>::new("subagent-back", format!("Back to {back_to}")).variant(ButtonVariant::Secondary).size(crate::ui::base::Size::Sm).icon(&icons::CHEVRON_LEFT).on_press(|s: &mut App| s.open_parent()).build(t));
    if crate::subagents::is_live(task.status) {
        buttons = buttons.child(Button::<App>::new("subagent-stop", format!("Stop {who}")).variant(ButtonVariant::Ghost).size(crate::ui::base::Size::Sm).icon(&icons::STOP).on_press(|s: &mut App| s.stop()).build(t));
    }
    col()
        .key("subagent-foot")
        .w_full()
        .max_w(COLUMN)
        .gap(t.space.at(2.5))
        .child(text(said.clone()).key("subagent-readonly").role("note").label(said).font(t.ty.sm).color(t.ink_muted()).wrap().min_w(0.0))
        .child(buttons)
}

/// Cuts a title to `max` characters at its end, for the band.
fn cut_end(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    format!("{}…", s.chars().take(max.saturating_sub(1)).collect::<String>().trim_end())
}

/// The band's title for a subagent: the breadcrumb, `Fix login / Kitten 1`. The thread's title is a button that goes to it; the
/// subagent is plain words. The whole is the page's heading (its name is the full breadcrumb), keyed `band-title`.
pub fn crumb(s: &App, t: &Theme, task: &Task, narrow: bool) -> El {
    let crumbs = s.crumbs(task);
    let full = s.crumb_words(task);
    let n = crumbs.len();
    let mut line = row().key("band-title").role("heading").level(1).label(full).gap(t.space.at(1.5)).center_y().min_w(crate::view::TITLE_MIN).shrink();
    for (i, (id, words)) in crumbs.into_iter().enumerate() {
        if i > 0 {
            line = line.child(text("/").font(weight(t.ty.lg, t.ty.regular)).color(t.ink_faint()).a11y_hidden());
        }
        let last = i + 1 == n;
        if last {
            line = line.child(text(words).key("crumb-subagent").font(weight(t.ty.lg, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).a11y_hidden());
            continue;
        }
        let key = if i == 0 { "crumb-thread".to_owned() } else { format!("crumb-subagent-{}", words.trim_start_matches("Kitten ")) };
        let shown = if i == 0 { cut_end(&words, if narrow { 18 } else { 36 }) } else { words.clone() };
        let mut b = Button::<App>::new(key, shown).variant(ButtonVariant::Ghost).size(crate::ui::base::Size::Sm);
        if let Some(id) = id {
            b = b.on_press(move |s: &mut App| s.open_task_id(&id));
        }
        line = line.child(b.build(t));
    }
    line
}

/// What leads a subagent where a cat's portrait would (Needs you): its number on the quiet ground a plugin's tile has, with the amber
/// dot, taking the room a portrait takes.
pub fn tile(t: &Theme, key: &str, size: f32, number: &str, under: rune::Color) -> El {
    use crate::ui::portrait::{Activity, Portrait};
    let face = row().size(size, size).center().child(text(number.to_owned()).font(weight(t.ty.md, t.ty.semibold)).color(crate::plugins_view::tile_ink(t)).a11y_hidden());
    Portrait::new(key, name(number), size).face(face).ground(crate::plugins_view::tile_ground(t)).state(Activity::NeedsYou).ringed(true).under(under).decorative(true).build(t)
}

/// Words that open something: the identity ink, a link's role, the row's hover and focus ring, no padding of a button's.
fn link(t: &Theme, key: &str, words: &str, press: impl Fn(&mut App) + 'static) -> El {
    let face = row().key(key.to_owned()).role("link").label(words.to_owned()).focusable(true).px(2.0).radius(t.radius.sm).round_as(t).center_y();
    paint_state(face, &t.mat.row, crate::ui::base::States::default()).child(text(words.to_owned()).font(t.ty.base).color(t.c.accent.fg).a11y_hidden()).on_press(move |s: &mut App| press(s))
}
