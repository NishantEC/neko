//! The Setup pages' rules in words, ported from `web/src/setup/lib.ts` and the setup pages: what a
//! connection's status is called, what a watch's last check amounts to, the recurrence rules the
//! Schedules editor writes, the public registry's answer. Pure functions, tested here.

use neko_protocol::decision_context::DecisionAction;
use neko_protocol::mcp_host::{McpConnection, Responsibility, ServerConfig};

use crate::link::SetupData;
use crate::status::StatusKind;

/// What a connection is doing, as the web's `connectionStatus` says it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conn {
    SignIn,
    Attention,
    Available,
    Discovering,
    NoTools,
    Paused,
}

impl Conn {
    pub fn label(self) -> &'static str {
        match self {
            Conn::SignIn => "Sign in needed",
            Conn::Attention => "Needs attention",
            Conn::Available => "Available",
            Conn::Discovering => "Discovering",
            Conn::NoTools => "No tools",
            Conn::Paused => "Paused",
        }
    }

    /// The glyph: connected is a mint check, needs sign-in an amber half circle, an error a coral cross.
    pub fn glyph(self) -> StatusKind {
        match self {
            Conn::Available => StatusKind::Done,
            Conn::SignIn => StatusKind::InProgress,
            Conn::Attention => StatusKind::Failed,
            Conn::Discovering => StatusKind::Todo,
            Conn::NoTools | Conn::Paused => StatusKind::Backlog,
        }
    }

    /// Problems first, then work in progress, then what works, then the rest.
    fn rank(self) -> u8 {
        match self {
            Conn::SignIn | Conn::Attention => 0,
            Conn::Discovering => 1,
            Conn::Available => 2,
            _ => 3,
        }
    }

    pub fn broken(self) -> bool {
        matches!(self, Conn::SignIn | Conn::Attention)
    }
}

pub fn conn_status(c: &McpConnection) -> Conn {
    if !c.enabled {
        return Conn::Paused;
    }
    let error = c.error.as_deref().unwrap_or("").to_lowercase();
    if error.contains("auth") || error.contains("oauth") || error.contains("sign-in") || error.contains("401") {
        return Conn::SignIn;
    }
    if !error.is_empty() {
        return Conn::Attention;
    }
    if c.tools.is_empty() {
        return if c.discovered_ms.is_none() { Conn::Discovering } else { Conn::NoTools };
    }
    Conn::Available
}

/// One line under a connection's name: why it is broken, or where it runs.
pub fn conn_detail(c: &McpConnection) -> String {
    let error = c.error.clone().unwrap_or_default();
    let lower = error.to_lowercase();
    if lower.contains("definition changed") {
        return "Its definition changed since you approved it. Review it to use it again.".to_owned();
    }
    if lower.contains("definition is unavailable") {
        return "No longer found where it was imported from. Pause it, or add it again.".to_owned();
    }
    if lower.contains("auth") || lower.contains("401") || lower.contains("sign-in") {
        return "Signed out. Open to sign in again.".to_owned();
    }
    if !error.is_empty() {
        return error.split(['.', '\n']).next().filter(|l| !l.is_empty()).unwrap_or(&error).to_owned();
    }
    match &c.config {
        ServerConfig::Http { url } => match host_of(url) {
            Some(host) if host == "127.0.0.1" || host == "localhost" => "A server on this Mac".to_owned(),
            Some(host) => host,
            None => "Remote server".to_owned(),
        },
        ServerConfig::Stdio { command, args, .. } => {
            let program = args.last().map(String::as_str).filter(|a| !a.is_empty()).unwrap_or(command);
            format!("Runs on this Mac · {}", program.rsplit('/').next().unwrap_or(program))
        }
    }
}

pub fn host_of(url: &str) -> Option<String> {
    url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_owned))
}

pub fn tool_summary(c: &McpConnection) -> String {
    match c.tools.len() {
        0 if !c.enabled || c.error.is_some() => "No tools".to_owned(),
        0 if c.discovered_ms.is_none() => "Discovering".to_owned(),
        0 => "No tools".to_owned(),
        1 => "1 tool".to_owned(),
        n => format!("{n} tools"),
    }
}

pub fn sort_connections(list: &[McpConnection]) -> Vec<&McpConnection> {
    let mut out: Vec<&McpConnection> = list.iter().collect();
    out.sort_by(|a, b| conn_status(a).rank().cmp(&conn_status(b).rank()).then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase())));
    out
}

/// What "reconnect" means for a connection: sign in again, approve a changed source definition, or discover its tools again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reconnect {
    SignIn,
    Relink,
    Discover,
}

impl Reconnect {
    pub fn label(self) -> &'static str {
        match self {
            Reconnect::SignIn => "Sign in again",
            Reconnect::Relink => "Review & approve",
            Reconnect::Discover => "Reconnect",
        }
    }
}

/// None when nothing the button could do would help (paused, or the source definition is gone).
pub fn reconnect_action(c: &McpConnection) -> Option<Reconnect> {
    if !c.enabled {
        return None;
    }
    let status = conn_status(c);
    if !status.broken() {
        return None;
    }
    let error = c.error.as_deref().unwrap_or("").to_lowercase();
    if error.contains("definition is unavailable") {
        return None;
    }
    if error.contains("definition changed") && c.source_link.is_some() {
        return Some(Reconnect::Relink);
    }
    if matches!(c.config, ServerConfig::Http { .. }) && status == Conn::SignIn {
        return Some(Reconnect::SignIn);
    }
    Some(Reconnect::Discover)
}

/// Names of a watch's connections that cannot be used yet.
pub fn ungranted(data: &SetupData, workspace_id: &str, connection_ids: &[String]) -> Vec<String> {
    let usable = |c: &McpConnection| c.enabled && c.trusted && c.error.is_none() && (c.workspace_id.is_empty() || c.workspace_id == workspace_id) && !c.tools.is_empty();
    connection_ids.iter().filter(|id| !data.connection(id).is_some_and(usable)).map(|id| data.connection(id).map_or("Removed tool".to_owned(), |c| c.label.clone())).collect()
}

pub fn connection_label(data: &SetupData, id: &str) -> String {
    data.connection(id).map_or("Removed tool".to_owned(), |c| c.label.clone())
}

/// A watch's short name, or its instruction until Neko has named it.
pub fn watch_title(w: &Responsibility) -> String {
    match w.name.as_deref().map(str::trim) {
        Some(name) if !name.is_empty() => name.to_owned(),
        _ => w.instruction.clone(),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Success,
    Warning,
}

/// What a watch's last check amounts to, in words.
pub fn watch_status(w: &Responsibility, now_ms: i64) -> (String, Tone) {
    if !w.enabled {
        return (if w.failures > 0 { "Paused · last check failed" } else { "Paused" }.to_owned(), Tone::Neutral);
    }
    if w.failures > 0 {
        return ("Needs attention".to_owned(), Tone::Warning);
    }
    match w.last_attempt_ms {
        None => ("First check queued".to_owned(), Tone::Neutral),
        Some(at) => (format!("Checked {}", relative_time(Some(at), now_ms)), Tone::Success),
    }
}

/// The glyph a watch wears: the web maps success to the half circle, a warning to failed, and the rest to the empty circle.
pub fn watch_glyph(tone: Tone) -> StatusKind {
    match tone {
        Tone::Success => StatusKind::InProgress,
        Tone::Warning => StatusKind::Failed,
        Tone::Neutral => StatusKind::Todo,
    }
}

pub const WATCH_SUGGEST_PROMPT: &str = "Look at the tools connected to this workspace and suggest up to three things you should keep an eye on for me. Only suggest what those tools can actually check.";

/// "3 minutes ago", "in 2 hours", "yesterday", "Never" (the web's `relativeTime`, in the same words).
pub fn relative_time(ms: Option<i64>, now_ms: i64) -> String {
    let Some(ms) = ms.filter(|m| *m > 0) else { return "Never".to_owned() };
    let diff = ms - now_ms;
    let abs = diff.abs();
    if abs < 45_000 {
        return "just now".to_owned();
    }
    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;
    const DAY: i64 = 24 * HOUR;
    let (n, unit) = if abs >= 365 * DAY {
        ((abs as f64 / (365 * DAY) as f64).round() as i64, "year")
    } else if abs >= 30 * DAY {
        ((abs as f64 / (30 * DAY) as f64).round() as i64, "month")
    } else if abs >= DAY {
        ((abs as f64 / DAY as f64).round() as i64, "day")
    } else if abs >= HOUR {
        ((abs as f64 / HOUR as f64).round() as i64, "hour")
    } else {
        ((abs as f64 / MIN as f64).round() as i64, "minute")
    };
    if n == 1 && unit == "day" {
        return if diff < 0 { "yesterday" } else { "tomorrow" }.to_owned();
    }
    let plural = if n == 1 { unit.to_owned() } else { format!("{unit}s") };
    if diff < 0 { format!("{n} {plural} ago") } else { format!("in {n} {plural}") }
}

// ---- Schedules: the same recurrence rules the web's editor writes.

pub const WEEKDAYS: [(&str, &str); 7] = [("MO", "Monday"), ("TU", "Tuesday"), ("WE", "Wednesday"), ("TH", "Thursday"), ("FR", "Friday"), ("SA", "Saturday"), ("SU", "Sunday")];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recurrence {
    Daily,
    Weekdays,
    Weekly,
    Hourly,
    Custom,
}

impl Recurrence {
    pub const ALL: [Recurrence; 5] = [Recurrence::Daily, Recurrence::Weekdays, Recurrence::Weekly, Recurrence::Hourly, Recurrence::Custom];

    pub fn label(self) -> &'static str {
        match self {
            Recurrence::Daily => "Daily",
            Recurrence::Weekdays => "Weekdays",
            Recurrence::Weekly => "Weekly",
            Recurrence::Hourly => "Every N hours",
            Recurrence::Custom => "Custom",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleParts {
    pub kind: Recurrence,
    pub hour: u32,
    pub minute: u32,
    pub weekday: String,
    pub interval: u32,
}

pub fn rule_for(kind: Recurrence, hour: u32, minute: u32, weekday: &str, interval: u32, custom: &str) -> String {
    match kind {
        Recurrence::Daily => format!("FREQ=DAILY;BYHOUR={hour};BYMINUTE={minute}"),
        Recurrence::Weekdays => format!("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR={hour};BYMINUTE={minute}"),
        Recurrence::Weekly => format!("FREQ=WEEKLY;BYDAY={weekday};BYHOUR={hour};BYMINUTE={minute}"),
        Recurrence::Hourly => format!("FREQ=HOURLY;INTERVAL={interval}"),
        Recurrence::Custom => custom.to_owned(),
    }
}

/// Parses a rule this editor wrote back into its pieces; anything else is "custom".
pub fn parse_rule(rule: &str) -> RuleParts {
    let base = RuleParts { kind: Recurrence::Custom, hour: 9, minute: 0, weekday: "MO".to_owned(), interval: 4 };
    let pairs: Vec<Vec<&str>> = rule.split(';').map(|p| p.split('=').collect()).collect();
    if !pairs.iter().all(|p| p.len() == 2) {
        return base;
    }
    let mut keys: Vec<&str> = pairs.iter().map(|p| p[0]).collect();
    keys.sort_unstable();
    keys.dedup();
    if keys.len() != pairs.len() {
        return base;
    }
    let get = |k: &str| pairs.iter().find(|p| p[0] == k).map(|p| p[1]);
    let n = pairs.len();
    if get("FREQ") == Some("HOURLY") && n == 2 && get("INTERVAL").is_some_and(|i| !i.is_empty() && i.chars().all(|c| c.is_ascii_digit())) {
        return RuleParts { kind: Recurrence::Hourly, interval: get("INTERVAL").and_then(|i| i.parse().ok()).unwrap_or(4), ..base };
    }
    let (Some(hour), Some(minute)) = (get("BYHOUR").and_then(|h| h.parse::<u32>().ok()), get("BYMINUTE").and_then(|m| m.parse::<u32>().ok())) else { return base };
    if hour > 23 || minute > 59 {
        return base;
    }
    if get("FREQ") == Some("DAILY") && n == 3 {
        return RuleParts { kind: Recurrence::Daily, hour, minute, ..base };
    }
    if get("FREQ") == Some("WEEKLY") && n == 4 {
        let day = get("BYDAY").unwrap_or("");
        if day == "MO,TU,WE,TH,FR" {
            return RuleParts { kind: Recurrence::Weekdays, hour, minute, ..base };
        }
        if WEEKDAYS.iter().any(|(code, _)| *code == day) {
            return RuleParts { kind: Recurrence::Weekly, hour, minute, weekday: day.to_owned(), ..base };
        }
    }
    base
}

pub fn rule_summary(rule: &str) -> String {
    let p = parse_rule(rule);
    let time = format!("{:02}:{:02}", p.hour, p.minute);
    match p.kind {
        Recurrence::Hourly => format!("Every {} hours", p.interval),
        Recurrence::Daily => format!("Daily at {time}"),
        Recurrence::Weekdays => format!("Weekdays at {time}"),
        Recurrence::Weekly => format!("Every {} at {time}", WEEKDAYS.iter().find(|(c, _)| *c == p.weekday).map_or("week", |(_, n)| n)),
        Recurrence::Custom => "Custom schedule".to_owned(),
    }
}

/// "09:30" as (hour, minute); None when it is not a time.
pub fn parse_time(text: &str) -> Option<(u32, u32)> {
    let (h, m) = text.trim().split_once(':')?;
    let (h, m) = (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

// ---- Adding a connection

/// The manual form's choices.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConnectionForm {
    pub local: bool,
    pub trust: bool,
    pub target: String,
    pub args: String,
    pub cwd: String,
}

/// The daemon's connection config, or what to fix first (the web's `configFor`).
pub fn config_for(form: &ConnectionForm) -> Result<ServerConfig, String> {
    let destination = form.target.trim();
    if form.local {
        if !destination.starts_with('/') {
            return Err("Use an absolute executable path.".to_owned());
        }
        let raw = if form.args.trim().is_empty() { "[]" } else { form.args.trim() };
        let args: Vec<String> = serde_json::from_str(raw).map_err(|_| "Arguments must be a JSON array of strings.".to_owned())?;
        let cwd = form.cwd.trim();
        if !cwd.is_empty() && !cwd.starts_with('/') {
            return Err("Choose an existing absolute working directory.".to_owned());
        }
        return Ok(ServerConfig::Stdio { command: destination.to_owned(), args, cwd: (!cwd.is_empty()).then(|| cwd.to_owned()) });
    }
    match url::Url::parse(destination) {
        Ok(u) if matches!(u.scheme(), "http" | "https") && u.host_str().is_some() => Ok(ServerConfig::Http { url: destination.to_owned() }),
        _ => Err("Enter a complete HTTP or HTTPS server URL.".to_owned()),
    }
}

/// One server of the public MCP registry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RegistryServer {
    pub name: String,
    pub short_name: String,
    pub description: String,
    pub url: String,
}

pub const REGISTRY: &str = "https://registry.modelcontextprotocol.io/v0.1/servers";

/// The featured server the web lists first.
pub fn featured() -> RegistryServer {
    RegistryServer { name: "io.github.getsentry/sentry-mcp".to_owned(), short_name: "sentry-mcp".to_owned(), description: "Investigate errors, traces, and issues in Sentry.".to_owned(), url: "https://mcp.sentry.dev/mcp".to_owned() }
}

/// The servers with a hosted streamable-HTTP address (the web's `parseRegistry`), without the featured one.
pub fn parse_registry(body: &str) -> Vec<RegistryServer> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(body) else { return Vec::new() };
    let featured_url = featured().url;
    json["servers"]
        .as_array()
        .map(|list| {
            list.iter()
                .filter_map(|entry| {
                    let server = &entry["server"];
                    let name = server["name"].as_str().filter(|n| !n.is_empty())?;
                    let remote = server["remotes"].as_array()?.iter().find(|r| r["type"] == "streamable-http" && r["url"].as_str().is_some_and(|u| u.starts_with("https://")))?;
                    let url = remote["url"].as_str()?.to_owned();
                    Some(RegistryServer { name: name.to_owned(), short_name: name.rsplit('/').next().unwrap_or(name).to_owned(), description: server["description"].as_str().unwrap_or("").to_owned(), url })
                })
                .filter(|r| r.url != featured_url)
                .collect()
        })
        .unwrap_or_default()
}

// ---- Skills, memory and guidance

pub fn curator_words(state: &str) -> &'static str {
    match state {
        "neko" => "Neko wrote this skill. Its daily curator tidies accumulated lessons by itself and keeps what it removes in an Archive section.",
        "locked" => "Locked: the curator only proposes changes to this skill, and you accept them.",
        "edited" => "You edited this by hand, so it is locked: the curator only proposes changes, and you accept them.",
        _ => "Not written by Neko: the curator only proposes changes, and you accept them.",
    }
}

pub fn kind_label(kind: neko_protocol::workbench::MemoryKind) -> &'static str {
    use neko_protocol::workbench::MemoryKind;
    match kind {
        MemoryKind::Profile => "Profile",
        MemoryKind::Workspace => "Workspace",
        MemoryKind::Decision => "Decision",
    }
}

pub fn memory_source(source: &str) -> &'static str {
    if source == "user" {
        "Added by you"
    } else if source == "chat" || source.starts_with("chat:") {
        "From a Home conversation"
    } else if source.starts_with("ticket:") {
        "From an agent conversation"
    } else {
        "Source details"
    }
}

pub fn decision_title(action: DecisionAction) -> &'static str {
    match action {
        DecisionAction::PrepareFix => "Prepare a local fix",
        DecisionAction::AskUser => "Ask for your decision",
        DecisionAction::Skip => "Defer this work",
        DecisionAction::ApproveLocalBuild => "You approved a local build",
        DecisionAction::AcceptLocalResult => "You accepted the local result",
        DecisionAction::ReportFailure => "Work stopped",
        DecisionAction::ReviewLocalResult => "Local work is ready to review",
        DecisionAction::AddNote => "Your direction was recorded",
        DecisionAction::ProposePlan => "Prepare a plan",
        // The only remaining one is the person stopping the task.
        _ => "You stopped this task",
    }
}

/// Phrases from a comma-separated field.
pub fn phrases(text: &str) -> Vec<String> {
    text.split(',').map(str::trim).filter(|p| !p.is_empty()).map(str::to_owned).collect()
}

/// A diff line's kind, for colouring a skill proposal's patch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffLine {
    Added,
    Removed,
    Hunk,
    Context,
}

pub fn diff_line(line: &str) -> DiffLine {
    if line.starts_with("+++") || line.starts_with("---") || line.starts_with("@@") {
        DiffLine::Hunk
    } else if line.starts_with('+') {
        DiffLine::Added
    } else if line.starts_with('-') {
        DiffLine::Removed
    } else {
        DiffLine::Context
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::mcp_host::McpTool;

    fn conn(enabled: bool, error: Option<&str>, tools: usize, discovered: Option<i64>) -> McpConnection {
        McpConnection {
            oauth: false,
            id: "c".into(),
            workspace_id: String::new(),
            label: "Sentry".into(),
            config: ServerConfig::Http { url: "https://mcp.example.com/mcp".into() },
            enabled,
            trusted: true,
            has_credentials: false,
            tools: (0..tools).map(|i| McpTool { read_only: true, name: format!("t{i}"), description: String::new(), input_schema: String::new(), schema_hash: "h".into() }).collect(),
            discovered_ms: discovered,
            error: error.map(str::to_owned),
            source_link: None,
        }
    }

    #[test]
    fn a_connections_status_is_what_the_web_says() {
        assert_eq!(conn_status(&conn(false, None, 3, Some(1))), Conn::Paused);
        assert_eq!(conn_status(&conn(true, Some("OAuth sign-in required"), 0, None)), Conn::SignIn);
        assert_eq!(conn_status(&conn(true, Some("spawn failed"), 0, None)), Conn::Attention);
        assert_eq!(conn_status(&conn(true, None, 0, None)), Conn::Discovering);
        assert_eq!(conn_status(&conn(true, None, 0, Some(1))), Conn::NoTools);
        assert_eq!(conn_status(&conn(true, None, 2, Some(1))), Conn::Available);
        assert_eq!((Conn::Available.glyph(), Conn::SignIn.glyph(), Conn::Attention.glyph()), (StatusKind::Done, StatusKind::InProgress, StatusKind::Failed));
    }

    #[test]
    fn problems_sort_first_then_by_name() {
        let mut broken = conn(true, Some("401"), 0, None);
        broken.label = "Zed".into();
        let ok = conn(true, None, 1, Some(1));
        let list = [ok.clone(), broken.clone()];
        let sorted = sort_connections(&list);
        assert_eq!(sorted[0].label, "Zed");
        assert_eq!(sorted[1].label, "Sentry");
    }

    #[test]
    fn reconnect_means_sign_in_for_a_signed_out_remote_server_and_nothing_when_paused() {
        assert_eq!(reconnect_action(&conn(true, Some("401 unauthorized"), 0, None)), Some(Reconnect::SignIn));
        assert_eq!(reconnect_action(&conn(true, Some("timed out"), 0, None)), Some(Reconnect::Discover));
        assert_eq!(reconnect_action(&conn(false, Some("timed out"), 0, None)), None);
        assert_eq!(reconnect_action(&conn(true, None, 2, Some(1))), None);
        assert_eq!(reconnect_action(&conn(true, Some("definition is unavailable"), 0, None)), None);
    }

    #[test]
    fn a_watch_says_what_its_last_check_amounts_to() {
        let mut w = Responsibility { id: "r".into(), workspace_id: "w".into(), instruction: "New issues".into(), connection_ids: vec![], enabled: true, prepare_low_risk: false, next_due_ms: 0, last_attempt_ms: None, last_result: String::new(), failures: 0, name: None, name_by_user: false };
        assert_eq!(watch_status(&w, 1_000_000).0, "First check queued");
        w.last_attempt_ms = Some(1_000_000 - 5 * 60_000);
        assert_eq!(watch_status(&w, 1_000_000), ("Checked 5 minutes ago".to_owned(), Tone::Success));
        w.failures = 2;
        assert_eq!(watch_status(&w, 1_000_000), ("Needs attention".to_owned(), Tone::Warning));
        w.enabled = false;
        assert_eq!(watch_status(&w, 1_000_000).0, "Paused · last check failed");
        assert_eq!(watch_title(&w), "New issues");
        w.name = Some("Sentry errors".into());
        assert_eq!(watch_title(&w), "Sentry errors");
    }

    #[test]
    fn relative_times_read_like_the_webs() {
        let now = 10 * 86_400_000;
        assert_eq!(relative_time(None, now), "Never");
        assert_eq!(relative_time(Some(now - 10_000), now), "just now");
        assert_eq!(relative_time(Some(now - 3_600_000), now), "1 hour ago");
        assert_eq!(relative_time(Some(now + 2 * 3_600_000), now), "in 2 hours");
        assert_eq!(relative_time(Some(now - 86_400_000), now), "yesterday");
        assert_eq!(relative_time(Some(now - 3 * 86_400_000), now), "3 days ago");
    }

    #[test]
    fn recurrence_rules_round_trip_and_anything_else_is_custom() {
        for (kind, rule) in [
            (Recurrence::Daily, "FREQ=DAILY;BYHOUR=9;BYMINUTE=30"),
            (Recurrence::Weekdays, "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR;BYHOUR=8;BYMINUTE=0"),
            (Recurrence::Weekly, "FREQ=WEEKLY;BYDAY=FR;BYHOUR=16;BYMINUTE=15"),
            (Recurrence::Hourly, "FREQ=HOURLY;INTERVAL=6"),
        ] {
            let p = parse_rule(rule);
            assert_eq!(p.kind, kind, "{rule}");
            assert_eq!(rule_for(p.kind, p.hour, p.minute, &p.weekday, p.interval, ""), rule);
        }
        assert_eq!(parse_rule("FREQ=MONTHLY;BYMONTHDAY=1").kind, Recurrence::Custom);
        assert_eq!(parse_rule("FREQ=DAILY;BYHOUR=99;BYMINUTE=0").kind, Recurrence::Custom);
        assert_eq!(rule_summary("FREQ=WEEKLY;BYDAY=FR;BYHOUR=16;BYMINUTE=15"), "Every Friday at 16:15");
        assert_eq!(rule_summary("FREQ=HOURLY;INTERVAL=6"), "Every 6 hours");
        assert_eq!(parse_time("09:05"), Some((9, 5)));
        assert_eq!(parse_time("24:00"), None);
    }

    #[test]
    fn a_manual_connection_must_be_a_url_or_an_absolute_path_with_string_arguments() {
        let remote = |t: &str| ConnectionForm { target: t.into(), ..ConnectionForm::default() };
        assert!(matches!(config_for(&remote("https://example.com/mcp")), Ok(ServerConfig::Http { .. })));
        assert!(config_for(&remote("example.com")).is_err() && config_for(&remote("ftp://x.com/a")).is_err());
        let local = |t: &str, a: &str| ConnectionForm { local: true, target: t.into(), args: a.into(), ..ConnectionForm::default() };
        assert!(matches!(config_for(&local("/usr/bin/mcp", "[\"--x\"]")), Ok(ServerConfig::Stdio { ref args, .. }) if args == &["--x"]));
        assert!(config_for(&local("mcp", "[]")).is_err());
        assert!(config_for(&local("/usr/bin/mcp", "[1]")).is_err() && config_for(&local("/usr/bin/mcp", "nope")).is_err());
        assert!(config_for(&ConnectionForm { cwd: "rel".into(), ..local("/usr/bin/mcp", "") }).is_err());
    }

    #[test]
    fn the_registry_answer_keeps_hosted_servers_and_drops_the_featured_one() {
        let body = r#"{"servers":[
            {"server":{"name":"io.github.acme/acme-mcp","description":"Acme","remotes":[{"type":"streamable-http","url":"https://mcp.acme.com/mcp"}]}},
            {"server":{"name":"io.github.local/stdio-only","remotes":[]}},
            {"server":{"name":"io.github.getsentry/sentry-mcp","remotes":[{"type":"streamable-http","url":"https://mcp.sentry.dev/mcp"}]}},
            {"server":{"name":"io.github.http/insecure","remotes":[{"type":"streamable-http","url":"http://x.com/mcp"}]}}]}"#;
        let found = parse_registry(body);
        assert_eq!(found.len(), 1);
        assert_eq!((found[0].short_name.as_str(), found[0].url.as_str()), ("acme-mcp", "https://mcp.acme.com/mcp"));
        assert!(parse_registry("not json").is_empty());
    }

    #[test]
    fn diffs_colour_by_their_first_character() {
        assert_eq!(diff_line("+new"), DiffLine::Added);
        assert_eq!(diff_line("-old"), DiffLine::Removed);
        assert_eq!(diff_line("@@ -1 +1 @@"), DiffLine::Hunk);
        assert_eq!(diff_line("--- a/x"), DiffLine::Hunk);
        assert_eq!(diff_line(" same"), DiffLine::Context);
        assert_eq!(phrases(" a, b ,, c"), ["a", "b", "c"]);
    }
}
