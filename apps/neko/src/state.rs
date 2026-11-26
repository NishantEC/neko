//! What the window holds, and what each thing the person or the daemon does changes in it. No drawing here and no I/O: the link
//! is reached only through [`App::send`].

use std::collections::{HashMap, HashSet};

use neko_protocol::{
    Change, Decision, Part, PartBody, Permission, PermissionId, PermissionMode, Reply, Request, Run, RunStatus, RuntimeInfo, ServerMessage, Settings, Task, TaskId, Transcript,
};
use rune::TextEdit;

use crate::link::{Conn, Incoming};
use crate::window_place;

/// Under this window width the sidebar folds by itself (points).
pub const FOLD_BELOW: f32 = 1000.0;

/// How the app stands with its daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Daemon {
    /// Not connected yet (just launched, or the daemon is starting).
    Starting,
    Connected,
    /// The connection ended; the link is trying again. The words say why.
    Lost(String),
    /// A picture or a test with no daemon at all.
    Absent,
}

pub struct App {
    /// The default workspace's tasks, newest first, as the daemon last said.
    pub tasks: Vec<Task>,
    /// The open task; None is Home.
    pub open: Option<TaskId>,
    /// The words in Home's composer.
    pub draft: TextEdit,
    pub daemon: Daemon,
    pub conn: Option<Conn>,
    /// The request that is creating a task from these words (kept until the daemon answers, then cleared or put back).
    pub creating: Option<(u64, String)>,
    /// One quiet line under the composer: why the last task could not be added.
    pub notice: Option<String>,
    /// The person's own choice of sidebar (⌃⌘S), until the window crosses [`FOLD_BELOW`] again.
    pub sidebar_choice: Option<bool>,
    /// The window's width at the last frame (None before the first layout).
    pub width: Option<f32>,
    /// Now, for "Today" and the greeting: milliseconds since the epoch, and the local zone's offset from UTC in seconds.
    pub now_ms: i64,
    pub utc_offset_s: i64,
    pub clock: fn() -> (i64, i64),
    /// The window's frame keeper (`window_place.rs`); `persist` only in the real window.
    pub window: window_place::Keeper,
    pub persist: bool,
    /// The title the window was last given.
    pub title_set: String,
    /// What the menu bar last showed (sidebar shown, full screen), so it is replaced only when that changes.
    pub menu_shown: Option<(bool, bool)>,
    pub full_screen: bool,
    /// Until when (seconds of the window's clock) the raw menu fix-ups keep running after a menu change.
    pub menu_fix_until: f64,
    pub handle: Option<rune::AppHandle<App>>,
    /// Each task's runs, parts and requests, as the daemon last said (loaded when the task is opened, kept by events).
    pub books: HashMap<TaskId, Book>,
    /// The words in each task's own composer.
    pub replies: HashMap<TaskId, TextEdit>,
    /// Folds the person opened (by key).
    pub open_folds: HashSet<String>,
    /// Whether the task's conversation shows its newest content (it reports this).
    pub at_bottom: bool,
    /// The folder Home's composer will give a new task (None: a scratch folder).
    pub folder: Option<String>,
    /// Settings (⌘,) shows instead of the content.
    pub settings_open: bool,
    /// The agent CLIs and the settings, as the daemon last said.
    pub runtimes: Option<(Vec<RuntimeInfo>, Settings)>,
    /// The person's home folder, so paths under it read `~/…` (None: shown whole).
    pub home: Option<String>,
    /// Whether each message's actions show (pointer over it or focus in it), by the message's key (rune-ui's `reveal_actions`).
    pub reveals: HashMap<String, crate::ui::message::Reveal>,
    /// The context ring's panel is open (in the open task's composer).
    pub context_open: bool,
    /// The task whose words stream to this window.
    pub watching: Option<TaskId>,
    /// Requests sent and what their reply is for.
    pub asked: HashMap<u64, Asked>,
    /// Something to say to assistive technology at the next frame (the words, and whether it interrupts).
    pub announce: Option<(String, bool)>,
    /// Return was pressed in Home before the daemon answered: the words in the composer are sent as soon as it does.
    pub queued: bool,
    /// The daemon's list of tasks has arrived (until then the sidebar says it is loading).
    pub tasks_loaded: bool,
    /// Since when the app has waited for its daemon (None: connected, or no daemon at all).
    pub waiting_since: Option<std::time::Instant>,
    /// How long the app waits for its daemon before it says so and offers Retry.
    pub slow_after: std::time::Duration,
    /// The slow line was drawn (so the frame that crosses the wait redraws once).
    pub slow_shown: bool,
}

/// How long the app waits for its daemon before a calm line says so, with Retry.
pub const SLOW_AFTER: std::time::Duration = std::time::Duration::from_secs(6);

/// What a sent request's reply is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// A follow-up: its words go back in the box if it is refused.
    FollowUp(TaskId, String),
    Other,
}

/// A task's transcript as the window holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Book {
    pub runs: Vec<Run>,
    pub parts: Vec<Part>,
    pub permissions: HashMap<PermissionId, Permission>,
    /// The daemon's whole transcript arrived (until then, events only add to what is there).
    pub loaded: bool,
    /// The agent is thinking now (the daemon says so as it happens; never stored).
    pub thinking: bool,
}

impl Book {
    pub fn from(t: Transcript) -> Book {
        Book { runs: t.runs, parts: t.parts, permissions: t.permissions.into_iter().map(|p| (p.id.clone(), p)).collect(), loaded: true, thinking: false }
    }

    fn run(&mut self, r: Run) {
        match self.runs.iter_mut().find(|x| x.id == r.id) {
            Some(x) => *x = r,
            None => {
                self.runs.push(r);
                self.runs.sort_by(|a, b| (a.started_ms, &a.id.0).cmp(&(b.started_ms, &b.id.0)));
            }
        }
    }

    fn part(&mut self, p: Part) {
        match self.parts.iter_mut().find(|x| x.id == p.id) {
            Some(x) => *x = p,
            None => {
                let at = self.parts.iter().position(|x| x.seq > p.seq).unwrap_or(self.parts.len());
                self.parts.insert(at, p);
            }
        }
    }

    fn delta(&mut self, part: &neko_protocol::PartId, more: &str) {
        if let Some(p) = self.parts.iter_mut().find(|x| &x.id == part) {
            match &mut p.body {
                PartBody::Agent { text, .. } | PartBody::Thinking { text, .. } => text.push_str(more),
                _ => {}
            }
        }
    }

    /// The run in progress, if any.
    pub fn running(&self) -> Option<&Run> {
        self.runs.iter().rev().find(|r| r.status == RunStatus::Running)
    }

    /// What the task's runs cost so far, as reported (None when none reported a cost).
    pub fn cost(&self) -> Option<f64> {
        self.runs.iter().filter_map(|r| r.usage.as_ref().and_then(|u| u.cost_usd)).fold(None, |a, c| Some(a.unwrap_or(0.0) + c))
    }
}

/// The real clock: now and the local offset.
pub fn system_clock() -> (i64, i64) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_millis() as i64);
    (now, crate::clock::local_offset_s(now / 1000))
}

impl Default for App {
    fn default() -> Self {
        App {
            tasks: Vec::new(),
            open: None,
            draft: TextEdit::default(),
            daemon: Daemon::Absent,
            conn: None,
            creating: None,
            notice: None,
            sidebar_choice: None,
            width: None,
            now_ms: 0,
            utc_offset_s: 0,
            clock: || (0, 0),
            window: window_place::Keeper::default(),
            persist: false,
            title_set: String::new(),
            menu_shown: None,
            full_screen: false,
            menu_fix_until: 0.0,
            handle: None,
            books: HashMap::new(),
            replies: HashMap::new(),
            open_folds: HashSet::new(),
            at_bottom: true,
            folder: None,
            settings_open: false,
            runtimes: None,
            home: None,
            reveals: HashMap::new(),
            context_open: false,
            watching: None,
            asked: HashMap::new(),
            announce: None,
            queued: false,
            tasks_loaded: false,
            waiting_since: None,
            slow_after: SLOW_AFTER,
            slow_shown: false,
        }
    }
}

/// Local midnight of the day `now_ms` is in, in milliseconds since the epoch.
pub fn midnight(now_ms: i64, utc_offset_s: i64) -> i64 {
    let local = now_ms / 1000 + utc_offset_s;
    (local.div_euclid(86_400) * 86_400 - utc_offset_s) * 1000
}

impl App {
    /// The open task, if it is one the daemon has.
    pub fn open_task(&self) -> Option<&Task> {
        self.open.as_ref().and_then(|id| self.tasks.iter().find(|t| &t.id == id))
    }

    /// What the window's title says: the open task's title, Settings, else "Neko".
    pub fn window_title(&self) -> String {
        if self.settings_open {
            return "Settings".to_owned();
        }
        self.open_task().map_or_else(|| "Neko".to_owned(), |t| t.title.clone())
    }

    /// The open task's transcript.
    pub fn book(&self) -> Option<&Book> {
        self.open.as_ref().and_then(|id| self.books.get(id))
    }

    /// Whether the open task's agent is at work.
    pub fn open_is_working(&self) -> bool {
        self.open_task().is_some_and(|t| t.status.is_active()) || self.book().is_some_and(|b| b.running().is_some())
    }

    /// The open task's composer.
    pub fn reply_mut(&mut self) -> &mut TextEdit {
        let id = self.open.clone().unwrap_or(TaskId(String::new()));
        self.replies.entry(id).or_default()
    }

    pub fn reply_box(&self) -> Option<&TextEdit> {
        self.open.as_ref().and_then(|id| self.replies.get(id))
    }

    /// Opens or shuts a fold.
    pub fn flip(&mut self, key: &str) {
        if !self.open_folds.remove(key) {
            self.open_folds.insert(key.to_owned());
        }
    }

    /// Whether a message's actions show now.
    pub fn reveal(&self, key: &str) -> crate::ui::message::Reveal {
        self.reveals.get(key).copied().unwrap_or_default()
    }

    pub fn is_open(&self, key: &str) -> bool {
        self.open_folds.contains(key)
    }

    /// Asks for the open task's transcript and its streaming words.
    fn follow_open(&mut self) {
        let want = self.open.clone();
        if self.watching != want {
            self.send(Request::WatchTask { task: want.clone() });
            self.watching = want.clone();
        }
        if let Some(id) = want {
            self.send(Request::GetTranscript { task: id });
        }
    }

    /// Return in a task's composer: the words continue the conversation.
    pub fn follow_up(&mut self) {
        let Some(task) = self.open.clone() else { return };
        let words = self.reply_box().map(|r| r.text.clone()).unwrap_or_default();
        if words.trim().is_empty() || self.open_is_working() {
            return;
        }
        if let Some(id) = self.send(Request::FollowUp { task: task.clone(), words: words.clone() }) {
            self.asked.insert(id, Asked::FollowUp(task.clone(), words));
            self.replies.insert(task, TextEdit::default());
            self.at_bottom = true;
        }
    }

    /// Stop (the composer's button, Escape): the open task's agent stops.
    pub fn stop(&mut self) {
        if let Some(task) = self.open.clone() {
            self.send(Request::StopTask { task });
        }
    }

    /// An answer to a permission request.
    pub fn decide(&mut self, permission: &PermissionId, decision: Decision) {
        self.send(Request::AnswerPermission { permission: permission.clone(), decision });
    }

    /// The folder new tasks get (None: a scratch folder). Remembered for the next launch in the real window.
    pub fn choose_folder(&mut self, folder: Option<String>) {
        self.folder = folder.clone();
        if self.persist {
            let _ = std::thread::Builder::new().name("neko-prefs".into()).spawn(move || {
                let mut prefs = rune::prefs::Prefs::open(crate::PREFS_ID);
                let _ = match folder {
                    Some(f) => prefs.set(crate::FOLDER_KEY, &f),
                    None => prefs.set(crate::FOLDER_KEY, &String::new()),
                };
            });
        }
    }

    /// Settings (⌘,).
    pub fn open_settings(&mut self) {
        self.settings_open = true;
        self.send(Request::ListRuntimes);
    }

    pub fn set_permission_mode(&mut self, mode: PermissionMode) {
        if let Some((_, s)) = &self.runtimes {
            let settings = Settings { permission_mode: mode, ..s.clone() };
            self.send(Request::SetSettings { settings });
        }
    }

    pub fn set_runtime(&mut self, id: &str) {
        if let Some((_, s)) = &self.runtimes {
            let settings = Settings { runtime: id.to_owned(), ..s.clone() };
            self.send(Request::SetSettings { settings });
        }
    }

    /// Whether the sidebar shows: the person's choice, else wide windows only.
    pub fn sidebar_shown(&self) -> bool {
        self.sidebar_choice.unwrap_or(self.width.is_none_or(|w| w >= FOLD_BELOW))
    }

    pub fn toggle_sidebar(&mut self) {
        self.sidebar_choice = Some(!self.sidebar_shown());
    }

    /// A new width: crossing [`FOLD_BELOW`] gives the sidebar back to the window's own rule.
    pub fn set_width(&mut self, w: f32) {
        let crossed = self.width.is_some_and(|old| (old >= FOLD_BELOW) != (w >= FOLD_BELOW));
        if crossed {
            self.sidebar_choice = None;
        }
        self.width = Some(w);
    }

    pub fn go_home(&mut self) {
        self.open = None;
        self.context_open = false;
        self.settings_open = false;
        self.follow_open();
    }

    /// New Task (⌘N, the sidebar's button): Home, with the keyboard in the composer.
    pub fn go_home_and_focus(&mut self) {
        self.go_home();
        self.draft.focused = true;
    }

    pub fn open_task_id(&mut self, id: &TaskId) {
        self.open = Some(id.clone());
        self.context_open = false;
        self.settings_open = false;
        self.at_bottom = true;
        self.follow_open();
    }

    /// The link is set up (after the first frame, `lib.rs`): the app waits for the daemon from now.
    pub fn link_started(&mut self, conn: Conn) {
        self.conn = Some(conn);
        if self.daemon != Daemon::Connected {
            self.daemon = Daemon::Starting;
            self.waiting_since = Some(std::time::Instant::now());
        }
    }

    /// The daemon has not answered for longer than [`App::slow_after`]: a calm line says so, with Retry.
    pub fn daemon_slow(&self) -> bool {
        matches!(self.daemon, Daemon::Starting | Daemon::Lost(_)) && self.waiting_since.is_some_and(|at| at.elapsed() >= self.slow_after)
    }

    /// Retry: the link starts the daemon again now, and the wait starts over.
    pub fn retry_link(&mut self) {
        if let Some(c) = &self.conn {
            c.retry();
            self.waiting_since = Some(std::time::Instant::now());
        }
    }

    /// Sends a request; None when there is no daemon to send it to.
    pub fn send(&self, r: Request) -> Option<u64> {
        self.conn.as_ref().and_then(|c| c.send(r))
    }

    /// Return in the composer: the words go to the daemon. They leave the box at once and come back if the daemon refuses them.
    pub fn submit(&mut self) {
        let words = self.draft.text.clone();
        if words.trim().is_empty() || self.creating.is_some() {
            return;
        }
        match self.send(Request::CreateTask { words: words.clone(), folder: self.folder.clone() }) {
            Some(id) => {
                self.creating = Some((id, words));
                self.draft = TextEdit::default();
                self.notice = None;
                self.queued = false;
            }
            // The link is coming up: the words stay in the box and go as soon as the daemon answers.
            None if self.conn.is_some() => {
                self.queued = true;
                self.notice = Some("Neko is starting. Your task is sent as soon as it is ready.".to_owned());
            }
            None => self.notice = Some("Neko is not running. Your words are kept.".to_owned()),
        }
    }

    /// Adds or replaces a task, keeping newest first.
    fn upsert(&mut self, mut task: Task) {
        // One short line, whatever the store holds (a title kept from before titles were cut): the sidebar, the band and the
        // window's title all read this.
        task.title = neko_protocol::title::title(&task.title);
        self.tasks.retain(|t| t.id != task.id);
        let at = self.tasks.iter().position(|t| (t.created_ms, &t.id) < (task.created_ms, &task.id)).unwrap_or(self.tasks.len());
        self.tasks.insert(at, task);
    }

    /// One thing from the link.
    pub fn take(&mut self, i: Incoming) {
        match i {
            Incoming::Connected(_) => {
                self.daemon = Daemon::Connected;
                self.waiting_since = None;
                self.send(Request::Subscribe);
                self.send(Request::ListTasks);
                self.send(Request::ListRuntimes);
                // A new connection watches nothing yet.
                self.watching = None;
                self.follow_open();
                if std::mem::take(&mut self.queued) {
                    self.submit();
                }
            }
            Incoming::Lost(why) => {
                self.daemon = Daemon::Lost(why);
                self.waiting_since.get_or_insert_with(std::time::Instant::now);
                // An answer that will never come: the words go back in the box.
                if let Some((_, words)) = self.creating.take()
                    && self.draft.text.is_empty()
                {
                    self.draft = TextEdit::new(&words);
                }
            }
            Incoming::Message(ServerMessage::Event { change, .. }) => self.change(change),
            Incoming::Message(ServerMessage::Reply { id, reply }) => self.reply(id, reply),
        }
    }

    /// A change the daemon pushed.
    pub fn change(&mut self, change: Change) {
        match change {
            Change::TaskCreated { task } => self.upsert(task),
            Change::TaskUpdated { task } => {
                let before = self.tasks.iter().find(|t| t.id == task.id).map(|t| t.status);
                if before != Some(task.status) {
                    self.say_status(&task);
                }
                self.upsert(task);
            }
            Change::RunChanged { run } => self.books.entry(run.task.clone()).or_default().run(run),
            Change::PartChanged { part } => self.books.entry(part.task.clone()).or_default().part(part),
            Change::PartDelta { task, part, text } => {
                if let Some(b) = self.books.get_mut(&task) {
                    b.delta(&part, &text);
                }
            }
            Change::PermissionChanged { permission } => {
                self.books.entry(permission.task.clone()).or_default().permissions.insert(permission.id.clone(), permission);
            }
            Change::Activity { task, thinking } => {
                if let Some(b) = self.books.get_mut(&task) {
                    b.thinking = thinking;
                }
            }
            Change::SettingsChanged { settings } => {
                if let Some((_, s)) = &mut self.runtimes {
                    *s = settings;
                }
            }
        }
    }

    /// Tells assistive technology what a task's new state means, when it matters.
    fn say_status(&mut self, task: &Task) {
        use neko_protocol::TaskStatus as S;
        let words = match task.status {
            S::NeedsYou => Some((format!("Neko needs you: {}", task.title), true)),
            S::Asked => Some((format!("Neko has a question for you: {}", task.title), true)),
            S::Done => Some((format!("Neko finished: {}", task.title), false)),
            S::Failed => Some((format!("Neko could not finish: {}", task.title), false)),
            S::Stopped => Some((format!("Stopped: {}", task.title), false)),
            _ => None,
        };
        if let Some(w) = words {
            self.announce = Some(w);
        }
    }

    fn reply(&mut self, id: u64, reply: Reply) {
        let mine = self.creating.as_ref().is_some_and(|(c, _)| *c == id);
        let asked = self.asked.remove(&id);
        match reply {
            Reply::Transcript { transcript } => {
                let id = transcript.task.id.clone();
                self.upsert(transcript.task.clone());
                // Words that streamed in between are in the stored part or arrive as deltas after it: the whole replaces.
                self.books.insert(id, Book::from(transcript));
                return;
            }
            Reply::Runtimes { runtimes, settings } => {
                self.runtimes = Some((runtimes, settings));
                return;
            }
            Reply::Settings { settings } => {
                if let Some((_, s)) = &mut self.runtimes {
                    *s = settings;
                }
                return;
            }
            Reply::DayUsage { .. } => return,
            Reply::Error { message, .. } if matches!(asked, Some(Asked::FollowUp(..))) => {
                if let Some(Asked::FollowUp(task, words)) = asked {
                    let r = self.replies.entry(task).or_default();
                    if r.text.is_empty() {
                        *r = TextEdit::new(&words);
                    }
                }
                self.notice = Some(message);
                return;
            }
            _ => {}
        }
        match reply {
            Reply::Tasks { tasks } => {
                self.tasks_loaded = true;
                // A list answers from the store, so it replaces what the window had (a task made meanwhile comes as an event).
                for t in tasks {
                    self.upsert(t);
                }
            }
            Reply::Task { task } => {
                if mine {
                    self.creating = None;
                    let id = task.id.clone();
                    self.upsert(task);
                    self.open_task_id(&id);
                    return;
                }
                self.upsert(task);
            }
            Reply::Error { message, .. } if mine => {
                if let Some((_, words)) = self.creating.take()
                    && self.draft.text.is_empty()
                {
                    self.draft = TextEdit::new(&words);
                }
                self.notice = Some(message);
            }
            _ => {}
        }
    }

    /// Everything waiting in the link's inbox. True when something changed.
    pub fn drain_link(&mut self) -> bool {
        let got = self.conn.as_ref().map(Conn::drain).unwrap_or_default();
        let any = !got.is_empty();
        for i in got {
            self.take(i);
        }
        any
    }

    /// Today's tasks and the earlier ones, in the local calendar.
    pub fn grouped(&self) -> (Vec<&Task>, Vec<&Task>) {
        let day = |ms: i64| (ms / 1000 + self.utc_offset_s).div_euclid(86_400);
        let today = day(self.now_ms);
        self.tasks.iter().partition(|t| day(t.created_ms) >= today)
    }

    /// "Good morning", "Good afternoon" or "Good evening", by the local hour.
    pub fn greeting(&self) -> &'static str {
        match crate::clock::local_hour(self.now_ms, self.utc_offset_s) {
            5..=11 => "Good morning",
            12..=17 => "Good afternoon",
            _ => "Good evening",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::TaskStatus;

    fn task(id: &str, ms: i64) -> Task {
        Task { id: TaskId(id.into()), workspace: "default".into(), words: format!("words of {id}"), title: id.into(), created_ms: ms, status: TaskStatus::New, folder: None }
    }

    #[test]
    fn tasks_stay_newest_first_and_are_never_doubled() {
        let mut a = App::default();
        a.take(Incoming::Message(ServerMessage::Reply { id: 1, reply: Reply::Tasks { tasks: vec![task("b", 2), task("a", 1)] } }));
        a.take(Incoming::Message(ServerMessage::Event { revision: 3, change: Change::TaskCreated { task: task("c", 3) } }));
        a.take(Incoming::Message(ServerMessage::Event { revision: 3, change: Change::TaskCreated { task: task("c", 3) } }));
        assert_eq!(a.tasks.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
    }

    #[test]
    fn the_answer_to_my_words_opens_the_task_and_a_refusal_puts_them_back() {
        let mut a = App { creating: Some((7, "Fix the build".into())), ..App::default() };
        a.take(Incoming::Message(ServerMessage::Reply { id: 7, reply: Reply::Task { task: task("t", 5) } }));
        assert_eq!(a.open, Some(TaskId("t".into())));
        assert_eq!(a.window_title(), "t");
        let mut b = App { creating: Some((8, "Too long".into())), ..App::default() };
        b.take(Incoming::Message(ServerMessage::Reply { id: 8, reply: Reply::error(neko_protocol::ErrorCode::InvalidWords, "No.") }));
        assert_eq!(b.draft.text, "Too long");
        assert_eq!(b.notice.as_deref(), Some("No."));
        let mut c = App { creating: Some((9, "Kept".into())), ..App::default() };
        c.take(Incoming::Lost("gone".into()));
        assert_eq!(c.draft.text, "Kept", "a lost connection gives the words back");
    }

    #[test]
    fn a_task_title_is_one_short_line_everywhere() {
        let mut long = task("t", 5);
        long.title = "Look at the failing nightly build, find the cause, fix it, and then write up what happened\nLogs:\nerror".into();
        let mut a = App::default();
        a.take(Incoming::Message(ServerMessage::Reply { id: 1, reply: Reply::Tasks { tasks: vec![long] } }));
        a.open_task_id(&TaskId("t".into()));
        let title = a.window_title();
        assert!(!title.contains('\n') && title.chars().count() <= neko_protocol::title::MAX + 1, "{title}");
        assert_eq!(a.tasks[0].title, title, "the sidebar and the window agree");
    }

    #[test]
    fn without_a_daemon_the_words_stay_in_the_box() {
        let mut a = App { draft: TextEdit::new("Hello"), ..App::default() };
        a.submit();
        assert_eq!(a.draft.text, "Hello");
        assert!(a.notice.is_some());
        assert!(!a.queued, "no link at all: nothing to send it to later");
    }

    #[test]
    fn the_sidebar_folds_under_1000_and_the_choice_lasts_until_the_edge_is_crossed() {
        let mut a = App::default();
        assert!(a.sidebar_shown(), "before the first layout: shown");
        a.set_width(1200.0);
        assert!(a.sidebar_shown());
        a.set_width(900.0);
        assert!(!a.sidebar_shown(), "narrow: folded");
        a.toggle_sidebar();
        assert!(a.sidebar_shown(), "asked for");
        a.set_width(880.0);
        assert!(a.sidebar_shown(), "still narrow: the choice holds");
        a.set_width(1100.0);
        a.set_width(900.0);
        assert!(!a.sidebar_shown(), "crossing the edge gives it back to the rule");
    }

    #[test]
    fn today_is_the_local_calendar_day() {
        // 2026-11-26 10:00 UTC, in a zone 5 hours behind: 05:00 local.
        let now = 1_795_687_200_000;
        let a = App { now_ms: now, utc_offset_s: -5 * 3600, tasks: vec![task("now", now), task("local-yesterday", now - 6 * 3_600_000)], ..App::default() };
        let (today, earlier) = a.grouped();
        assert_eq!(today.len(), 1);
        assert_eq!(earlier[0].title, "local-yesterday");
        assert_eq!(a.greeting(), "Good morning");
    }
}
