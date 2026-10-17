//! Neko's rulebook: one document every agent works to.
//!
//! Neko is the user's chief of staff. It delegates work to task agents
//! (scouts, builders, reviewers, supervisors) and answers for all of it. The
//! rulebook is how its standards reach every one of them: the same text goes
//! into Neko's own prompt and into every task agent's prompt, so a rule the
//! user writes once applies everywhere. The user edits it in Settings; an
//! empty rulebook means the built-in default below.
//!
//! Some rules are also enforced in code (Neko alone commits, pushes and opens
//! pull requests; only reviewed work leaves the machine; a worktree exists
//! only while an agent works in it). Stating them here keeps agents from
//! fighting the code.

use neko_protocol::workbench::Snapshot;

/// The most a custom rulebook may hold.
pub const MAX_BYTES: usize = 32 * 1024;

pub const ENGINEERING_STANDARD: &str = "Engineering standard, the way an experienced engineer on this codebase works:
1. Prove the cause before fixing it. Read the real evidence through the user's tools (the error's actual events, stack traces, error codes and counts in Sentry; logs; the issue and its comments) and confirm the ticket's stated cause matches it. If the evidence points elsewhere, fix the real cause and say so.
2. Fix the class, not the instance. Search the codebase for the same pattern: other callers, sibling code paths, the same call without the guard. Handle the ones in scope and name the rest.
3. Weigh the approaches. When more than one fix is plausible, compare at least two, and check every contract the change touches: callers and what they do with return values, server routes and payload formats, backend APIs and types. Prefer the one that leaves unaffected paths exactly as they were; use an existing library helper only if its behaviour matches the contract.
4. Never hide a failure. An error you catch must still be reported as a handled error through the codebase's own logger or Sentry, and the person using the product must see a clear, human message, never raw error text or a spinner that never ends.
5. Lose nothing. Data, logging, analytics and behaviour that worked before must still work.
6. Prove the fix. Add a regression test that fails without the change and passes with it, asserting outcomes, with clean test hygiene (mocks restored in afterEach, no leaked state). Run the relevant tests, typecheck and lint.
7. Look around. Check open pull requests and recent commits touching the same code, and state any ordering or conflict.
Report the cause the evidence supports, the alternatives you weighed and why you chose this one.";

pub const REVIEW_STANDARD: &str = "Review as an adversarial senior engineer, not a checker of the builder's claims: re-establish the cause from the evidence yourself, search for sibling occurrences, and check the contracts the change touches. Each of these is a finding (passed=false), with the fix you recommend: a cause the evidence does not support, an in-scope sibling left unfixed, a caught error that is no longer reported, raw error text or an endless wait shown to users, lost data or logging, a regression test that would pass without the fix, or leaking test state.";

pub const MACHINE: &str = "This machine is shared with the user's own work and other agents, and it has overheated and restarted under load before:
1. One heavy job at a time: no parallel dependency installs, full test suites, Xcode or Docker builds. If the one-minute load average is above 20, wait.
2. Keep build output inside your task folder (Xcode -derivedDataPath, caches, bundles), never in /tmp or a shared location. Your task folder is removed when you stop, and everything inside it goes with it.
3. Never create extra Git worktrees or clones; work in the folder you were given.
4. For a browser, use the shared one (~/.agents/bin/shared-chrome acquire, connect over CDP, release); never launch your own.
5. Stop anything you started (servers, simulators, watchers) before you finish.";

pub const REPOSITORIES: &str = "Repositories:
1. Neko alone commits, pushes and opens pull requests, and only after an independent review passes. Nothing is ever merged by an agent.
2. Commit and pull request titles follow Conventional Commits: type(scope): summary.
3. Team repositories take changes only as pull requests for human review.";

/// The rulebook Neko ships with.
pub fn default_text() -> String {
    format!(
        "# How Neko's agents work\n\nNeko is the user's chief of staff: it hands work to task agents, keeps track of all of it, and answers for it. Every agent works to these rules.\n\n## Engineering\n{ENGINEERING_STANDARD}\n\n## Review\n{REVIEW_STANDARD}\n\n## This machine\n{MACHINE}\n\n## {REPOSITORIES}"
    )
}

/// The rulebook in force: the user's own if they wrote one, else the default.
pub fn text(snapshot: &Snapshot) -> String {
    snapshot
        .rulebook
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(default_text)
}

/// Saves the user's rulebook. Empty restores the default.
pub fn set(snapshot: &mut Snapshot, text: &str) -> Result<(), String> {
    let text = text.trim();
    if text.len() > MAX_BYTES {
        return Err(format!("The rulebook can be at most {} KB", MAX_BYTES / 1024));
    }
    snapshot.rulebook = (!text.is_empty() && text != default_text().trim()).then(|| text.to_owned());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_carries_every_standard_and_an_edit_replaces_it() {
        let mut snapshot = Snapshot::default();
        let text = text(&snapshot);
        for part in [ENGINEERING_STANDARD, REVIEW_STANDARD, MACHINE] {
            assert!(text.contains(part));
        }
        set(&mut snapshot, "Always write tests first.").unwrap();
        assert_eq!(super::text(&snapshot), "Always write tests first.");
        set(&mut snapshot, "  ").unwrap();
        assert!(snapshot.rulebook.is_none(), "empty restores the default");
        set(&mut snapshot, &default_text()).unwrap();
        assert!(snapshot.rulebook.is_none(), "saving the default unchanged stores nothing");
        assert!(set(&mut snapshot, &"x".repeat(MAX_BYTES + 1)).is_err());
    }
}
