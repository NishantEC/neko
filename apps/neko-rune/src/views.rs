//! Full views: what replaces the session in the middle region when chosen from the sessions
//! column's foot (Calendar, Routines, Settings) or its "More" menu (Usage, Workspaces, Plugins).
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
}

impl FullView {
    /// At the column's foot, in order.
    pub const FOOT: [FullView; 3] = [FullView::Calendar, FullView::Routines, FullView::Settings];
    /// In "More", above "Open the web UI".
    pub const MORE: [FullView; 3] = [FullView::Usage, FullView::Workspaces, FullView::Plugins];
    pub const ALL: [FullView; 6] = [FullView::Calendar, FullView::Routines, FullView::Settings, FullView::Usage, FullView::Workspaces, FullView::Plugins];

    pub fn title(self) -> &'static str {
        match self {
            FullView::Calendar => "Calendar",
            FullView::Routines => "Routines",
            FullView::Settings => "Settings",
            FullView::Usage => "Usage",
            FullView::Workspaces => "Workspaces",
            FullView::Plugins => "Plugins",
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_view_has_one_place_a_route_and_an_icon() {
        let mut placed: Vec<FullView> = FullView::FOOT.to_vec();
        placed.extend(FullView::MORE);
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
        assert_eq!(FullView::FOOT.map(FullView::title), ["Calendar", "Routines", "Settings"]);
        assert_eq!(FullView::MORE.map(FullView::title), ["Usage", "Workspaces", "Plugins"]);
    }
}
