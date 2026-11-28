//! Timers and watches in the app (docs/knowledge/waits.md): what a thread rests on said in place ("Waiting until 18:00"), its timers
//! and watches as quiet lines with Stop watching (the activity bar, Overview's Waiting on), the note where one woke the thread, and a
//! standing round's words in its Cattain's thread. The daemon keeps them (`Task::waits`); this file only says them.

use neko_protocol::schedule::{Zone, local_parts};
use neko_protocol::{EventKind, Part, PartBody, Task, TaskId, TaskStatus, Wait, WaitKind};
use rune::{El, Tx, col, row, text};

use crate::state::App;
use crate::ui::agent::{FoldLine, PressTx, icon_button};
use crate::ui::response::Response;
use crate::ui::base::{Shape, Theme, glyph};
use crate::view::COLUMN;

/// A time near now in a few words: "18:00" today, "tomorrow 09:00", "Tue 18:00", "29 Nov 18:00".
pub fn at_words(ms: i64, now_ms: i64, zone: &dyn Zone) -> String {
    let (at, now) = (local_parts(zone, ms), local_parts(zone, now_ms));
    let hm = format!("{:02}:{:02}", at.hour, at.minute);
    match at.day_number - now.day_number {
        0 => hm,
        1 => format!("tomorrow {hm}"),
        2..=6 => format!("{} {hm}", at.weekday.short()),
        _ => {
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            format!("{} {} {hm}", at.day, MONTHS[(at.month - 1) as usize])
        }
    }
}

/// "every 5 minutes", "every hour".
fn every_words(ms: i64) -> String {
    let m = ms / 60_000;
    match m {
        60 => "every hour".to_owned(),
        n if n > 60 && n % 60 == 0 => format!("every {} hours", n / 60),
        1 => "every minute".to_owned(),
        n => format!("every {n} minutes"),
    }
}

/// What a thread at rest waits on among its timers and watches, in a few words for its row and its run's line: "Waiting until 18:00",
/// "Waiting on a file change", "Waiting on a check", or both ("Waiting until 18:00 or a file change"). None when it has none.
pub fn rest_words(task: &Task, now_ms: i64, zone: &dyn Zone) -> Option<String> {
    if task.waits.is_empty() {
        return None;
    }
    let soonest = task.waits.iter().filter(|w| w.kind == WaitKind::Timer).filter_map(|w| w.at_ms).min();
    let files = task.waits.iter().filter(|w| w.kind == WaitKind::Files).count();
    let checks = task.waits.iter().filter(|w| w.kind == WaitKind::Command).count();
    let watch = match (files, checks) {
        (0, 0) => None,
        (1, 0) => Some("a file change".to_owned()),
        (0, 1) => Some("a check".to_owned()),
        (f, 0) => Some(format!("{f} watches")),
        (0, c) => Some(format!("{c} checks")),
        (1, 1) => Some("a file change or a check".to_owned()),
        (f, c) => Some(format!("{} watches", f + c)),
    };
    Some(match (soonest, watch) {
        (Some(at), None) => format!("Waiting until {}", at_words(at, now_ms, zone)),
        (Some(at), Some(w)) => format!("Waiting until {} or {w}", at_words(at, now_ms, zone)),
        (None, Some(w)) => format!("Waiting on {w}"),
        (None, None) => return None,
    })
}

/// One timer or watch in words: what it is ("Until 18:00", "Watching build.log", "Checking gh pr checks 12") and what goes with it (the
/// note, how often, until when).
pub fn line_words(w: &Wait, now_ms: i64, zone: &dyn Zone) -> (String, String) {
    let until = w.until_ms.map(|u| format!("until {}", at_words(u, now_ms, zone)));
    match w.kind {
        WaitKind::Timer => (format!("Until {}", w.at_ms.map_or_else(String::new, |a| at_words(a, now_ms, zone))), w.note.clone()),
        WaitKind::Files => (format!("Watching {}", w.paths.join(", ")), [Some(w.note.clone()).filter(|n| !n.is_empty()), until].into_iter().flatten().collect::<Vec<_>>().join(" · ")),
        WaitKind::Command => {
            let how = w.every_ms.map(every_words);
            let last = match w.last_exit {
                Some(0) | None => None,
                Some(_) => Some("not yet".to_owned()),
            };
            (format!("Checking {}", w.command.as_deref().unwrap_or("")), [how, last, until].into_iter().flatten().collect::<Vec<_>>().join(" · "))
        }
    }
}

fn mark(w: &Wait) -> &'static rune::Icon {
    match w.kind {
        WaitKind::Timer => &crate::ui::icons::CLOCK,
        WaitKind::Files => &crate::icons::EYE,
        WaitKind::Command => &crate::ui::icons::TERMINAL,
    }
}

/// What the × on one says.
pub fn stop_words(w: &Wait) -> &'static str {
    if w.kind == WaitKind::Timer { "Stop the timer" } else { "Stop watching" }
}

/// One timer or watch as a quiet line: its mark, what it is, what goes with it, and its × (Stop the timer, Stop watching).
pub fn wait_row(s: &App, t: &Theme, key: &str, task: &TaskId, w: &Wait) -> El {
    let zone = s.zone();
    let (name, detail) = line_words(w, s.now_ms, &*zone);
    let (task, id) = (task.clone(), w.id.clone());
    let stop: PressTx<App> = std::rc::Rc::new(move |s: &mut App, _: &mut Tx| s.stop_waiting(&task, &id));
    row()
        .key(key.to_owned())
        .role("listitem")
        .label(if detail.is_empty() { name.clone() } else { format!("{name}, {detail}") })
        .w_full()
        .min_h(t.control.sm + 4.0)
        .pl(t.space.at(3.0))
        .pr(t.space.at(1.0))
        .gap(t.space.at(2.0))
        .center_y()
        .radius(t.radius.lg)
        .round_as(t)
        .child(glyph(mark(w), t.control.icon_sm, t.ink_faint()).a11y_hidden())
        .child(text(name).font(t.ty.sm).color(t.ink_muted()).truncate().shrink().min_w(0.0).a11y_hidden())
        .child(text(detail).font(t.ty.sm).color(t.ink_faint()).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden())
        .child(icon_button(t, &format!("{key}-stop"), stop_words(w), &crate::ui::icons::CLOSE, None, false, Some(stop)))
}

/// The thread's timers and watches as a list of quiet lines, keyed under `prefix`.
pub fn rows(s: &App, t: &Theme, prefix: &str, task: &Task) -> Vec<El> {
    task.waits.iter().map(|w| wait_row(s, t, &format!("{prefix}-{}", w.id), &task.id, w)).collect()
}

/// The conversation's last line while the thread rests on its timers and watches alone (no kitten, no crew): "Waiting until 18:00",
/// the still ring before it, as the waiting line on kittens reads.
pub fn waiting_line(s: &App, t: &Theme, task: &Task) -> Option<El> {
    if task.status != TaskStatus::Waiting || task.subagent.is_some() || !s.live_subagents(&task.id).is_empty() {
        return None;
    }
    let said = rest_words(task, s.now_ms, &*s.zone())?;
    Some(
        row()
            .key("waiting-line")
            .role("status")
            .label(said.clone())
            .w_full()
            .min_h(t.control.lg)
            .pl(t.space.at(1.5))
            .gap(t.metrics.gap)
            .center_y()
            .child(row().size(t.control.icon, t.control.icon).center().child(crate::subagents_view::still_ring(t, t.control.icon - 2.0).a11y_hidden()))
            .child(text(said).font(t.ty.base).color(t.ink_muted()).a11y_hidden()),
    )
}

/// The centred note where a timer or watch woke the thread ("Woken by its timer · 18:00", "Woken by a change to build.log", "Woken:
/// gh pr checks 12 passed", "The watch on src/** ended"), and under it what it carried, folded. A standing round's words are not a
/// note: they are a message ([`round_message`]).
pub fn wake_note(s: &App, t: &Theme, p: &Part) -> Option<El> {
    let PartBody::EventWake { event, note, detail, at_ms, .. } = &p.body else { return None };
    let zone = s.zone();
    let first = detail.lines().next().unwrap_or("").to_owned();
    let said = match event {
        EventKind::Round => return None,
        EventKind::Timer => match at_ms {
            Some(a) => format!("Woken by its timer · {}", at_words(*a, p.at_ms, &*zone)),
            None => "Woken by its timer".to_owned(),
        },
        EventKind::Files => {
            let n = detail.lines().count();
            if n > 1 { format!("Woken by a change to {first} and {} more", n - 1) } else { format!("Woken by a change to {first}") }
        }
        EventKind::Command => "Woken: its check passed".to_owned(),
        EventKind::WatchEnded => {
            if detail.is_empty() { "A watch ended after its time".to_owned() } else { format!("The watch on {detail} ended after its time") }
        }
    };
    let key = crate::task_view::kk(format!("event-wake-{}", p.seq));
    let mut out = col().w_full().gap(0.0).child(crate::subagents_view::centred_note(t, &key, &said));
    // What it carried: the agent's own note, then the files or the check's last words.
    let body_words = match event {
        EventKind::Files | EventKind::Command => [note.as_str(), detail.as_str()].iter().filter(|x| !x.trim().is_empty()).copied().collect::<Vec<_>>().join("\n\n"),
        _ => note.clone(),
    };
    if body_words.trim().is_empty() {
        return Some(out);
    }
    let fold_key = crate::task_view::kk(format!("event-wake-said-{}", p.seq));
    let open = s.is_open(&fold_key);
    let k = fold_key.clone();
    let head = if matches!(event, EventKind::Command) { "What the check said" } else { "Its note" };
    let fold = FoldLine::<App>::new(fold_key.clone(), head, open).lead(crate::ui::agent::hollow_dot(t, t.control.icon)).on_toggle(move |s: &mut App| s.flip(&k));
    let body = if open { vec![col().key(format!("{fold_key}-body")).w_full().pl(t.space.at(1.5)).child(Response::new(format!("{fold_key}-words"), body_words).selectable(true).muted(true).column(COLUMN).build(t))] } else { Vec::new() };
    out = out.child(fold.build(t, body));
    Some(out)
}

/// A standing round's words in its Cattain's thread: the person's own words, in their well, "From the round Morning check".
pub fn round_message(s: &App, t: &Theme, p: &Part) -> Option<El> {
    let PartBody::EventWake { event: EventKind::Round, note, detail, .. } = &p.body else { return None };
    Some(crate::turn::person_from(s, t, &format!("round-message-{}", p.seq), &format!("round-message-{}-words", p.seq), detail, &format!("the round {note}")))
}

/// The activity bar's lines when the thread rests on its timers and watches alone: one quiet line each, with its ×.
pub fn activity(s: &App, t: &Theme, task: &Task) -> El {
    col().key("activity").role("list").label("What this thread waits on").w_full().gap(0.0).children(rows(s, t, "activity-wait", task))
}

/// The words a summary of the activity bar adds for them ("until 18:00", "1 watch").
pub fn summary_words(task: &Task, now_ms: i64, zone: &dyn Zone) -> Option<String> {
    let soonest = task.waits.iter().filter(|w| w.kind == WaitKind::Timer).filter_map(|w| w.at_ms).min();
    let watches = task.waits.iter().filter(|w| w.is_watch()).count();
    let parts: Vec<String> = [soonest.map(|a| format!("until {}", at_words(a, now_ms, zone))), (watches > 0).then(|| if watches == 1 { "1 watch".to_owned() } else { format!("{watches} watches") })].into_iter().flatten().collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// The heading over them in Overview's Waiting on.
pub fn section_rows(s: &App, t: &Theme, task: &Task) -> Vec<El> {
    rows(s, t, "pane-wait", task)
}

impl App {
    /// Stop watching (or Stop the timer) on one of a thread's: the daemon ends it and tells every screen.
    pub fn stop_waiting(&mut self, task: &TaskId, id: &str) {
        self.send(neko_protocol::Request::StopWaiting { task: task.clone(), wait: id.to_owned() });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::schedule::Fixed;

    const NOW: i64 = 1_764_324_000_000; // 10:00 UTC

    fn wait(kind: WaitKind, at: Option<i64>) -> Wait {
        Wait { id: "wait_1".into(), kind, note: String::new(), at_ms: at, until_ms: None, paths: vec!["build.log".into()], command: Some("gh pr checks 12".into()), every_ms: Some(300_000), checked_ms: None, last_exit: None, created_ms: NOW }
    }

    #[test]
    fn a_thread_at_rest_says_what_it_waits_for_in_a_few_words() {
        let z = Fixed(0);
        let mut t = Task::blank(TaskId("t".into()), "default", "w", "t", NOW, None);
        assert_eq!(rest_words(&t, NOW, &z), None);
        t.waits = vec![wait(WaitKind::Timer, Some(NOW + 8 * 3_600_000))];
        assert_eq!(rest_words(&t, NOW, &z).as_deref(), Some("Waiting until 18:00"));
        t.waits[0].at_ms = Some(NOW + 23 * 3_600_000);
        assert_eq!(rest_words(&t, NOW, &z).as_deref(), Some("Waiting until tomorrow 09:00"));
        t.waits = vec![wait(WaitKind::Files, None)];
        assert_eq!(rest_words(&t, NOW, &z).as_deref(), Some("Waiting on a file change"));
        t.waits.push(wait(WaitKind::Timer, Some(NOW + 3_600_000)));
        assert_eq!(rest_words(&t, NOW, &z).as_deref(), Some("Waiting until 11:00 or a file change"));
        t.waits = vec![wait(WaitKind::Command, None)];
        assert_eq!(rest_words(&t, NOW, &z).as_deref(), Some("Waiting on a check"));
    }

    #[test]
    fn each_line_names_what_it_is() {
        let z = Fixed(0);
        assert_eq!(line_words(&wait(WaitKind::Timer, Some(NOW + 8 * 3_600_000)), NOW, &z).0, "Until 18:00");
        assert_eq!(line_words(&wait(WaitKind::Files, None), NOW, &z).0, "Watching build.log");
        let mut c = wait(WaitKind::Command, None);
        c.last_exit = Some(1);
        assert_eq!(line_words(&c, NOW, &z), ("Checking gh pr checks 12".to_owned(), "every 5 minutes · not yet".to_owned()));
        assert_eq!(stop_words(&c), "Stop watching");
    }
}
