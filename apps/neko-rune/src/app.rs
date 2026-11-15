//! The app's state and what changes it. No drawing here (`view.rs`) and no I/O: the link thread
//! (`connection.rs`) sends [`LinkEvent`]s in and handlers call [`App::open_in_web`] through a
//! function pointer, so everything in this file is tested without a window or a daemon.

use std::io;
use std::time::Duration;

use rune::{TextEdit, Tx};
use rune_kit::shell::TabDragState;

use crate::link::{LinkError, LinkEvent, Summary, calm};
use crate::nav::Tabs;
use crate::pages::Page;
use crate::web_link::{WebTarget, open_url};

/// Where the connection to the daemon stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Connecting,
    Live,
    /// Down, with the calm sentence to show. The last workbench stays in [`App::summary`].
    Down { message: String },
}

/// ⌘K: go to a page. A stub in P0 (docs/decisions/0002-rune-for-everything.md, P4 gives it the
/// web UI's actions, tasks and search).
pub struct PaletteState {
    pub open: bool,
    pub query: TextEdit,
    pub selected: usize,
}

impl Default for PaletteState {
    fn default() -> Self {
        PaletteState { open: false, query: TextEdit::new(""), selected: 0 }
    }
}

/// One row of the palette.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Entry {
    GoTo(Page),
    OpenInWeb(Page),
}

impl Entry {
    pub fn title(self) -> String {
        match self {
            Entry::GoTo(page) => page.title().to_owned(),
            Entry::OpenInWeb(page) => format!("Open {} in the web UI", page.title()),
        }
    }
}

/// The palette's rows for a query: pages whose title contains it (any case), and, once there is
/// a query, the same pages in the web UI. With no query, every page.
pub fn entries(query: &str) -> Vec<Entry> {
    let needle = query.trim().to_lowercase();
    let matches = |page: &Page| needle.is_empty() || page.title().to_lowercase().contains(&needle) || page.id().contains(&needle);
    let mut rows: Vec<Entry> = Page::RAIL.iter().chain([&Page::FOOT]).chain(Page::MORE.iter()).filter(|p| matches(p)).map(|p| Entry::GoTo(*p)).collect();
    if !needle.is_empty() {
        rows.extend(Page::ALL.iter().filter(|p| matches(p)).map(|p| Entry::OpenInWeb(*p)));
    }
    rows
}

pub struct App {
    pub tabs: Tabs,
    pub tab_drag: TabDragState,
    pub link: LinkState,
    /// The last workbench the daemon sent; kept while the link is down.
    pub summary: Option<Summary>,
    /// Where the web UI is, once the link thread has found the daemon's token.
    pub web: Option<WebTarget>,
    pub palette: PaletteState,
    /// The "More" menu is open.
    pub more_open: bool,
    /// One calm line under a button that could not do its job ("The web UI's address isn't known yet").
    pub notice: Option<String>,
    /// How a URL is opened. The browser by default; a no-op in headless scenes and tests.
    pub opener: fn(&str) -> io::Result<()>,
    /// Whether a daemon connection will be made at all (`nolink` turns it off for demos).
    pub linked: bool,
    /// Show the design lab (tokens and status glyphs) in place of the page.
    pub lab: bool,
}

impl Default for App {
    fn default() -> Self {
        App {
            tabs: Tabs::default(),
            tab_drag: TabDragState::new(),
            link: LinkState::Connecting,
            summary: None,
            web: None,
            palette: PaletteState::default(),
            more_open: false,
            notice: None,
            opener: open_url,
            linked: true,
            lab: false,
        }
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

    /// Opens `page` in the web UI. Says why in one calm line when it cannot.
    pub fn open_in_web(&mut self, page: Page) {
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

    pub fn open(&mut self, page: Page) {
        self.tabs.open(page);
        self.notice = None;
        self.more_open = false;
    }

    pub fn open_palette(&mut self) {
        self.palette = PaletteState { open: true, ..PaletteState::default() };
        self.palette.query.focused = true;
    }

    pub fn close_palette(&mut self) {
        self.palette.open = false;
    }

    /// Runs the palette's selected row.
    pub fn choose(&mut self, entry: Entry) {
        self.close_palette();
        match entry {
            Entry::GoTo(page) => self.open(page),
            Entry::OpenInWeb(page) => self.open_in_web(page),
        }
    }

    pub fn move_selection(&mut self, by: isize) {
        let n = entries(&self.palette.query.text).len();
        if n == 0 {
            self.palette.selected = 0;
            return;
        }
        self.palette.selected = (self.palette.selected as isize + by).rem_euclid(n as isize) as usize;
    }

    /// Called after the query changed: the selection returns to the top.
    pub fn query_changed(&mut self) {
        self.palette.selected = 0;
    }
}

/// The one line at the foot of every page: the link's state, then what the workbench says.
/// Leads with the status, says what needs the person or that nothing does (DESIGN.md §6).
pub fn status_line(link: &LinkState, summary: Option<&Summary>) -> String {
    let plural = |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let known = summary.map(|s| {
        let tasks = plural(s.tasks.len(), "task", "tasks");
        let need = s.needs_you();
        let tail = match need {
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

/// `retry_in` of a link that is down, for scenes.
pub fn down(error: LinkError, secs: u64) -> LinkEvent {
    LinkEvent::Down { error, attempt: 1, retry_in: Duration::from_secs(secs) }
}

/// ⌘K and the tab shortcuts. True when the key was taken.
pub fn on_key(s: &mut App, ev: &rune::KeyEvent, tx: &mut Tx) -> bool {
    use rune::Chord;
    let chord = |spec: &str| Chord::parse(spec).is_ok_and(|c| c.matches_tx(ev, tx));
    if chord("mod+k") {
        if s.palette.open { s.close_palette() } else { s.open_palette() }
        return true;
    }
    if s.palette.open {
        return false;
    }
    if chord("mod+[") {
        s.tabs.back();
        true
    } else if chord("mod+]") {
        s.tabs.forward();
        true
    } else if chord("mod+w") {
        let page = s.tabs.active();
        s.tabs.close(page);
        true
    } else {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::TaskRow;
    use neko_protocol::workbench::TaskStatus;

    fn summary(statuses: &[TaskStatus]) -> Summary {
        Summary {
            digest: format!("d{}", statuses.len()),
            tasks: statuses.iter().enumerate().map(|(i, s)| TaskRow { id: format!("t{i}"), key: None, title: format!("t{i}"), status: *s, updated_at_ms: 0 }).collect(),
        }
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
    fn open_in_web_opens_the_same_route_and_never_shows_the_token() {
        OPENED.with(|o| o.borrow_mut().clear());
        let token = "ab".repeat(32);
        let mut app = App { opener: record, web: Some(WebTarget::new(4747, &token)), ..App::headless() };
        app.open_in_web(Page::Inbox);
        let opened = OPENED.with(|o| o.borrow().clone());
        assert_eq!(opened, vec![format!("http://127.0.0.1:4747/?token={token}#/inbox")]);
        assert_eq!(app.notice, None);
    }

    #[test]
    fn open_in_web_says_why_when_it_cannot() {
        let mut app = App::headless();
        app.open_in_web(Page::Home);
        assert_eq!(app.notice.as_deref(), Some("Neko isn't running, so there's no web UI to open yet."));
        app.link = LinkState::Live;
        app.open_in_web(Page::Home);
        assert!(app.notice.as_deref().is_some_and(|n| n.contains("web address isn't known")));
        let mut failing = App { opener: |_| Err(io::Error::other("no browser")), web: Some(WebTarget::new(1, &"0".repeat(64))), ..App::headless() };
        failing.open_in_web(Page::Home);
        assert_eq!(failing.notice.as_deref(), Some("Couldn't open your browser."));
    }

    #[test]
    fn the_palette_filters_pages_and_offers_the_web_ui() {
        assert_eq!(entries("").len(), Page::ALL.len());
        assert_eq!(entries("")[0], Entry::GoTo(Page::Home));
        let inbox = entries("INB");
        assert_eq!(inbox, vec![Entry::GoTo(Page::Inbox), Entry::OpenInWeb(Page::Inbox)]);
        assert!(entries("zzzz").is_empty());
        assert_eq!(entries("pull")[0], Entry::GoTo(Page::PullRequests));
    }

    #[test]
    fn choosing_a_row_closes_the_palette_and_goes_there() {
        let mut app = App::headless();
        app.open_palette();
        assert!(app.palette.open && app.palette.query.focused);
        app.choose(Entry::GoTo(Page::Tasks));
        assert!(!app.palette.open);
        assert_eq!(app.tabs.active(), Page::Tasks);
    }

    #[test]
    fn the_selection_wraps() {
        let mut app = App::headless();
        app.open_palette();
        app.move_selection(-1);
        assert_eq!(app.palette.selected, Page::ALL.len() - 1);
        app.move_selection(1);
        assert_eq!(app.palette.selected, 0);
        app.palette.query.text = "zzzz".into();
        app.move_selection(1);
        assert_eq!(app.palette.selected, 0);
    }
}
