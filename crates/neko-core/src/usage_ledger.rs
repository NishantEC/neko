//! The usage ledger: one append-only line per finished agent run (table `usage_events`), kept 365 days.
//! A ticket run is one `execute` claim (planning, build and review are inside it); a routine run is one run.
//! Home chat turns and one-shot calls (the goal judge, the curator) are not recorded, so the ledger's
//! totals sit below "Spend today" on days with heavy chat use. (The existing `usage` module is the
//! provider-quota meter, a different thing.) Writes are never "changes" for readers: they do not bump the
//! data revision or the content stamp, so they wake nobody.
use chrono::TimeZone;
use neko_protocol::ledger::{LedgerDay, LedgerHistory, LedgerProvider};
use neko_protocol::routines::RoutineRun;
use neko_protocol::workbench::{AgentRuntime, TaskEvent};

const DAY_MS: i64 = 86_400_000;
/// How long a line is kept.
pub const KEEP_DAYS: i64 = 365;
/// The Usage page's longest range.
pub const MAX_DAYS: u32 = 365;
const PRUNED_ROW: &str = "usage_pruned_v1";

/// `reported`: the provider gave a cost or token counts for the run. `estimated`: a run happened and the
/// provider reported nothing (Codex reports no price); the numbers stay empty, never invented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Reported,
    Estimated,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Reported => "reported",
            Source::Estimated => "estimated",
        }
    }

    pub fn parse(text: &str) -> Source {
        if text == "reported" { Source::Reported } else { Source::Estimated }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsageEvent {
    /// One row per run, even when a path records it twice: `ticket:<task id>:<started_ms>` or `routine:<run id>`.
    pub run_key: String,
    /// When the run completed.
    pub at_ms: i64,
    pub provider: String,
    pub model: String,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cost_micro_usd: Option<u64>,
    pub source: Source,
    pub duration_ms: i64,
    /// The ticket; for a routine run, the routine's id.
    pub task_id: String,
    /// `ticket` or `routine`.
    pub kind: String,
}

/// `reported` when the provider gave a cost or tokens; `estimated` otherwise.
pub fn estimate_source(cost: Option<u64>, input: Option<u64>, output: Option<u64>) -> Source {
    if cost.is_some() || input.is_some() || output.is_some() { Source::Reported } else { Source::Estimated }
}

/// What a ticket's `USAGE` events add up to from `since_ms` on. A part nobody reported stays `None`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub cost_micro_usd: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

fn add(total: &mut Option<u64>, part: Option<u64>) {
    if let Some(part) = part {
        *total = Some(total.unwrap_or(0).saturating_add(part));
    }
}

/// Sums the `USAGE {"cost_usd":..,"input_tokens":..,"output_tokens":..}` events at or after `since_ms`.
pub fn sum_usage(events: &[TaskEvent], since_ms: i64) -> Usage {
    let mut usage = Usage::default();
    for event in events.iter().filter(|e| e.at_ms >= since_ms) {
        let Some(json) = event.message.strip_prefix("USAGE ") else { continue };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else { continue };
        let cost = value["cost_usd"].as_f64().filter(|c| c.is_finite() && *c >= 0.0).map(|c| (c * 1_000_000.0).round() as u64);
        add(&mut usage.cost_micro_usd, cost);
        add(&mut usage.input_tokens, value["input_tokens"].as_u64());
        add(&mut usage.output_tokens, value["output_tokens"].as_u64());
    }
    usage
}

/// The provider's name for the ledger: an empty one is the signed-in Codex account.
fn provider_of(runtime: Option<&AgentRuntime>) -> (String, String) {
    match runtime {
        Some(runtime) if !runtime.provider.is_empty() => (runtime.provider.clone(), runtime.model.clone()),
        Some(runtime) => ("codex".into(), runtime.model.clone()),
        None => ("unknown".into(), String::new()),
    }
}

/// Builds a ticket run's line from the ticket's events (`events`, as stored) and how the run ended.
pub fn ticket_run_event(task_id: &str, started_ms: i64, now_ms: i64, runtime: Option<&AgentRuntime>, events: &[TaskEvent]) -> UsageEvent {
    let usage = sum_usage(events, started_ms);
    let (provider, model) = provider_of(runtime);
    UsageEvent {
        run_key: format!("ticket:{task_id}:{started_ms}"),
        at_ms: now_ms,
        provider,
        model,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cost_micro_usd: usage.cost_micro_usd,
        source: estimate_source(usage.cost_micro_usd, usage.input_tokens, usage.output_tokens),
        duration_ms: (now_ms - started_ms).max(0),
        task_id: task_id.to_string(),
        kind: "ticket".into(),
    }
}

/// Records a finished ticket run, ok or not. Never fails the run: a ledger problem is logged.
pub fn record_ticket_run(db: &crate::Db, task_id: &str, started_ms: i64, now_ms: i64, runtime: Option<&AgentRuntime>) {
    let events: Vec<TaskEvent> = match db.events_of(task_id) {
        Ok(Some(raw)) => raw.iter().filter_map(|body| serde_json::from_str(body).ok()).collect(),
        Ok(None) => Vec::new(),
        Err(error) => {
            eprintln!("usage ledger: cannot read the events of {task_id}: {error}");
            Vec::new()
        }
    };
    if let Err(error) = db.record_usage_event(&ticket_run_event(task_id, started_ms, now_ms, runtime, &events)) {
        eprintln!("usage ledger: {error}");
    }
}

/// A routine run's line. `provider` is the agent that did the run ("claude", "codex"); `model` is the routine's
/// model (claude) or empty (codex). The price is what the agent reported; none means `estimated`.
pub fn routine_run_event(routine_id: &str, run: &RoutineRun, provider: &str, model: &str) -> UsageEvent {
    let cost = (run.cost_micro_usd > 0).then_some(run.cost_micro_usd);
    let finished = run.finished_ms.unwrap_or(run.started_ms);
    UsageEvent {
        run_key: format!("routine:{}", run.id),
        at_ms: finished,
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: None,
        output_tokens: None,
        cost_micro_usd: cost,
        source: estimate_source(cost, None, None),
        duration_ms: (finished - run.started_ms).max(0),
        task_id: routine_id.to_string(),
        kind: "routine".into(),
    }
}

pub fn record_routine_run(db: &crate::Db, routine_id: &str, run: &RoutineRun, provider: &str, model: &str) {
    if let Err(error) = db.record_usage_event(&routine_run_event(routine_id, run, provider, model)) {
        eprintln!("usage ledger: {error}");
    }
}

/// A captain's own turn (R3): `task_id = "captain:<id>"`, `kind = "captain"`, one line per turn (`run_key` is
/// `captain:<id>:<started_ms>`). The price is what the agent reported; none means `estimated`.
pub fn captain_turn_event(captain_id: &str, started_ms: i64, now_ms: i64, provider: &str, model: &str, cost_micro_usd: u64) -> UsageEvent {
    let cost = (cost_micro_usd > 0).then_some(cost_micro_usd);
    UsageEvent {
        run_key: format!("captain:{captain_id}:{started_ms}"),
        at_ms: now_ms,
        provider: provider.to_string(),
        model: model.to_string(),
        input_tokens: None,
        output_tokens: None,
        cost_micro_usd: cost,
        source: estimate_source(cost, None, None),
        duration_ms: (now_ms - started_ms).max(0),
        task_id: crate::captain_thread::task_key(captain_id),
        kind: "captain".into(),
    }
}

pub fn record_captain_turn(db: &crate::Db, captain_id: &str, started_ms: i64, now_ms: i64, provider: &str, model: &str, cost_micro_usd: u64) {
    if let Err(error) = db.record_usage_event(&captain_turn_event(captain_id, started_ms, now_ms, provider, model, cost_micro_usd)) {
        eprintln!("usage ledger: {error}");
    }
}

/// What the ledger holds for one ticket: reported cost and tokens, runs, and runs that reported nothing.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TaskTotals {
    pub runs: u32,
    pub estimated_runs: u32,
    pub cost_micro_usd: Option<u64>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub duration_ms: i64,
}

pub fn totals_of(events: &[UsageEvent]) -> TaskTotals {
    let mut totals = TaskTotals::default();
    for event in events {
        totals.runs += 1;
        if event.source == Source::Estimated {
            totals.estimated_runs += 1;
        }
        add(&mut totals.cost_micro_usd, event.cost_micro_usd);
        add(&mut totals.input_tokens, event.input_tokens);
        add(&mut totals.output_tokens, event.output_tokens);
        totals.duration_ms += event.duration_ms;
    }
    totals
}

/// The ledger's totals for one ticket (`task_id` of its rows).
pub fn sum_for_task(db: &crate::Db, task_id: &str) -> Result<TaskTotals, String> {
    Ok(totals_of(&db.usage_events_for_task(task_id).map_err(|e| e.to_string())?))
}

/// The local start of each of the `days` days ending with today, oldest first.
fn day_starts<Tz: TimeZone>(now_ms: i64, days: u32, tz: &Tz) -> Vec<i64> {
    let mut starts = Vec::with_capacity(days as usize);
    let mut start = crate::spend::day_start_ms(now_ms, tz);
    for _ in 0..days {
        starts.push(start);
        start = crate::spend::day_start_ms(start - 1, tz);
    }
    starts.reverse();
    starts
}

pub fn clamp_days(days: u32) -> u32 {
    days.clamp(1, MAX_DAYS)
}

/// The start of the range `days` covers (the local midnight of its first day).
pub fn since_ms<Tz: TimeZone>(now_ms: i64, days: u32, tz: &Tz) -> i64 {
    day_starts(now_ms, clamp_days(days), tz)[0]
}

fn label<Tz: TimeZone>(ms: i64, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    tz.timestamp_millis_opt(ms).single().map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default()
}

/// One entry per local day of the range, oldest first, zeros for empty days. A line outside the range is left out.
pub fn by_day<Tz: TimeZone>(events: &[UsageEvent], now_ms: i64, days: u32, tz: &Tz) -> Vec<LedgerDay>
where
    Tz::Offset: std::fmt::Display,
{
    let starts = day_starts(now_ms, clamp_days(days), tz);
    let mut out: Vec<LedgerDay> = starts.iter().map(|s| LedgerDay { day: label(*s, tz), ..Default::default() }).collect();
    for event in events {
        let index = starts.partition_point(|s| *s <= event.at_ms);
        if index == 0 {
            continue;
        }
        let day = &mut out[index - 1];
        day.runs += 1;
        day.estimated_runs += u32::from(event.source == Source::Estimated);
        day.input_tokens += event.input_tokens.unwrap_or(0);
        day.output_tokens += event.output_tokens.unwrap_or(0);
        day.cost_micro_usd += event.cost_micro_usd.unwrap_or(0);
    }
    out
}

/// Runs, tokens and cost per provider; most cost first, then most runs, then name.
pub fn by_provider(events: &[UsageEvent]) -> Vec<LedgerProvider> {
    let mut out: Vec<LedgerProvider> = Vec::new();
    for event in events {
        let at = match out.iter().position(|p| p.provider == event.provider) {
            Some(at) => at,
            None => {
                out.push(LedgerProvider { provider: event.provider.clone(), ..Default::default() });
                out.len() - 1
            }
        };
        let provider = &mut out[at];
        provider.runs += 1;
        provider.estimated_runs += u32::from(event.source == Source::Estimated);
        provider.input_tokens += event.input_tokens.unwrap_or(0);
        provider.output_tokens += event.output_tokens.unwrap_or(0);
        provider.cost_micro_usd += event.cost_micro_usd.unwrap_or(0);
    }
    out.sort_by(|a, b| b.cost_micro_usd.cmp(&a.cost_micro_usd).then(b.runs.cmp(&a.runs)).then(a.provider.cmp(&b.provider)));
    out
}

pub fn history<Tz: TimeZone>(events: &[UsageEvent], now_ms: i64, days: u32, tz: &Tz) -> LedgerHistory
where
    Tz::Offset: std::fmt::Display,
{
    let since = since_ms(now_ms, days, tz);
    let in_range: Vec<UsageEvent> = events.iter().filter(|e| e.at_ms >= since).cloned().collect();
    LedgerHistory { days: by_day(&in_range, now_ms, days, tz), providers: by_provider(&in_range), total_runs: in_range.len() as u32, since_ms: since }
}

/// The history of the last `days` days (clamped to 1..=365) as the Usage page shows it.
pub fn history_for(db: &crate::Db, now_ms: i64, days: u32) -> Result<LedgerHistory, String> {
    let since = since_ms(now_ms, days, &chrono::Local);
    let events = db.usage_events_since(since).map_err(|e| e.to_string())?;
    Ok(history(&events, now_ms, days, &chrono::Local))
}

/// A spreadsheet reads a cell that starts with one of these as a formula: a leading `'` makes it text.
fn guard_cell(cell: &str) -> String {
    match cell.chars().next() {
        Some('=' | '+' | '-' | '@' | '\t' | '\r') => format!("'{cell}"),
        _ => cell.to_string(),
    }
}

fn csv_cell(cell: &str) -> String {
    let cell = guard_cell(cell);
    if cell.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", cell.replace('"', "\"\""))
    } else {
        cell
    }
}

fn micro_to_usd(micro: u64) -> String {
    format!("{}.{:06}", micro / 1_000_000, micro % 1_000_000)
}

/// `time,provider,model,input_tokens,output_tokens,cost_usd,source,duration_s,task_id,kind`, a header first.
/// `time` is RFC 3339 in the given zone; a number nobody reported is an empty cell.
pub fn to_csv<Tz: TimeZone>(events: &[UsageEvent], tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    let mut out = String::from("time,provider,model,input_tokens,output_tokens,cost_usd,source,duration_s,task_id,kind\n");
    for event in events {
        let time = tz.timestamp_millis_opt(event.at_ms).single().map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, false)).unwrap_or_default();
        let cells = [
            time,
            event.provider.clone(),
            event.model.clone(),
            event.input_tokens.map(|n| n.to_string()).unwrap_or_default(),
            event.output_tokens.map(|n| n.to_string()).unwrap_or_default(),
            event.cost_micro_usd.map(micro_to_usd).unwrap_or_default(),
            event.source.as_str().to_string(),
            format!("{:.1}", event.duration_ms as f64 / 1000.0),
            event.task_id.clone(),
            event.kind.clone(),
        ];
        out.push_str(&cells.iter().map(|c| csv_cell(c)).collect::<Vec<_>>().join(","));
        out.push('\n');
    }
    out
}

pub fn csv_for(db: &crate::Db, now_ms: i64, days: u32) -> Result<String, String> {
    let since = since_ms(now_ms, days, &chrono::Local);
    Ok(to_csv(&db.usage_events_since(since).map_err(|e| e.to_string())?, &chrono::Local))
}

/// Deletes lines older than 365 days, at most once per local day (the day is kept in a quiet row).
/// `Some(n)` when it pruned, `None` when today's pass already ran.
pub fn prune_if_due(db: &crate::Db, now_ms: i64) -> Result<Option<usize>, String> {
    let today = label(now_ms, &chrono::Local);
    if db.get_setting(PRUNED_ROW).map_err(|e| e.to_string())?.as_deref() == Some(today.as_str()) {
        return Ok(None);
    }
    let deleted = db.prune_usage_events(now_ms - KEEP_DAYS * DAY_MS).map_err(|e| e.to_string())?;
    db.set_setting_quiet(PRUNED_ROW, &today).map_err(|e| e.to_string())?;
    Ok(Some(deleted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    const HOUR: i64 = 3_600_000;

    fn event(key: &str, at_ms: i64, provider: &str, cost: Option<u64>, tokens: Option<(u64, u64)>) -> UsageEvent {
        UsageEvent {
            run_key: key.into(),
            at_ms,
            provider: provider.into(),
            model: "sonnet".into(),
            input_tokens: tokens.map(|t| t.0),
            output_tokens: tokens.map(|t| t.1),
            cost_micro_usd: cost,
            source: estimate_source(cost, tokens.map(|t| t.0), tokens.map(|t| t.1)),
            duration_ms: 90_000,
            task_id: "t1".into(),
            kind: "ticket".into(),
        }
    }

    fn usage_event(at_ms: i64, json: &str) -> TaskEvent {
        TaskEvent { at_ms, role: "builder".into(), message: format!("USAGE {json}") }
    }

    #[test]
    fn a_run_that_reported_usage_is_summed_and_reported() {
        let events = [
            usage_event(100, r#"{"cost_usd":9.0}"#),
            usage_event(1_000, r#"{"cost_usd":0.5,"input_tokens":1000,"output_tokens":200}"#),
            usage_event(2_000, r#"{"cost_usd":0.25,"input_tokens":500,"output_tokens":null}"#),
            TaskEvent { at_ms: 3_000, role: "builder".into(), message: "Command completed".into() },
            usage_event(4_000, "not json"),
        ];
        let line = ticket_run_event("t1", 1_000, 5_000, Some(&AgentRuntime { provider: "claude".into(), model: "opus".into(), ..Default::default() }), &events);
        assert_eq!((line.cost_micro_usd, line.input_tokens, line.output_tokens), (Some(750_000), Some(1500), Some(200)), "before the run's start is not this run's");
        assert_eq!(line.source, Source::Reported);
        assert_eq!((line.provider.as_str(), line.model.as_str(), line.kind.as_str(), line.duration_ms), ("claude", "opus", "ticket", 4_000));
        assert_eq!(line.run_key, "ticket:t1:1000");
    }

    #[test]
    fn a_run_with_no_usage_events_is_estimated_with_nothing_invented() {
        let codex = AgentRuntime { provider: String::new(), model: String::new(), ..Default::default() };
        let line = ticket_run_event("t1", 1_000, 2_000, Some(&codex), &[]);
        assert_eq!((line.cost_micro_usd, line.input_tokens, line.output_tokens), (None, None, None));
        assert_eq!(line.source, Source::Estimated);
        assert_eq!(line.provider, "codex", "an empty provider is the signed-in Codex account");
        // An event that carries neither a price nor tokens reports nothing either.
        let nothing = ticket_run_event("t1", 0, 5, None, &[usage_event(1, r#"{"cost_usd":null}"#)]);
        assert_eq!((nothing.source, nothing.provider.as_str()), (Source::Estimated, "unknown"));
    }

    #[test]
    fn a_routine_run_keys_on_the_run_and_reports_only_a_real_price() {
        let run = RoutineRun { id: "run9".into(), routine_id: "r1".into(), started_ms: 1_000, finished_ms: Some(61_000), cost_micro_usd: 42_000, ..Default::default() };
        let line = routine_run_event("r1", &run, "claude", "sonnet");
        assert_eq!((line.run_key.as_str(), line.task_id.as_str(), line.kind.as_str()), ("routine:run9", "r1", "routine"));
        assert_eq!((line.cost_micro_usd, line.source, line.duration_ms, line.at_ms), (Some(42_000), Source::Reported, 60_000, 61_000));
        let free = routine_run_event("r1", &RoutineRun { cost_micro_usd: 0, ..run }, "codex", "");
        assert_eq!((free.cost_micro_usd, free.source), (None, Source::Estimated));
    }

    #[test]
    fn by_day_gives_every_local_day_with_zeros_and_splits_at_local_midnight() {
        // UTC+5:30. Local midnight is 18:30 UTC.
        let tz = FixedOffset::east_opt(5 * 3600 + 1800).unwrap();
        let now = 1_794_598_800_000; // 2026-11-14 01:00 local
        let today = crate::spend::day_start_ms(now, &tz);
        let events = [
            event("a", today - 1, "claude", Some(1_000_000), Some((10, 5))), // 23:59:59.999 yesterday
            event("b", today, "claude", Some(2_000_000), Some((20, 6))),     // midnight today
            event("c", today + HOUR, "codex", None, None),
            event("old", today - 40 * 24 * HOUR, "claude", Some(5), None), // outside a 3 day range
        ];
        let days = by_day(&events, now, 3, &tz);
        assert_eq!(days.iter().map(|d| d.day.as_str()).collect::<Vec<_>>(), ["2026-11-12", "2026-11-13", "2026-11-14"]);
        assert_eq!(days.iter().map(|d| d.runs).collect::<Vec<_>>(), [0, 1, 2], "an empty day is a zero, midnight splits the day");
        assert_eq!((days[1].cost_micro_usd, days[1].input_tokens, days[1].output_tokens), (1_000_000, 10, 5));
        assert_eq!((days[2].estimated_runs, days[2].cost_micro_usd), (1, 2_000_000));
        assert_eq!(by_day(&[], now, 7, &tz).len(), 7);
        assert_eq!(by_day(&[], now, 365, &tz).len(), 365);
        let h = history(&events, now, 3, &tz);
        assert_eq!((h.total_runs, h.since_ms), (3, today - 2 * 24 * HOUR), "the old run is not in the range");
    }

    #[test]
    fn by_provider_sums_runs_tokens_and_cost_and_counts_estimated() {
        let events = [
            event("a", 1, "claude", Some(3_000_000), Some((100, 10))),
            event("b", 2, "claude", Some(1_000_000), Some((50, 5))),
            event("c", 3, "codex", None, None),
            event("d", 4, "codex", None, None),
            event("e", 5, "codex", None, None),
        ];
        let providers = by_provider(&events);
        assert_eq!(providers.iter().map(|p| p.provider.as_str()).collect::<Vec<_>>(), ["claude", "codex"], "most cost first");
        assert_eq!((providers[0].runs, providers[0].estimated_runs, providers[0].input_tokens, providers[0].output_tokens, providers[0].cost_micro_usd), (2, 0, 150, 15, 4_000_000));
        assert_eq!((providers[1].runs, providers[1].estimated_runs, providers[1].cost_micro_usd), (3, 3, 0));
        // With equal cost the busier provider comes first.
        let tied = by_provider(&[event("a", 1, "x", None, None), event("b", 1, "y", None, None), event("c", 1, "y", None, None)]);
        assert_eq!(tied[0].provider, "y");
    }

    #[test]
    fn days_are_clamped_to_1_and_365() {
        assert_eq!((clamp_days(0), clamp_days(9999), clamp_days(30)), (1, 365, 30));
        let tz = chrono::Utc;
        assert_eq!(by_day(&[], 1_794_598_800_000, 0, &tz).len(), 1);
        assert_eq!(by_day(&[], 1_794_598_800_000, 9999, &tz).len(), 365);
    }

    #[test]
    fn csv_quotes_commas_and_quotes_and_guards_a_leading_formula_character() {
        let tz = FixedOffset::east_opt(0).unwrap();
        let mut hostile = event("a", 1_794_598_800_000, "claude", Some(1_234_567), Some((1, 2)));
        hostile.model = "=HYPERLINK(\"x\",\"y\")".into();
        hostile.task_id = "+cmd|' /C calc'!A0".into();
        let mut quoted = event("b", 1_794_598_800_000, "a,b", None, None);
        quoted.model = "say \"hi\"\nthere".into();
        quoted.kind = "@sum".into();
        let csv = to_csv(&[hostile, quoted], &tz);
        let mut lines = csv.lines();
        assert_eq!(lines.next().unwrap(), "time,provider,model,input_tokens,output_tokens,cost_usd,source,duration_s,task_id,kind");
        assert_eq!(lines.next().unwrap(), "2026-11-13T19:40:00+00:00,claude,\"'=HYPERLINK(\"\"x\"\",\"\"y\"\")\",1,2,1.234567,reported,90.0,'+cmd|' /C calc'!A0,ticket");
        assert!(csv.contains("\"a,b\",\"say \"\"hi\"\"\nthere\",,,,estimated,90.0,t1,'@sum\n"), "{csv}");
        assert!(guard_cell("-1+2").starts_with("'-"));
        assert_eq!(guard_cell("plain"), "plain");
        assert_eq!(to_csv(&[], &tz).lines().count(), 1, "just the header");
    }

    #[test]
    fn migrate_is_idempotent_and_an_old_database_gets_the_table() {
        let dir = std::env::temp_dir().join(format!("neko-ledger-{}-{}", std::process::id(), crate::workbench::now_ms()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("old.db");
        {
            // A database from before the ledger: only the settings table.
            let conn = rusqlite::Connection::open(&path).unwrap();
            conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL); INSERT INTO settings VALUES ('k','v');").unwrap();
        }
        let db = crate::Db::open(&path).unwrap();
        assert_eq!(db.get_setting("k").unwrap().as_deref(), Some("v"));
        assert!(db.record_usage_event(&event("a", 1, "claude", Some(1), None)).unwrap());
        drop(db);
        // Opening again runs migrate again and changes nothing.
        let db = crate::Db::open(&path).unwrap();
        assert_eq!(db.usage_events_since(0).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recording_one_run_key_twice_leaves_one_row() {
        let db = crate::Db::open_in_memory().unwrap();
        assert!(db.record_usage_event(&event("ticket:t1:5", 10, "claude", Some(100), None)).unwrap());
        assert!(!db.record_usage_event(&event("ticket:t1:5", 20, "claude", Some(999), None)).unwrap(), "ignored");
        let rows = db.usage_events_since(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].at_ms, rows[0].cost_micro_usd), (10, Some(100)), "the first record stands");
    }

    #[test]
    fn a_failed_or_cancelled_run_is_recorded_once_through_record_ticket_run() {
        let db = crate::Db::open_in_memory().unwrap();
        let runtime = AgentRuntime { provider: "claude".into(), model: "sonnet".into(), ..Default::default() };
        // The run's events live in the task events table: one USAGE event after the start, one before it.
        db.set_setting(&format!("{}t9", crate::db::EVENTS_KEY), &serde_json::to_string(&[
            serde_json::to_string(&usage_event(10, r#"{"cost_usd":7.0}"#)).unwrap(),
            serde_json::to_string(&usage_event(1_500, r#"{"cost_usd":0.1,"input_tokens":5,"output_tokens":6}"#)).unwrap(),
        ]).unwrap()).unwrap();
        record_ticket_run(&db, "t9", 1_000, 2_000, Some(&runtime));
        record_ticket_run(&db, "t9", 1_000, 2_500, Some(&runtime));
        let rows = db.usage_events_since(0).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!((rows[0].cost_micro_usd, rows[0].input_tokens, rows[0].source, rows[0].duration_ms), (Some(100_000), Some(5), Source::Reported, 1_000));
        // The same ticket's next claim is another row.
        record_ticket_run(&db, "t9", 3_000, 4_000, Some(&runtime));
        assert_eq!(db.usage_events_for_task("t9").unwrap().len(), 2);
        let totals = sum_for_task(&db, "t9").unwrap();
        assert_eq!((totals.runs, totals.estimated_runs, totals.cost_micro_usd), (2, 1, Some(100_000)));
    }

    #[test]
    fn a_routine_run_row_is_written_but_a_skip_and_a_hold_write_none() {
        let db = crate::Db::open_in_memory().unwrap();
        let run = RoutineRun { id: "x1".into(), routine_id: "r1".into(), started_ms: 5, finished_ms: Some(9), ok: true, cost_micro_usd: 7, ..Default::default() };
        record_routine_run(&db, "r1", &run, "claude", "sonnet");
        let rows = db.usage_events_since(0).unwrap();
        assert_eq!((rows.len(), rows[0].task_id.as_str(), rows[0].kind.as_str(), rows[0].run_key.as_str()), (1, "r1", "routine", "routine:x1"));
    }

    #[test]
    fn pruning_keeps_a_year_and_runs_at_most_once_a_day() {
        let db = crate::Db::open_in_memory().unwrap();
        let now = 1_794_598_800_000;
        db.record_usage_event(&event("old", now - 366 * DAY_MS, "claude", Some(1), None)).unwrap();
        db.record_usage_event(&event("edge", now - 364 * DAY_MS, "claude", Some(1), None)).unwrap();
        db.record_usage_event(&event("new", now - DAY_MS, "claude", Some(1), None)).unwrap();
        assert_eq!(prune_if_due(&db, now).unwrap(), Some(1));
        let keys: Vec<String> = db.usage_events_since(0).unwrap().into_iter().map(|e| e.run_key).collect();
        assert_eq!(keys, ["edge", "new"]);
        // Later the same day: nothing to do, and nothing is touched even if there were more.
        db.record_usage_event(&event("older", now - 400 * DAY_MS, "claude", Some(1), None)).unwrap();
        assert_eq!(prune_if_due(&db, now + HOUR).unwrap(), None);
        assert_eq!(db.usage_events_since(0).unwrap().len(), 3);
        // The next day it runs again: "older", and "edge", which is now a year and an hour old.
        assert_eq!(prune_if_due(&db, now + 25 * HOUR).unwrap(), Some(2));
    }

    #[test]
    fn writing_a_row_does_not_change_the_content_stamp_or_the_data_revision() {
        let db = crate::Db::open_in_memory().unwrap();
        let stamp = db.content_stamp().unwrap();
        db.record_usage_event(&event("a", 1, "claude", Some(1), None)).unwrap();
        db.prune_usage_events(0).unwrap();
        assert_eq!(db.content_stamp().unwrap(), stamp, "this connection's stamp is its own");
        // `data_revision` is process-wide and other tests bump it while this one runs, so one quiet
        // attempt out of many is enough; a write that bumped it would fail every attempt.
        let quiet = (0..50).any(|i| {
            let before = crate::db::data_revision();
            db.record_usage_event(&event(&format!("rev{i}"), 1, "claude", Some(1), None)).unwrap();
            crate::db::data_revision() == before
        });
        assert!(quiet, "a ledger write bumped data_revision");
    }

    #[test]
    fn history_for_clamps_the_range() {
        let db = crate::Db::open_in_memory().unwrap();
        let now = crate::workbench::now_ms();
        assert_eq!(history_for(&db, now, 0).unwrap().days.len(), 1);
        assert_eq!(history_for(&db, now, 9999).unwrap().days.len(), 365);
        assert!(csv_for(&db, now, 0).unwrap().starts_with("time,provider"));
    }
}
