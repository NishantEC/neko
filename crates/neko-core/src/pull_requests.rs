//! What happens to a pull request after Neko opens it.
//!
//! Neko used to stop at "Opened a pull request": a PR that was merged, closed,
//! failed CI, fell into conflict or got a reviewer's comment changed nothing
//! on the task, so tasks sat at "Ready to review" while GitHub had moved on
//! (20 of 42 when first measured). The daemon's watch
//! (`workbench/pull_requests.rs`) reads each open PR with `gh` and calls
//! [`reconcile`], which is pure and decides what that means:
//!
//! - merged: the task is done; closed: the task is cancelled;
//! - a new failing check, a merge conflict, or a comment from a person
//!   (not a bot, not the PR's own author, which is also how Neko and other
//!   agents post) goes back to the task's agent as a note, and the task runs
//!   again in its own folder; review and publishing then work as always and
//!   push to the same PR;
//! - each is handed back once, and after [`MAX_FOLLOW_UPS`] the task waits
//!   for you instead, so a check that can't be fixed never loops.
//!
//! Merging is yours: `SetMergeWhenReady` marks a PR, and the watch merges it
//! once checks pass and nothing conflicts. Closing is yours too.

use neko_protocol::workbench::{PrStatus, PullRequest, Snapshot, Task, TaskStatus};

/// Automatic hand-backs per pull request before it waits for you.
pub const MAX_FOLLOW_UPS: u32 = 4;
/// Comment ids remembered per pull request.
const SEEN_LIMIT: usize = 200;
/// Who on GitHub counts as a person reviewing, rather than a bot or a stranger.
const REVIEWER_ASSOCIATIONS: &[&str] = &["OWNER", "MEMBER", "COLLABORATOR", "CONTRIBUTOR"];
/// The role the watch's own events use on a task.
pub const ROLE: &str = "github";

/// One comment or review from a person.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub author: String,
    pub body: String,
    /// "file.ts:12" for an inline comment.
    pub place: Option<String>,
}

/// A pull request read from `gh pr view --json` (`FIELDS`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observed {
    pub status: PrStatus,
    /// Failing checks with a link to their logs.
    pub failures: Vec<(String, String)>,
    pub comments: Vec<Comment>,
    /// A person left a review, so inline comments are worth fetching.
    pub reviewed: bool,
    /// Who opened it: the signed-in account, which Neko and other agents
    /// also comment as, so its comments are never treated as a review.
    pub author: String,
}

/// The fields the watch asks `gh pr view` for.
pub const FIELDS: &str = "state,mergeable,headRefOid,baseRefName,reviewDecision,author,statusCheckRollup,reviews,comments";

/// Reads `gh pr view --json FIELDS` output.
pub fn parse_view(json: &str) -> Result<Observed, String> {
    let v: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Unreadable pull request status: {e}"))?;
    let text = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_owned();
    let pr_author = text(&v["author"]["login"]);
    let mut failures = Vec::new();
    let (mut pending, mut any) = (false, false);
    for check in v["statusCheckRollup"].as_array().into_iter().flatten() {
        any = true;
        let name = [&check["name"], &check["context"]].into_iter().map(text).find(|n| !n.is_empty()).unwrap_or_else(|| "check".into());
        let link = [&check["detailsUrl"], &check["targetUrl"]].into_iter().map(text).find(|n| !n.is_empty()).unwrap_or_default();
        let (conclusion, state, status) = (text(&check["conclusion"]), text(&check["state"]), text(&check["status"]));
        if matches!(conclusion.as_str(), "FAILURE" | "TIMED_OUT" | "ACTION_REQUIRED" | "STARTUP_FAILURE") || matches!(state.as_str(), "FAILURE" | "ERROR") {
            failures.push((name, link));
        } else if (!status.is_empty() && status != "COMPLETED") || matches!(state.as_str(), "PENDING" | "EXPECTED") {
            pending = true;
        }
    }
    let checks = if !failures.is_empty() { "failing" } else if pending { "pending" } else if any { "passing" } else { "none" };
    let person = |c: &serde_json::Value| {
        let login = text(&c["author"]["login"]);
        !login.is_empty() && login != pr_author && !login.ends_with("[bot]")
            && REVIEWER_ASSOCIATIONS.contains(&c["authorAssociation"].as_str().unwrap_or_default())
    };
    let reviewed = v["reviews"].as_array().into_iter().flatten().any(|c| person(c));
    let mut comments = Vec::new();
    for c in v["comments"].as_array().into_iter().flatten().chain(v["reviews"].as_array().into_iter().flatten()) {
        let body = text(&c["body"]).trim().to_owned();
        if person(c) && !body.is_empty() {
            comments.push(Comment { id: text(&c["id"]), author: text(&c["author"]["login"]), body, place: None });
        }
    }
    Ok(Observed {
        status: PrStatus {
            state: text(&v["state"]),
            checks: checks.into(),
            failing: failures.iter().map(|(n, _)| n.clone()).collect(),
            mergeable: text(&v["mergeable"]),
            review: text(&v["reviewDecision"]),
            head: text(&v["headRefOid"]),
            base: text(&v["baseRefName"]),
        },
        failures,
        comments,
        reviewed,
        author: pr_author,
    })
}

/// Reads `gh api repos/{repo}/pulls/{n}/comments` (inline review comments).
/// Bots are left out by GitHub's own account type.
pub fn parse_inline_comments(json: &str, pr_author: &str) -> Vec<Comment> {
    let Ok(serde_json::Value::Array(items)) = serde_json::from_str(json) else { return Vec::new() };
    items.iter().filter_map(|c| {
        let login = c["user"]["login"].as_str()?;
        let body = c["body"].as_str()?.trim();
        if login == pr_author || c["user"]["type"].as_str() == Some("Bot") || body.is_empty()
            || !REVIEWER_ASSOCIATIONS.contains(&c["author_association"].as_str().unwrap_or_default()) {
            return None;
        }
        let place = c["path"].as_str().map(|p| match c["line"].as_u64() { Some(l) => format!("{p}:{l}"), None => p.to_owned() });
        Some(Comment { id: c["id"].to_string(), author: login.to_owned(), body: body.to_owned(), place })
    }).collect()
}

/// `owner/repo` and the number, from a pull request URL.
pub fn locate(url: &str) -> Option<(String, u64)> {
    let rest = url.split("github.com/").nth(1)?;
    let mut parts = rest.split('/');
    let (owner, repo, kind, number) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    (kind == "pull").then_some(())?;
    Some((format!("{owner}/{repo}"), number.split(['#', '?']).next()?.parse().ok()?))
}

/// What the watch should do about one task's pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Next {
    Nothing,
    /// Hand the task back to its agent with this note.
    HandBack(String),
    /// Merge it now (`--admin` when the repository allows merging without review).
    Merge,
    /// It was asked to merge, but can't be: say why and stop trying.
    MergeBlocked(String),
}

/// Records what GitHub says on `task` and returns what to do next. Pure:
/// the caller saves the snapshot and performs any merge or hand-back.
pub fn reconcile(task: &mut Task, observed: &Observed, working: bool) -> Next {
    let Some(pr) = task.pull_request.as_mut() else { return Next::Nothing };
    let first_look = pr.status.is_none();
    pr.status = Some(observed.status.clone());
    let status = &observed.status;
    match status.state.as_str() {
        "MERGED" => {
            pr.merge_when_ready = false;
            pr.close_suggestion = None;
            if task.status != TaskStatus::Completed {
                task.status = TaskStatus::Completed;
                crate::workbench::append_event(task, ROLE, "Merged on GitHub. This task is done.");
            }
            return Next::Nothing;
        }
        "CLOSED" => {
            pr.merge_when_ready = false;
            if !matches!(task.status, TaskStatus::Cancelled | TaskStatus::Completed) {
                task.status = TaskStatus::Cancelled;
                crate::workbench::append_event(task, ROLE, "Closed on GitHub without merging. Reply to reopen the task.");
            }
            return Next::Nothing;
        }
        _ => {}
    }
    // Only a finished task is handed back or merged; work in progress pushes
    // to the same pull request when it is reviewed.
    if working || task.status != TaskStatus::ReadyForReview {
        return Next::Nothing;
    }
    // The first time Neko reads a pull request, what is already there is a
    // baseline, not news: otherwise turning the watch on would start an
    // agent for every old failure and comment at once. Those show in the
    // Pull requests table, and "Re-check all" acts on them deliberately.
    if first_look {
        pr.follow_up.seen_comments.extend(observed.comments.iter().map(|c| c.id.clone()));
        if status.mergeable == "CONFLICTING" { pr.follow_up.handled_conflict = Some(status.head.clone()); }
        if status.checks == "failing" { pr.follow_up.handled_failure = Some(format!("{}:{}", status.head, status.failing.join(","))); }
    }
    let new: Vec<&Comment> = observed.comments.iter().filter(|c| !pr.follow_up.seen_comments.contains(&c.id)).collect();
    let conflict = status.mergeable == "CONFLICTING" && pr.follow_up.handled_conflict.as_deref() != Some(&status.head);
    let failure_key = format!("{}:{}", status.head, status.failing.join(","));
    let failure = status.checks == "failing" && pr.follow_up.handled_failure.as_deref() != Some(&failure_key);

    if pr.merge_when_ready && new.is_empty() {
        if status.mergeable == "CONFLICTING" {
            pr.merge_when_ready = false;
            return Next::MergeBlocked("It conflicts with the branch it merges into.".into());
        }
        if status.checks == "failing" {
            pr.merge_when_ready = false;
            return Next::MergeBlocked(format!("Checks failed: {}.", status.failing.join(", ")));
        }
        if matches!(status.checks.as_str(), "passing" | "none") && status.mergeable == "MERGEABLE" {
            return Next::Merge;
        }
        return Next::Nothing; // checks still running, or GitHub still computing mergeability
    }
    if new.is_empty() && !conflict && !failure {
        return Next::Nothing;
    }
    // Everything seen now counts as handled, even past the limit, so the
    // same item is never sent twice.
    for c in &new {
        pr.follow_up.seen_comments.push(c.id.clone());
    }
    let excess = pr.follow_up.seen_comments.len().saturating_sub(SEEN_LIMIT);
    pr.follow_up.seen_comments.drain(..excess);
    if conflict { pr.follow_up.handled_conflict = Some(status.head.clone()); }
    if failure { pr.follow_up.handled_failure = Some(failure_key); }
    if pr.follow_up.count >= MAX_FOLLOW_UPS {
        crate::workbench::append_event(task, ROLE, &format!(
            "The pull request needs attention again ({}), but Neko has already handed it back {MAX_FOLLOW_UPS} times. Reply to send it back to the agent.",
            reasons(!new.is_empty(), conflict, failure)
        ));
        task.status = TaskStatus::AwaitingApproval;
        return Next::Nothing;
    }
    pr.follow_up.count += 1;
    Next::HandBack(hand_back_note(pr, status, &observed.failures, &new, conflict, failure))
}

fn reasons(comments: bool, conflict: bool, failure: bool) -> String {
    [(comments, "new review comments"), (conflict, "a merge conflict"), (failure, "failing checks")]
        .into_iter().filter(|(on, _)| *on).map(|(_, r)| r).collect::<Vec<_>>().join(", ")
}

/// What the agent is told. It works in the task's own folder; Neko reviews
/// the result and pushes it to the same pull request.
fn hand_back_note(pr: &PullRequest, status: &PrStatus, failures: &[(String, String)], comments: &[&Comment], conflict: bool, failure: bool) -> String {
    let mut note = format!(
        "Neko, from the pull request {url}: {why}. Work on it in this folder; Neko reviews the result and pushes it to the same pull request.\n\
         First bring this branch up to date with the pull request, which others may have pushed to: `git fetch origin {branch} && git merge --ff-only origin/{branch}` (if that refuses, merge it and keep both sides' intent).",
        url = pr.url, branch = pr.branch, why = reasons(!comments.is_empty(), conflict, failure),
    );
    if conflict {
        note.push_str(&format!(
            "\n\nIt conflicts with `{base}`. Run `git fetch origin {base} && git merge origin/{base}`, resolve every conflict keeping both sides' intent, and re-run the affected tests.",
            base = if status.base.is_empty() { "main" } else { &status.base }
        ));
    }
    if failure {
        note.push_str("\n\nThese checks failed:");
        for (name, link) in failures {
            note.push_str(&format!("\n- {name}{}", if link.is_empty() { String::new() } else { format!(": {link}") }));
        }
        note.push_str("\nRead the failure itself (`gh run view <run id> --log-failed`, or the link) and find its cause. If this change caused it, fix it. If it fails the same way without this change (on the base branch, a flaky test, infrastructure, a policy scanner), change nothing and end with `NO_CHANGE: <the evidence that it is unrelated>`.");
    }
    if !comments.is_empty() {
        note.push_str("\n\nReviewers commented:");
        for c in comments {
            let body: String = c.body.chars().take(2000).collect();
            note.push_str(&format!("\n- @{}{}: {}", c.author, c.place.as_deref().map(|p| format!(" on {p}")).unwrap_or_default(), body));
        }
        note.push_str("\nAddress each one. Where a reviewer is right, change the code. Where you disagree, change nothing for that point and say why in your result. A point that is a product decision is the user's: end with `QUESTION: <it>`.");
    }
    note
}

/// The note that asks a task's agent to re-check its open pull request,
/// the way a careful reviewer would before it is merged. `others` names the
/// other open pull requests so a duplicate can be recognised.
pub fn review_note(pr: &PullRequest, others: &[String]) -> String {
    let base = pr.status.as_ref().map(|s| s.base.as_str()).filter(|b| !b.is_empty()).unwrap_or("main");
    let mut note = format!(
        "Neko: re-check your pull request {url} before it is merged. Work in this folder; Neko reviews the result and pushes it to the same pull request.\n\
         1. Bring it up to date: `git fetch origin {branch} {base} && git merge --ff-only origin/{branch}`, then `git merge origin/{base}`, resolving any conflict.\n\
         2. Check the cause again against the real evidence (the error's events and stack traces, logs, the issue). If the stated cause is wrong, fix the real one and correct the description.\n\
         3. Check whether work already merged into `{base}` covers it, and whether one of the other open pull requests below does the same thing.\n\
         4. Cut the change to what the cause needs: keep the cause and its in-scope siblings covered, drop polish, refactors and anything else, and prefer fewer lines and a shallower change. Keep a test that fails without it.\n\
         5. End with exactly one of these as your final line:\n\
         - `SCRAP: <the PR or commit that already covers it, or why it should not be merged>` when it should be closed instead (change nothing);\n\
         - `QUESTION: <the decision>` when merging it changes what users see or do in a way only the user can decide;\n\
         - nothing special when it is ready to merge.",
        url = pr.url, branch = pr.branch,
    );
    if !others.is_empty() {
        note.push_str("\n\nOther open pull requests:");
        for other in others.iter().take(60) {
            note.push_str(&format!("\n- {other}"));
        }
    }
    note
}

/// Hands a task back to its agent with `note`: it runs again in its own
/// folder, directly, without asking (the work it already had authority for).
pub fn hand_back(snapshot: &mut Snapshot, task_id: &str, note: &str) -> Result<(), String> {
    let task = snapshot.tasks.iter_mut().find(|t| t.id == task_id).ok_or("Task no longer exists")?;
    crate::workbench::append_event(task, crate::workbench::NOTE_ROLE, note);
    task.status = TaskStatus::Queued;
    task.supervision = None;
    snapshot.task_read_only.remove(task_id);
    snapshot.start_when_planned.insert(task_id.to_owned());
    // Neko wrote this note itself; there is nothing for the reply
    // interpreter to read, and its authority is the task's own.
    snapshot.task_replies.insert(task_id.to_owned(), neko_protocol::workbench::TaskReplyState {
        revision: crate::workbench::new_id(), handled: true, pending: vec![],
    });
    Ok(())
}

/// Every task whose open pull request can be re-checked now, in `workspace`.
pub fn reviewable(snapshot: &Snapshot, workspace: Option<&str>) -> Vec<String> {
    snapshot.tasks.iter()
        .filter(|t| workspace.is_none_or(|w| t.workspace_id == w))
        .filter(|t| t.status == TaskStatus::ReadyForReview)
        .filter(|t| t.pull_request.as_ref().is_some_and(PullRequest::is_open))
        .map(|t| t.id.clone())
        .collect()
}

/// Asks every reviewable task's agent to re-check its pull request.
/// Returns how many were handed back.
pub fn review_all(snapshot: &mut Snapshot, workspace: Option<&str>) -> Result<usize, String> {
    let ids = reviewable(snapshot, workspace);
    let open: Vec<(String, String)> = snapshot.tasks.iter()
        .filter(|t| t.pull_request.as_ref().is_some_and(PullRequest::is_open))
        .map(|t| (t.id.clone(), format!("{} {}", t.pull_request.as_ref().map(|p| p.url.as_str()).unwrap_or_default(),
                                         t.display_title.as_deref().unwrap_or(&t.title))))
        .collect();
    for id in &ids {
        let pr = snapshot.tasks.iter().find(|t| &t.id == id).and_then(|t| t.pull_request.clone()).ok_or("Task no longer exists")?;
        let others: Vec<String> = open.iter().filter(|(other, _)| other != id).map(|(_, line)| line.clone()).collect();
        hand_back(snapshot, id, &review_note(&pr, &others))?;
        if let Some(pr) = snapshot.tasks.iter_mut().find(|t| &t.id == id).and_then(|t| t.pull_request.as_mut()) {
            pr.close_suggestion = None;
        }
    }
    Ok(ids.len())
}

/// A task's proposal to fix something in another repository: the
/// `FOLLOW_UP: <repository> | <title> | <what to change and why>` lines in
/// its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FollowUp {
    pub repository: String,
    pub title: String,
    pub detail: String,
}

pub fn follow_ups(result: &str) -> Vec<FollowUp> {
    result.lines().filter_map(|line| {
        let line = line.trim().trim_start_matches(['*', '_', '#', '`', ' ', '-', '>']);
        let rest = line.strip_prefix("FOLLOW_UP:")?;
        let mut parts = rest.splitn(3, '|').map(|p| p.trim().trim_matches(['*', '_', '`', ' ']).to_owned());
        let (repository, title, detail) = (parts.next()?, parts.next()?, parts.next().unwrap_or_default());
        (!repository.is_empty() && !title.is_empty()).then(|| FollowUp {
            repository,
            title: title.chars().take(200).collect(),
            detail: detail.chars().take(4000).collect(),
        })
    }).take(3).collect()
}

/// The repository a follow-up names: a checkout whose folder, origin remote
/// or package is called that.
pub fn resolve_repository<'a>(name: &str, candidates: &'a [crate::workbench::RepositoryCandidate]) -> Option<&'a str> {
    let want = name.trim().trim_end_matches(".git").rsplit('/').next().unwrap_or_default().to_lowercase();
    if want.is_empty() { return None; }
    candidates.iter().find(|c| {
        let folder = std::path::Path::new(&c.path).file_name().map(|f| f.to_string_lossy().to_lowercase()).unwrap_or_default();
        let remote = c.remote.trim_end_matches('/').trim_end_matches(".git").rsplit(['/', ':']).next().unwrap_or_default().to_lowercase();
        folder == want || remote == want || c.package.to_lowercase() == want
    }).map(|c| c.path.as_str())
}

/// Turns new `FOLLOW_UP:` lines in finished tasks' results into proposed
/// tasks in the named repository: each waits for your approval, and
/// `Snapshot::follow_up_of` records which task found it. Returns whether
/// anything changed. `candidates_for` lists the checkouts in a workspace.
pub fn propose_follow_ups(snapshot: &mut Snapshot, candidates_for: impl Fn(&str) -> Vec<crate::workbench::RepositoryCandidate>) -> Result<bool, String> {
    let finished: Vec<(String, String, String, Vec<FollowUp>)> = snapshot.tasks.iter()
        .filter(|t| matches!(t.status, TaskStatus::ReadyForReview | TaskStatus::Completed | TaskStatus::AwaitingApproval))
        .map(|t| (t.id.clone(), t.workspace_id.clone(), t.display_title.clone().unwrap_or_else(|| t.title.clone()), follow_ups(&t.result)))
        .filter(|(.., found)| !found.is_empty())
        .collect();
    let mut changed = false;
    for (parent, workspace, parent_title, found) in finished {
        let candidates = candidates_for(&workspace);
        for follow_up in found {
            let exists = snapshot.follow_up_of.iter().any(|(child, of)| of == &parent
                && snapshot.tasks.iter().any(|t| &t.id == child && t.title == follow_up.title));
            if exists { continue; }
            let parent_task = snapshot.tasks.iter_mut().find(|t| t.id == parent).ok_or("Task no longer exists")?;
            let Some(root) = resolve_repository(&follow_up.repository, &candidates).map(str::to_owned) else {
                let note = format!("Its agent proposed a follow-up in “{}”, but no repository by that name is in this workspace's folders: {}", follow_up.repository, follow_up.title);
                if !parent_task.events.iter().any(|e| e.message == note) {
                    crate::workbench::append_event(parent_task, ROLE_FOLLOW_UP, &note);
                    changed = true;
                }
                continue;
            };
            let repo_name = std::path::Path::new(&root).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default();
            crate::workbench::append_event(parent_task, ROLE_FOLLOW_UP, &format!("Proposed a follow-up in {repo_name}: {}. It waits for your approval.", follow_up.title));
            let goal = format!("{}\n\nFound while working on “{parent_title}”.", if follow_up.detail.is_empty() { &follow_up.title } else { &follow_up.detail });
            let mut task = crate::workbench::create_task(workspace.clone(), None, follow_up.title.clone(), goal.clone())?;
            task.status = TaskStatus::AwaitingApproval;
            task.plan = format!("Proposed by the agent working on “{parent_title}”, which found the cause here:\n\n{goal}\n\nApprove to have an agent fix it in {repo_name}.");
            crate::workbench::append_event(&mut task, ROLE_FOLLOW_UP, &format!("Proposed from “{parent_title}”. Approve to start it."));
            snapshot.task_roots.insert(task.id.clone(), root);
            snapshot.follow_up_of.insert(task.id.clone(), parent.clone());
            snapshot.tasks.push(task);
            changed = true;
        }
    }
    Ok(changed)
}

/// The role events about cross-repository follow-ups use.
pub const ROLE_FOLLOW_UP: &str = "neko";

#[cfg(test)]
mod tests {
    use super::*;
    use neko_protocol::workbench::PrFollowUp;

    fn view(state: &str, mergeable: &str, checks: &str, comments: &str) -> String {
        format!(r#"{{"state":"{state}","mergeable":"{mergeable}","headRefOid":"abc","baseRefName":"main","reviewDecision":"REVIEW_REQUIRED",
            "author":{{"login":"me"}},"statusCheckRollup":[{checks}],"reviews":[],"comments":[{comments}]}}"#)
    }
    const PASS: &str = r#"{"__typename":"CheckRun","name":"Test and Lint","status":"COMPLETED","conclusion":"SUCCESS","detailsUrl":"u1"}"#;
    const FAIL: &str = r#"{"__typename":"CheckRun","name":"Test and Lint","status":"COMPLETED","conclusion":"FAILURE","detailsUrl":"https://ci/1"}"#;
    const RUNNING: &str = r#"{"__typename":"CheckRun","name":"Test and Lint","status":"IN_PROGRESS","conclusion":""}"#;
    const BOT: &str = r#"{"id":"c1","author":{"login":"coderabbitai"},"authorAssociation":"NONE","body":"summary"}"#;
    const SELF: &str = r#"{"id":"c2","author":{"login":"me"},"authorAssociation":"OWNER","body":"On hold"}"#;
    const PERSON: &str = r#"{"id":"c3","author":{"login":"aakar"},"authorAssociation":"MEMBER","body":"Use the shared helper here"}"#;

    fn task() -> Task {
        Task {
            id: "t".into(), workspace_id: "w".into(), issue_id: None, title: "Fix it".into(), goal: String::new(),
            status: TaskStatus::ReadyForReview, plan: String::new(), result: String::new(), worktree: None, events: vec![],
            created_at_ms: 0, updated_at_ms: 0, source_revision: None, supervision: None, base_commit: None,
            pull_request: Some(PullRequest { url: "https://github.com/acme/app/pull/7".into(), branch: "codex/neko-t".into(),
                status: Some(PrStatus { state: "OPEN".into(), ..Default::default() }), ..Default::default() }),
            landed_commit: None, display_title: None, parked: None,
        }
    }

    #[test]
    fn checks_read_as_one_word_and_only_people_count_as_reviewers() {
        let o = parse_view(&view("OPEN", "MERGEABLE", &[PASS, r#"{"__typename":"StatusContext","context":"CodeRabbit","state":"SUCCESS"}"#].join(","), &[BOT, SELF, PERSON].join(","))).unwrap();
        assert_eq!(o.status.checks, "passing");
        assert_eq!(o.comments.iter().map(|c| c.id.as_str()).collect::<Vec<_>>(), ["c3"], "bots and the PR's own author are not reviewers");
        assert_eq!(parse_view(&view("OPEN", "MERGEABLE", &[PASS, FAIL].join(","), "")).unwrap().failures, [("Test and Lint".to_string(), "https://ci/1".to_string())]);
        assert_eq!(parse_view(&view("OPEN", "MERGEABLE", RUNNING, "")).unwrap().status.checks, "pending");
        assert_eq!(parse_view(&view("OPEN", "MERGEABLE", "", "")).unwrap().status.checks, "none");
    }

    #[test]
    fn merged_and_closed_finish_the_task() {
        let mut t = task();
        assert_eq!(reconcile(&mut t, &parse_view(&view("MERGED", "UNKNOWN", PASS, "")).unwrap(), false), Next::Nothing);
        assert_eq!(t.status, TaskStatus::Completed);
        let mut t = task();
        reconcile(&mut t, &parse_view(&view("CLOSED", "UNKNOWN", PASS, "")).unwrap(), false);
        assert_eq!(t.status, TaskStatus::Cancelled);
        assert!(t.events.iter().any(|e| e.role == ROLE && e.message.contains("Closed on GitHub")));
    }

    #[test]
    fn a_failure_a_conflict_or_a_comment_is_handed_back_once() {
        let mut t = task();
        let failing = parse_view(&view("OPEN", "MERGEABLE", FAIL, "")).unwrap();
        let Next::HandBack(note) = reconcile(&mut t, &failing, false) else { panic!("expected a hand-back") };
        assert!(note.contains("Test and Lint: https://ci/1") && note.contains("NO_CHANGE"));
        assert!(note.contains("git merge --ff-only origin/codex/neko-t"));
        assert_eq!(reconcile(&mut t, &failing, false), Next::Nothing, "the same failure is sent once");

        let both = parse_view(&view("OPEN", "CONFLICTING", PASS, PERSON)).unwrap();
        let Next::HandBack(note) = reconcile(&mut t, &both, false) else { panic!() };
        assert!(note.contains("git merge origin/main") && note.contains("@aakar: Use the shared helper"));
        assert_eq!(reconcile(&mut t, &both, false), Next::Nothing);
        assert_eq!(t.pull_request.as_ref().unwrap().follow_up.count, 2);
    }

    #[test]
    fn the_first_look_is_a_baseline_and_only_later_changes_are_handed_back() {
        let mut t = task();
        t.pull_request.as_mut().unwrap().status = None;
        let old = parse_view(&view("OPEN", "CONFLICTING", FAIL, PERSON)).unwrap();
        assert_eq!(reconcile(&mut t, &old, false), Next::Nothing, "turning the watch on starts nothing");
        assert_eq!(t.status, TaskStatus::ReadyForReview);
        let newer = view("OPEN", "CONFLICTING", FAIL, &[PERSON, r#"{"id":"c9","author":{"login":"aakar"},"authorAssociation":"MEMBER","body":"One more thing"}"#].join(","));
        let Next::HandBack(note) = reconcile(&mut t, &parse_view(&newer).unwrap(), false) else { panic!() };
        assert!(note.contains("One more thing") && !note.contains("Use the shared helper"), "only the new comment");
    }

    #[test]
    fn work_in_progress_is_left_alone_and_the_limit_hands_it_to_you() {
        let mut t = task();
        let failing = parse_view(&view("OPEN", "MERGEABLE", FAIL, "")).unwrap();
        assert_eq!(reconcile(&mut t, &failing, true), Next::Nothing, "a running agent pushes to the PR itself");
        t.pull_request.as_mut().unwrap().follow_up = PrFollowUp { count: MAX_FOLLOW_UPS, ..Default::default() };
        assert_eq!(reconcile(&mut t, &failing, false), Next::Nothing);
        assert_eq!(t.status, TaskStatus::AwaitingApproval);
        assert_eq!(reconcile(&mut t, &failing, false), Next::Nothing, "and is not repeated");
    }

    #[test]
    fn merging_waits_for_checks_and_stops_on_a_failure_or_conflict() {
        let mut t = task();
        t.pull_request.as_mut().unwrap().merge_when_ready = true;
        assert_eq!(reconcile(&mut t, &parse_view(&view("OPEN", "MERGEABLE", RUNNING, "")).unwrap(), false), Next::Nothing);
        assert_eq!(reconcile(&mut t, &parse_view(&view("OPEN", "UNKNOWN", PASS, "")).unwrap(), false), Next::Nothing, "GitHub still computing");
        assert_eq!(reconcile(&mut t, &parse_view(&view("OPEN", "MERGEABLE", PASS, "")).unwrap(), false), Next::Merge);
        assert!(matches!(reconcile(&mut t, &parse_view(&view("OPEN", "MERGEABLE", FAIL, "")).unwrap(), false), Next::MergeBlocked(_)));
        assert!(!t.pull_request.as_ref().unwrap().merge_when_ready);
        let mut t = task();
        t.pull_request.as_mut().unwrap().merge_when_ready = true;
        assert!(matches!(reconcile(&mut t, &parse_view(&view("OPEN", "MERGEABLE", PASS, PERSON)).unwrap(), false), Next::HandBack(_)),
                "a reviewer's new comment comes before merging");
    }

    #[test]
    fn handing_back_runs_the_task_directly_with_the_note() {
        let mut s = Snapshot { tasks: vec![task()], ..Default::default() };
        s.task_read_only.insert("t".into());
        hand_back(&mut s, "t", "fix CI").unwrap();
        let t = &s.tasks[0];
        assert_eq!(t.status, TaskStatus::Queued);
        assert!(t.events.iter().any(|e| e.role == crate::workbench::NOTE_ROLE && e.message == "fix CI"));
        assert!(s.start_when_planned.contains("t") && !s.task_read_only.contains("t"));
        assert!(s.task_replies["t"].handled, "nothing for the reply interpreter to read");
    }

    #[test]
    fn review_all_hands_back_open_ready_prs_and_names_the_others() {
        let mut second = task();
        second.id = "u".into();
        second.pull_request.as_mut().unwrap().url = "https://github.com/acme/app/pull/8".into();
        let mut merged = task();
        merged.id = "m".into();
        merged.pull_request.as_mut().unwrap().status = Some(PrStatus { state: "MERGED".into(), ..Default::default() });
        let mut s = Snapshot { tasks: vec![task(), second, merged], ..Default::default() };
        assert_eq!(review_all(&mut s, None).unwrap(), 2);
        let note = &s.tasks[0].events.last().unwrap().message;
        assert!(note.contains("SCRAP:") && note.contains("pull/8") && !note.contains("pull/7 Fix"));
        assert_eq!(s.tasks[2].status, TaskStatus::ReadyForReview, "a merged PR is not re-checked");
    }

    #[test]
    fn a_follow_up_becomes_one_proposed_task_in_the_named_repository() {
        let mut parent = task();
        parent.result = "Fixed the client side.\nFOLLOW_UP: streaming_service | Close web streams on the JSON stop chunk | It only stops on the raw marker\nFOLLOW_UP: nowhere | Something else | x".into();
        let mut s = Snapshot { tasks: vec![parent], ..Default::default() };
        let candidates = |_: &str| vec![
            crate::workbench::RepositoryCandidate { path: "/code/athena".into(), remote: "git@github.com:acme/athena.git".into(), package: String::new() },
            crate::workbench::RepositoryCandidate { path: "/code/stream".into(), remote: "https://github.com/acme/streaming_service.git".into(), package: String::new() },
        ];
        assert!(propose_follow_ups(&mut s, candidates).unwrap());
        assert_eq!(s.tasks.len(), 2);
        let child = &s.tasks[1];
        assert_eq!((child.title.as_str(), child.status), ("Close web streams on the JSON stop chunk", TaskStatus::AwaitingApproval));
        assert_eq!(s.task_roots[&child.id], "/code/stream", "matched by its remote's name");
        assert_eq!(s.follow_up_of[&child.id], "t");
        assert!(s.tasks[0].events.iter().any(|e| e.message.contains("no repository by that name")));
        assert!(!propose_follow_ups(&mut s, candidates).unwrap(), "proposed once");
        assert_eq!(s.tasks.len(), 2);
    }

    #[test]
    fn follow_ups_and_urls_parse() {
        let found = follow_ups("Fixed the client.\n- FOLLOW_UP: streaming_service | Close web streams on the JSON stop chunk | The endpoint only stops on '#$T0P'\nFOLLOW_UP: | missing repo");
        assert_eq!(found, [FollowUp { repository: "streaming_service".into(), title: "Close web streams on the JSON stop chunk".into(), detail: "The endpoint only stops on '#$T0P'".into() }]);
        assert_eq!(locate("https://github.com/HealthifyMe/athena/pull/4618"), Some(("HealthifyMe/athena".into(), 4618)));
        assert_eq!(locate("https://github.com/a/b/issues/3"), None);
        let inline = parse_inline_comments(r#"[{"id":5,"user":{"login":"aakar","type":"User"},"author_association":"MEMBER","body":"nit","path":"a.ts","line":3},
            {"id":6,"user":{"login":"coderabbitai[bot]","type":"Bot"},"author_association":"NONE","body":"x","path":"a.ts"}]"#, "me");
        assert_eq!(inline, [Comment { id: "5".into(), author: "aakar".into(), body: "nit".into(), place: Some("a.ts:3".into()) }]);
    }
}
