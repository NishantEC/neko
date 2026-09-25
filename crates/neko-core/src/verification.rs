//! Fail-closed independent review gate. Runner receipts, not prose, back checks.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Verdict {
    pub passed: bool,
    pub findings: Vec<String>,
    pub files: Vec<String>,
    pub tests: Vec<String>,
    pub summary: String,
}

/// Decode only the single, literal script argument of known shell wrappers.
/// No shell execution, interpolation, substring matching, or inner-script
/// rewriting. Codex's display may concatenate single/double quoted segments.
fn normalized_command(command: &str) -> String {
    for shell in [
        "/bin/bash",
        "/bin/zsh",
        "/bin/sh",
        "/usr/bin/bash",
        "/usr/bin/zsh",
        "/usr/bin/sh",
    ] {
        for flag in ["-lc", "-c"] {
            let prefix = format!("{shell} {flag} ");
            if let Some(script) = command.strip_prefix(&prefix) {
                if let Some(decoded) = literal_script(script) {
                    return decoded;
                }
                return command.into();
            }
        }
    }
    command.into()
}

fn literal_script(script: &str) -> Option<String> {
    let mut chars = script.chars();
    let mut quote = None;
    let mut quoted = false;
    let mut out = String::new();
    while let Some(c) = chars.next() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    out.push(c);
                }
            }
            Some('"') => match c {
                '"' => quote = None,
                '$' | '`' => return None,
                '\\' => {
                    let next = chars.next()?;
                    if next == '\n' || next == '\r' {
                        return None;
                    }
                    if !matches!(next, '\\' | '"' | '$' | '`') {
                        out.push('\\');
                    }
                    out.push(next);
                }
                _ => out.push(c),
            },
            None => match c {
                '\'' | '"' => {
                    quote = Some(c);
                    quoted = true;
                }
                '\\' => {
                    let next = chars.next()?;
                    if next == '\n' || next == '\r' {
                        return None;
                    }
                    out.push(next);
                }
                c if c.is_whitespace()
                    || matches!(
                        c,
                        '$' | '`'
                            | ';'
                            | '|'
                            | '&'
                            | '('
                            | ')'
                            | '<'
                            | '>'
                            | '#'
                            | '*'
                            | '?'
                            | '['
                            | ']'
                            | '{'
                            | '}'
                            | '~'
                    ) =>
                {
                    return None;
                }
                _ => out.push(c),
            },
            _ => unreachable!(),
        }
    }
    (quoted && quote.is_none()).then_some(out)
}

pub fn accept(
    answer: &str,
    changed: &[String],
    allowed: &[String],
    receipts: &[String],
    required_checks: &[String],
) -> Result<Verdict, String> {
    let verdict: Verdict = serde_json::from_str(answer.trim())
        .map_err(|e| format!("Verifier returned a malformed verdict: {e}"))?;
    if !verdict.passed || !verdict.findings.is_empty() {
        return Err(format!(
            "Independent verification failed: {} {}",
            verdict.summary,
            verdict.findings.join("; ")
        ));
    }
    if verdict.summary.trim().is_empty() || verdict.tests.is_empty() || changed.is_empty() {
        return Err("Verification missing summary, executed checks, or changed files".into());
    }
    if changed
        .iter()
        .any(|f| !verdict.files.contains(f) || (!allowed.is_empty() && !allowed.contains(f)))
    {
        return Err("Actual changed files exceed verified or approved scope".into());
    }
    for required in required_checks {
        if normalized_command(required).trim().is_empty()
            || !verdict
                .tests
                .iter()
                .any(|test| normalized_command(test) == normalized_command(required))
        {
            return Err(format!(
                "Independent verification omitted approved check: {required}"
            ));
        }
    }
    for test in &verdict.tests {
        if normalized_command(test).trim().is_empty() || test.len() > 8192 {
            return Err("Verifier check names must be bounded commands".into());
        }
        let proven = receipts
            .iter()
            .filter_map(|r| r.strip_prefix("VERIFICATION_COMMAND "))
            .filter_map(|r| serde_json::from_str::<serde_json::Value>(r).ok())
            .filter(|r| {
                r["command"]
                    .as_str()
                    .is_some_and(|command| normalized_command(command) == normalized_command(test))
            })
            .last()
            .is_some_and(|r| r["exit_code"].as_i64() == Some(0) && r["output"].is_string());
        if !proven {
            return Err(format!(
                "No successful independent execution evidence for: {test}"
            ));
        }
    }
    Ok(verdict)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn answer() -> String {
        serde_json::json!({"passed":true,"findings":[],"files":["src/a.rs"],"tests":["cargo test"],"summary":"Verified boundary"}).to_string()
    }
    #[test]
    fn independent_review_requires_successful_actual_command_receipts() {
        let files = vec!["src/a.rs".into()];
        assert!(accept(&answer(), &files, &files, &[], &[]).is_err());
        assert!(accept(&answer(), &files, &files, &[r#"VERIFICATION_COMMAND {"command":"cargo test","exit_code":1,"output":"FAILED"}"#.into()], &[]).is_err());
        assert!(accept(&answer(), &files, &files, &[r#"VERIFICATION_COMMAND {"command":"cargo test","exit_code":0,"output":"1 passed"}"#.into()], &[]).is_ok());
    }
    #[test]
    fn rejects_findings_malformed_missing_files_and_scope_escape() {
        let files = vec!["src/a.rs".into()];
        let receipt = vec![
            r#"VERIFICATION_COMMAND {"command":"cargo test","exit_code":0,"output":"1 passed"}"#
                .into(),
        ];
        for text in [
            "looks fine".into(),
            answer().replace("true", "false"),
            answer().replace("\"findings\":[]", "\"findings\":[\"broken\"]"),
            answer().replace("src/a.rs", "src/b.rs"),
        ] {
            assert!(accept(&text, &files, &files, &receipt, &[]).is_err());
        }
        assert!(accept(&answer(), &files, &["other.rs".into()], &receipt, &[]).is_err());
    }

    #[test]
    fn unrelated_success_cannot_replace_an_approved_check() {
        let files = vec!["src/a.rs".into()];
        let receipts = vec![
            r#"VERIFICATION_COMMAND {"command":"cargo test","exit_code":0,"output":"1 passed"}"#
                .into(),
        ];
        let required = vec!["cargo test integration_boundary".into()];
        let error = accept(&answer(), &files, &files, &receipts, &required).unwrap_err();
        assert!(error.contains("cargo test integration_boundary"));
        assert!(accept(&answer(), &files, &files, &receipts, &["cargo test".into()]).is_ok());
    }

    fn receipt(command: &str, code: i64, output: &str) -> String {
        format!(
            "VERIFICATION_COMMAND {}",
            serde_json::json!({"command":command,"exit_code":code,"output":output})
        )
    }

    #[test]
    fn recognized_shell_wrapper_preserves_the_exact_approved_command() {
        let files = vec!["src/a.rs".into()];
        for command in [
            "/bin/bash -lc 'cargo test'",
            "/bin/zsh -lc 'cargo test'",
            "/bin/sh -c \"cargo test\"",
        ] {
            assert!(
                accept(
                    &answer(),
                    &files,
                    &files,
                    &[receipt(command, 0, "passed")],
                    &["cargo test".into()]
                )
                .is_ok(),
                "{command}"
            );
            let wrapped_answer = serde_json::json!({"passed":true,"findings":[],"files":["src/a.rs"],"tests":[command],"summary":"Checked"}).to_string();
            assert!(
                accept(
                    &wrapped_answer,
                    &files,
                    &files,
                    &[receipt(command, 0, "passed")],
                    &["cargo test".into()]
                )
                .is_ok()
            );
        }
    }

    #[test]
    fn wrappers_do_not_allow_substrings_compounds_arguments_or_expansion() {
        let files = vec!["src/a.rs".into()];
        for command in [
            "/tmp/bash -lc 'cargo test'",
            "/bin/bash -lc 'cargo test || true'",
            "/bin/bash -lc 'echo cargo test'",
            "/bin/bash -lc 'cargo test-extra'",
            "/bin/bash -lc 'cargo test' trailing",
            "/bin/bash -lc 'cargo test'; true",
            "/bin/bash -lc \"cargo $TEST\"",
            "/bin/bash -lc 'cargo test'\ntrue",
        ] {
            assert!(
                accept(
                    &answer(),
                    &files,
                    &files,
                    &[receipt(command, 0, "passed")],
                    &["cargo test".into()]
                )
                .is_err(),
                "{command}"
            );
        }
    }

    #[test]
    fn silent_success_is_evidence_but_latest_failure_and_missing_exit_are_not() {
        let files = vec!["src/a.rs".into()];
        let text = answer().replace("cargo test", "test -f src/a.rs");
        let command = "/bin/bash -lc 'test -f src/a.rs'";
        assert!(
            accept(
                &text,
                &files,
                &files,
                &[receipt(command, 0, "")],
                &["test -f src/a.rs".into()]
            )
            .is_ok()
        );
        assert!(
            accept(
                &text,
                &files,
                &files,
                &[receipt(command, 0, ""), receipt(command, 1, "")],
                &["test -f src/a.rs".into()]
            )
            .is_err()
        );
        let missing = format!(
            "VERIFICATION_COMMAND {}",
            serde_json::json!({"command":command,"output":""})
        );
        assert!(
            accept(
                &text,
                &files,
                &files,
                &[missing],
                &["test -f src/a.rs".into()]
            )
            .is_err()
        );
    }

    #[test]
    fn real_codex_mixed_quote_silent_receipt_matches_the_full_script() {
        // Exact shell display from the disposable live smoke reviewer receipt.
        let wrapped = r#"/bin/zsh -lc 'test "$(wc -c < neko-smoke.txt)" -eq 21 && printf '"'Isolated task output\\n' | cmp - neko-smoke.txt""#;
        let script = r#"test "$(wc -c < neko-smoke.txt)" -eq 21 && printf 'Isolated task output\n' | cmp - neko-smoke.txt"#;
        assert_eq!(normalized_command(wrapped), script);
        let answer = serde_json::json!({"passed":true,"findings":[],"files":["neko-smoke.txt"],"tests":[script],"summary":"Exact bytes checked"}).to_string();
        let files = vec!["neko-smoke.txt".into()];
        assert!(
            accept(
                &answer,
                &files,
                &files,
                &[receipt(wrapped, 0, "")],
                &[script.into()]
            )
            .is_ok()
        );
        assert!(
            accept(
                &answer,
                &files,
                &files,
                &[receipt(&format!("{wrapped}; true"), 0, "")],
                &[script.into()]
            )
            .is_err()
        );
    }

    #[test]
    fn live_multiline_script_keeps_its_final_newline_exactly() {
        let script = "GIT_OPTIONAL_LOCKS=0 git -c core.fsmonitor=false -c core.untrackedCache=false status --porcelain=v1 --untracked-files=all\nGIT_OPTIONAL_LOCKS=0 git -c core.fsmonitor=false -c core.untrackedCache=false diff --no-ext-diff --no-textconv 96c8d8f5dc574fc026f6285e27af253ffc57f68d --\n";
        let wrapped = format!("/bin/zsh -lc '{script}'");
        let answer = serde_json::json!({"passed":true,"findings":[],"files":["neko-smoke.txt"],"tests":[script],"summary":"Scope checked"}).to_string();
        let files = vec!["neko-smoke.txt".into()];
        assert!(
            accept(
                &answer,
                &files,
                &files,
                &[receipt(&wrapped, 0, "?? neko-smoke.txt")],
                &[script.into()]
            )
            .is_ok()
        );
        for exact in [
            "printf value\\\n",
            "printf value\\ ",
            " printf value\n",
            "printf value\n\n",
        ] {
            assert_eq!(
                normalized_command(exact),
                exact,
                "Bare scripts must not lose semantic trailing bytes"
            );
            assert_eq!(
                normalized_command(&format!("/bin/zsh -lc '{exact}'")),
                exact
            );
        }
        assert_ne!(
            normalized_command("printf value\\\n"),
            normalized_command("printf value\\")
        );
    }
}
