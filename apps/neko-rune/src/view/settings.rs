//! The Settings page: a list of sections at the left (200 wide) and one column of rows (640 wide) at
//! the right. A row is its label at the left and its control at the right; help is a tooltip. ⌘F
//! filters the rows of every section by what the person types. General, AI and agents (the runtime
//! defaults and Inbox clean-up), Budget (the daily ceiling), Phone (Reminders, ntfy, quiet hours,
//! the digest's time), Neko updates, Workspaces and About. Rows the web shows read-only are
//! read-only here. Ported from `pages/setup/Settings.tsx` and its sections; the same requests.

use neko_protocol::agent_models::{CatalogModel, ModelAccess};
use neko_protocol::calendar::CalendarAccess;
use neko_protocol::phone::RemindersAccess;
use rune::{El, ScrollSpec, col, el, icon, row, text};
use rune_kit::TextInput;

use super::ui::{edit_of, editing, note, pick, switch};
use super::webkit::{self, setting_row};
use super::{Frame, band, primary_button, quiet_button};
use crate::app::App;
use crate::icons;
use crate::pages::{Section, ago};
use crate::permissions::{Access, Pane};
use crate::theme::Neko;

/// One row of a section, with the words search matches against.
pub(super) struct Item {
    pub(super) section: Section,
    /// The title of the web `<Section title="...">` this row sits under; "" for an item that brings its own section.
    pub(super) group: &'static str,
    pub(super) id: String,
    pub(super) words: String,
    pub(super) el: El,
}

pub(super) fn item(section: Section, group: &'static str, id: impl Into<String>, label: &str, help: &str, el: El) -> Item {
    Item { section, group, id: id.into(), words: format!("{label} {help} {}", section.title()).to_lowercase(), el }
}

/// A read-only value at the right of a row.
pub(super) fn value(t: &Neko, key: &str, words: &str) -> El {
    text(words.to_owned()).key(key.to_owned()).font(t.ty.meta).color(t.text.muted).truncate().max_w(300.0)
}

pub(super) fn with_note(s: &App, t: &Neko, key: &str, r: El) -> El {
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

pub(super) fn limit_options(values: &[u32], zero: &str, current: Option<u32>) -> Vec<String> {
    let mut all: Vec<u32> = values.to_vec();
    if let Some(c) = current.filter(|c| !all.contains(c)) {
        all.push(c);
        all.sort_unstable();
    }
    std::iter::once(zero.to_owned()).chain(all.iter().map(u32::to_string)).collect()
}

pub(super) fn limit_of(chosen: &str) -> Option<u32> {
    chosen.parse().ok()
}

/// A group's title as a key: lowercase, anything that is not a letter or digit becomes a dash.
fn slug(group: &str) -> String {
    group.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect()
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>, frame: Frame) -> El {
    let query = s.pages.text("settings:search").trim().to_lowercase();
    let words: Vec<&str> = query.split_whitespace().collect();
    super::webkit::set_page_width(frame.region_w);
    let all = items(s, t, layer);
    let mut content = col().key("settings-content").w_full().gap(if words.is_empty() { 32.0 } else { 2.0 });
    if words.is_empty() {
        // The web has no Budget section (its settings are "Budget & limits" in "AI & agents") and no Workspaces one (a Setup page).
        let shown = |i: &Item| i.section == s.pages.section;
        // One block per web `<Section>`, in the order its first row appears; an item with no group already is a section.
        let mut blocks: Vec<(&'static str, Vec<El>)> = Vec::new();
        for it in all.into_iter().filter(shown) {
            match blocks.iter_mut().find(|(g, _)| !g.is_empty() && *g == it.group) {
                Some((_, rows)) => rows.push(it.el),
                None => blocks.push((it.group, vec![it.el])),
            }
        }
        for (group, rows) in blocks {
            if group.is_empty() {
                content = content.children(rows);
            } else {
                content = content.child(webkit::section(t, &format!("group-{}", slug(group)), group, None, None, webkit::panel(rows)));
            }
        }
    } else {
        let mut any = false;
        for it in all.into_iter().filter(|i| i.section != Section::Workspaces).filter(|i| words.iter().all(|w| i.words.contains(w))) {
            any = true;
            content = content.child(col().key(format!("hit-{}", it.id)).w_full().child(text(it.section.title().to_owned()).font(t.ty.id).color(t.text.faint).px(12.0)).child(it.el));
        }
        if !any {
            content = content.child(text(format!("No setting matches “{}”.", s.pages.text("settings:search").trim())).key("settings-none").font(t.ty.meta).color(t.text.muted).px(6.0).py(8.0));
        }
    }
    let title = if words.is_empty() { s.pages.section.title() } else { "Search" };
    // The web's two layouts (`min-[900px]`): under 900 the sections are a strip of pills under the search box; from 900 a 240 point rail.
    let wide = frame.region_w >= 900.0;
    let mut column = col().key("settings-body").w_full().grow(1.0).min_h(0.0);
    column = column.child(breadcrumb(t, title));
    let side = if wide { 240.0 } else { 0.0 };
    // `w-full max-w-[52rem] px-10 pt-6 pb-24`: at most 832 wide counting the 40 point sides.
    let inner = (frame.region_w - side - if wide { 1.0 } else { 0.0 }).min(832.0) - 80.0;
    let page = col().key("full-settings").w(inner + 80.0).px(40.0).pt(24.0).pb(96.0).gap(12.0).child(content);
    let scroll = col().key("page-scroll").grow(1.0).min_w(0.0).h_full().clip_rounded(0.0).scroll(ScrollSpec { stick_bottom: false }).child(page);
    let rail = rail(s, t, wide);
    column = if wide { column.child(row().w_full().grow(1.0).min_h(0.0).child(rail).child(scroll)) } else { column.child(rail).child(scroll) };
    col().w_full().h_full().child(band(t)).child(column)
}

/// `Settings / Appearance`: 48 high, 24 at the sides, 14 point words 8 apart (`<header>` in `Settings.tsx`).
fn breadcrumb(t: &Neko, current: &str) -> El {
    let plain = rune::TextStyle { size: 14.0, line_height: 20.0, ..t.ty.row };
    row()
        .key("settings-breadcrumb")
        .w_full()
        .h(48.0)
        .px(24.0)
        .gap(8.0)
        .center_y()
        .child(text("Settings".to_owned()).font(plain).color(t.text.muted))
        .child(text("/".to_owned()).font(plain).color(t.text.muted.alpha(0.5)).a11y_hidden())
        .child(text(current.to_owned()).key("title").font(rune::TextStyle { weight: 500.0, ..plain }).color(t.text.base).role("heading"))
}

/// The search box and the section list: a strip of pills with a line under it, or a 240 point rail with a line beside it.
fn rail(s: &App, t: &Neko, wide: bool) -> El {
    let searching = !s.pages.text("settings:search").trim().is_empty();
    let mut field = TextInput::new(edit_of(s, "settings:search")).label("Find a setting").placeholder("Find a setting").width(None).leading_icon(&icons::SEARCH).on_edit(editing("settings:search".to_owned()));
    // The ⌘F hint hides while the field has text (the clear button takes its place), so the two never share the corner.
    field = if searching { field.clear_button() } else { field.trailing(text("⌘ F").key("settings-search-hint").font(t.ty.hint).color(t.text.muted).a11y_hidden()) };
    let search = field.build(&t.kit).key("settings-search").w_full().h(40.0).radius(24.0).fill(t.text.base.alpha(0.05));
    let pill = |section: Section| {
        let active = !searching && s.pages.section == section;
        let ink = if active { t.text.base } else { t.text.base.alpha(0.65) };
        row()
            .key(format!("settings-section-{}", section.key()))
            .role("button")
            .label(section.title())
            .focusable(true)
            .h(36.0)
            .px(if wide { 12.0 } else { 16.0 })
            .center_y()
            .radius(if wide { 8.0 } else { 18.0 })
            .fill(if active { t.text.base.alpha(0.07) } else { rune::Color::TRANSPARENT })
            .hover(t.surface.hover)
            .focus_ring(2.0, 2.0, t.kit.c.border_focus)
            .on_press(move |s: &mut App| s.go_section(section))
            .child(text(section.title().to_owned()).font(rune::TextStyle { size: 13.5, line_height: 21.9375, weight: if active { 500.0 } else { 400.0 }, ..t.ty.row }).color(ink).a11y_hidden())
    };
    if !wide {
        let strip = row().key("settings-nav").mx(-4.0).h(40.0).px(4.0).pb(4.0).gap(4.0).scroll_x().children(Section::ALL.iter().map(|&section| pill(section)));
        return col().key("settings-rail").w_full().px(16.0).pt(12.0).pb(8.0).gap(12.0).child(search).child(strip).child(el().w_full().h(1.0).mt(-12.0).fill(t.surface.border));
    }
    let mut groups = col().key("settings-nav").w_full().gap(16.0);
    for group in ["Neko", "This Mac"] {
        let mut g = col().w_full().gap(2.0).child(text(group.to_owned()).key(format!("settings-group-{group}")).font(rune::TextStyle { size: 12.0, line_height: 16.0, weight: 500.0, ..t.ty.meta }).color(t.text.muted).px(12.0).pb(4.0));
        for section in Section::ALL.iter().copied().filter(|s| s.group_name() == group) {
            g = g.child(pill(section));
        }
        groups = groups.child(g);
    }
    row().key("settings-rail").w(241.0).h_full().child(col().w(240.0).h_full().px(12.0).pt(24.0).pb(24.0).gap(16.0).scroll(ScrollSpec { stick_bottom: false }).child(search).child(groups)).child(el().w(1.0).h_full().fill(t.surface.border))
}

fn items(s: &App, t: &Neko, layer: &mut Vec<El>) -> Vec<Item> {
    let mut out = Vec::new();
    general(s, t, &mut out);
    super::settings_d::general(s, t, &mut out);
    super::settings_ai::all(s, t, layer, &mut out);
    appearance(s, t, &mut out);
    super::settings_d::appearance(s, t, layer, &mut out);
    super::settings_d::rulebook(s, t, &mut out);
    super::settings_d::quick_panel(s, t, &mut out);
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
    let help = "Background checks of messages, pull requests and your day. Off until you turn them on.";
    let control = if loaded { switch(t, "set-routines-switch", "Routines", routines_on, move |s: &mut App| s.call(crate::link::Call::SetRoutinesOn { enabled: !routines_on })) } else { webkit::skeleton(t, "set-routines-loading", 36.0, 20.0) };
    out.push(item(Section::General, "Setup", "routines-on", "Routines", help, with_note(s, t, "setting:routines-on", setting_row(t, "set-routines-on", "Routines", Some(help), control))));
    let cal = s.pages.calendar.as_ref().map(|c| c.status.clone());
    let enabled = cal.as_ref().is_some_and(|c| c.enabled);
    // What the app reports about macOS access: not in the web's row, so it shows only when there is something to say.
    let state = match &cal {
        Some(c) if c.enabled && c.access == CalendarAccess::Granted => "Reading your calendars".to_owned(),
        Some(c) if c.enabled && c.access == CalendarAccess::Denied => "Not allowed in System Settings".to_owned(),
        Some(c) if c.enabled => "Waiting for the app".to_owned(),
        _ => String::new(),
    };
    let help = "Show your macOS Calendar beside Neko’s work. macOS asks once.";
    let toggle = if cal.is_some() { switch(t, "set-calendar-switch", "Calendar", enabled, move |s: &mut App| s.call(crate::link::Call::SetCalendarOn { enabled: !enabled })) } else { webkit::skeleton(t, "set-calendar-loading", 36.0, 20.0) };
    let mut control = row().gap(10.0).center_y();
    if !state.is_empty() {
        control = control.child(value(t, "set-calendar-state", &state));
    }
    out.push(item(Section::General, "Setup", "calendar-on", "Calendar", help, with_note(s, t, "setting:calendar-on", setting_row(t, "set-calendar-on", "Calendar", Some(help), control.child(toggle)))));
}

pub(super) fn usable(m: &CatalogModel) -> bool {
    m.access != ModelAccess::Unavailable
}

pub(super) fn cap(word: &str) -> String {
    let mut c = word.chars();
    c.next().map_or(String::new(), |f| f.to_uppercase().collect::<String>() + c.as_str())
}

fn phone(s: &App, t: &Neko, layer: &mut Vec<El>, out: &mut Vec<Item>) {
    let Some(data) = &s.pages.phone else {
        out.push(item(Section::Phone, "", "phone-loading", "Phone", "", text(s.pages.notes.get("page:phone").map_or("Reading the phone settings…", String::as_str).to_owned()).key("phone-loading").font(t.ty.meta).color(t.text.muted).px(6.0).py(8.0)));
        return;
    };
    let start = out.len();
    let p = data.settings.clone();
    let status = &data.status;
    let denied = status.reminders_access == RemindersAccess::Denied;
    let help = "A reminder in the Neko list for each thing that needs you while you are away.";
    out.push(item(
        Section::Phone,
        "Channels",
        "phone-reminders",
        "Apple Reminders",
        help,
        setting_row(t, "set-reminders", "Apple Reminders", Some(help), row().gap(10.0).center_y().child(value(t, "set-reminders-state", if denied { "Not allowed in System Settings" } else { "" })).child(switch(t, "set-reminders-switch", "Apple Reminders", p.reminders, move |s: &mut App| s.change_phone("reminders", |p| p.reminders = !p.reminders)))),
    ));
    let help = "Pushes to the ntfy topic Neko made for you. Only the title is sent unless you turn that off.";
    out.push(item(Section::Phone, "Channels", "phone-ntfy", "ntfy", help, setting_row(t, "set-ntfy", "ntfy", Some(help), switch(t, "set-ntfy-switch", "ntfy", p.ntfy, move |s: &mut App| s.change_phone("ntfy", |p| p.ntfy = !p.ntfy)))));
    out.push(item(
        Section::Phone,
        "ntfy",
        "phone-ntfy-server",
        "ntfy server",
        "Where pushes are sent. Press Return to save.",
        setting_row(t, "set-ntfy-server", "ntfy server", Some("Where pushes are sent. Press Return to save."), super::ui::input(s, t, "ntfy-server", "ntfy server", "https://ntfy.sh", 240.0, |s: &mut App| s.save_ntfy_server())),
    ));
    out.push(item(Section::Phone, "ntfy", "phone-titles-only", "Send only the title", "The ntfy server sees the text of a push unless this is on.", setting_row(t, "set-titles-only", "Send only the title", Some("The ntfy server sees the text of a push unless this is on."), switch(t, "set-titles-switch", "Send only the title", p.titles_only, move |s: &mut App| s.change_phone("titles-only", |p| p.titles_only = !p.titles_only)))));
    out.push(item(Section::Phone, "When", "phone-quiet", "Quiet hours", "Pushes wait until quiet hours end, except what cannot wait.", setting_row(t, "set-quiet", "Quiet hours", Some("Pushes wait until quiet hours end, except what cannot wait."), switch(t, "set-quiet-switch", "Quiet hours", p.quiet_enabled, move |s: &mut App| s.change_phone("quiet", |p| p.quiet_enabled = !p.quiet_enabled)))));
    if p.quiet_enabled {
        let hs = hours();
        out.push(item(
            Section::Phone,
            "When",
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
    out.push(item(Section::Phone, "Morning digest", "phone-digest", "Morning digest", "One calm summary of what finished, what is stuck and what needs a decision.", setting_row(t, "set-digest", "Morning digest", Some("One calm summary of what finished, what is stuck and what needs a decision."), switch(t, "set-digest-switch", "Morning digest", p.digest, move |s: &mut App| s.change_phone("digest", |p| p.digest = !p.digest)))));
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
            "Morning digest",
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
        "When",
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
        "Neko updates",
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
        out.push(item(Section::Updates, "Neko updates", "update-status", "Status", "", setting_row(t, "set-update-status", "Status", None, value(t, "set-update-status-value", &u.status))));
    }
    if u.pending.is_some() {
        out.push(item(
            Section::Updates,
            "Neko updates",
            "pending-update",
            "Apply update",
            "An update waits for your click: it touches safety-critical code, or its automatic update is off.",
            setting_row(t, "set-pending", "An update is waiting", Some("It touches safety-critical code, or its automatic update is off."), primary_button("Apply update", "update-apply", t, |s: &mut App| s.call(crate::link::Call::ApplyUpdate))),
        ));
    }
    out.push(item(Section::Updates, "Neko updates", "update-version", "Version in use", "", setting_row(t, "set-version", "Version in use", None, value(t, "set-version-value", &format!("app {} · web {}", short(&u.native_commit), short(&u.web_commit))))));
    let (ui, native) = (u.auto_ui, u.auto_native);
    out.push(item(Section::Updates, "Neko updates", "auto-update-ui", "Update the web UI automatically", "Rebuilds and swaps in the web UI after a change lands.", setting_row(t, "set-auto-ui", "Update the web UI automatically", Some("Rebuilds and swaps in the web UI after a change lands."), switch(t, "set-auto-ui-switch", "Update the web UI automatically", ui, move |s: &mut App| s.call(crate::link::Call::SetSelfUpdate { auto_ui: !ui, auto_native: native })))));
    out.push(item(Section::Updates, "Neko updates", "auto-update-app", "Reinstall the app automatically", "Reinstalls the app once Neko is idle and the Mac is fine to build.", setting_row(t, "set-auto-native", "Reinstall the app automatically", Some("Reinstalls the app once Neko is idle and the Mac is fine to build."), switch(t, "set-auto-native-switch", "Reinstall the app automatically", native, move |s: &mut App| s.call(crate::link::Call::SetSelfUpdate { auto_ui: ui, auto_native: !native })))));
    let dirty = s.pages.edited("dev-checkout", false);
    let mut control = row().gap(6.0).center_y().shrink().child(super::ui::input(s, t, "dev-checkout", "Development checkout", "Not set", 260.0, |s: &mut App| s.save_dev_checkout()));
    if dirty {
        control = control.child(quiet_button("Save", "dev-checkout-save", t, |s: &mut App| s.save_dev_checkout()));
    }
    let help = "The folder where Neko’s own tasks change Neko. A task that lands there is pushed to the published main.";
    out.push(item(Section::Updates, "Neko updates", "dev-checkout", "Development checkout", help, with_note(s, t, "setting:dev-checkout", setting_row(t, "set-dev-checkout", "Development checkout", Some(help), control))));
}

fn workspaces(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let list = s.summary.as_ref().map(|m| m.settings.workspaces.clone()).unwrap_or_default();
    if list.is_empty() {
        out.push(item(Section::Workspaces, "", "workspaces-none", "Workspaces", "", text("No workspaces yet").key("workspaces-none").font(t.ty.meta).color(t.text.muted).px(6.0).py(8.0)));
    }
    for (i, w) in list.iter().enumerate() {
        let n = w.folders.len().max(usize::from(!w.repository.is_empty()));
        let words = format!("{n} {}", if n == 1 { "folder" } else { "folders" });
        let id = format!("workspace-{i}");
        out.push(item(Section::Workspaces, "", id.clone(), &w.name, &w.repository, setting_row(t, &format!("set-{id}"), &w.name, Some(&w.repository), value(t, &format!("set-{id}-value"), &words))));
    }
}

fn about(s: &App, t: &Neko, out: &mut Vec<Item>) {
    let u = s.summary.as_ref().and_then(|m| m.settings.self_update.clone());
    let version = u.as_ref().map_or("unknown".to_owned(), |u| format!("app {} · web {}", short(&u.native_commit), short(&u.web_commit)));
    out.push(item(Section::About, "About", "about-version", "Version", "", setting_row(t, "set-about-version", "Version", None, value(t, "set-about-version-value", &version))));
    out.push(item(Section::About, "About", "about-link", "Connection to Neko", "", setting_row(t, "set-about-link", "Connection to Neko", None, value(t, "set-about-link-value", &crate::app::status_line(&s.link, s.summary.as_ref())))));
    out.push(item(
        Section::About,
        "About",
        "about-web",
        "The web UI",
        "The web UI stays the default while this app reaches parity.",
        setting_row(t, "set-about-web", "The web UI", Some("The web UI stays the default while this app reaches parity."), row().gap(8.0).center_y().child(icon(&icons::EXTERNAL, 14.0).color(t.text.muted).a11y_hidden()).child(quiet_button("Open the web UI", "about-open-web", t, |s: &mut App| s.open_in_web(None)))),
    ));
}

/// Appearance, Mode: System, Light or Dark. It is the daemon's setting (the web UI's own), so the window and every browser agree.
fn appearance(s: &App, t: &Neko, out: &mut Vec<Item>) {
    use super::webkit::button;
    const MODES: [(&str, &str, &icons::IconRef); 3] = [("system", "System", &icons::CIRCLE_HALF), ("light", "Light", &icons::SUN), ("dark", "Dark", &icons::MOON)];
    let now = s.pages.appearance.as_deref().unwrap_or("system");
    let (light_id, dark_id) = s.theme_choice();
    let (dark_now, name) = match now {
        "light" => (false, light_id),
        "dark" => (true, dark_id),
        _ if t.dark => (true, dark_id),
        _ => (false, light_id),
    };
    let shown = s.theme_entries().into_iter().find(|e| e.id == name).map(|e| e.name).unwrap_or_default();
    let mut modes = row().role("radiogroup").label("Mode").gap(6.0);
    for (mode, label, glyph) in MODES {
        modes = modes.child(button(t, &format!("set-appearance-{mode}"), label, Some(glyph), false, now == mode, move |s: &mut App| s.set_appearance(mode)).role("radio").checked(now == mode));
    }
    let help = "System follows macOS. The choice is kept by Neko, so the web UI and this window agree.";
    let line = setting_row(t, "set-appearance", "Mode", Some(&format!("Showing {shown} ({}).", if dark_now { "dark" } else { "light" })), modes);
    out.push(item(Section::Appearance, "Appearance", "appearance-mode", "Theme Mode", help, with_note(s, t, "setting:appearance", line)));
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
        out.push(item(Section::Permissions, "What Neko can access", format!("perm-{id}"), label, help, setting_row(t, &format!("set-perm-{id}"), label, Some(help), control)));
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
        out.push(item(Section::Diagnostics, "Diagnostics", format!("diag-{id}"), label, words, setting_row(t, &format!("set-diag-{id}"), label, None, value(t, &format!("diag-{id}-value"), words))));
    }
    let report: String = rows.iter().map(|(_, label, words)| format!("{label}: {words}")).collect::<Vec<_>>().join("\n");
    let button = quiet_button("Copy diagnostics", "diag-copy", t, |_: &mut App| {}).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
        tx.copy(report.clone());
        s.show_toast("Copied the diagnostics");
    });
    out.push(item(Section::Diagnostics, "Diagnostics", "diag-copy", "Copy diagnostics", "Versions, the data folder and the daemon's state, as text to paste into a report.", setting_row(t, "set-diag-copy", "Copy diagnostics", Some("Versions, the data folder and the daemon's state, as text to paste into a report."), button)));
}
