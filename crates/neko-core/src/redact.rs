//! A hand-written scan that hides what looks like a secret before text is kept somewhere a person may share
//! (a goal's receipt). No regex dependency, on purpose, and it errs toward hiding: any `name=value` goes.
//! It cannot be complete: a secret with no keyword, no `KEY=` form and no known prefix passes.
//!
//! Over whitespace-separated tokens (the words around a token's punctuation are kept):
//! 1. `NAME=value` (a name of letters, digits, `_`, `.`, `-` that starts with a letter or `_`, then a value) goes.
//! 2. A token containing `token`, `secret` or `password` (any case) goes, and so does the token after it,
//!    unless that one is a keyword token itself (which goes on its own account).
//! 3. `Bearer` (any case) and the token after it go.
//! 4. A token that starts with `ghp_`, `gho_`, `github_pat_`, `sk-`, `xoxb-`, `xoxp-` or `AKIA` and is at least
//!    16 characters goes, and so does a `-----BEGIN ... PRIVATE KEY-----` block through its `-----END` line
//!    (or to the end of the text when it never ends).
//!
//! A hidden part reads `[redacted]`. `scrub(scrub(x)) == scrub(x)`.

const HIDDEN: &str = "[redacted]";
const KEYWORDS: [&str; 3] = ["token", "secret", "password"];
const SECRET_PREFIXES: [&str; 7] = ["ghp_", "gho_", "github_pat_", "sk-", "xoxb-", "xoxp-", "AKIA"];
const MIN_PREFIXED: usize = 16;

fn is_leading_punct(c: char) -> bool {
    matches!(c, '"' | '\'' | '(' | '<' | '{' | '`')
}

fn is_trailing_punct(c: char) -> bool {
    matches!(c, '"' | '\'' | ',' | '.' | ';' | ')' | '>' | '}' | '`')
}

/// A token split into the punctuation before it, the part that is judged and the punctuation after it.
fn split_punct(token: &str) -> (&str, &str, &str) {
    let core_start = token.find(|c| !is_leading_punct(c)).unwrap_or(token.len());
    let rest = &token[core_start..];
    let core_len = rest.trim_end_matches(is_trailing_punct).len();
    (&token[..core_start], &rest[..core_len], &rest[core_len..])
}

fn has_keyword(core: &str) -> bool {
    let lower = core.to_ascii_lowercase();
    KEYWORDS.iter().any(|k| lower.contains(k))
}

fn is_assignment(core: &str) -> bool {
    let Some((name, value)) = core.split_once('=') else { return false };
    let mut chars = name.chars();
    let Some(first) = chars.next() else { return false };
    (first.is_ascii_alphabetic() || first == '_') && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-')) && !value.is_empty()
}

fn is_prefixed_secret(core: &str) -> bool {
    core.chars().count() >= MIN_PREFIXED && SECRET_PREFIXES.iter().any(|p| core.starts_with(p))
}

/// Pieces of one line: runs of whitespace and runs of other characters, in order.
fn pieces(line: &str) -> Vec<(bool, &str)> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut in_space = None;
    for (at, c) in line.char_indices() {
        let space = c.is_whitespace();
        match in_space {
            Some(was) if was == space => {}
            Some(was) => {
                out.push((was, &line[start..at]));
                start = at;
                in_space = Some(space);
            }
            None => in_space = Some(space),
        }
    }
    if let Some(was) = in_space {
        out.push((was, &line[start..]));
    }
    out
}

/// Hides what looks like a secret in `text`.
pub fn scrub(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut in_key = false;
    // A rule 2 or 3 hit carries over to the next token, even on the next line.
    let mut hide_next = false;
    for (index, line) in text.split('\n').enumerate() {
        // Inside a key block neither its lines nor the breaks between them are kept.
        if index > 0 && !in_key {
            out.push('\n');
        }
        if in_key {
            if line.contains("-----END") && line.contains("PRIVATE KEY-----") {
                in_key = false;
            }
            continue;
        }
        if line.contains("-----BEGIN") && line.contains("PRIVATE KEY-----") {
            out.push_str(HIDDEN);
            in_key = !(line.contains("-----END") && line.rfind("-----END") > line.find("-----BEGIN"));
            continue;
        }
        for (space, piece) in pieces(line) {
            if space {
                out.push_str(piece);
                continue;
            }
            let (lead, core, trail) = split_punct(piece);
            let keyword = has_keyword(core);
            let hide = hide_next && !core.is_empty() && !keyword;
            hide_next = false;
            if core.is_empty() {
                out.push_str(piece);
            } else if hide || keyword || is_assignment(core) || is_prefixed_secret(core) || core.eq_ignore_ascii_case("bearer") {
                out.push_str(lead);
                out.push_str(HIDDEN);
                out.push_str(trail);
                hide_next = (keyword && !is_assignment(core)) || core.eq_ignore_ascii_case("bearer");
            } else {
                out.push_str(piece);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_value_pair_goes_whole() {
        assert_eq!(scrub("run with API_KEY=abc123 now"), "run with [redacted] now");
        assert_eq!(scrub("AWS_SECRET_ACCESS_KEY=wJalrXUtnFEMI/K7MDENG"), "[redacted]");
        assert_eq!(scrub("(db.host-1=prod),"), "([redacted]),", "punctuation around it stays");
        // Any x=y goes, on purpose; an empty value and a non-name do not.
        assert_eq!(scrub("left=3"), "[redacted]");
        assert_eq!(scrub("key= alone and =x and 9a=b"), "key= alone and =x and 9a=b");
    }

    #[test]
    fn a_bearer_credential_a_keyword_and_the_word_after_it_go() {
        assert_eq!(scrub("Authorization: Bearer abc.def"), "Authorization: [redacted] [redacted]");
        assert_eq!(scrub("bearer: x"), "bearer: x", "only the bare word counts");
        assert_eq!(scrub("password: hunter2 ok"), "[redacted] [redacted] ok");
        assert_eq!(scrub("my_token xyz tail"), "[redacted] [redacted] tail");
        assert_eq!(scrub("client-secret=abc next"), "[redacted] next", "the assignment is the whole token");
        assert_eq!(scrub("PASSWORD:\nhunter2 done"), "[redacted]\n[redacted] done", "the next token may be on the next line");
        assert_eq!(scrub("see \"api_token\", then more"), "see \"[redacted]\", [redacted] more");
        assert_eq!(scrub("token secret x y"), "[redacted] [redacted] [redacted] y", "a keyword takes the next word unless that is a keyword");
        assert_eq!(scrub("https://example.com/p?token=abc"), "[redacted]");
    }

    #[test]
    fn well_known_prefixes_and_pem_blocks_go() {
        let github = format!("ghp_{}", "a".repeat(36));
        assert_eq!(scrub(&format!("pushed with {github} ok")), "pushed with [redacted] ok");
        for prefix in ["gho_", "github_pat_", "sk-", "xoxb-", "xoxp-", "AKIA"] {
            let secret = format!("{prefix}{}", "Z".repeat(20));
            assert_eq!(scrub(&secret), "[redacted]", "{prefix}");
        }
        assert_eq!(scrub("sk-short"), "sk-short", "under 16 characters is a word, not a key");
        let pem = "before\n-----BEGIN RSA PRIVATE KEY-----\nMIIEow\nabcdef\n-----END RSA PRIVATE KEY-----\nafter";
        assert_eq!(scrub(pem), "before\n[redacted]\nafter");
        assert_eq!(scrub("x\n-----BEGIN PRIVATE KEY-----\nabc\nnever ends"), "x\n[redacted]", "an unfinished block goes to the end");
        assert_eq!(scrub("-----BEGIN PRIVATE KEY----- abc -----END PRIVATE KEY-----"), "[redacted]");
    }

    #[test]
    fn ordinary_prose_and_markdown_are_untouched() {
        let prose = "# Receipt: Ship the exporter\n\nGoal finished 2026-11-20 10:15. The check passed.\n\n## Checks\n### `cargo test` exit 0 (10:14, neko)\n```\ntest result: ok. 42 passed; 0 failed\n```\n- src/lib.rs (and 3 more)\nCost: $1.23 reported across 2 runs (1 run reported no price)\n";
        assert_eq!(scrub(prose), prose);
        assert_eq!(scrub(""), "");
        assert_eq!(scrub("  leading and  double   spaces\n\n\ttabs\n"), "  leading and  double   spaces\n\n\ttabs\n");
        assert_eq!(scrub("café naïve ✓ ok"), "café naïve ✓ ok");
    }

    #[test]
    fn scrubbing_twice_is_scrubbing_once() {
        let nasty = [
            "API_KEY=abc password: hunter2 Bearer x.y ghp_aaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----\ntoken",
            "\"password\", (secret) <token> `x=y`",
            "[redacted] [redacted]=1 tail token",
            "end with secret",
            "",
        ];
        for text in nasty {
            let once = scrub(text);
            assert_eq!(scrub(&once), once, "{text:?}");
        }
    }
}
