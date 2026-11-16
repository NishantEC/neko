//! The app's state and what changes it. No drawing here (`view.rs` and its region modules) and no
//! I/O: the link thread (`connection.rs`) sends [`LinkEvent`]s in and the buttons that leave the
//! app go through function pointers, so everything in this file is tested without a window or a
//! daemon.
//!
//! The window is four regions (docs/decisions/0002-rune-for-everything.md): the sessions column,
//! the session view (or a full view in its place), the context panel and the Neko column. P0
//! fills the sessions and the counts from the live snapshot; the rest are placeholders.

use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rune::TextEdit;

use crate::groups::{Group, first_session, grouped};
use crate::layout::Layout;
use crate::link::{LinkError, LinkEvent, Summary, TaskRow, calm};
use crate::views::FullView;
use crate::web_link::{WebTarget, open_url};

/// Where the connection to the daemon stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Live,
    /// Down, with the calm sentence to show. The last workbench stays in [`App::summary`].
    Down { message: String },
}

/// What a session produces, shown in the context panel (plugin slot (a): viewers by kind).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ContextKind {
    Diff,
    Files,
    Terminal,
    Browser,
    Screen,
    Canvas,
}

impl ContextKind {
    pub const ALL: [ContextKind; 6] = [ContextKind::Diff, ContextKind::Files, ContextKind::Terminal, ContextKind::Browser, ContextKind::Screen, ContextKind::Canvas];

    pub fn title(self) -> &'static str {
        match self {
            ContextKind::Diff => "Diff",
            ContextKind::Files => "Files",
            ContextKind::Terminal => "Terminal",
            ContextKind::Browser => "Browser",
            ContextKind::Screen => "Screen",
            ContextKind::Canvas => "Canvas",
        }
    }

    /// Browser and Screen are live views of what the agent is doing right now.
    pub fn live(self) -> bool {
        matches!(self, ContextKind::Browser | ContextKind::Screen)
    }

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|k| *k == self).unwrap_or(0)
    }
}

/// How a message to a running agent lands (`docs/knowledge/the-chat-directs-the-crew.md`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Steer {
    Now,
    AfterTurn,
    Interrupt,
}

impl Steer {
    pub const ALL: [Steer; 3] = [Steer::Now, Steer::AfterTurn, Steer::Interrupt];

    pub fn label(self) -> &'static str {
        match self {
            Steer::Now => "Steer now",
            Steer::AfterTurn => "After this turn",
            Steer::Interrupt => "Interrupt and restart",
        }
    }

    pub fn detail(self) -> &'static str {
        match self {
            Steer::Now => "The agent reads it at its next step",
            Steer::AfterTurn => "Waits for the turn to finish",
            Steer::Interrupt => "Stops the turn and starts again with it",
        }
    }
}

pub struct App {
    pub link: LinkState,
    /// The last workbench the daemon sent; kept while the link is down.
    pub summary: Option<Summary>,
    /// Where the web UI is, once the link thread has found the daemon's token.
    pub web: Option<WebTarget>,
    /// One calm line under a button that could not do its job.
    pub notice: Option<String>,
    /// How a URL is opened. The browser by default; a no-op in headless scenes and tests.
    pub opener: fn(&str) -> io::Result<()>,
    /// The time, in epoch milliseconds: the system's, or a fixed one in headless scenes.
    pub clock: fn() -> i64,
    /// When the last snapshot arrived, by `clock`: what "5m ago" counts from.
    pub now_ms: i64,
    /// Whether a daemon connection will be made at all (`nolink` turns it off for demos).
    pub linked: bool,
    /// Show the design lab (tokens and status glyphs) in place of the session.
    pub lab: bool,

    pub layout: Layout,
    /// The selected session's task id.
    pub selected: Option<String>,
    /// A full view in the session's place, or the session.
    pub view: Option<FullView>,
    /// Which of the three groups are collapsed, in `Group::ALL` order.
    pub collapsed: [bool; 3],
    pub context: ContextKind,
    pub steer: Steer,
    pub steer_open: bool,
    pub more_open: bool,
    pub composer: TextEdit,
    pub ask: TextEdit,
}

/// The system clock, in epoch milliseconds.
pub fn system_ms() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Default for App {
    fn default() -> Self {
        App {
            link: LinkState::Connecting,
            summary: None,
            web: None,
            notice: None,
            opener: open_url,
            clock: system_ms,
            now_ms: system_ms(),
            linked: true,
            lab: false,
            layout: Layout::default(),
            selected: None,
            view: None,
            collapsed: [false; 3],
            context: ContextKind::Diff,
            steer: Steer::AfterTurn,
            steer_open: false,
            more_open: false,
            composer: TextEdit::new(""),
            ask: TextEdit::new(""),
        }
    }
}

/// The time headless scenes pretend it is (a fixed instant, so a snapshot is a fact).
pub const DEMO_NOW: i64 = 1_800_000_000_000;

impl App {
    /// The state a headless run starts from: it never opens anything outside itself and its clock
    /// stands still.
    pub fn headless() -> App {
        App { opener: |_| Ok(()), clock: || DEMO_NOW, now_ms: DEMO_NOW, linked: false, ..App::default() }
    }

    pub fn apply_link(&mut self, event: LinkEvent) -> bool {
        match event {
            LinkEvent::Connecting => {
                let changed = self.link != LinkState::Connecting && self.summary.is_none();
                if self.summary.is_none() {
                    self.link = LinkState::Connecting;
                }
                changed
            }
            LinkEvent::Live(summary) => {
                let changed = self.link != LinkState::Live || self.summary.as_ref() != Some(&summary);
                self.link = LinkState::Live;
                self.now_ms = (self.clock)();
                // A window opens on what needs you; a selection that has gone is replaced.
                let gone = self.selected.as_ref().is_none_or(|id| !summary.tasks.iter().any(|t| &t.id == id));
                if gone {
                    self.selected = first_session(&summary, self.now_ms);
                }
                self.summary = Some(summary);
                changed
            }
            LinkEvent::Quiet => false,
            LinkEvent::Down { error, retry_in, .. } => {
                let state = LinkState::Down { message: calm(&error, retry_in) };
                let changed = self.link != state;
                self.link = state;
                changed
            }
        }
    }

    pub fn set_web(&mut self, web: Option<WebTarget>) -> bool {
        let changed = self.web != web;
        self.web = web;
        changed
    }

    /// The selected session's row, if the snapshot still has it.
    pub fn session(&self) -> Option<&TaskRow> {
        let id = self.selected.as_ref()?;
        self.summary.as_ref()?.tasks.iter().find(|t| &t.id == id)
    }

    /// Chooses a session: it shows in the middle, replacing any full view.
    pub fn select(&mut self, id: &str) {
        self.selected = Some(id.to_owned());
        self.view = None;
        self.notice = None;
    }

    pub fn open_view(&mut self, view: FullView) {
        self.view = Some(view);
        self.more_open = false;
        self.notice = None;
    }

    pub fn toggle_group(&mut self, group: Group) {
        let i = group.index();
        self.collapsed[i] = !self.collapsed[i];
    }

    /// Opens the web UI at `page` (its hash route id), home when none. Says why in one calm line
    /// when it cannot.
    pub fn open_in_web(&mut self, page: Option<&str>) {
        self.notice = None;
        let Some(web) = &self.web else {
            self.notice = Some(match self.link {
                LinkState::Live => "Neko is running, but its web address isn't known yet. Try again in a moment.".to_owned(),
                _ => "Neko isn't running, so there's no web UI to open yet.".to_owned(),
            });
            return;
        };
        if (self.opener)(&web.url(page)).is_err() {
            self.notice = Some("Couldn't open your browser.".to_owned());
        }
    }

    /// How many sessions wait on the person, for the Neko column and its collapsed icon.
    pub fn waiting(&self) -> usize {
        self.summary.as_ref().map_or(0, |s| grouped(s, self.now_ms).first().map_or(0, |(_, rows)| rows.len()))
    }
}

/// The Neko column's one-line crew status: who is working, then what needs the person.
pub fn crew_line(working: usize, waiting: usize) -> String {
    let work = match working {
        0 => "No agents working".to_owned(),
        1 => "1 agent working".to_owned(),
        n => format!("{n} agents working"),
    };
    let wait = match waiting {
        0 => "nothing needs you".to_owned(),
        1 => "1 needs you".to_owned(),
        n => format!("{n} need you"),
    };
    format!("{work} · {wait}")
}

/// The line at the Neko column's foot: the link's state, then what the workbench says. Leads with
/// the status, says what needs the person or that nothing does (DESIGN.md §6).
pub fn status_line(link: &LinkState, summary: Option<&Summary>) -> String {
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let known = summary.map(|s| plural(s.tasks.len(), "task", "tasks"));
    match (link, known) {
        (LinkState::Connecting, None) => "Connecting to Neko…".to_owned(),
        (LinkState::Connecting, Some(k)) => format!("Connecting to Neko… Last known: {k}"),
        (LinkState::Live, Some(k)) => format!("Connected · {k}"),
        (LinkState::Live, None) => "Connected".to_owned(),
        (LinkState::Down { message }, None) => message.clone(),
        (LinkState::Down { message }, Some(k)) => format!("{message} Last known: {k}"),
    }
}

/// A link that is down, for scenes.
pub fn down(error: LinkError, secs: u64) -> LinkEvent {
    LinkEvent::Down { error, attempt: 1, retry_in: Duration::from_secs(secs) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::TaskStatus;

    fn row(id: &str, status: TaskStatus, ago_ms: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: DEMO_NOW - ago_ms, goal: String::new() }
    }

    fn summary(rows: Vec<TaskRow>) -> Summary {
        Summary { digest: format!("d{}", rows.len()), runtime: String::new(), tasks: rows }
    }

    thread_local! { static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }

    fn record(url: &str) -> io::Result<()> {
        OPENED.with(|o| o.borrow_mut().push(url.to_owned()));
        Ok(())
    }

    #[test]
    fn the_link_goes_connecting_live_down_live() {
        let mut app = App::headless();
        let s = || summary(vec![row("a", TaskStatus::Building, 1000)]);
        assert_eq!(app.link, LinkState::Connecting);
        assert!(app.apply_link(LinkEvent::Live(s())));
        assert_eq!(app.link, LinkState::Live);
        // The same workbench again changes nothing, so the window does not redraw.
        assert!(!app.apply_link(LinkEvent::Live(s())));
        assert!(!app.apply_link(LinkEvent::Quiet));
        assert!(app.apply_link(down(LinkError::Dropped, 1)));
        assert!(matches!(&app.link, LinkState::Down { message } if message.contains("dropped")));
        assert_eq!(app.summary.as_ref().map(|s| s.tasks.len()), Some(1), "what was known stays");
        assert!(app.apply_link(LinkEvent::Live(s())));
        assert_eq!(app.link, LinkState::Live);
    }

    #[test]
    fn a_reconnect_after_having_data_does_not_flash_connecting() {
        let mut app = App::headless();
        app.apply_link(LinkEvent::Live(summary(vec![])));
        app.apply_link(down(LinkError::NotRunning, 1));
        app.apply_link(LinkEvent::Connecting);
        assert!(matches!(app.link, LinkState::Down { .. }));
    }

    #[test]
    fn a_window_opens_on_what_needs_you_and_keeps_your_choice() {
        let mut app = App::headless();
        app.apply_link(LinkEvent::Live(summary(vec![row("work", TaskStatus::Building, 1000), row("ask", TaskStatus::AwaitingApproval, 9000)])));
        assert_eq!(app.selected.as_deref(), Some("ask"));
        app.select("work");
        app.apply_link(LinkEvent::Live(summary(vec![row("work", TaskStatus::Building, 500), row("ask", TaskStatus::AwaitingApproval, 9000)])));
        assert_eq!(app.selected.as_deref(), Some("work"), "a new snapshot does not move the selection");
        app.apply_link(LinkEvent::Live(summary(vec![row("ask", TaskStatus::AwaitingApproval, 9000)])));
        assert_eq!(app.selected.as_deref(), Some("ask"), "a session that is gone is replaced");
        assert_eq!(app.session().map(|t| t.id.as_str()), Some("ask"));
    }

    #[test]
    fn choosing_a_session_replaces_a_full_view_and_choosing_a_view_replaces_the_session() {
        let mut app = App::headless();
        app.open_view(FullView::Settings);
        assert_eq!(app.view, Some(FullView::Settings));
        app.select("t1");
        assert_eq!(app.view, None);
        app.more_open = true;
        app.open_view(FullView::Usage);
        assert!(!app.more_open, "choosing closes the menu");
    }

    #[test]
    fn groups_collapse_one_at_a_time() {
        let mut app = App::headless();
        app.toggle_group(Group::Running);
        assert_eq!(app.collapsed, [false, true, false]);
        app.toggle_group(Group::Running);
        assert_eq!(app.collapsed, [false; 3]);
    }

    #[test]
    fn the_crew_line_says_who_works_and_what_needs_you() {
        assert_eq!(crew_line(3, 1), "3 agents working · 1 needs you");
        assert_eq!(crew_line(1, 2), "1 agent working · 2 need you");
        assert_eq!(crew_line(0, 0), "No agents working · nothing needs you");
    }

    #[test]
    fn waiting_counts_the_needs_you_group() {
        use TaskStatus::*;
        let mut app = App::headless();
        app.apply_link(LinkEvent::Live(summary(vec![row("a", AwaitingApproval, 1), row("b", ReadyForReview, 1), row("c", Failed, 1), row("d", Building, 1), row("e", Completed, 1)])));
        assert_eq!(app.waiting(), 3);
    }

    #[test]
    fn status_lines_lead_with_the_status() {
        let s = summary(vec![row("a", TaskStatus::Building, 1), row("b", TaskStatus::Building, 1)]);
        assert_eq!(status_line(&LinkState::Live, Some(&s)), "Connected · 2 tasks");
        assert_eq!(status_line(&LinkState::Connecting, None), "Connecting to Neko…");
        let down = LinkState::Down { message: "Neko isn't running. Trying again in 4 s.".into() };
        assert_eq!(status_line(&down, None), "Neko isn't running. Trying again in 4 s.");
        assert!(status_line(&down, Some(&s)).ends_with("Last known: 2 tasks"));
    }

    #[test]
    fn open_in_web_opens_the_same_place_and_never_shows_the_token() {
        OPENED.with(|o| o.borrow_mut().clear());
        let token = "ab".repeat(32);
        let mut app = App { opener: record, web: Some(WebTarget::new(4747, &token)), ..App::headless() };
        app.open_in_web(Some("schedules"));
        app.open_in_web(None);
        assert_eq!(OPENED.with(|o| o.borrow().clone()), vec![format!("http://127.0.0.1:4747/?token={token}#/schedules"), format!("http://127.0.0.1:4747/?token={token}")]);
        assert_eq!(app.notice, None);
    }

    #[test]
    fn open_in_web_says_why_when_it_cannot() {
        let mut app = App::headless();
        app.open_in_web(None);
        assert_eq!(app.notice.as_deref(), Some("Neko isn't running, so there's no web UI to open yet."));
        app.link = LinkState::Live;
        app.open_in_web(None);
        assert!(app.notice.as_deref().is_some_and(|n| n.contains("web address isn't known")));
        let mut failing = App { opener: |_| Err(io::Error::other("no browser")), web: Some(WebTarget::new(1, &"0".repeat(64))), ..App::headless() };
        failing.open_in_web(None);
        assert_eq!(failing.notice.as_deref(), Some("Couldn't open your browser."));
    }

    #[test]
    fn the_context_kinds_and_steer_modes_are_named() {
        assert_eq!(ContextKind::ALL.map(ContextKind::title), ["Diff", "Files", "Terminal", "Browser", "Screen", "Canvas"]);
        assert_eq!(ContextKind::ALL.iter().filter(|k| k.live()).count(), 2);
        assert_eq!(Steer::ALL.map(Steer::label), ["Steer now", "After this turn", "Interrupt and restart"]);
    }
}
