//! When a round is due (docs/knowledge/rounds.md): the schedule kinds and the arithmetic that says when the next time is. It lives
//! here, not in the daemon, so the app can tell the person when the first run will be before they save, with the very same code.
//!
//! Nothing here reads the clock. The time and the time zone are handed in ([`Zone`]), so the tests drive both.
//!
//! - **Daily at HH:MM** and **chosen weekdays at HH:MM** are wall-clock times in the Mac's time zone.
//! - **Every N hours** (1 to 24) counts real elapsed time from an anchor (when the round was made, turned on or its schedule
//!   changed): the first run is N hours after the anchor, then every N hours. Daylight saving never moves it.
//! - **Manual** is never due: the round runs only when the person presses Run now.
//!
//! Across a daylight-saving change a wall-clock time behaves as Calendar does:
//! - a time that does not exist that day (02:30 when the clocks jump from 02:00 to 03:00) happens once, shifted forward by the
//!   length of the gap (03:30 by the new clock, the same instant 02:30 would have been had the clocks not moved);
//! - a time that happens twice (01:30 when the clocks go back from 02:00 to 01:00) happens once, at its first occurrence.
//!
//! So a daily round runs exactly once on every date it is due, however the clocks move.

use serde::{Deserialize, Serialize};

/// A day of the week, Monday first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    pub const ALL: [Weekday; 7] = [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat, Weekday::Sun];

    /// Monday is 0.
    pub fn index(self) -> usize {
        self as usize
    }

    pub fn from_index(i: usize) -> Weekday {
        Weekday::ALL[i % 7]
    }

    /// "Monday".
    pub fn name(self) -> &'static str {
        ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"][self.index()]
    }

    /// "Mon".
    pub fn short(self) -> &'static str {
        ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"][self.index()]
    }
}

/// How often a round starts by itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Every day at `at` ("HH:MM", 24-hour, local time).
    Daily { at: String },
    /// On these days (Monday first, no repeats) at `at`.
    Weekdays { days: Vec<Weekday>, at: String },
    /// Every `hours` hours (1 to 24) of real time, counted from when the round was made, turned on or changed.
    Every { hours: u32 },
    /// Never by itself: only when the person presses Run now.
    Manual,
}

/// The longest gap "every N hours" may have.
pub const MAX_EVERY_HOURS: u32 = 24;

/// "09:05" as minutes after midnight. One or two digits of hour, two of minute.
pub fn parse_at(at: &str) -> Option<u32> {
    let (h, m) = at.trim().split_once(':')?;
    if h.is_empty() || h.len() > 2 || m.len() != 2 || !h.bytes().chain(m.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

/// Minutes after midnight as "HH:MM".
pub fn format_at(minutes: u32) -> String {
    format!("{:02}:{:02}", minutes / 60 % 24, minutes % 60)
}

impl Schedule {
    /// The schedule as it is kept: the time as "HH:MM", the days in week order without repeats. Refused, with the words why, when
    /// it makes no sense (a time that is not a time, no day chosen, hours outside 1 to 24).
    pub fn normalized(&self) -> Result<Schedule, String> {
        let at = |s: &str| parse_at(s).map(format_at).ok_or_else(|| format!("\"{s}\" is not a time of day. Use hours and minutes, like 09:00."));
        match self {
            Schedule::Daily { at: a } => Ok(Schedule::Daily { at: at(a)? }),
            Schedule::Weekdays { days, at: a } => {
                let mut days = days.clone();
                days.sort();
                days.dedup();
                if days.is_empty() {
                    return Err("Choose at least one day of the week.".into());
                }
                Ok(Schedule::Weekdays { days, at: at(a)? })
            }
            Schedule::Every { hours } if (1..=MAX_EVERY_HOURS).contains(hours) => Ok(self.clone()),
            Schedule::Every { .. } => Err(format!("Every N hours takes a number from 1 to {MAX_EVERY_HOURS}.")),
            Schedule::Manual => Ok(Schedule::Manual),
        }
    }

    /// Whether the round starts by itself at all.
    pub fn is_automatic(&self) -> bool {
        !matches!(self, Schedule::Manual)
    }

    /// The first time after `after_ms` the round is due (milliseconds since the epoch), or None for a schedule that is never due
    /// (manual, or one that does not validate). `anchor_ms` is where "every N hours" counts from. Strictly after: a time that is
    /// `after_ms` itself has been handled.
    pub fn next_due(&self, anchor_ms: i64, after_ms: i64, zone: &dyn Zone) -> Option<i64> {
        match self {
            Schedule::Manual => None,
            Schedule::Every { hours } => {
                let step = i64::from(*hours).clamp(1, i64::from(MAX_EVERY_HOURS)) * 3_600_000;
                // Due at anchor + k * step for k = 1, 2, …
                let k = if after_ms < anchor_ms { 1 } else { (after_ms - anchor_ms) / step + 1 };
                Some(anchor_ms + k * step)
            }
            Schedule::Daily { at } => next_wall(parse_at(at)?, None, after_ms, zone),
            Schedule::Weekdays { days, at } => (!days.is_empty()).then(|| next_wall(parse_at(at)?, Some(days), after_ms, zone)).flatten(),
        }
    }

    /// Every due time in `(after_ms, until_ms]`, oldest first, at most `limit` of them.
    pub fn due_between(&self, anchor_ms: i64, after_ms: i64, until_ms: i64, zone: &dyn Zone, limit: usize) -> Vec<i64> {
        let mut out = Vec::new();
        let mut from = after_ms;
        while out.len() < limit {
            match self.next_due(anchor_ms, from, zone) {
                Some(t) if t <= until_ms => {
                    out.push(t);
                    from = t;
                }
                _ => break,
            }
        }
        out
    }
}

/// The next wall-clock `minutes` (on one of `days`, or every day) strictly after `after_ms`.
fn next_wall(minutes: u32, days: Option<&[Weekday]>, after_ms: i64, zone: &dyn Zone) -> Option<i64> {
    let after_s = after_ms.div_euclid(1000);
    let local = after_s + i64::from(zone.offset_secs(after_s));
    let today = local.div_euclid(86_400);
    // Yesterday (a time in the small hours after a clock change can still lie ahead), then up to a week and a day more.
    for day in today - 1..today + 9 {
        if let Some(days) = days
            && !days.contains(&weekday_of(day))
        {
            continue;
        }
        let due = local_to_utc(zone, day * 86_400 + i64::from(minutes) * 60) * 1000;
        if due > after_ms {
            return Some(due);
        }
    }
    None
}

/// The instant a local wall-clock reading happens (`local_secs`: the reading as if it were UTC), by the rules in the module
/// documentation: the first of two, and a time that was skipped is shifted forward by the length of the gap.
pub fn local_to_utc(zone: &dyn Zone, local_secs: i64) -> i64 {
    let before = zone.offset_secs(local_secs - 86_400);
    let after = zone.offset_secs(local_secs + 86_400);
    let fits = |off: i32| {
        let u = local_secs - i64::from(off);
        (zone.offset_secs(u) == off).then_some(u)
    };
    match (fits(before), fits(after)) {
        (Some(a), Some(b)) => a.min(b),
        (Some(u), None) | (None, Some(u)) => u,
        // In the gap: counted with the offset before it, so 02:30 lands on what the clocks then call 03:30.
        (None, None) => local_secs - i64::from(before),
    }
}

/// Days since 1970-01-01 to the weekday (Monday first).
pub fn weekday_of(days: i64) -> Weekday {
    // 1970-01-01 was a Thursday.
    Weekday::from_index((days + 3).rem_euclid(7) as usize)
}

/// Days since 1970-01-01 of a civil date (proleptic Gregorian). Howard Hinnant's algorithm, written out again.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((m + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The civil date of a day count: (year, month 1-12, day 1-31).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

/// A moment as the wall clock of a zone reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub year: i64,
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    pub weekday: Weekday,
    /// Days since 1970-01-01 of this local date.
    pub day_number: i64,
}

/// What `zone`'s wall clock reads at `ms`.
pub fn local_parts(zone: &dyn Zone, ms: i64) -> Local {
    let s = ms.div_euclid(1000);
    let local = s + i64::from(zone.offset_secs(s));
    let day_number = local.div_euclid(86_400);
    let (year, month, day) = civil_from_days(day_number);
    let secs = local.rem_euclid(86_400);
    Local { year, month, day, hour: (secs / 3600) as u32, minute: (secs % 3600 / 60) as u32, weekday: weekday_of(day_number), day_number }
}

/// A time zone: how far its clock is from UTC at an instant.
pub trait Zone {
    /// Seconds to add to UTC to get this zone's wall clock at `utc_secs`.
    fn offset_secs(&self, utc_secs: i64) -> i32;
}

/// A zone that never changes: UTC, or a fixed offset.
pub struct Fixed(pub i32);

impl Zone for Fixed {
    fn offset_secs(&self, _: i64) -> i32 {
        self.0
    }
}

/// The Mac's own time zone (`TZ`, else the system's), as libc says it for each instant.
pub struct SystemZone;

impl Zone for SystemZone {
    fn offset_secs(&self, utc_secs: i64) -> i32 {
        unsafe extern "C" {
            // Re-reads TZ. Declared here: the libc crate does not carry it for this target.
            fn tzset();
        }
        // SAFETY: `localtime_r` writes into `tm`, a zeroed struct of ours; `tzset` only reads the environment.
        unsafe {
            tzset();
            let t: libc::time_t = utc_secs as libc::time_t;
            let mut tm: libc::tm = std::mem::zeroed();
            if libc::localtime_r(&t, &mut tm).is_null() { 0 } else { tm.tm_gmtoff as i32 }
        }
    }
}

/// A zone with a standard offset and one daylight-saving rule, for tests and for the dialog's tests: no time zone database is
/// read, so a test says exactly where the clocks move.
pub struct Rules {
    /// Standard time's offset from UTC, in seconds.
    pub standard: i32,
    /// Where the clocks move, as a function of the year: the instants (UTC seconds) daylight time starts and ends.
    pub transitions: fn(year: i64, standard: i32) -> (i64, i64),
}

impl Rules {
    /// The United States' rules (2007 on): forward at 02:00 on the second Sunday of March, back at 02:00 on the first Sunday of
    /// November. `standard` -18000 is New York.
    pub fn us(standard: i32) -> Rules {
        Rules { standard, transitions: us_transitions }
    }

    /// The European Union's rules: forward at 01:00 UTC on the last Sunday of March, back at 01:00 UTC on the last Sunday of
    /// October. `standard` 0 is London.
    pub fn eu(standard: i32) -> Rules {
        Rules { standard, transitions: eu_transitions }
    }
}

impl Zone for Rules {
    fn offset_secs(&self, utc_secs: i64) -> i32 {
        let local = utc_secs + i64::from(self.standard);
        let (year, ..) = civil_from_days(local.div_euclid(86_400));
        let (start, end) = (self.transitions)(year, self.standard);
        if (start..end).contains(&utc_secs) { self.standard + 3600 } else { self.standard }
    }
}

/// The `n`th (1-based) Sunday of a month, as a day count; `n` of 0 means the last.
fn sunday(year: i64, month: u32, n: u32) -> i64 {
    if n == 0 {
        let next = if month == 12 { days_from_civil(year + 1, 1, 1) } else { days_from_civil(year, month + 1, 1) };
        let last = next - 1;
        return last - ((weekday_of(last).index() as i64 + 1) % 7);
    }
    let first = days_from_civil(year, month, 1);
    let to_sunday = (6 - weekday_of(first).index() as i64).rem_euclid(7);
    first + to_sunday + 7 * i64::from(n - 1)
}

fn us_transitions(year: i64, standard: i32) -> (i64, i64) {
    let start = sunday(year, 3, 2) * 86_400 + 2 * 3600 - i64::from(standard);
    let end = sunday(year, 11, 1) * 86_400 + 2 * 3600 - i64::from(standard) - 3600;
    (start, end)
}

fn eu_transitions(year: i64, _: i32) -> (i64, i64) {
    (sunday(year, 3, 0) * 86_400 + 3600, sunday(year, 10, 0) * 86_400 + 3600)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3_600_000;

    fn utc(y: i64, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
        (days_from_civil(y, mo, d) * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60) * 1000
    }

    fn daily(at: &str) -> Schedule {
        Schedule::Daily { at: at.into() }
    }

    #[test]
    fn civil_dates_round_trip_and_the_weekday_is_right() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(days_from_civil(2024, 2, 29)), (2024, 2, 29));
        assert_eq!(civil_from_days(days_from_civil(1900, 3, 1)), (1900, 3, 1));
        assert_eq!(weekday_of(0), Weekday::Thu);
        // 2026-11-27 is a Friday.
        assert_eq!(weekday_of(days_from_civil(2026, 11, 27)), Weekday::Fri);
        for d in -800..800 {
            let (y, m, dd) = civil_from_days(d * 37);
            assert_eq!(days_from_civil(y, m, dd), d * 37);
        }
    }

    #[test]
    fn the_time_of_day_is_read_strictly_and_kept_as_hh_mm() {
        assert_eq!(parse_at("09:00"), Some(540));
        assert_eq!(parse_at("9:05"), Some(545));
        assert_eq!(parse_at("23:59"), Some(1439));
        for bad in ["24:00", "09:60", "0900", "9:5", "", "ab:cd", "-1:00", "123:00"] {
            assert_eq!(parse_at(bad), None, "{bad}");
        }
        assert_eq!(daily("9:05").normalized().unwrap(), daily("09:05"));
        assert!(daily("25:00").normalized().is_err());
    }

    #[test]
    fn weekdays_are_sorted_without_repeats_and_must_not_be_empty() {
        let s = Schedule::Weekdays { days: vec![Weekday::Fri, Weekday::Mon, Weekday::Fri], at: "9:00".into() };
        assert_eq!(s.normalized().unwrap(), Schedule::Weekdays { days: vec![Weekday::Mon, Weekday::Fri], at: "09:00".into() });
        assert!(Schedule::Weekdays { days: vec![], at: "09:00".into() }.normalized().is_err());
        assert!(Schedule::Every { hours: 0 }.normalized().is_err());
        assert!(Schedule::Every { hours: 25 }.normalized().is_err());
        assert!(Schedule::Every { hours: 24 }.normalized().is_ok());
    }

    #[test]
    fn daily_is_the_next_wall_clock_time_strictly_after() {
        let z = Fixed(0);
        let s = daily("09:00");
        assert_eq!(s.next_due(0, utc(2026, 11, 27, 8, 59), &z), Some(utc(2026, 11, 27, 9, 0)));
        assert_eq!(s.next_due(0, utc(2026, 11, 27, 9, 0), &z), Some(utc(2026, 11, 28, 9, 0)), "the time itself has been handled");
        assert_eq!(s.next_due(0, utc(2026, 11, 27, 23, 59), &z), Some(utc(2026, 11, 28, 9, 0)));
    }

    #[test]
    fn a_fixed_offset_moves_the_wall_clock() {
        // UTC+5:30: 09:00 there is 03:30 UTC.
        let z = Fixed(19_800);
        assert_eq!(daily("09:00").next_due(0, utc(2026, 11, 27, 0, 0), &z), Some(utc(2026, 11, 27, 3, 30)));
        // UTC-8: 09:00 there is 17:00 UTC; at 20:00 UTC (12:00 there) the next is tomorrow's.
        let z = Fixed(-8 * 3600);
        assert_eq!(daily("09:00").next_due(0, utc(2026, 11, 27, 20, 0), &z), Some(utc(2026, 11, 28, 17, 0)));
    }

    #[test]
    fn weekdays_skip_the_days_not_chosen_across_month_and_year_ends() {
        let z = Fixed(0);
        let weekdays = Schedule::Weekdays { days: vec![Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri], at: "09:00".into() };
        // Friday 2026-11-27 after 09:00: the next is Monday 2026-11-30 (a month end).
        assert_eq!(weekdays.next_due(0, utc(2026, 11, 27, 10, 0), &z), Some(utc(2026, 11, 30, 9, 0)));
        // Thursday 2026-12-31 after 09:00: Friday 2027-01-01 (a year end).
        assert_eq!(weekdays.next_due(0, utc(2026, 12, 31, 10, 0), &z), Some(utc(2027, 1, 1, 9, 0)));
        // Friday 2027-01-01 after 09:00: the weekend is skipped.
        assert_eq!(weekdays.next_due(0, utc(2027, 1, 1, 10, 0), &z), Some(utc(2027, 1, 4, 9, 0)));
        // Only Sundays: the next Sunday from a Friday.
        let sundays = Schedule::Weekdays { days: vec![Weekday::Sun], at: "18:30".into() };
        assert_eq!(sundays.next_due(0, utc(2026, 11, 27, 10, 0), &z), Some(utc(2026, 11, 29, 18, 30)));
        // A leap day exists in 2028 and not in 2027.
        assert_eq!(daily("12:00").next_due(0, utc(2028, 2, 28, 13, 0), &z), Some(utc(2028, 2, 29, 12, 0)));
        assert_eq!(daily("12:00").next_due(0, utc(2027, 2, 28, 13, 0), &z), Some(utc(2027, 3, 1, 12, 0)));
        assert_eq!(daily("23:59").next_due(0, utc(2026, 12, 31, 23, 59), &z), Some(utc(2027, 1, 1, 23, 59)));
    }

    #[test]
    fn every_n_hours_counts_real_hours_from_its_anchor() {
        let z = Fixed(0);
        let anchor = utc(2026, 11, 27, 10, 17);
        let s = Schedule::Every { hours: 6 };
        assert_eq!(s.next_due(anchor, anchor, &z), Some(anchor + 6 * HOUR), "the first run is N hours after the anchor");
        assert_eq!(s.next_due(anchor, anchor + 6 * HOUR - 1, &z), Some(anchor + 6 * HOUR));
        assert_eq!(s.next_due(anchor, anchor + 6 * HOUR, &z), Some(anchor + 12 * HOUR));
        assert_eq!(s.next_due(anchor, anchor + 100 * HOUR, &z), Some(anchor + 102 * HOUR), "stays on its grid");
        assert_eq!(s.next_due(anchor, anchor - 5 * HOUR, &z), Some(anchor + 6 * HOUR), "a clock set back");
        assert_eq!(Schedule::Every { hours: 24 }.next_due(anchor, anchor, &z), Some(anchor + 24 * HOUR));
        // Daylight saving does not move it: it is elapsed time.
        let ny = Rules::us(-5 * 3600);
        let before = utc(2026, 3, 7, 12, 0);
        assert_eq!(Schedule::Every { hours: 24 }.next_due(before, before, &ny), Some(before + 24 * HOUR));
        assert_eq!(Schedule::Every { hours: 24 }.next_due(before, before + 24 * HOUR, &ny), Some(before + 48 * HOUR));
    }

    #[test]
    fn manual_is_never_due() {
        assert_eq!(Schedule::Manual.next_due(0, 0, &Fixed(0)), None);
        assert!(Schedule::Manual.due_between(0, 0, i64::MAX / 2, &Fixed(0), 10).is_empty());
        assert!(!Schedule::Manual.is_automatic());
    }

    #[test]
    fn the_us_rules_put_the_clocks_where_the_law_does() {
        let ny = Rules::us(-5 * 3600);
        // 2026: forward on Sunday March 8 at 07:00 UTC, back on Sunday November 1 at 06:00 UTC.
        assert_eq!(ny.offset_secs(utc(2026, 3, 8, 6, 59) / 1000), -5 * 3600);
        assert_eq!(ny.offset_secs(utc(2026, 3, 8, 7, 0) / 1000), -4 * 3600);
        assert_eq!(ny.offset_secs(utc(2026, 11, 1, 5, 59) / 1000), -4 * 3600);
        assert_eq!(ny.offset_secs(utc(2026, 11, 1, 6, 0) / 1000), -5 * 3600);
        let london = Rules::eu(0);
        assert_eq!(london.offset_secs(utc(2026, 3, 29, 0, 59) / 1000), 0);
        assert_eq!(london.offset_secs(utc(2026, 3, 29, 1, 0) / 1000), 3600);
        assert_eq!(london.offset_secs(utc(2026, 10, 25, 0, 59) / 1000), 3600);
        assert_eq!(london.offset_secs(utc(2026, 10, 25, 1, 0) / 1000), 0);
    }

    #[test]
    fn spring_forward_a_skipped_time_happens_once_shifted_by_the_gap() {
        let ny = Rules::us(-5 * 3600);
        // Sunday 2026-03-08 02:30 does not exist in New York (02:00 jumps to 03:00). It runs at 03:30 EDT = 07:30 UTC.
        let s = daily("02:30");
        assert_eq!(s.next_due(0, utc(2026, 3, 7, 12, 0), &ny), Some(utc(2026, 3, 8, 7, 30)));
        let due: Vec<i64> = s.due_between(0, utc(2026, 3, 6, 12, 0), utc(2026, 3, 10, 12, 0), &ny, 20);
        assert_eq!(due.len(), 4, "once on every date, the skipped one too: {due:?}");
        assert_eq!(due[0], utc(2026, 3, 7, 7, 30), "Saturday, before the change: 02:30 EST");
        assert_eq!(due[1], utc(2026, 3, 8, 7, 30), "Sunday: 03:30 EDT, the gap's length later");
        assert_eq!(due[2], utc(2026, 3, 9, 6, 30), "Monday: 02:30 EDT");
        // 03:00 sharp and 09:00 are untouched.
        assert_eq!(daily("03:00").next_due(0, utc(2026, 3, 8, 6, 0), &ny), Some(utc(2026, 3, 8, 7, 0)));
        assert_eq!(daily("09:00").next_due(0, utc(2026, 3, 8, 6, 0), &ny), Some(utc(2026, 3, 8, 13, 0)));
        // In Europe the clocks jump from 01:00 to 02:00 UTC+1: 01:30 on 2026-03-29 runs at 02:30 BST = 01:30 UTC.
        let london = Rules::eu(0);
        assert_eq!(daily("01:30").next_due(0, utc(2026, 3, 28, 12, 0), &london), Some(utc(2026, 3, 29, 1, 30)));
    }

    #[test]
    fn fall_back_a_repeated_time_happens_once_at_its_first_occurrence() {
        let ny = Rules::us(-5 * 3600);
        // Sunday 2026-11-01 01:30 happens twice in New York (first EDT = 05:30 UTC, then EST = 06:30 UTC). It runs once, at 05:30 UTC.
        let s = daily("01:30");
        let due = s.due_between(0, utc(2026, 10, 30, 12, 0), utc(2026, 11, 2, 12, 0), &ny, 20);
        assert_eq!(due, vec![utc(2026, 10, 31, 5, 30), utc(2026, 11, 1, 5, 30), utc(2026, 11, 2, 6, 30)], "{due:?}");
        // Asked again right after the first occurrence, the second hour's 01:30 is not offered.
        assert_eq!(s.next_due(0, utc(2026, 11, 1, 5, 30), &ny), Some(utc(2026, 11, 2, 6, 30)));
        // An every-day 09:00 across the change stays at 09:00 by the wall clock: an hour later in UTC after the clocks go back.
        let nine = daily("09:00");
        assert_eq!(nine.next_due(0, utc(2026, 10, 31, 14, 0), &ny), Some(utc(2026, 11, 1, 14, 0)));
        assert_eq!(nine.next_due(0, utc(2026, 11, 1, 14, 0), &ny), Some(utc(2026, 11, 2, 14, 0)));
        let london = Rules::eu(0);
        // Sunday 2026-10-25 01:30 happens twice in London (BST 00:30 UTC, then GMT 01:30 UTC): once, at 00:30 UTC.
        let due = daily("01:30").due_between(0, utc(2026, 10, 24, 12, 0), utc(2026, 10, 26, 12, 0), &london, 20);
        assert_eq!(due, vec![utc(2026, 10, 25, 0, 30), utc(2026, 10, 26, 1, 30)]);
    }

    #[test]
    fn the_wall_clock_reads_back_from_an_instant() {
        let ny = Rules::us(-5 * 3600);
        let l = local_parts(&ny, utc(2026, 3, 8, 7, 30));
        assert_eq!((l.year, l.month, l.day, l.hour, l.minute, l.weekday), (2026, 3, 8, 3, 30, Weekday::Sun));
        let l = local_parts(&Fixed(-3600), 0);
        assert_eq!((l.year, l.month, l.day, l.hour), (1969, 12, 31, 23));
    }

    #[test]
    fn json_shapes_are_plain() {
        let s = Schedule::Weekdays { days: vec![Weekday::Mon, Weekday::Fri], at: "09:00".into() };
        assert_eq!(serde_json::to_string(&s).unwrap(), r#"{"kind":"weekdays","days":["mon","fri"],"at":"09:00"}"#);
        assert_eq!(serde_json::to_string(&Schedule::Every { hours: 6 }).unwrap(), r#"{"kind":"every","hours":6}"#);
        assert_eq!(serde_json::to_string(&Schedule::Manual).unwrap(), r#"{"kind":"manual"}"#);
        assert_eq!(serde_json::from_str::<Schedule>(r#"{"kind":"daily","at":"07:15"}"#).unwrap(), daily("07:15"));
    }
}
