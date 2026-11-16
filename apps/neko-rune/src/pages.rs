//! What the full views keep between frames and what pressing their controls does: the Routines
//! page's open routine and editor, the Calendar's week, Settings' section and search, Usage, the
//! workspaces' open state, the plugins. No drawing and no I/O here: every request goes out through
//! [`App::call`], so each rule is tested by looking at what was asked of the daemon.
//!
//! The rules are the web UI's, ported: `pages/Inbox.tsx` (routines, their editor, standing orders),
//! `pages/Calendar.tsx`, `pages/setup/Settings.tsx` and its sections, `pages/setup/Usage.tsx`,
//! `pages/setup/Workspaces.tsx`.

use std::collections::{HashMap, HashSet};

use neko_protocol::agent_models::ModelCatalog;
use neko_protocol::phone::PhoneSettings;
use neko_protocol::routines::{Cadence, Routine, RoutineView};
use neko_protocol::workbench::AgentRuntime;
use neko_protocol::PluginHealth;
use rune::TextEdit;

use crate::app::App;
use crate::clock;
use crate::link::{CalendarData, Call, PhoneData, QuotaRow, Reply, RoutinesData};
use crate::views::FullView;

/// How long a page's data is trusted before it is asked for again while the page is open.
pub const PAGE_POLL_MS: i64 = 30_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalMode {
    Day,
    Week,
    /// Six weeks of seven days that hold the month (the web's `daysFor("month")`).
    Month,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Section {
    General,
    Ai,
    Appearance,
    Rulebook,
    Budget,
    Phone,
    Updates,
    Workspaces,
    QuickPanel,
    Permissions,
    Diagnostics,
    About,
}

impl Section {
    pub const ALL: [Section; 12] = [Section::General, Section::Ai, Section::Appearance, Section::Rulebook, Section::Budget, Section::Phone, Section::QuickPanel, Section::Permissions, Section::Diagnostics, Section::Updates, Section::Workspaces, Section::About];

    pub fn title(self) -> &'static str {
        match self {
            Section::General => "General",
            Section::Ai => "AI and agents",
            Section::Appearance => "Appearance",
            Section::Rulebook => "Rulebook",
            Section::QuickPanel => "Quick panel",
            Section::Permissions => "Permissions",
            Section::Diagnostics => "Diagnostics",
            Section::Budget => "Budget",
            Section::Phone => "Phone",
            Section::Updates => "Neko updates",
            Section::Workspaces => "Workspaces",
            Section::About => "About",
        }
    }

    /// The rail's heading above this section, where one starts a group: "Neko" and "This Mac", as the web's rail has them.
    pub fn group(self) -> Option<&'static str> {
        match self {
            Section::General => Some("Neko"),
            Section::QuickPanel => Some("This Mac"),
            _ => None,
        }
    }

    pub fn key(self) -> &'static str {
        match self {
            Section::General => "general",
            Section::Ai => "ai",
            Section::Appearance => "appearance",
            Section::Rulebook => "rulebook",
            Section::QuickPanel => "quick-panel",
            Section::Permissions => "permissions",
            Section::Diagnostics => "diagnostics",
            Section::Budget => "budget",
            Section::Phone => "phone",
            Section::Updates => "updates",
            Section::Workspaces => "workspaces",
            Section::About => "about",
        }
    }
}

/// The routine editor's choices that are not text (the instruction and the model are fields).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoutineDraft {
    pub id: String,
    pub cadence: Cadence,
    pub model: String,
    pub browser: bool,
    pub computer: bool,
    pub catch_up: bool,
}

impl RoutineDraft {
    pub fn of(routine: &Routine) -> RoutineDraft {
        RoutineDraft { id: routine.id.clone(), cadence: routine.cadence.clone(), model: routine.model.clone(), browser: routine.browser, computer: routine.computer, catch_up: routine.catches_up() }
    }
}

/// The runtime being chosen in Settings: the saved one until something is picked.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeDraft(pub AgentRuntime);

/// The words for a routine's cadence, as the web's Inbox page writes them.
pub fn cadence_text(c: &Cadence) -> String {
    match c {
        Cadence::Every { minutes, from_hour, to_hour } if from_hour == to_hour => format!("Every {minutes} min"),
        Cadence::Every { minutes, from_hour, to_hour } => format!("Every {minutes} min, {from_hour:02}:00–{to_hour:02}:00"),
        Cadence::Daily { hour, minute } => format!("Daily at {hour:02}:{minute:02}"),
    }
}

/// The web's `clampInt`: the number a field holds, kept between `min` and `max`; `fallback` when it holds none.
pub fn clamp_int(text: &str, min: u32, max: u32, fallback: u32) -> u32 {
    text.trim().parse::<u32>().map_or(fallback, |n| n.clamp(min, max))
}

/// "HH:MM" as the daily time field reads it (the web's `time.split(":").map(Number || 0)`), hour kept in 0-23 and minute in 0-59.
pub fn parse_time(text: &str) -> Option<(u32, u32)> {
    let (h, m) = text.trim().split_once(':')?;
    let (h, m) = (h.trim().parse::<u32>().ok()?, m.trim().parse::<u32>().ok()?);
    (h < 24 && m < 60).then_some((h, m))
}

/// What the cadence select offers: the routine's own first, then the usual choices. A choice keeps
/// the routine's hours window when it repeats through the day.
pub fn cadence_options(current: &Cadence) -> Vec<(String, Cadence)> {
    let (from, to) = match current {
        Cadence::Every { from_hour, to_hour, .. } => (*from_hour, *to_hour),
        Cadence::Daily { .. } => (8, 23),
    };
    let mut out: Vec<Cadence> = vec![current.clone()];
    for minutes in [5, 10, 20, 30, 60, 120] {
        out.push(Cadence::Every { minutes, from_hour: from, to_hour: to });
    }
    for hour in [6, 7, 8, 9, 10, 12, 14, 16, 18, 20, 22] {
        out.push(Cadence::Daily { hour, minute: 0 });
    }
    let mut seen = Vec::new();
    out.into_iter()
        .filter(|c| !seen.contains(c) && {
            seen.push(c.clone());
            true
        })
        .map(|c| (cadence_text(&c), c))
        .collect()
}

/// Claude Code's model aliases a routine may use; empty means the first (the web's hint).
pub fn model_options(current: &str) -> Vec<(String, String)> {
    let mut out = vec![("sonnet (default)".to_owned(), String::new()), ("opus".to_owned(), "opus".to_owned()), ("haiku".to_owned(), "haiku".to_owned())];
    if !current.is_empty() && !out.iter().any(|(_, v)| v == current) {
        out.push((current.to_owned(), current.to_owned()));
    }
    out
}

/// The routine's state in the one status language: running, held, failed, paused, on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RoutineGlyph {
    On,
    Running,
    Held(String),
    Failed,
    Paused,
}

pub fn routine_glyph(view: &RoutineView, now_ms: i64) -> RoutineGlyph {
    let (routine, state) = (&view.routine, &view.state);
    if state.running {
        return RoutineGlyph::Running;
    }
    if !routine.enabled {
        return RoutineGlyph::Paused;
    }
    if let Some(hold) = state.hold.as_ref().filter(|h| h.until_ms > now_ms) {
        return RoutineGlyph::Held(format!("Held until {}", clock::hhmm(hold.until_ms)));
    }
    match view.runs.first() {
        Some(last) if !last.ok => RoutineGlyph::Failed,
        _ => RoutineGlyph::On,
    }
}

/// When the next run is due, in a few words ("in 12m", "due now", "queued").
pub fn next_due_text(view: &RoutineView, now_ms: i64) -> String {
    let state = &view.state;
    if state.running {
        return "running".to_owned();
    }
    if state.run_requested {
        return "queued".to_owned();
    }
    if !view.routine.enabled {
        return String::new();
    }
    match state.next_due_ms {
        Some(due) if due > now_ms => format!("next {}", short_in(due - now_ms)),
        _ => "due now".to_owned(),
    }
}

fn short_in(ms: i64) -> String {
    match ms / 60_000 {
        0 => "in under a minute".to_owned(),
        m if m < 60 => format!("in {m}m"),
        m if m < 24 * 60 => format!("in {}h", m / 60),
        m => format!("in {}d", m / (24 * 60)),
    }
}

/// How long ago, quietly ("3m ago").
pub fn ago(now_ms: i64, at_ms: i64) -> String {
    match (now_ms - at_ms).max(0) / 60_000 {
        0 => "just now".to_owned(),
        m if m < 60 => format!("{m}m ago"),
        m if m < 24 * 60 => format!("{}h ago", m / 60),
        m => format!("{}d ago", m / (24 * 60)),
    }
}

/// One thing a page keeps.
pub struct Pages {
    /// The appearance mode the daemon keeps (`system`, `light`, `dark`); None until it answers.
    pub appearance: Option<String>,
    /// What this process may do on the Mac, as last probed (Settings, Permissions).
    pub access: Option<crate::permissions::Local>,
    pub routines: Option<RoutinesData>,
    pub open_routine: Option<String>,
    pub draft: Option<RoutineDraft>,
    /// Standing orders as last saved, per workspace, to tell an edited box from an untouched one.
    pub orders_saved: HashMap<String, String>,
    pub fields: HashMap<String, TextEdit>,
    /// What a seeded field last showed as saved: a field still showing that has not been edited.
    pub field_saved: HashMap<String, String>,
    /// The phone row last changed, so a refusal shows under it.
    pub phone_row: &'static str,
    pub calendar: Option<CalendarData>,
    pub cal_mode: CalMode,
    /// The local midnight of the day the calendar is on (the week that holds it in week view).
    pub cal_day: i64,
    /// The event whose "Open in Calendar" is showing.
    pub cal_event: Option<String>,
    pub phone: Option<PhoneData>,
    pub phone_test: Vec<String>,
    pub catalog: Option<ModelCatalog>,
    pub runtime_draft: Option<RuntimeDraft>,
    pub quota: Option<Vec<QuotaRow>>,
    pub plugins: Option<Vec<PluginHealth>>,
    pub section: Section,
    /// The select whose list is open, by key.
    pub open_select: Option<String>,
    pub ws_open: HashSet<String>,
    /// A refusal shown next to the control it belongs to, by control key.
    pub notes: HashMap<String, String>,
    /// When the open page last asked, by the app's clock.
    pub asked_ms: i64,
    pub routines_error: Option<String>,
    /// What a text field shows before anything was typed in it.
    pub blank: TextEdit,
}

impl Default for Pages {
    fn default() -> Self {
        Pages {
            appearance: None,
            access: None,
            routines: None,
            open_routine: None,
            draft: None,
            orders_saved: HashMap::new(),
            fields: HashMap::new(),
            field_saved: HashMap::new(),
            phone_row: "test",
            calendar: None,
            cal_mode: CalMode::Week,
            cal_day: 0,
            cal_event: None,
            phone: None,
            phone_test: Vec::new(),
            catalog: None,
            runtime_draft: None,
            quota: None,
            plugins: None,
            section: Section::General,
            open_select: None,
            ws_open: HashSet::new(),
            notes: HashMap::new(),
            asked_ms: i64::MIN / 2,
            routines_error: None,
            blank: TextEdit::new(""),
        }
    }
}

impl Pages {
    /// The text field with this key, made empty on first use.
    pub fn field(&mut self, key: &str) -> &mut TextEdit {
        self.fields.entry(key.to_owned()).or_insert_with(|| TextEdit::new(""))
    }

    /// The text field's contents ("" before it exists).
    pub fn text(&self, key: &str) -> &str {
        self.fields.get(key).map_or("", |f| f.text.as_str())
    }

    pub fn set_field(&mut self, key: &str, text: &str) {
        self.field(key).replace_text(text);
    }

    /// Shows what is saved in a field, unless the person has edited it (it then shows their words).
    /// A field that was never seeded is never saved: Save and Return do nothing for it.
    pub fn seed(&mut self, key: &str, saved: &str) {
        let untouched = match (self.fields.get(key), self.field_saved.get(key)) {
            (None, _) => true,
            (Some(f), Some(prev)) => f.text == *prev,
            (Some(_), None) => false,
        };
        if untouched {
            self.set_field(key, saved);
        }
        self.field_saved.insert(key.to_owned(), saved.to_owned());
    }

    /// The field has been seeded and now shows something other than what is saved.
    pub fn edited(&self, key: &str, trim_dollar: bool) -> bool {
        match (self.fields.get(key), self.field_saved.get(key)) {
            (Some(f), Some(saved)) => {
                let now = f.text.trim();
                let now = if trim_dollar { now.trim_start_matches('$').trim() } else { now };
                now != saved.trim()
            }
            _ => false,
        }
    }

    pub fn routine(&self, id: &str) -> Option<&RoutineView> {
        self.routines.as_ref()?.views.iter().find(|v| v.routine.id == id)
    }
}

/// A control's key for a refusal: the routine, the setting, the page.
pub fn control_key(call: &Call) -> Option<String> {
    Some(match call {
        Call::SaveRoutine(r) => format!("routine:{}", r.id),
        Call::RunRoutine { id } | Call::SetRoutineEnabled { id, .. } => format!("routine:{id}"),
        Call::SetRoutinesOn { .. } => "setting:routines-on".to_owned(),
        Call::SetStandingOrders { workspace_id, .. } => format!("orders:{workspace_id}"),
        Call::SetInboxAgeOut { .. } => "setting:age-out".to_owned(),
        Call::SetCalendarOn { .. } => "setting:calendar-on".to_owned(),
        Call::SavePhone(_) | Call::PhoneTest => "setting:phone".to_owned(),
        Call::SetAgentRuntime(_) => "setting:runtime".to_owned(),
        Call::SetTaskBudget { .. } => "setting:task-budget".to_owned(),
        Call::SetDailyBudget { .. } => "setting:daily-budget".to_owned(),
        Call::SetMaxRunning { .. } => "setting:max-running".to_owned(),
        Call::SetMaxWritable { .. } => "setting:max-writable".to_owned(),
        Call::SetPlanFirst { .. } => "setting:plan-first".to_owned(),
        Call::SetStartWithoutApproval { .. } => "setting:start-without-approval".to_owned(),
        Call::SetSelfUpdate { .. } | Call::UpdateNow | Call::ApplyUpdate => "setting:updates".to_owned(),
        Call::SetDevCheckout { .. } => "setting:dev-checkout".to_owned(),
        Call::SetAppearance { .. } => "setting:appearance".to_owned(),
        Call::Appearance => "page:appearance".to_owned(),
        Call::RoutinesFull => "page:routines".to_owned(),
        Call::CalendarEvents { .. } => "page:calendar".to_owned(),
        Call::PhoneSettings => "page:phone".to_owned(),
        Call::Quota => "page:quota".to_owned(),
        Call::Plugins => "page:plugins".to_owned(),
        _ => return None,
    })
}

/// A budget as its field shows it: dollars and cents, or empty for none.
pub fn dollars(cents: Option<u32>) -> String {
    cents.map_or(String::new(), |c| format!("{:.2}", c as f64 / 100.0))
}

/// Cents from the text a money field holds ("12.50" or "$12.50"); None for empty (no limit).
/// Err for something that is not an amount in range.
pub fn parse_dollars(text: &str, max: f64) -> Result<Option<u32>, &'static str> {
    let clean = text.trim().trim_start_matches('$').trim();
    if clean.is_empty() {
        return Ok(None);
    }
    match clean.parse::<f64>() {
        Ok(v) if v.is_finite() && (0.01..=max).contains(&v) => Ok(Some((v * 100.0).round() as u32)),
        _ => Err("Enter an amount, or leave it empty."),
    }
}

impl App {
    /// A full view opens: its data is asked for at once, then again while it stays open.
    pub fn enter_view(&mut self, view: FullView) {
        self.pages.open_select = None;
        self.pages.notes.clear();
        self.pages.asked_ms = i64::MIN / 2;
        if view == FullView::Calendar && self.pages.cal_day == 0 {
            self.pages.cal_day = clock::day_start(self.now_ms);
        }
        self.refresh_page(view);
    }

    /// Asks for what the open view shows (at most every [`PAGE_POLL_MS`] unless forced by `asked_ms`).
    pub fn refresh_page(&mut self, view: FullView) {
        if self.now_ms - self.pages.asked_ms < PAGE_POLL_MS {
            return;
        }
        self.pages.asked_ms = self.now_ms;
        match view {
            FullView::Routines | FullView::Workspaces => self.call(Call::RoutinesFull),
            FullView::Calendar => self.ask_calendar(),
            FullView::Usage => {
                self.call(Call::RoutinesFull);
                self.call(Call::Quota);
            }
            FullView::Plugins => self.call(Call::Plugins),
            FullView::AllSessions => {}
            FullView::Settings => {
                self.call(Call::RoutinesFull);
                self.call(Call::PhoneSettings);
                self.call(Call::Models);
                self.call(Call::Appearance);
                self.pages.access = Some((self.access_probe)());
                self.ask_calendar();
            }
        }
    }

    /// The days the calendar shows now, as [from, to) in local midnights.
    pub fn calendar_range(&self) -> (i64, i64) {
        let day = if self.pages.cal_day == 0 { clock::day_start(self.now_ms) } else { self.pages.cal_day };
        match self.pages.cal_mode {
            CalMode::Day => (day, clock::add_days(day, 1)),
            CalMode::Week => {
                let monday = clock::week_start(day + 12 * 3_600_000);
                (monday, clock::add_days(monday, 7))
            }
            CalMode::Month => {
                // The week of the 1st, then six weeks: 42 days, 6 x 7.
                let monday = clock::week_start(clock::month_start(day) + 12 * 3_600_000);
                (monday, clock::add_days(monday, 42))
            }
        }
    }

    pub fn ask_calendar(&mut self) {
        let (from_ms, to_ms) = self.calendar_range();
        self.call(Call::CalendarEvents { from_ms, to_ms });
    }

    pub fn cal_mode(&mut self, mode: CalMode) {
        self.pages.cal_mode = mode;
        self.pages.cal_event = None;
        self.ask_calendar();
    }

    /// Steps the calendar by a day or a week; `0` goes to today.
    pub fn cal_step(&mut self, direction: i64) {
        let today = clock::day_start(self.now_ms);
        let step = if self.pages.cal_mode == CalMode::Week { 7 } else { 1 };
        let day = if self.pages.cal_day == 0 { today } else { self.pages.cal_day };
        self.pages.cal_day = match (direction, self.pages.cal_mode) {
            (0, _) => today,
            (_, CalMode::Month) => clock::add_months(clock::month_start(day), direction),
            _ => clock::add_days(day, direction * step),
        };
        self.pages.cal_event = None;
        self.ask_calendar();
    }

    /// A day number in the month grid: that day, in the Day view (the web's `onPickDay`).
    pub fn cal_pick_day(&mut self, day: i64) {
        self.pages.cal_day = day;
        self.cal_mode(CalMode::Day);
    }

    /// "Join meeting" and "Open link" in an event's popover: the event's own address, in the browser.
    pub fn cal_open_link(&mut self, url: &str) {
        self.pages.cal_event = None;
        self.open_link(url);
    }

    /// "Open in Calendar": macOS Calendar at the event's day (`calshow:`), not a web page.
    pub fn open_in_calendar(&mut self, start_ms: i64) {
        let url = format!("calshow:{}", clock::calshow_seconds(start_ms));
        if (self.opener)(&url).is_err() {
            self.pages.notes.insert("page:calendar".to_owned(), "Couldn’t open Calendar.".to_owned());
        }
    }

    // ---- routines

    /// Opens a routine's details under its row (accordion: one at a time), or closes it.
    pub fn open_routine(&mut self, id: &str) {
        if self.pages.open_routine.as_deref() == Some(id) {
            self.pages.open_routine = None;
            self.pages.draft = None;
            return;
        }
        let Some(view) = self.pages.routine(id).cloned() else { return };
        self.pages.open_routine = Some(id.to_owned());
        self.pages.draft = Some(RoutineDraft::of(&view.routine));
        self.pages.set_field("rt:instruction", &view.routine.instruction);
        self.seed_cadence_fields();
        self.pages.open_select = None;
        self.pages.notes.remove(&format!("routine:{id}"));
    }

    /// The cadence fields show the draft's numbers (the minutes as typed text, the time as HH:MM).
    pub fn seed_cadence_fields(&mut self) {
        let Some(draft) = self.pages.draft.clone() else { return };
        match draft.cadence {
            Cadence::Every { minutes, .. } => self.pages.set_field("rt:minutes", &minutes.to_string()),
            Cadence::Daily { hour, minute } => self.pages.set_field("rt:time", &format!("{hour:02}:{minute:02}")),
        }
    }

    /// The segmented control: Every (repeats through the day) or Daily (once a day). Each starts from the web's own defaults.
    pub fn set_cadence_mode(&mut self, every: bool) {
        let Some(d) = self.pages.draft.as_mut() else { return };
        let is_every = matches!(d.cadence, Cadence::Every { .. });
        if is_every == every {
            return;
        }
        d.cadence = if every { Cadence::Every { minutes: 20, from_hour: 8, to_hour: 23 } } else { Cadence::Daily { hour: 10, minute: 30 } };
        self.seed_cadence_fields();
    }

    /// The minutes field was typed in: the draft follows (kept between 5 and 720; the field keeps what was typed until it is saved).
    pub fn cadence_minutes_typed(&mut self) {
        let typed = self.pages.text("rt:minutes").to_owned();
        if let Some(Cadence::Every { minutes, .. }) = self.pages.draft.as_mut().map(|d| &mut d.cadence) {
            *minutes = clamp_int(&typed, 5, 720, 20);
        }
    }

    /// The steppers beside the minutes: 5 more or fewer.
    pub fn cadence_minutes_step(&mut self, by: i32) {
        let current = clamp_int(self.pages.text("rt:minutes"), 5, 720, 20) as i32;
        let next = (((current + by) / 5) * 5).clamp(5, 720) as u32;
        if let Some(Cadence::Every { minutes, .. }) = self.pages.draft.as_mut().map(|d| &mut d.cadence) {
            *minutes = next;
        }
        self.pages.set_field("rt:minutes", &next.to_string());
    }

    /// From and Until: whole hours, 0 to 23 (an Until the routine already has of 24 stays).
    pub fn cadence_hours(&mut self, from: Option<u32>, to: Option<u32>) {
        if let Some(Cadence::Every { from_hour, to_hour, .. }) = self.pages.draft.as_mut().map(|d| &mut d.cadence) {
            if let Some(f) = from {
                *from_hour = f.min(23);
            }
            if let Some(t) = to {
                *to_hour = t.min(24);
            }
        }
    }

    /// The time field was typed in: a valid HH:MM becomes the draft's time (anything else leaves it, as the web's `|| "10:30"`).
    pub fn cadence_time_typed(&mut self) {
        let typed = self.pages.text("rt:time").to_owned();
        if let (Some((hour, minute)), Some(Cadence::Daily { hour: h, minute: m })) = (parse_time(&typed), self.pages.draft.as_mut().map(|d| &mut d.cadence)) {
            *h = hour;
            *m = minute;
        }
    }

    /// The editor differs from the saved routine.
    pub fn routine_dirty(&self) -> bool {
        let (Some(id), Some(draft)) = (self.pages.open_routine.as_deref(), self.pages.draft.as_ref()) else { return false };
        let Some(view) = self.pages.routine(id) else { return false };
        *draft != RoutineDraft::of(&view.routine) || self.pages.text("rt:instruction").trim() != view.routine.instruction.trim()
    }

    pub fn save_routine(&mut self) {
        let (Some(id), Some(draft)) = (self.pages.open_routine.clone(), self.pages.draft.clone()) else { return };
        let instruction = self.pages.text("rt:instruction").trim().to_owned();
        if instruction.is_empty() {
            self.pages.notes.insert(format!("routine:{id}"), "A routine needs an instruction.".to_owned());
            return;
        }
        let Some(view) = self.pages.routine(&id) else { return };
        // The numbers are kept in range as the web keeps them when it saves (`clampInt`): minutes 5 to 720, hours 0 to 23.
        let cadence = match draft.cadence {
            Cadence::Every { minutes, from_hour, to_hour } => Cadence::Every { minutes: minutes.clamp(5, 720), from_hour: from_hour.min(23), to_hour: to_hour.clamp(0, 24) },
            Cadence::Daily { hour, minute } => Cadence::Daily { hour: hour.min(23), minute: minute.min(59) },
        };
        let draft = RoutineDraft { cadence, ..draft };
        // The web sends the whole routine with the edited fields replaced.
        let routine = Routine { instruction, cadence: draft.cadence, model: draft.model.trim().to_owned(), browser: draft.browser, computer: draft.computer, catch_up: Some(draft.catch_up), ..view.routine.clone() };
        self.pages.notes.remove(&format!("routine:{id}"));
        self.call(Call::SaveRoutine(Box::new(routine)));
    }

    pub fn run_routine(&mut self, id: &str) {
        self.call(Call::RunRoutine { id: id.to_owned() });
    }

    pub fn set_routine_enabled(&mut self, id: &str, enabled: bool) {
        self.call(Call::SetRoutineEnabled { id: id.to_owned(), enabled });
    }

    pub fn save_orders(&mut self, workspace_id: &str) {
        let text = self.pages.text(&format!("orders:{workspace_id}")).to_owned();
        self.call(Call::SetStandingOrders { workspace_id: workspace_id.to_owned(), text });
    }

    /// Standing orders were edited here and not saved yet.
    pub fn orders_dirty(&self, workspace_id: &str) -> bool {
        self.pages.text(&format!("orders:{workspace_id}")) != self.pages.orders_saved.get(workspace_id).map_or("", String::as_str)
    }

    // ---- selects and settings

    pub fn toggle_select(&mut self, key: &str) {
        self.pages.open_select = if self.pages.open_select.as_deref() == Some(key) { None } else { Some(key.to_owned()) };
    }

    /// The runtime Settings shows: what is being picked, else what is saved.
    pub fn runtime_shown(&self) -> AgentRuntime {
        self.pages.runtime_draft.as_ref().map(|d| d.0.clone()).unwrap_or_else(|| self.summary.as_ref().map(|m| m.settings.runtime.clone()).unwrap_or_default())
    }

    pub fn runtime_dirty(&self) -> bool {
        self.pages.runtime_draft.as_ref().is_some_and(|d| Some(&d.0) != self.summary.as_ref().map(|m| &m.settings.runtime))
    }

    pub fn pick_runtime(&mut self, change: impl FnOnce(&mut AgentRuntime)) {
        let mut runtime = self.runtime_shown();
        change(&mut runtime);
        self.pages.runtime_draft = Some(RuntimeDraft(runtime));
    }

    pub fn use_runtime(&mut self) {
        if let Some(d) = self.pages.runtime_draft.clone() {
            self.call(Call::SetAgentRuntime(d.0));
        }
    }

    /// A money setting saved from its field: refuses what is not an amount, says so next to it.
    pub fn save_money(&mut self, field: &str, max: f64, make: impl FnOnce(Option<u32>) -> Call) {
        // A field that was never filled from what is saved, or still shows it, sends nothing: an empty
        // box that was never seeded must not read as "no limit".
        if !self.pages.edited(field, true) {
            return;
        }
        match parse_dollars(self.pages.text(field), max) {
            Ok(cents) => {
                self.pages.notes.remove(&format!("setting:{field}"));
                self.call(make(cents));
            }
            Err(why) => {
                self.pages.notes.insert(format!("setting:{field}"), format!("{why} Between 0.01 and {max:.0}."));
            }
        }
    }

    /// The development checkout from its field; nothing when it still shows what is saved.
    pub fn save_dev_checkout(&mut self) {
        if self.pages.edited("dev-checkout", false) {
            let path = self.pages.text("dev-checkout").trim().to_owned();
            self.call(Call::SetDevCheckout { path });
        }
    }

    /// The ntfy server from its field; nothing when it still shows what is saved.
    pub fn save_ntfy_server(&mut self) {
        if self.pages.edited("ntfy-server", false) {
            let server = self.pages.text("ntfy-server").trim().to_owned();
            if !server.is_empty() {
                self.change_phone("ntfy-server", |p| p.ntfy_server = server);
            }
        }
    }

    /// Fills the settings fields from what the daemon says is saved (until the person edits them).
    pub fn seed_settings(&mut self) {
        if let Some(st) = self.summary.as_ref().map(|m| m.settings.clone()) {
            self.pages.seed("task-budget", &dollars(st.task_budget_cents));
            self.pages.seed("daily-budget", &dollars(st.daily_budget_cents));
            self.pages.seed("dev-checkout", &st.self_update.as_ref().and_then(|u| u.dev_checkout.clone()).unwrap_or_default());
        }
        if let Some(server) = self.pages.phone.as_ref().map(|p| p.settings.ntfy_server.clone()) {
            self.pages.seed("ntfy-server", &server);
        }
    }

    /// Changes one phone setting and saves, as the web does on each change. `row` is the row it
    /// belongs to: a refusal shows under it.
    pub fn change_phone(&mut self, row: &'static str, change: impl FnOnce(&mut PhoneSettings)) {
        let Some(mut settings) = self.pages.phone.as_ref().map(|p| p.settings.clone()) else { return };
        change(&mut settings);
        self.pages.phone_row = row;
        self.call(Call::SavePhone(Box::new(settings)));
    }

    /// What a page-related answer does to the state. None when the reply is not one of theirs.
    pub fn apply_page_reply(&mut self, call: &Call, reply: &Reply) -> Option<bool> {
        match reply {
            Reply::Routines(data) => {
                // Standing orders show as saved until edited here.
                for (workspace, text) in &data.standing_orders {
                    let key = format!("orders:{workspace}");
                    let edited = match (self.pages.fields.get(&key), self.pages.orders_saved.get(workspace)) {
                        (Some(field), Some(saved)) => field.text != *saved,
                        _ => false,
                    };
                    if !edited {
                        self.pages.set_field(&key, text);
                    }
                    self.pages.orders_saved.insert(workspace.clone(), text.clone());
                }
                if let Some(key) = control_key(call) {
                    self.pages.notes.remove(&key);
                }
                let changed = self.pages.routines.as_ref() != Some(&**data);
                self.pages.routines = Some((**data).clone());
                Some(changed)
            }
            Reply::Calendar(data) => {
                // An answer for days that are no longer on screen (the person moved on) is dropped.
                if (data.from_ms, data.to_ms) != self.calendar_range() {
                    return Some(false);
                }
                let changed = self.pages.calendar.as_ref() != Some(&**data);
                self.pages.calendar = Some((**data).clone());
                self.pages.notes.remove("page:calendar");
                Some(changed)
            }
            Reply::CalendarStatus(status) => {
                if let Some(c) = self.pages.calendar.as_mut() {
                    c.status = status.clone();
                }
                self.pages.asked_ms = i64::MIN / 2;
                self.ask_calendar();
                Some(true)
            }
            Reply::Phone(data) => {
                let changed = self.pages.phone.as_ref() != Some(&**data);
                self.pages.phone = Some((**data).clone());
                self.pages.notes.retain(|k, _| !k.starts_with("setting:phone") && k != "page:phone");
                self.seed_settings();
                Some(changed)
            }
            Reply::Appearance(mode) => {
                let changed = self.pages.appearance.as_ref() != Some(mode);
                self.pages.appearance = Some(mode.clone());
                self.pages.notes.remove("setting:appearance");
                Some(changed)
            }
            Reply::PhoneTest(lines) => {
                self.pages.phone_test = lines.clone();
                Some(true)
            }
            Reply::Quota(rows) => {
                let changed = self.pages.quota.as_ref() != Some(rows);
                self.pages.quota = Some(rows.clone());
                Some(changed)
            }
            Reply::Plugins(list) => {
                let changed = self.pages.plugins.as_ref() != Some(list);
                self.pages.plugins = Some(list.clone());
                Some(changed)
            }
            Reply::Models { catalog, .. } => {
                let changed = self.pages.catalog.as_ref() != Some(&**catalog);
                self.pages.catalog = Some((**catalog).clone());
                // The new-session form takes its choices from the same answer.
                Some(changed)
            }
            _ => None,
        }
    }

    /// A refusal or failure of a page request: said next to the control it belongs to, never in the
    /// session. None when the call is not a page's; Some(changed) when it is. A read that fails while
    /// the page has older data stays quiet.
    pub fn page_refused(&mut self, call: &Call, message: String) -> Option<bool> {
        let mut key = control_key(call)?;
        if call.reads() && self.pages_have(call) {
            return Some(false);
        }
        if matches!(call, Call::SavePhone(_)) {
            key = format!("setting:phone:{}", self.pages.phone_row);
        } else if matches!(call, Call::PhoneTest) {
            key = "setting:phone:test".to_owned();
        }
        self.pages.notes.insert(key, message);
        Some(true)
    }

    fn pages_have(&self, call: &Call) -> bool {
        match call {
            Call::RoutinesFull => self.pages.routines.is_some(),
            Call::CalendarEvents { .. } => self.pages.calendar.is_some(),
            Call::PhoneSettings => self.pages.phone.is_some(),
            Call::Appearance => self.pages.appearance.is_some(),
            Call::Quota => self.pages.quota.is_some(),
            Call::Plugins => self.pages.plugins.is_some(),
            _ => false,
        }
    }
}

/// Goals appear in the routines list as pseudo-routines the web's Inbox page leaves out.
pub fn is_goal_routine(view: &RoutineView) -> bool {
    view.routine.id.starts_with("goals:")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn money_fields_take_dollars_and_say_when_they_are_not() {
        assert_eq!(parse_dollars("", 100.0), Ok(None));
        assert_eq!(parse_dollars(" $12.5 ", 100.0), Ok(Some(1250)));
        assert!(parse_dollars("0", 100.0).is_err() && parse_dollars("abc", 100.0).is_err() && parse_dollars("101", 100.0).is_err());
    }

    #[test]
    fn the_cadence_select_leads_with_the_routines_own_and_keeps_its_hours() {
        let own = Cadence::Every { minutes: 25, from_hour: 9, to_hour: 18 };
        let options = cadence_options(&own);
        assert_eq!(options[0].0, "Every 25 min, 09:00–18:00");
        assert!(options.iter().any(|(l, c)| l == "Every 30 min, 09:00–18:00" && matches!(c, Cadence::Every { from_hour: 9, to_hour: 18, .. })));
        assert!(options.iter().any(|(l, _)| l == "Daily at 08:00"));
        let labels: Vec<_> = options.iter().map(|(l, _)| l.clone()).collect();
        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), labels.len(), "no two options read alike");
        assert_eq!(cadence_text(&Cadence::Daily { hour: 9, minute: 5 }), "Daily at 09:05");
        assert_eq!(cadence_text(&Cadence::Every { minutes: 20, from_hour: 0, to_hour: 0 }), "Every 20 min");
    }

    #[test]
    fn the_model_select_keeps_an_alias_it_does_not_know() {
        assert_eq!(model_options("")[0], ("sonnet (default)".to_owned(), String::new()));
        assert_eq!(model_options("fable").last(), Some(&("fable".to_owned(), "fable".to_owned())));
    }

    #[test]
    fn ages_and_dues_read_in_a_few_words() {
        assert_eq!(ago(10 * 60_000, 7 * 60_000), "3m ago");
        assert_eq!(ago(0, 5000), "just now");
        assert_eq!(short_in(3 * 3_600_000), "in 3h");
        assert_eq!(short_in(90_000), "in 1m");
    }

    #[test]
    fn sections_have_keys_and_titles() {
        assert_eq!(Section::ALL.map(Section::title), ["General", "AI and agents", "Appearance", "Rulebook", "Budget", "Phone", "Quick panel", "Permissions", "Diagnostics", "Neko updates", "Workspaces", "About"]);
        let mut keys: Vec<_> = Section::ALL.iter().map(|s| s.key()).collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 12);
    }
}
