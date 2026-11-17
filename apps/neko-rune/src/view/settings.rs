//! The Settings page: a list of sections at the left (200 wide) and one column of rows (640 wide) at
//! the right. A row is its label at the left and its control at the right; help is a tooltip. ⌘F
//! filters the rows of every section by what the person types. General, AI and agents (the runtime
//! defaults and Inbox clean-up), Budget (the daily ceiling), Phone (Reminders, ntfy, quiet hours,
//! the digest's time), Neko updates, Workspaces and About. Rows the web shows read-only are
//! read-only here. Ported from `pages/setup/Settings.tsx` and its sections; the same requests.

use neko_protocol::agent_models::{CatalogModel, ModelAccess, SourceStatus};
use neko_protocol::calendar::CalendarAccess;
use neko_protocol::phone::RemindersAccess;
use rune::{El, ScrollSpec, col, icon, row, text};
use rune_kit::{SegmentedControl, TextInput};

use super::ui::{edit_of, editing, note, pick, setting_row, switch, switch_when};
use super::{Frame, band, nav_label_row, primary_button, quiet_button};
use crate::app::App;
use crate::icons;
use crate::pages::{Section, ago};
use crate::permissions::{Access, Pane};
use crate::theme::Neko;

/// One row of a section, with the words search matches against.
pub(super) struct Item {
    pub(super) section: Section,
    pub(super) id: String,
    pub(super) words: String,
    pub(super) el: El,
}

pub(super) fn item(section: Section, id: impl Into<String>, label: &str, help: &str, el: El) -> Item {
    Item { section, id: id.into(), words: format!("{label} {help} {}", section.title()).to_lowercase(), el }
}

/// A read-only value at the right of a row.
fn value(t: &Neko, key: &str, words: &str) -> El {
    text(words.to_owned()).key(key.to_owned()).font(t.ty.meta).color(t.text.muted).truncate().max_w(300.0)
}

fn with_note(s: &App, t: &Neko, key: &str, r: El) -> El {
    match note(s, t, key) {
        Some(n) => col().w_full().child(r).child(n),
        None => r,
    }
}

fn hours() -> Vec<String> {
    (0..24).map(|h| format!("{h:02}:00")).collect()
}

fn hour_of(label: &str) -> u32 {
    label.get(..2).and_then(|h| h.parse().ok()).unwrap_or(0)
}

fn limit_options(values: &[u32], zero: &str, current: Option<u32>) -> Vec<String> {
    let mut all: Vec<u32> = values.to_vec();
    if let Some(c) = current.filter(|c| !all.contains(c)) {
        all.push(c);
        all.sort_unstable();
    }
    std::iter::once(zero.to_owned()).chain(all.iter().map(u32::to_string)).collect()
}

fn limit_of(chosen: &str) -> Option<u32> {
    chosen.parse().ok()
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>, frame: Frame) -> El {
    let query = s.pages.text("settings:search").trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    let all = items(s, t, layer);
    let mut content = col().key("settings-content").w_full().gap(2.0);
    if words.is_empty() {
        for it in all.into_iter().filter(|i| i.section == s.pages.section) {
            content = content.child(it.el);
        }
    } else {
        let mut any = false;
        for it in all.into_iter().filter(|i| words.iter().all(|w| i.words.contains(w))) {
            any = true;
            content = content.child(col().key(format!("hit-{}", it.id)).w_full().child(text(it.section.title().to_owned()).font(t.ty.id).color(t.text.faint).px(6.0)).child(it.el));
        }
        if !any {
            content = content.child(text(format!("No setting matches “{}”.", s.pages.text("settings:search").trim())).key("settings-none").font(t.ty.meta).color(t.text.muted).px(6.0).py(8.0));
        }
    }
    let title = if words.is_empty() { s.pages.section.title() } else { "Search" };
    // Under 760 points the rail narrows to 150 and the sides to 16, so the rows keep their room.
    let narrow = frame.region_w < 760.0;
    let side = if narrow { 16.0 } else { 32.0 };
    let column = col().key("full-settings").w_full().max_w(640.0 + 2.0 * side).px(side).pt(8.0).pb(40.0).gap(16.0).child(text(title.to_owned()).key("title").font(t.ty.title).color(t.text.base).role("heading")).child(content);
    let scroll = col().key("page-scroll").grow(1.0).min_w(0.0).h_full().clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(row().w_full().justify_center().child(column));
    col().w_full().h_full().child(band(t)).child(row().w_full().grow(1.0).min_h(0.0).child(rail(s, t, narrow)).child(scroll))
}

/// The section list, with the search field above it.
fn rail(s: &App, t: &Neko, narrow: bool) -> El {
    let searching = !s.pages.text("settings:search").trim().is_empty();
    let mut field = TextInput::new(edit_of(s, "settings:search")).label("Find a setting").placeholder("Find a setting").width(None).leading_icon(&icons::SEARCH).on_edit(editing("settings:search".to_owned()));
    // The ⌘F hint hides while the field has text (the clear button takes its place), so the two never share the corner.
    field = if searching { field.clear_button() } else { field.trailing(text("⌘F").key("settings-search-hint").font(t.ty.hint).color(t.text.faint).a11y_hidden()) };
    let search = field.build(&t.kit).key("settings-search").w_full();
    // The sections, under the web's two headings (Neko, This Mac); the list scrolls when the window is short.
    let mut rows = Vec::new();
    for section in Section::ALL {
        if let Some(heading) = section.group() {
            rows.push(text(heading.to_owned()).key(format!("settings-group-{}", section.key())).font(t.ty.group).color(t.text.muted).px(10.0).pt(if section == Section::General { 0.0 } else { 12.0 }).pb(4.0));
        }
        let active = !searching && s.pages.section == section;
        rows.push(nav_label_row(section.title(), &format!("settings-section-{}", section.key()), active, t).on_press(move |s: &mut App| s.go_section(section)));
    }
    super::ui::rail("settings", narrow, Some(search), rows)
}

fn items(s: &App, t: &Neko, layer: &mut Vec<El>) -> Vec<Item> {
    let mut out = Vec::new();
    general(s, t, &mut out);
    super::settings_d::general(s, t, &mut out);
    ai(s, t, layer, &mut out);
    super::settings_d::ai(s, t, &mut out);
    appearance(s, t, &mut out);
    super::settings_d::appearance(s, t, layer, &mut out);
    super::settings_d::rulebook(s, t, &mut out);
    super::settings_d::quick_panel(s, t, &mut out);
    budget(s, t, layer, &mut out);
    phone(s, t, layer, &mut out);
    updates(s, t, &mut out);
    super::settings_d::updates(s, t, &mut out);
    workspaces(s, t, &mut out);
    super::settings_d::clipboard(s, t, &mut out);
    permissions(s, t, &mut out);
    diagnostics(s, t, &mut out);
    super::settings_d::diagnostics(s, t, &mut out);
    about(s, t, &mut out);
    out
}

fn general(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let loaded = s.pages.routines.is_some();
    let routines_on = s.pages.routines.as_ref().is_none_or(|d| d.routines_on);
    let help = "Neko checks messages, PRs and your day in the background. Off until you turn it on.";
    out.push(item(
        Section::General,
        "routines-on",
        "Routines",
        help,
        with_note(s, t, "setting:routines-on", setting_row(t, "set-routines-on", "Routines", Some(help), switch_when(t, "set-routines-switch", "Routines", routines_on, loaded, move |s: &mut App| s.call(crate::link::Call::SetRoutinesOn { enabled: !routines_on })))),
    ));
    let cal = s.pages.calendar.as_ref().map(|c| c.status.clone());
    let enabled = cal.as_ref().is_some_and(|c| c.enabled);
    let state = match &cal {
        Some(c) if c.enabled && c.access == CalendarAccess::Granted => "Reading your calendars".to_owned(),
        Some(c) if c.enabled && c.access == CalendarAccess::Denied => "Not allowed in System Settings".to_owned(),
        Some(c) if c.enabled => "Waiting for the app".to_owned(),
        _ => String::new(),
    };
    let help = "Shows your meetings beside Neko’s dated work. Off until you turn it on; the app then asks macOS for access.";
    out.push(item(
        Section::General,
        "calendar-on",
        "Calendar",
        help,
        with_note(
            s,
            t,
            "setting:calendar-on",
            setting_row(t, "set-calendar-on", "Calendar", Some(help), row().gap(10.0).center_y().child(value(t, "set-calendar-state", &state)).child(switch(t, "set-calendar-switch", "Calendar", enabled, move |s: &mut App| s.call(crate::link::Call::SetCalendarOn { enabled: !enabled })))),
        ),
    ));
}

fn usable(m: &CatalogModel) -> bool {
    m.access != ModelAccess::Unavailable
}

fn cap(word: &str) -> String {
    let mut c = word.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
}

fn ai(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    let runtime = s.runtime_shown();
    let provider_now = if runtime.provider.is_empty() { "codex".to_owned() } else { runtime.provider.clone() };
    let catalog = s.pages.catalog.as_ref();
    let sources: Vec<(String, String)> = catalog.map(|c| c.sources.iter().filter(|x| x.status == SourceStatus::Ready || x.provider == provider_now).map(|x| (x.provider.clone(), x.label.clone())).collect()).unwrap_or_default();
    let source = catalog.and_then(|c| c.sources.iter().find(|x| x.provider == provider_now));
    let model = source.and_then(|x| x.models.iter().find(|m| m.id == runtime.model));

    // Runs with.
    let provider_labels: Vec<String> = if sources.is_empty() { vec![cap(&provider_now)] } else { sources.iter().map(|(_, l)| l.clone()).collect() };
    let provider_label = sources.iter().find(|(p, _)| *p == provider_now).map_or_else(|| cap(&provider_now), |(_, l)| l.clone());
    let by = sources.clone();
    let help = "The runtime that does the work for new conversations, tasks and background checks. Work already running isn’t interrupted.";
    out.push(item(
        Section::Ai,
        "runtime",
        "Runs with",
        help,
        setting_row(
            t,
            "set-runtime",
            "Runs with",
            Some(help),
            pick(s, t, layer, "set-runtime-pick", "Agent runtime", &provider_labels, &provider_label, 220.0, move |s: &mut App, chosen: &str| {
                if let Some((p, _)) = by.iter().find(|(_, l)| l == chosen) {
                    let p = p.clone();
                    s.pick_runtime(|r| {
                        r.provider = p;
                        r.model.clear();
                        r.reasoning_effort = None;
                        r.service_tier = None;
                    });
                }
            }),
        ),
    ));

    // Model.
    let mut model_labels = vec!["Default model".to_owned()];
    let mut model_ids = vec![String::new()];
    if let Some(src) = source {
        for m in src.models.iter().filter(|m| usable(m)) {
            model_labels.push(m.label.clone());
            model_ids.push(m.id.clone());
        }
    }
    if !runtime.model.is_empty() && !model_ids.contains(&runtime.model) {
        model_labels.push(runtime.model.clone());
        model_ids.push(runtime.model.clone());
    }
    let model_now = model_ids.iter().position(|m| *m == runtime.model).map_or("Default model".to_owned(), |i| model_labels[i].clone());
    let (labels, ids) = (model_labels.clone(), model_ids.clone());
    out.push(item(
        Section::Ai,
        "model",
        "Model",
        "Leave on Default to let the runtime choose.",
        setting_row(
            t,
            "set-model",
            "Model",
            Some("Leave on Default to let the runtime choose."),
            pick(s, t, layer, "set-model-pick", "Model", &model_labels, &model_now, 220.0, move |s: &mut App, chosen: &str| {
                if let Some(i) = labels.iter().position(|l| l == chosen) {
                    let id = ids[i].clone();
                    s.pick_runtime(|r| {
                        r.model = id;
                        r.reasoning_effort = None;
                        r.service_tier = None;
                    });
                }
            }),
        ),
    ));

    // Reasoning effort and speed: only what the model advertises.
    let efforts: Vec<(String, String)> = model
        .map(|m| match &m.effort_options {
            Some(list) => list.iter().map(|o| (o.id.clone(), o.label.clone())).collect(),
            None => m.reasoning_efforts.iter().map(|e| (e.clone(), cap(e))).collect(),
        })
        .unwrap_or_default();
    let mut effort_labels = vec!["Auto".to_owned()];
    effort_labels.extend(efforts.iter().map(|(_, l)| l.clone()));
    let effort_now = runtime.reasoning_effort.as_ref().and_then(|e| efforts.iter().find(|(id, _)| id == e).map(|(_, l)| l.clone()).or_else(|| Some(e.clone()))).unwrap_or_else(|| "Auto".to_owned());
    let e2 = efforts.clone();
    out.push(item(
        Section::Ai,
        "effort",
        "Reasoning effort",
        "Only the levels this model advertises.",
        setting_row(
            t,
            "set-effort",
            "Reasoning effort",
            Some("Only the levels this model advertises."),
            pick(s, t, layer, "set-effort-pick", "Reasoning effort", &effort_labels, &effort_now, 160.0, move |s: &mut App, chosen: &str| {
                let id = e2.iter().find(|(_, l)| l == chosen).map(|(id, _)| id.clone());
                s.pick_runtime(|r| r.reasoning_effort = id);
            }),
        ),
    ));
    let speeds: Vec<(String, String)> = model.and_then(|m| m.speed_options.as_ref()).map(|list| list.iter().map(|o| (o.id.clone(), o.label.clone())).collect()).unwrap_or_default();
    let mut speed_labels = vec!["Auto".to_owned(), "Normal".to_owned()];
    speed_labels.extend(speeds.iter().map(|(_, l)| l.clone()));
    let speed_now = match runtime.service_tier.as_deref() {
        None => "Auto".to_owned(),
        Some("default") => "Normal".to_owned(),
        Some(id) => speeds.iter().find(|(i, _)| i == id).map_or_else(|| id.to_owned(), |(_, l)| l.clone()),
    };
    let s2 = speeds.clone();
    out.push(item(
        Section::Ai,
        "speed",
        "Speed",
        "Extra speed may use more allowance.",
        setting_row(
            t,
            "set-speed",
            "Speed",
            Some("Extra speed may use more allowance."),
            pick(s, t, layer, "set-speed-pick", "Speed", &speed_labels, &speed_now, 160.0, move |s: &mut App, chosen: &str| {
                let tier = match chosen {
                    "Auto" => None,
                    "Normal" => Some("default".to_owned()),
                    other => s2.iter().find(|(_, l)| l == other).map(|(id, _)| id.clone()),
                };
                s.pick_runtime(|r| r.service_tier = tier);
            }),
        ),
    ));
    if s.runtime_dirty() {
        out.push(item(
            Section::Ai,
            "runtime-use",
            "Use this runtime",
            "Saves the runtime without sending a test message.",
            with_note(s, t, "setting:runtime", row().w_full().px(6.0).py(4.0).gap(8.0).child(primary_button("Use this runtime", "runtime-use", t, |s: &mut App| s.use_runtime())).child(quiet_button("Cancel", "runtime-cancel", t, |s: &mut App| s.pages.runtime_draft = None))),
        ));
    }

    // Work style and the Inbox clean-up.
    let st = s.summary.as_ref().map(|m| m.settings.clone()).unwrap_or_default();
    let plan = st.plan_first;
    let help = "Off: a ticket is a normal session that writes code right away. On: every ticket first gets a read-only plan to approve.";
    out.push(item(Section::Ai, "plan-first", "Plan before working", help, with_note(s, t, "setting:plan-first", setting_row(t, "set-plan-first", "Plan before working", Some(help), switch(t, "set-plan-first-switch", "Plan before working", plan, move |s: &mut App| s.call(crate::link::Call::SetPlanFirst { enabled: !plan }))))));
    let swa = st.start_without_approval;
    let help = "When plans are required, a planned chat or manual ticket starts building without waiting. Nothing is ever published.";
    out.push(item(Section::Ai, "start-without-approval", "Start without approval", help, with_note(s, t, "setting:start-without-approval", setting_row(t, "set-swa", "Start without approval", Some(help), switch(t, "set-swa-switch", "Start without approval", swa, move |s: &mut App| s.call(crate::link::Call::SetStartWithoutApproval { enabled: !swa }))))));
    let days = s.pages.routines.as_ref().map_or(7, |d| d.age_out_days);
    let mut day_labels = vec!["Never".to_owned()];
    for d in [3u32, 7, 14, 30] {
        day_labels.push(format!("{d} days"));
    }
    if days > 0 && ![3, 7, 14, 30].contains(&days) {
        day_labels.push(format!("{days} days"));
    }
    let days_now = if days == 0 { "Never".to_owned() } else { format!("{days} days") };
    let help = "An Inbox item nothing has touched for this long closes itself and moves to Closed recently, where you can reopen it. Items also close when their pull request is merged or closed.";
    out.push(item(
        Section::Ai,
        "inbox-age-out",
        "Inbox items age out after",
        help,
        with_note(
            s,
            t,
            "setting:age-out",
            setting_row(
                t,
                "set-age-out",
                "Inbox items age out after",
                Some(help),
                pick(s, t, layer, "set-age-out-pick", "Inbox items age out after", &day_labels, &days_now, 140.0, |s: &mut App, chosen: &str| {
                    let days = chosen.split(' ').next().and_then(|d| d.parse().ok()).unwrap_or(0);
                    s.call(crate::link::Call::SetInboxAgeOut { days });
                }),
            ),
        ),
    ));
}

#[allow(clippy::too_many_arguments)]
fn money_row(s: &App, t: &Neko, id: &'static str, label: &str, help: &str, placeholder: &str, max: f64, saved: Option<u32>, make: fn(Option<u32>) -> crate::link::Call) -> Item {
    let _ = saved;
    let dirty = s.pages.edited(id, true);
    let mut control = row().gap(6.0).center_y().shrink().child(text("US$").font(t.ty.mono).color(t.text.muted)).child(super::ui::input(s, t, id, label, placeholder, 96.0, move |s: &mut App| s.save_money(id, max, make)));
    if dirty {
        control = control.child(quiet_button("Save", &format!("{id}-save"), t, move |s: &mut App| s.save_money(id, max, make)));
    }
    item(Section::Budget, id, label, help, with_note(s, t, &format!("setting:{id}"), setting_row(t, &format!("set-{id}"), label, Some(help), control)))
}

fn budget(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    let st = s.summary.as_ref().map(|m| m.settings.clone()).unwrap_or_default();
    out.push(money_row(
        s,
        t,
        "task-budget",
        "Stop a task after",
        "Counts the cost Claude Code and OpenCode report for each task and stops it past this amount. Codex subscriptions don’t report a price. Empty is no limit.",
        "No limit",
        1000.0,
        st.task_budget_cents,
        |cents| crate::link::Call::SetTaskBudget { cents },
    ));
    out.push(money_row(
        s,
        t,
        "daily-budget",
        "Pause new work for the day at",
        "A daily ceiling: when today’s reported cost reaches this, new goal turns and routine runs wait until tomorrow and one Inbox item asks whether to raise it. Work already running finishes. Off when empty.",
        "Off",
        10000.0,
        st.daily_budget_cents,
        |cents| crate::link::Call::SetDailyBudget { cents },
    ));
    let running = limit_options(&[1, 2, 3, 5], "No limit", st.max_running);
    let running_now = st.max_running.map_or("No limit".to_owned(), |n| n.to_string());
    let help = "Other tasks wait their turn and start as running ones finish. Work already running is not stopped.";
    out.push(item(
        Section::Budget,
        "tasks-at-once",
        "Tasks at once",
        help,
        with_note(s, t, "setting:max-running", setting_row(t, "set-max-running", "Tasks at once", Some(help), pick(s, t, layer, "set-max-running-pick", "Tasks at once", &running, &running_now, 140.0, |s: &mut App, chosen: &str| s.call(crate::link::Call::SetMaxRunning { limit: limit_of(chosen) })))),
    ));
    let writable = limit_options(&[1, 2, 3, 6, 8], "2 (default)", st.max_writable);
    let writable_now = st.max_writable.map_or("2 (default)".to_owned(), |n| n.to_string());
    let help = "Each writing task has its own copy of the repository. Extra tasks in the same repository wait; planning and questions are not limited.";
    out.push(item(
        Section::Budget,
        "writing-per-repository",
        "Writing per repository",
        help,
        with_note(s, t, "setting:max-writable", setting_row(t, "set-max-writable", "Writing per repository", Some(help), pick(s, t, layer, "set-max-writable-pick", "Writing per repository", &writable, &writable_now, 140.0, |s: &mut App, chosen: &str| s.call(crate::link::Call::SetMaxWritable { limit: limit_of(chosen) })))),
    ));
}

fn phone(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    let Some(data) = &s.pages.phone else {
        out.push(item(Section::Phone, "phone-loading", "Phone", "", text(s.pages.notes.get("page:phone").map_or("Reading the phone settings…", String::as_str).to_owned()).key("phone-loading").font(t.ty.meta).color(t.text.muted).px(6.0).py(8.0)));
        return;
    };
    let start = out.len();
    let p = data.settings.clone();
    let status = &data.status;
    let denied = status.reminders_access == RemindersAccess::Denied;
    let help = "A reminder in the Neko list for each thing that needs you while you are away.";
    out.push(item(
        Section::Phone,
        "phone-reminders",
        "Apple Reminders",
        help,
        setting_row(t, "set-reminders", "Apple Reminders", Some(help), row().gap(10.0).center_y().child(value(t, "set-reminders-state", if denied { "Not allowed in System Settings" } else { "" })).child(switch(t, "set-reminders-switch", "Apple Reminders", p.reminders, move |s: &mut App| s.change_phone("reminders", |p| p.reminders = !p.reminders)))),
    ));
    let help = "Pushes to the ntfy topic Neko made for you. Only the title is sent unless you turn that off.";
    out.push(item(Section::Phone, "phone-ntfy", "ntfy", help, setting_row(t, "set-ntfy", "ntfy", Some(help), switch(t, "set-ntfy-switch", "ntfy", p.ntfy, move |s: &mut App| s.change_phone("ntfy", |p| p.ntfy = !p.ntfy)))));
    out.push(item(
        Section::Phone,
        "phone-ntfy-server",
        "ntfy server",
        "Where pushes are sent. Press Return to save.",
        setting_row(t, "set-ntfy-server", "ntfy server", Some("Where pushes are sent. Press Return to save."), super::ui::input(s, t, "ntfy-server", "ntfy server", "https://ntfy.sh", 240.0, |s: &mut App| s.save_ntfy_server())),
    ));
    out.push(item(Section::Phone, "phone-titles-only", "Send only the title", "The ntfy server sees the text of a push unless this is on.", setting_row(t, "set-titles-only", "Send only the title", Some("The ntfy server sees the text of a push unless this is on."), switch(t, "set-titles-switch", "Send only the title", p.titles_only, move |s: &mut App| s.change_phone("titles-only", |p| p.titles_only = !p.titles_only)))));
    out.push(item(Section::Phone, "phone-quiet", "Quiet hours", "Pushes wait until quiet hours end, except what cannot wait.", setting_row(t, "set-quiet", "Quiet hours", Some("Pushes wait until quiet hours end, except what cannot wait."), switch(t, "set-quiet-switch", "Quiet hours", p.quiet_enabled, move |s: &mut App| s.change_phone("quiet", |p| p.quiet_enabled = !p.quiet_enabled)))));
    if p.quiet_enabled {
        let hs = hours();
        out.push(item(
            Section::Phone,
            "phone-quiet-window",
            "Quiet from and until",
            "",
            setting_row(
                t,
                "set-quiet-window",
                "Quiet from and until",
                None,
                row()
                    .gap(8.0)
                    .center_y()
                    .child(pick(s, t, layer, "set-quiet-from", "Quiet from", &hs, &format!("{:02}:00", p.quiet_from_hour), 100.0, |s: &mut App, chosen: &str| s.change_phone("quiet-window", |p| p.quiet_from_hour = hour_of(chosen))))
                    .child(text("to").font(t.ty.meta).color(t.text.muted))
                    .child(pick(s, t, layer, "set-quiet-to", "Quiet until", &hs, &format!("{:02}:00", p.quiet_to_hour), 100.0, |s: &mut App, chosen: &str| s.change_phone("quiet-window", |p| p.quiet_to_hour = hour_of(chosen)))),
            ),
        ));
    }
    out.push(item(Section::Phone, "phone-digest", "Morning digest", "One calm summary of what finished, what is stuck and what needs a decision.", setting_row(t, "set-digest", "Morning digest", Some("One calm summary of what finished, what is stuck and what needs a decision."), switch(t, "set-digest-switch", "Morning digest", p.digest, move |s: &mut App| s.change_phone("digest", |p| p.digest = !p.digest)))));
    if p.digest {
        let hs = hours();
        let minutes: Vec<String> = ["00", "15", "30", "45"].iter().map(|m| (*m).to_owned()).collect();
        let minute_now = format!("{:02}", p.digest_minute);
        let mut minutes = minutes;
        if !minutes.contains(&minute_now) {
            minutes.push(minute_now.clone());
        }
        out.push(item(
            Section::Phone,
            "phone-digest-time",
            "Digest time",
            "",
            setting_row(
                t,
                "set-digest-time",
                "Digest time",
                None,
                row()
                    .gap(8.0)
                    .center_y()
                    .child(pick(s, t, layer, "set-digest-hour", "Digest hour", &hs, &format!("{:02}:00", p.digest_hour), 100.0, |s: &mut App, chosen: &str| s.change_phone("digest-time", |p| p.digest_hour = hour_of(chosen))))
                    .child(pick(s, t, layer, "set-digest-minute", "Digest minute", &minutes, &minute_now, 80.0, |s: &mut App, chosen: &str| s.change_phone("digest-time", |p| p.digest_minute = chosen.parse().unwrap_or(0)))),
            ),
        ));
    }
    let last = match (status.last_push_ms, status.last_error.as_ref()) {
        (_, Some(e)) => e.clone(),
        (Some(at), None) => format!("{} · {}", status.last_channel, ago(s.now_ms, at)),
        (None, None) => "Nothing sent yet".to_owned(),
    };
    let tests = if s.pages.phone_test.is_empty() { String::new() } else { s.pages.phone_test.join(" · ") };
    out.push(item(
        Section::Phone,
        "phone-test",
        "Send a test",
        "Sends a push on every channel that is on, ignoring quiet hours.",
        setting_row(t, "set-phone-test", "Send a test", Some("Sends a push on every channel that is on, ignoring quiet hours."), row().gap(10.0).center_y().child(value(t, "set-phone-last", if tests.is_empty() { &last } else { &tests })).child(quiet_button("Send a test", "phone-test", t, |s: &mut App| s.call(crate::link::Call::PhoneTest)))),
    ));
    // A refusal of a change shows under the row that made it.
    for it in &mut out[start..] {
        let row = it.id.trim_start_matches("phone-").to_owned();
        let el = std::mem::replace(&mut it.el, rune::el());
        it.el = with_note(s, t, &format!("setting:phone:{row}"), el);
    }
}

fn short(commit: &Option<String>) -> String {
    commit.as_deref().map_or("unknown".to_owned(), |c| c.chars().take(7).collect())
}

fn updates(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let Some(u) = s.summary.as_ref().and_then(|m| m.settings.self_update.clone()) else { return };
    let last = if u.check.at_ms == 0 {
        "Never".to_owned()
    } else if u.check.error.is_empty() {
        ago(s.now_ms, u.check.at_ms)
    } else {
        format!("{} · {}", ago(s.now_ms, u.check.at_ms), u.check.error)
    };
    let busy = u.requested || u.installing.is_some();
    let help = "Looks at the published repository now and applies everything new.";
    out.push(item(
        Section::Updates,
        "update-check",
        "Last check",
        help,
        with_note(
            s,
            t,
            "setting:updates",
            setting_row(t, "set-update-check", "Last check", Some(help), row().gap(10.0).center_y().child(value(t, "set-update-last", &last)).child(if busy { quiet_button("Updating", "update-now", t, |_| {}).disabled(true) } else { quiet_button("Update now", "update-now", t, |s: &mut App| s.call(crate::link::Call::UpdateNow)) })),
        ),
    ));
    if !u.status.is_empty() {
        out.push(item(Section::Updates, "update-status", "Status", "", setting_row(t, "set-update-status", "Status", None, value(t, "set-update-status-value", &u.status))));
    }
    if u.pending.is_some() {
        out.push(item(
            Section::Updates,
            "pending-update",
            "Apply update",
            "An update waits for your click: it touches safety-critical code, or its automatic update is off.",
            setting_row(t, "set-pending", "An update is waiting", Some("It touches safety-critical code, or its automatic update is off."), primary_button("Apply update", "update-apply", t, |s: &mut App| s.call(crate::link::Call::ApplyUpdate))),
        ));
    }
    out.push(item(Section::Updates, "update-version", "Version in use", "", setting_row(t, "set-version", "Version in use", None, value(t, "set-version-value", &format!("app {} · web {}", short(&u.native_commit), short(&u.web_commit))))));
    let (ui, native) = (u.auto_ui, u.auto_native);
    out.push(item(Section::Updates, "auto-update-ui", "Update the web UI automatically", "Rebuilds and swaps in the web UI after a change lands.", setting_row(t, "set-auto-ui", "Update the web UI automatically", Some("Rebuilds and swaps in the web UI after a change lands."), switch(t, "set-auto-ui-switch", "Update the web UI automatically", ui, move |s: &mut App| s.call(crate::link::Call::SetSelfUpdate { auto_ui: !ui, auto_native: native })))));
    out.push(item(Section::Updates, "auto-update-app", "Reinstall the app automatically", "Reinstalls the app once Neko is idle and the Mac is fine to build.", setting_row(t, "set-auto-native", "Reinstall the app automatically", Some("Reinstalls the app once Neko is idle and the Mac is fine to build."), switch(t, "set-auto-native-switch", "Reinstall the app automatically", native, move |s: &mut App| s.call(crate::link::Call::SetSelfUpdate { auto_ui: ui, auto_native: !native })))));
    let dirty = s.pages.edited("dev-checkout", false);
    let mut control = row().gap(6.0).center_y().shrink().child(super::ui::input(s, t, "dev-checkout", "Development checkout", "Not set", 260.0, |s: &mut App| s.save_dev_checkout()));
    if dirty {
        control = control.child(quiet_button("Save", "dev-checkout-save", t, |s: &mut App| s.save_dev_checkout()));
    }
    let help = "The folder where Neko’s own tasks change Neko. A task that lands there is pushed to the published main.";
    out.push(item(Section::Updates, "dev-checkout", "Development checkout", help, with_note(s, t, "setting:dev-checkout", setting_row(t, "set-dev-checkout", "Development checkout", Some(help), control))));
}

fn workspaces(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let list = s.summary.as_ref().map(|m| m.settings.workspaces.clone()).unwrap_or_default();
    if list.is_empty() {
        out.push(item(Section::Workspaces, "workspaces-none", "Workspaces", "", text("No workspaces yet").key("workspaces-none").font(t.ty.meta).color(t.text.muted).px(6.0).py(8.0)));
    }
    for (i, w) in list.iter().enumerate() {
        let n = w.folders.len().max(usize::from(!w.repository.is_empty()));
        let words = format!("{n} {}", if n == 1 { "folder" } else { "folders" });
        let id = format!("workspace-{i}");
        out.push(item(Section::Workspaces, id.clone(), &w.name, &w.repository, setting_row(t, &format!("set-{id}"), &w.name, Some(&w.repository), value(t, &format!("set-{id}-value"), &words))));
    }
}

fn about(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let u = s.summary.as_ref().and_then(|m| m.settings.self_update.clone());
    let version = u.as_ref().map_or("unknown".to_owned(), |u| format!("app {} · web {}", short(&u.native_commit), short(&u.web_commit)));
    out.push(item(Section::About, "about-version", "Version", "", setting_row(t, "set-about-version", "Version", None, value(t, "set-about-version-value", &version))));
    out.push(item(Section::About, "about-link", "Connection to Neko", "", setting_row(t, "set-about-link", "Connection to Neko", None, value(t, "set-about-link-value", &crate::app::status_line(&s.link, s.summary.as_ref())))));
    out.push(item(
        Section::About,
        "about-web",
        "The web UI",
        "The web UI stays the default while this app reaches parity.",
        setting_row(t, "set-about-web", "The web UI", Some("The web UI stays the default while this app reaches parity."), row().gap(8.0).center_y().child(icon(&icons::EXTERNAL, 14.0).color(t.text.muted).a11y_hidden()).child(quiet_button("Open the web UI", "about-open-web", t, |s: &mut App| s.open_in_web(None)))),
    ));
}

/// Appearance: System, Light or Dark. It is the daemon's setting (the web UI's own), so the window and every browser agree.
fn appearance(s: &App, t: &Neko, out: &mut Vec<Item>) {
    const MODES: [&str; 3] = ["system", "light", "dark"];
    let now = s.pages.appearance.as_deref().and_then(|m| MODES.iter().position(|x| *x == m)).unwrap_or(0);
    let help = "System follows macOS. The choice is kept by Neko, so the web UI and this window agree.";
    let control = SegmentedControl::new(&["System", "Light", "Dark"], Some(now)).label("Theme").width(Some(260.0)).on_pick(|s: &mut App, i: usize, _| s.set_appearance(MODES[i.min(2)])).build(&t.kit).key("set-appearance-pick");
    out.push(item(Section::Appearance, "appearance-mode", "Theme", help, with_note(s, t, "setting:appearance", setting_row(t, "set-appearance", "Theme", Some(help), control))));
}

/// Permissions: each kind of access with its status and a button that opens its pane of System Settings. Accessibility,
/// Full Disk Access and Screen Recording are this process's own (asked of macOS, never prompting); Calendar and Reminders
/// are what the daemon already reports.
fn permissions(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let local = s.pages.access.unwrap_or_default();
    let calendar = match s.pages.calendar.as_ref().map(|c| c.status.access) {
        Some(CalendarAccess::Granted) => Access::Granted,
        Some(CalendarAccess::Denied) => Access::Denied,
        _ => Access::Unknown,
    };
    let reminders = match s.pages.phone.as_ref().map(|p| p.status.reminders_access) {
        Some(RemindersAccess::Granted) => Access::Granted,
        Some(RemindersAccess::Denied) => Access::Denied,
        _ => Access::Unknown,
    };
    let rows: [(&str, &str, Access, Pane, &str); 5] = [
        ("accessibility", "Accessibility", local.accessibility, Pane::Accessibility, "Lets Neko paste for you and read the text you have selected."),
        ("full-disk", "Full Disk Access", local.full_disk, Pane::FullDisk, "Allows access to protected folders, including Documents, Desktop and iCloud Drive."),
        ("calendar", "Calendar", calendar, Pane::Calendars, "Shows your meetings beside Neko’s dated work."),
        ("reminders", "Reminders", reminders, Pane::Reminders, "A reminder in the Neko list for each thing that needs you."),
        ("screen-recording", "Screen Recording", local.screen_recording, Pane::ScreenRecording, "Only for agents you let see the screen."),
    ];
    for (id, label, access, pane, help) in rows {
        let ink = match access {
            Access::Granted => t.meaning.mint,
            Access::Denied => t.meaning.amber,
            Access::Unknown => t.text.muted,
        };
        let control = row()
            .gap(10.0)
            .center_y()
            .child(text(access.words()).key(format!("perm-{id}-state")).font(t.ty.meta).color(ink))
            .child(quiet_button("Open System Settings", &format!("perm-{id}-open"), t, move |s: &mut App| {
                if (s.opener)(pane.url()).is_err() {
                    s.notice = Some("Couldn’t open System Settings.".to_owned());
                }
            }));
        out.push(item(Section::Permissions, format!("perm-{id}"), label, help, setting_row(t, &format!("set-perm-{id}"), label, Some(help), control)));
    }
}

/// Diagnostics: the versions in use, the data folder, whether the daemon answers, and one button that copies it all.
fn diagnostics(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let home = std::env::var("HOME").unwrap_or_default();
    let data = crate::terminal::tilde(&neko_protocol::support_dir().to_string_lossy(), &home);
    let pin = include_str!("../../RUNE_REV").lines().next().unwrap_or("").chars().take(9).collect::<String>();
    let app = format!("Neko for Rune {} · Rune {pin}", env!("CARGO_PKG_VERSION"));
    let u = s.summary.as_ref().and_then(|m| m.settings.self_update.clone());
    let neko = u.as_ref().map_or("unknown".to_owned(), |u| format!("app {} · web {}", short(&u.native_commit), short(&u.web_commit)));
    let daemon = crate::app::status_line(&s.link, s.summary.as_ref());
    let rows = [("app", "This app", app), ("neko", "Neko", neko), ("data", "Data folder", data), ("daemon", "Daemon", daemon)];
    for (id, label, words) in &rows {
        out.push(item(Section::Diagnostics, format!("diag-{id}"), label, words, setting_row(t, &format!("set-diag-{id}"), label, None, value(t, &format!("diag-{id}-value"), words))));
    }
    let report: String = rows.iter().map(|(_, label, words)| format!("{label}: {words}")).collect::<Vec<_>>().join("\n");
    let button = quiet_button("Copy diagnostics", "diag-copy", t, |_: &mut App| {}).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
        tx.copy(report.clone());
        s.show_toast("Copied the diagnostics");
    });
    out.push(item(Section::Diagnostics, "diag-copy", "Copy diagnostics", "Versions, the data folder and the daemon's state, as text to paste into a report.", setting_row(t, "set-diag-copy", "Copy diagnostics", Some("Versions, the data folder and the daemon's state, as text to paste into a report."), button)));
}
