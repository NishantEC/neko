//! The Mac's own time zone, as libc reads it from `TZ`, agrees with the rules the unit tests use. One test in this file, so nothing
//! else in the process reads the environment while it is set.

use neko_protocol::schedule::{Schedule, SystemZone, Zone, days_from_civil};

fn utc(y: i64, mo: u32, d: u32, h: u32, mi: u32) -> i64 {
    (days_from_civil(y, mo, d) * 86_400 + i64::from(h) * 3600 + i64::from(mi) * 60) * 1000
}

#[test]
fn the_system_zone_follows_tz_across_both_daylight_saving_changes() {
    if !std::path::Path::new("/usr/share/zoneinfo/America/New_York").exists() {
        eprintln!("no zoneinfo on this machine: skipped");
        return;
    }
    // SAFETY: the only test in this process, and no thread of ours is reading the environment.
    unsafe { std::env::set_var("TZ", "America/New_York") };
    let z = SystemZone;
    assert_eq!(z.offset_secs(utc(2026, 3, 8, 6, 59) / 1000), -5 * 3600);
    assert_eq!(z.offset_secs(utc(2026, 3, 8, 7, 0) / 1000), -4 * 3600);
    assert_eq!(z.offset_secs(utc(2026, 11, 1, 5, 59) / 1000), -4 * 3600);
    assert_eq!(z.offset_secs(utc(2026, 11, 1, 6, 0) / 1000), -5 * 3600);

    // The same answers as the unit tests give with the rules written out.
    let daily = |at: &str| Schedule::Daily { at: at.into() };
    assert_eq!(daily("02:30").next_due(0, utc(2026, 3, 7, 12, 0), &z), Some(utc(2026, 3, 8, 7, 30)), "a skipped time runs once, the gap's length later");
    assert_eq!(daily("01:30").next_due(0, utc(2026, 10, 31, 12, 0), &z), Some(utc(2026, 11, 1, 5, 30)), "a repeated time runs at its first occurrence");
    assert_eq!(daily("01:30").next_due(0, utc(2026, 11, 1, 5, 30), &z), Some(utc(2026, 11, 2, 6, 30)));

    // SAFETY: as above.
    unsafe { std::env::set_var("TZ", "Asia/Kolkata") };
    assert_eq!(z.offset_secs(utc(2026, 11, 27, 0, 0) / 1000), 19_800);
    assert_eq!(daily("09:00").next_due(0, utc(2026, 11, 27, 0, 0), &z), Some(utc(2026, 11, 27, 3, 30)));
}
