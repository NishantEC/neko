//! What the window holds, and what each thing the person or the daemon does changes in it. No drawing here and no I/O: the link
//! is reached only through [`App::send`].

use neko_protocol::{Change, Reply, Request, ServerMessage, Task, TaskId};
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
        }
    }
}

impl App {
    /// The open task, if it is one the daemon has.
    pub fn open_task(&self) -> Option<&Task> {
        self.open.as_ref().and_then(|id| self.tasks.iter().find(|t| &t.id == id))
    }

    /// What the window's title says: the open task's title, else "Neko".
    pub fn window_title(&self) -> String {
        self.open_task().map_or_else(|| "Neko".to_owned(), |t| t.title.clone())
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
    }

    /// New Task (⌘N, the sidebar's button): Home, with the keyboard in the composer.
    pub fn go_home_and_focus(&mut self) {
        self.open = None;
        self.draft.focused = true;
    }

    pub fn open_task_id(&mut self, id: &TaskId) {
        self.open = Some(id.clone());
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
        match self.send(Request::CreateTask { words: words.clone() }) {
            Some(id) => {
                self.creating = Some((id, words));
                self.draft = TextEdit::default();
                self.notice = None;
            }
            None => self.notice = Some("Neko is still starting. Your words are kept; press Return again in a moment.".to_owned()),
        }
    }

    /// Adds or replaces a task, keeping newest first.
    fn upsert(&mut self, task: Task) {
        self.tasks.retain(|t| t.id != task.id);
        let at = self.tasks.iter().position(|t| (t.created_ms, &t.id) < (task.created_ms, &task.id)).unwrap_or(self.tasks.len());
        self.tasks.insert(at, task);
    }

    /// One thing from the link.
    pub fn take(&mut self, i: Incoming) {
        match i {
            Incoming::Connected(_) => {
                self.daemon = Daemon::Connected;
                self.send(Request::Subscribe);
                self.send(Request::ListTasks);
            }
            Incoming::Lost(why) => {
                self.daemon = Daemon::Lost(why);
                // An answer that will never come: the words go back in the box.
                if let Some((_, words)) = self.creating.take()
                    && self.draft.text.is_empty()
                {
                    self.draft = TextEdit::new(&words);
                }
            }
            Incoming::Message(ServerMessage::Event { change: Change::TaskCreated { task }, .. }) => self.upsert(task),
            Incoming::Message(ServerMessage::Reply { id, reply }) => self.reply(id, reply),
        }
    }

    fn reply(&mut self, id: u64, reply: Reply) {
        let mine = self.creating.as_ref().is_some_and(|(c, _)| *c == id);
        match reply {
            Reply::Tasks { tasks } => {
                // A list answers from the store, so it replaces what the window had (a task made meanwhile comes as an event).
                for t in tasks {
                    self.upsert(t);
                }
            }
            Reply::Task { task } => {
                if mine {
                    self.creating = None;
                    self.open = Some(task.id.clone());
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
            Reply::Error { .. } | Reply::Hello { .. } | Reply::Subscribed { .. } => {}
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
        Task { id: TaskId(id.into()), workspace: "default".into(), words: format!("words of {id}"), title: id.into(), created_ms: ms, status: TaskStatus::New }
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
    fn without_a_daemon_the_words_stay_in_the_box() {
        let mut a = App { draft: TextEdit::new("Hello"), ..App::default() };
        a.submit();
        assert_eq!(a.draft.text, "Hello");
        assert!(a.notice.is_some());
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
