//! Tabs and history (DESIGN.md §1): every place opens as a tab; opening something that is already
//! open focuses its tab instead. Back and forward walk the order in which tabs were focused, as a
//! browser's do, skipping tabs that have since closed. Home is always there.
//!
//! Pure state with no Rune types, so the rules are tested without a window.

use crate::pages::Page;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tabs {
    open: Vec<Page>,
    active: Page,
    back: Vec<Page>,
    forward: Vec<Page>,
}

impl Default for Tabs {
    fn default() -> Self {
        Tabs { open: vec![Page::Home], active: Page::Home, back: Vec::new(), forward: Vec::new() }
    }
}

impl Tabs {
    pub fn open_pages(&self) -> &[Page] {
        &self.open
    }

    pub fn active(&self) -> Page {
        self.active
    }

    pub fn is_open(&self, page: Page) -> bool {
        self.open.contains(&page)
    }

    /// Opens `page` in a new tab at the end, or focuses its tab when it is open already.
    pub fn open(&mut self, page: Page) {
        if !self.is_open(page) {
            self.open.push(page);
        }
        self.focus(page);
    }

    /// Focuses an open tab (a click on it). A page that is not open opens.
    pub fn select(&mut self, page: Page) {
        self.open(page);
    }

    fn focus(&mut self, page: Page) {
        if page == self.active {
            return;
        }
        self.back.push(self.active);
        self.forward.clear();
        self.active = page;
    }

    /// Closes a tab. Home never closes. Closing the focused tab focuses the one before it in
    /// the history that is still open, else its neighbour.
    pub fn close(&mut self, page: Page) {
        if page == Page::Home {
            return;
        }
        let Some(at) = self.open.iter().position(|p| *p == page) else { return };
        self.open.remove(at);
        self.back.retain(|p| *p != page);
        self.forward.retain(|p| *p != page);
        if self.active == page {
            self.active = self.back.pop().unwrap_or_else(|| self.open[at.min(self.open.len() - 1)]);
        }
    }

    pub fn can_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn back(&mut self) {
        if let Some(prev) = self.back.pop() {
            self.forward.push(self.active);
            self.active = prev;
        }
    }

    pub fn forward(&mut self) {
        if let Some(next) = self.forward.pop() {
            self.back.push(self.active);
            self.active = next;
        }
    }

    /// Moves a tab from one position to another (a drag). Positions past the end clamp.
    pub fn reorder(&mut self, from: usize, to: usize) {
        if from >= self.open.len() {
            return;
        }
        let page = self.open.remove(from);
        self.open.insert(to.min(self.open.len()), page);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_on_home_with_nothing_to_go_back_to() {
        let t = Tabs::default();
        assert_eq!(t.open_pages(), &[Page::Home]);
        assert_eq!(t.active(), Page::Home);
        assert!(!t.can_back() && !t.can_forward());
    }

    #[test]
    fn opening_an_open_page_focuses_its_tab_instead_of_duplicating_it() {
        let mut t = Tabs::default();
        t.open(Page::Inbox);
        t.open(Page::Tasks);
        t.open(Page::Inbox);
        assert_eq!(t.open_pages(), &[Page::Home, Page::Inbox, Page::Tasks]);
        assert_eq!(t.active(), Page::Inbox);
    }

    #[test]
    fn back_and_forward_walk_focus_order() {
        let mut t = Tabs::default();
        t.open(Page::Inbox);
        t.open(Page::Tasks);
        t.back();
        assert_eq!(t.active(), Page::Inbox);
        t.back();
        assert_eq!(t.active(), Page::Home);
        assert!(!t.can_back());
        t.forward();
        t.forward();
        assert_eq!(t.active(), Page::Tasks);
        assert!(!t.can_forward());
        // Going somewhere new forgets the forward path, as a browser does.
        t.back();
        t.open(Page::Calendar);
        assert!(!t.can_forward());
    }

    #[test]
    fn closing_the_focused_tab_returns_to_where_you_were() {
        let mut t = Tabs::default();
        t.open(Page::Inbox);
        t.open(Page::Tasks);
        t.close(Page::Tasks);
        assert_eq!(t.active(), Page::Inbox);
        assert_eq!(t.open_pages(), &[Page::Home, Page::Inbox]);
        t.close(Page::Inbox);
        assert_eq!(t.active(), Page::Home);
    }

    #[test]
    fn closed_tabs_leave_the_history() {
        let mut t = Tabs::default();
        t.open(Page::Inbox);
        t.open(Page::Tasks);
        t.close(Page::Inbox);
        t.back();
        assert_eq!(t.active(), Page::Home, "back skips the closed Inbox");
    }

    #[test]
    fn home_never_closes_and_the_last_tab_is_never_lost() {
        let mut t = Tabs::default();
        t.close(Page::Home);
        assert_eq!(t.open_pages(), &[Page::Home]);
        t.open(Page::Usage);
        t.close(Page::Usage);
        assert_eq!(t.active(), Page::Home);
    }

    #[test]
    fn closing_a_background_tab_keeps_the_focus() {
        let mut t = Tabs::default();
        t.open(Page::Inbox);
        t.open(Page::Tasks);
        t.close(Page::Inbox);
        assert_eq!(t.active(), Page::Tasks);
    }

    #[test]
    fn reordering_moves_a_tab() {
        let mut t = Tabs::default();
        t.open(Page::Inbox);
        t.open(Page::Tasks);
        t.reorder(2, 1);
        assert_eq!(t.open_pages(), &[Page::Home, Page::Tasks, Page::Inbox]);
        t.reorder(9, 0);
        assert_eq!(t.open_pages().len(), 3);
    }
}
