//! The Briefing page (docs/knowledge/briefing.md, ⌘4): what happened while the person was away, read in five seconds. Needs you comes
//! first, with the very cards Needs you has (answered where they are), then the Cattains whose digest changed (each led by its face, with
//! its Needs you and New lines in its own words) and those made by asking, then what finished (each led by its status mark, with the first
//! sentence of its answer, its files, and Review and Undo where Neko already offers them), what the rounds did, what is still
//! working, a totals line, and "Mark as read". After that the page says the person is caught up and when the next briefing is.
//!
//! Keys: Up and Down move between the rows of a list, Return opens a row's task, Escape goes back, ⌘↩ marks it read. Tab reaches each
//! row's own controls, in reading order, ending on "Mark as read".

use neko_protocol::briefing::{BriefingCattain, BriefingNewCattain, BriefingRound, BriefingTask, BriefingWorking, FinishedEnd, RoundOutcome};
use neko_protocol::undo::Action;
use neko_protocol::{Briefing, TaskStatus};
use rune::{El, Key, KeyEvent, Tx, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size, tabular, weight};

use crate::briefing::{self as words, active_workspaces, group_by, task_key};
use crate::faces::portrait_in;
use crate::state::App;
use crate::ui::base::{Shape, States, Theme, paint_state};
use crate::ui::button::Button;
use crate::ui::empty::Empty;
use crate::ui::icons;
use crate::ui::kbd::Kbd;
use crate::view::{COLUMN, well};

/// The sidebar row's and the page's key.
pub const KEY: &str = "briefing";

/// How far down a finished row its mark sits: on the title's line, under the line that says how it ended.
const MARK_TOP: f32 = 33.0;
/// Every row's lead is this wide (a Cattain's 24 point face, a thread's mark centred in the same room): each list's words line up.
const LEAD: f32 = 24.0;

/// A section's head: its title and how many, the count in the section's own ink.
fn head(t: &Theme, key: &str, title: &str, count: usize, ink: rune::Color) -> El {
    row()
        .key(format!("{key}-head"))
        .w_full()
        .gap(t.space.at(2.0))
        .center_y()
        .child(text(title.to_owned()).key(format!("{key}-title")).role("heading").level(2).font(weight(t.ty.base, t.ty.semibold)).color(t.ink()))
        .child(text(count.to_string()).key(format!("{key}-count")).font(tabular(weight(t.ty.sm, t.ty.medium))).color(ink).a11y_hidden())
}

/// A workspace's name as a small heading, over its part of a section.
fn workspace_head(s: &App, t: &Theme, key: &str, ws: &str) -> El {
    let name = crate::needs_view::group_name(s, ws);
    text(name).key(format!("{key}-ws-{ws}")).role("heading").level(3).font(weight(t.ty.sm, t.ty.medium)).color(t.ink_muted()).truncate().min_w(0.0)
}

/// A well of rows: one surface, hairlines between.
fn rows_well(t: &Theme, key: &str, label: &str, rows: Vec<El>) -> El {
    let l = col().key(key.to_owned()).role("list").label(label.to_owned()).w_full().radius(t.radius.xl).round_as(t);
    let l = well(t, l).fill(t.c.input.bg);
    rows.into_iter().enumerate().fold(l, |l, (i, r)| l.child(if i > 0 { r.border_sides([0.5, 0.0, 0.0, 0.0], t.c.line) } else { r }))
}

/// Up and Down to the row before and after, Return opens the row's task, Escape goes back.
fn row_keys(s: &mut App, ev: &KeyEvent, tx: &mut Tx, task: &neko_protocol::TaskId, before: Option<&str>, after: Option<&str>) -> bool {
    if ev.mods.cmd || ev.mods.alt || ev.mods.ctrl || ev.mods.shift {
        return false;
    }
    match ev.key {
        Key::Up | Key::Down => {
            if let Some(k) = if ev.key == Key::Up { before } else { after } {
                tx.focus(k.to_owned(), true);
            }
            true
        }
        Key::Enter => {
            s.open_task_id(task);
            true
        }
        Key::Escape => {
            s.leave_briefing();
            true
        }
        _ => false,
    }
}

/// A row's lead: the thread's status mark in its 16 point slot, `top` down so it sits on the row's title line (a face now means a
/// Cattain, so no thread is led by one). A thread the window does not know (it was removed since) keeps the slot, with the red dot
/// when it could not finish.
fn mark_lead(s: &App, t: &Theme, key: &str, task: &neko_protocol::TaskId, failed: bool, top: f32) -> El {
    let under = t.c.input.bg;
    let mark = match s.tasks.iter().find(|x| &x.id == task) {
        Some(task) => crate::status_mark::bare_mark(t, key, task, under, crate::status_mark::SLOT),
        None => {
            let slot = el().key(key.to_owned()).size(crate::status_mark::SLOT, crate::status_mark::SLOT).center().a11y_hidden();
            if failed { slot.child(el().size(8.0, 8.0).radius(4.0).fill(crate::status_mark::ink(t, crate::status_mark::Mark::Failed, under).unwrap_or(t.c.danger.fg))) } else { slot }
        }
    };
    col().w(LEAD).min_w(LEAD).items_center().child(mark.mt(top)).a11y_hidden()
}

/// A Cattain's 24 point face as a row's lead, in the same room a thread's mark has.
fn face_lead(t: &Theme, key: &str, face: &str) -> El {
    col().w(LEAD).min_w(LEAD).items_center().child(portrait_in(t, key, face, LEAD, None, t.c.input.bg))
}

/// "miso's crew" for a thread in a Cattain's crew, else nothing.
fn crew_words(s: &App, task: &neko_protocol::TaskId) -> Option<String> {
    match s.whose(task) {
        crate::notify::Whose::Crew(name) => Some(format!("{name}'s crew")),
        _ => None,
    }
}

/// Up and Down between the Cattains section's rows, Return does the row's press, Escape goes back.
fn cattain_row_keys(s: &mut App, ev: &KeyEvent, tx: &mut Tx, open: &dyn Fn(&mut App), before: Option<&str>, after: Option<&str>) -> bool {
    if ev.mods.cmd || ev.mods.alt || ev.mods.ctrl || ev.mods.shift {
        return false;
    }
    match ev.key {
        Key::Up | Key::Down => {
            if let Some(k) = if ev.key == Key::Up { before } else { after } {
                tx.focus(k.to_owned(), true);
            }
            true
        }
        Key::Enter => {
            open(s);
            true
        }
        Key::Escape => {
            s.leave_briefing();
            true
        }
        _ => false,
    }
}

/// A label over a part of a Cattain's block ("Needs you", "New"), and its lines in the Cattain's own words.
fn digest_part(t: &Theme, key: &str, label: &str, ink: rune::Color, line_ink: rune::Color, lines: &[String], more: u32) -> El {
    let mut part = col().key(key.to_owned()).role("list").label(format!("{label}, in its digest")).w_full().gap(t.space.at(1.0));
    part = part.child(text(label.to_owned()).key(format!("{key}-label")).font(weight(t.ty.xs, t.ty.semibold)).color(ink).a11y_hidden());
    for (i, l) in lines.iter().enumerate() {
        part = part.child(text(l.clone()).key(format!("{key}-{i}")).role("listitem").label(l.clone()).font(t.ty.base).color(line_ink).wrap().min_w(0.0));
    }
    if let Some(m) = words::more_words(more) {
        part = part.child(text(m).key(format!("{key}-more")).font(t.ty.sm).color(t.ink_faint()));
    }
    part
}

/// One Cattain whose digest changed: its face and name, when it wrote, its digest's Needs you and New lines quoted as it wrote them,
/// and Open digest (its thread on the Digest tab). A Cattain made by asking in the window says so here instead of on a line of its own.
#[allow(clippy::too_many_arguments)]
fn digest_block(s: &App, t: &Theme, c: &BriefingCattain, made: Option<&BriefingNewCattain>, pos: usize, n: usize, before: Option<String>, after: Option<String>) -> El {
    let key = words::digest_key(&c.cat);
    let zone = s.zone();
    let when = words::when(c.changed_ms, s.now_ms, &*zone);
    let cat = c.cat.clone();
    let open_digest = move |s: &mut App| s.open_cattain_on(&cat, crate::pane::Tab::Digest);
    let open = {
        let open_digest = open_digest.clone();
        Button::new(format!("{key}-open"), "Open digest").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(open_digest).build(t)
    };
    let mut who = row().key(format!("{key}-who")).w_full().gap(t.space.at(1.5)).center_y().child(text(c.name.clone()).key(format!("{key}-name")).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0).shrink().a11y_hidden());
    if made.is_some() {
        who = who.child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden()).child(text("New Cattain").key(format!("{key}-made")).font(t.ty.sm).color(t.ink_muted()).a11y_hidden());
    }
    who = who
        .child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden())
        .child(text(format!("wrote {when}")).key(format!("{key}-when")).font(tabular(t.ty.sm)).color(t.ink_faint()).truncate().min_w(0.0).shrink().a11y_hidden())
        .child(el().grow(1.0))
        .child(open);
    let mut body = col().grow(1.0).min_w(0.0).gap(t.space.at(2.5)).child(who.mt(-t.space.at(0.5)).mb(-t.space.at(1.0)));
    if !c.needs.is_empty() {
        body = body.child(digest_part(t, &format!("{key}-needs"), "Needs you", t.c.warning.fg, t.ink(), &c.needs, c.needs_more));
    }
    if !c.new.is_empty() {
        body = body.child(digest_part(t, &format!("{key}-new"), "New", t.ink_muted(), t.ink_muted(), &c.new, c.new_more));
    }
    if c.needs.is_empty() && c.new.is_empty() {
        body = body.child(text("Nothing under Needs you or New.").key(format!("{key}-quiet")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0));
    }
    let mut label = format!("{}'s digest", c.name);
    if made.is_some() {
        label = format!("{label}, a new Cattain");
    }
    let says = |what: &str, v: &[String]| (!v.is_empty()).then(|| format!("{what}: {}", v.join("; ")));
    let desc = [says("Needs you", &c.needs), says("New", &c.new), Some(format!("written {when}"))].into_iter().flatten().collect::<Vec<_>>().join(". ");
    let r = row().key(key.clone()).role("listitem").label(label).description(desc).position_in_set(pos, n).focusable(true).w_full().px(t.space.at(4.0)).py(t.space.at(3.5)).gap(t.space.at(3.5)).items_start();
    paint_state(r, &t.mat.row, States::default())
        .child(face_lead(t, &format!("{key}-face"), s.cat_face(&c.cat)))
        .child(body)
        .on_key(move |s: &mut App, ev: &KeyEvent, tx: &mut Tx| cattain_row_keys(s, ev, tx, &open_digest, before.as_deref(), after.as_deref()))
}

/// "New Cattain: yuzu (iOS work)", led by its face, with Open thread (its own thread), once in the window it was made.
fn new_cattain_row(s: &App, t: &Theme, m: &BriefingNewCattain, pos: usize, n: usize, before: Option<String>, after: Option<String>) -> El {
    let key = words::new_cattain_key(&m.cat);
    let zone = s.zone();
    let when = words::when(m.made_ms, s.now_ms, &*zone);
    let cat = m.cat.clone();
    let open_it = move |s: &mut App| s.open_cattain(&cat);
    let open = {
        let open_it = open_it.clone();
        Button::new(format!("{key}-open"), "Open thread").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(open_it).build(t)
    };
    let line = words::new_cattain_words(m);
    let r = row().key(key.clone()).role("listitem").label(line.clone()).description(format!("made by asking {when}")).position_in_set(pos, n).focusable(true).w_full().px(t.space.at(4.0)).py(t.space.at(2.5)).gap(t.space.at(3.5)).center_y();
    paint_state(r, &t.mat.row, States::default())
        .child(face_lead(t, &format!("{key}-face"), s.cat_face(&m.cat)))
        .child(text(line).key(format!("{key}-words")).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink().a11y_hidden())
        .child(text(when).key(format!("{key}-when")).font(tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden())
        .child(el().grow(1.0))
        .child(open)
        .on_key(move |s: &mut App, ev: &KeyEvent, tx: &mut Tx| cattain_row_keys(s, ev, tx, &open_it, before.as_deref(), after.as_deref()))
}

/// What a finished task's second line says about its changes: "3 files" with its figures. The branch is the task's detail, not
/// the Briefing's: it shows in the task.
fn facts(t: &Theme, key: &str, item: &BriefingTask) -> Option<El> {
    let mut r = row().key(format!("{key}-facts")).gap(t.space.at(3.0)).center_y().min_w(0.0).flex_wrap();
    let mut any = false;
    if let Some(c) = item.changes {
        any = true;
        let mut files = row().gap(t.space.at(2.0)).center_y().role("note").label(words::changes_label(c)).child(text(words::files(c.files)).font(t.ty.sm).color(t.ink_muted()).a11y_hidden());
        files = files.children(crate::turn::file_figures(t, c.added, c.removed));
        r = r.child(files);
    }
    if item.offer.undone {
        any = true;
        r = r.child(text("Undone").key(format!("{key}-undone")).font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_muted()));
    }
    any.then_some(r)
}

/// One finished thread: how it ended and when, the title, the first sentence of the answer, the files, and
/// the actions the run's card offers.
fn finished_row(s: &App, t: &Theme, item: &BriefingTask, pos: usize, n: usize, before: Option<String>, after: Option<String>) -> El {
    let key = task_key(&item.task);
    let zone = s.zone();
    let failed = item.end == FinishedEnd::Failed;
    let says = words::end_words(item.end).to_owned();
    let when = words::when(item.ended_ms, s.now_ms, &*zone);
    let (task, run) = (item.task.clone(), item.run.clone());
    let open = {
        let task = task.clone();
        Button::new(format!("{key}-open"), "Open thread").variant(ButtonVariant::Ghost).size(Size::Sm).on_press(move |s: &mut App| s.open_task_id(&task)).build(t)
    };
    // How and when on the left (and whose crew, for a crew thread); the way into the task at the right, on the same line for every row.
    let crew = crew_words(s, &item.task);
    let mut who = row()
        .key(format!("{key}-who"))
        .w_full()
        .gap(t.space.at(1.5))
        .center_y()
        .child(text(says.clone()).font(weight(t.ty.sm, t.ty.semibold)).color(if failed { t.c.danger.fg } else { t.ink_muted() }).truncate().min_w(0.0).shrink().a11y_hidden())
        .child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden())
        .child(text(when.clone()).key(format!("{key}-when")).font(tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden());
    if let Some(c) = &crew {
        who = who.child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden()).child(text(c.clone()).key(format!("{key}-crew")).font(t.ty.sm).color(t.ink_faint()).truncate().min_w(0.0).shrink().a11y_hidden());
    }
    let who = who.child(el().grow(1.0)).child(open);
    let title = text(item.title.clone()).key(format!("{key}-title")).font(weight(t.ty.md, t.ty.semibold)).color(t.ink()).truncate().min_w(0.0);
    let mut body = col().grow(1.0).min_w(0.0).gap(t.space.at(1.0)).child(who.mt(-t.space.at(1.0)).mb(-t.space.at(0.5))).child(title);
    if let Some(a) = &item.answer {
        body = body.child(text(a.clone()).key(format!("{key}-answer")).font(t.ty.base).color(t.ink_muted()).wrap().min_w(0.0).a11y_hidden());
    }
    let mut foot = row().key(format!("{key}-foot")).w_full().gap(t.space.at(3.0)).center_y().flex_wrap().pt(t.space.at(1.5));
    let mut has_foot = false;
    if let Some(f) = facts(t, &key, item) {
        foot = foot.child(f);
        has_foot = true;
    }
    foot = foot.child(el().grow(1.0));
    if item.offer.review {
        let (task, run) = (task.clone(), run.clone());
        has_foot = true;
        foot = foot.child(Button::new(format!("{key}-review"), "Review").variant(ButtonVariant::Secondary).size(Size::Sm).icon(&icons::FILE_CODE).on_press(move |s: &mut App| s.review_from_briefing(&task, &run)).build(t));
    }
    if let Some((action, enabled)) = item.offer.action {
        let (task, run) = (task.clone(), run.clone());
        let (label, icon, redo) = match action {
            Action::Undo => ("Undo", &icons::RESTORE, false),
            Action::Redo => ("Redo", &icons::RETRY, true),
        };
        has_foot = true;
        foot = foot.child(Button::new(format!("{key}-{}", label.to_lowercase()), label).variant(ButtonVariant::Secondary).size(Size::Sm).icon(icon).disabled(!enabled).on_press(move |s: &mut App| s.undo_run(&task, &run, redo)).build(t));
    }
    if has_foot {
        body = body.child(foot);
    }
    let mut label = format!("{says}: {}", item.title);
    if let Some(a) = &item.answer {
        label = format!("{label}. {a}");
    }
    let id = item.task.clone();
    let description = match &crew {
        Some(c) => format!("{when}, {c}"),
        None => when,
    };
    let r = row().key(key.clone()).role("listitem").label(label).description(description).position_in_set(pos, n).focusable(true).w_full().px(t.space.at(4.0)).py(t.space.at(3.5)).gap(t.space.at(3.5)).items_start();
    // Led by the thread's status mark, on the title's line: no thread is led by a face, not even a crew thread.
    paint_state(r, &t.mat.row, States::default())
        .child(mark_lead(s, t, &format!("{key}-mark"), &item.task, failed, MARK_TOP))
        .child(body)
        .on_key(move |s: &mut App, ev: &KeyEvent, tx: &mut Tx| row_keys(s, ev, tx, &id, before.as_deref(), after.as_deref()))
}

/// One line of what a round did: its thread's status mark, "Docs check ran", what came of it, when. A line with a task opens it.
fn round_row(s: &App, t: &Theme, l: &BriefingRound, i: usize, n: usize) -> El {
    let (what, after) = words::round_line(l);
    let zone = s.zone();
    let when = words::when(l.due_ms.unwrap_or(l.at_ms), s.now_ms, &*zone);
    let key = format!("briefing-round-{}-{}", l.round, l.at_ms);
    let bad = matches!(l.outcome, RoundOutcome::Braked) || l.task_status == Some(TaskStatus::Failed);
    let mut line = row().w_full().gap(t.space.at(2.0)).center_y().min_w(0.0).child(text(what.clone()).key(format!("{key}-what")).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).shrink().a11y_hidden());
    if let Some(a) = &after {
        line = line.child(text(format!("· {a}")).key(format!("{key}-after")).font(t.ty.base).color(if bad { t.c.danger.fg } else { t.ink_muted() }).truncate().min_w(0.0).shrink().a11y_hidden());
    }
    let label = match &after {
        Some(a) => format!("{what}: {a}"),
        None => what.clone(),
    };
    let desc = if l.cat_name.is_empty() { when.clone() } else { format!("{}'s round, {when}", l.cat_name) };
    let mut r = row().key(key.clone()).role("listitem").label(label).description(desc).position_in_set(i, n).w_full().px(t.space.at(4.0)).py(t.space.at(2.5)).gap(t.space.at(3.5)).center_y();
    let mark = match &l.task {
        Some(task) => mark_lead(s, t, &format!("{key}-mark"), task, false, 0.0),
        None => mark_lead(s, t, &format!("{key}-mark"), &neko_protocol::TaskId(String::new()), bad, 0.0),
    };
    r = r
        .child(mark)
        .child(line.grow(1.0))
        .child(text(when).font(tabular(t.ty.sm)).color(t.ink_faint()).a11y_hidden());
    match l.task.clone().filter(|id| s.tasks.iter().any(|x| &x.id == id)) {
        Some(task) => paint_state(r.focusable(true).press_on_return(), &t.mat.row, States::default()).on_press(move |s: &mut App| s.open_task_id(&task)),
        None => r,
    }
}

/// A task still working: its status mark (the working ring), the title, and how long (and whose crew).
fn working_row(s: &App, t: &Theme, w: &BriefingWorking, i: usize, n: usize) -> El {
    let key = format!("briefing-working-{}", w.task.0);
    let id = w.task.clone();
    let r = row()
        .key(key.clone())
        .role("listitem")
        .label(w.title.clone())
        .description(format!("working, {}", crate::needs_view::waited(w.since_ms, s.now_ms)))
        .position_in_set(i, n)
        .focusable(true)
        .press_on_return()
        .w_full()
        .px(t.space.at(4.0))
        .py(t.space.at(2.5))
        .gap(t.space.at(3.5))
        .center_y();
    // The working ring, as the sidebar draws it; a thread the window does not know yet still turns.
    let lead = match s.tasks.iter().find(|x| x.id == w.task) {
        Some(_) => mark_lead(s, t, &format!("{key}-mark"), &w.task, false, 0.0),
        None => col().w(LEAD).min_w(LEAD).items_center().child(row().key(format!("{key}-mark")).size(crate::status_mark::SLOT, crate::status_mark::SLOT).center().child(crate::ui::base::spinner_in(t, crate::status_mark::MARK, t.c.accent.fg, "").a11y_hidden())).a11y_hidden(),
    };
    let mut second = format!("Working for {}", crate::needs_view::waited(w.since_ms, s.now_ms));
    if let Some(c) = crew_words(s, &w.task) {
        second = format!("{second} \u{b7} {c}");
    }
    paint_state(r, &t.mat.row, States::default())
        .child(lead)
        .child(
            col()
                .grow(1.0)
                .min_w(0.0)
                .gap(2.0)
                .child(text(w.title.clone()).font(weight(t.ty.base, t.ty.medium)).color(t.ink()).truncate().min_w(0.0).a11y_hidden())
                .child(text(second).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).a11y_hidden()),
        )
        .on_press(move |s: &mut App| s.open_task_id(&id))
}

/// The totals, in one quiet line: tasks, files with their figures, the cost as Claude Code reported it. The note under the cost says
/// it is not money charged when it is not.
fn totals(t: &Theme, b: &Briefing) -> El {
    let (tasks, changed, cost) = words::totals_words(b);
    let part = |key: &str, words: String| text(words).key(format!("briefing-totals-{key}")).font(t.ty.sm).color(t.ink_muted());
    let mut line = row().key("briefing-totals").role("note").w_full().gap(t.space.at(3.0)).center_y().flex_wrap().child(part("tasks", tasks.clone()));
    let mut label = tasks;
    if let Some(c) = changed {
        label = format!("{label}, {c}, {} lines added, {} removed", b.totals.added, b.totals.removed);
        let mut files = row().gap(t.space.at(2.0)).center_y().child(part("files", c));
        files = files.children(crate::turn::file_figures(t, b.totals.added, b.totals.removed));
        line = line.child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden()).child(files);
    }
    if let Some(c) = cost {
        label = format!("{label}, {c}");
        line = line.child(text("·").font(t.ty.sm).color(t.ink_faint()).a11y_hidden()).child(part("cost", c));
    }
    let mut block = col().w_full().gap(t.space.at(1.0)).child(line.label(label));
    if b.totals.cost_usd.is_some() && b.totals.api_equivalent {
        block = block.child(text(words::API_EQUIVALENT).key("briefing-totals-note").font(t.ty.xs).color(t.ink_faint()).wrap().min_w(0.0));
    }
    block
}

/// "Mark as read" and its key, at the page's end.
fn read_button(t: &Theme) -> El {
    let b = Button::new("briefing-read", "Mark as read").size(Size::Md).on_press(|s: &mut App| s.mark_briefing_read()).build(t);
    row().key("briefing-read-row").gap(t.space.at(3.0)).center_y().child(b).child(Kbd::new("briefing-read-keys", "Cmd Return").build(t))
}

/// The caught-up block: the person has read it all and nothing is new. Says when the next briefing is.
fn caught_up(s: &App, t: &Theme, b: &Briefing) -> El {
    let zone = s.zone();
    let since = words::since_phrase(b.read_ms.unwrap_or(b.from_ms), s.now_ms, &*zone);
    let mut d = format!("Nothing new since {since}.");
    let next = words::next_words(b, s.briefing_settings.as_ref(), s.now_ms, &*zone);
    if !next.is_empty() {
        d = format!("{d} {next}");
    }
    Empty::new("briefing-caught-up", &icons::CIRCLE_CHECK, "You're caught up").description(d).build(t)
}

pub fn briefing_view(s: &App, t: &Theme) -> El {
    let mut page = col().key("briefing").w_full().max_w(COLUMN).gap(t.space.at(7.0));
    let Some(b) = &s.briefing else {
        let waiting = if s.daemon == crate::state::Daemon::Absent { "" } else { "Loading…" };
        let h = col()
            .w_full()
            .gap(t.space.at(1.5))
            .child(text(crate::clock::date_line(s.now_ms, s.utc_offset_s)).key("briefing-date").font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_faint()))
            .child(text("Briefing").key("briefing-title").role("heading").level(1).font(weight(t.ty.xl, t.ty.medium)).color(t.ink()))
            .child(text(waiting).key("briefing-empty").role("status").font(t.ty.lg).color(t.ink_muted()));
        return wrap(t, page.child(h));
    };
    let zone = s.zone();
    let needs: Vec<&neko_protocol::NeedsItem> = s.visible_needs();
    let counts = b.counts();
    // Nothing new, and read before: the caught-up state. Nothing new and never read: the window says so in one line.
    let caught = !counts.news && b.read_ms.is_some();
    let mut headline = col()
        .key("briefing-head")
        .w_full()
        .gap(t.space.at(1.5))
        .child(text(crate::clock::date_line(s.now_ms, s.utc_offset_s)).key("briefing-date").font(weight(t.ty.sm, t.ty.semibold)).color(t.ink_faint()))
        .child(text(if caught { "Briefing".to_owned() } else { words::heading(b, s.now_ms, &*zone) }).key("briefing-title").role("heading").level(1).font(weight(t.ty.xl, t.ty.medium)).color(t.ink()));
    if !caught {
        headline = headline.child(text(words::summary(b, s.now_ms, &*zone)).key("briefing-summary").font(weight(t.ty.lg, t.ty.regular)).color(t.ink_muted()).wrap().min_w(0.0));
    }
    page = page.child(headline);
    let grouped = active_workspaces(b, &needs).len() > 1;
    if caught {
        page = page.child(caught_up(s, t, b));
    } else if !counts.news {
        let next = words::next_words(b, s.briefing_settings.as_ref(), s.now_ms, &*zone);
        if !next.is_empty() {
            page = page.child(text(next).key("briefing-next").font(t.ty.base).color(t.ink_faint()).wrap().min_w(0.0));
        }
    }

    // Needs you: the very cards of the Needs you list, answered where they are.
    if !needs.is_empty() {
        let keys: Vec<String> = needs.iter().map(|i| crate::state::need_key(&i.id)).collect();
        let mut sec = col().key("briefing-needs").w_full().gap(t.space.at(3.0)).child(head(t, "briefing-needs", if caught { "Still waiting for you" } else { "Needs you" }, needs.len(), t.c.warning.fg));
        let mut at = 0usize;
        let groups: Vec<(String, Vec<&&neko_protocol::NeedsItem>)> = if grouped { group_by(&needs, |i| i.workspace.as_str()) } else { vec![(String::new(), needs.iter().collect())] };
        for (ws, list) in groups {
            let name = if ws.is_empty() && grouped { crate::needs_view::group_name(s, &ws) } else { s.workspace_of(&ws).map_or_else(|| if ws.is_empty() { "Plugins".to_owned() } else { "No project".to_owned() }, |w| w.name.clone()) };
            let mut cards = col().key(format!("briefing-needs-list-{ws}")).role("list").label(if grouped { format!("Waiting in {name}") } else { "Waiting for you".to_owned() }).w_full().gap(t.space.at(3.0));
            let m = list.len();
            for (j, item) in list.into_iter().enumerate() {
                let before = at.checked_sub(1).map(|b| keys[b].clone());
                let after = keys.get(at + 1).cloned();
                cards = cards.child(crate::needs_view::item_row(s, t, item, &name, j + 1, m, before, after));
                at += 1;
            }
            if grouped {
                sec = sec.child(workspace_head(s, t, "briefing-needs", &ws));
            }
            sec = sec.child(cards);
        }
        page = page.child(sec);
    }

    // Cattains: each whose digest changed, in its own words, and each made by asking (a face means a Cattain: these alone have one).
    let ckeys = words::cattain_keys(b);
    if !ckeys.is_empty() {
        let mut sec = col().key("briefing-cattains").w_full().gap(t.space.at(3.0)).child(head(t, "briefing-cattains", "Cattains", ckeys.len(), t.ink_faint()));
        let n = ckeys.len();
        let around = |i: usize| (i.checked_sub(1).map(|b| ckeys[b].clone()), ckeys.get(i + 1).cloned());
        let mut rows: Vec<El> = Vec::new();
        for c in &b.cattains {
            let i = rows.len();
            let (before, after) = around(i);
            rows.push(digest_block(s, t, c, b.new_cattains.iter().find(|m| m.cat == c.cat), i + 1, n, before, after));
        }
        for m in b.new_cattains.iter().filter(|m| !b.cattains.iter().any(|c| c.cat == m.cat)) {
            let i = rows.len();
            let (before, after) = around(i);
            rows.push(new_cattain_row(s, t, m, i + 1, n, before, after));
        }
        sec = sec.child(rows_well(t, "briefing-cattains-list", "Cattains", rows));
        page = page.child(sec);
    }

    // Finished.
    if !b.finished.is_empty() {
        let total = b.totals.finished as usize;
        let mut sec = col().key("briefing-finished").w_full().gap(t.space.at(3.0)).child(head(t, "briefing-finished", "Finished", total, t.ink_faint()));
        let keys: Vec<String> = b.finished.iter().map(|f| task_key(&f.task)).collect();
        let mut at = 0usize;
        let groups: Vec<(String, Vec<&BriefingTask>)> = if grouped { group_by(&b.finished, |f| f.workspace.as_str()).into_iter().map(|(w, v)| (w, v.into_iter().collect())).collect() } else { vec![(String::new(), b.finished.iter().collect())] };
        for (ws, list) in groups {
            let m = list.len();
            let rows: Vec<El> = list
                .into_iter()
                .enumerate()
                .map(|(j, item)| {
                    let (before, after) = (at.checked_sub(1).map(|x| keys[x].clone()), keys.get(at + 1).cloned());
                    at += 1;
                    finished_row(s, t, item, j + 1, m, before, after)
                })
                .collect();
            if grouped {
                sec = sec.child(workspace_head(s, t, "briefing-finished", &ws));
            }
            sec = sec.child(rows_well(t, &format!("briefing-finished-list-{ws}"), "Finished", rows));
        }
        if b.finished_more() > 0 {
            sec = sec.child(text(format!("and {} more finished earlier", b.finished_more())).key("briefing-finished-more").font(t.ty.sm).color(t.ink_faint()).px(t.space.at(1.0)));
        }
        page = page.child(sec);
    }

    // Rounds.
    if !b.rounds.is_empty() {
        let total = (b.rounds_ran + b.rounds_not_run) as usize;
        let mut sec = col().key("briefing-rounds").w_full().gap(t.space.at(3.0)).child(head(t, "briefing-rounds", "Rounds", total, t.ink_faint()));
        // Newest first by the time each line shows (the time it was due, else when it happened).
        let mut sorted = b.rounds.clone();
        sorted.sort_by_key(|r| std::cmp::Reverse(r.due_ms.unwrap_or(r.at_ms)));
        let groups: Vec<(String, Vec<BriefingRound>)> = if grouped { group_by(&sorted, |r| r.workspace.as_str()).into_iter().map(|(w, v)| (w, v.into_iter().cloned().collect())).collect() } else { vec![(String::new(), sorted.clone())] };
        for (ws, list) in groups {
            let m = list.len();
            let rows: Vec<El> = list.iter().enumerate().map(|(j, l)| round_row(s, t, l, j + 1, m)).collect();
            if grouped {
                sec = sec.child(workspace_head(s, t, "briefing-rounds", &ws));
            }
            sec = sec.child(rows_well(t, &format!("briefing-rounds-list-{ws}"), "Rounds", rows));
        }
        if b.rounds_more() > 0 {
            sec = sec.child(text(format!("and {} more", b.rounds_more())).key("briefing-rounds-more").font(t.ty.sm).color(t.ink_faint()).px(t.space.at(1.0)));
        }
        page = page.child(sec);
    }

    // Still working.
    if !b.working.is_empty() {
        let mut sec = col().key("briefing-working").w_full().gap(t.space.at(3.0)).child(head(t, "briefing-working", "Still working", b.working.len(), t.c.accent.fg));
        let groups: Vec<(String, Vec<&BriefingWorking>)> = if grouped { group_by(&b.working, |w| w.workspace.as_str()).into_iter().map(|(w, v)| (w, v.into_iter().collect())).collect() } else { vec![(String::new(), b.working.iter().collect())] };
        for (ws, list) in groups {
            let m = list.len();
            let rows: Vec<El> = list.into_iter().enumerate().map(|(j, w)| working_row(s, t, w, j + 1, m)).collect();
            if grouped {
                sec = sec.child(workspace_head(s, t, "briefing-working", &ws));
            }
            sec = sec.child(rows_well(t, &format!("briefing-working-list-{ws}"), "Still working", rows));
        }
        page = page.child(sec);
    }

    // The foot: the totals, and the one thing left to do.
    if b.totals.finished > 0 {
        page = page.child(totals(t, b));
    }
    if counts.news {
        page = page.child(read_button(t));
    }
    wrap(t, page)
}

fn wrap(t: &Theme, page: El) -> El {
    col().key("briefing-scroll").w_full().grow(1.0).min_h(0.0).items_center().px(t.space.at(8.0)).pt(t.space.at(6.0)).pb(t.space.at(12.0)).scroll(rune::ScrollSpec { stick_bottom: false }).child(page)
}
