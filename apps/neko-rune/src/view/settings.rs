//! The Settings page: a list of sections at the left (200 wide) and one column of rows (640 wide) at
//! the right. A row is its label at the left and its control at the right; help is a tooltip. ⌘F
//! filters the rows of every section by what the person types. General, AI and agents (the runtime
//! defaults and Inbox clean-up), Budget (the daily ceiling), Phone (Reminders, ntfy, quiet hours,
//! the digest's time), Neko updates, Workspaces and About. Rows the web shows read-only are
//! read-only here. Ported from `pages/setup/Settings.tsx` and its sections; the same requests.

use neko_protocol::agent_models::{CatalogModel, ModelAccess};
use neko_protocol::calendar::CalendarAccess;
use neko_protocol::phone::RemindersAccess;
use rune::{El, ScrollSpec, col, el, row, text};
use rune_kit::TextInput;

use super::ui::{edit_of, editing, note, switch};
use super::webkit::{self, setting_row};
use super::{Frame, band};
use crate::app::App;
use crate::icons;
use crate::pages::Section;
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
    field = if searching { field.clear_button() } else { field.trailing(row().key("settings-search-hint").h(20.0).px(4.0).gap(4.0).center_y().child(text("⌘".to_owned()).font(webkit::font(t, 12.0, 500.0, 16.0)).color(t.text.muted)).child(text("F".to_owned()).font(webkit::font(t, 12.0, 500.0, 16.0)).color(t.text.muted)).a11y_hidden()) };
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
    phone(s, t, layer, &mut out);
    super::settings_system::all(s, t, layer, &mut out);
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
        out.push(item(Section::Phone, "", "phone-loading", "Phone", "", text(s.pages.notes.get("page:phone").map_or("Loading…", String::as_str).to_owned()).key("phone-loading").font(webkit::font(t, 13.0, 400.0, 21.125)).color(t.text.muted)));
        return;
    };
    let start = out.len();
    let p = data.settings.clone();
    let status = &data.status;
    let note_font = webkit::font(t, 12.5, 400.0, 20.3125);

    // Channels: one line each, no subtitles (the web's Phone page has none), then the paragraph about replies.
    out.push(item(Section::Phone, "Channels", "phone-reminders", "Apple Reminders", "", setting_row(t, "set-reminders", "Apple Reminders", None, switch(t, "set-reminders-switch", "Apple Reminders", p.reminders, move |s: &mut App| s.change_phone("reminders", |p| p.reminders = !p.reminders)))));
    out.push(item(Section::Phone, "Channels", "phone-ntfy", "ntfy", "", setting_row(t, "set-ntfy", "ntfy", None, switch(t, "set-ntfy-switch", "ntfy", p.ntfy, move |s: &mut App| s.change_phone("ntfy", |p| p.ntfy = !p.ntfy)))));
    let replies = "Replies need Apple Reminders: complete a reminder to take its default, or add a line that starts with > to answer in words. ntfy only notifies, because answering through it would mean opening your Mac to the internet.";
    out.push(item(Section::Phone, "Channels", "phone-replies", "Replies", replies, col().key("phone-replies").w_full().px(24.0).pt(14.0).child(text(replies.to_owned()).font(note_font).color(t.text.muted).wrap().min_w(0.0))));

    // ntfy, only while it is on.
    if p.ntfy {
        const G: &str = "ntfy";
        out.push(item(Section::Phone, G, "phone-ntfy-server", "Server", "", setting_row(t, "set-ntfy-server", "Server", None, webkit::input(s, t, "ntfy-server", "ntfy server", "", 256.0, false, |s: &mut App| s.save_ntfy_server()))));
        let topic = p.ntfy_topic.clone();
        let mono = rune::TextStyle { size: 12.5, line_height: 20.3125, ..t.ty.mono };
        let copy_topic = topic.clone();
        let copy = |key: &str, label: &'static str, words: String| {
            webkit::ghost_button(t, key, "Copy", None, |_: &mut App| {}).label(label).on_press_tx(move |s: &mut App, tx: &mut rune::Tx| {
                tx.copy(words.clone());
                s.show_toast("Copied");
            })
        };
        let control = row().gap(8.0).center_y().flex_wrap().child(text(topic).key("set-ntfy-topic-value").font(mono).color(t.text.muted).wrap().min_w(0.0)).child(copy("set-ntfy-topic-copy", "Copy topic", copy_topic)).child(webkit::ghost_button(t, "set-ntfy-topic-new", "New topic", None, |s: &mut App| s.change_phone("ntfy-topic", |p| p.ntfy_topic = String::new())));
        out.push(item(Section::Phone, G, "phone-ntfy-topic", "Topic", "", setting_row(t, "set-ntfy-topic", "Topic", None, control)));
        let link = status.subscribe_link.clone();
        let control = row().gap(8.0).center_y().flex_wrap().child(text(link.clone()).key("set-ntfy-link-value").font(mono).color(t.meaning.iris).wrap().min_w(0.0)).child(copy("set-ntfy-link-copy", "Copy subscribe link", link));
        out.push(item(Section::Phone, G, "phone-ntfy-link", "Subscribe", "", setting_row(t, "set-ntfy-link", "Subscribe", None, control)));
        out.push(item(Section::Phone, G, "phone-titles-only", "Send titles only", "", setting_row(t, "set-titles-only", "Send titles only", None, switch(t, "set-titles-switch", "Send titles only", p.titles_only, move |s: &mut App| s.change_phone("titles-only", |p| p.titles_only = !p.titles_only)))));
        out.push(item(Section::Phone, G, "phone-ntfy-open", "Open in Neko address", "", setting_row(t, "set-ntfy-open", "Open in Neko address", None, webkit::input(s, t, "ntfy-open", "Open in Neko address", "Empty: no button", 256.0, false, |s: &mut App| s.save_ntfy_open()))));
    }

    // Morning digest: the switch and the time (HH:MM; Return saves it).
    let digest = p.digest;
    let time = webkit::input(s, t, "digest-time", "Digest time", "08:00", 112.0, false, move |s: &mut App| {
        if digest {
            s.save_digest_time();
        }
    });
    let time = if digest { time } else { time.opacity(0.5) };
    let control = row().gap(8.0).center_y().child(switch(t, "set-digest-switch", "Morning digest", digest, move |s: &mut App| s.change_phone("digest", |p| p.digest = !p.digest))).child(time);
    out.push(item(Section::Phone, "Morning digest", "phone-digest", "Morning digest", "", setting_row(t, "set-digest", "Morning digest", None, control)));

    // When: quiet hours, then a test.
    let hs = hours();
    let on = p.quiet_enabled;
    let from = webkit::select(s, t, layer, "set-quiet-from", "Quiet hours start", &hs, &format!("{:02}:00", p.quiet_from_hour), 96.0, move |s: &mut App, chosen: &str| {
        if on {
            s.change_phone("quiet", |p| p.quiet_from_hour = hour_of(chosen));
        }
    });
    let to = webkit::select(s, t, layer, "set-quiet-to", "Quiet hours end", &hs, &format!("{:02}:00", p.quiet_to_hour), 96.0, move |s: &mut App, chosen: &str| {
        if on {
            s.change_phone("quiet", |p| p.quiet_to_hour = hour_of(chosen));
        }
    });
    let (from, to) = if on { (from, to) } else { (from.opacity(0.5), to.opacity(0.5)) };
    let control = row().gap(8.0).center_y().child(switch(t, "set-quiet-switch", "Quiet hours", on, move |s: &mut App| s.change_phone("quiet", |p| p.quiet_enabled = !p.quiet_enabled))).child(from).child(text("to".to_owned()).font(note_font).color(t.text.muted)).child(to);
    out.push(item(Section::Phone, "When", "phone-quiet", "Quiet hours", "", setting_row(t, "set-quiet", "Quiet hours", None, control)));
    let can_test = p.reminders || p.ntfy;
    let test = webkit::button(t, "phone-test", "Send a test", None, false, false, webkit::gated(can_test, |s: &mut App| s.call(crate::link::Call::PhoneTest)));
    out.push(item(Section::Phone, "When", "phone-test", "Send a test", "", setting_row(t, "set-phone-test", "Send a test", None, if can_test { test } else { test.disabled(true).opacity(0.5) })));

    // A refusal of a change shows under the row that made it.
    for it in &mut out[start..] {
        let row = it.id.trim_start_matches("phone-").to_owned();
        let el = std::mem::replace(&mut it.el, rune::el());
        it.el = with_note(s, t, &format!("setting:phone:{row}"), el);
    }

    // Status: plain lines, not rows.
    let line = webkit::font(t, 13.0, 400.0, 21.125);
    let access = match status.reminders_access {
        RemindersAccess::Granted => "Reminders allowed",
        RemindersAccess::Denied => "Reminders blocked: allow Neko in System Settings → Privacy & Security → Reminders",
        _ if p.reminders => "Waiting for the app to ask for Reminders access",
        _ => "Reminders not asked yet",
    };
    let last = match status.last_push_ms {
        Some(at) => format!("Last push: {}, {}", status.last_channel, webkit::relative_time(s.now_ms, at)),
        None => "No push sent yet".to_owned(),
    } + &if status.held > 0 { format!(", {} waiting for quiet hours to end", status.held) } else { String::new() };
    let mut lines = col().key("phone-status").w_full().gap(4.0).child(text(access.to_owned()).key("phone-access").font(line).color(t.text.muted).wrap().min_w(0.0)).child(text(last).key("phone-last-push").font(line).color(t.text.muted).wrap().min_w(0.0));
    if let Some(e) = status.last_error.as_ref() {
        lines = lines.child(text(format!("Last error: {e}")).key("phone-error").font(line).color(t.meaning.amber).wrap().min_w(0.0));
    }
    for (i, l) in s.pages.phone_test.iter().enumerate() {
        lines = lines.child(text(l.clone()).key(format!("phone-test-line-{i}")).font(line).color(t.text.muted).wrap().min_w(0.0));
    }
    out.push(item(Section::Phone, "", "phone-status", "Status", "", webkit::section(t, "group-status", "Status", None, None, lines)));
}

pub(super) fn short(commit: &Option<String>) -> String {
    commit.as_deref().map_or("unknown".to_owned(), |c| c.chars().take(7).collect())
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
