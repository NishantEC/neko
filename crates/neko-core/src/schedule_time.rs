//! Bounded RFC recurrence evaluation. Timezone and anchor are durable data;
//! importing a schedule does not silently turn local wall time into UTC.

/// Returns one future occurrence, with the recurrence library's validation and
/// iteration limits enabled. Caller should evaluate outside the database lock.
pub fn next_occurrence(
    rule: &str,
    timezone: &str,
    anchor_ms: i64,
    after_ms: i64,
) -> Result<i64, String> {
    following_occurrence(rule, timezone, anchor_ms, after_ms)?
        .ok_or_else(|| "Schedule has ended".into())
}

/// None means a validated recurrence ended, not a traversal or validation error.
/// The distinction lets a persisted final due occurrence run before disabling.
pub fn following_occurrence(
    rule: &str,
    timezone: &str,
    anchor_ms: i64,
    after_ms: i64,
) -> Result<Option<i64>, String> {
    if rule.len() > 2048 || timezone.len() > 128 || rule.contains(['\n', '\r']) {
        return Err("Schedule rule is invalid or too large".into());
    }
    let rule = rule.strip_prefix("RRULE:").unwrap_or(rule);
    if !rule.split(';').any(|field| {
        matches!(
            field,
            "FREQ=HOURLY" | "FREQ=DAILY" | "FREQ=WEEKLY" | "FREQ=MONTHLY" | "FREQ=YEARLY"
        )
    }) {
        return Err("Use an hourly, daily, weekly, monthly or yearly schedule".into());
    }
    // Dense second/minute expansion can exhaust the recurrence search budget.
    for field in rule.split(';') {
        if let Some(value) = field
            .strip_prefix("BYSECOND=")
            .or_else(|| field.strip_prefix("BYMINUTE="))
        {
            if value.contains(',') || value.parse::<u8>().is_err() {
                return Err("Use one minute and second within each scheduled hour".into());
            }
        }
    }
    std::panic::catch_unwind(|| {
        let zone: chrono_tz::Tz = timezone.parse().map_err(|_| "Unknown schedule timezone")?;
        let zone = rrule::Tz::from(zone);
        let start = chrono::DateTime::from_timestamp_millis(anchor_ms)
            .ok_or("Invalid schedule anchor")?
            .with_timezone(&zone);
        let after = chrono::DateTime::from_timestamp_millis(
            after_ms.checked_add(1).ok_or("Invalid schedule time")?,
        )
        .ok_or("Invalid schedule time")?
        .with_timezone(&zone);
        let rule: rrule::RRule<rrule::Unvalidated> =
            rule.parse().map_err(|_| "Invalid recurrence rule")?;
        let rule = rule
            .validate(start)
            .map_err(|_| "Unsupported recurrence rule")?;
        // An UNTIL at/before the exclusive lower bound proves no future event.
        if rule.get_until().is_some_and(|until| *until < after) {
            return Ok(None);
        }
        let count = rule.get_count();
        let has_until = rule.get_until().is_some();
        // Collect BEFORE filtering: after() would leave skipped traversal
        // unbounded. The public result also reports internal iterator limits.
        let set = rrule::RRuleSet::new(start).rrule(rule).limit();
        let result = set.all(10_000);
        if let Some(date) = result.dates.iter().find(|date| **date >= after) {
            return Ok(Some(date.timestamp_millis()));
        }
        // all() marks an exact 10,000 item result limited even when COUNT was
        // exactly 10,000. Reaching the validated count proves exhaustion.
        if count.is_some_and(|count| result.dates.len() >= count as usize) {
            return Ok(None);
        }
        if result.limited || !has_until {
            return Err("Schedule exceeds the bounded recurrence search");
        }
        Ok(None)
    })
    .map_err(|_| "Recurrence evaluation failed safely".to_owned())?
    .map_err(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn at(value: &str) -> i64 {
        chrono::DateTime::parse_from_rfc3339(value)
            .unwrap()
            .timestamp_millis()
    }
    #[test]
    fn old_hourly_anchor_exhausts_total_search_budget() {
        assert!(
            next_occurrence(
                "FREQ=HOURLY",
                "UTC",
                at("1900-01-01T00:00:00Z"),
                at("2026-01-01T00:00:00Z")
            )
            .is_err()
        );
    }
    #[test]
    fn daily_schedule_preserves_wall_clock_across_dst() {
        let next = next_occurrence(
            "FREQ=DAILY;BYHOUR=9;BYMINUTE=0;BYSECOND=0",
            "America/New_York",
            at("2026-03-07T14:00:00Z"),
            at("2026-03-07T14:00:00Z"),
        )
        .unwrap();
        assert_eq!(next, at("2026-03-08T13:00:00Z"));
    }
    #[test]
    fn interval_uses_original_anchor_and_skips_missed_occurrences() {
        let next = next_occurrence(
            "FREQ=HOURLY;INTERVAL=2",
            "UTC",
            at("2026-09-25T00:00:00Z"),
            at("2026-09-25T05:00:00Z"),
        )
        .unwrap();
        assert_eq!(next, at("2026-09-25T06:00:00Z"));
    }
    #[test]
    fn malformed_exhausted_and_excessively_frequent_rules_fail_closed() {
        for rule in [
            "invalid",
            "FREQ=SECONDLY",
            "FREQ=MINUTELY",
            "FREQ=DAILY;COUNT=1",
        ] {
            assert!(
                next_occurrence(
                    rule,
                    "UTC",
                    at("2026-01-01T00:00:00Z"),
                    at("2026-09-25T00:00:00Z")
                )
                .is_err()
            );
        }
        assert!(next_occurrence("FREQ=DAILY", "not-a-timezone", 0, 1).is_err());
    }
    #[test]
    fn finite_end_is_distinct_from_invalid_rule_and_both_traversal_limits() {
        assert_eq!(
            following_occurrence("FREQ=HOURLY;COUNT=1", "UTC", 0, 0).unwrap(),
            None
        );
        assert_eq!(
            following_occurrence("FREQ=HOURLY;COUNT=10000", "UTC", 0, 36_000_000_000).unwrap(),
            None
        );
        assert!(following_occurrence("FREQ=HOURLY;COUNT=10001", "UTC", 0, 36_000_000_000).is_err());
        assert!(following_occurrence("FREQ=DAILY;BYMONTH=2;BYMONTHDAY=30", "UTC", 0, 0).is_err());
        assert!(following_occurrence("invalid", "UTC", 0, 0).is_err());
    }
}
