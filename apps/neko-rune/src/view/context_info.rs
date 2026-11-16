//! The context panel's Plan and Info tabs. Plan is the plan as a checklist (the web's checklist view): each step with its
//! state, a done tick, the active step's glyph or an empty circle. Info is where the session lives: its workspace, branch,
//! pull request, runtime, when it was made and last changed, and the ticket it came from.

use rune::{El, ScrollSpec, col, row, text};

use super::placeholder;
use crate::app::App;
use crate::clock;
use crate::plan::{StepState, steps};
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;

/// One step's glyph: the status language's own (done, in progress, to do).
fn kind(state: &StepState) -> StatusKind {
    match state {
        StepState::Done => StatusKind::Done,
        StepState::Active => StatusKind::InProgress,
        StepState::Todo => StatusKind::Todo,
    }
}

pub fn plan(s: &App, t: &Neko) -> El {
    let Some(d) = s.detail() else { return placeholder(&["Nothing to show yet."], t) };
    let list = steps(&d.task.plan, d.task.status);
    if list.is_empty() {
        return placeholder(&["The agent has not written a plan for this task."], t);
    }
    let done = list.iter().filter(|x| x.state == StepState::Done).count();
    let mut c = col().key("plan-list").w_full().px(12.0).py(8.0).gap(2.0).child(text(format!("{done} of {} done", list.len())).key("plan-count").font(t.ty.group).color(t.text.muted).px(4.0).pb(6.0));
    for (i, step) in list.iter().enumerate() {
        let words = match step.state {
            StepState::Done => ", done",
            StepState::Active => ", in progress",
            StepState::Todo => "",
        };
        c = c.child(
            row()
                .key(format!("plan-step-{i}"))
                .label(format!("{}{words}", step.text))
                .w_full()
                .min_h(32.0)
                .px(4.0)
                .py(6.0)
                .gap(8.0)
                .items_start()
                .child(col().h(20.0).center_y().child(status_glyph(kind(&step.state), t, 14.0, Some(""))))
                .child(text(step.text.clone()).font(t.ty.row).color(if step.state == StepState::Done { t.text.muted } else { t.text.base }).wrap().grow(1.0).min_w(0.0).selectable(true).a11y_hidden()),
        );
    }
    col().key("plan").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}

/// "2026-09-16 (2mo ago)" style: the date, and how long ago in the sessions column's short words.
fn when(now_ms: i64, ms: i64) -> String {
    let l = clock::local(ms);
    format!("{:04}-{:02}-{:02} · {}", l.year, l.month, l.day, crate::groups::age(now_ms, ms))
}

fn line(t: &Neko, key: &str, label: &str, value: El) -> El {
    row().key(format!("info-{key}")).w_full().min_h(32.0).px(4.0).gap(12.0).center_y().child(text(label.to_owned()).font(t.ty.meta).color(t.text.muted).w(84.0).a11y_hidden()).child(row().grow(1.0).min_w(0.0).child(value))
}

fn value(t: &Neko, words: &str) -> El {
    text(words.to_owned()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).selectable(true)
}

fn link(t: &Neko, key: &str, words: &str, url: String) -> El {
    row()
        .key(key.to_owned())
        .role("button")
        .label(words.to_owned())
        .focusable(true)
        .h(26.0)
        .px(6.0)
        .center_y()
        .radius(t.radius.control)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.open_link(&url))
        .child(text(words.to_owned()).font(t.ty.row).color(t.meaning.iris).truncate().min_w(0.0).a11y_hidden())
}

pub fn info(s: &App, t: &Neko) -> El {
    let Some(d) = s.detail() else { return placeholder(&["Nothing to show yet."], t) };
    let task = &d.task;
    let mut c = col().key("info-list").w_full().px(12.0).py(8.0).gap(2.0);
    c = c.child(line(t, "workspace", "Workspace", value(t, if d.workspace.is_empty() { "None" } else { &d.workspace })));
    let branch = task.pull_request.as_ref().map(|p| p.branch.clone()).filter(|b| !b.is_empty()).or_else(|| task.parked.as_ref().map(|p| p.branch.clone()));
    if let Some(b) = branch {
        c = c.child(line(t, "branch", "Branch", text(b).font(t.ty.mono_meta).color(t.text.base).truncate().min_w(0.0).selectable(true)));
    }
    if let Some(pr) = task.pull_request.as_ref().filter(|p| !p.url.is_empty()) {
        let number = pr.url.rsplit('/').next().unwrap_or("").to_owned();
        c = c.child(line(t, "pr", "Pull request", link(t, "info-pr-link", &format!("#{number}"), pr.url.clone())));
    }
    c = c.child(line(t, "runtime", "Runtime", value(t, &d.runtime)));
    c = c.child(line(t, "created", "Created", value(t, &when(s.now_ms, task.created_at_ms))));
    c = c.child(line(t, "updated", "Updated", value(t, &when(s.now_ms, task.updated_at_ms))));
    if let Some((id, url)) = &d.source {
        c = c.child(line(t, "source", "Ticket", link(t, "info-source-link", id, url.clone())));
    }
    col().key("info").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}
