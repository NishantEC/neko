//! Fuzzy matching and ranking. A small, dependency-free subsequence scorer
//! (fzf-style) plus a recency boost from `launches`.

use std::collections::HashMap;

use neko_protocol::{ResultKind, SearchItem};

use crate::apps::AppEntry;

/// Score a query against a title as a case-insensitive subsequence match.
/// Returns `None` if `query`'s characters don't all appear in `title`, in
/// order. Higher is better.
///
/// Bonuses: matching at the start of the title or right after a word
/// boundary (space, `-`, `_`), and matching runs of consecutive characters
/// rather than scattered ones — the same shape fzf and most launcher
/// fuzzy-finders use, because it's what makes "chr" preferentially match
/// "Chrome" over "Character Map".
pub fn fuzzy_score(query: &str, title: &str) -> Option<f32> {
    if query.is_empty() {
        return Some(0.0);
    }
    let title_chars: Vec<char> = title.chars().collect();
    let title_lower: Vec<char> = title.to_lowercase().chars().collect();
    let query_lower = query.to_lowercase();
    let mut query_chars = query_lower.chars().peekable();

    let mut score = 0.0f32;
    let mut consecutive = 0u32;
    let mut matched_any = false;
    let mut ti = 0usize;

    while let Some(&qc) = query_chars.peek() {
        let found_at = title_lower[ti..].iter().position(|&c| c == qc)?;
        let idx = ti + found_at;

        let at_boundary = idx == 0
            || title_chars
                .get(idx - 1)
                .is_some_and(|c| c.is_whitespace() || *c == '-' || *c == '_');
        let is_gap = found_at > 0;

        if is_gap {
            consecutive = 0;
        }
        consecutive += 1;

        score += 1.0;
        score += consecutive as f32 * 0.5;
        if at_boundary {
            score += 2.0;
        }
        if idx == 0 {
            score += 3.0;
        }

        matched_any = true;
        ti = idx + 1;
        query_chars.next();
    }

    // Shorter titles rank slightly higher for an equally good match ("Mail"
    // over "Mailchimp Companion" for query "mail").
    let length_penalty = title_chars.len() as f32 * 0.02;
    matched_any.then_some(score - length_penalty)
}

/// A launched-recently boost, tapering over roughly two weeks, plus a small
/// per-launch frequency term. Recency dominates frequency: a thing you used
/// once yesterday should usually beat a thing you used fifty times last
/// year.
fn recency_boost(last_launched_at_unix_ms: i64, launch_count: i64, now_unix_ms: i64) -> f32 {
    let age_ms = (now_unix_ms - last_launched_at_unix_ms).max(0) as f32;
    let age_days = age_ms / (1000.0 * 60.0 * 60.0 * 24.0);
    let half_life_days = 5.0;
    let recency = 8.0 * 0.5f32.powf(age_days / half_life_days);
    let frequency = (launch_count as f32).ln_1p() * 0.5;
    recency + frequency
}

pub fn rank_apps(
    query: &str,
    apps: &[AppEntry],
    recency: &HashMap<String, (i64, i64)>,
    now_unix_ms: i64,
    limit: usize,
) -> Vec<SearchItem> {
    let mut scored: Vec<(f32, &AppEntry)> = apps
        .iter()
        .filter_map(|app| {
            let mut score = fuzzy_score(query, &app.name)?;
            if let Some(&(last, count)) = recency.get(&app.id) {
                score += recency_boost(last, count, now_unix_ms);
            }
            Some((score, app))
        })
        .collect();

    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.name.cmp(&b.1.name)));

    scored
        .into_iter()
        .take(limit)
        .map(|(_, app)| SearchItem {
            id: app.id.clone(),
            kind: ResultKind::App,
            title: app.name.clone(),
            subtitle: None,
            icon_path: crate::icons::cached_icon_path(&app.id)
                .filter(|p| p.exists())
                .map(|p| p.to_string_lossy().into_owned()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subsequence_that_does_not_appear_scores_none() {
        assert_eq!(fuzzy_score("xyz", "Chrome"), None);
    }

    #[test]
    fn prefix_match_beats_scattered_match() {
        let prefix = fuzzy_score("chr", "Chrome").unwrap();
        let scattered = fuzzy_score("cr", "Character Map").unwrap();
        assert!(prefix > scattered, "{prefix} should beat {scattered}");
    }

    #[test]
    fn shorter_title_wins_ties_for_an_equally_good_match() {
        let short = fuzzy_score("mail", "Mail").unwrap();
        let long = fuzzy_score("mail", "Mail Merge Helper").unwrap();
        assert!(short > long);
    }

    #[test]
    fn empty_query_matches_everything_with_zero_base_score() {
        assert_eq!(fuzzy_score("", "Anything"), Some(0.0));
    }

    #[test]
    fn ranking_prefers_recently_launched_apps_on_a_tied_fuzzy_score() {
        let apps = vec![
            AppEntry {
                id: "a".into(),
                name: "Finder".into(),
                path: "/Applications/Finder.app".into(),
            },
            AppEntry {
                id: "b".into(),
                name: "Finder".into(),
                path: "/Applications/OtherFinder.app".into(),
            },
        ];
        let mut recency = HashMap::new();
        recency.insert("b".to_string(), (1_000_000, 3));
        let ranked = rank_apps("find", &apps, &recency, 1_000_000 + 1000, 10);
        assert_eq!(ranked[0].id, "b");
    }
}
