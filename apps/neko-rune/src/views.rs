//! Full views: what replaces the session in the middle region when chosen from the sessions
//! column's nav (Routines, Calendar, Workspaces) or the account footer's menu (Settings, Usage, Plugins).
//! Plugin pages join "More" in P3 (docs/decisions/0002-rune-for-everything.md, plugin slot (b)).

use rune::Icon;

use crate::icons;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FullView {
    Calendar,
    Routines,
    Settings,
    Usage,
    Workspaces,
    Plugins,
    /// Every session, newest first: what "View all" in the sessions column opens.
    AllSessions,
}

impl FullView {
    /// The sessions column's nav rows, in order (polish spec §1): each with its count.
    pub const NAV: [FullView; 3] = [FullView::Routines, FullView::Calendar, FullView::Workspaces];
    /// In the account footer's menu, above "Open the web UI" and "Quit".
    pub const MENU: [FullView; 3] = [FullView::Settings, FullView::Usage, FullView::Plugins];
    /// The views the nav and the account menu place (every one has exactly one place).
    pub const ALL: [FullView; 6] = [FullView::Calendar, FullView::Routines, FullView::Settings, FullView::Usage, FullView::Workspaces, FullView::Plugins];

    pub fn title(self) -> &'static str {
        match self {
            FullView::Calendar => "Calendar",
            FullView::Routines => "Routines",
            FullView::Settings => "Settings",
            FullView::Usage => "Usage",
            FullView::Workspaces => "Workspaces",
            FullView::Plugins => "Plugins",
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
            FullView::Workspaces => "workspaces",
            FullView::Plugins => "tools",
            FullView::AllSessions => "tasks",
        }
    }

    pub fn icon(self) -> &'static Icon {
        match self {
            FullView::Calendar => &icons::CALENDAR,
            FullView::Routines => &icons::CLOCK,
            FullView::Settings => &icons::GEAR,
            FullView::Usage => &icons::GAUGE,
            FullView::Workspaces => &icons::PEOPLE,
            FullView::Plugins => &icons::WRENCH,
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
        assert_eq!(FullView::NAV.map(FullView::title), ["Routines", "Calendar", "Workspaces"]);
        assert_eq!(FullView::MENU.map(FullView::title), ["Settings", "Usage", "Plugins"]);
    }
}
