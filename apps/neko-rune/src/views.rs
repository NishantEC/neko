//! Full views: what replaces the session in the middle region when chosen from the sessions
//! column's foot (Calendar, Routines, Settings) or its "More" menu (Usage and the rest). In P0
//! each is a placeholder that offers the same place in the web UI, so nobody is stuck. Plugin
//! pages join "More" (docs/decisions/0002-rune-for-everything.md, plugin slot (b)).

use rune::Icon;

use crate::icons;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FullView {
    Calendar,
    Routines,
    Settings,
    Usage,
    PullRequests,
    Watching,
    Memory,
    Profiles,
    Tools,
}

impl FullView {
    /// At the column's foot, in order.
    pub const FOOT: [FullView; 3] = [FullView::Calendar, FullView::Routines, FullView::Settings];
    /// In "More".
    pub const MORE: [FullView; 6] = [FullView::Usage, FullView::PullRequests, FullView::Watching, FullView::Memory, FullView::Profiles, FullView::Tools];
    pub const ALL: [FullView; 9] = [
        FullView::Calendar,
        FullView::Routines,
        FullView::Settings,
        FullView::Usage,
        FullView::PullRequests,
        FullView::Watching,
        FullView::Memory,
        FullView::Profiles,
        FullView::Tools,
    ];

    pub fn title(self) -> &'static str {
        match self {
            FullView::Calendar => "Calendar",
            FullView::Routines => "Routines",
            FullView::Settings => "Settings",
            FullView::Usage => "Usage & machine",
            FullView::PullRequests => "Pull requests",
            FullView::Watching => "Watching",
            FullView::Memory => "Memory",
            FullView::Profiles => "Profiles",
            FullView::Tools => "Tools & skills",
        }
    }

    /// The web UI's page id, so "open in the web UI" lands on the same place (routines are the
    /// web UI's Schedules).
    pub fn web_id(self) -> &'static str {
        match self {
            FullView::Calendar => "calendar",
            FullView::Routines => "schedules",
            FullView::Settings => "settings",
            FullView::Usage => "usage",
            FullView::PullRequests => "prs",
            FullView::Watching => "watching",
            FullView::Memory => "memory",
            FullView::Profiles => "profiles",
            FullView::Tools => "tools",
        }
    }

    pub fn icon(self) -> &'static Icon {
        match self {
            FullView::Calendar => &icons::CALENDAR,
            FullView::Routines => &icons::CLOCK,
            FullView::Settings => &icons::GEAR,
            FullView::Usage => &icons::GAUGE,
            FullView::PullRequests => &icons::PULL_REQUEST,
            FullView::Watching => &icons::EYE,
            FullView::Memory => &icons::MEMORY,
            FullView::Profiles => &icons::PEOPLE,
            FullView::Tools => &icons::WRENCH,
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
    }
}
