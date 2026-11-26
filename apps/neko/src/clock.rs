//! Local time, from the system's zone rules (`localtime_r`), as plain numbers the rest of the app computes with.

/// The local zone's offset from UTC, in seconds, at `unix_s`.
pub fn local_offset_s(unix_s: i64) -> i64 {
    let t = unix_s as libc::time_t;
    // SAFETY: a zeroed `tm` is a valid out-parameter; localtime_r is thread-safe.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    let ok = unsafe { !libc::localtime_r(&t, &mut tm).is_null() };
    if ok { tm.tm_gmtoff } else { 0 }
}

/// The local hour (0 to 23) at `now_ms`.
pub fn local_hour(now_ms: i64, offset_s: i64) -> i64 {
    (now_ms / 1000 + offset_s).rem_euclid(86_400) / 3600
}

/// "14:05", the local time of day at `ms`.
pub fn time_of_day(ms: i64, offset_s: i64) -> String {
    let s = (ms / 1000 + offset_s).rem_euclid(86_400);
    format!("{:02}:{:02}", s / 3600, s % 3600 / 60)
}

/// When a task was added, as a person says it: "today at 14:05", "yesterday at 09:30", else the date.
pub fn added(ms: i64, now_ms: i64, offset_s: i64) -> String {
    let day = |m: i64| (m / 1000 + offset_s).div_euclid(86_400);
    let at = time_of_day(ms, offset_s);
    match day(now_ms) - day(ms) {
        0 => format!("today at {at}"),
        1 => format!("yesterday at {at}"),
        _ => {
            let (y, m, d) = civil(day(ms));
            const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
            format!("{d} {} {y}", MONTHS[(m - 1) as usize])
        }
    }
}

/// Days since 1970-01-01 to (year, month, day), by Howard Hinnant's civil-from-days.
fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_read_as_a_person_says_them() {
        let now = 1_795_687_200_000; // 2026-11-26 10:00 UTC
        assert_eq!(added(now, now, 0), "today at 10:00");
        assert_eq!(added(now - 86_400_000, now, 0), "yesterday at 10:00");
        assert_eq!(added(now - 3 * 86_400_000, now, 0), "23 November 2026");
        assert_eq!(added(now, now, 3600 + 1800), "today at 11:30");
        assert_eq!(civil(0), (1970, 1, 1));
    }
}

/// The day, as a page is dated ("Thursday 26 November").
pub fn date_line(now_ms: i64, offset_s: i64) -> String {
    let days = (now_ms / 1000 + offset_s).div_euclid(86_400);
    // 1970-01-01 was a Thursday.
    let weekday = ["Thursday", "Friday", "Saturday", "Sunday", "Monday", "Tuesday", "Wednesday"][days.rem_euclid(7) as usize];
    // The civil date from days since 1970 (Howard Hinnant's algorithm, written out).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let month = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"][(m - 1) as usize];
    format!("{weekday} {d} {month}")
}

#[cfg(test)]
mod date_tests {
    #[test]
    fn a_day_reads_as_a_page_is_dated() {
        assert_eq!(super::date_line(crate::fixed_clock().0, 0), "Thursday 26 November");
    }
}
