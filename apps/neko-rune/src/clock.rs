//! Local time for the Calendar and the pages that print a time of day. The daemon speaks Unix
//! milliseconds; people read their own wall clock, which has time zones and daylight saving. The
//! C library knows both (`localtime_r`, `mktime`), so this is a thin, tested wrapper rather than a
//! date crate.

/// A moment as the wall clock shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Local {
    pub year: i32,
    /// 1 to 12.
    pub month: u32,
    pub day: u32,
    pub hour: u32,
    pub minute: u32,
    /// 0 is Monday.
    pub weekday: u32,
}

fn tm_at(ms: i64) -> libc::tm {
    let secs = ms.div_euclid(1000) as libc::time_t;
    // SAFETY: a zeroed `tm` is a valid value; `localtime_r` fills it from `secs`, and neither pointer escapes.
    unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&secs, &mut tm);
        tm
    }
}

pub fn local(ms: i64) -> Local {
    let tm = tm_at(ms);
    Local { year: tm.tm_year + 1900, month: tm.tm_mon as u32 + 1, day: tm.tm_mday as u32, hour: tm.tm_hour as u32, minute: tm.tm_min as u32, weekday: ((tm.tm_wday + 6) % 7) as u32 }
}

fn from_tm(mut tm: libc::tm) -> i64 {
    tm.tm_isdst = -1;
    // SAFETY: `mktime` normalises the struct it is given (out-of-range days roll over).
    let secs = unsafe { libc::mktime(&mut tm) };
    secs as i64 * 1000
}

/// Local midnight at the start of the day `ms` falls in.
pub fn day_start(ms: i64) -> i64 {
    let mut tm = tm_at(ms);
    tm.tm_hour = 0;
    tm.tm_min = 0;
    tm.tm_sec = 0;
    from_tm(tm)
}

/// Local midnight `n` days after the one that starts at `day_start_ms` (days can be 23 or 25 hours long).
pub fn add_days(day_start_ms: i64, n: i64) -> i64 {
    let mut tm = tm_at(day_start_ms + 12 * 3_600_000);
    tm.tm_mday += n as i32;
    tm.tm_hour = 0;
    tm.tm_min = 0;
    tm.tm_sec = 0;
    from_tm(tm)
}

/// The Monday that starts the week `ms` falls in.
pub fn week_start(ms: i64) -> i64 {
    let start = day_start(ms);
    add_days(start, -(local(start + 12 * 3_600_000).weekday as i64))
}

/// Local midnight of the first day of the month `ms` falls in.
pub fn month_start(ms: i64) -> i64 {
    let mut tm = tm_at(ms);
    tm.tm_mday = 1;
    tm.tm_hour = 0;
    tm.tm_min = 0;
    tm.tm_sec = 0;
    from_tm(tm)
}

/// Local midnight of the first day of the month `n` months after the one that starts at `month_start_ms`.
pub fn add_months(month_start_ms: i64, n: i64) -> i64 {
    let mut tm = tm_at(month_start_ms + 12 * 3_600_000);
    tm.tm_mday = 1;
    tm.tm_mon += n as i32;
    tm.tm_hour = 0;
    tm.tm_min = 0;
    tm.tm_sec = 0;
    from_tm(tm)
}

pub const MONTHS_LONG: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// "November 2026".
pub fn month_label(ms: i64) -> String {
    let l = local(ms + 12 * 3_600_000);
    format!("{} {}", MONTHS_LONG[l.month as usize - 1], l.year)
}

/// "14:05".
pub fn hhmm(ms: i64) -> String {
    let l = local(ms);
    format!("{:02}:{:02}", l.hour, l.minute)
}

pub const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
pub const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// "Mon 16 Nov".
pub fn day_label(ms: i64) -> String {
    let l = local(ms + 12 * 3_600_000);
    format!("{} {} {}", WEEKDAYS[l.weekday as usize], l.day, MONTHS[l.month as usize - 1])
}

/// Seconds since 2001-01-01 UTC, which is what Calendar's `calshow:` link counts.
pub fn calshow_seconds(ms: i64) -> i64 {
    ms.div_euclid(1000) - 978_307_200
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_day_starts_at_local_midnight_and_a_week_on_monday() {
        let ms = 1_800_000_000_000;
        let start = day_start(ms);
        assert!(start <= ms && ms - start < 25 * 3_600_000);
        assert_eq!((local(start).hour, local(start).minute), (0, 0));
        let monday = week_start(ms);
        assert_eq!(local(monday + 12 * 3_600_000).weekday, 0, "a week starts on Monday");
        assert!(ms - monday < 7 * 25 * 3_600_000 && monday <= ms);
    }

    #[test]
    fn adding_days_lands_on_midnight_even_across_a_clock_change() {
        let start = day_start(1_800_000_000_000);
        for n in [-40, -1, 1, 7, 100, 200] {
            let d = add_days(start, n);
            assert_eq!((local(d).hour, local(d).minute), (0, 0), "{n}");
            assert!((d - start - n * 86_400_000).abs() <= 3_600_000, "{n}: within the clock change");
        }
        assert_eq!(add_days(add_days(start, 3), -3), start);
    }

    #[test]
    fn months_start_on_the_first_and_step_by_whole_months() {
        let ms = 1_800_000_000_000;
        let first = month_start(ms);
        assert_eq!((local(first + 12 * 3_600_000).day, local(first).hour, local(first).minute), (1, 0, 0));
        assert!(first <= ms && ms - first < 32 * 86_400_000);
        let next = add_months(first, 1);
        assert_eq!(local(next + 12 * 3_600_000).day, 1);
        assert!(next - first >= 28 * 86_400_000 - 3_600_000 && next - first <= 31 * 86_400_000 + 3_600_000);
        assert_eq!(add_months(next, -1), first);
        assert!(add_months(first, 12) - first >= 364 * 86_400_000);
        assert!(month_label(first).ends_with(&local(first + 12 * 3_600_000).year.to_string()));
    }

    #[test]
    fn labels_are_short_and_calshow_counts_from_2001() {
        assert_eq!(calshow_seconds(978_307_200_000), 0);
        let monday = week_start(1_800_000_000_000);
        assert!(day_label(monday).starts_with("Mon "), "{}", day_label(monday));
        assert_eq!(hhmm(day_start(1_800_000_000_000)), "00:00");
    }
}
