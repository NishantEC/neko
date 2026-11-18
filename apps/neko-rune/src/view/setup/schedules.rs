//! Setup, Schedules: Neko's scheduled planning, as the web's `pages/setup/Schedules.tsx` draws it. A schedule runs a read-only plan on
//! a recurrence (a morning brief, a weekly review) and each run appears as a task. The page is the workspace picker, then one section
//! per workspace (its name, a faint count, "New" at the end) holding its schedules or "No schedules"; schedules with no workspace sit
//! under "Without a workspace". Saving always pauses a schedule, so turning it on is a separate, deliberate step.

use neko_protocol::scheduled_plans::Schedule;
use rune::{El, col, row, text};

use super::Page;
use super::state::SetupSection;
use super::words;
use crate::app::App;
use crate::icons_phosphor as ph;
use crate::link::SetupAction;
use crate::status::StatusKind;
use crate::theme::Neko;
use crate::view::ui::{MenuEntry, note, with_menu};
use crate::view::webkit;
use crate::view::page_button;

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> Page {
    let Some(data) = s.setup_data() else { return Page::quiet(t, "sch-none", "Looking for your schedules…") };
    if data.workspaces.is_empty() {
        let body = webkit::empty(t, "sch-no-workspace", &ph::CLOCK_REGULAR, "Add a workspace first", "Schedules belong to a workspace. Add one, then create its first schedule here.", Some(("sch-no-workspace-action", "Open Workspaces", Box::new(|s: &mut App| s.open_setup(SetupSection::Workspaces)))));
        return Page { count: None, trailing: Vec::new(), body, title: None, back: None };
    }
    let scope = s.setup.scope.as_str();
    let mut body = col().key("sch-body").w_full().gap(32.0);
    if let Some(n) = note(s, t, "setup:page") {
        body = body.child(n);
    }
    // `ScopePicker`: only with two or more workspaces.
    if data.workspaces.len() > 1 {
        let mut names = vec!["All workspaces".to_owned()];
        names.extend(data.workspaces.iter().map(|w| w.name.clone()));
        let by: Vec<(String, String)> = data.workspaces.iter().map(|w| (w.name.clone(), w.id.clone())).collect();
        let current = if scope.is_empty() { "All workspaces".to_owned() } else { data.workspace_name(scope) };
        let picker = webkit::pick(s, t, layer, "sch-scope", "Show schedules for", &names, &current, move |s: &mut App, chosen: &str| {
            s.setup.scope = by.iter().find(|(n, _)| n == chosen).map(|(_, id)| id.clone()).unwrap_or_default();
        });
        body = body.child(row().gap(8.0).center_y().child(picker));
    }
    for w in data.workspaces.iter().filter(|w| scope.is_empty() || w.id == scope) {
        let list: Vec<&Schedule> = data.schedules.iter().filter(|x| x.workspace_id.as_deref() == Some(w.id.as_str())).collect();
        let id = w.id.clone();
        let new = webkit::ghost_xs_button(t, &format!("sch-group-new-{}", w.id), "New", Some(&ph::PLUS), &format!("New schedule in {}", w.name), move |s: &mut App| s.open_schedule_dialog(None, &id));
        let inner = if list.is_empty() {
            text("No schedules".to_owned()).key(format!("sch-none-{}", w.id)).font(webkit::font(t, 13.0, 400.0, 21.125)).color(t.text.muted)
        } else {
            webkit::panel(list.iter().map(|item| schedule_row(s, t, layer, item)).collect())
        };
        let count = (!list.is_empty()).then(|| list.len().to_string());
        body = body.child(webkit::section(t, &format!("sch-group-{}", w.id), &w.name, count.as_deref(), Some(new), inner));
    }
    let orphans: Vec<&Schedule> = data.schedules.iter().filter(|x| x.workspace_id.as_deref().is_none_or(|w| data.workspace(w).is_none())).collect();
    if scope.is_empty() && !orphans.is_empty() {
        let rows: Vec<El> = orphans.iter().map(|item| schedule_row(s, t, layer, item)).collect();
        body = body.child(webkit::section(t, "sch-group-none", "Without a workspace", Some(&orphans.len().to_string()), None, webkit::panel(rows)));
    }
    // Rune-only: the page's own "New schedule" (it makes one in the first workspace), the only control in the header.
    let first = data.workspaces[0].id.clone();
    let target = if scope.is_empty() { first } else { scope.to_owned() };
    let new = page_button("New schedule", "sch-new", t, move |s: &mut App| s.open_schedule_dialog(None, &target));
    Page { count: None, trailing: vec![new], body, title: None, back: None }
}

fn schedule_row(s: &App, t: &Neko, layer: &mut Vec<El>, item: &Schedule) -> El {
    let id = item.id.clone();
    let key = format!("schedule:{id}");
    let open = s.row_open(&key);
    let toggle = key.clone();
    let enabled = item.enabled;
    // `{ruleSummary(rule)}` then `{next … | "" | " · paused"}`: two text nodes in the web's JSX.
    let rule = words::rule_summary(&item.rule);
    let extra = match item.next_due_ms {
        Some(due) if enabled => format!(" · next {}", words::relative_time(Some(due), s.now_ms)),
        _ if enabled => String::new(),
        _ => " · paused".to_owned(),
    };
    let when = format!("{rule}{extra}");
    let small = webkit::font(t, 12.5, 400.0, 20.3125);
    let (run, edit, del, sw) = (id.clone(), id.clone(), id.clone(), id.clone());
    let (menu_run, menu_edit, menu_del) = (id.clone(), id.clone(), id.clone());

    let mut when_nodes = row().key(format!("schedule-when-{id}")).help(item.timezone.clone()).child(text(rule.clone()).font(webkit::tnum(small)).color(t.text.muted.alpha(0.7)).a11y_hidden());
    if !extra.is_empty() {
        when_nodes = when_nodes.child(text(extra).font(webkit::tnum(small)).color(t.text.muted.alpha(0.7)).a11y_hidden());
    }
    let button = row()
        .key(format!("schedule-{id}"))
        .role("button")
        .label(format!("{} details for {}", if open { "Hide" } else { "Show" }, item.name))
        .expanded(open)
        .focusable(true)
        .grow(1.0)
        .shrink()
        .min_w(0.0)
        .gap(12.0)
        .center_y()
        .radius(10.0)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.toggle_row(&toggle))
        .child(text(item.name.clone()).font(webkit::font(t, 13.5, 400.0, 21.9375)).color(t.text.base).truncate().min_w(0.0).shrink().a11y_hidden())
        .child(text(item.prompt.clone()).font(small).color(t.text.muted).truncate().grow(1.0).shrink().min_w(0.0).help(item.prompt.clone()).a11y_hidden())
        .child(when_nodes);
    let run_now = move |s: &mut App, id: &str| {
        s.setup_call(SetupAction::RunSchedule { id: id.to_owned() });
        s.show_toast("Queued a read-only plan. It appears as a task.");
    };
    let run_press = webkit::row_action(t, &format!("schedule-run-{id}"), &format!("Run {} now", item.name), &ph::PLAY_REGULAR, move |s: &mut App| run_now(s, &run));
    let edit_press = webkit::row_action(t, &format!("schedule-edit-{id}"), "Edit schedule", &ph::PENCIL_SIMPLE_REGULAR, move |s: &mut App| s.open_schedule_dialog(Some(&edit), ""));
    let more = webkit::more_menu(
        s,
        t,
        layer,
        &format!("schedule-more-{id}"),
        &format!("Actions for {}", item.name),
        false,
        vec![
            Some(("Run now".to_owned(), &ph::PLAY_REGULAR, false, true, std::rc::Rc::new(move |s: &mut App| run_now(s, &menu_run)) as std::rc::Rc<dyn Fn(&mut App)>)),
            Some(("Edit schedule".to_owned(), &ph::PENCIL_SIMPLE_REGULAR, false, true, std::rc::Rc::new(move |s: &mut App| s.open_schedule_dialog(Some(&menu_edit), "")))),
            None,
            Some(("Delete schedule…".to_owned(), &ph::TRASH_REGULAR, true, true, std::rc::Rc::new(move |s: &mut App| s.confirm_delete_schedule(&menu_del)))),
        ],
    );
    let line = vec![
        super::kit::glyph(t, if enabled { StatusKind::InProgress } else { StatusKind::Todo }, if enabled { "Enabled" } else { "Paused" }),
        button,
        webkit::row_actions(vec![run_press, edit_press, more]),
        webkit::toggle(t, &format!("schedule-switch-{id}"), &format!("{} {}", if enabled { "Pause" } else { "Enable" }, item.name), enabled, move |s: &mut App| {
            let on = s.setup_data().and_then(|d| d.schedules.iter().find(|x| x.id == sw)).is_some_and(|x| x.enabled);
            s.setup_call(SetupAction::ScheduleEnabled { id: sw.clone(), enabled: !on })
        }),
    ];

    let mut below: Vec<El> = Vec::new();
    if open {
        below.push(details(s, t, item, &id));
    }
    if let Some(n) = note(s, t, &format!("setup:{id}")) {
        below.push(n);
    }

    // The right-click menu: every action of the row.
    let mut entries: Vec<MenuEntry> = Vec::new();
    let (a, b, c, d) = (id.clone(), id.clone(), id.clone(), id.clone());
    entries.push(("Run now".to_owned(), std::rc::Rc::new(move |s: &mut App| {
        s.setup_call(SetupAction::RunSchedule { id: a.clone() });
        s.show_toast("Queued a read-only plan. It appears as a task.");
    })));
    entries.push(("Edit schedule…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.open_schedule_dialog(Some(&b), ""))));
    entries.push((if enabled { "Pause" } else { "Enable" }.to_owned(), std::rc::Rc::new(move |s: &mut App| s.setup_call(SetupAction::ScheduleEnabled { id: c.clone(), enabled: !enabled }))));
    entries.push(("Delete schedule…".to_owned(), std::rc::Rc::new(move |s: &mut App| s.confirm_delete_schedule(&d))));
    let _ = (when, del);
    with_menu(s, t, layer, &key, webkit::item_row(t, &format!("schedule-block-{id}"), line, below), entries)
}

/// The opened row: the prompt in full (13, at most 70 characters wide), the recurrence and zone, the last run's words, "Last task".
fn details(s: &App, t: &Neko, item: &Schedule, id: &str) -> El {
    let body = webkit::font(t, 13.0, 400.0, 21.125);
    let max = 70.0 * rune::text::measure("0", &[], body, None).0;
    let mut d = col().key(format!("schedule-details-{id}")).w_full().pl(28.0).pb(4.0).gap(8.0);
    d = d.child(text(item.prompt.clone()).key(format!("schedule-prompt-{id}")).font(body).color(t.text.base.alpha(0.85)).wrap().max_w(max).min_w(0.0));
    d = d.child(row().key(format!("schedule-rule-{id}")).child(text(words::rule_summary(&item.rule)).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted)).child(text(" · ".to_owned()).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted)).child(text(item.timezone.clone()).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted)));
    if !item.last_result.trim().is_empty() {
        let line = row().key(format!("schedule-last-{id}")).max_w(max).px(12.0).py(8.0).radius(10.0).fill(t.text.base.alpha(0.03)).gap(6.0).child(text("Last run".to_owned()).font(webkit::font(t, 12.5, 400.0, 20.3125)).color(t.text.muted).a11y_hidden()).child(text(item.last_result.clone()).font(body).color(t.text.base.alpha(0.8)).wrap().min_w(0.0).shrink());
        d = d.child(line);
    }
    if let Some(task) = item.last_task_id.as_deref().filter(|task| s.summary.as_ref().is_some_and(|m| m.task(task).is_some())) {
        let task = task.to_owned();
        d = d.child(row().child(webkit::ghost_button(t, &format!("schedule-last-task-{id}"), "Last task", None, move |s: &mut App| s.select(&task))));
    }
    d
}
