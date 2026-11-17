//! The context panel's Plan and Details tabs. Plan is the plan as a checklist (the web's checklist view): each step with its
//! state, a done tick, the active step's glyph or an empty circle. Details is the web's `Details` (`work/TaskDetail.tsx`) in
//! sections that each fold: the risk assessment, parallel work (the proposal and its subtasks, linked, and "Propose parallel
//! work"), decisions, the working copy (path, branch, Reveal in Finder, Copy path), Info (where the session lives: its workspace,
//! branch, pull request, runtime, when it was made and last changed, and the ticket it came from) and the activity log.

use neko_protocol::workbench::{Risk, SupervisorDecision, TaskEvent};
use rune::{Color, El, ScrollSpec, TextStyle, col, row, text};

use super::{placeholder, quiet_button};
use crate::app::App;
use crate::clock;
use crate::icons;
use crate::link::{Call, ChildView, Detail};
use crate::panel::Section;
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

/// A section's header: 32 high, a chevron, the title and at the right a faint count (or a few words). Pressing it folds the section.
fn section_head(s: &App, t: &Neko, section: Section, trailing: Option<(String, Color)>) -> El {
    let open = s.panel.section_open(section);
    let mut head = row()
        .key(format!("details-{}", section.key()))
        .role("button")
        .label(format!("{}{}", section.title(), trailing.as_ref().map_or(String::new(), |(w, _)| format!(", {w}"))))
        .expanded(open)
        .focusable(true)
        .w_full()
        .h(32.0)
        .px(4.0)
        .gap(6.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.surface.ring)
        .on_press(move |s: &mut App| s.panel.toggle_section(section))
        .child(rune::icon(if open { &icons::CHEVRON_DOWN } else { &icons::CHEVRON_RIGHT }, 16.0).color(t.text.muted).a11y_hidden())
        .child(text(section.title()).font(TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.muted).truncate().grow(1.0).min_w(0.0).a11y_hidden());
    if let Some((words, color)) = trailing {
        head = head.child(text(words).key(format!("details-{}-count", section.key())).font(t.ty.meta).color(color).a11y_hidden());
    }
    head
}

/// A section: its header, and its body when open.
fn section(s: &App, t: &Neko, which: Section, trailing: Option<(String, Color)>, body: impl FnOnce() -> El) -> El {
    let mut c = col().key(format!("details-section-{}", which.key())).w_full().gap(2.0).child(section_head(s, t, which, trailing));
    if s.panel.section_open(which) {
        c = c.child(col().key(format!("details-body-{}", which.key())).w_full().px(4.0).pb(8.0).gap(2.0).child(body()));
    }
    c
}

/// A line of quiet words.
fn words(t: &Neko, key: String, line: &str) -> El {
    text(line.to_owned()).key(key).font(t.ty.meta).color(t.text.muted).wrap().min_w(0.0).selectable(true)
}

fn bullets(t: &Neko, key: &str, label: &str, items: &[String]) -> El {
    let mut c = col().key(format!("risk-{key}")).w_full().gap(2.0).pt(6.0).child(text(label.replace('_', " ")).font(t.ty.meta).color(t.text.muted));
    for (i, item) in items.iter().enumerate() {
        c = c.child(row().key(format!("risk-{key}-{i}")).w_full().gap(8.0).items_start().child(text("•").font(t.ty.row).color(t.text.faint).a11y_hidden()).child(text(item.clone()).font(t.ty.row).color(t.text.base).wrap().grow(1.0).min_w(0.0).selectable(true)));
    }
    c
}

/// Risk assessment: the level, the reason, and what supports it (the web's `data-slot="risk"`).
fn risk_body(t: &Neko, d: &SupervisorDecision) -> El {
    let mut c = col().key("risk-body").w_full().gap(2.0).child(text(if d.reason.trim().is_empty() { "No reason was written.".to_owned() } else { d.reason.clone() }).key("risk-reason").font(t.ty.row).color(t.text.base).wrap().min_w(0.0).selectable(true));
    for (key, items) in [("evidence", &d.evidence), ("files", &d.files), ("tests", &d.tests), ("sensitive_areas", &d.sensitive_areas), ("uncertainties", &d.uncertainties)] {
        if !items.is_empty() {
            c = c.child(bullets(t, key, key, items));
        }
    }
    c
}

/// One linked row of Parallel work: a number, the title, the subtask's state, a chevron; pressing it opens that task.
fn child_row(s: &App, t: &Neko, split: usize, i: usize, c: &ChildView) -> El {
    let key = format!("split-{split}-{i}");
    let state = match &c.task {
        Some((_, status)) => crate::board::friendly_status(*status).to_owned(),
        None => "Awaiting approval".to_owned(),
    };
    let mut r = row().key(key.clone()).w_full().min_h(32.0).px(4.0).gap(8.0).center_y().radius(t.radius.row).child(text(format!("{}.", i + 1)).font(t.ty.id).color(t.text.faint).w(18.0).a11y_hidden()).child(text(c.title.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(c.goal.clone()).a11y_hidden());
    r = r.child(text(state.clone()).font(t.ty.meta).color(t.text.muted).a11y_hidden());
    match &c.task {
        Some((id, _)) => {
            let id = id.clone();
            r = r.role("button").label(format!("{}, {state}. Open subtask", c.title)).focusable(true).hover(t.surface.hover).focus_ring(2.0, 2.0, t.surface.ring).on_press(move |s: &mut App| s.select(&id)).child(rune::icon(&icons::CHEVRON_RIGHT, 16.0).color(t.text.faint).a11y_hidden());
        }
        None => r = r.label(format!("{}, {state}", c.title)),
    }
    let _ = s;
    r
}

fn parallel_body(s: &App, t: &Neko, d: &Detail) -> El {
    let mut c = col().key("parallel-body").w_full().gap(2.0);
    for (si, split) in d.splits.iter().enumerate() {
        let line = if split.approved {
            if split.integrated { "Parallel work. The subtasks’ changes are in this task’s working copy." } else { "Parallel work. The subtasks’ changes are not yet in this task’s working copy." }
        } else {
            "Parallel proposal. It needs your approval before it starts."
        };
        c = c.child(words(t, format!("split-note-{si}"), line));
        for (i, child) in split.children.iter().enumerate() {
            c = c.child(child_row(s, t, si, i, child));
        }
    }
    if d.splits.is_empty() {
        c = c.child(words(t, "split-none".to_owned(), "Nothing proposed yet. A plan the agent wrote can be split into parts that run side by side."));
    }
    if d.can_split {
        let id = d.task.id.clone();
        c = c.child(row().pt(6.0).child(quiet_button("Propose parallel work", "details-split-propose", t, move |s: &mut App| {
            s.call(Call::ProposeSplit { task_id: id.clone() });
            s.show_toast("Asked for a parallel proposal.");
        })));
    }
    c
}

fn decisions_body(t: &Neko, d: &Detail) -> El {
    let mut c = col().key("decisions-body").w_full().gap(2.0);
    for (i, r) in d.decisions.iter().enumerate() {
        let l = clock::local(r.at_ms);
        let mut block = col().key(format!("decision-{i}")).w_full().gap(2.0).py(4.0).child(
            row()
                .w_full()
                .gap(8.0)
                .items_start()
                .child(text(format!("{:04}-{:02}-{:02}", l.year, l.month, l.day)).font(t.ty.id).color(t.text.faint).a11y_hidden())
                .child(text(r.title.clone()).font(TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).wrap().grow(1.0).min_w(0.0).selectable(true)),
        );
        block = block.child(words(t, format!("decision-text-{i}"), r.outcome.as_deref().unwrap_or("Outcome not yet observed.")));
        if r.accepted_locally {
            block = block.child(words(t, format!("decision-accepted-{i}"), "Accepted locally."));
        }
        if let Some(fix) = &r.correction {
            block = block.child(words(t, format!("decision-correction-{i}"), &format!("Your correction: {fix}")));
        }
        c = c.child(block);
    }
    c
}

/// The task's working folder written `~/…`, and the branch its work is on.
fn copy_body(s: &App, t: &Neko, d: &Detail) -> El {
    let task = &d.task;
    let home = std::env::var("HOME").unwrap_or_default();
    let mut c = col().key("copy-body").w_full().gap(2.0);
    if let Some(p) = &task.parked {
        c = c.child(words(t, "copy-parked".to_owned(), &format!("Saved on branch {}. Its folder comes back when the task runs again.", p.branch)));
    }
    match task.worktree.as_deref().filter(|w| !w.is_empty()) {
        Some(path) => {
            let shown = crate::terminal::tilde(path, &home);
            let (reveal, copy) = (path.to_owned(), path.to_owned());
            c = c.child(line(t, "copy-path", "Path", text(shown).font(t.ty.mono_meta).color(t.text.base).truncate_middle().min_w(0.0).help(path.to_owned()).selectable(true)));
            if !task.parked.is_some() {
                let branch = task.pull_request.as_ref().map(|p| p.branch.clone()).filter(|b| !b.is_empty());
                if let Some(b) = branch {
                    c = c.child(line(t, "copy-branch", "Branch", text(b).font(t.ty.mono_meta).color(t.text.base).truncate().min_w(0.0).selectable(true)));
                }
            }
            c = c.child(
                row()
                    .key("copy-actions")
                    .gap(8.0)
                    .pt(6.0)
                    .child(quiet_button("Reveal in Finder", "copy-reveal", t, move |s: &mut App| s.reveal(&reveal)))
                    .child(quiet_button("Copy path", "copy-copy", t, |_: &mut App| {}).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
                        tx.copy(copy.clone());
                        s.show_toast("Path copied");
                    })),
            );
        }
        None if task.parked.is_none() => c = c.child(words(t, "copy-none".to_owned(), "No working copy yet.")),
        None => {}
    }
    let _ = s;
    c
}

/// The old Info tab: where the session lives, and when.
fn info_body(s: &App, t: &Neko, d: &Detail) -> El {
    let task = &d.task;
    let mut c = col().key("info-list").w_full().gap(2.0);
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
    c
}

/// How many of the newest events the Activity log shows before "Show N earlier" (the web's `EVENT_CAP`).
pub const LOG_SHOWN: usize = 150;

/// One event as a line: its first line, or what its receipt or usage says.
pub fn event_words(e: &TaskEvent) -> String {
    if let Some(r) = crate::turns::command_receipt(e) {
        return format!("Ran {}", r.command.lines().next().unwrap_or(""));
    }
    if e.message.starts_with("USAGE ") {
        return "Reported what the run cost".to_owned();
    }
    e.message.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("").to_owned()
}

/// The activity log: the newest event first, each a 32 point row, the time (12, faint, tabular) then the words (12.5).
fn log_body(s: &App, t: &Neko, d: &Detail) -> El {
    let events = &d.task.events;
    let mut c = col().key("log-body").w_full().gap(0.0);
    if events.is_empty() {
        return c.child(words(t, "log-none".to_owned(), "Nothing recorded yet."));
    }
    let shown = if s.panel.log_all { events.len() } else { events.len().min(LOG_SHOWN) };
    for (i, e) in events.iter().rev().take(shown).enumerate() {
        let day = clock::day_start(e.at_ms) == clock::day_start(s.now_ms);
        let stamp = if day { clock::hhmm(e.at_ms) } else { format!("{} {}", clock::day_label(e.at_ms), clock::hhmm(e.at_ms)) };
        c = c.child(
            row()
                .key(format!("log-row-{i}"))
                .w_full()
                .h(32.0)
                .px(4.0)
                .gap(10.0)
                .center_y()
                .radius(t.radius.row)
                .hover(t.surface.hover)
                .child(text(stamp).font(t.ty.id).color(t.text.faint).w(if day { 40.0 } else { 108.0 }).a11y_hidden())
                .child(text(event_words(e)).font(t.ty.meta).color(t.text.base).truncate().grow(1.0).min_w(0.0).help(e.role.clone()).a11y_hidden()),
        );
    }
    if events.len() > shown {
        let more = events.len() - shown;
        c = c.child(row().pt(6.0).child(quiet_button(&format!("Show {more} earlier"), "log-earlier", t, |s: &mut App| s.panel.log_all = true)));
    }
    c
}

/// The Details tab: the task's sections, each folding (the web's `Details`). Info is the old Info tab.
pub fn details(s: &App, t: &Neko) -> El {
    let Some(d) = s.detail() else { return placeholder(&["Nothing to show yet."], t) };
    let mut c = col().key("details-list").w_full().px(12.0).py(8.0).gap(4.0);
    if let Some(r) = d.task.supervision.as_ref() {
        let (words, color) = match r.risk {
            Risk::Low => ("Low", t.text.muted),
            Risk::Medium => ("Medium", t.meaning.amber),
            Risk::High => ("High", t.meaning.coral),
            Risk::Unknown => ("Unknown", t.text.muted),
        };
        c = c.child(section(s, t, Section::Risk, Some((words.to_owned(), color)), || risk_body(t, r)));
    }
    if d.can_split || !d.splits.is_empty() {
        let n = d.splits.iter().map(|x| x.children.len()).sum::<usize>();
        c = c.child(section(s, t, Section::Parallel, Some(n).filter(|n| *n > 0).map(|n| (n.to_string(), t.text.faint)), || parallel_body(s, t, d)));
    }
    if !d.decisions.is_empty() {
        c = c.child(section(s, t, Section::Decisions, Some((d.decisions.len().to_string(), t.text.faint)), || decisions_body(t, d)));
    }
    c = c.child(section(s, t, Section::Copy, None, || copy_body(s, t, d)));
    c = c.child(section(s, t, Section::Info, None, || info_body(s, t, d)));
    c = c.child(section(s, t, Section::Log, Some((d.task.events.len().to_string(), t.text.faint)), || log_body(s, t, d)));
    col().key("info").w_full().grow(1.0).min_h(0.0).clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(c)
}
