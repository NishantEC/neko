//! The places Neko has, in the web UI's order (`web/src/shell/AppSidebar.tsx`, DESIGN.md §8 rule 5):
//! the rail holds six (Home, Inbox, Tasks, Calendar, Workspaces, and Settings at its foot);
//! every other page is one click away in "More", in ⌘K and in the `+` tab. A new page joins
//! "More", not the rail.
//!
//! A page's `route` is the web UI's hash route, so "open this in the web UI" lands on the same
//! place (`#/`, `#/inbox`, `#/tasks`, ...).

use rune::Icon;

/// When a page gets its native screen (docs/decisions/0002-rune-for-everything.md): P1 is the
/// core screens, P2 the rest of the pages. Until then the page says so and offers the web UI.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    P1,
    P2,
}

impl Phase {
    pub fn name(self) -> &'static str {
        match self {
            Phase::P1 => "P1",
            Phase::P2 => "P2",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Page {
    Home,
    Inbox,
    Tasks,
    Calendar,
    Workspaces,
    Settings,
    PullRequests,
    Watching,
    Schedules,
    Memory,
    Profiles,
    Tools,
    Usage,
}

impl Page {
    /// The five places on the rail, in order. Settings sits at its foot.
    pub const RAIL: [Page; 5] = [Page::Home, Page::Inbox, Page::Tasks, Page::Calendar, Page::Workspaces];
    pub const FOOT: Page = Page::Settings;
    /// In "More", in the web UI's order: the daily ones, then the ones you set up once.
    pub const MORE: [Page; 7] = [Page::PullRequests, Page::Watching, Page::Schedules, Page::Memory, Page::Profiles, Page::Tools, Page::Usage];

    pub const ALL: [Page; 13] = [
        Page::Home,
        Page::Inbox,
        Page::Tasks,
        Page::Calendar,
        Page::Workspaces,
        Page::Settings,
        Page::PullRequests,
        Page::Watching,
        Page::Schedules,
        Page::Memory,
        Page::Profiles,
        Page::Tools,
        Page::Usage,
    ];

    /// The tab's id and the web UI's page id.
    pub fn id(self) -> &'static str {
        match self {
            Page::Home => "home",
            Page::Inbox => "inbox",
            Page::Tasks => "tasks",
            Page::Calendar => "calendar",
            Page::Workspaces => "workspaces",
            Page::Settings => "settings",
            Page::PullRequests => "prs",
            Page::Watching => "watching",
            Page::Schedules => "schedules",
            Page::Memory => "memory",
            Page::Profiles => "profiles",
            Page::Tools => "tools",
            Page::Usage => "usage",
        }
    }

    pub fn from_id(id: &str) -> Option<Page> {
        Page::ALL.into_iter().find(|p| p.id() == id)
    }

    pub fn title(self) -> &'static str {
        match self {
            Page::Home => "Home",
            Page::Inbox => "Inbox",
            Page::Tasks => "Tasks",
            Page::Calendar => "Calendar",
            Page::Workspaces => "Workspaces",
            Page::Settings => "Settings",
            Page::PullRequests => "Pull requests",
            Page::Watching => "Watching",
            Page::Schedules => "Schedules",
            Page::Memory => "Memory",
            Page::Profiles => "Profiles",
            Page::Tools => "Tools & skills",
            Page::Usage => "Usage & machine",
        }
    }

    /// The hash route in the web UI: `#/` for Home, `#/<id>` for the rest (`hrefFor` in
    /// `web/src/router.ts`).
    pub fn hash(self) -> String {
        match self {
            Page::Home => "#/".to_owned(),
            other => format!("#/{}", other.id()),
        }
    }

    pub fn phase(self) -> Phase {
        match self {
            Page::Home | Page::Inbox | Page::Tasks => Phase::P1,
            _ => Phase::P2,
        }
    }

    pub fn icon(self) -> &'static Icon {
        match self {
            Page::Home => &HOME,
            Page::Inbox => &INBOX,
            Page::Tasks => &TASKS,
            Page::Calendar => &CALENDAR,
            Page::Workspaces => &STACK,
            Page::Settings => &GEAR,
            Page::PullRequests => &PULL_REQUEST,
            Page::Watching => &EYE,
            Page::Schedules => &CLOCK,
            Page::Memory => &MEMORY,
            Page::Profiles => &PEOPLE,
            Page::Tools => &WRENCH,
            Page::Usage => &GAUGE,
        }
    }
}

macro_rules! icon_body {
    ($name:ident, $id:literal, $body:literal) => {
        pub static $name: Icon = Icon { name: concat!("neko-", $id), view: 16.0, body: $body };
    };
}

// 16-unit line icons at 1.2 stroke, drawn in `currentColor` so a button's ink tints them.
icon_body!(HOME, "home", r#"<path d="M3 7L8 3L13 7V12.5H9.5V9.5H6.5V12.5H3Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"#);
icon_body!(INBOX, "inbox", r#"<path d="M2.5 9.5L4.2 3.5H11.8L13.5 9.5V12.5H2.5ZM2.5 9.5H6L6.7 11H9.3L10 9.5H13.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/>"#);
icon_body!(TASKS, "tasks", r#"<path d="M6.5 4.5H13.5M6.5 8H13.5M6.5 11.5H13.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><circle cx="3.5" cy="4.5" r="0.9" fill="currentColor"/><circle cx="3.5" cy="8" r="0.9" fill="currentColor"/><circle cx="3.5" cy="11.5" r="0.9" fill="currentColor"/>"#);
icon_body!(CALENDAR, "calendar", r#"<rect x="2.5" y="3.5" width="11" height="10" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M2.5 6.5H13.5M5.5 2V4.5M10.5 2V4.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(STACK, "stack", r#"<path d="M8 2.5L13.5 5.5L8 8.5L2.5 5.5ZM2.5 8.5L8 11.5L13.5 8.5M2.5 11L8 14L13.5 11" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round" stroke-linecap="round"/>"#);
icon_body!(GEAR, "gear", r#"<circle cx="8" cy="8" r="2.2" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M8 2V3.6M8 12.4V14M2 8H3.6M12.4 8H14M3.8 3.8L4.9 4.9M11.1 11.1L12.2 12.2M3.8 12.2L4.9 11.1M11.1 4.9L12.2 3.8" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(MORE, "more", r#"<circle cx="3.5" cy="8" r="1.1" fill="currentColor"/><circle cx="8" cy="8" r="1.1" fill="currentColor"/><circle cx="12.5" cy="8" r="1.1" fill="currentColor"/>"#);
icon_body!(PLUS, "plus", r#"<path d="M8 3.5V12.5M3.5 8H12.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(ARROW_LEFT, "arrow-left", r#"<path d="M13 8H3.5M7.5 4L3.5 8L7.5 12" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(ARROW_RIGHT, "arrow-right", r#"<path d="M3 8H12.5M8.5 4L12.5 8L8.5 12" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(CLOSE_SMALL, "close-small", r#"<path d="M5 5L11 11M11 5L5 11" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(SEARCH, "search", r#"<circle cx="7" cy="7" r="3.75" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M10 10L12.75 12.75" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(PULL_REQUEST, "pull-request", r#"<circle cx="4.5" cy="3.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="4.5" cy="12.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/><circle cx="11.5" cy="12.5" r="1.4" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M4.5 5V11M11.5 11V7.5C11.5 6.4 10.6 5.5 9.5 5.5H8M9.5 3.8L8 5.5L9.5 7.2" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(EYE, "eye", r#"<path d="M1.8 8C3 5.6 5.3 4.2 8 4.2C10.7 4.2 13 5.6 14.2 8C13 10.4 10.7 11.8 8 11.8C5.3 11.8 3 10.4 1.8 8Z" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linejoin="round"/><circle cx="8" cy="8" r="1.8" fill="none" stroke="currentColor" stroke-width="1.2"/>"#);
icon_body!(CLOCK, "clock", r#"<circle cx="8" cy="8" r="5.5" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M8 5V8L10 9.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round" stroke-linejoin="round"/>"#);
icon_body!(MEMORY, "memory", r#"<path d="M5.5 3C4 3 3 4.1 3 5.4C2.3 5.9 2 6.6 2 7.3C2 8.2 2.5 8.9 3.2 9.2C3.2 10.6 4.2 11.6 5.5 11.6V13H8.5V3Z M10.5 3C12 3 13 4.1 13 5.4C13.7 5.9 14 6.6 14 7.3C14 8.2 13.5 8.9 12.8 9.2C12.8 10.6 11.8 11.6 10.5 11.6V13H8V3Z" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/>"#);
icon_body!(PEOPLE, "people", r#"<circle cx="6" cy="5.5" r="2.2" fill="none" stroke="currentColor" stroke-width="1.2"/><path d="M2 13C2 10.8 3.8 9.2 6 9.2C8.2 9.2 10 10.8 10 13M10.5 3.8C11.9 3.9 13 5 13 6.3C13 7.3 12.4 8.1 11.5 8.5M11.5 9.4C13 9.8 14 11.2 14 13" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/>"#);
icon_body!(WRENCH, "wrench", r#"<path d="M10.2 2.5C8.4 2.5 7 3.9 7 5.7C7 6.1 7.1 6.5 7.2 6.8L2.7 11.3C2.2 11.8 2.2 12.6 2.7 13.1C3.2 13.6 4 13.6 4.5 13.1L9 8.6C9.3 8.7 9.7 8.8 10.1 8.8C11.9 8.8 13.3 7.4 13.3 5.6L11.5 7.4L9.6 6.4L8.7 4.5L10.5 2.7Z" fill="none" stroke="currentColor" stroke-width="1.1" stroke-linejoin="round"/>"#);
icon_body!(GAUGE, "gauge", r#"<path d="M2.5 11.5A5.5 5.5 0 1 1 13.5 11.5M8 9L10.5 5.5" fill="none" stroke="currentColor" stroke-width="1.2" stroke-linecap="round"/><circle cx="8" cy="9.2" r="1" fill="currentColor"/>"#);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rail_holds_six_places_and_the_rest_are_in_more() {
        // Five on the rail plus Settings at the foot.
        assert_eq!(Page::RAIL.len() + 1, 6);
        assert_eq!(Page::FOOT, Page::Settings);
        let mut placed: Vec<Page> = Page::RAIL.to_vec();
        placed.push(Page::FOOT);
        placed.extend(Page::MORE);
        assert_eq!(placed.len(), Page::ALL.len(), "every page has exactly one place");
        for page in Page::ALL {
            assert_eq!(placed.iter().filter(|p| **p == page).count(), 1, "{page:?}");
        }
    }

    #[test]
    fn ids_and_routes_match_the_web_ui() {
        assert_eq!(Page::Home.hash(), "#/");
        assert_eq!(Page::Inbox.hash(), "#/inbox");
        assert_eq!(Page::PullRequests.hash(), "#/prs");
        assert_eq!(Page::Tools.hash(), "#/tools");
        for page in Page::ALL {
            assert_eq!(Page::from_id(page.id()), Some(page));
        }
        let mut ids: Vec<&str> = Page::ALL.iter().map(|p| p.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), Page::ALL.len());
    }

    #[test]
    fn core_screens_are_p1_and_the_rest_p2() {
        for page in [Page::Home, Page::Inbox, Page::Tasks] {
            assert_eq!(page.phase(), Phase::P1);
        }
        assert_eq!(Page::Calendar.phase(), Phase::P2);
        assert_eq!(Page::Settings.phase(), Phase::P2);
    }

    #[test]
    fn every_page_has_its_own_icon() {
        let mut names: Vec<&str> = Page::ALL.iter().map(|p| p.icon().name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Page::ALL.len());
    }
}
