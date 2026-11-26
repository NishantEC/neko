//! Rounds in the app (docs/knowledge/rounds.md): the words for a schedule and for when a round runs next, the New round / Edit form,
//! and what each press sends. The daemon keeps the rounds and works out their next time; this file only says it in words.

use neko_protocol::rounds::{RunEnd, RunKind};
use neko_protocol::schedule::{Fixed, Schedule, SystemZone, Weekday, Zone, format_at, local_parts, parse_at};
use neko_protocol::{GENERAL, Request, Round, RoundRun, RoundStatus, TaskId, TaskStatus};
use rune::TextEdit;

use crate::state::{App, Asked, Confirm};

/// The schedule kinds the form's picker offers, in order.
pub const KINDS: [&str; 4] = ["Every day", "Some days", "Every N hours", "Run now only"];

/// The New round / Edit panel's fields while the person fills them in.
#[derive(Clone, Debug, PartialEq)]
pub struct RoundForm {
    /// The round being edited; None for a new one.
    pub id: Option<String>,
    pub name: TextEdit,
    pub words: TextEdit,
    pub cat: String,
    pub workspace: String,
    /// An index into [`KINDS`].
    pub kind: usize,
    pub time: TextEdit,
    pub days: [bool; 7],
    pub hours: TextEdit,
    /// Why the daemon refused it, in words.
    pub error: Option<String>,
}

impl RoundForm {
    /// A blank form for a new round by `cat` in `workspace`: every weekday at 09:00 to start from.
    pub fn blank(cat: &str, workspace: &str) -> RoundForm {
        let mut name = TextEdit::default();
        name.focused = true;
        RoundForm {
            id: None,
            name,
            words: TextEdit::default(),
            cat: cat.to_owned(),
            workspace: workspace.to_owned(),
            kind: 1,
            time: TextEdit::new("09:00"),
            days: [true, true, true, true, true, false, false],
            hours: TextEdit::new("6"),
            error: None,
        }
    }

    /// The form of an existing round.
    pub fn of(r: &Round) -> RoundForm {
        let mut f = RoundForm::blank(&r.cat, &r.workspace);
        f.id = Some(r.id.clone());
        f.name = TextEdit::new(&r.name);
        f.words = TextEdit::new(&r.words);
        match &r.schedule {
            Schedule::Daily { at } => (f.kind, f.time) = (0, TextEdit::new(at)),
            Schedule::Weekdays { days, at } => {
                f.kind = 1;
                f.time = TextEdit::new(at);
                f.days = Weekday::ALL.map(|d| days.contains(&d));
            }
            Schedule::Every { hours } => (f.kind, f.hours) = (2, TextEdit::new(&hours.to_string())),
            Schedule::Manual => f.kind = 3,
        }
        f.name.focused = false;
        f
    }

    /// The schedule the fields say, or the words for what is wrong with them.
    pub fn schedule(&self) -> Result<Schedule, String> {
        let at = || parse_at(self.time.text.trim()).map(format_at).ok_or_else(|| "Write the time as hours and minutes, like 09:00.".to_owned());
        match self.kind {
            0 => Ok(Schedule::Daily { at: at()? }),
            1 => {
                let days: Vec<Weekday> = Weekday::ALL.into_iter().filter(|d| self.days[d.index()]).collect();
                if days.is_empty() {
                    return Err("Choose at least one day.".into());
                }
                Ok(Schedule::Weekdays { days, at: at()? })
            }
            2 => match self.hours.text.trim().parse::<u32>() {
                Ok(h) if (1..=24).contains(&h) => Ok(Schedule::Every { hours: h }),
                _ => Err("Every N hours takes a number from 1 to 24.".into()),
            },
            _ => Ok(Schedule::Manual),
        }
    }

    /// Whether Save may be pressed.
    pub fn ready(&self) -> bool {
        !self.name.text.trim().is_empty() && !self.words.text.trim().is_empty() && self.schedule().is_ok()
    }
}

/// "09:00", "Mon, Wed, Fri": the schedule as a person says it.
pub fn schedule_words(s: &Schedule) -> String {
    match s {
        Schedule::Daily { at } => format!("Every day at {at}"),
        Schedule::Weekdays { days, at } => {
            let set: Vec<Weekday> = days.clone();
            let week = [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri];
            let days = if set.len() == 7 {
                return format!("Every day at {at}");
            } else if set == week {
                "Weekdays".to_owned()
            } else if set == [Weekday::Sat, Weekday::Sun] {
                "Weekends".to_owned()
            } else if let [d] = set.as_slice() {
                format!("{}s", d.name())
            } else {
                set.iter().map(|d| d.short()).collect::<Vec<_>>().join(", ")
            };
            format!("{days} at {at}")
        }
        Schedule::Every { hours: 1 } => "Every hour".to_owned(),
        Schedule::Every { hours } => format!("Every {hours} hours"),
        Schedule::Manual => "Only when you press Run now".to_owned(),
    }
}

/// A moment as a person says it next to a time: "today 09:00", "tomorrow 09:00", "Friday 09:00", then "30 Nov 09:00".
pub fn when(ms: i64, now_ms: i64, zone: &dyn Zone) -> String {
    let (at, now) = (local_parts(zone, ms), local_parts(zone, now_ms));
    let at_time = format!("{:02}:{:02}", at.hour, at.minute);
    match at.day_number - now.day_number {
        0 => format!("today {at_time}"),
        1 => format!("tomorrow {at_time}"),
        2..=6 => format!("{} {at_time}", at.weekday.name()),
        _ => {
            const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
            format!("{} {} {at_time}", at.day, MONTHS[(at.month - 1) as usize])
        }
    }
}

/// The same, as the middle of a sentence ("today at 09:00").
pub fn when_at(ms: i64, now_ms: i64, zone: &dyn Zone) -> String {
    let w = when(ms, now_ms, zone);
    match w.rsplit_once(' ') {
        Some((day, time)) => format!("{day} at {time}"),
        None => w,
    }
}

/// What the form says about the first run, before it is saved.
pub fn first_run_words(f: &RoundForm, now_ms: i64, zone: &dyn Zone) -> String {
    let schedule = match f.schedule() {
        Ok(s) => s,
        Err(why) => return why,
    };
    match schedule.next_due(now_ms, now_ms, zone) {
        None => "It never starts by itself: press Run now when you want it.".to_owned(),
        Some(ms) => match schedule {
            Schedule::Every { hours } => format!("The first run is {}, then every {}.", when_at(ms, now_ms, zone), if hours == 1 { "hour".to_owned() } else { format!("{hours} hours") }),
            _ => format!("The first run is {}.", when_at(ms, now_ms, zone)),
        },
    }
}

/// Why a round is not due by itself, in a word or two; None while it is.
pub fn why_not(r: &Round) -> Option<String> {
    match r.status {
        RoundStatus::Scheduled => None,
        RoundStatus::Manual => Some("Run now only".to_owned()),
        RoundStatus::Off => Some("Off".to_owned()),
        RoundStatus::Braked => Some("Turned itself off".to_owned()),
        RoundStatus::Paused => Some("Paused".to_owned()),
        RoundStatus::CatRetired => Some(r.note.clone().unwrap_or_else(|| "Its cat is retired".to_owned()).trim_end_matches('.').to_owned()),
        RoundStatus::FolderMissing => Some("Folder missing".to_owned()),
        RoundStatus::WorkspaceRemoved => Some("Workspace removed".to_owned()),
    }
}

/// How a history line ended, in a word.
pub fn end_word(end: Option<RunEnd>, task: Option<TaskStatus>) -> &'static str {
    match (end, task) {
        (Some(RunEnd::Done), _) => "Done",
        (Some(RunEnd::Asked), _) => "Asked a question",
        (Some(RunEnd::Failed), _) => "Failed",
        (Some(RunEnd::Stopped), _) => "Stopped",
        (Some(RunEnd::Interrupted), _) => "Interrupted",
        (Some(RunEnd::Removed), _) => "Task removed",
        (None, Some(TaskStatus::NeedsYou | TaskStatus::Asked)) => "Needs you",
        (None, _) => "Working",
    }
}

/// A history line as one sentence: "Done", "Skipped: The last run is still working.", "Missed: …".
pub fn line_words(l: &RoundRun, task: Option<TaskStatus>) -> String {
    match l.kind {
        RunKind::Started => end_word(l.end, task).to_owned(),
        RunKind::Skipped => format!("Skipped: {}", l.reason.as_deref().unwrap_or("not run")),
        RunKind::Missed => l.reason.clone().unwrap_or_else(|| "Missed".to_owned()),
    }
}

impl App {
    /// The time zone the round words use: the Mac's own in the real window, the clock's fixed offset in pictures and tests.
    pub fn zone(&self) -> Box<dyn Zone> {
        if self.persist { Box::new(SystemZone) } else { Box::new(Fixed(self.utc_offset_s as i32)) }
    }

    pub fn round(&self, id: &str) -> Option<&Round> {
        self.rounds.iter().find(|r| r.id == id)
    }

    /// The rounds of a cat.
    pub fn rounds_of(&self, cat: &str) -> Vec<&Round> {
        self.rounds.iter().filter(|r| r.cat == cat).collect()
    }

    /// The soonest round that will run by itself: for Home's quiet line.
    pub fn next_round(&self) -> Option<&Round> {
        self.rounds.iter().filter(|r| r.next_due_ms.is_some()).min_by_key(|r| r.next_due_ms)
    }

    /// The Rounds page (⌘3, the sidebar's row).
    pub fn open_rounds(&mut self) {
        self.go_home();
        self.rounds_open = true;
    }

    fn round_request(&mut self, r: Request, asked: Asked) {
        if let Some(id) = self.send(r) {
            self.asked.insert(id, asked);
        }
    }

    /// New round… (the page's button, a cat's page): a blank form for `cat` (else the cat chosen in Home's composer, else the first).
    pub fn start_new_round(&mut self, cat: Option<&str>) {
        let cat = cat.map(str::to_owned).or_else(|| self.cat_pick().map(str::to_owned)).or_else(|| self.active_cats().first().map(|c| c.id.clone())).unwrap_or_default();
        let ws = if self.chosen().is_some_and(neko_protocol::Workspace::is_active) { self.workspace.clone() } else { GENERAL.to_owned() };
        self.menu_open = None;
        self.round_form = Some(RoundForm::blank(&cat, &ws));
    }

    pub fn start_edit_round(&mut self, id: &str) {
        self.menu_open = None;
        if let Some(r) = self.round(id) {
            self.round_form = Some(RoundForm::of(r));
        }
    }

    pub fn form_mut(&mut self) -> &mut RoundForm {
        self.round_form.as_mut().expect("the round form is open")
    }

    /// Save in the panel: the whole round goes to the daemon; it closes when the daemon accepts it.
    pub fn save_round(&mut self) {
        let Some(f) = self.round_form.clone() else { return };
        let Ok(schedule) = f.schedule() else { return };
        if !f.ready() {
            return;
        }
        let workspace = Some(f.workspace.clone()).filter(|w| w != GENERAL);
        let request = match &f.id {
            None => Request::CreateRound { name: f.name.text.clone(), words: f.words.text.clone(), cat: f.cat.clone(), workspace, schedule, on: true },
            Some(id) => Request::UpdateRound { round: id.clone(), name: f.name.text.clone(), words: f.words.text.clone(), cat: f.cat.clone(), workspace, schedule },
        };
        self.round_request(request, Asked::RoundForm);
    }

    pub fn set_round_on(&mut self, id: &str, on: bool) {
        self.round_request(Request::SetRoundOn { round: id.to_owned(), on }, Asked::Round);
    }

    pub fn run_round_now(&mut self, id: &str) {
        self.round_request(Request::RunRoundNow { round: id.to_owned() }, Asked::RoundRun);
    }

    pub fn set_rounds_paused(&mut self, paused: bool) {
        self.round_request(Request::SetRoundsPaused { paused }, Asked::Round);
    }

    pub fn ask_remove_round(&mut self, id: &str) {
        self.menu_open = None;
        self.confirm = Some(Confirm::RemoveRound(id.to_owned()));
    }

    pub fn remove_round(&mut self, id: &str) {
        self.round_request(Request::RemoveRound { round: id.to_owned() }, Asked::Round);
    }

    /// A round's history line list opens or closes; opening asks the daemon for it.
    pub fn toggle_round_history(&mut self, id: &str) {
        if !self.round_history_open.remove(id) {
            self.round_history_open.insert(id.to_owned());
            self.round_request(Request::RoundHistory { round: id.to_owned() }, Asked::RoundHistory(id.to_owned()));
        }
    }

    /// The status of a task a round made, for a history line.
    pub fn task_status(&self, id: &TaskId) -> Option<TaskStatus> {
        self.tasks.iter().find(|t| &t.id == id).map(|t| t.status)
    }

    /// Keeps the list as the daemon sent it: the round added or replaced whole, in the order the daemon lists them.
    pub fn upsert_round(&mut self, r: Round) {
        match self.rounds.iter_mut().find(|x| x.id == r.id) {
            Some(x) => *x = r,
            None => {
                self.rounds.push(r);
                self.rounds.sort_by_key(|x| (x.created_ms, x.id.clone()));
            }
        }
    }

    /// A round's history changed (it ran, was skipped): the list that is open follows.
    pub fn refresh_round_history(&mut self, id: &str) {
        if self.round_history_open.contains(id) {
            self.round_request(Request::RoundHistory { round: id.to_owned() }, Asked::RoundHistory(id.to_owned()));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::schedule::days_from_civil;

    fn at(d: u32, h: u32, m: u32) -> i64 {
        (days_from_civil(2026, 11, d) * 86_400 + i64::from(h) * 3600 + i64::from(m) * 60) * 1000
    }

    #[test]
    fn a_schedule_reads_as_a_person_says_it() {
        let d = |days: &[Weekday]| schedule_words(&Schedule::Weekdays { days: days.to_vec(), at: "09:00".into() });
        assert_eq!(d(&[Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri]), "Weekdays at 09:00");
        assert_eq!(d(&[Weekday::Sat, Weekday::Sun]), "Weekends at 09:00");
        assert_eq!(d(&Weekday::ALL), "Every day at 09:00");
        assert_eq!(d(&[Weekday::Mon, Weekday::Wed, Weekday::Fri]), "Mon, Wed, Fri at 09:00");
        assert_eq!(d(&[Weekday::Sun]), "Sundays at 09:00");
        assert_eq!(schedule_words(&Schedule::Daily { at: "07:15".into() }), "Every day at 07:15");
        assert_eq!(schedule_words(&Schedule::Every { hours: 1 }), "Every hour");
        assert_eq!(schedule_words(&Schedule::Every { hours: 6 }), "Every 6 hours");
        assert_eq!(schedule_words(&Schedule::Manual), "Only when you press Run now");
    }

    #[test]
    fn a_time_reads_relative_to_now() {
        let z = Fixed(0);
        let now = at(26, 14, 5);
        assert_eq!(when(at(26, 18, 0), now, &z), "today 18:00");
        assert_eq!(when(at(27, 9, 0), now, &z), "tomorrow 09:00");
        assert_eq!(when(at(29, 9, 0), now, &z), "Sunday 09:00");
        assert_eq!(when(at(30, 9, 0), now, &z), "Monday 09:00");
        assert_eq!(when(at(3, 9, 0) + 30 * 86_400_000, now, &z), "3 Dec 09:00");
        assert_eq!(when_at(at(27, 9, 0), now, &z), "tomorrow at 09:00");
    }

    #[test]
    fn the_form_says_when_the_first_run_will_be_before_it_is_saved() {
        let z = Fixed(0);
        let now = at(26, 14, 5);
        let mut f = RoundForm::blank("cat_mochi", GENERAL);
        assert_eq!(first_run_words(&f, now, &z), "The first run is tomorrow at 09:00.", "weekdays at 09:00, asked on a Thursday afternoon");
        f.kind = 0;
        f.time = TextEdit::new("18:30");
        assert_eq!(first_run_words(&f, now, &z), "The first run is today at 18:30.");
        f.kind = 2;
        assert_eq!(first_run_words(&f, now, &z), "The first run is today at 20:05, then every 6 hours.");
        f.kind = 3;
        assert!(first_run_words(&f, now, &z).starts_with("It never starts by itself"));
        f.kind = 1;
        f.days = [false; 7];
        assert_eq!(first_run_words(&f, now, &z), "Choose at least one day.");
        assert!(!f.ready());
        f.kind = 0;
        f.time = TextEdit::new("9am");
        assert!(first_run_words(&f, now, &z).contains("like 09:00"));
    }

    #[test]
    fn a_form_is_ready_only_with_a_name_words_and_a_schedule() {
        let mut f = RoundForm::blank("cat_mochi", GENERAL);
        assert!(!f.ready());
        f.name = TextEdit::new("Docs check");
        f.words = TextEdit::new("Check the docs");
        assert!(f.ready());
        f.kind = 2;
        f.hours = TextEdit::new("25");
        assert!(!f.ready());
        f.hours = TextEdit::new("24");
        assert_eq!(f.schedule(), Ok(Schedule::Every { hours: 24 }));
    }
}
