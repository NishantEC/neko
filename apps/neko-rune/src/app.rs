//! The app's state and what changes it. No drawing here (`view.rs`) and no I/O: the link thread
//! (`connection.rs`) sends [`LinkEvent`]s in and the one button calls [`App::open_in_web`]
//! through a function pointer, so everything in this file is tested without a window or a daemon.
//!
//! P0 has no layout to speak of: one plain view with the connection and a few live tasks. The
//! layout (sessions, an agent workspace, Neko as a side agent, contextual panels) is pending an
//! owner decision, so nothing here assumes tabs, a rail or pages.

use std::io;
use std::time::Duration;

use neko_protocol::workbench::TaskStatus;

use crate::link::{LinkError, LinkEvent, Summary, TaskRow, calm};
use crate::web_link::{WebTarget, open_url};

/// Where the connection to the daemon stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Live,
    /// Down, with the calm sentence to show. The last workbench stays in [`App::summary`].
    Down { message: String },
}

pub struct App {
    pub link: LinkState,
    /// The last workbench the daemon sent; kept while the link is down.
    pub summary: Option<Summary>,
    /// Where the web UI is, once the link thread has found the daemon's token.
    pub web: Option<WebTarget>,
    /// One calm line under the button when it could not do its job.
    pub notice: Option<String>,
    /// How a URL is opened. The browser by default; a no-op in headless scenes and tests.
    pub opener: fn(&str) -> io::Result<()>,
    /// Whether a daemon connection will be made at all (`nolink` turns it off for demos).
    pub linked: bool,
    /// Show the design lab (tokens and status glyphs) in place of the tasks.
    pub lab: bool,
}

impl Default for App {
    fn default() -> Self {
        App { link: LinkState::Connecting, summary: None, web: None, notice: None, opener: open_url, linked: true, lab: false }
    }
}

impl App {
    /// The state a headless run starts from: it never opens anything outside itself.
    pub fn headless() -> App {
        App { opener: |_| Ok(()), linked: false, ..App::default() }
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

    /// Opens the web UI. Says why in one calm line when it cannot.
    pub fn open_in_web(&mut self) {
        self.notice = None;
        let Some(web) = &self.web else {
            self.notice = Some(match self.link {
                LinkState::Live => "Neko is running, but its web address isn't known yet. Try again in a moment.".to_owned(),
                _ => "Neko isn't running, so there's no web UI to open yet.".to_owned(),
            });
            return;
        };
        if (self.opener)(&web.url()).is_err() {
            self.notice = Some("Couldn't open your browser.".to_owned());
        }
    }
}

/// How many tasks the placeholder shows.
pub const FEATURED: usize = 8;

/// What the person should see first: what needs them, then what is working, then review, failures,
/// the queue and finally the finished; newest first within each.
pub fn featured(summary: &Summary, n: usize) -> Vec<&TaskRow> {
    let rank = |s: TaskStatus| match s {
        TaskStatus::AwaitingApproval => 0,
        TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing => 1,
        TaskStatus::ReadyForReview => 2,
        TaskStatus::Failed => 3,
        TaskStatus::Queued => 4,
        TaskStatus::Completed => 5,
        TaskStatus::Cancelled => 6,
    };
    let mut rows: Vec<&TaskRow> = summary.tasks.iter().collect();
    rows.sort_by(|a, b| rank(a.status).cmp(&rank(b.status)).then(b.updated_at_ms.cmp(&a.updated_at_ms)));
    rows.truncate(n);
    rows
}

/// The one line at the foot: the link's state, then what the workbench says. Leads with the
/// status, says what needs the person or that nothing does (DESIGN.md §6).
pub fn status_line(link: &LinkState, summary: Option<&Summary>) -> String {
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let known = summary.map(|s| {
        let tasks = plural(s.tasks.len(), "task", "tasks");
        let tail = match s.needs_you() {
            0 => "nothing needs you right now".to_owned(),
            1 => "1 needs you".to_owned(),
            n => format!("{n} need you"),
        };
        format!("{tasks} · {tail}")
    });
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

    fn row(id: &str, status: TaskStatus, at: i64) -> TaskRow {
        TaskRow { id: id.into(), key: None, title: id.into(), status, updated_at_ms: at }
    }

    fn summary(statuses: &[TaskStatus]) -> Summary {
        Summary { digest: format!("d{}", statuses.len()), tasks: statuses.iter().enumerate().map(|(i, s)| row(&format!("t{i}"), *s, 0)).collect() }
    }

    thread_local! { static OPENED: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) }; }

    fn record(url: &str) -> io::Result<()> {
        OPENED.with(|o| o.borrow_mut().push(url.to_owned()));
        Ok(())
    }

    #[test]
    fn the_link_goes_connecting_live_down_live() {
        let mut app = App::headless();
        assert_eq!(app.link, LinkState::Connecting);
        assert!(app.apply_link(LinkEvent::Live(summary(&[TaskStatus::Building]))));
        assert_eq!(app.link, LinkState::Live);
        // The same workbench again changes nothing, so the window does not redraw.
        assert!(!app.apply_link(LinkEvent::Live(summary(&[TaskStatus::Building]))));
        assert!(!app.apply_link(LinkEvent::Quiet));
        assert!(app.apply_link(down(LinkError::Dropped, 1)));
        assert!(matches!(&app.link, LinkState::Down { message } if message.contains("dropped")));
        assert_eq!(app.summary.as_ref().map(|s| s.tasks.len()), Some(1), "what was known stays");
        assert!(app.apply_link(LinkEvent::Live(summary(&[TaskStatus::Building]))));
        assert_eq!(app.link, LinkState::Live);
    }

    #[test]
    fn a_reconnect_after_having_data_does_not_flash_connecting() {
        let mut app = App::headless();
        app.apply_link(LinkEvent::Live(summary(&[])));
        app.apply_link(down(LinkError::NotRunning, 1));
        app.apply_link(LinkEvent::Connecting);
        assert!(matches!(app.link, LinkState::Down { .. }));
    }

    #[test]
    fn status_lines_lead_with_the_status_and_say_what_needs_you() {
        let s = summary(&[TaskStatus::AwaitingApproval, TaskStatus::Building, TaskStatus::AwaitingApproval]);
        assert_eq!(status_line(&LinkState::Live, Some(&s)), "Connected · 3 tasks · 2 need you");
        let one = summary(&[TaskStatus::AwaitingApproval]);
        assert_eq!(status_line(&LinkState::Live, Some(&one)), "Connected · 1 task · 1 needs you");
        let calm_one = summary(&[TaskStatus::Completed]);
        assert_eq!(status_line(&LinkState::Live, Some(&calm_one)), "Connected · 1 task · nothing needs you right now");
        assert_eq!(status_line(&LinkState::Connecting, None), "Connecting to Neko…");
        let down = LinkState::Down { message: "Neko isn't running. Trying again in 4 s.".into() };
        assert_eq!(status_line(&down, None), "Neko isn't running. Trying again in 4 s.");
        assert!(status_line(&down, Some(&s)).ends_with("Last known: 3 tasks · 2 need you"));
    }

    #[test]
    fn the_first_rows_are_what_needs_you_then_what_is_working() {
        use TaskStatus::*;
        let s = Summary {
            digest: String::new(),
            tasks: vec![
                row("done", Completed, 9),
                row("queued", Queued, 8),
                row("old-ask", AwaitingApproval, 1),
                row("review", ReadyForReview, 7),
                row("new-ask", AwaitingApproval, 5),
                row("work", Building, 6),
                row("failed", Failed, 4),
            ],
        };
        let ids: Vec<&str> = featured(&s, 5).iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["new-ask", "old-ask", "work", "review", "failed"]);
        assert_eq!(featured(&s, 99).len(), 7);
    }

    #[test]
    fn open_in_web_opens_the_web_ui_and_never_shows_the_token() {
        OPENED.with(|o| o.borrow_mut().clear());
        let token = "ab".repeat(32);
        let mut app = App { opener: record, web: Some(WebTarget::new(4747, &token)), ..App::headless() };
        app.open_in_web();
        assert_eq!(OPENED.with(|o| o.borrow().clone()), vec![format!("http://127.0.0.1:4747/?token={token}")]);
        assert_eq!(app.notice, None);
    }

    #[test]
    fn open_in_web_says_why_when_it_cannot() {
        let mut app = App::headless();
        app.open_in_web();
        assert_eq!(app.notice.as_deref(), Some("Neko isn't running, so there's no web UI to open yet."));
        app.link = LinkState::Live;
        app.open_in_web();
        assert!(app.notice.as_deref().is_some_and(|n| n.contains("web address isn't known")));
        let mut failing = App { opener: |_| Err(io::Error::other("no browser")), web: Some(WebTarget::new(1, &"0".repeat(64))), ..App::headless() };
        failing.open_in_web();
        assert_eq!(failing.notice.as_deref(), Some("Couldn't open your browser."));
    }
}
