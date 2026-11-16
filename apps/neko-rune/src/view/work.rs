//! The session view's working parts: the goal line, the runs with their diffs and undo, and the
//! note, question or confirmation that sits above the composer. Ported from the web UI's
//! `work/goals.tsx`, `work/Runs.tsx` and `work/queue.tsx`.

use neko_protocol::checkpoints::CheckpointList;
use neko_protocol::goals::{Goal, GoalStatus};
use neko_protocol::workbench::{Task, TaskStatus};
use rune::{El, ScrollSpec, col, row, text};
use rune_kit::diff::DiffView;

use super::{primary_button, quiet_button};
use crate::app::App;
use crate::link::Call;
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;
use crate::work::{DiffLoad, RunKey, UndoOptions, diff_key, goal_actions, goal_kind, goal_state, goal_word, stop_reason, turn_text, waiting_reason};

/// One goal on one line: glyph, objective, "turn 4/20 · check failing", and a blocked goal's
/// one question with its default. Open, its ledger, journal and actions.
pub fn goal_line(s: &App, t: &Neko, goal: &Goal) -> El {
    let key = format!("goal:{}", goal.id);
    let open = s.open(&key);
    let gid = goal.id.clone();
    let header = row()
        .key(format!("goal-{}", goal.id))
        .role("button")
        .label(format!("Goal: {}, {}, {}", goal_word(goal.status), goal.objective, turn_text(goal)))
        .expanded(open)
        .focusable(true)
        .w_full()
        .min_h(32.0)
        .px(12.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .hover(t.surface.hover)
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle(&format!("goal:{gid}")))
        .child(status_glyph(goal_kind(goal.status), t, 14.0, Some("")))
        .child(text(goal.objective.clone()).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
        .child(text(format!("{} · {}", turn_text(goal), goal_state(goal))).key(format!("goal-meta-{}", goal.id)).font(t.ty.meta).color(t.text.muted).truncate().a11y_hidden());
    let mut c = col().key(format!("goalbox-{}", goal.id)).w_full().child(header);
    if goal.status == GoalStatus::Blocked {
        if let Some(blocked) = &goal.blocked {
            let (id1, id2) = (goal.id.clone(), goal.id.clone());
            c = c.child(
                col()
                    .w_full()
                    .pl(36.0)
                    .pr(12.0)
                    .pb(6.0)
                    .gap(6.0)
                    .child(text(blocked.question.clone()).font(t.ty.meta).color(t.text.base).wrap())
                    .child(
                        row()
                            .gap(8.0)
                            .center_y()
                            .child(
                                rune_kit::TextInput::new(&s.work.goal_answer)
                                    .placeholder(blocked.default_answer.clone())
                                    .label("Your answer")
                                    .on_edit(|s: &mut App| &mut s.work.goal_answer)
                                    .on_submit(move |s: &mut App, _| s.goal_answer(&id1, false))
                                    .build(&t.kit)
                                    .key("goal-answer")
                                    .grow(1.0)
                                    .min_w(0.0),
                            )
                            .child(primary_button("Use the default", "goal-default", t, move |s: &mut App| s.goal_answer(&id2, true))),
                    ),
            );
        }
    }
    if open {
        c = c.child(goal_details(s, t, goal));
    }
    c
}

fn goal_details(_s: &App, t: &Neko, goal: &Goal) -> El {
    let mut c = col().key(format!("goal-details-{}", goal.id)).w_full().pl(36.0).pr(12.0).pb(8.0).gap(10.0);
    if goal.done_check.is_empty() {
        c = c.child(text("No check. Done needs a file or a link that exists.").font(t.ty.meta).color(t.text.muted).wrap());
    } else {
        c = c.child(row().gap(6.0).child(text("Check").font(t.ty.meta).color(t.text.base)).child(text(goal.done_check.clone()).font(t.ty.mono).color(t.text.muted).truncate().min_w(0.0)));
    }
    if !goal.constraints.is_empty() {
        c = c.child(text(format!("Constraints  {}", goal.constraints)).font(t.ty.meta).color(t.text.muted).wrap());
    }
    if !goal.ledger.is_empty() {
        let mut ledger = col().gap(6.0).child(text("Evidence").font(t.ty.group).color(t.text.base));
        for entry in goal.ledger.iter().rev().take(8) {
            let passed = entry.exit_code == Some(0);
            let mut line = col().gap(2.0).child(
                row()
                    .gap(8.0)
                    .center_y()
                    .child(text(entry.exit_code.map_or("no exit".to_owned(), |c| format!("exit {c}"))).font(t.ty.id).color(if passed { t.text.base } else { t.meaning.amber }))
                    .child(text(entry.command.clone()).font(t.ty.mono).color(t.text.muted).truncate().grow(1.0).min_w(0.0))
                    .child(text(entry.source.clone()).font(t.ty.id).color(t.text.faint)),
            );
            if !entry.tail.is_empty() {
                line = line.child(text(entry.tail.chars().take(600).collect::<String>()).font(t.ty.mono).color(t.text.faint).wrap().selectable(true));
            }
            ledger = ledger.child(line);
        }
        c = c.child(ledger);
    }
    if !goal.journal.is_empty() {
        let mut journal = col().gap(4.0).child(text("Journal").font(t.ty.group).color(t.text.base));
        for entry in goal.journal.iter().rev().take(8) {
            let mut words = entry.done.clone();
            if !entry.next.is_empty() {
                words.push_str(&format!(" Next: {}", entry.next));
            }
            if !entry.note.is_empty() {
                words.push_str(&format!(" {}", entry.note));
            }
            journal = journal.child(row().gap(8.0).child(text(entry.turn.to_string()).font(t.ty.id).color(t.text.faint)).child(text(words.trim().to_owned()).font(t.ty.meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0)));
        }
        c = c.child(journal);
    }
    if !goal.reason.is_empty() {
        c = c.child(text(format!("Judge  {}{}", if goal.verdict.is_empty() { String::new() } else { format!("{}: ", goal.verdict) }, goal.reason)).font(t.ty.meta).color(t.text.muted).wrap());
    }
    let mut actions = row().gap(8.0).wrap();
    for act in goal_actions(goal) {
        let (g, label) = (goal.clone(), act.label(goal));
        actions = actions.child(quiet_button(&label, &format!("goal-{}-{act:?}", goal.id), t, move |s: &mut App| s.goal_do(&g, act)));
    }
    c.child(actions)
}

fn runs_glyph(outcome: &str) -> (StatusKind, &'static str) {
    match outcome {
        "running" => (StatusKind::InProgress, "Running"),
        "done" => (StatusKind::Done, "Done"),
        "failed" => (StatusKind::Failed, "Failed"),
        "cancelled" => (StatusKind::Cancelled, "Stopped"),
        _ => (StatusKind::Backlog, "Unknown"),
    }
}

fn counts(t: &Neko, files: usize, added: usize, removed: usize) -> El {
    row()
        .gap(6.0)
        .center_y()
        .child(text(format!("{files} {}", if files == 1 { "file" } else { "files" })).font(t.ty.mono).color(t.text.muted))
        .child(text(format!("+{added}")).font(t.ty.mono).color(t.meaning.mint))
        .child(text(format!("−{removed}")).font(t.ty.mono).color(t.meaning.coral))
}

fn run_diff(s: &App, t: &Neko, task_id: &str, run: Option<u32>) -> El {
    let key = diff_key(task_id, run);
    let body = match s.work.diff_load.get(&key) {
        Some(DiffLoad::Failed(message)) => text(message.clone()).font(t.ty.meta).color(t.meaning.amber).wrap().px(12.0).py(8.0),
        Some(DiffLoad::Ready { .. }) if s.work.diffs.get(&key).is_some_and(|d| d.files().is_empty()) => text("This run changed no files.").font(t.ty.meta).color(t.text.muted).px(12.0).py(8.0),
        Some(DiffLoad::Ready { truncated }) => {
            let (a, b) = (key.clone(), key.clone());
            let view = DiffView::new(move |s: &App| s.work.diffs.get(&a).unwrap_or(&s.work.empty_diff), move |s: &mut App| s.work.diffs.entry(b.clone()).or_insert_with(|| rune_kit::diff::DiffState::new(Vec::new())))
                .key(format!("run-diff-{key}"))
                .build(s, &t.kit);
            let mut c = col().w_full().child(col().w_full().h(320.0).clip_rounded(t.radius.row).child(view));
            if *truncated {
                c = c.child(text("The diff is long, so it is cut short here.").font(t.ty.meta).color(t.text.faint).px(12.0).py(6.0));
            }
            c
        }
        _ => text("Loading the diff…").font(t.ty.meta).color(t.text.faint).px(12.0).py(8.0),
    };
    col().key(format!("run-diffbox-{key}")).w_full().child(body)
}

/// A task's runs, one line each (`Run 3 · 4 files · +120 −30`), from the per-run checkpoints.
pub fn runs(s: &App, t: &Neko, task_id: &str, list: &CheckpointList) -> Option<El> {
    if list.runs.is_empty() {
        return None;
    }
    let undo_off = if !list.restorable {
        Some("This task's folder is parked or missing, so there is nothing to restore.")
    } else if list.busy {
        Some("A run is working in this folder.")
    } else {
        None
    };
    let mut head = row().w_full().center_y().child(text("Runs").font(t.ty.group).color(t.text.muted).grow(1.0));
    if let Some(redo) = &list.redo {
        let id = task_id.to_owned();
        let label = format!("Redo {}", redo.trim_start_matches("undo of "));
        let b = quiet_button(&label, "redo", t, move |s: &mut App| s.redo(&id));
        head = head.child(if undo_off.is_some() || s.work.undoing { b.disabled(true) } else { b });
    }
    let mut card = col().key("runs-card").w_full().radius(t.radius.row + 4.0).fill(t.surface.card).clip_rounded(t.radius.row);
    for r in &list.runs {
        let key = RunKey::Run(r.n);
        let open = s.work.open_run == Some(key);
        let (kind, word) = runs_glyph(&r.outcome);
        let id = task_id.to_owned();
        let mut line = row()
            .key(format!("run-{}", r.n))
            .role("button")
            .label(format!("Run {}, {word}, {} files, {} added, {} removed{}", r.n, r.files, r.added, r.removed, if r.undone { ", undone" } else { "" }))
            .expanded(open)
            .focusable(true)
            .grow(1.0)
            .min_w(0.0)
            .h(36.0)
            .px(12.0)
            .gap(8.0)
            .center_y()
            .hover(t.surface.hover)
            .focus_ring(2.0, 0.0, t.kit.c.border_focus)
            .on_press(move |s: &mut App| s.toggle_run(&id, key))
            .child(text(if open { "▾" } else { "▸" }).font(t.ty.meta).color(t.text.faint).a11y_hidden())
            .child(status_glyph(kind, t, 14.0, Some("")))
            .child(text(format!("Run {}", r.n)).font(rune::TextStyle { weight: 500.0, ..t.ty.meta }).color(if r.undone { t.text.faint } else { t.text.base }).a11y_hidden());
        if r.undone {
            line = line.child(text("undone").font(t.ty.id).color(t.text.faint).a11y_hidden());
        }
        if r.outcome != "running" {
            line = line.child(counts(t, r.files, r.added, r.removed));
        }
        if r.skipped_count > 0 {
            line = line.child(text(format!("{} large {} not saved", r.skipped_count, if r.skipped_count == 1 { "file" } else { "files" })).font(t.ty.id).color(t.meaning.amber).a11y_hidden());
        }
        let mut top = row().w_full().center_y().child(line);
        if !r.undone && r.outcome != "running" {
            let (id, n) = (task_id.to_owned(), r.n);
            let b = quiet_button("Undo this run", &format!("undo-run-{}", r.n), t, move |s: &mut App| s.ask_undo(&id, UndoOptions { run: n, ..UndoOptions::default() }));
            top = top.child(row().pr(8.0).child(if undo_off.is_some() || s.work.undoing { b.disabled(true).help(undo_off.unwrap_or("Working on it.")) } else { b }));
        }
        let mut item = col().w_full().child(top);
        if open {
            item = item.child(run_diff(s, t, task_id, Some(r.n)));
        }
        card = card.child(item);
    }
    if list.runs.len() > 1 {
        let open = s.work.open_run == Some(RunKey::Whole);
        let id = task_id.to_owned();
        let mut whole = col().w_full().child(
            row()
                .key("run-whole")
                .role("button")
                .label("Whole task, first start to last end")
                .expanded(open)
                .focusable(true)
                .w_full()
                .h(36.0)
                .px(12.0)
                .gap(8.0)
                .center_y()
                .hover(t.surface.hover)
                .focus_ring(2.0, 0.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| s.toggle_run(&id, RunKey::Whole))
                .child(text(if open { "▾" } else { "▸" }).font(t.ty.meta).color(t.text.faint).a11y_hidden())
                .child(text("Whole task").font(rune::TextStyle { weight: 500.0, ..t.ty.meta }).color(t.text.base).a11y_hidden())
                .child(text("first start to last end").font(t.ty.meta).color(t.text.muted).a11y_hidden()),
        );
        if open {
            whole = whole.child(run_diff(s, t, task_id, None));
        }
        card = card.child(whole);
    }
    let mut c = col().key("runs").w_full().gap(8.0).child(head).child(card);
    if let Some(r) = &s.work.refusal {
        let mut offers = row().gap(8.0).wrap();
        let (id, o) = (task_id.to_owned(), r.options);
        match r.code.as_str() {
            "later_runs" => offers = offers.child(quiet_button("Undo back to here", "offer-through", t, { let id = id.clone(); move |s: &mut App| s.ask_undo(&id, UndoOptions { through_later: true, ..o }) })),
            "unsaved_changes" => offers = offers.child(quiet_button("Snapshot them first, then undo", "offer-snapshot", t, { let id = id.clone(); move |s: &mut App| s.ask_undo(&id, UndoOptions { snapshot_unsaved: true, ..o }) })),
            "pushed" | "foreign_commits" => offers = offers.child(quiet_button("Add a revert commit instead", "offer-revert", t, { let id = id.clone(); move |s: &mut App| s.ask_undo(&id, UndoOptions { revert: true, ..o }) })),
            _ => {}
        }
        offers = offers.child(quiet_button("Dismiss", "offer-dismiss", t, |s: &mut App| s.work.refusal = None));
        c = c.child(
            col()
                .key("undo-refused")
                .w_full()
                .px(14.0)
                .py(10.0)
                .gap(8.0)
                .radius(t.radius.row + 4.0)
                .fill(t.surface.card)
                .child(text(r.message.clone()).font(t.ty.meta).color(t.meaning.amber).wrap().live_region(rune::LiveRegion::Polite))
                .child(offers),
        );
    }
    Some(c)
}

/// The note above the composer: a confirmation, the planner's question or why the task stopped,
/// the actions the person can take, and the messages still waiting for the agent.
pub fn tray(s: &App, t: &Neko, task: &Task) -> Option<El> {
    let mut c = col().key("tray").w_full().gap(6.0);
    let mut any = false;
    if let Some(confirm) = &s.work.confirm {
        any = true;
        c = c.child(
            col()
                .key("confirm")
                .w_full()
                .px(14.0)
                .py(10.0)
                .gap(8.0)
                .radius(t.radius.row + 4.0)
                .fill(t.surface.card)
                .child(text(confirm.title.clone()).font(rune::TextStyle { weight: 500.0, ..t.ty.row }).color(t.text.base).wrap())
                .child(text(confirm.detail.clone()).font(t.ty.meta).color(t.text.muted).wrap())
                .child(
                    row()
                        .gap(8.0)
                        .child(primary_button(&confirm.ok, "confirm-yes", t, |s: &mut App| s.confirm_yes()))
                        .child(quiet_button(&confirm.cancel, "confirm-no", t, |s: &mut App| s.confirm_no())),
                ),
        );
    }
    let question = waiting_reason(task);
    if let Some(body) = &question {
        any = true;
        c = c.child(note(t, "The planner has a question", body, false));
    } else if task.status == TaskStatus::Failed {
        any = true;
        c = c.child(note(t, "Why it stopped", &stop_reason(task), true));
    }
    // What the person can do about the task, as the web's action list decides.
    let mut actions = row().gap(8.0).center_y();
    let mut has_action = false;
    let id = task.id.clone();
    let title = task.display_title.clone().unwrap_or_else(|| task.title.clone());
    match task.status {
        TaskStatus::AwaitingApproval if question.is_none() => {
            has_action = true;
            actions = actions.child(primary_button("Approve plan", "approve", t, move |s: &mut App| s.call(Call::Approve { task_id: id.clone() })));
        }
        TaskStatus::ReadyForReview => {
            has_action = true;
            if let Some(url) = task.pull_request.as_ref().map(|p| p.url.clone()).filter(|u| !u.is_empty()) {
                actions = actions.child(primary_button("Open pull request", "open-pr", t, move |s: &mut App| s.open_link(&url)));
                let id = id.clone();
                actions = actions.child(quiet_button("Mark done", "complete", t, move |s: &mut App| s.call(Call::Complete { task_id: id.clone() })));
            } else {
                actions = actions.child(primary_button("Mark done", "complete", t, move |s: &mut App| s.call(Call::Complete { task_id: id.clone() })));
            }
        }
        TaskStatus::Failed | TaskStatus::Cancelled => {
            has_action = true;
            actions = actions.child(quiet_button("Start again", "start", t, move |s: &mut App| s.call(Call::Start { task_id: id.clone() })));
        }
        _ => {}
    }
    if matches!(task.status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing) {
        has_action = true;
        let id = task.id.clone();
        actions = actions.child(quiet_button("Stop", "stop", t, move |s: &mut App| s.ask_stop(&id, &title)));
    }
    if has_action {
        any = true;
        c = c.child(actions);
    }
    let queued = s.summary.as_ref().map(|m| m.queued_for(&task.id)).unwrap_or_default();
    if !queued.is_empty() {
        any = true;
        let mut list = col().key("queued").w_full().gap(2.0);
        for m in queued {
            let mode = match m.mode {
                neko_protocol::crew::DeliveryMode::AfterTurn => "after the turn",
                neko_protocol::crew::DeliveryMode::SteerNow => "steer now",
                neko_protocol::crew::DeliveryMode::InterruptRestart => "interrupt",
            };
            let (tid, mid) = (task.id.clone(), m.id.clone());
            let mut line = row()
                .key(format!("queued-{}", m.id))
                .w_full()
                .gap(8.0)
                .center_y()
                .child(row().px(6.0).py(1.0).radius(4.0).fill(t.surface.active).child(text(format!("Queued · {mode}")).font(t.ty.id).color(t.text.muted)))
                .child(text(m.text.lines().next().unwrap_or("").to_owned()).font(t.ty.meta).color(t.text.base).truncate().grow(1.0).min_w(0.0));
            if !m.source.is_empty() && m.source != "you" {
                line = line.child(text(format!("from {}", m.source)).font(t.ty.id).color(t.text.faint));
            }
            line = line.child(quiet_button("Cancel", &format!("cancel-queued-{}", m.id), t, move |s: &mut App| s.cancel_queued(&tid, &mid)));
            list = list.child(line);
        }
        c = c.child(list);
    }
    if let Some(n) = &s.work.notice {
        any = true;
        c = c.child(text(n.clone()).key("notice").font(t.ty.meta).color(t.meaning.amber).wrap().live_region(rune::LiveRegion::Polite));
    }
    any.then_some(c)
}

fn note(t: &Neko, title: &str, body: &str, error: bool) -> El {
    col()
        .key("task-note")
        .w_full()
        .px(14.0)
        .py(10.0)
        .gap(2.0)
        .radius(t.radius.row + 4.0)
        .fill(t.surface.card)
        .child(text(title.to_owned()).font(rune::TextStyle { weight: 500.0, ..t.ty.meta }).color(if error { t.meaning.coral } else { t.meaning.amber }))
        .child(col().max_h(120.0).scroll(ScrollSpec { stick_bottom: false }).child(text(body.to_owned()).font(t.ty.meta).color(t.text.base).wrap().selectable(true)))
}
