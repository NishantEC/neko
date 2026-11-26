//! What goes ahead without asking (docs/knowledge/permissions.md).
//!
//! "Ask before risky steps", the default: reading anywhere in the task's folder, and editing there when the folder is the task's
//! own (a scratch folder or a git worktree), go ahead. Everything else asks the person in the task: running a command, reading or
//! writing outside the folder, the network, a tool Neko does not know, and the first edit in a chosen folder that is not a
//! repository (there is no branch to undo it). "Full access" lets everything go ahead.
//!
//! An answer of "Always allow" lets the same kind of step go ahead for the rest of the task only: the same program for a
//! command, the same folder outside the task's own, the same host on the network. A destructive step (it deletes or overwrites
//! things) can be allowed only once, never always.

use std::path::{Component, Path, PathBuf};

use neko_protocol::PermissionMode;

use crate::runtime::Action;

/// What the policy knows about the task.
pub struct Context<'a> {
    pub workdir: &'a Path,
    pub mode: PermissionMode,
    /// A chosen folder that is not a repository: edits ask until the person says "Always allow".
    pub plain: bool,
    /// Rules answered "Always allow" in this task.
    pub always: &'a [String],
}

/// The policy's answer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Ask(Ask),
}

/// A request to put to the person.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ask {
    /// What it wants to do, in words.
    pub title: String,
    /// Why Neko asks.
    pub why: String,
    /// The exact command, path or address.
    pub detail: String,
    pub destructive: bool,
    /// What "Always allow" would allow (empty for a destructive step: never always).
    pub rule: String,
}

/// `path` made absolute against `base`, with `.` and `..` resolved by name (the file may not exist yet).
pub fn resolve(base: &Path, path: &str) -> PathBuf {
    let p = Path::new(path);
    let joined = if p.is_absolute() { p.to_path_buf() } else { base.join(p) };
    let mut out = PathBuf::new();
    for c in joined.components() {
        match c {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

/// Whether `path` is inside `dir` (or is it), with links followed where they exist: a link inside the folder that points out of it
/// is outside.
pub fn inside(dir: &Path, path: &Path) -> bool {
    let canon = |p: &Path| {
        // The deepest part that exists, with links followed, and the rest after it.
        let mut head = p.to_path_buf();
        let mut rest = Vec::new();
        while !head.exists() {
            match (head.file_name().map(ToOwned::to_owned), head.parent().map(Path::to_path_buf)) {
                (Some(n), Some(parent)) => {
                    rest.push(n);
                    head = parent;
                }
                _ => break,
            }
        }
        let mut c = head.canonicalize().unwrap_or(head);
        for n in rest.into_iter().rev() {
            c.push(n);
        }
        c
    };
    canon(path).starts_with(canon(dir))
}

/// Commands that delete or overwrite things.
pub fn destructive(command: &str) -> bool {
    let c = format!(" {} ", command.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" "));
    [
        " rm ", " rmdir ", " unlink ", " shred ", " mkfs", " dd ", " sudo ", " chmod -r", " chown -r", "git push --force", "git push -f", "git reset --hard", "git clean -", "git checkout -- ",
        "git branch -d", "git branch -D", " truncate ", "drop table", "drop database", " > /", " kill ", " killall ", " pkill ", "launchctl ", " diskutil ",
    ]
    .iter()
    .any(|p| c.contains(&p.to_lowercase()))
        || c.contains(" rm -") // rm with flags
}

/// The program a command runs first ("git" in `cd x && git status`), for an "always" rule.
fn program(command: &str) -> String {
    let first = command.split(['&', '|', ';', '\n']).map(str::trim).find(|s| !s.is_empty() && !s.starts_with("cd ")).unwrap_or(command.trim());
    first.split_whitespace().find(|w| !w.contains('=')).unwrap_or("").rsplit('/').next().unwrap_or("").to_owned()
}

fn host(target: &str) -> String {
    let rest = target.split("://").nth(1).unwrap_or(target);
    rest.split(['/', '?', '#']).next().unwrap_or(rest).to_lowercase()
}

/// Decides one step.
pub fn decide(action: &Action, cx: &Context<'_>) -> Verdict {
    if cx.mode == PermissionMode::Full {
        return Verdict::Allow;
    }
    let ask = |title: String, why: &str, detail: String, destructive: bool, rule: String| {
        if !destructive && cx.always.contains(&rule) {
            return Verdict::Allow;
        }
        Verdict::Ask(Ask { title, why: why.to_owned(), detail, destructive, rule: if destructive { String::new() } else { rule } })
    };
    match action {
        Action::Read { path } => {
            let p = resolve(cx.workdir, path);
            if inside(cx.workdir, &p) {
                return Verdict::Allow;
            }
            let folder = p.parent().unwrap_or(&p).to_string_lossy().into_owned();
            ask("Read outside the task's folder".into(), "It is outside the folder this task works in.", p.to_string_lossy().into_owned(), false, format!("read:{folder}"))
        }
        Action::Edit { path } => {
            let p = resolve(cx.workdir, path);
            let shown = p.to_string_lossy().into_owned();
            if inside(cx.workdir, &p) {
                if !cx.plain {
                    return Verdict::Allow;
                }
                return ask(
                    "Edit files in this folder".into(),
                    "This folder is not a git repository, so there is no branch to undo the change.",
                    shown,
                    false,
                    format!("edit:{}", cx.workdir.to_string_lossy()),
                );
            }
            let folder = p.parent().unwrap_or(&p).to_string_lossy().into_owned();
            ask("Change a file outside the task's folder".into(), "It is outside the folder this task works in.", shown, false, format!("edit:{folder}"))
        }
        Action::Command { command } => {
            let d = destructive(command);
            let why = if d { "This command deletes or overwrites things." } else { "Commands can change anything on this Mac." };
            ask(if d { "Run a command that deletes or overwrites".into() } else { "Run a command".into() }, why, command.clone(), d, format!("command:{}", program(command)))
        }
        Action::Network { target } => ask("Reach the internet".into(), "It sends a request out of this Mac.", target.clone(), false, format!("network:{}", host(target))),
        Action::Other { summary } => {
            let tool = summary.split_whitespace().next().unwrap_or("tool").to_owned();
            ask(format!("Use {tool}"), "Neko does not know what this tool does.", summary.clone(), false, format!("tool:{tool}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx<'a>(always: &'a [String], plain: bool) -> Context<'a> {
        Context { workdir: Path::new("/tmp/neko-policy-w"), mode: PermissionMode::Ask, plain, always }
    }

    fn asked(v: Verdict) -> Ask {
        match v {
            Verdict::Ask(a) => a,
            Verdict::Allow => panic!("expected to ask"),
        }
    }

    #[test]
    fn reading_and_editing_inside_go_ahead_and_outside_asks() {
        let c = cx(&[], false);
        assert_eq!(decide(&Action::Read { path: "/tmp/neko-policy-w/src/a.rs".into() }, &c), Verdict::Allow);
        assert_eq!(decide(&Action::Read { path: "src/../b.rs".into() }, &c), Verdict::Allow);
        assert_eq!(decide(&Action::Edit { path: "/tmp/neko-policy-w/a.rs".into() }, &c), Verdict::Allow);
        assert!(matches!(decide(&Action::Read { path: "../secret".into() }, &c), Verdict::Ask(_)));
        assert!(matches!(decide(&Action::Read { path: "/tmp/neko-policy-w-other/x".into() }, &c), Verdict::Ask(_)), "a sibling with the same prefix is outside");
        assert!(matches!(decide(&Action::Edit { path: "/etc/hosts".into() }, &c), Verdict::Ask(_)));
    }

    #[test]
    fn a_plain_folder_asks_before_its_first_edit() {
        let a = asked(decide(&Action::Edit { path: "a.txt".into() }, &cx(&[], true)));
        assert_eq!(a.rule, "edit:/tmp/neko-policy-w");
        assert_eq!(decide(&Action::Edit { path: "a.txt".into() }, &cx(&[a.rule.clone()], true)), Verdict::Allow, "until always allowed");
    }

    #[test]
    fn commands_network_and_unknown_tools_ask_and_always_is_scoped() {
        let c = cx(&[], false);
        let a = asked(decide(&Action::Command { command: "cd sub && cargo test --all".into() }, &c));
        assert_eq!(a.rule, "command:cargo");
        assert!(!a.destructive);
        let rules = vec![a.rule];
        assert_eq!(decide(&Action::Command { command: "cargo build".into() }, &cx(&rules, false)), Verdict::Allow);
        assert!(matches!(decide(&Action::Command { command: "npm test".into() }, &cx(&rules, false)), Verdict::Ask(_)), "another program still asks");
        assert_eq!(asked(decide(&Action::Network { target: "https://example.com/a?b".into() }, &c)).rule, "network:example.com");
        assert_eq!(asked(decide(&Action::Other { summary: "mcp__x do it".into() }, &c)).rule, "tool:mcp__x");
    }

    #[test]
    fn destructive_steps_are_never_always() {
        let a = asked(decide(&Action::Command { command: "rm -rf build".into() }, &cx(&[], false)));
        assert!(a.destructive);
        assert_eq!(a.rule, "");
        let rules = vec!["command:rm".to_owned(), String::new()];
        assert!(matches!(decide(&Action::Command { command: "rm -rf build".into() }, &cx(&rules, false)), Verdict::Ask(_)));
        assert!(destructive("git push --force origin main"));
        assert!(!destructive("git status"));
        assert!(!destructive("cargo test"));
    }

    #[test]
    fn full_access_allows_everything() {
        let c = Context { workdir: Path::new("/w"), mode: PermissionMode::Full, plain: true, always: &[] };
        assert_eq!(decide(&Action::Command { command: "rm -rf /".into() }, &c), Verdict::Allow);
    }
}
