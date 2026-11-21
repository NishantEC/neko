//! Full views: what replaces the session in the middle region when chosen from the sessions
//! column's nav (Routines, Calendar, Setup) or the account footer's menu (Settings, Usage, Plugins).
//! Plugin pages join "More" in P3 (docs/decisions/0002-rune-for-everything.md, plugin slot (b)).

use rune::Icon;

use crate::icons;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FullView {
    Calendar,
    Routines,
    Settings,
    Usage,
    /// Setup: Workspaces, Watching, Tools, Skills, Memory, Profiles, Schedules (a section rail).
    Setup,
    Plugins,
    /// Every pull request Neko opened, grouped by where it stands.
    PullRequests,
    /// Every session, newest first: what "View all" in the sessions column opens.
    AllSessions,
}

impl FullView {
    /// The sidebar's nav rows that open a full view (Home, Search and Tasks are not full views of this list), in order: each may carry a count.
    pub const NAV: [FullView; 3] = [FullView::Calendar, FullView::Routines, FullView::Settings];
    /// In the account footer's menu, above "Stop all work", "Open the web UI" and "Quit".
    pub const MENU: [FullView; 3] = [FullView::Usage, FullView::Plugins, FullView::PullRequests];
    /// Setup (workspaces, tools, skills, memory, profiles, packs, improvements) lives in Settings: its rail's last group opens these.
    pub const IN_SETTINGS: [FullView; 1] = [FullView::Setup];
    /// The views the nav and the account menu place (every one has exactly one place).
    pub const ALL: [FullView; 7] = [FullView::Calendar, FullView::Routines, FullView::Settings, FullView::Usage, FullView::Setup, FullView::Plugins, FullView::PullRequests];

    pub fn title(self) -> &'static str {
        match self {
            FullView::Calendar => "Calendar",
            FullView::Routines => "Rounds",
            FullView::Settings => "Settings",
            FullView::Usage => "Usage",
            FullView::Setup => "Setup",
            FullView::Plugins => "Plugins",
            FullView::PullRequests => "Pull requests",
            FullView::AllSessions => "All sessions",
        }
    }

    /// The web UI's page id, so "open in the web UI" lands on the same place (routines are edited on
    /// the web's Inbox page; plugins live under its Tools).
    pub fn web_id(self) -> &'static str {
        match self {
            FullView::Calendar => "calendar",
            FullView::Routines => "inbox",
            FullView::Settings => "settings",
            FullView::Usage => "usage",
            FullView::Setup => "setup",
            FullView::Plugins => "tools",
            FullView::PullRequests => "prs",
            FullView::AllSessions => "tasks",
        }
    }

    pub fn icon(self) -> &'static Icon {
        match self {
            FullView::Calendar => &icons::CALENDAR,
            FullView::Routines => &icons::CLOCK,
            FullView::Settings => &icons::GEAR,
            FullView::Usage => &icons::GAUGE,
            FullView::Setup => &icons::PEOPLE,
            FullView::Plugins => &icons::WRENCH,
            FullView::PullRequests => &icons::PULL_REQUEST,
            FullView::AllSessions => &icons::REASON_LIST,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_has_one_place_a_route_and_an_icon() {
        let mut placed: Vec<FullView> = FullView::NAV.to_vec();
        placed.extend(FullView::MENU);
        placed.extend(FullView::IN_SETTINGS);
        assert_eq!(placed.len(), FullView::ALL.len());
        for v in FullView::ALL {
            assert_eq!(placed.iter().filter(|p| **p == v).count(), 1, "{v:?}");
        }
        let mut ids: Vec<&str> = FullView::ALL.iter().map(|v| v.web_id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), FullView::ALL.len());
        let mut icons: Vec<&str> = FullView::ALL.iter().map(|v| v.icon().name).collect();
        icons.sort_unstable();
        icons.dedup();
        assert_eq!(icons.len(), FullView::ALL.len());
        assert_eq!(FullView::NAV.map(FullView::title), ["Calendar", "Rounds", "Settings"]);
        assert_eq!(FullView::MENU.map(FullView::title), ["Usage", "Plugins", "Pull requests"]);
    }
}
