//! The Pull requests view's rules (`web/src/pages/work/PullRequests.tsx`; `view/prs.rs` draws it): the pull requests Neko opened,
//! grouped by where each stands (the web's order), each group folding. A row's actions are "Merge when ready" (a switch),
//! Close (a dialog with the reason, then a coral button), Open on GitHub and Open its task. The header has "Merge all ready"
//! (behind a dialog that lists them), "Check GitHub", and in its overflow "Review pull requests", merged and closed, and
//! "Merge without review" for each repository. The sidebar's count is what waits on the person (`PrRow::needs_action`).

use std::collections::HashSet;

use crate::app::App;
use crate::link::{Call, PrRow, Verdict};

/// What the view keeps between frames.
#[derive(Default)]
pub struct PrState {
    /// Groups the person folded.
    pub folded: HashSet<Verdict>,
    /// Merged and closed pull requests are shown.
    pub show_finished: bool,
    /// The header's overflow menu is open.
    pub menu_open: bool,
}

impl App {
    /// The pull requests the window is scoped to (the workspace switcher), decisions first.
    pub fn pr_rows(&self) -> Vec<&PrRow> {
        let Some(summary) = self.summary.as_ref() else { return Vec::new() };
        let scope = self.scope();
        summary.pull_requests.iter().filter(|r| scope.is_none_or(|w| summary.workspace_of(&r.id) == w)).collect()
    }

    /// The groups the view shows, in the web's order, with their rows; merged and closed only when asked for.
    pub fn pr_groups(&self) -> Vec<(Verdict, Vec<&PrRow>)> {
        let rows = self.pr_rows();
        Verdict::ALL
            .into_iter()
            .filter(|v| self.prs.show_finished || !v.finished())
            .map(|v| (v, rows.iter().copied().filter(|r| r.verdict == v).collect::<Vec<_>>()))
            .filter(|(_, rows)| !rows.is_empty())
            .collect()
    }

    /// How many merged and closed ones are hidden.
    pub fn pr_hidden_finished(&self) -> usize {
        if self.prs.show_finished { 0 } else { self.pr_rows().iter().filter(|r| r.verdict.finished()).count() }
    }

    /// The pull requests "Merge all ready" would take.
    pub fn ready_pull_requests(&self) -> Vec<PrRow> {
        self.pr_rows().into_iter().filter(|r| r.verdict == Verdict::Ready).cloned().collect()
    }

    /// What the sidebar's "Pull requests" row counts: those waiting on the person. None at 0.
    pub fn pr_attention(&self) -> Option<usize> {
        Some(self.pr_rows().iter().filter(|r| r.needs_action()).count()).filter(|n| *n > 0)
    }

    pub fn pr_toggle_group(&mut self, verdict: Verdict) {
        if !self.prs.folded.remove(&verdict) {
            self.prs.folded.insert(verdict);
        }
    }

    /// "Merge when ready" on or off for one pull request: Neko merges it once its checks pass and nothing conflicts.
    pub fn set_merge_when_ready(&mut self, id: &str, enabled: bool) {
        let number = self.summary.as_ref().and_then(|m| m.pull_requests.iter().find(|r| r.id == id)).map(|r| r.number.clone()).unwrap_or_default();
        let number = if number.is_empty() { "it".to_owned() } else { number };
        self.call(Call::SetMergeWhenReady { task_id: id.to_owned(), enabled });
        self.show_toast(&if enabled { format!("Merging {number} once its checks pass.") } else { format!("Won't merge {number}.") });
    }

    /// "Check GitHub": every open pull request is read now instead of on the timer.
    pub fn check_github(&mut self) {
        self.call(Call::CheckPullRequests);
        self.show_toast("Checking GitHub.");
    }

    /// The repositories the pull requests belong to, with whether merging without review is allowed in each: (path, name, on).
    pub fn pr_repositories(&self) -> Vec<(String, String, bool)> {
        let Some(summary) = self.summary.as_ref() else { return Vec::new() };
        let mut paths: Vec<&str> = self.pr_rows().into_iter().map(|r| r.repository_path.as_str()).filter(|p| !p.is_empty()).collect();
        paths.sort_unstable();
        paths.dedup();
        paths.into_iter().map(|p| (p.to_owned(), crate::task_actions::repo_name(p), summary.merge_without_review.iter().any(|r| r == p))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::link::{Checks, Summary, TaskMeta};

    fn pr(id: &str, verdict: Verdict, checks: Checks) -> PrRow {
        PrRow {
            id: id.into(),
            title: format!("Title {id}"),
            url: format!("https://github.com/acme/api/pull/{id}"),
            number: format!("#{id}"),
            repository: "api".into(),
            repository_path: "/work/api".into(),
            verdict,
            reason: None,
            checks,
            review: String::new(),
            conflicts: false,
            behind: false,
            draft: false,
            updated_ms: 1,
            author: "Claude".into(),
            merging: verdict == Verdict::Merging,
            follow_ups: 0,
            base: "main".into(),
            base_failing: false,
        }
    }

    fn app() -> App {
        let mut app = App::headless();
        let rows = vec![pr("1", Verdict::Ready, Checks::Passing), pr("2", Verdict::Ready, Checks::Failing(vec!["Lint".into()])), pr("3", Verdict::Merging, Checks::Pending), pr("4", Verdict::Merged, Checks::Passing)];
        let mut summary = Summary { pull_requests: rows, merge_without_review: vec!["/work/api".into()], ..Summary::default() };
        for id in ["1", "2", "3", "4"] {
            summary.meta.insert(id.into(), TaskMeta::default());
        }
        app.summary = Some(summary);
        app
    }

    #[test]
    fn groups_come_in_the_webs_order_and_hide_what_is_finished_until_asked() {
        let mut app = app();
        let titles: Vec<_> = app.pr_groups().iter().map(|(v, r)| (v.title(), r.len())).collect();
        assert_eq!(titles, [("Ready to merge", 2), ("Merging when checks pass", 1)]);
        assert_eq!(app.pr_hidden_finished(), 1);
        app.prs.show_finished = true;
        assert_eq!(app.pr_groups().len(), 3);
        assert_eq!(app.pr_hidden_finished(), 0);
    }

    #[test]
    fn the_sidebar_counts_what_waits_on_the_person_and_shows_nothing_at_zero() {
        let mut app = app();
        assert_eq!(app.pr_attention(), Some(2), "two are ready; the merging one is waiting on checks, not on you");
        app.summary.as_mut().unwrap().pull_requests.retain(|r| r.verdict == Verdict::Merged);
        assert_eq!(app.pr_attention(), None);
    }

    #[test]
    fn merge_when_ready_sends_the_webs_request_with_the_switch_state() {
        let mut app = app();
        app.set_merge_when_ready("1", true);
        app.set_merge_when_ready("3", false);
        assert_eq!(app.outbox, vec![Call::SetMergeWhenReady { task_id: "1".into(), enabled: true }, Call::SetMergeWhenReady { task_id: "3".into(), enabled: false }]);
    }

    #[test]
    fn close_asks_for_a_reason_first_and_sends_it_as_the_comment() {
        let mut app = app();
        app.ask_close_pr("1");
        assert!(app.outbox.is_empty(), "nothing goes before the dialog says yes");
        let ask = app.dialog.clone().expect("a dialog");
        assert!(ask.destructive && ask.field.is_some());
        app.pages.set_field(crate::task_actions::ASK_FIELD, "A duplicate of #9");
        app.ask_yes();
        assert_eq!(app.outbox, vec![Call::ClosePr { task_id: "1".into(), comment: "A duplicate of #9".into() }]);
        // Keep it sends nothing.
        let mut app = self::app();
        app.ask_close_pr("1");
        app.ask_no();
        assert!(app.outbox.is_empty() && app.dialog.is_none());
    }

    #[test]
    fn merge_all_lists_the_ready_ones_and_merges_each_on_yes() {
        let mut app = app();
        app.ask_merge_all();
        let ask = app.dialog.clone().expect("a dialog");
        assert_eq!(ask.title, "Merge 2 pull requests?");
        assert_eq!(ask.list.len(), 2);
        assert!(app.outbox.is_empty());
        app.ask_yes();
        assert_eq!(app.outbox, vec![Call::SetMergeWhenReady { task_id: "1".into(), enabled: true }, Call::SetMergeWhenReady { task_id: "2".into(), enabled: true }]);
    }

    #[test]
    fn merge_without_review_asks_when_turning_on_and_not_when_turning_off() {
        let mut app = app();
        app.summary.as_mut().unwrap().merge_without_review.clear();
        app.toggle_merge_without_review("/work/api");
        assert!(app.outbox.is_empty() && app.dialog.is_some(), "turning it on asks first");
        app.ask_yes();
        assert_eq!(app.outbox, vec![Call::SetMergeWithoutReview { repository: "/work/api".into(), enabled: true }]);
        let mut app = self::app();
        app.toggle_merge_without_review("/work/api");
        assert!(app.dialog.is_none());
        assert_eq!(app.outbox, vec![Call::SetMergeWithoutReview { repository: "/work/api".into(), enabled: false }]);
        assert_eq!(app.pr_repositories(), vec![("/work/api".to_owned(), "api".to_owned(), true)]);
    }

    #[test]
    fn check_and_review_send_the_webs_requests() {
        let mut app = app();
        app.check_github();
        assert_eq!(app.outbox, vec![Call::CheckPullRequests]);
        app.outbox.clear();
        app.ask_review_prs();
        assert!(app.outbox.is_empty());
        app.ask_yes();
        assert_eq!(app.outbox, vec![Call::ReviewPullRequests { workspace_id: None }]);
    }
}
