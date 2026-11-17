//! Setup, Schedules: Neko's scheduled planning. A schedule runs a read-only plan on a recurrence (a morning
//! brief, a weekly review) and each run appears as a task. New, edit, enable, run now, delete (with a
//! confirmation). Saving always pauses a schedule, so turning it on is a separate, deliberate step.
//! Ported from `pages/setup/Schedules.tsx`.

use neko_protocol::scheduled_plans::Schedule;
use rune::{El, col, row, text};

use super::Page;
use super::kit::{self, action, details_block, empty, glyph, list_row};
use super::state::SetupSection;
use super::words;
use crate::app::App;
use crate::icons;
use crate::link::SetupAction;
use crate::status::StatusKind;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, switch, with_menu};
use crate::view::{page_button, quiet_button};

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "sch-none", "Looking for your schedules…") };
    if data.workspaces.is_empty() {
        let body = empty(t, "sch-no-workspace", "Schedules belong to a workspace. Add one, then create its first schedule here.", Some(("Open Workspaces", Box::new(|s: &mut App| s.open_setup(SetupSection::Workspaces)))));
        return Page { count: None, trailing: Vec::new(), body, title: None, back: None };
    }
    let first = data.workspaces[0].id.clone();
    let new = page_button("New schedule", "sch-new", t, move |s: &mut App| s.open_schedule_dialog(None, &first));
    let mut body = col().w_full().gap(2.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    for (i, w) in data.workspaces.iter().enumerate() {
        let list: Vec<&Schedule> = data.schedules.iter().filter(|x| x.workspace_id.as_deref() == Some(w.id.as_str())).collect();
        let id = w.id.clone();
        let head = kit::heading(t, &format!("sch-group-{}", w.id), &w.name, (!list.is_empty()).then_some(list.len()), i == 0).hover_group().child(kit::fill()).child(action(t, format!("sch-group-new-{}", w.id), &format!("New schedule in {}", w.name), &icons::PLUS, move |s: &mut App| s.open_schedule_dialog(None, &id)));
        body = body.child(head);
        if list.is_empty() {
            let id = w.id.clone();
            body = body.child(empty(t, &format!("sch-none-{}", w.id), "No schedules", Some(("New schedule", Box::new(move |s: &mut App| s.open_schedule_dialog(None, &id))))));
        }
        for item in list {
            body = body.child(schedule_row(s, t, layer, item));
        }
    }
    let orphans: Vec<&Schedule> = data.schedules.iter().filter(|x| x.workspace_id.as_deref().is_none_or(|w| data.workspace(w).is_none())).collect();
    if !orphans.is_empty() {
        body = body.child(kit::heading(t, "sch-group-none", "Without a workspace", Some(orphans.len()), false));
        for item in orphans {
            body = body.child(schedule_row(s, t, layer, item));
        }
    }
    Page { count: (!data.schedules.is_empty()).then_some(data.schedules.len()), trailing: vec![new], body, title: None, back: None }
}

fn schedule_row(s: &App, t: &Neko, layer: &mut Vec<El>, item: &Schedule) -> El {
    let id = item.id.clone();
    let key = format!("schedule:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let enabled = item.enabled;
    let when = format!("{}{}", words::rule_summary(&item.rule), match item.next_due_ms {
        Some(due) if enabled => format!(" · next {}", words::relative_time(Some(due), s.now_ms)),
        _ if enabled => String::new(),
        _ => " · paused".to_owned(),
    });
    let (run, edit, del, sw) = (id.clone(), id.clone(), id.clone(), id.clone());
    let line = list_row(t, format!("schedule-{id}"), format!("{}, {when}", item.name), Some(open))
        .on_press(move |s: &mut App| s.toggle_row(&toggle))
        .child(glyph(t, if enabled { StatusKind::InProgress } else { StatusKind::Todo }, if enabled { "Enabled" } else { "Paused" }))
        .child(text(item.name.clone()).font(t.ty.row).color(t.text.base).truncate().min_w(0.0).max_w(220.0).a11y_hidden())
        .child(text(item.prompt.clone()).font(t.ty.meta).color(t.text.muted).truncate().grow(1.0).min_w(0.0).help(item.prompt.clone()).a11y_hidden())
        .child(text(when.clone()).key(format!("schedule-when-{id}")).font(t.ty.meta).color(t.text.faint).truncate().min_w(0.0).max_w(260.0).help(item.timezone.clone()).a11y_hidden())
        .child(action(t, format!("schedule-run-{id}"), &format!("Run {} now", item.name), &icons::PLAY, move |s: &mut App| {
            s.setup_call(SetupAction::RunSchedule { id: run.clone() });
            s.show_toast("Queued a read-only plan. It appears as a task.");
        }))
        .child(action(t, format!("schedule-edit-{id}"), "Edit schedule", &icons::PENCIL, move |s: &mut App| s.open_schedule_dialog(Some(&edit), "")))
        .child(action(t, format!("schedule-delete-{id}"), "Delete schedule…", &icons::TRASH, move |s: &mut App| s.confirm_delete_schedule(&del)))
        .child(switch(t, &format!("schedule-switch-{id}"), &format!("{} {}", if enabled { "Pause" } else { "Enable" }, item.name), enabled, move |s: &mut App| {
            let on = s.setup_data().and_then(|d| d.schedules.iter().find(|x| x.id == sw)).is_some_and(|x| x.enabled);
            s.setup_call(SetupAction::ScheduleEnabled { id: sw.clone(), enabled: !on })
        }));
    let mut entries: Vec<MenuEntry> = Vec::new();
    let (a, b, c, d) = (id.clone(), id.clone(), id.clone(), id.clone());
    entries.push(("Run now".to_owned(), std::rc::Rc::new(move |s: &mut App| {
        s.setup_call(SetupAction::RunSchedule { id: a.clone() });
        s.show_toast("Queued a read-only plan. It appears as a task.");
    })));
    entries.push(("Edit schedule…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_schedule_dialog(Some(&b), ""))));
    entries.push((if enabled { "Pause" } else { "Enable" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::ScheduleEnabled { id: c.clone(), enabled: !enabled }))));
    entries.push(("Delete schedule…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_delete_schedule(&d))));
    let mut out = col().key(format!("schedule-block-{id}")).w_full().child(with_menu(s, t, layer, &key, line, entries));
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        out = out.child(n);
    }
    if open {
        let mut d = details_block(format!("schedule-details-{id}"));
        d = d.child(text(item.prompt.clone()).key(format!("schedule-prompt-{id}")).font(t.ty.row).color(t.text.base).wrap().px(8.0).min_w(0.0));
        d = d.child(kit::words(t, &format!("schedule-rule-{id}"), &format!("{} · {}", words::rule_summary(&item.rule), item.timezone)));
        if !item.last_result.trim().is_empty() {
            d = d.child(text(format!("Last run: {}", item.last_result)).key(format!("schedule-last-{id}")).font(t.ty.meta).color(t.text.base).wrap().min_w(0.0).px(8.0).py(8.0).radius(t.radius.row).fill(t.surface.hover));
        }
        if let Some(task) = item.last_task_id.as_deref().filter(|task| s.summary.as_ref().is_some_and(|m| m.task(task).is_some())) {
            let task = task.to_owned();
            d = d.child(row().px(8.0).child(quiet_button("Last task", &format!("schedule-last-task-{id}"), t, move |s: &mut App| s.select(&task))));
        }
        out = out.child(d);
    }
    out
}
