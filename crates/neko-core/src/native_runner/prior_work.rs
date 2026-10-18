//! Work that may already exist for a ticket, found before its first run.
//!
//! Measured 2026-10-18: all 17 Completed tickets changed no code. Six found
//! the fix already on an unpushed local branch, others found an open pull
//! request that covered it, and each spent a full agent run discovering that
//! from a fresh checkout of `main`. Before a ticket's first run, its issue
//! keys are looked up in local branches (names and commit messages not yet
//! in the ticket's base) and its keys or title in open pull requests, and
//! what is found goes to the agent as context. Best effort: any Git or `gh`
//! failure just finds nothing.

use super::*;

/// Bounds the context: a handful of leads is enough to check.
const MAX_LEADS: usize = 5;
const GH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PriorWork {
    /// `branch: latest matching commit subject` for local branches.
    pub branches: Vec<String>,
    /// `#123 title (branch) url` for open pull requests.
    pub pull_requests: Vec<String>,
}

impl PriorWork {
    pub fn is_empty(&self) -> bool {
        self.branches.is_empty() && self.pull_requests.is_empty()
    }

    /// Instructions for the agent, or `None` when nothing was found.
    pub fn context(&self) -> Option<String> {
        if self.is_empty() {
            return None;
        }
        let mut text = String::from("Existing work that may already cover this ticket (found by Neko before this run; this worktree starts from the base without it). Check it first. If it already fixes the ticket, do not redo it: end with `NO_CHANGE: covered by <branch or pull request>`. If it partly does, build on it and say how.");
        for branch in &self.branches {
            text.push_str(&format!("\n- Local branch {branch}"));
        }
        for pr in &self.pull_requests {
            text.push_str(&format!("\n- Open pull request {pr}"));
        }
        Some(text)
    }
}

/// Looks in `worktree`'s repository for local branches carrying `keys`
/// (excluding `own_branch`) and, with `gh`, open pull requests matching the
/// keys or `title`.
pub fn find(worktree: &Path, own_branch: &str, keys: &[String], title: &str, gh: Option<&Path>, cancel: &AtomicBool) -> PriorWork {
    let mut found = PriorWork::default();
    let branches = git_output(worktree, &["for-each-ref", "--format=%(refname:short)", "refs/heads"], cancel).unwrap_or_default();
    for key in keys {
        let lower = key.to_lowercase();
        for name in branches.lines().filter(|b| *b != own_branch && b.to_lowercase().contains(&lower)) {
            let subject = git_output(worktree, &["log", "-1", "--format=%s", name], cancel).unwrap_or_default();
            push(&mut found.branches, format!("{name}: {}", subject.trim()));
        }
        // Commits mentioning the key on local branches the base lacks.
        let log = git_output(
            worktree,
            &["log", "--branches", "--not", "HEAD", "--source", "--regexp-ignore-case", "--fixed-strings", &format!("--grep={key}"), "--format=%S%x09%s", "-n", "20"],
            cancel,
        )
        .unwrap_or_default();
        for line in log.lines() {
            let Some((branch, subject)) = line.split_once('\t') else { continue };
            let branch = branch.strip_prefix("refs/heads/").unwrap_or(branch);
            if branch != own_branch && !found.branches.iter().any(|b| b.starts_with(&format!("{branch}:"))) {
                push(&mut found.branches, format!("{branch}: {subject}"));
            }
        }
    }
    if let Some(gh) = gh {
        let mut queries: Vec<String> = keys.iter().take(2).map(|k| format!("\"{k}\"")).collect();
        let words = crate::duplicates::normalized_title(title);
        let words: Vec<&str> = words.split_whitespace().filter(|w| w.len() > 2).take(8).collect();
        if words.len() >= 3 {
            queries.push(format!("{} in:title", words.join(" ")));
        }
        for query in queries {
            for pr in open_pull_requests(gh, worktree, &query, cancel) {
                if !pr.contains(&format!("({own_branch})")) {
                    push(&mut found.pull_requests, pr);
                }
            }
        }
    }
    found
}

fn push(list: &mut Vec<String>, item: String) {
    if list.len() < MAX_LEADS && !list.contains(&item) {
        list.push(bounded_text(&item, 300));
    }
}

fn open_pull_requests(gh: &Path, directory: &Path, query: &str, cancel: &AtomicBool) -> Vec<String> {
    let mut command = Command::new(gh);
    command
        .current_dir(directory)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .args(["pr", "list", "--state", "open", "--limit", "5", "--json", "number,title,headRefName,url", "--search", query]);
    let mut stdout = Vec::new();
    let Ok(output) = execute(command, &[], cancel, GH_TIMEOUT, |chunk| {
        if stdout.len() + chunk.len() > 256 * 1024 {
            return Err("gh output exceeded the limit".into());
        }
        stdout.extend_from_slice(chunk);
        Ok(())
    }) else {
        return Vec::new();
    };
    if !output.status.success() {
        return Vec::new();
    }
    #[derive(serde::Deserialize)]
    struct Pr {
        number: u64,
        title: String,
        #[serde(rename = "headRefName")]
        head: String,
        url: String,
    }
    serde_json::from_slice::<Vec<Pr>>(&stdout)
        .unwrap_or_default()
        .into_iter()
        .map(|pr| format!("#{} {} ({}) {}", pr.number, pr.title, pr.head, pr.url))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(repo: &Path, args: &[&str]) {
        let out = git_command(repo).args(["-c", "user.name=T", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn finds_unpushed_branches_and_open_pull_requests_for_the_ticket() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir(&repo).unwrap();
        git(&repo, &["init", "-q", "-b", "main"]);
        fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "base"]);
        // The real case: the fix sits on a feature branch nobody pushed.
        git(&repo, &["switch", "-q", "-c", "feat/report-review-call"]);
        fs::write(repo.join("a.txt"), "fixed\n").unwrap();
        git(&repo, &["commit", "-qam", "fix(review): coaches see only shared reports (HEA-3006)"]);
        git(&repo, &["switch", "-q", "-c", "hea-3007-moved-calls", "main"]);
        git(&repo, &["switch", "-q", "main"]);
        let worktree = create_worktree_in(&repo, "t1", &dir.path().join("wt")).unwrap();
        let cancel = AtomicBool::new(false);

        let gh = dir.path().join("gh");
        fs::write(&gh, "#!/bin/sh\necho '[{\"number\":4612,\"title\":\"fix: rate-limit GetOrCreateChannel\",\"headRefName\":\"codex/neko-x\",\"url\":\"https://github.com/o/r/pull/4612\"}]'\n").unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();

        let keys = vec!["HEA-3006".to_owned()];
        let found = find(&worktree, "codex/neko-t1", &keys, "HEA-3006: Coaches see only the reports shared", Some(&gh), &cancel);
        assert_eq!(found.branches, vec!["feat/report-review-call: fix(review): coaches see only shared reports (HEA-3006)".to_owned()]);
        assert_eq!(found.pull_requests, vec!["#4612 fix: rate-limit GetOrCreateChannel (codex/neko-x) https://github.com/o/r/pull/4612".to_owned()]);
        assert!(found.context().unwrap().contains("NO_CHANGE: covered by"));

        let by_name = find(&worktree, "codex/neko-t1", &["HEA-3007".to_owned()], "x", None, &cancel);
        assert_eq!(by_name.branches.len(), 1, "a branch named for the key counts too");
        let nothing = find(&worktree, "codex/neko-t1", &["HEA-9999".to_owned()], "x", None, &cancel);
        assert!(nothing.context().is_none());
    }
}
