//! What goes ahead without asking (docs/knowledge/permissions.md).
//!
//! "Follow Claude Code's own rules", the default: Neko adds nothing. Whatever reaches Neko is a step Claude Code itself would ask
//! about under the person's own settings, so it is asked in the task (Claude Code's bookkeeping tools aside).
//!
//! "Ask before risky steps": reading anywhere in the task's folder, and editing there when the folder is the task's own (a scratch
//! folder or a git worktree), go ahead. Everything else asks the person in the task: running any command, reading or writing
//! outside the folder, the network, a tool Neko does not know, and the first edit in a chosen folder that is not a repository
//! (there is no branch to undo it). Neko enforces this through a hook whatever the person's Claude Code settings allow.
//!
//! "Full access" lets everything go ahead.
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
    /// The plugins whose servers this run was given (their names), so a request for one of their tools says so.
    pub plugins: &'a [String],
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

/// Commands that delete or overwrite things: each part of a command line (split at `&&`, `||`, `;`, `|` and new lines) is read
/// by its program and arguments, so `grep -r rm src` is not `rm`.
pub fn destructive(command: &str) -> bool {
    let lower = command.to_lowercase();
    if ["drop table", "drop database", "drop schema", "truncate table", "delete from"].iter().any(|p| lower.contains(p)) {
        return true;
    }
    command.split(['&', '|', ';', '\n', '(', ')', '`']).any(|part| {
        let words: Vec<&str> = part.split_whitespace().skip_while(|w| w.contains('=') && !w.starts_with('-')).collect();
        let Some((first, args)) = words.split_first() else { return false };
        let program = first.rsplit('/').next().unwrap_or(first);
        let has = |flags: &[&str]| args.iter().any(|a| flags.contains(a));
        match program {
            "sudo" | "doas" | "rm" | "rmdir" | "unlink" | "shred" | "dd" | "truncate" | "kill" | "killall" | "pkill" | "launchctl" | "diskutil" | "srm" => true,
            p if p.starts_with("mkfs") => true,
            "chmod" | "chown" => has(&["-R", "-r", "--recursive"]),
            "find" => has(&["-delete"]) || args.windows(2).any(|w| w[0] == "-exec" && matches!(w[1], "rm" | "/bin/rm")),
            "git" => {
                let sub: Vec<&str> = args.iter().copied().skip_while(|a| a.starts_with('-')).collect();
                match sub.first().copied() {
                    Some("push") => sub.iter().any(|a| matches!(*a, "--force" | "-f" | "--force-with-lease" | "--delete" | "-d" | "--mirror") || a.starts_with('+') || a.starts_with("--force") || a.starts_with(":")),
                    Some("reset") => sub.contains(&"--hard") || sub.contains(&"--merge"),
                    Some("clean") => !sub.iter().any(|a| matches!(*a, "-n" | "--dry-run")),
                    Some("branch") => sub.iter().any(|a| matches!(*a, "-D" | "-d" | "--delete" | "-M" | "-f" | "--force")),
                    Some("checkout") => sub.iter().any(|a| matches!(*a, "--" | "." | "-f" | "--force")),
                    Some("restore") => true,
                    Some("stash") => sub.iter().any(|a| matches!(*a, "drop" | "clear")),
                    Some("filter-branch" | "filter-repo") => true,
                    Some("reflog") => sub.contains(&"expire") || sub.contains(&"delete"),
                    Some("update-ref") => has(&["-d"]),
                    _ => false,
                }
            }
            _ => false,
        }
    })
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
    if cx.mode == PermissionMode::Full || matches!(action, Action::Internal { .. }) {
        return Verdict::Allow;
    }
    // Following Claude Code's own rules, nothing goes ahead by Neko's say: what arrives is what Claude Code asks about.
    let shortcut = cx.mode == PermissionMode::Ask;
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
                if shortcut {
                    return Verdict::Allow;
                }
                let folder = p.parent().unwrap_or(&p).to_string_lossy().into_owned();
                return ask("Read a file".into(), "Claude Code asks before this under your own settings.", p.to_string_lossy().into_owned(), false, format!("read:{folder}"));
            }
            let folder = p.parent().unwrap_or(&p).to_string_lossy().into_owned();
            ask("Read outside the thread's folder".into(), "It is outside the folder this thread works in.", p.to_string_lossy().into_owned(), false, format!("read:{folder}"))
        }
        Action::Edit { path } => {
            let p = resolve(cx.workdir, path);
            let shown = p.to_string_lossy().into_owned();
            if inside(cx.workdir, &p) {
                if !shortcut {
                    return ask("Edit a file".into(), "Claude Code asks before this under your own settings.", shown, false, format!("edit:{}", cx.workdir.to_string_lossy()));
                }
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
            ask("Change a file outside the thread's folder".into(), "It is outside the folder this thread works in.", shown, false, format!("edit:{folder}"))
        }
        Action::Command { command } => {
            let d = destructive(command);
            let why = if d { "This command deletes or overwrites things." } else { "Commands can change anything on this Mac." };
            ask(if d { "Run a command that deletes or overwrites".into() } else { "Run a command".into() }, why, command.clone(), d, format!("command:{}", program(command)))
        }
        Action::Network { target } => ask("Reach the internet".into(), "It sends a request out of this Mac.", target.clone(), false, format!("network:{}", host(target))),
        Action::Mcp { server, tool, input } => {
            let ours = cx.plugins.iter().any(|p| p == server);
            let why = if ours { format!("{server} is a plugin you approved in Neko. Its tools can reach whatever the plugin can.") } else { format!("{server} is an MCP server from your own Claude Code settings. Its tools can reach whatever it can.") };
            ask(format!("Use {tool} from {server}"), &why, input.clone(), false, format!("tool:mcp__{server}__{tool}"))
        }
        Action::Other { summary } => {
            let tool = summary.split_whitespace().next().unwrap_or("tool").to_owned();
            ask(format!("Use {tool}"), "Neko does not know what this tool does.", summary.clone(), false, format!("tool:{tool}"))
        }
        Action::Internal { .. } => Verdict::Allow,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cx<'a>(always: &'a [String], plain: bool) -> Context<'a> {
        Context { workdir: Path::new("/tmp/neko-policy-w"), mode: PermissionMode::Ask, plain, always, plugins: &[] }
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
    fn following_claude_codes_rules_asks_whatever_reaches_neko() {
        let c = Context { workdir: Path::new("/tmp/neko-policy-w"), mode: PermissionMode::Follow, plain: false, always: &[], plugins: &[] };
        let a = asked(decide(&Action::Edit { path: "/tmp/neko-policy-w/a.rs".into() }, &c));
        assert_eq!(a.title, "Edit a file", "Claude Code asked, so the person is asked: Neko lets nothing through by itself");
        assert!(matches!(decide(&Action::Read { path: "/tmp/neko-policy-w/a.rs".into() }, &c), Verdict::Ask(_)));
        assert!(asked(decide(&Action::Command { command: "git reset --hard".into() }, &c)).destructive, "destructive keeps its form");
        let rules = vec![a.rule];
        assert_eq!(decide(&Action::Edit { path: "/tmp/neko-policy-w/b.rs".into() }, &Context { always: &rules, ..c }), Verdict::Allow, "always allow still holds");
        assert_eq!(decide(&Action::Internal { tool: "TodoWrite".into() }, &c), Verdict::Allow);
    }

    #[test]
    fn every_destructive_command_is_known() {
        for c in ["rm -rf build", "git push --force origin main", "git push -f", "git branch -D old", "git reset --hard HEAD~1", "git clean -fdx", "psql -c 'DROP TABLE users'", "sqlite3 a.db 'drop database x'", "cd x && rm a.txt"] {
            assert!(destructive(c), "{c}");
        }
        for c in ["git status", "cargo test", "ls -la", "git branch", "grep -r rm src"] {
            assert!(!destructive(c), "{c}");
        }
    }

    #[test]
    fn a_plugins_tool_asks_by_name_and_always_is_that_tool_alone() {
        let plugins = vec!["acme-docs".to_owned()];
        let c = Context { plugins: &plugins, ..cx(&[], false) };
        let call = Action::Mcp { server: "acme-docs".into(), tool: "search".into(), input: r#"{"query":"retry"}"#.into() };
        let a = asked(decide(&call, &c));
        assert_eq!(a.title, "Use search from acme-docs");
        assert!(a.why.contains("a plugin you approved in Neko"), "{}", a.why);
        assert_eq!(a.detail, r#"{"query":"retry"}"#);
        assert_eq!(a.rule, "tool:mcp__acme-docs__search");
        assert!(!a.destructive);
        let rules = vec![a.rule];
        assert_eq!(decide(&call, &Context { always: &rules, ..c }), Verdict::Allow);
        let other = Action::Mcp { server: "acme-docs".into(), tool: "delete_page".into(), input: "{}".into() };
        assert!(matches!(decide(&other, &Context { always: &rules, ..cx(&[], false) }), Verdict::Ask(_)), "another tool of the same plugin still asks");
        let own = asked(decide(&Action::Mcp { server: "notes".into(), tool: "read".into(), input: "{}".into() }, &cx(&[], false)));
        assert!(own.why.contains("your own Claude Code settings"), "{}", own.why);
        let follow = Context { mode: PermissionMode::Follow, ..cx(&[], false) };
        assert!(matches!(decide(&call, &follow), Verdict::Ask(_)));
        let full = Context { mode: PermissionMode::Full, ..cx(&[], false) };
        assert_eq!(decide(&call, &full), Verdict::Allow);
    }

    #[test]
    fn full_access_allows_everything() {
        let c = Context { workdir: Path::new("/w"), mode: PermissionMode::Full, plain: true, always: &[], plugins: &[] };
        assert_eq!(decide(&Action::Command { command: "rm -rf /".into() }, &c), Verdict::Allow);
    }
}
