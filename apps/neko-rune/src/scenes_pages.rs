//! Made-up data and named states for the full views' snapshots (`--scene routines`, ...): routines
//! with runs, a week of meetings and Neko's dated work, the phone settings, quota meters, plugins.
//! Nothing here names anyone or anything real (the repository is public).

use std::collections::BTreeMap;

use neko_protocol::agent_models::{CatalogModel, ModelAccess, ModelCatalog, ModelSource, RuntimeOption, SourceStatus};
use neko_protocol::calendar::{CalendarAccess, CalendarEvent, CalendarStatus, NekoCalendarItem, NekoItemKind};
use neko_protocol::phone::{PhoneSettings, PhoneStatus};
use neko_protocol::routines::{Cadence, Hold, NeedsYou, Routine, RoutineKind, RoutineRun, RoutineState, RoutineView, ScopeProposal, ScopeServer, SpendToday};
use neko_protocol::workbench::{AgentRuntime, SelfUpdate, UpdateCheck};
use neko_protocol::PluginHealth;

use crate::app::{App, DEMO_NOW};
use crate::clock;
use crate::link::{CalendarData, PhoneData, QuotaRow, RoutinesData, SettingsView, WorkspaceInfo};
use crate::pages::CalMode;
use crate::views::FullView;

const MIN: i64 = 60_000;
const HOUR: i64 = 3_600_000;

fn routine(id: &str, kind: RoutineKind, name: &str, instruction: &str, cadence: Cadence, enabled: bool) -> Routine {
    Routine { id: id.into(), workspace_id: "w1".into(), kind, name: name.into(), instruction: instruction.into(), cadence, enabled, model: String::new(), paseo_schedule: None, browser: true, computer: false, customized: false, catch_up: None }
}

fn run(id: &str, routine_id: &str, ago_min: i64, ok: bool, did: &[&str], needs: usize, error: Option<&str>) -> RoutineRun {
    RoutineRun {
        id: id.into(),
        routine_id: routine_id.into(),
        started_ms: DEMO_NOW - ago_min * MIN,
        finished_ms: Some(DEMO_NOW - ago_min * MIN + 40_000),
        ok,
        quiet: did.is_empty() && ok,
        did: did.iter().map(|d| (*d).to_owned()).collect(),
        needs_you: (0..needs).map(|i| NeedsYou { seq: 100 + i as u64, title: "Reply before the call".into(), ..NeedsYou::default() }).collect(),
        error: error.map(str::to_owned),
        ..RoutineRun::default()
    }
}

pub fn routines_data() -> RoutinesData {
    let messages = routine("r1", RoutineKind::Messages, "Messages", "Read the team chat and my mail. Answer what is simple, and tell me what only I can answer.", Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 }, true);
    let digest = routine("r2", RoutineKind::Digest, "Morning digest", "Summarise what finished, what is stuck and what needs a decision.", Cadence::Daily { hour: 8, minute: 30 }, true);
    let sweep = routine("r3", RoutineKind::PrSweep, "Pull requests", "Check my open pull requests against what the host reports now.", Cadence::Every { minutes: 60, from_hour: 9, to_hour: 19 }, true);
    let care = routine("r4", RoutineKind::MacCare, "Mac care", "Keep the disk healthy.", Cadence::Daily { hour: 22, minute: 0 }, false);
    let weekly = routine("r5", RoutineKind::Custom, "Weekly review", "Look back over the week's work and propose three priorities.", Cadence::Daily { hour: 17, minute: 0 }, true);
    let views = vec![
        RoutineView {
            routine: messages,
            state: RoutineState { last_run_ms: Some(DEMO_NOW - 14 * MIN), next_due_ms: Some(DEMO_NOW + 6 * MIN), ..RoutineState::default() },
            runs: vec![run("x1", "r1", 14, true, &["Replied to the budget thread", "Archived 6 newsletters"], 1, None), run("x2", "r1", 34, true, &[], 0, None), run("x3", "r1", 54, true, &["Declined a duplicate invite"], 0, None)],
        },
        RoutineView { routine: digest, state: RoutineState { last_run_ms: Some(DEMO_NOW - 5 * HOUR), next_due_ms: Some(DEMO_NOW + 19 * HOUR), ..RoutineState::default() }, runs: vec![run("x4", "r2", 300, true, &["Wrote today's digest"], 0, None)] },
        RoutineView { routine: sweep, state: RoutineState { running: true, ..RoutineState::default() }, runs: vec![run("x5", "r3", 62, true, &["Two pull requests are ready"], 0, None)] },
        RoutineView { routine: care, state: RoutineState::default(), runs: vec![] },
        RoutineView {
            routine: weekly,
            state: RoutineState { hold: Some(Hold { until_ms: DEMO_NOW + 3 * HOUR, reason: "usage limit".into() }), next_due_ms: Some(DEMO_NOW + 3 * HOUR), ..RoutineState::default() },
            runs: vec![run("x6", "r5", 1500, false, &[], 0, Some("The agent stopped: usage limit reached"))],
        },
    ];
    let scopes = vec![
        ScopeProposal {
            name: "Neko".into(),
            folders: vec!["/work/neko".into(), "/work/docs-site".into()],
            servers: vec![ScopeServer { name: "chat".into(), origin: "/work/neko/.mcp.json".into(), level: "scope".into(), repo: None }, ScopeServer { name: "tickets".into(), origin: "/work/neko/.mcp.json".into(), level: "scope".into(), repo: None }, ScopeServer { name: "calendar".into(), origin: "/work/.mcp.json".into(), level: "repo".into(), repo: None }],
            problems: vec![],
            workspace_id: Some("w1".into()),
            discovered: true,
            instructions: String::new(),
        },
        ScopeProposal { name: "Home".into(), folders: vec!["/home/me".into()], servers: vec![], problems: vec!["One server has no command".into()], workspace_id: Some("w2".into()), discovered: true, instructions: String::new() },
    ];
    let mut orders = BTreeMap::new();
    orders.insert("w1".to_owned(), "Never send anything to a customer. Keep replies under three lines.".to_owned());
    RoutinesData { views, standing_orders: orders, scopes, owner_name: "Sam".into(), age_out_days: 7, routines_on: true, spend_today: SpendToday { tasks_usd: 1.84, routines_usd: 0.62, ceiling_cents: Some(500), held: false } }
}

fn local_at(day_start: i64, hour: i64, minute: i64) -> i64 {
    day_start + hour * HOUR + minute * MIN
}

pub fn calendar_data(now_ms: i64, from_ms: i64, to_ms: i64) -> CalendarData {
    let monday = clock::week_start(now_ms);
    let day = |n: i64| clock::add_days(monday, n);
    let ev = |id: &str, title: &str, d: i64, h: i64, m: i64, mins: i64, calendar: &str| CalendarEvent {
        id: id.into(),
        calendar: calendar.into(),
        title: title.into(),
        start_ms: local_at(day(d), h, m),
        end_ms: local_at(day(d), h, m) + mins * MIN,
        ..CalendarEvent::default()
    };
    let events = vec![
        ev("e1", "Planning sync", 0, 10, 0, 60, "Work"),
        ev("e2", "Design review", 1, 14, 0, 90, "Work"),
        ev("e3", "Standup", 2, 9, 30, 30, "Work"),
        ev("e4", "Budget call", 2, 15, 0, 60, "Work"),
        ev("e5", "Customer interview", 2, 15, 30, 45, "Work"),
        ev("e6", "Standup", 3, 9, 30, 30, "Work"),
        ev("e7", "Dentist", 3, 17, 0, 60, "Family"),
        ev("e8", "Planning sync", 4, 10, 0, 60, "Work"),
        ev("e9", "Team lunch", 4, 12, 30, 90, "Work"),
        CalendarEvent { id: "e10".into(), calendar: "Family".into(), title: "Public holiday".into(), start_ms: day(5), end_ms: day(6), all_day: true, ..CalendarEvent::default() },
    ];
    let task = |id: &str, title: &str, d: i64, status: &str, detail: &str| NekoCalendarItem { id: id.into(), kind: NekoItemKind::Task, title: title.into(), at_ms: day(d), all_day: true, status: status.into(), detail: detail.into(), route: format!("tasks?task={id}") };
    let neko = vec![
        task("t6", "Fix the empty state on the board", 0, "Completed", "Done"),
        task("t7", "Rename the export command", 2, "ReadyForReview", "Ready for review"),
        task("t8", "Cache the repository list", 2, "Completed", "Done"),
        task("t9", "Add a retry to the sync job", 3, "Failed", "Failed"),
        NekoCalendarItem { id: "r1".into(), kind: NekoItemKind::Routine, title: "Weekly review".into(), at_ms: local_at(day(4), 17, 0), all_day: false, status: String::new(), detail: "Daily at 17:00".into(), route: "inbox".into() },
    ];
    CalendarData { events, neko, status: CalendarStatus { access: CalendarAccess::Granted, source: "macOS Calendar".into(), last_sync_ms: Some(now_ms - 3 * MIN), events: 10, truncated: false, enabled: true }, from_ms, to_ms }
}

pub fn phone_data() -> PhoneData {
    PhoneData { settings: PhoneSettings { reminders: true, ntfy: false, ..PhoneSettings::default() }, status: PhoneStatus { last_push_ms: Some(DEMO_NOW - 50 * MIN), last_channel: "Reminders".into(), ..PhoneStatus::default() } }
}

pub fn quota_rows() -> Vec<QuotaRow> {
    vec![
        QuotaRow { label: "Claude Code · 5-hour window".into(), used: 0.62, words: vec!["38% left".into(), "resets 14:00".into()], value: "38%".into(), unit: "left".into(), detail: "resets 14:00".into() },
        QuotaRow { label: "Codex · weekly".into(), used: 0.84, words: vec!["16% left".into(), "resets Fri".into()], value: "16%".into(), unit: "left".into(), detail: "84% used · resets Fri".into() },
        QuotaRow { label: "Grok · monthly".into(), used: 0.97, words: vec!["3% left".into(), "resets 1 Dec".into()], value: "3%".into(), unit: "left".into(), detail: "97% used · resets 1 Dec".into() },
    ]
}

pub fn plugin_list() -> Vec<PluginHealth> {
    vec![
        PluginHealth { id: "board-viewer".into(), version: 4, ok: true, problem: None, rolled_back: false, at_ms: DEMO_NOW - 10 * MIN },
        PluginHealth { id: "release-notes".into(), version: 2, ok: false, problem: Some("a script threw on load".into()), rolled_back: false, at_ms: DEMO_NOW - 2 * HOUR },
        PluginHealth { id: "calendar-sync".into(), version: 7, ok: true, problem: None, rolled_back: true, at_ms: DEMO_NOW - 9 * HOUR },
    ]
}

pub(crate) fn catalog() -> ModelCatalog {
    let model = |id: &str, label: &str, efforts: &[(&str, &str)], speeds: &[(&str, &str)]| CatalogModel {
        id: id.into(),
        label: label.into(),
        description: None,
        recommended: false,
        access: ModelAccess::Listed,
        reason: None,
        reasoning_efforts: efforts.iter().map(|(i, _)| (*i).to_owned()).collect(),
        effort_options: Some(efforts.iter().map(|(i, l)| RuntimeOption { id: (*i).into(), label: (*l).into(), description: None }).collect()),
        default_effort: None,
        speed_options: Some(speeds.iter().map(|(i, l)| RuntimeOption { id: (*i).into(), label: (*l).into(), description: None }).collect()),
        default_speed: None,
    };
    ModelCatalog {
        sources: vec![
            ModelSource { provider: "codex".into(), label: "Codex".into(), connection: "Signed in".into(), status: SourceStatus::Ready, note: None, default_model: None, models: vec![model("gpt-a", "GPT A", &[("low", "Low"), ("medium", "Medium"), ("high", "High")], &[("fast", "Fast")]), model("gpt-b", "GPT B mini", &[], &[])] },
            ModelSource { provider: "claude".into(), label: "Claude Code".into(), connection: "Signed in".into(), status: SourceStatus::Ready, note: None, default_model: None, models: vec![model("sonnet", "Sonnet", &[("high", "High")], &[]), model("opus", "Opus", &[("high", "High")], &[])] },
        ],
        read_at_ms: DEMO_NOW,
    }
}

/// The settings a made-up workbench has.
pub fn settings_view() -> SettingsView {
    let day = 86_400_000;
    SettingsView {
        runtime: AgentRuntime { provider: "codex".into(), model: "gpt-a".into(), reasoning_effort: Some("high".into()), service_tier: None },
        task_budget_cents: Some(300),
        daily_budget_cents: Some(500),
        max_running: Some(3),
        max_writable: None,
        plan_first: false,
        start_without_approval: false,
        self_update: Some(SelfUpdate {
            dev_checkout: Some("/work/neko".into()),
            native_commit: Some("af17eea828b0".into()),
            web_commit: Some("a233e48dd01c".into()),
            check: UpdateCheck { at_ms: DEMO_NOW - 7 * MIN, remote_head: Some("af17eea".into()), error: String::new() },
            status: "Up to date".into(),
            ..SelfUpdate::default()
        }),
        workspaces: vec![
            WorkspaceInfo { id: "w1".into(), name: "Neko".into(), repository: "/work/neko".into(), folders: vec!["/work/neko".into(), "/work/docs-site".into()] },
            WorkspaceInfo { id: "w2".into(), name: "Home".into(), repository: "/home/me".into(), folders: vec!["/home/me".into()] },
        ],
        usage: (0..40).map(|i| (DEMO_NOW - (i * 4 * HOUR + 17 * MIN), ((0.3 + ((i * 7) % 11) as f64 / 20.0) * 1_000_000.0) as u64)).filter(|(at, _)| DEMO_NOW - at < 7 * day).collect(),
        rulebook: String::new(),
        rulebook_default: "Be careful with other people's files.\nPrefer small, reviewable changes.\nRun the tests before saying a task is done.".into(),
        model_runs: [("gpt-a", Some("high")), ("gpt-a", Some("high")), ("gpt-a", None), ("claude-x", Some("medium")), ("", None)].iter().map(|(m, e)| ((*m).to_owned(), e.map(str::to_owned))).collect(),
        spend: crate::link::SpendReport {
            total: crate::link::tree_model::Usd(14.37),
            runs: 12,
            tokens_in: 2_300,
            tokens_out: 480_000,
            tasks: vec![
                crate::link::SpendTask { title: "Add a frosted glass theme option".into(), cost: crate::link::tree_model::Usd(9.10), runs: 4 },
                crate::link::SpendTask { title: "Fix users unable to join studio sessions".into(), cost: crate::link::tree_model::Usd(4.08), runs: 5 },
                crate::link::SpendTask { title: "Triage interrupted message stream errors".into(), cost: crate::link::tree_model::Usd(1.19), runs: 3 },
            ],
            priced_tasks: 3,
            tasks_total: 6,
        },
        connections: 3,
        home_last: Some(crate::link::LastRun { model: "gpt-a".into(), effort: Some("high".into()), automatic: true, reason: "Picked the model that fits a small question.".into() }),
    }
}

/// Opens a view over the usual made-up workbench, with the page's data already in.
pub(crate) fn base(s: &mut App, view: FullView) {
    super::scenes::live_for_pages(s);
    s.now_ms = DEMO_NOW;
    s.open_view(view);
    s.outbox.clear();
}

/// The made-up now of the calendar scenes: late morning in the viewer's own time zone, so the
/// current-time line is on the grid wherever the snapshot is made.
pub fn calendar_now() -> i64 {
    local_at(clock::week_start(DEMO_NOW), 2 * 24 + 11, 40)
}

pub(crate) fn with_settings(s: &mut App) {
    base(s, FullView::Settings);
    if let Some(m) = s.summary.as_mut() {
        m.settings = settings_view();
    }
    s.pages.routines = Some(routines_data());
    s.pages.phone = Some(phone_data());
    s.pages.catalog = Some(catalog());
    s.pages.calendar = Some(calendar_data(DEMO_NOW, 0, 0));
    // The fields show what the daemon says is saved, as they do in the app.
    s.seed_settings();
}

pub fn routines(s: &mut App) {
    base(s, FullView::Routines);
    s.pages.routines = Some(routines_data());
    for (w, text) in &routines_data().standing_orders {
        s.pages.set_field(&format!("orders:{w}"), text);
        s.pages.orders_saved.insert(w.clone(), text.clone());
    }
}

pub fn routines_open(s: &mut App) {
    base(s, FullView::Routines);
    s.pages.routines = Some(routines_data());
    for (w, text) in &routines_data().standing_orders {
        s.pages.set_field(&format!("orders:{w}"), text);
        s.pages.orders_saved.insert(w.clone(), text.clone());
    }
    s.open_routine("r1");
    // An edit in flight: the editor has something to save.
    s.pages.set_field("rt:instruction", "Read the team chat and my mail. Answer what is simple, and tell me what only I can answer. Skip newsletters.");
}

pub fn calendar(s: &mut App) {
    base(s, FullView::Calendar);
    s.now_ms = calendar_now();
    s.pages.cal_day = clock::day_start(s.now_ms);
    let (from, to) = s.calendar_range();
    s.pages.calendar = Some(calendar_data(s.now_ms, from, to));
}

pub fn calendar_day(s: &mut App) {
    base(s, FullView::Calendar);
    s.now_ms = calendar_now();
    s.pages.cal_mode = CalMode::Day;
    s.pages.cal_day = clock::day_start(s.now_ms);
    let (from, to) = s.calendar_range();
    s.pages.calendar = Some(calendar_data(s.now_ms, from, to));
}

pub fn calendar_event(s: &mut App) {
    base(s, FullView::Calendar);
    s.now_ms = calendar_now();
    s.pages.cal_day = clock::day_start(s.now_ms);
    let (from, to) = s.calendar_range();
    s.pages.calendar = Some(calendar_data(s.now_ms, from, to));
    s.pages.cal_event = Some("e4".into());
}

pub fn settings(s: &mut App) {
    with_settings(s);
}

pub fn settings_ai(s: &mut App) {
    with_settings(s);
    s.pages.section = crate::pages::Section::Ai;
}

pub fn settings_budget(s: &mut App) {
    with_settings(s);
    s.pages.section = crate::pages::Section::Ai;
}

pub fn settings_phone(s: &mut App) {
    with_settings(s);
    s.pages.section = crate::pages::Section::Phone;
}

pub fn settings_updates(s: &mut App) {
    with_settings(s);
    s.pages.section = crate::pages::Section::Updates;
}

pub fn settings_search(s: &mut App) {
    with_settings(s);
    s.pages.set_field("settings:search", "quiet");
}

pub fn usage(s: &mut App) {
    base(s, FullView::Usage);
    if let Some(m) = s.summary.as_mut() {
        m.settings = settings_view();
    }
    s.pages.routines = Some(routines_data());
    s.pages.quota = Some(quota_rows());
}

/// The plugins as `/api/plugins` lists them, made up: three plugins, one switched off.
pub fn plugin_listings() -> Vec<crate::link::http::PluginListing> {
    use crate::link::http::PluginListing;
    vec![
        PluginListing { id: "board-viewer".into(), title: "board-viewer".into(), entry: "/plugins/board-viewer/index.js".into(), version: 4, manifest_version: "0.4.0".into(), path: "/data/plugins/board-viewer".into(), description: "Draws a board artifact".into(), viewers: vec![("board".into(), "Board".into(), vec!["board".into()])], ..PluginListing::default() },
        PluginListing { id: "release-notes".into(), title: "release-notes".into(), entry: "/plugins/release-notes/index.js".into(), version: 2, manifest_version: "0.2.0".into(), path: "/data/plugins/release-notes".into(), description: "A page of release notes".into(), ..PluginListing::default() },
        PluginListing { id: "calendar-sync".into(), title: "calendar-sync".into(), entry: "/plugins/calendar-sync/index.js".into(), version: 7, manifest_version: "1.0.0".into(), path: "/data/plugins/calendar-sync".into(), description: "Mirrors a calendar".into(), disabled: true, ..PluginListing::default() },
    ]
}

pub fn plugins(s: &mut App) {
    base(s, FullView::Plugins);
    s.pages.plugins = Some(plugin_list());
    s.plugin_ui.listings = Some(plugin_listings());
    s.plugin_ui.custom = Some(crate::link::http::Customisation { plugins_dir: "/data/plugins".into(), css_path: "/data/custom.css".into(), css_exists: false });
    s.plugin_pages = Some(vec![neko_protocol::PluginPage { id: "board-viewer:board".into(), plugin: "board-viewer".into(), title: "Board".into() }]);
}

/// The Customisations page with nothing customised yet (the web's Empty with the manifest example).
pub fn plugins_none(s: &mut App) {
    plugins(s);
    s.pages.plugins = Some(Vec::new());
    s.plugin_ui.listings = Some(Vec::new());
    s.plugin_pages = Some(Vec::new());
}

/// Settings with the sessions column dragged wide: the page's own width is under 760.
pub fn settings_narrow(s: &mut App) {
    with_settings(s);
    s.pages.section = crate::pages::Section::Updates;
    s.layout.drag(crate::layout::Region::Sessions, 300.0, 120.0);
}
