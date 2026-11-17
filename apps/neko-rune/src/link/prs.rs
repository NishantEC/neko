//! The pull requests Neko opened, as the Pull requests view lists them: the web's `pullRequestRow` (`web/src/work/model.ts`)
//! ported, so a row lands in the same group here and there. A row is cut from the task that owns the pull request; the
//! snapshot is dropped as soon as the rows exist (the client keeps no more of it than it shows).

use neko_protocol::workbench::{Snapshot, Task, TaskStatus};

/// Where a pull request stands, in the web's order (`VERDICTS`): the groups of the view, top to bottom.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]
pub enum Verdict {
    Ready,
    Close,
    NeedsYou,
    Merging,
    Working,
    Unchecked,
    Merged,
    Closed,
}

impl Verdict {
    pub const ALL: [Verdict; 8] = [Verdict::Ready, Verdict::Close, Verdict::NeedsYou, Verdict::Merging, Verdict::Working, Verdict::Unchecked, Verdict::Merged, Verdict::Closed];

    /// The web's `VERDICT_TITLE`.
    pub fn title(self) -> &'static str {
        match self {
            Verdict::Ready => "Ready to merge",
            Verdict::Close => "Suggested to close",
            Verdict::NeedsYou => "Needs your call",
            Verdict::Merging => "Merging when checks pass",
            Verdict::Working => "Being updated",
            Verdict::Unchecked => "Not checked on GitHub yet",
            Verdict::Merged => "Merged",
            Verdict::Closed => "Closed",
        }
    }

    /// Merged and closed ones are hidden until asked for.
    pub fn finished(self) -> bool {
        matches!(self, Verdict::Merged | Verdict::Closed)
    }
}

/// What GitHub says about a pull request's checks.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Checks {
    /// Not read yet.
    Unknown,
    Passing,
    /// How many failed (at least one) and their names.
    Failing(Vec<String>),
    Pending,
    /// No checks are configured.
    None,
}

/// One pull request, with the task that opened it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PrRow {
    /// The task that owns it (the web's row id).
    pub id: String,
    pub title: String,
    pub url: String,
    /// "#101".
    pub number: String,
    /// The repository's folder name ("acme-api"), empty until the task has one.
    pub repository: String,
    pub repository_path: String,
    pub verdict: Verdict,
    pub reason: Option<String>,
    pub checks: Checks,
    /// GitHub's review state: "APPROVED", "CHANGES_REQUESTED", "REVIEW_REQUIRED" or empty.
    pub review: String,
    pub conflicts: bool,
    pub behind: bool,
    pub draft: bool,
    pub updated_ms: i64,
    /// Who wrote it: the agent runtime that last ran its task ("Claude"). GitHub's author is not in the snapshot.
    pub author: String,
    /// "Merge when ready" is on.
    pub merging: bool,
    /// How many times a failure was handed back to the agent.
    pub follow_ups: u32,
}

impl PrRow {
    /// It waits on the person: ready to merge, checks failing, or a review requested (the sidebar's count).
    pub fn needs_action(&self) -> bool {
        if self.verdict.finished() || matches!(self.verdict, Verdict::Working | Verdict::Unchecked) {
            return false;
        }
        matches!(self.verdict, Verdict::Ready | Verdict::Close | Verdict::NeedsYou) || matches!(self.checks, Checks::Failing(_)) || self.review == "REVIEW_REQUIRED"
    }

    /// The problems a row's second line names (failing checks, conflicts, behind main).
    pub fn problems(&self) -> Vec<String> {
        let mut out = Vec::new();
        if let Checks::Failing(names) = &self.checks {
            out.push(if names.is_empty() { "Checks failing".to_owned() } else { format!("Checks failing: {}", names.join(", ")) });
        }
        if self.conflicts {
            out.push("Conflicts with the base".to_owned());
        }
        if self.behind {
            out.push("Behind the base".to_owned());
        }
        out
    }
}

/// "#101" from the pull request's address.
pub fn number_of(url: &str) -> String {
    url.split('/').rfind(|p| !p.is_empty()).map(|n| format!("#{n}")).unwrap_or_default()
}

fn basename(path: &str) -> String {
    path.trim_end_matches('/').rsplit('/').next().unwrap_or(path).to_owned()
}

/// The newest line a stopped task left that is not a person's or Neko's own: why it stopped, short.
fn stop_reason(task: &Task) -> Option<String> {
    let line = task.events.iter().rev().find(|e| !matches!(e.role.as_str(), "user" | "note" | "host" | "neko")).map(|e| e.message.trim().to_owned()).filter(|l| !l.is_empty())?;
    Some(if line.chars().count() > 160 { format!("{}…", line.chars().take(159).collect::<String>()) } else { line })
}

/// The row of one task, or None when it has no pull request.
pub fn row_of(task: &Task, snapshot: &Snapshot, author: String) -> Option<PrRow> {
    let pr = task.pull_request.as_ref().filter(|p| !p.url.is_empty())?;
    let status = pr.status.clone().unwrap_or_default();
    let has_status = pr.status.is_some();
    let conflicts = status.merge_state == "DIRTY" || status.mergeable == "CONFLICTING";
    let behind = status.merge_state == "BEHIND";
    let checks = match status.checks.as_str() {
        "passing" => Checks::Passing,
        "failing" => Checks::Failing(status.failing.clone()),
        "pending" => Checks::Pending,
        "none" => Checks::None,
        _ => Checks::Unknown,
    };
    let mut reason = None;
    let verdict = if has_status && status.state == "MERGED" {
        Verdict::Merged
    } else if has_status && status.state == "CLOSED" {
        Verdict::Closed
    } else if matches!(task.status, TaskStatus::Queued | TaskStatus::Planning | TaskStatus::Building | TaskStatus::Reviewing) {
        Verdict::Working
    } else if !has_status || status.state.is_empty() {
        // Until Neko has read it, it may already be merged or closed: no verdict.
        reason = Some("Neko reads it on GitHub within a few minutes.".to_owned());
        Verdict::Unchecked
    } else if pr.merge_when_ready {
        reason = pr.follow_up.merge_note.as_ref().filter(|n| !n.is_empty()).map(|n| format!("Waiting: {n}"));
        Verdict::Merging
    } else if let Some(s) = pr.close_suggestion.as_ref().filter(|s| !s.is_empty()) {
        reason = Some(s.clone());
        Verdict::Close
    } else if task.status == TaskStatus::Cancelled {
        reason = Some("Its task was cancelled here, but the pull request is still open.".to_owned());
        Verdict::Close
    } else if matches!(task.status, TaskStatus::AwaitingApproval | TaskStatus::Failed) {
        reason = if task.status == TaskStatus::Failed { stop_reason(task) } else { Some("It waits on your answer.".to_owned()) };
        Verdict::NeedsYou
    } else {
        Verdict::Ready
    };
    let repository_path = snapshot.task_roots.get(&task.id).cloned().unwrap_or_default();
    Some(PrRow {
        id: task.id.clone(),
        title: task.display_title.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| task.title.clone()),
        url: pr.url.clone(),
        number: number_of(&pr.url),
        repository: if repository_path.is_empty() { String::new() } else { basename(&repository_path) },
        repository_path,
        verdict,
        reason,
        checks,
        review: status.review.clone(),
        conflicts,
        behind,
        draft: status.draft,
        updated_ms: task.updated_at_ms,
        author,
        merging: pr.merge_when_ready,
        follow_ups: pr.follow_up.count,
    })
}

/// Every pull request, decisions first: the web's `pullRequestRows` order (by verdict, then newest).
pub fn rows(snapshot: &Snapshot, author_of: impl Fn(&Task) -> String) -> Vec<PrRow> {
    let mut out: Vec<PrRow> = snapshot.tasks.iter().filter_map(|t| row_of(t, snapshot, author_of(t))).collect();
    out.sort_by(|a, b| a.verdict.cmp(&b.verdict).then(b.updated_ms.cmp(&a.updated_ms)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(tasks: serde_json::Value) -> Snapshot {
        let task_roots = [("t1", "/work/acme-api"), ("t2", "/work/acme-api"), ("t3", "/work/acme-web")].into_iter().map(|(id, root)| (id.to_owned(), root.to_owned())).collect();
        Snapshot { tasks: serde_json::from_value(tasks).expect("tasks"), task_roots, ..Snapshot::default() }
    }

    fn task(id: &str, status: &str, pr: serde_json::Value) -> serde_json::Value {
        serde_json::json!({
            "id": id, "workspace_id": "w", "issue_id": null, "title": format!("Task {id}"), "goal": "g", "status": status, "plan": "", "result": "",
            "worktree": null, "created_at_ms": 0, "updated_at_ms": 5, "events": [], "pull_request": pr,
        })
    }

    fn pr(state: &str, checks: &str, extra: serde_json::Value) -> serde_json::Value {
        let mut status = serde_json::json!({"state": state, "checks": checks, "mergeable": "MERGEABLE", "review": "REVIEW_REQUIRED", "head": "a", "base": "main", "merge_state": "BLOCKED"});
        if let (Some(s), Some(e)) = (status.as_object_mut(), extra.as_object()) {
            s.extend(e.clone());
        }
        serde_json::json!({"url": "https://github.com/acme/acme-api/pull/101", "branch": "b", "status": status})
    }

    fn one(t: serde_json::Value) -> PrRow {
        let snap = snapshot(serde_json::json!([t]));
        rows(&snap, |_| "Claude".into()).pop().expect("a row")
    }

    #[test]
    fn a_task_without_a_pull_request_has_no_row() {
        let snap = snapshot(serde_json::json!([{"id": "t1", "workspace_id": "w", "issue_id": null, "title": "T", "goal": "g", "status": "Completed", "plan": "", "result": "", "worktree": null, "created_at_ms": 0, "updated_at_ms": 5, "events": []}]));
        assert!(rows(&snap, |_| String::new()).is_empty());
    }

    #[test]
    fn the_verdict_follows_the_webs_rules() {
        assert_eq!(one(task("t1", "Completed", pr("OPEN", "passing", serde_json::json!({})))).verdict, Verdict::Ready);
        assert_eq!(one(task("t1", "Completed", pr("MERGED", "passing", serde_json::json!({})))).verdict, Verdict::Merged);
        assert_eq!(one(task("t1", "Completed", pr("CLOSED", "passing", serde_json::json!({})))).verdict, Verdict::Closed);
        assert_eq!(one(task("t1", "Building", pr("OPEN", "passing", serde_json::json!({})))).verdict, Verdict::Working);
        assert_eq!(one(task("t1", "Cancelled", pr("OPEN", "passing", serde_json::json!({})))).verdict, Verdict::Close);
        assert_eq!(one(task("t1", "Failed", pr("OPEN", "passing", serde_json::json!({})))).verdict, Verdict::NeedsYou);
        // No status yet: unchecked. A request to merge: merging.
        assert_eq!(one(task("t1", "Completed", serde_json::json!({"url": "https://github.com/acme/acme-api/pull/9", "branch": "b"}))).verdict, Verdict::Unchecked);
        let mut merging = pr("OPEN", "passing", serde_json::json!({}));
        merging["merge_when_ready"] = serde_json::json!(true);
        let row = one(task("t1", "Completed", merging));
        assert_eq!((row.verdict, row.merging), (Verdict::Merging, true));
        let mut suggested = pr("OPEN", "passing", serde_json::json!({}));
        suggested["close_suggestion"] = serde_json::json!("A duplicate of #99");
        let row = one(task("t1", "Completed", suggested));
        assert_eq!((row.verdict, row.reason.as_deref()), (Verdict::Close, Some("A duplicate of #99")));
    }

    #[test]
    fn checks_and_problems_are_read_from_the_status() {
        let row = one(task("t1", "Completed", pr("OPEN", "failing", serde_json::json!({"failing": ["Lint", "Unit"], "merge_state": "DIRTY"}))));
        assert_eq!(row.checks, Checks::Failing(vec!["Lint".into(), "Unit".into()]));
        assert_eq!(row.problems(), ["Checks failing: Lint, Unit", "Conflicts with the base"]);
        assert_eq!((row.number.as_str(), row.repository.as_str()), ("#101", "acme-api"));
    }

    #[test]
    fn the_sidebar_counts_what_waits_on_the_person() {
        let ready = one(task("t1", "Completed", pr("OPEN", "passing", serde_json::json!({}))));
        assert!(ready.needs_action());
        assert!(!one(task("t1", "Completed", pr("MERGED", "passing", serde_json::json!({})))).needs_action());
        assert!(!one(task("t1", "Building", pr("OPEN", "failing", serde_json::json!({})))).needs_action(), "an agent is already on it");
        let mut merging = pr("OPEN", "failing", serde_json::json!({"review": "APPROVED"}));
        merging["merge_when_ready"] = serde_json::json!(true);
        assert!(one(task("t1", "Completed", merging)).needs_action(), "failing checks under a requested merge still need a look");
        let mut quiet = pr("OPEN", "passing", serde_json::json!({"review": "APPROVED"}));
        quiet["merge_when_ready"] = serde_json::json!(true);
        assert!(!one(task("t1", "Completed", quiet)).needs_action());
    }

    #[test]
    fn rows_are_in_verdict_order_then_newest() {
        let snap = snapshot(serde_json::json!([
            task("t1", "Completed", pr("MERGED", "passing", serde_json::json!({}))),
            task("t2", "Completed", pr("OPEN", "passing", serde_json::json!({}))),
            task("t3", "Failed", pr("OPEN", "passing", serde_json::json!({}))),
        ]));
        let ids: Vec<String> = rows(&snap, |_| String::new()).into_iter().map(|r| r.id).collect();
        assert_eq!(ids, ["t2", "t3", "t1"]);
    }
}
