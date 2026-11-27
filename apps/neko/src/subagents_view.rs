//! Subagents in the conversation (docs/knowledge/app-shell.md and design.md, "Subagents in the app"): the card where a thread started
//! them, the note where one's report woke the thread, the line that says what the thread waits on, the read-only foot of a
//! subagent opened on its own, and the band's breadcrumb. The words and the state are `subagents.rs`.

use std::rc::Rc;

use neko_protocol::{Part, PartBody, Task, TaskId, TaskStatus};
use rune::{El, col, el, row, text};
use rune_ui_style::{ButtonVariant, weight};

use crate::state::App;
use crate::subagents::{card_word, name, names_sentence, wake_line};
use crate::ui::agent::{FoldLine, Marks, mark_with, mono};
use crate::ui::base::{Shape, SizeExt, Theme, glyph, hit_area, paint_state, recipes, states};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::response::Response;
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

/// A secondary button whose visible words are short and whose name for assistive technology says more ("Open", "Open Subagent 1").
fn named_button(t: &Theme, key: &str, words: &str, name: &str, press: impl Fn(&mut App) + 'static) -> El {
    let size = crate::ui::base::Size::Sm;
    let parts = recipes::button(t, ButtonVariant::Secondary);
    let st = states(None, false, false);
    let ink = parts.label.resolve(st);
    let pad = size.pad(t);
    let f = Rc::new(press);
    let g = f.clone();
    let mut face = row().key(key.to_owned()).role("button").label(name.to_owned()).focusable(true).h(size.height(t)).pl(pad).pr(pad).center().radius(size.radius(t)).round_as(t);
    face = paint_state(face, &parts.root, st).child(text(words.to_owned()).font(size.text(t, t.ty.medium)).color(ink).a11y_hidden());
    face = face.on_press(move |s: &mut App| g(s));
    let press: crate::ui::base::Press<App> = crate::ui::base::press(move |s: &mut App| f(s));
    hit_area(key, face, 0.0, t.control.slop(size.height(t)), Some(press))
}

/// What a subagent's card knows: its task (if the window has it), its words.
struct Row<'a> {
    number: &'a str,
    title: &'a str,
    id: &'a TaskId,
}

/// One row of a card: `Subagent 1`, its title, its state at the right; "Shared folder" and its last line under; Open.
fn card_row(s: &App, t: &Theme, r: &Row, first: bool) -> El {
    let task = s.tasks.iter().find(|x| &x.id == r.id);
    let st = task.map_or(TaskStatus::New, |x| x.status);
    let who = name(r.number);
    let word = card_word(st);
    let last = s.subagent_line(r.id);
    let head = row()
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .child(text(who.clone()).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()).a11y_hidden())
        .child(text(r.title.to_owned()).font(t.ty.base).color(t.ink_muted()).truncate().min_w(0.0).grow(1.0).a11y_hidden())
        .child(status_glyph(t, st, t.control.icon - 2.0))
        .child(text(word).font(weight(t.ty.sm, t.ty.medium)).color(word_ink(t, st)).a11y_hidden());
    let mut under = row().w_full().gap(t.space.at(1.5)).center_y().child(text("Shared folder").font(t.ty.sm).color(t.ink_muted()).a11y_hidden());
    if let Some(l) = &last {
        under = under.child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden()).child(text(l.clone()).key(format!("subagent-line-{}", r.number)).font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0).shrink().a11y_hidden());
    }
    let id = r.id.clone();
    let open = named_button(t, &format!("subagent-open-{}", r.number), "Open", &format!("Open {who}"), move |s: &mut App| s.open_task_id(&id));
    let said = match &last {
        Some(l) => format!("{who}: {}, {word}. Shared folder. {l}", r.title),
        None => format!("{who}: {}, {word}. Shared folder.", r.title),
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
        .child(col().grow(1.0).min_w(0.0).gap(2.0).child(head).child(under))
        .child(open);
    if !first {
        line = line.border_sides([0.5, 0.0, 0.0, 0.0], t.c.line);
    }
    line
}

/// Subagent parts that follow each other, as one raised card with a row each.
pub fn card_stack(s: &App, t: &Theme, parts: &[&Part]) -> Option<El> {
    let rows: Vec<Row> = parts
        .iter()
        .filter_map(|p| match &p.body {
            PartBody::Subagent { subagent, number, title } => Some(Row { number, title, id: subagent }),
            _ => None,
        })
        .collect();
    let first = rows.first()?;
    let label = if rows.len() == 1 { "Subagent".to_owned() } else { "Subagents".to_owned() };
    let mut card = col().key(format!("subagent-cards-{}", first.number)).role("list").label(label).w_full().radius(t.radius.lg + 2.0).round_as(t).clip_rounded(t.radius.lg + 2.0);
    for (i, r) in rows.iter().enumerate() {
        card = card.child(card_row(s, t, r, i == 0));
    }
    // A little air above and below: the card is the one raised thing between lines of steps and words.
    Some(crate::turn::settling(raised(t, card)).my(t.space.at(1.0)))
}

/// The note where a subagent's report woke the thread: a centred line with a hairline each side, and under it the report, folded.
pub fn wake_note(s: &App, t: &Theme, p: &Part) -> Option<El> {
    let PartBody::Wake { number, status, summary, files, branch, .. } = &p.body else { return None };
    let said = wake_line(number, status);
    let hair = || el().grow(1.0).h(1.0).fill(t.c.line);
    let line = row()
        .key(format!("wake-{}", p.seq))
        .role("note")
        .label(said.clone())
        .w_full()
        .h(t.control.md)
        .gap(t.space.at(3.0))
        .center_y()
        .child(hair())
        .child(text(said).font(t.ty.xs).color(t.ink_muted()).a11y_hidden())
        .child(hair());
    let mut note = col().w_full().gap(0.0).child(line);
    if summary.trim().is_empty() && files.is_empty() && branch.is_none() {
        return Some(note);
    }
    let key = format!("wake-report-{}", p.seq);
    let open = s.is_open(&key);
    let k = key.clone();
    let fold = FoldLine::<App>::new(key.clone(), format!("Report from {}", name(number)), open).lead(crate::ui::agent::hollow_dot(t, t.control.icon)).on_toggle(move |s: &mut App| s.flip(&k));
    let mut body = Vec::new();
    if open {
        let mut inner = col().key(format!("{key}-body")).w_full().gap(t.space.at(3.0)).pl(t.space.at(1.5));
        if !summary.trim().is_empty() {
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

/// The conversation's last line while the open thread waits: what it waits on, each name a link that opens that subagent.
pub fn waiting_line(s: &App, t: &Theme, task: &Task) -> Option<El> {
    if task.status != TaskStatus::Waiting || task.subagent.is_some() {
        return None;
    }
    let live = s.live_subagents(&task.id);
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
        line = line.child(link(t, &format!("waiting-link-{num}"), &name(&num), move |s: &mut App| s.open_task_id(&id)));
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

/// The band's title for a subagent: the breadcrumb, `Fix login / Subagent 1`. The thread's title is a button that goes to it; the
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
        let key = if i == 0 { "crumb-thread".to_owned() } else { format!("crumb-subagent-{}", words.trim_start_matches("Subagent ")) };
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
