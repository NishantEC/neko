//! The Calendar page: a week (or a day) of the macOS Calendar the daemon holds, read-only. Hours
//! 07:00 to 22:00 at 48 points each; an all-day row on top holds Neko's dated work with its status
//! glyphs (tasks on the day they finished, routines' coming runs); events are mint blocks with their
//! titles and the current time is an Iris line. Pressing an event offers "Open in Calendar".
//! Ported from `pages/Calendar.tsx` and `docs/knowledge/calendar.md`; the same requests, no writes.

use neko_protocol::calendar::{CalendarAccess, CalendarEvent, NekoCalendarItem, NekoItemKind};
use rune::{El, col, el, icon, row, text};

use super::ui::{page_wide, quiet_line};
use super::{quiet_button, webkit};
use crate::app::App;
use crate::clock;
use crate::icons;
use crate::pages::CalMode;
use crate::status::{StatusKind, status_glyph};
use crate::theme::Neko;
use crate::views::FullView;

pub const HOUR_H: f32 = 48.0;
pub const FIRST_HOUR: u32 = 7;
pub const LAST_HOUR: u32 = 22;
/// The width of the hour labels at the left of the grid.
const AXIS: f32 = 44.0;

pub fn grid_height() -> f32 {
    (LAST_HOUR - FIRST_HOUR) as f32 * HOUR_H
}

/// Where a moment sits in a day's column, in points from its top (clamped to the hours shown).
pub fn y_of(ms: i64, day_start: i64) -> f32 {
    let l = clock::local(ms);
    let hours = l.hour as f32 + l.minute as f32 / 60.0;
    // A moment on another day than the column's lands at its edge.
    let hours = if ms < day_start { 0.0 } else { hours };
    ((hours - FIRST_HOUR as f32) * HOUR_H).clamp(0.0, grid_height())
}

/// Overlapping events share a column side by side: the lane each takes, and how many lanes there are.
pub fn lanes(events: &[&CalendarEvent]) -> Vec<(usize, usize)> {
    let mut sorted: Vec<usize> = (0..events.len()).collect();
    sorted.sort_by_key(|&i| (events[i].start_ms, events[i].end_ms));
    let mut lane_end: Vec<i64> = Vec::new();
    let mut lane_of = vec![0usize; events.len()];
    for &i in &sorted {
        let at = lane_end.iter().position(|&end| end <= events[i].start_ms).unwrap_or_else(|| {
            lane_end.push(0);
            lane_end.len() - 1
        });
        lane_end[at] = events[i].end_ms.max(events[i].start_ms + 15 * 60_000);
        lane_of[i] = at;
    }
    // A lane count per event: the widest set of events that overlap it.
    (0..events.len())
        .map(|i| {
            let overlapping = (0..events.len()).filter(|&j| events[j].start_ms < events[i].end_ms.max(events[i].start_ms + 15 * 60_000) && events[i].start_ms < events[j].end_ms.max(events[j].start_ms + 15 * 60_000));
            let most = overlapping.map(|j| lane_of[j] + 1).max().unwrap_or(1).max(lane_of[i] + 1);
            (lane_of[i], most)
        })
        .collect()
}

/// Groups of events that overlap one another (a group's span is where its lanes are shared).
pub fn clusters<'a>(events: &[&'a CalendarEvent]) -> Vec<Vec<&'a CalendarEvent>> {
    let mut sorted: Vec<&CalendarEvent> = events.to_vec();
    sorted.sort_by_key(|e| (e.start_ms, e.end_ms));
    let mut out: Vec<Vec<&CalendarEvent>> = Vec::new();
    let mut end = i64::MIN;
    for e in sorted {
        let e_end = e.end_ms.max(e.start_ms + 15 * 60_000);
        if out.is_empty() || e.start_ms >= end {
            out.push(Vec::new());
            end = e_end;
        } else {
            end = end.max(e_end);
        }
        if let Some(last) = out.last_mut() {
            last.push(e);
        }
    }
    out
}

fn days_shown(s: &App) -> Vec<i64> {
    let (from, to) = s.calendar_range();
    let mut days = Vec::new();
    let mut d = from;
    while d < to && days.len() < 42 {
        days.push(d);
        d = clock::add_days(d, 1);
    }
    days
}

fn kind_of(status: &str) -> StatusKind {
    match status {
        "Completed" => StatusKind::Done,
        "ReadyForReview" => StatusKind::Review,
        "Failed" => StatusKind::Failed,
        _ => StatusKind::Todo,
    }
}

fn day_items(items: &[NekoCalendarItem], day: i64, next: i64) -> Vec<&NekoCalendarItem> {
    items.iter().filter(|i| i.at_ms >= day && i.at_ms < next).collect()
}

/// ISO 8601 week number (weeks start on Monday; week 1 holds the first Thursday), as `isoWeek` in `Calendar.tsx`.
pub fn iso_week(year: i32, month: u32, day: u32) -> u32 {
    // Days since 1970-01-01 of the civil date (Howard Hinnant's algorithm).
    let days_from_civil = |y: i32, m: u32, d: u32| -> i64 {
        let y = i64::from(if m <= 2 { y - 1 } else { y });
        let era = y.div_euclid(400);
        let yoe = y - era * 400;
        let mp = i64::from((m + 9) % 12);
        let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    };
    let days = days_from_civil(year, month, day);
    // 1970-01-01 was a Thursday; Monday is 1 .. Sunday is 7.
    let weekday = (days + 3).rem_euclid(7) + 1;
    let thursday = days + 4 - weekday;
    // The year the Thursday is in: the latest January 1st that is not after it.
    let jan1 = (year - 1..=year + 1).map(|y| days_from_civil(y, 1, 1)).filter(|j| *j <= thursday).max().unwrap_or(0);
    ((thursday - jan1) / 7 + 1) as u32
}

/// `titleFor` (`Calendar.tsx`): the month for the month and for a week inside one month, "Nov – Dec 2026" across two, the long day for a day.
fn title_of(s: &App, days: &[i64]) -> String {
    let anchor = if s.pages.cal_day == 0 { clock::day_start(s.now_ms) } else { s.pages.cal_day };
    match s.pages.cal_mode {
        CalMode::Month => clock::month_label(anchor),
        CalMode::Day => {
            let l = clock::local(anchor + 12 * 3_600_000);
            format!("{}, {} {}, {}", WEEKDAYS_LONG[l.weekday as usize], clock::MONTHS_LONG[l.month as usize - 1], l.day, l.year)
        }
        CalMode::Week => {
            let (Some(first), Some(last)) = (days.first(), days.last()) else { return clock::month_label(anchor) };
            let (f, l) = (clock::local(first + 12 * 3_600_000), clock::local(last + 12 * 3_600_000));
            if f.month == l.month {
                clock::month_label(*first)
            } else if f.year == l.year {
                format!("{} – {} {}", &clock::MONTHS_LONG[f.month as usize - 1][..3], &clock::MONTHS_LONG[l.month as usize - 1][..3], l.year)
            } else {
                format!("{} – {}", clock::month_label(*first), clock::month_label(*last))
            }
        }
    }
}

const WEEKDAYS_LONG: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];

/// The page's actions (`CalendarPage`): Today, the two arrows, and the Day / Week / Month switch.
fn header(s: &App, t: &Neko, days: &[i64]) -> Vec<El> {
    let today = clock::day_start(s.now_ms);
    let anchor = if s.pages.cal_day == 0 { today } else { s.pages.cal_day };
    // `showsToday`: the shown days hold today, and a month holds it only when it is today's month.
    let shows_today = days.contains(&today) && (s.pages.cal_mode != CalMode::Month || clock::local(anchor + 12 * 3_600_000).month == clock::local(today + 12 * 3_600_000).month);
    let kind = match s.pages.cal_mode {
        CalMode::Day => "day",
        CalMode::Week => "week",
        CalMode::Month => "month",
    };
    let now_ms = s.now_ms;
    let sync = s.pages.calendar.as_ref().filter(|d| d.status.access == CalendarAccess::Granted).map(|d| format!("From {}, synced {}.", if d.status.source.is_empty() { "macOS Calendar" } else { d.status.source.as_str() }, webkit::relative_time(now_ms, d.status.last_sync_ms.unwrap_or(0))));
    let today_button = webkit::ghost_button(t, "cal-today", "Today", None, webkit::gated(!shows_today, |s: &mut App| s.cal_step(0)));
    let today_button = if shows_today { today_button.opacity(0.64) } else { today_button };
    let today_button = match sync {
        Some(words) => today_button.help(words),
        None => today_button,
    };
    let arrow = |key: &str, label: String, glyph: &'static rune::Icon, by: i64| {
        row().key(key.to_owned()).role("button").label(label.clone()).help(label).focusable(true).size(36.0, 36.0).center().radius(18.0).hover(t.text.base.alpha(0.07)).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.cal_step(by)).child(icon(glyph, 16.0).color(t.text.base).a11y_hidden())
    };
    let arrows = row().gap(0.0).center_y().child(arrow("cal-prev", format!("Previous {kind}"), &icons::CHEVRON_LEFT, -1)).child(arrow("cal-next", format!("Next {kind}"), &icons::CHEVRON_RIGHT, 1));
    vec![row().gap(6.0).center_y().flex_wrap().child(today_button).child(arrows).child(mode_switch(s, t))]
}

/// `SegmentedControl` (`ui/segmented-control.tsx`): a 40 high well (radius 24, 3 inside, 2 between) of 34 high items with 14 at the
/// sides in 14 point medium words; the open one is a pebble.
fn mode_switch(s: &App, t: &Neko) -> El {
    let mut track = webkit::well(t, row().key("cal-mode").role("radiogroup").label("Calendar view").h(40.0).padding(3.0).gap(2.0).center_y().radius(24.0));
    for (key, label, mode) in [("cal-mode-day", "Day", CalMode::Day), ("cal-mode-week", "Week", CalMode::Week), ("cal-mode-month", "Month", CalMode::Month)] {
        let open = s.pages.cal_mode == mode;
        let item = row().key(key).role("radio").label(label).checked(open).focusable(true).h(34.0).px(14.0).center().radius(21.0).hover(if open { rune::Color::TRANSPARENT } else { t.surface.hover }).focus_ring(2.0, 2.0, t.kit.c.border_focus).on_press(move |s: &mut App| s.cal_mode(mode));
        let item = if open { webkit::pebble(t, item) } else { item };
        track = track.child(item.child(text(label).font(webkit::font(t, 14.0, 500.0, 20.0)).color(if open { t.mat.pebble_fg } else { t.text.muted }).a11y_hidden()));
    }
    track
}

pub fn view(s: &App, t: &Neko, layer: &mut Vec<El>) -> El {
    let days = days_shown(s);
    let data = s.pages.calendar.as_ref();
    let events: Vec<&CalendarEvent> = data.map(|d| d.events.iter().collect()).unwrap_or_default();
    let neko: Vec<NekoCalendarItem> = data.map(|d| s.scoped_neko_rows(d).into_iter().cloned().collect()).unwrap_or_default();
    // The web's column is 1400 wide with a 12 point gap between its notes and the grid.
    let mut body = col().w_full().gap(12.0);
    if let Some(d) = data {
        if let Some(n) = access_note(s, t, d) {
            body = body.child(n);
        }
        if let Some(n) = sync_line(s, t, d, events.is_empty()) {
            body = body.child(n);
        }
    }
    if let Some(n) = s.pages.notes.get("page:calendar").filter(|_| data.is_some()) {
        body = body.child(quiet_line(t, "cal-note", n));
    }
    if let Some(chosen) = s.pages.cal_event.as_ref().and_then(|id| events.iter().find(|e| &e.id == id)) {
        mount_event_popover(t, chosen, layer);
    }
    if data.is_some() {
        body = body.child(if s.pages.cal_mode == CalMode::Month { month_grid(s, t, &days, &events, &neko) } else { grid(s, t, &days, &events, &neko) });
    } else if let Some(n) = s.pages.notes.get("page:calendar") {
        body = body.child(quiet_line(t, "cal-loading", n));
    } else {
        // The web's loading block: 384 high, the foreground at 3%.
        body = body.child(el().key("cal-loading").role("progressbar").label("Loading the calendar").w_full().h(384.0).radius(20.0).fill(t.text.base.alpha(0.03)));
    }
    let count = (s.pages.cal_mode != CalMode::Month).then(|| {
        let first = days.first().copied().unwrap_or(s.now_ms);
        let l = clock::local(first + 12 * 3_600_000);
        format!("Week {}", iso_week(l.year, l.month, l.day))
    });
    page_wide(t, "calendar", &title_of(s, &days), count, header(s, t, &days), body, 1400.0)
}

/// `AccessNote`: what to do when Calendar is off or cannot be read, in the person's words.
fn access_note(s: &App, t: &Neko, d: &crate::link::CalendarData) -> Option<El> {
    let small = webkit::font(t, 13.0, 400.0, 19.5);
    let calendar_icon = || icon(&icons::CALENDAR, 14.0).color(t.text.muted).a11y_hidden();
    if !d.status.enabled {
        let words = row().child(text("Calendar is off.".to_owned()).font(small).color(t.text.base).a11y_hidden()).child(text(" Turn it on to see your meetings beside Neko’s work.".to_owned()).font(small).color(t.text.muted).a11y_hidden());
        return Some(row().key("cal-access").w_full().gap(8.0).center_y().flex_wrap().child(calendar_icon()).child(words).child(webkit::button(t, "cal-turn-on", "Turn on", None, false, false, |s: &mut App| s.call(crate::link::Call::SetCalendarOn { enabled: true }))));
    }
    let _ = s;
    match d.status.access {
        CalendarAccess::Denied => Some(
            col()
                .key("cal-access")
                .w_full()
                .gap(6.0)
                .padding(12.0)
                .radius(20.0)
                .fill(t.text.base.alpha(0.03))
                .child(text("Neko can’t read your calendars".to_owned()).font(webkit::font(t, 13.0, 500.0, 19.5)).color(t.text.base))
                .child(text("Allow Calendar access in System Settings → Privacy & Security → Calendars: turn on Neko, then reopen Neko. Neko’s own work still shows below.".to_owned()).font(small).color(t.text.muted).wrap().min_w(0.0))
                .child(row().child(webkit::button(t, "cal-open-settings", "Open Privacy & Security", None, false, false, |s: &mut App| s.open_view(FullView::Settings)))),
        ),
        CalendarAccess::NotAsked if d.status.last_sync_ms.is_none() => Some(row().key("cal-access").w_full().gap(8.0).center_y().child(calendar_icon()).child(row().child(text("Your meetings are not here yet.".to_owned()).font(small).color(t.text.base).a11y_hidden()).child(text(" Open the Neko app and choose Allow when macOS asks about Calendar.".to_owned()).font(small).color(t.text.muted).a11y_hidden()))),
        _ => None,
    }
}

/// `SyncLine`: shown only when it matters (an empty span, a stale sync, a cut-off list); the plain line is the Today button's tooltip.
fn sync_line(s: &App, t: &Neko, d: &crate::link::CalendarData, empty: bool) -> Option<El> {
    if d.status.access != CalendarAccess::Granted {
        return None;
    }
    let stale = d.status.last_sync_ms.is_some_and(|at| s.now_ms - at > 20 * 60_000);
    if !(empty || stale || d.status.truncated) {
        return None;
    }
    let span = match s.pages.cal_mode {
        CalMode::Day => "today",
        CalMode::Week => "this week",
        CalMode::Month => "this month",
    };
    let style = webkit::font(t, 12.5, 400.0, 18.75);
    let node = |words: String| text(words).font(style).color(t.text.muted).a11y_hidden();
    let source = if d.status.source.is_empty() { "macOS Calendar" } else { d.status.source.as_str() };
    let mut line = row().key("cal-sync").role("status").w_full().flex_wrap();
    if empty {
        line = line.child(node(format!("No meetings {span}. ")));
    }
    line = line.child(node("From ".to_owned())).child(node(source.to_owned())).child(node(", synced ".to_owned())).child(node(webkit::relative_time(s.now_ms, d.status.last_sync_ms.unwrap_or(0)))).child(node(if stale { ". Neko may not be running; events update while the app is open.".to_owned() } else { ".".to_owned() }));
    if d.status.truncated {
        line = line.child(node(" Showing the first 2000 events.".to_owned()));
    }
    Some(line)
}

/// The hosts the web calls a meeting (`MEETING_HOSTS` in `pages/Calendar.tsx`).
const MEETING_HOSTS: [&str; 12] = ["meet.google.com", "zoom.us", "zoom.com", "teams.microsoft.com", "teams.live.com", "webex.com", "whereby.com", "meet.jit.si", "chime.aws", "facetime.apple.com", "gotomeeting.com", "around.co"];

/// Whether an address is a meeting: its host is one of the meeting hosts or a subdomain of one (the web's `isMeeting`).
pub fn is_meeting(url: &str) -> bool {
    let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")).unwrap_or("");
    let host = rest.split(['/', '?', '#']).next().unwrap_or("").rsplit('@').next().unwrap_or("").split(':').next().unwrap_or("").to_lowercase();
    MEETING_HOSTS.iter().any(|h| host == *h || host.ends_with(&format!(".{h}")))
}

/// The event's address when it is an http(s) one (the web's `webLink`).
pub fn web_link(url: Option<&str>) -> Option<String> {
    url.filter(|u| { let l = u.to_ascii_lowercase(); l.starts_with("https://") || l.starts_with("http://") }).map(str::to_owned)
}

/// What an event says of when it is: "Mon 16 Nov · 14:00 – 15:00", or "... · All day".
pub fn when_words(e: &CalendarEvent) -> String {
    if e.all_day { format!("{} · All day", clock::day_label(e.start_ms)) } else { format!("{} · {} – {}", clock::day_label(e.start_ms), clock::hhmm(e.start_ms), clock::hhmm(e.end_ms)) }
}

/// An event's popover (the web's `EventDetails`): its title, when, where and in which calendar, and "Join meeting" (the primary
/// button) when its address is a meeting (Meet, Zoom, Teams and the web's other hosts), else "Open link" when it has one.
fn mount_event_popover(t: &Neko, e: &CalendarEvent, layer: &mut Vec<El>) {
    let start = e.start_ms;
    let link = web_link(e.url.as_deref());
    let mut c = col().key("cal-selected").role("dialog").label(if e.title.is_empty() { "Event".to_owned() } else { e.title.clone() }).w(300.0).padding(14.0).gap(10.0);
    let mut head = col().gap(4.0).child(text(if e.title.is_empty() { "Untitled event".to_owned() } else { e.title.clone() }).key("cal-event-title").font(rune::TextStyle { weight: 600.0, ..t.ty.row }).color(t.text.base).wrap());
    head = head.child(row().gap(6.0).center_y().child(icon(&icons::CLOCK, 14.0).color(t.text.muted).a11y_hidden()).child(text(when_words(e)).key("cal-event-when").font(t.ty.meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0)));
    if let Some(loc) = e.location.as_ref().filter(|l| !l.is_empty()) {
        head = head.child(row().gap(6.0).items_start().child(icon(&icons::FOLDER, 14.0).color(t.text.muted).a11y_hidden()).child(text(loc.clone()).key("cal-event-where").font(t.ty.meta).color(t.text.muted).wrap().grow(1.0).min_w(0.0)));
    }
    if !e.calendar.is_empty() {
        head = head.child(text(e.calendar.clone()).font(t.ty.meta).color(t.text.muted));
    }
    c = c.child(head);
    let mut buttons = row().gap(8.0).center_y().wrap();
    if let Some(link) = link {
        let meeting = is_meeting(&link);
        let label = if meeting { "Join meeting" } else { "Open link" };
        let go = link.clone();
        buttons = buttons.child(if meeting {
            super::primary_button(label, "cal-event-link", t, move |s: &mut App| s.cal_open_link(&go))
        } else {
            quiet_button(label, "cal-event-link", t, move |s: &mut App| s.cal_open_link(&go))
        });
    }
    buttons = buttons.child(quiet_button("Open in Calendar", "cal-open-in-calendar", t, move |s: &mut App| s.open_in_calendar(start)));
    c = c.child(buttons);
    let surface = rune_kit::popover::raised(&t.kit, c, t.kit.radius.md);
    layer.push(rune_kit::scrim("scrim-event-popover", |s: &mut App| s.pages.cal_event = None));
    layer.push(rune_kit::popover::anchored(&t.kit, surface, &format!("event-{}", e.id), rune_kit::Placement::BottomStart, 6.0));
}

/// A time as the web's `time` prints it: hours unpadded, "4:00", "19:15".
fn time_text(ms: i64) -> String {
    let l = clock::local(ms);
    format!("{}:{:02}", l.hour, l.minute)
}

/// The web's `border-border/50`: the foreground's 3.5% in dark, 4% in light.
fn hairline(t: &Neko) -> rune::Color {
    if t.dark { rune::Color::WHITE.alpha(0.0353) } else { rune::Color::BLACK.alpha(0.04) }
}

/// The calendar's colour, or mint when it has none (`tint`).
fn tint(t: &Neko, e: &CalendarEvent) -> rune::Color {
    if e.color.len() == 7 { rune::color::hex(&e.color).unwrap_or(t.meaning.mint) } else { t.meaning.mint }
}

/// The month (`MonthGrid`): a weekday line, then six weeks of a 40 point week label and seven cells, hairlines between and under;
/// each cell has the day's 28 point number (today in Iris, other months muted; pressing it opens the day), up to three chips
/// (all-day and timed events, Neko's own work) and "N more".
fn month_grid(s: &App, t: &Neko, days: &[i64], events: &[&CalendarEvent], neko: &[NekoCalendarItem]) -> El {
    const SHOWN: usize = 3;
    let today = clock::day_start(s.now_ms);
    let anchor = if s.pages.cal_day == 0 { today } else { s.pages.cal_day };
    let month = clock::local(clock::month_start(anchor) + 12 * 3_600_000).month;
    let line = hairline(t);
    let head_style = webkit::font(t, 12.0, 400.0, 18.0);
    let mut heads = row().w_full().child(el().w(40.0));
    for name in clock::WEEKDAYS {
        heads = heads.child(row().grow(1.0).w(0.0).child(el().w(1.0).fill(line)).child(row().grow(1.0).min_w(0.0).px(8.0).py(6.0).child(text(name).font(head_style).color(t.text.muted))));
    }
    let mut c = col().key("cal-month").w_full().child(heads).child(el().w_full().h(1.0).fill(line));
    let weeks: Vec<&[i64]> = days.chunks(7).collect();
    for (w, week) in weeks.iter().enumerate() {
        let first = clock::local(week[0] + 12 * 3_600_000);
        let label = row().w(40.0).justify_center().items_start().pt(8.0).child(text("W".to_owned()).font(webkit::tnum(webkit::font(t, 11.0, 400.0, 16.5))).color(t.text.muted).a11y_hidden()).child(text(iso_week(first.year, first.month, first.day).to_string()).font(webkit::tnum(webkit::font(t, 11.0, 400.0, 16.5))).color(t.text.muted).a11y_hidden());
        let mut cells = row().w_full().min_h(112.0).child(label);
        for &d in week.iter() {
            let next = clock::add_days(d, 1);
            let l = clock::local(d + 12 * 3_600_000);
            let mut chips: Vec<El> = Vec::new();
            for e in events.iter().filter(|e| e.all_day && e.start_ms < next && e.end_ms.max(e.start_ms + 1) > d) {
                chips.push(all_day_chip(t, e));
            }
            for e in events.iter().filter(|e| !e.all_day && e.start_ms < next && e.end_ms.max(e.start_ms + 1) > d) {
                chips.push(timed_chip(t, e, d));
            }
            for item in day_items(neko, d, next) {
                chips.push(neko_chip(t, item));
            }
            let hidden = chips.len().saturating_sub(SHOWN);
            let ink = if d == today { rune::Color::WHITE } else if l.month != month { t.text.muted.alpha(0.6) } else { t.text.base };
            let number = row()
                .key(format!("cal-day-{d}"))
                .role("button")
                .label(format!("{}{}. Show this day.", clock::day_label(d), if d == today { ", today" } else { "" }))
                .focusable(true)
                .size(28.0, 28.0)
                .center()
                .radius(14.0)
                .fill(if d == today { t.meaning.iris } else { rune::Color::TRANSPARENT })
                .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                .on_press(move |s: &mut App| s.cal_pick_day(d))
                .child(text(l.day.to_string()).font(webkit::tnum(webkit::font(t, 13.0, if d == today { 600.0 } else { 400.0 }, 19.5))).color(ink).a11y_hidden());
            let mut inner = col().key(format!("cal-cell-{d}")).grow(1.0).min_w(0.0).padding(4.0).gap(2.0).child(row().px(2.0).child(number));
            for chip in chips.into_iter().take(SHOWN) {
                inner = inner.child(chip);
            }
            if hidden > 0 {
                inner = inner.child(
                    row()
                        .key(format!("cal-more-{d}"))
                        .role("button")
                        .label(format!("{hidden} more on {}", clock::day_label(d)))
                        .focusable(true)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
                        .on_press(move |s: &mut App| s.cal_pick_day(d))
                        .child(text(hidden.to_string()).font(webkit::font(t, 11.0, 400.0, 16.5)).color(t.text.muted).a11y_hidden())
                        .child(text(" more".to_owned()).font(webkit::font(t, 11.0, 400.0, 16.5)).color(t.text.muted).a11y_hidden()),
                );
            }
            cells = cells.child(row().grow(1.0).w(0.0).child(el().w(1.0).fill(line)).child(inner));
        }
        c = c.child(cells);
        if w + 1 < weeks.len() {
            c = c.child(el().w_full().h(1.0).fill(line));
        }
    }
    c
}

/// An all-day event in a cell (`AllDayEvent`): a card tinted with its calendar's colour and a 3 point stripe of it at the start.
fn all_day_chip(t: &Neko, e: &CalendarEvent) -> El {
    let id = e.id.clone();
    let color = tint(t, e);
    let title = if e.title.is_empty() { "Untitled event".to_owned() } else { e.title.clone() };
    let card = if t.dark { rune::color::hex("#1d1d1d").unwrap_or(t.surface.card) } else { t.surface.card };
    row()
        .key(format!("event-{}", e.id))
        .role("button")
        .label(format!("{title}, all day"))
        .help(e.title.clone())
        .focusable(true)
        .w_full()
        .radius(10.0)
        .clip_rounded(10.0)
        .fill(card.mix(color, 0.14))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.pages.cal_event = Some(id.clone()))
        .child(el().w(3.0).fill(color))
        .child(row().grow(1.0).min_w(0.0).px(6.0).py(2.0).child(text(title).font(webkit::font(t, 12.0, 400.0, 18.0)).color(t.text.base).truncate().min_w(0.0).a11y_hidden()))
}

/// A timed event in a cell: its calendar's dot, its time and its title.
fn timed_chip(t: &Neko, e: &CalendarEvent, day: i64) -> El {
    let id = e.id.clone();
    let style = webkit::font(t, 12.0, 400.0, 18.0);
    let title = if e.title.is_empty() { "Untitled event".to_owned() } else { e.title.clone() };
    row()
        .key(format!("event-{}", e.id))
        .role("button")
        .label(format!("{title}, {} to {}", time_text(e.start_ms), time_text(e.end_ms)))
        .help(e.title.clone())
        .focusable(true)
        .w_full()
        .px(6.0)
        .py(2.0)
        .gap(6.0)
        .center_y()
        .radius(10.0)
        .hover(t.text.base.alpha(0.05))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.pages.cal_event = Some(id.clone()))
        .child(el().size(8.0, 8.0).radius(4.0).fill(tint(t, e)).a11y_hidden())
        .child(text(time_text(e.start_ms.max(day))).font(webkit::tnum(style)).color(t.text.muted).a11y_hidden())
        .child(text(title).font(style).color(t.text.base).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden())
}

/// One of Neko's dated things in a cell (`NekoChip compact`): the status glyph or a clock, its time and its title, in Iris.
fn neko_chip(t: &Neko, item: &NekoCalendarItem) -> El {
    let route = item.route.clone();
    let style = webkit::font(t, 12.0, 400.0, 18.0);
    let lead: El = match item.kind {
        NekoItemKind::Task if !item.status.is_empty() => status_glyph(kind_of(&item.status), t, 12.0, Some(item.status.as_str())),
        _ => icon(&icons::CLOCK, 12.0).color(t.meaning.iris).a11y_hidden(),
    };
    let mut r = row()
        .key(format!("cal-neko-{}", item.id))
        .role("button")
        .label(format!("{}{}{}. Open in Neko.", item.title, if item.detail.is_empty() { String::new() } else { format!(", {}", item.detail) }, if item.all_day { String::new() } else { format!(", {}", time_text(item.at_ms)) }))
        .help(item.detail.clone())
        .focusable(true)
        .w_full()
        .px(6.0)
        .py(2.0)
        .gap(6.0)
        .center_y()
        .radius(10.0)
        .hover(t.meaning.iris.alpha(0.1))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.open_neko_item(&route))
        .child(lead);
    if !item.all_day {
        r = r.child(text(time_text(item.at_ms)).font(webkit::tnum(style)).color(t.meaning.iris.alpha(0.7)).a11y_hidden());
    }
    r.child(text(item.title.clone()).font(style).color(t.meaning.iris).truncate().grow(1.0).shrink().min_w(0.0).a11y_hidden())
}

fn grid(s: &App, t: &Neko, days: &[i64], events: &[&CalendarEvent], neko: &[NekoCalendarItem]) -> El {
    let today = clock::day_start(s.now_ms);
    // Day headings.
    let mut heads = row().w_full().h(26.0).center_y().child(el().w(AXIS));
    for &d in days {
        let l = clock::local(d + 12 * 3_600_000);
        let is_today = d == today;
        let label = if days.len() == 1 { clock::day_label(d) } else { format!("{} {}", clock::WEEKDAYS[l.weekday as usize], l.day) };
        heads = heads.child(row().grow(1.0).min_w(0.0).px(4.0).child(text(label).key(format!("cal-head-{d}")).font(rune::TextStyle { weight: if is_today { 500.0 } else { 400.0 }, ..t.ty.meta }).color(if is_today { t.meaning.iris } else { t.text.muted })));
    }
    // The all-day row: Neko's dated work, and all-day events.
    let mut all_day = row().key("cal-allday").w_full().min_h(26.0).py(2.0).child(text("all-day").font(t.ty.id).color(t.text.faint).w(AXIS).px(2.0));
    for &d in days {
        let next = clock::add_days(d, 1);
        let mut cell = col().grow(1.0).min_w(0.0).px(2.0).gap(2.0);
        let items = day_items(neko, d, next);
        for item in items.iter().take(3) {
            cell = cell.child(neko_item(t, item));
        }
        if items.len() > 3 {
            cell = cell.child(text(format!("+{} more", items.len() - 3)).font(t.ty.id).color(t.text.faint).px(4.0));
        }
        for e in events.iter().filter(|e| e.all_day && e.start_ms < next && e.end_ms > d) {
            cell = cell.child(event_block(t, e, s.pages.cal_event.as_deref() == Some(e.id.as_str())).h(20.0).w_full());
        }
        all_day = all_day.child(cell);
    }
    // The hours.
    let mut axis = col().w(AXIS).h(grid_height());
    for h in FIRST_HOUR..=LAST_HOUR {
        axis = axis.child(text(format!("{h:02}:00")).font(t.ty.id).color(t.text.faint).absolute(2.0, ((h - FIRST_HOUR) as f32 * HOUR_H - 8.0).max(0.0)));
    }
    let mut columns = row().grow(1.0).min_w(0.0).h(grid_height()).clip_rounded(0.0);
    for &d in days {
        let next = clock::add_days(d, 1);
        let timed: Vec<&CalendarEvent> = events.iter().copied().filter(|e| !e.all_day && e.start_ms < next && e.end_ms.max(e.start_ms + 1) > d).collect();
        let mut day = row().grow(1.0).min_w(0.0).h(grid_height()).clip_rounded(0.0);
        for h in FIRST_HOUR..=LAST_HOUR {
            day = day.child(el().w_full().h(1.0).fill(t.surface.border.alpha(0.3)).absolute(0.0, ((h - FIRST_HOUR) as f32 * HOUR_H).min(grid_height() - 1.0)));
        }
        // Events that overlap share a band of the column side by side; the others have it all.
        for cluster in clusters(&timed) {
            let spans: Vec<(f32, f32)> = cluster
                .iter()
                .map(|e| {
                    let top = y_of(e.start_ms.max(d), d);
                    let bottom = if e.end_ms >= next { grid_height() } else { y_of(e.end_ms, d) };
                    (top, (top + (bottom - top).max(18.0)).min(grid_height().max(top + 18.0)))
                })
                .collect();
            let top = spans.iter().map(|s| s.0).fold(f32::MAX, f32::min);
            let bottom = spans.iter().map(|s| s.1).fold(0.0, f32::max);
            let lane = lanes(&cluster);
            let count = lane.iter().map(|(_, n)| *n).max().unwrap_or(1);
            let mut band = row().w_full().h(bottom - top).absolute(0.0, top);
            for n in 0..count {
                let mut lane_col = col().grow(1.0).min_w(0.0).h(bottom - top).px(1.0);
                for ((e, (at, _)), (y0, y1)) in cluster.iter().zip(lane.iter()).zip(spans.iter()) {
                    if *at == n {
                        lane_col = lane_col.child(event_block(t, e, s.pages.cal_event.as_deref() == Some(e.id.as_str())).w_full().h(y1 - y0).absolute(0.0, y0 - top));
                    }
                }
                band = band.child(lane_col);
            }
            day = day.child(band);
        }
        if d == today {
            let y = y_of(s.now_ms, d);
            if s.now_ms >= d && y > 0.0 && y < grid_height() {
                day = day.child(el().key("cal-now").w_full().h(1.5).fill(t.meaning.iris).absolute(0.0, y - 0.75)).child(el().size(7.0, 7.0).radius(3.5).fill(t.meaning.iris).absolute(-3.0, y - 3.5));
            }
        }
        columns = columns.child(day);
    }
    col().key("cal-grid").w_full().child(heads).child(all_day).child(row().w_full().h(grid_height()).child(axis).child(columns))
}

/// One of Neko's dated things on the all-day row: the status glyph (or a clock for a routine's run) and its title.
fn neko_item(t: &Neko, item: &NekoCalendarItem) -> El {
    let (id, route) = (item.id.clone(), item.route.clone());
    let lead: El = match item.kind {
        NekoItemKind::Task => status_glyph(kind_of(&item.status), t, 12.0, Some(if item.status.is_empty() { "Task" } else { item.status.as_str() })),
        _ => icon(&icons::CLOCK, 12.0).color(t.text.muted).a11y_hidden(),
    };
    let title = if item.all_day { item.title.clone() } else { format!("{} {}", clock::hhmm(item.at_ms), item.title) };
    row()
        .key(format!("cal-neko-{id}"))
        .role("button")
        .label(format!("{}{}", item.title, if item.detail.is_empty() { String::new() } else { format!(", {}", item.detail) }))
        .help(item.detail.clone())
        .focusable(true)
        .w_full()
        .h(20.0)
        .px(4.0)
        .gap(5.0)
        .center_y()
        .radius(5.0)
        .hover(t.surface.hover)
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.open_neko_item(&route))
        .child(lead)
        .child(text(title).font(t.ty.id).color(t.text.base).truncate().grow(1.0).min_w(0.0).a11y_hidden())
}

/// An event: a mint block with its title. Pressing it selects it.
fn event_block(t: &Neko, e: &CalendarEvent, selected: bool) -> El {
    let id = e.id.clone();
    let when = if e.all_day { "all day".to_owned() } else { format!("{}–{}", clock::hhmm(e.start_ms), clock::hhmm(e.end_ms)) };
    row()
        .key(format!("event-{}", e.id))
        .role("button")
        .label(format!("{}, {when}", e.title))
        .help(format!("{} · {when}", e.title))
        .focusable(true)
        .px(6.0)
        .py(2.0)
        .radius(6.0)
        .fill(t.meaning.mint.alpha(if selected { 0.42 } else { 0.24 }))
        .hover(t.meaning.mint.alpha(0.14))
        .focus_ring(2.0, 2.0, t.kit.c.border_focus)
        .on_press(move |s: &mut App| s.pages.cal_event = Some(id.clone()))
        .child(text(e.title.clone()).font(t.ty.id).color(t.text.base).truncate().min_w(0.0).a11y_hidden())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(start: i64, end: i64) -> CalendarEvent {
        CalendarEvent { id: format!("{start}"), title: "x".into(), start_ms: start, end_ms: end, ..CalendarEvent::default() }
    }

    #[test]
    fn events_that_overlap_share_the_column_and_others_do_not() {
        let (a, b, c) = (ev(0, 3_600_000), ev(1_800_000, 5_400_000), ev(7_200_000, 8_000_000));
        let l = lanes(&[&a, &b, &c]);
        assert_eq!(l[0], (0, 2));
        assert_eq!(l[1], (1, 2));
        assert_eq!(l[2], (0, 1), "a later event has the whole column");
        assert_eq!(lanes(&[]), Vec::<(usize, usize)>::new());
    }

    #[test]
    fn clusters_hold_only_events_that_touch() {
        let (a, b, c) = (ev(0, 3_600_000), ev(1_800_000, 5_400_000), ev(7_200_000, 8_000_000));
        let groups = clusters(&[&c, &a, &b]);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].len(), 2);
        assert_eq!(groups[1].len(), 1);
    }

    #[test]
    fn the_grid_is_fifteen_hours_of_forty_eight_points() {
        assert_eq!(grid_height(), 720.0);
        let day = clock::day_start(1_800_000_000_000);
        assert_eq!(y_of(day + 7 * 3_600_000, day), 0.0);
        assert!((y_of(day + 9 * 3_600_000 + 30 * 60_000, day) - 120.0).abs() < 1.0);
        assert_eq!(y_of(day + 23 * 3_600_000, day), grid_height(), "clamped to the last hour");
        assert_eq!(y_of(day + 3_600_000, day), 0.0, "before the first hour");
    }
}
