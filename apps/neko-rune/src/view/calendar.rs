//! The Calendar page: a week (or a day) of the macOS Calendar the daemon holds, read-only. Hours
//! 07:00 to 22:00 at 48 points each; an all-day row on top holds Neko's dated work with its status
//! glyphs (tasks on the day they finished, routines' coming runs); events are mint blocks with their
//! titles and the current time is an Iris line. Pressing an event offers "Open in Calendar".
//! Ported from `pages/Calendar.tsx` and `docs/knowledge/calendar.md`; the same requests, no writes.

use neko_protocol::calendar::{CalendarAccess, CalendarEvent, NekoCalendarItem, NekoItemKind};
use rune::{El, col, el, icon, row, text};
use rune_kit::SegmentedControl;

use super::ui::{page, quiet_line};
use super::{icon_button, quiet_button};
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
    while d < to && days.len() < 7 {
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

fn header(s: &App, t: &Neko, days: &[i64]) -> Vec<El> {
    let range = match (days.first(), days.last()) {
        (Some(a), Some(b)) if a != b => format!("{} – {}", clock::day_label(*a), clock::day_label(*b)),
        (Some(a), _) => clock::day_label(*a),
        _ => String::new(),
    };
    let selected = if s.pages.cal_mode == CalMode::Week { 1 } else { 0 };
    vec![
        text(range).key("cal-range").font(t.ty.meta).color(t.text.muted),
        icon_button(&icons::CHEVRON_LEFT, "Earlier", "cal-prev", t, |s: &mut App| s.cal_step(-1)),
        quiet_button("Today", "cal-today", t, |s: &mut App| s.cal_step(0)),
        icon_button(&icons::CHEVRON_RIGHT, "Later", "cal-next", t, |s: &mut App| s.cal_step(1)),
        SegmentedControl::new(&["Day", "Week"], Some(selected))
            .label("View")
            .width(Some(120.0))
            .on_pick(|s: &mut App, i, _| s.cal_mode(if i == 0 { CalMode::Day } else { CalMode::Week }))
            .build(&t.kit)
            .key("cal-mode"),
    ]
}

pub fn view(s: &App, t: &Neko) -> El {
    let days = days_shown(s);
    let data = s.pages.calendar.as_ref();
    let events: Vec<&CalendarEvent> = data.map(|d| d.events.iter().collect()).unwrap_or_default();
    let neko: Vec<NekoCalendarItem> = data.map(|d| d.neko.clone()).unwrap_or_default();
    let count = events.len() + neko.len();
    let mut body = col().w_full().gap(10.0);
    // Why there are no events, in one line, with the way to fix it.
    if let Some(d) = data {
        let line = if !d.status.enabled {
            Some("Calendar is off. Turn it on in Settings.")
        } else {
            match d.status.access {
                CalendarAccess::Denied => Some("Neko can’t read your calendars. Allow it in System Settings, Privacy, Calendars."),
                CalendarAccess::NotAsked => Some("Waiting for the Neko app to read your calendar."),
                CalendarAccess::Granted => None,
            }
        };
        if let Some(line) = line {
            body = body.child(row().w_full().gap(10.0).center_y().child(quiet_line(t, "cal-access", line)).child(quiet_button("Open Settings", "cal-open-settings", t, |s: &mut App| s.open_view(FullView::Settings))));
        }
    } else {
        body = body.child(quiet_line(t, "cal-loading", s.pages.notes.get("page:calendar").map_or("Reading your calendar…", String::as_str)));
    }
    if let Some(n) = s.pages.notes.get("page:calendar").filter(|_| data.is_some()) {
        body = body.child(quiet_line(t, "cal-note", n));
    }
    if let Some(chosen) = s.pages.cal_event.as_ref().and_then(|id| events.iter().find(|e| &e.id == id)) {
        body = body.child(selected_bar(t, chosen));
    }
    body = body.child(grid(s, t, &days, &events, &neko));
    page(t, "calendar", "Calendar", (count > 0).then_some(count), header(s, t, &days), body)
}

fn selected_bar(t: &Neko, e: &CalendarEvent) -> El {
    let start = e.start_ms;
    let when = if e.all_day { "All day".to_owned() } else { format!("{}–{}", clock::hhmm(e.start_ms), clock::hhmm(e.end_ms)) };
    let mut words = format!("{} · {when}", e.title);
    if let Some(loc) = e.location.as_ref().filter(|l| !l.is_empty()) {
        words.push_str(&format!(" · {loc}"));
    }
    if !e.calendar.is_empty() {
        words.push_str(&format!(" · {}", e.calendar));
    }
    row()
        .key("cal-selected")
        .w_full()
        .min_h(36.0)
        .px(10.0)
        .gap(10.0)
        .center_y()
        .radius(t.radius.row)
        .fill(t.surface.hover)
        .child(text(words).font(t.ty.row).color(t.text.base).truncate().grow(1.0).min_w(0.0))
        .child(quiet_button("Open in Calendar", "cal-open-in-calendar", t, move |s: &mut App| s.open_in_calendar(start)))
        .child(icon_button(&icons::CLOSE_SMALL, "Close", "cal-close", t, |s: &mut App| s.pages.cal_event = None))
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
    let mut columns = row().grow(1.0).min_w(0.0).h(grid_height());
    for &d in days {
        let next = clock::add_days(d, 1);
        let timed: Vec<&CalendarEvent> = events.iter().copied().filter(|e| !e.all_day && e.start_ms < next && e.end_ms.max(e.start_ms + 1) > d).collect();
        let mut day = row().grow(1.0).min_w(0.0).h(grid_height());
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
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
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
        .focus_ring(2.0, 0.0, t.kit.c.border_focus)
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
