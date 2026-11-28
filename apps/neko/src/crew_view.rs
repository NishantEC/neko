//! A Cattain's crew in the conversation (docs/knowledge/cats.md, "The crew"; design.md, "A Cattain's crew"): in a crew thread its
//! Cattain started, the quiet strip "Started by miso" above the conversation and miso's brief and messages as its own ("From miso"),
//! never as the person's; in the Cattain's own thread, the note where a crew thread's turn end woke it and the line that says what of
//! its crew it waits on. Quiet on purpose: a crew thread finishing is the Cattain's news, not the person's.

use neko_protocol::{Part, PartBody, Task, TaskStatus};
use rune::{El, col, el, row, text};
use rune_ui_style::{ButtonVariant, Size};

use crate::state::App;
use crate::ui::agent::FoldLine;
use crate::ui::base::{Shape, Theme, glyph, paint_state};
use crate::ui::button::Button;
use crate::ui::icons;
use crate::ui::response::Response;
use crate::view::COLUMN;

/// How many crew threads the waiting line names before "and 2 more".
const NAMED: usize = 3;

/// The crew threads of a Cattain that tell it their turns and have not finished: what its own thread waits on.
pub fn live_crew<'a>(s: &'a App, cat: &str) -> Vec<&'a Task> {
    let mut v: Vec<&Task> = s
        .tasks
        .iter()
        .filter(|t| t.crew_of() == Some(cat) && t.round.is_none() && !t.archived && matches!(t.status, TaskStatus::New | TaskStatus::Working | TaskStatus::NeedsYou | TaskStatus::Waiting))
        .collect();
    v.sort_by_key(|t| (t.created_ms, t.id.0.clone()));
    v
}

/// A crew thread's turn end, as the note in its Cattain's thread says it: "Woken by Fix login · done", "Fix login asks you", "Fix login
/// needs you".
pub fn wake_words(title: &str, status: &str) -> String {
    match status {
        "asked" => format!("{title} asks you"),
        "needs_you" => format!("{title} needs you"),
        other => format!("Woken by {title} \u{b7} {other}"),
    }
}

/// The note where a crew thread woke its Cattain: a centred line between hairlines with Open (the crew thread), and under it what it
/// said, folded.
pub fn wake_note(s: &App, t: &Theme, p: &Part) -> Option<El> {
    let PartBody::CrewWake { thread, title, status, summary, files, question } = &p.body else { return None };
    // The title as the thread has it now (it may have been renamed), else as it was.
    let now = s.tasks.iter().find(|x| &x.id == thread).map_or(title.as_str(), |x| x.title.as_str());
    let said = wake_words(now, status);
    let key = crate::task_view::kk(format!("crew-wake-{}", p.seq));
    let hair = || el().grow(1.0).h(1.0).fill(t.c.line);
    let mut line = row().key(key.clone()).role("note").label(said.clone()).w_full().h(t.control.md).gap(t.space.at(3.0)).center_y().child(hair());
    let ink = if matches!(status.as_str(), "asked" | "needs_you") { t.c.warning.fg } else { t.ink_muted() };
    line = line.child(text(said).font(t.ty.xs).color(ink).truncate().min_w(0.0).shrink().a11y_hidden());
    if s.tasks.iter().any(|x| &x.id == thread) {
        let id = thread.clone();
        line = line.child(Button::<App>::new(format!("{key}-open"), "Open").variant(ButtonVariant::Link).size(Size::Sm).on_press(move |s: &mut App| s.open_task_id(&id)).build(t));
    }
    let mut note = col().w_full().gap(0.0).child(line.child(hair()));
    let asked = question.as_deref().filter(|_| status == "asked");
    if summary.trim().is_empty() && files.is_empty() && asked.is_none() {
        return Some(note);
    }
    let fold_key = crate::task_view::kk(format!("crew-wake-said-{}", p.seq));
    let open = s.is_open(&fold_key);
    let k = fold_key.clone();
    let head = match status.as_str() {
        "needs_you" => "What it waits for".to_owned(),
        "asked" => "Its question".to_owned(),
        _ => "What it said".to_owned(),
    };
    let fold = FoldLine::<App>::new(fold_key.clone(), head, open).lead(crate::ui::agent::hollow_dot(t, t.control.icon)).on_toggle(move |s: &mut App| s.flip(&k));
    let mut body = Vec::new();
    if open {
        let mut inner = col().key(format!("{fold_key}-body")).w_full().gap(t.space.at(3.0)).pl(t.space.at(1.5));
        if let Some(q) = asked {
            inner = inner.child(text(q.to_owned()).key(format!("{fold_key}-question")).role("note").font(t.ty.base).color(t.ink()).wrap().min_w(0.0).selectable(true));
        }
        if !summary.trim().is_empty() && asked.is_none_or(|q| q.trim() != summary.trim()) {
            inner = inner.child(Response::new(format!("{fold_key}-summary"), summary.clone()).selectable(true).muted(true).column(COLUMN).build(t));
        }
        if !files.is_empty() {
            let mut list = col().key(format!("{fold_key}-files")).role("list").label("Files changed").w_full().gap(2.0);
            for f in files {
                list = list.child(
                    row()
                        .role("listitem")
                        .label(f.clone())
                        .w_full()
                        .gap(t.space.at(2.0))
                        .center_y()
                        .child(glyph(&icons::FILE, t.control.icon_sm, t.ink_faint()))
                        .child(text(f.clone()).font(crate::ui::agent::mono(t, t.ty.sm)).color(t.ink()).truncate_middle().min_w(0.0).shrink().a11y_hidden()),
                );
            }
            inner = inner.child(list);
        }
        body.push(inner);
    }
    note = note.child(fold.build(t, body));
    Some(note)
}

/// What a Cattain's own thread waits on, in words: "Waiting on its crew: Fix login and Fix docs", "…and 2 more". None unless it rests on
/// its crew (with no kittens of its own at work: those say themselves).
pub fn waiting_words(s: &App, task: &Task) -> Option<String> {
    if task.status != TaskStatus::Waiting || !task.cattain || !s.live_subagents(&task.id).is_empty() {
        return None;
    }
    let crew = live_crew(s, task.cat.as_deref()?);
    Some(match crew.len() {
        0 => "Reading its crew's news".to_owned(),
        n if n <= NAMED => format!("Waiting on its crew: {}", crate::subagents::names_sentence(&crew.iter().map(|x| x.title.clone()).collect::<Vec<_>>())),
        n => format!("Waiting on its crew: {} and {} more", crew.iter().take(NAMED).map(|x| x.title.clone()).collect::<Vec<_>>().join(", "), n - NAMED),
    })
}

/// The same in a few words, for the line of the run that ended before it: "Waiting on its crew (3)". The titles are on the waiting line.
pub fn run_words(s: &App, task: &Task) -> Option<String> {
    waiting_words(s, task)?;
    Some(match live_crew(s, task.cat.as_deref()?).len() {
        0 => "Reading its crew's news".to_owned(),
        n => format!("Waiting on its crew ({n})"),
    })
}

/// The conversation's last line while a Cattain rests on its crew: the still ring, then what it waits on, each title a link that opens
/// that crew thread.
pub fn waiting_line(s: &App, t: &Theme, task: &Task) -> Option<El> {
    let sentence = waiting_words(s, task)?;
    let mut line = row()
        .key("waiting-line")
        .role("status")
        .label(sentence.clone())
        .w_full()
        .min_h(t.control.lg)
        .pl(t.space.at(1.5))
        .gap(0.0)
        .center_y()
        .flex_wrap()
        .child(row().size(t.control.icon, t.control.icon).mr(t.metrics.gap).center().child(crate::subagents_view::still_ring(t, t.control.icon - 2.0).a11y_hidden()));
    let words = |w: String| text(w).font(t.ty.base).color(t.ink_muted()).a11y_hidden();
    let crew = live_crew(s, task.cat.as_deref().unwrap_or_default());
    if crew.is_empty() {
        return Some(line.child(words(sentence)));
    }
    line = line.child(words("Waiting on its crew: ".to_owned()));
    let shown = crew.len().min(NAMED);
    for (i, x) in crew.iter().take(shown).enumerate() {
        if i > 0 {
            let last = i == shown - 1 && crew.len() <= NAMED;
            line = line.child(words(if last { " and ".to_owned() } else { ", ".to_owned() }));
        }
        let id = x.id.clone();
        line = line.child(link(t, &format!("waiting-crew-{}", x.id.0), &x.title, move |s: &mut App| s.open_task_id(&id)));
    }
    if crew.len() > NAMED {
        line = line.child(words(format!(" and {} more", crew.len() - NAMED)));
    }
    Some(line)
}

fn link(t: &Theme, key: &str, words: &str, press: impl Fn(&mut App) + 'static) -> El {
    let face = row().key(key.to_owned()).role("link").label(words.to_owned()).focusable(true).px(2.0).radius(t.radius.sm).round_as(t).center_y();
    paint_state(face, &t.mat.row, crate::ui::base::States::default()).child(text(words.to_owned()).font(t.ty.base).color(t.c.accent.fg).truncate().max_w(220.0).a11y_hidden()).on_press(move |s: &mut App| press(s))
}

/// The strip above a crew thread its Cattain started: the Cattain's 14 point face, "Started by miso", and Open miso.
pub fn started_by(s: &App, t: &Theme, task: &Task) -> Option<El> {
    let c = s.cat(task.started_by.as_deref()?)?;
    let said = format!("Started by {}", c.name);
    let mut line = row().key("task-started-by").role("note").label(format!("{said}, in its crew")).w_full().gap(t.space.at(2.0)).center_y().min_w(0.0);
    line = line.child(crate::faces::portrait(t, "task-started-by-face", &c.face, 14.0).a11y_hidden());
    line = line.child(text(said).font(t.ty.sm).color(t.ink_muted()).truncate().min_w(0.0).shrink().a11y_hidden());
    let id = c.id.clone();
    line = line.child(Button::<App>::new("task-started-by-open", format!("Open {}", c.name)).variant(ButtonVariant::Link).size(Size::Sm).on_press(move |s: &mut App| s.open_cattain(&id)).build(t));
    Some(line)
}

/// Who wrote a message in a crew thread when it was not the person: the Cattain whose brief opened a thread it started, or the
/// Cattain whose `message_crew` words these are (by the name they carry, else the crew's own Cattain: it may have been renamed since).
/// Returns the Cattain and its words.
pub fn not_the_persons<'a>(s: &'a App, task: &'a Task, words: &'a str, first: bool) -> Option<(&'a neko_protocol::Cat, &'a str)> {
    if let Some((name, said)) = neko_protocol::cats::read_from_cattain(words) {
        let c = s.cats.iter().find(|c| c.name == name).or_else(|| task.cat.as_deref().and_then(|id| s.cat(id)))?;
        return Some((c, said));
    }
    if first {
        return Some((s.cat(task.started_by.as_deref()?)?, words));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::TaskId;

    fn crew_thread(n: u32, status: TaskStatus) -> Task {
        let mut t = Task::blank(TaskId(format!("task_{n}")), "default", "Fix it", &format!("Fix {n}"), n as i64, Some("cat_miso"));
        t.started_by = Some("cat_miso".into());
        t.status = status;
        t
    }

    #[test]
    fn a_crew_wake_says_whose_turn_ended_and_a_need_says_itself() {
        assert_eq!(wake_words("Fix login", "done"), "Woken by Fix login \u{b7} done");
        assert_eq!(wake_words("Fix login", "asked"), "Fix login asks you");
        assert_eq!(wake_words("Fix login", "needs_you"), "Fix login needs you");
    }

    #[test]
    fn the_cattains_brief_and_messages_are_never_the_persons() {
        let s = crate::headless_state();
        let t = crew_thread(1, TaskStatus::Done);
        assert_eq!(not_the_persons(&s, &t, "Fix it", true).map(|(c, w)| (c.name.as_str(), w)), Some(("miso", "Fix it")), "its brief");
        assert!(not_the_persons(&s, &t, "Also the tests", false).is_none(), "a later message of the person's");
        let from = neko_protocol::cats::from_cattain("miso", "Merge it.");
        assert_eq!(not_the_persons(&s, &t, &from, false).map(|(c, w)| (c.name.as_str(), w)), Some(("miso", "Merge it.")));
        let mine = Task::blank(TaskId("task_9".into()), "default", "Mine", "Mine", 9, None);
        assert!(not_the_persons(&s, &mine, "Mine", true).is_none(), "a thread the person started opens on their words");
    }

    #[test]
    fn a_cattain_resting_on_its_crew_names_what_it_waits_on() {
        let mut s = crate::headless_state();
        let mut own = Task::blank(TaskId("task_own".into()), "default", "", "miso", 0, Some("cat_miso"));
        own.cattain = true;
        own.status = TaskStatus::Waiting;
        s.tasks = vec![own.clone(), crew_thread(1, TaskStatus::Working), crew_thread(2, TaskStatus::NeedsYou), crew_thread(3, TaskStatus::Done)];
        assert_eq!(waiting_words(&s, &own).as_deref(), Some("Waiting on its crew: Fix 1 and Fix 2"));
        assert_eq!(run_words(&s, &own).as_deref(), Some("Waiting on its crew (2)"));
        assert!(s.rests_on_crew(&own));
        s.tasks.extend((4..8).map(|n| crew_thread(n, TaskStatus::Working)));
        assert_eq!(waiting_words(&s, &own).as_deref(), Some("Waiting on its crew: Fix 1, Fix 2, Fix 4 and 3 more"));
        own.status = TaskStatus::Done;
        assert_eq!(waiting_words(&s, &own), None);
    }
}
