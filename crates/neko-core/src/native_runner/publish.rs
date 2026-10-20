//! Publishing a reviewed task result: commit the worktree, push the task
//! branch, and open (or reuse) a GitHub pull request.
//!
//! This runs on the host after an independent review has passed, never inside
//! a model run, so only the exact worktree state the reviewer certified leaves
//! the machine. Workers themselves are still told not to push. Nothing here
//! merges: a pull request is a proposal for the person to accept.

use super::{AtomicBool, Command, Duration, Path, PathBuf, bounded_text, checkout_lock, execute, executable_directories, git_command, git_output, wait_for_checkout, MAX_EVENT};
use std::fs;
use std::os::unix::fs::PermissionsExt;

/// Task branches are created as `codex/neko-<id>`; anything else is not ours to push.
const TASK_BRANCH_PREFIX: &str = "codex/neko-";
const PUSH_TIMEOUT: Duration = Duration::from_secs(120);
const GH_TIMEOUT: Duration = Duration::from_secs(60);
const MAX_TITLE: usize = 120;

/// How a reviewed change is named (a Conventional Commits subject, used for
/// the commit and the pull request title) and explained (the PR body).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Description {
    pub title: String,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub url: String,
    pub branch: String,
    /// False when the branch already had an open pull request, which the push updated.
    pub created: bool,
}

/// `owner/repo` for a GitHub remote URL, in any of the forms Git accepts.
pub fn github_repository(remote: &str) -> Option<String> {
    let remote = remote.trim();
    let path = remote
        .strip_prefix("git@github.com:")
        .or_else(|| remote.strip_prefix("ssh://git@github.com/"))
        .or_else(|| {
            let rest = remote.strip_prefix("https://")?;
            let rest = rest.split_once('@').map_or(rest, |(_, host)| host);
            rest.strip_prefix("github.com/")
        })?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let (owner, name) = path.split_once('/')?;
    let valid = |s: &str| !s.is_empty() && s != "." && s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
    (valid(owner) && valid(name)).then(|| format!("{owner}/{name}"))
}

/// The GitHub CLI Neko uses to open pull requests. `NEKO_GH_PATH` overrides discovery.
pub fn resolve_gh() -> Result<PathBuf, String> {
    let executable = |path: &Path| path.is_absolute() && fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
    if let Some(path) = std::env::var_os("NEKO_GH_PATH").map(PathBuf::from) {
        return if executable(&path) { Ok(path) } else { Err("NEKO_GH_PATH is not an executable file".into()) };
    }
    executable_directories()
        .into_iter()
        .map(|dir| dir.join("gh"))
        .find(|path| executable(path))
        .ok_or_else(|| "GitHub CLI (gh) is not installed. Install it and run `gh auth login` so Neko can open pull requests.".into())
}

/// Commit everything the reviewer saw, push the task branch and open a pull
/// request. `Ok(None)` means there was nothing to publish since `base`.
/// `describe` runs once the change is staged, so it can read exactly what is
/// about to be committed. `proceed` is checked immediately before the push,
/// the first step that leaves the machine, so a ticket that changed
/// meanwhile is not published.
pub fn publish(
    directory: &Path,
    base: &str,
    describe: &dyn Fn() -> Description,
    proceed: &dyn Fn() -> bool,
    cancel: &AtomicBool,
) -> Result<Option<Published>, String> {
    publish_with(&resolve_gh()?, directory, base, describe, proceed, cancel)
}

/// The task worktree's own `codex/neko-*` branch; nothing else is published.
fn task_branch(directory: &Path, cancel: &AtomicBool) -> Result<String, String> {
    let branch = git_output(directory, &["symbolic-ref", "--short", "HEAD"], cancel)
        .map_err(|_| "The task worktree is not on its own branch".to_string())?
        .trim()
        .to_owned();
    if !branch.starts_with(TASK_BRANCH_PREFIX) {
        return Err(format!("Refusing to publish branch {branch}: only Neko task branches are published"));
    }
    Ok(branch)
}

/// A git command that can write commits even where no identity is configured.
fn identified_git(directory: &Path, cancel: &AtomicBool) -> Command {
    let mut command = git_command(directory);
    if git_output(directory, &["config", "user.email"], cancel).is_err() {
        command.args(["-c", "user.name=Neko", "-c", "user.email=neko@localhost.invalid"]);
    }
    command
}

/// Stage everything the reviewer saw, describe it, and commit it under the
/// description's title. `None` when the branch has nothing beyond `base`, so
/// there is nothing to deliver (and nothing is described).
fn commit_reviewed(directory: &Path, base: &str, describe: &dyn Fn() -> Description, cancel: &AtomicBool) -> Result<Option<Description>, String> {
    let dirty = !git_output(directory, &["status", "--porcelain=v1", "-z", "--untracked-files=all"], cancel)?.is_empty();
    if dirty {
        let mut add = git_command(directory);
        add.args(["add", "-A"]);
        run("git add", add, b"", cancel, PUSH_TIMEOUT)?;
    } else if git_output(directory, &["rev-parse", "HEAD"], cancel)?.trim() == base.trim() {
        return Ok(None);
    }
    let description = describe();
    let description = Description { title: one_line(&description.title), body: description.body };
    if dirty {
        let mut commit = identified_git(directory, cancel);
        commit.args(["commit", "--no-verify", "--quiet", "-F", "-"]);
        let message = format!("{}\n\nReviewed by Neko before publishing.\n", description.title);
        run("git commit", commit, message.as_bytes(), cancel, PUSH_TIMEOUT)?;
    }
    Ok(Some(description))
}

/// The ordinary checkout a task worktree was created from.
pub fn source_checkout(directory: &Path, cancel: &AtomicBool) -> Result<PathBuf, String> {
    let common = git_output(directory, &["rev-parse", "--path-format=absolute", "--git-common-dir"], cancel)?;
    let common = Path::new(common.trim());
    let checkout = common.parent().filter(|_| common.file_name().is_some_and(|n| n == ".git"))
        .ok_or("The task's repository is not an ordinary checkout")?;
    checkout.canonicalize().map_err(|_| "The task's repository is missing".into())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Landed {
    /// What `main` points at now.
    pub commit: String,
    /// `main` had moved on, so the reviewed change was rebased onto it first.
    pub rebased: bool,
    /// Where `main` was just before landing: the base of what this ticket added.
    pub onto: String,
}

pub const LANDING_BRANCH: &str = "main";

/// Commit the reviewed worktree and fast-forward the source checkout's local
/// `main` to it. Nothing is pushed. When `main` has moved on since the ticket
/// started, the change is rebased onto it; a conflicting rebase is undone and
/// reported, leaving `main` untouched. Local edits in the source checkout are
/// kept: Git refuses the fast-forward rather than overwrite them.
pub fn land(
    directory: &Path,
    base: &str,
    describe: &dyn Fn() -> Description,
    proceed: &dyn Fn() -> bool,
    cancel: &AtomicBool,
) -> Result<Option<Landed>, String> {
    let branch = task_branch(directory, cancel)?;
    let source = source_checkout(directory, cancel)?;
    let target = format!("refs/heads/{LANDING_BRANCH}");
    git_output(&source, &["rev-parse", "--verify", "--quiet", &target], cancel)
        .map_err(|_| format!("This repository has no {LANDING_BRANCH} branch to land on"))?;
    if commit_reviewed(directory, base, describe, cancel)?.is_none() {
        return Ok(None);
    }
    // One landing at a time per repository; worktree creation shares the lock.
    let lock = checkout_lock(&source, cancel)?;
    let _guard = wait_for_checkout(&lock, cancel)?;
    let mut rebased = false;
    let ancestor = git_command(directory).args(["merge-base", "--is-ancestor", &target, "HEAD"]).status()
        .map_err(|e| format!("Cannot compare with {LANDING_BRANCH}: {e}"))?.success();
    if !ancestor {
        let mut rebase = identified_git(directory, cancel);
        rebase.args(["rebase", "--quiet", &target]);
        if let Err(error) = run("git rebase", rebase, b"", cancel, PUSH_TIMEOUT) {
            let mut abort = git_command(directory);
            abort.args(["rebase", "--abort"]);
            let _ = run("git rebase --abort", abort, b"", cancel, PUSH_TIMEOUT);
            return Err(format!("{LANDING_BRANCH} has moved on and the reviewed change conflicts with it, so nothing was landed ({error})"));
        }
        rebased = true;
    }
    if !proceed() {
        return Err(format!("The ticket changed before landing; {LANDING_BRANCH} was not touched"));
    }
    let onto = git_output(&source, &["rev-parse", &target], cancel)?.trim().to_owned();
    let on_target = git_output(&source, &["symbolic-ref", "--quiet", "HEAD"], cancel).is_ok_and(|head| head.trim() == target);
    let mut advance = git_command(&source);
    if on_target {
        // Updates the checked-out files too; refuses to overwrite local edits.
        advance.args(["merge", "--ff-only", "--quiet", &branch]);
    } else {
        // Moves the branch without touching whatever is checked out; refuses a non-fast-forward.
        advance.args(["fetch", "--quiet", ".", &format!("{branch}:{target}")]);
    }
    run(&format!("Fast-forwarding {LANDING_BRANCH}"), advance, b"", cancel, PUSH_TIMEOUT)?;
    let commit = git_output(&source, &["rev-parse", &target], cancel)?.trim().to_owned();
    Ok(Some(Landed { commit, rebased, onto }))
}

fn publish_with(
    gh: &Path,
    directory: &Path,
    base: &str,
    describe: &dyn Fn() -> Description,
    proceed: &dyn Fn() -> bool,
    cancel: &AtomicBool,
) -> Result<Option<Published>, String> {
    let branch = task_branch(directory, cancel)?;
    // The configured URL, not `remote get-url`, which applies local rewrites.
    let remote = git_output(directory, &["config", "--get", "remote.origin.url"], cancel)
        .map_err(|_| "This repository has no origin remote".to_string())?;
    let repository = github_repository(&remote)
        .ok_or_else(|| format!("Origin is not a GitHub repository ({}); only GitHub pull requests are supported", remote.trim()))?;
    let Some(Description { title, body }) = commit_reviewed(directory, base, describe, cancel)? else {
        return Ok(None);
    };
    if !proceed() {
        return Err("The ticket changed before publishing; nothing was pushed".into());
    }
    let mut push = git_command(directory);
    push.args(["push", "--porcelain", "origin"]).arg(format!("HEAD:refs/heads/{branch}"));
    run("git push", push, b"", cancel, PUSH_TIMEOUT)?;

    let mut list = gh_command(gh, directory);
    list.args(["pr", "list", "-R", &repository, "--head", &branch, "--state", "open", "--json", "url", "--jq", ".[0].url // empty"]);
    let existing = capture("gh pr list", list, b"", cancel, GH_TIMEOUT)?;
    if let Some(url) = pull_request_url(&existing) {
        // Keep the description true to what is now on the branch. The push
        // already succeeded, so a failed edit is not a failed publish.
        let mut edit = gh_command(gh, directory);
        edit.args(["pr", "edit", &url, "-R", &repository, "--title", &title, "--body-file", "-"]);
        let _ = capture("gh pr edit", edit, body.as_bytes(), cancel, GH_TIMEOUT);
        return Ok(Some(Published { url, branch, created: false }));
    }
    let mut create = gh_command(gh, directory);
    create.args(["pr", "create", "-R", &repository, "--head", &branch, "--title", &title, "--body-file", "-"]);
    let created = capture("gh pr create", create, body.as_bytes(), cancel, GH_TIMEOUT)?;
    let url = pull_request_url(&created).ok_or("GitHub did not return a pull request URL")?;
    Ok(Some(Published { url, branch, created: true }))
}

fn one_line(title: &str) -> String {
    let line = title.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("Neko task");
    bounded_text(line, MAX_TITLE)
}

fn pull_request_url(output: &str) -> Option<String> {
    output
        .lines()
        .map(str::trim)
        .rev()
        .find(|line| line.starts_with("https://") && line.contains("/pull/"))
        .map(str::to_owned)
}

fn gh_command(gh: &Path, directory: &Path) -> Command {
    let mut command = Command::new(gh);
    command
        .current_dir(directory)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    command
}

fn run(step: &str, command: Command, input: &[u8], cancel: &AtomicBool, timeout: Duration) -> Result<(), String> {
    capture(step, command, input, cancel, timeout).map(|_| ())
}

/// Runs the GitHub CLI with `args` and returns its output (at most 4 MiB:
/// a pull request's comments can be long). Used by Neko's pull request watch.
pub fn gh(args: &[&str], cancel: &AtomicBool, timeout: Duration) -> Result<String, String> {
    let mut command = Command::new(resolve_gh()?);
    command.args(args).env("GH_PROMPT_DISABLED", "1").env("NO_COLOR", "1");
    let mut stdout = Vec::new();
    let output = execute(command, b"", cancel, timeout, |chunk| {
        if stdout.len() + chunk.len() > 4 * 1024 * 1024 {
            return Err("GitHub's answer was larger than expected".into());
        }
        stdout.extend_from_slice(chunk);
        Ok(())
    })?;
    if !output.status.success() {
        let reason = bounded_text(output.stderr.trim(), MAX_EVENT);
        return Err(if reason.is_empty() { "gh failed with no details".into() } else { reason });
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

fn capture(step: &str, command: Command, input: &[u8], cancel: &AtomicBool, timeout: Duration) -> Result<String, String> {
    let mut stdout = Vec::new();
    let output = execute(command, input, cancel, timeout, |chunk| {
        if stdout.len() + chunk.len() > 64 * 1024 {
            return Err("Publishing output exceeded the limit".into());
        }
        stdout.extend_from_slice(chunk);
        Ok(())
    })?;
    if !output.status.success() {
        let reason = bounded_text(output.stderr.trim(), MAX_EVENT);
        return Err(format!("{step} failed: {}", if reason.is_empty() { "no details" } else { &reason }));
    }
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(title: &str, body: &str) -> Description {
        Description { title: title.into(), body: body.into() }
    }
    use std::process::Command as Process;

    fn git(directory: &Path, args: &[&str]) -> String {
        let output = Process::new("git").current_dir(directory).args(args).output().unwrap();
        assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        String::from_utf8_lossy(&output.stdout).trim().to_owned()
    }

    #[test]
    fn recognizes_only_github_remotes() {
        for remote in [
            "git@github.com:NishantEC/neko.git",
            "ssh://git@github.com/NishantEC/neko.git",
            "https://github.com/NishantEC/neko",
            "https://github.com/NishantEC/neko.git\n",
            "https://token@github.com/NishantEC/neko.git",
        ] {
            assert_eq!(github_repository(remote).as_deref(), Some("NishantEC/neko"), "{remote}");
        }
        for remote in ["https://gitlab.com/a/b.git", "/tmp/bare.git", "git@github.com:only-owner", "https://github.com/a/../b", "https://github.com/a/b c"] {
            assert_eq!(github_repository(remote), None, "{remote}");
        }
    }

    struct Fixture {
        _root: tempfile::TempDir,
        worktree: PathBuf,
        bare: PathBuf,
        gh: PathBuf,
        log: PathBuf,
        base: String,
    }

    /// A task worktree whose GitHub origin is rewritten to a local bare
    /// repository, plus a fake `gh` that records its calls.
    fn fixture(existing_pr: bool) -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let bare = root.path().join("origin.git");
        let worktree = root.path().join("work");
        git(root.path(), &["init", "--quiet", "--bare", bare.to_str().unwrap()]);
        fs::create_dir(&worktree).unwrap();
        git(&worktree, &["init", "--quiet", "-b", "main"]);
        git(&worktree, &["config", "user.email", "test@example.com"]);
        git(&worktree, &["config", "user.name", "Test"]);
        git(&worktree, &["config", "commit.gpgsign", "false"]);
        git(&worktree, &["remote", "add", "origin", "https://github.com/acme/widgets.git"]);
        git(&worktree, &["config", &format!("url.{}.insteadOf", bare.display()), "https://github.com/acme/widgets.git"]);
        fs::write(worktree.join("a.txt"), "one\n").unwrap();
        git(&worktree, &["add", "."]);
        git(&worktree, &["commit", "--quiet", "-m", "base"]);
        git(&worktree, &["switch", "--quiet", "-c", "codex/neko-task1"]);
        let base = git(&worktree, &["rev-parse", "HEAD"]);
        let log = root.path().join("gh.log");
        let gh = root.path().join("gh");
        let list = if existing_pr { "https://github.com/acme/widgets/pull/3" } else { "" };
        fs::write(&gh, format!(
            "#!/bin/sh\necho \"$@\" >> '{log}'\nif [ \"$2\" = list ]; then echo '{list}'; exit 0; fi\ncat > '{log}.body'\necho 'Creating pull request'\necho 'https://github.com/acme/widgets/pull/7'\n",
            log = log.display()
        )).unwrap();
        fs::set_permissions(&gh, fs::Permissions::from_mode(0o755)).unwrap();
        Fixture { _root: root, worktree, bare, gh, log, base }
    }

    #[test]
    fn commits_pushes_and_opens_a_pull_request_for_the_reviewed_worktree() {
        let f = fixture(false);
        fs::write(f.worktree.join("a.txt"), "two\n").unwrap();
        fs::write(f.worktree.join("new.txt"), "added\n").unwrap();
        let cancel = AtomicBool::new(false);
        let published = publish_with(&f.gh, &f.worktree, &f.base, &|| named("Fix the widget\nextra", "What changed"), &|| true, &cancel).unwrap().unwrap();
        assert_eq!(published, Published { url: "https://github.com/acme/widgets/pull/7".into(), branch: "codex/neko-task1".into(), created: true });
        // Everything the reviewer saw is committed and on the remote branch.
        assert!(git(&f.worktree, &["status", "--porcelain"]).is_empty());
        let pushed = git(&f.bare, &["rev-parse", "refs/heads/codex/neko-task1"]);
        assert_eq!(pushed, git(&f.worktree, &["rev-parse", "HEAD"]));
        assert_eq!(git(&f.bare, &["log", "-1", "--format=%s", "codex/neko-task1"]), "Fix the widget");
        assert_eq!(git(&f.bare, &["show", "codex/neko-task1:new.txt"]), "added");
        // No upstream tracking is written into the shared repository config.
        assert!(Process::new("git").current_dir(&f.worktree).args(["config", "branch.codex/neko-task1.remote"]).output().unwrap().stdout.is_empty());
        let calls = fs::read_to_string(&f.log).unwrap();
        assert!(calls.contains("pr list -R acme/widgets --head codex/neko-task1"), "{calls}");
        assert!(calls.contains("pr create -R acme/widgets --head codex/neko-task1 --title Fix the widget --body-file -"), "{calls}");
        assert_eq!(fs::read_to_string(format!("{}.body", f.log.display())).unwrap(), "What changed");
    }

    #[test]
    fn the_description_reads_the_staged_change_before_it_is_committed() {
        let f = fixture(false);
        fs::write(f.worktree.join("new.txt"), "added\n").unwrap();
        let cancel = AtomicBool::new(false);
        let seen = std::cell::RefCell::new(String::new());
        let describe = || {
            *seen.borrow_mut() = git(&f.worktree, &["diff", "--cached", "--name-only", &f.base]);
            assert_eq!(git(&f.worktree, &["rev-parse", "HEAD"]), f.base, "not committed yet");
            named("fix(widget): add the new file", "body")
        };
        publish_with(&f.gh, &f.worktree, &f.base, &describe, &|| true, &cancel).unwrap().unwrap();
        assert_eq!(*seen.borrow(), "new.txt");
        assert_eq!(git(&f.bare, &["log", "-1", "--format=%s", "codex/neko-task1"]), "fix(widget): add the new file");
        assert!(fs::read_to_string(&f.log).unwrap().contains("--title fix(widget): add the new file"));
    }

    #[test]
    fn an_open_pull_request_is_updated_rather_than_duplicated() {
        let f = fixture(true);
        fs::write(f.worktree.join("a.txt"), "two\n").unwrap();
        let cancel = AtomicBool::new(false);
        let published = publish_with(&f.gh, &f.worktree, &f.base, &|| named("Fix", "body"), &|| true, &cancel).unwrap().unwrap();
        assert_eq!(published.url, "https://github.com/acme/widgets/pull/3");
        assert!(!published.created);
        let calls = fs::read_to_string(&f.log).unwrap();
        assert!(!calls.contains("pr create"), "{calls}");
        assert!(calls.contains("pr edit https://github.com/acme/widgets/pull/3 -R acme/widgets --title Fix --body-file -"), "{calls}");
        assert_eq!(fs::read_to_string(format!("{}.body", f.log.display())).unwrap(), "body");
    }

    #[test]
    fn nothing_changed_publishes_nothing_and_a_stale_ticket_never_pushes() {
        let f = fixture(false);
        let cancel = AtomicBool::new(false);
        assert_eq!(publish_with(&f.gh, &f.worktree, &f.base, &|| named("Fix", "body"), &|| true, &cancel).unwrap(), None);
        fs::write(f.worktree.join("a.txt"), "two\n").unwrap();
        let error = publish_with(&f.gh, &f.worktree, &f.base, &|| named("Fix", "body"), &|| false, &cancel).unwrap_err();
        assert!(error.contains("nothing was pushed"), "{error}");
        assert!(Process::new("git").current_dir(&f.bare).args(["rev-parse", "--verify", "refs/heads/codex/neko-task1"]).output().unwrap().stdout.is_empty());
        assert!(!f.log.exists());
    }

    #[test]
    fn refuses_branches_and_remotes_that_are_not_neko_task_github_ones() {
        let f = fixture(false);
        let cancel = AtomicBool::new(false);
        git(&f.worktree, &["switch", "--quiet", "main"]);
        assert!(publish_with(&f.gh, &f.worktree, &f.base, &|| named("t", "b"), &|| true, &cancel).unwrap_err().contains("only Neko task branches"));
        git(&f.worktree, &["switch", "--quiet", "codex/neko-task1"]);
        git(&f.worktree, &["remote", "set-url", "origin", "https://gitlab.com/acme/widgets.git"]);
        assert!(publish_with(&f.gh, &f.worktree, &f.base, &|| named("t", "b"), &|| true, &cancel).unwrap_err().contains("not a GitHub repository"));
    }

    /// A source checkout on `main` plus a task worktree branched from it.
    fn landing_fixture() -> (tempfile::TempDir, PathBuf, PathBuf, String) {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("repo");
        fs::create_dir(&source).unwrap();
        git(&source, &["init", "--quiet", "-b", "main"]);
        git(&source, &["config", "user.email", "test@example.com"]);
        git(&source, &["config", "user.name", "Test"]);
        git(&source, &["config", "commit.gpgsign", "false"]);
        fs::write(source.join("a.txt"), "one\n").unwrap();
        fs::write(source.join("b.txt"), "b\n").unwrap();
        git(&source, &["add", "."]);
        git(&source, &["commit", "--quiet", "-m", "base"]);
        let base = git(&source, &["rev-parse", "HEAD"]);
        let worktree = root.path().join("task");
        git(&source, &["worktree", "add", "--quiet", "-b", "codex/neko-t1", worktree.to_str().unwrap(), "HEAD"]);
        (root, source.canonicalize().unwrap(), worktree, base)
    }

    #[test]
    fn lands_the_reviewed_change_on_main_and_updates_the_checkout() {
        let (_root, source, worktree, base) = landing_fixture();
        assert_eq!(source_checkout(&worktree, &AtomicBool::new(false)).unwrap(), source);
        fs::write(worktree.join("a.txt"), "two\n").unwrap();
        let landed = land(&worktree, &base, &|| named("Fix a", ""), &|| true, &AtomicBool::new(false)).unwrap().unwrap();
        assert!(!landed.rebased);
        assert_eq!(landed.commit, git(&source, &["rev-parse", "main"]));
        assert_eq!(fs::read_to_string(source.join("a.txt")).unwrap(), "two\n", "checked-out main is updated");
        assert_eq!(git(&source, &["log", "-1", "--format=%s", "main"]), "Fix a");
        assert_eq!(land(&worktree, &landed.commit, &|| named("Fix a", ""), &|| true, &AtomicBool::new(false)).unwrap(), None);
    }

    #[test]
    fn rebases_onto_a_newer_main_and_refuses_conflicts_without_touching_main() {
        let (_root, source, worktree, base) = landing_fixture();
        fs::write(source.join("b.txt"), "b2\n").unwrap();
        git(&source, &["commit", "--quiet", "-am", "meanwhile"]);
        fs::write(worktree.join("a.txt"), "two\n").unwrap();
        let landed = land(&worktree, &base, &|| named("Fix a", ""), &|| true, &AtomicBool::new(false)).unwrap().unwrap();
        assert!(landed.rebased);
        assert_eq!(git(&source, &["log", "--format=%s", "main"]).lines().collect::<Vec<_>>(), ["Fix a", "meanwhile", "base"]);

        // Now a conflicting change on main.
        let start = git(&source, &["rev-parse", "main"]);
        fs::write(source.join("a.txt"), "three\n").unwrap();
        git(&source, &["commit", "--quiet", "-am", "conflicting"]);
        let main_before = git(&source, &["rev-parse", "main"]);
        fs::write(worktree.join("a.txt"), "four\n").unwrap();
        let error = land(&worktree, &start, &|| named("Fix again", ""), &|| true, &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("conflicts"), "{error}");
        assert_eq!(git(&source, &["rev-parse", "main"]), main_before);
        assert!(!worktree.join(".git").join("rebase-merge").exists());
        assert_eq!(git(&worktree, &["symbolic-ref", "--short", "HEAD"]), "codex/neko-t1", "the aborted rebase leaves the task branch checked out");
    }

    #[test]
    fn moves_main_without_touching_another_checked_out_branch_and_keeps_local_edits() {
        let (_root, source, worktree, base) = landing_fixture();
        git(&source, &["switch", "--quiet", "-c", "feature"]);
        fs::write(worktree.join("a.txt"), "two\n").unwrap();
        let landed = land(&worktree, &base, &|| named("Fix a", ""), &|| true, &AtomicBool::new(false)).unwrap().unwrap();
        assert_eq!(landed.commit, git(&source, &["rev-parse", "main"]));
        assert_eq!(git(&source, &["symbolic-ref", "--short", "HEAD"]), "feature");
        assert_eq!(fs::read_to_string(source.join("a.txt")).unwrap(), "one\n", "the feature checkout is untouched");

        // Back on main with an uncommitted edit to the file the next landing changes.
        git(&source, &["switch", "--quiet", "main"]);
        fs::write(source.join("a.txt"), "my local edit\n").unwrap();
        let before = git(&source, &["rev-parse", "main"]);
        fs::write(worktree.join("a.txt"), "three\n").unwrap();
        assert!(land(&worktree, &landed.commit, &|| named("Fix b", ""), &|| true, &AtomicBool::new(false)).is_err());
        assert_eq!(git(&source, &["rev-parse", "main"]), before);
        assert_eq!(fs::read_to_string(source.join("a.txt")).unwrap(), "my local edit\n");
        // A stale ticket never moves main.
        fs::write(source.join("a.txt"), "two\n").unwrap();
        let error = land(&worktree, &landed.commit, &|| named("Fix b", ""), &|| false, &AtomicBool::new(false)).unwrap_err();
        assert!(error.contains("not touched"), "{error}");
        assert_eq!(git(&source, &["rev-parse", "main"]), before);
    }

}
