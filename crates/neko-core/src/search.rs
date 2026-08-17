//! Fuzzy matching (shared by every provider) and cross-provider ranking.
//! Per-provider scoring (recency boosts, etc.) lives with each provider
//! instead — see `apps::AppsProvider`, `clipboard::ClipboardProvider`,
//! `files::FileProvider` — since "each provider owns its own matching and
//! scoring" (the launch brief's own wording) is exactly the line this
//! module doesn't cross.

use neko_protocol::SearchItem;

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

/// One provider's own scored result, before cross-provider allocation.
/// Comparable across providers only because every built-in provider's score
/// is ultimately built from [`fuzzy_score`] plus a same-shaped recency
/// boost — see [`allocate`]'s doc comment for what that buys.
pub struct Candidate {
    pub score: f32,
    pub item: SearchItem,
}

/// Merges every provider's own candidate list into one response of at most
/// `limit` items — the "today's concatenate-with-a-reservation is a
/// stopgap" fix the launch brief asks for, generalized from a hard-coded
/// two-provider special case (apps get whatever's left, clipboard's top
/// match is reserved a slot) to however many providers are registered,
/// with zero provider-specific code anywhere in this function.
///
/// `providers` must be given in section render order (`AppState::new`'s own
/// registration order) — the output preserves that order, matching today's
/// "Applications" section always rendering above "Clipboard". Every
/// provider is treated symmetrically by the allocation itself (see the
/// reservation pass below); render order is purely about where each
/// section's header lands on screen, not about who gets first claim on the
/// shared budget. `SearchItem`s stay grouped by provider in the output (each provider's
/// own candidates stay contiguous, in that provider's own score order) so
/// the panel's contiguous-run section-header detection keeps working
/// unchanged.
///
/// Two passes:
///
/// 1. **Reservation.** Every provider with at least one candidate reserves
///    one slot, subtracted from `limit` up front — this is what stops a
///    long run of matches from one provider crowding a genuine match from
///    *any other* provider out of the response entirely, the same
///    guarantee `server.rs`'s own tests pin today (there, specifically for
///    clipboard), now symmetric across every registered provider rather
///    than hard-coded to one pair. This has to include the first
///    (highest-priority) provider too, not just the ones after it: an
///    earlier version of this function only reserved a floor for providers
///    after the first, reasoning that the first one, "primary," would
///    naturally win most of the greedy phase below anyway — live testing
///    (`docs/evidence/`) proved that reasoning wrong the first time a
///    provider whose matches score consistently higher (file search, on a
///    clean prefix match) was registered: it won every contested slot, not
///    just most of them, leaving the *app* section with zero rows even
///    though real apps matched the query. "No provider crowds another out
///    entirely" has to hold for every provider, including whichever one
///    happens to be first.
/// 2. **Greedy interleave.** Whatever's left of `limit` is spent one slot
///    at a time on the single highest-scoring not-yet-taken candidate
///    across *every* provider — this is the actual cross-provider ranking:
///    a provider whose matches are more relevant to this particular query
///    earns more of the shared budget than one that only barely cleared
///    its reservation floor, rather than every provider being capped at
///    exactly one row regardless of how well it matched. Ties (equal
///    score) go to the earlier provider in `providers`' order, so behavior
///    stays deterministic.
pub fn allocate(mut providers: Vec<(&str, Vec<Candidate>)>, limit: usize) -> Vec<SearchItem> {
    for (_, candidates) in providers.iter_mut() {
        candidates.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.item.title.cmp(&b.item.title)));
    }

    let n = providers.len();
    let mut taken = vec![0usize; n];
    let mut remaining = limit;

    // Every provider with a match reserves one slot for its own top
    // candidate, subtracted from the shared budget before anything else is
    // allocated — see this function's doc comment for why this must not be
    // skipped for the first provider.
    for i in 0..n {
        if remaining == 0 {
            break;
        }
        if !providers[i].1.is_empty() {
            taken[i] = 1;
            remaining -= 1;
        }
    }

    while remaining > 0 {
        let mut best: Option<(usize, f32)> = None;
        for (i, (_, candidates)) in providers.iter().enumerate() {
            if let Some(candidate) = candidates.get(taken[i])
                && best.is_none_or(|(_, best_score)| candidate.score > best_score)
            {
                best = Some((i, candidate.score));
            }
        }
        let Some((i, _)) = best else { break };
        taken[i] += 1;
        remaining -= 1;
    }

    providers
        .into_iter()
        .zip(taken)
        .flat_map(|((_, candidates), take)| candidates.into_iter().take(take).map(|c| c.item))
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

    fn item(kind: &str, id: &str, title: &str) -> SearchItem {
        SearchItem {
            id: id.to_string(),
            kind: kind.to_string(),
            title: title.to_string(),
            subtitle: None,
            icon: neko_protocol::Icon::Placeholder,
            section_label: kind.to_string(),
            action_label: "Open  ↵".to_string(),
            badge: None,
            accessory: None,
        }
    }

    fn candidates(kind: &str, scores: &[f32]) -> Vec<Candidate> {
        scores
            .iter()
            .enumerate()
            .map(|(i, &score)| Candidate {
                score,
                item: item(kind, &format!("{kind}-{i}"), &format!("{kind} {i}")),
            })
            .collect()
    }

    #[test]
    fn a_pure_primary_query_still_returns_the_full_limit() {
        let providers = vec![("app", candidates("app", &[10.0; 10])), ("clipboard", Vec::new())];
        let items = allocate(providers, 8);
        assert_eq!(items.len(), 8);
        assert!(items.iter().all(|i| i.kind == "app"));
    }

    #[test]
    fn a_secondary_match_is_never_crowded_out_by_many_primary_matches() {
        let providers = vec![("app", candidates("app", &[10.0; 10])), ("clipboard", candidates("clipboard", &[1.0]))];
        let items = allocate(providers, 8);
        assert_eq!(items.len(), 8);
        assert!(items.iter().any(|i| i.kind == "clipboard"), "clipboard's one match must survive");
        assert_eq!(items.iter().filter(|i| i.kind == "clipboard").count(), 1);
        assert_eq!(items.iter().filter(|i| i.kind == "app").count(), 7);
    }

    #[test]
    fn three_providers_each_keep_their_reservation() {
        let providers = vec![
            ("app", candidates("app", &[10.0; 10])),
            ("file", candidates("file", &[9.0; 5])),
            ("clipboard", candidates("clipboard", &[1.0])),
        ];
        let items = allocate(providers, 8);
        assert_eq!(items.len(), 8);
        assert!(items.iter().any(|i| i.kind == "file"), "file's reservation must survive");
        assert!(items.iter().any(|i| i.kind == "clipboard"), "clipboard's reservation must survive");
    }

    #[test]
    fn the_first_provider_cannot_be_crowded_out_by_a_consistently_higher_scoring_one_either() {
        // The real bug this test pins, caught live (not in a unit test
        // first): a secondary provider whose matches score consistently
        // higher than the first (registered) provider's — file search on a
        // clean prefix match easily outscores an app's scattered
        // subsequence match — must not be able to win every single
        // contested slot and reduce the first provider to zero rows. Every
        // provider's own reservation (including the first) is what
        // prevents this; the greedy phase alone does not.
        let providers = vec![
            ("app", candidates("app", &[3.0; 4])),
            ("file", candidates("file", &[10.0; 7])),
            ("clipboard", candidates("clipboard", &[8.0])),
        ];
        let items = allocate(providers, 8);
        assert_eq!(items.len(), 8);
        assert!(items.iter().any(|i| i.kind == "app"), "the first provider's reservation must survive too");
        assert!(items.iter().any(|i| i.kind == "clipboard"));
        assert!(items.iter().any(|i| i.kind == "file"));
    }

    #[test]
    fn an_unused_reservation_rolls_over_to_the_highest_remaining_score() {
        // Only 1 app match, so its own "budget" (limit minus reservations)
        // goes unused unless it rolls over to whichever provider's next
        // candidate scores highest — here that's clipboard's remaining
        // three.
        let providers = vec![("app", candidates("app", &[10.0])), ("clipboard", candidates("clipboard", &[5.0, 4.0, 3.0]))];
        let items = allocate(providers, 8);
        assert_eq!(items.iter().filter(|i| i.kind == "clipboard").count(), 3);
        assert_eq!(items.iter().filter(|i| i.kind == "app").count(), 1);
    }

    #[test]
    fn results_stay_grouped_by_provider_in_score_order() {
        let providers = vec![("app", candidates("app", &[1.0, 5.0, 3.0]))];
        let items = allocate(providers, 8);
        let scores: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(scores, vec!["app-1", "app-2", "app-0"]);
    }

    #[test]
    fn empty_providers_produce_no_results() {
        assert_eq!(allocate(Vec::new(), 8), Vec::new());
        assert_eq!(allocate(vec![("app", Vec::new())], 8), Vec::new());
    }
}
