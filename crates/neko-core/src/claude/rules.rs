//! What the person's own Claude Code settings let it do without asking, in one line for Settings (docs/knowledge/permissions.md).
//!
//! Read from the files Claude Code reads, highest first: managed settings, the folder's `.claude/settings.local.json` and
//! `.claude/settings.json`, then `~/.claude/settings.json`. Neko only reads them; it never writes a settings file. Allow rules from
//! every file add up (as Claude Code merges lists); the default mode is the highest file's that sets one.

use std::path::{Path, PathBuf};

use serde_json::Value;

/// Where managed settings live on a Mac.
pub const MANAGED: &str = "/Library/Application Support/ClaudeCode/managed-settings.json";

/// What the files say.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Rules {
    /// Tools allowed outright ("Bash", "Edit"), in the order first met.
    pub tools: Vec<String>,
    /// Allow rules narrowed to some calls ("Bash(git status:*)").
    pub narrower: usize,
    /// `permissions.defaultMode`, when a file sets it.
    pub default_mode: Option<String>,
}

/// The files, highest precedence first.
pub fn files(home: &Path, folder: Option<&Path>) -> Vec<PathBuf> {
    let mut f = vec![PathBuf::from(MANAGED)];
    if let Some(d) = folder {
        f.push(d.join(".claude/settings.local.json"));
        f.push(d.join(".claude/settings.json"));
    }
    f.push(home.join(".claude/settings.json"));
    f
}

/// Reads the files that exist. A file that is not JSON is skipped, as Claude Code skips it.
pub fn read(paths: &[PathBuf]) -> Rules {
    let mut r = Rules::default();
    for p in paths {
        let Some(v) = std::fs::read(p).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()) else { continue };
        let perms = &v["permissions"];
        if r.default_mode.is_none() {
            r.default_mode = perms["defaultMode"].as_str().map(str::to_owned);
        }
        for rule in perms["allow"].as_array().into_iter().flatten().filter_map(Value::as_str) {
            let rule = rule.trim();
            if rule.contains('(') && !rule.ends_with("(*)") && !rule.ends_with("(**)") {
                r.narrower += 1;
            } else {
                let tool = rule.split('(').next().unwrap_or(rule).to_owned();
                if !tool.is_empty() && !r.tools.contains(&tool) {
                    r.tools.push(tool);
                }
            }
        }
    }
    r
}

fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// The one line Settings shows.
pub fn words(r: &Rules) -> String {
    if r.default_mode.as_deref() == Some("bypassPermissions") {
        return "Your Claude Code settings let it do anything without asking.".to_owned();
    }
    let mut line = match (r.tools.is_empty(), r.narrower) {
        (true, 0) => "Your Claude Code settings allow nothing extra, so Claude Code asks before edits and commands.".to_owned(),
        (true, n) => format!("Your Claude Code settings allow {n} narrower rule{} without asking.", if n == 1 { "" } else { "s" }),
        (false, 0) => format!("Your Claude Code settings allow {} without asking.", list(&r.tools)),
        (false, n) => format!("Your Claude Code settings allow {} without asking, and {n} narrower rule{}.", list(&r.tools), if n == 1 { "" } else { "s" }),
    };
    match r.default_mode.as_deref() {
        Some("auto") => line.push_str(" Its default mode is auto: Claude Code decides the rest by itself."),
        Some("acceptEdits") => line.push_str(" Its default mode accepts file edits without asking."),
        Some("plan") => line.push_str(" Its default mode is plan: it changes nothing until a plan is approved."),
        Some("dontAsk") => line.push_str(" Its default mode denies whatever it would ask about."),
        _ => {}
    }
    line
}

/// The line for this Mac: `home`'s settings and, when given, a folder's own.
pub fn summary(home: &Path, folder: Option<&Path>) -> String {
    words(&read(&files(home, folder)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rules(tools: &[&str], narrower: usize, mode: Option<&str>) -> Rules {
        Rules { tools: tools.iter().map(|s| (*s).to_owned()).collect(), narrower, default_mode: mode.map(str::to_owned) }
    }

    #[test]
    fn the_line_says_what_runs_without_asking() {
        assert_eq!(words(&rules(&["Bash", "Edit", "Write"], 0, None)), "Your Claude Code settings allow Bash, Edit and Write without asking.");
        assert_eq!(words(&rules(&["Bash"], 2, Some("auto"))), "Your Claude Code settings allow Bash without asking, and 2 narrower rules. Its default mode is auto: Claude Code decides the rest by itself.");
        assert_eq!(words(&rules(&[], 0, None)), "Your Claude Code settings allow nothing extra, so Claude Code asks before edits and commands.");
        assert_eq!(words(&rules(&["Read"], 0, Some("bypassPermissions"))), "Your Claude Code settings let it do anything without asking.");
    }

    #[test]
    fn files_add_their_rules_and_the_highest_sets_the_mode() {
        let d = std::env::temp_dir().join(format!("neko-rules-{}", std::process::id()));
        let (home, folder) = (d.join("home"), d.join("acme"));
        std::fs::create_dir_all(home.join(".claude")).unwrap();
        std::fs::create_dir_all(folder.join(".claude")).unwrap();
        std::fs::write(home.join(".claude/settings.json"), r#"{"permissions":{"allow":["Bash","Edit","Bash(git status:*)"],"defaultMode":"auto"}}"#).unwrap();
        std::fs::write(folder.join(".claude/settings.local.json"), r#"{"permissions":{"allow":["Write","Edit"],"defaultMode":"acceptEdits"}}"#).unwrap();
        std::fs::write(folder.join(".claude/settings.json"), "not json").unwrap();
        let r = read(&files(&home, Some(&folder))[1..]);
        assert_eq!(r, rules(&["Write", "Edit", "Bash"], 1, Some("acceptEdits")));
        let before = std::fs::read(home.join(".claude/settings.json")).unwrap();
        let _ = summary(&home, Some(&folder));
        assert_eq!(std::fs::read(home.join(".claude/settings.json")).unwrap(), before, "read, never written");
        let _ = std::fs::remove_dir_all(&d);
    }
}
