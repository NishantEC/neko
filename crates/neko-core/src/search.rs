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

/// The deliberate ranking advantage `allocate` gives every "app" candidate
/// — see [`app_category_score`]'s doc comment for the full justification.
/// Sized against real measured `fuzzy_score` gaps (see `search.rs`'s test
/// module and `docs/evidence/ranking-before-after.md`): the worst
/// vendor-prefixed case measured (query "word" against "Microsoft Word"
/// vs. a same-scoring "word.doc" file) leaves a ~3.1-point gap even after
/// [`app_category_score`]'s per-word rescoring closes most of it; `3.0`
/// covers every case in that evidence table with a comfortable margin
/// (≥1.7 points) without being so large that a near-empty scattered app
/// match would leapfrog a strong, clearly-relevant file or clipboard
/// match — reservation already guarantees the app section a slot even
/// when this bonus alone wouldn't win it one.
const APP_CATEGORY_BONUS: f32 = 3.0;

/// Rescoring applied only to the "app" provider's own candidates, inside
/// [`allocate`], before cross-provider ranking — the audit's fix for two
/// separate, measured defects in `fuzzy_score("query", app.name)` alone
/// (`docs/evidence/ranking-before-after.md` has the real before/after
/// numbers this was tuned against):
///
/// 1. **Vendor-prefixed names lose `fuzzy_score`'s own idx==0 bonus.**
///    `fuzzy_score("chrome", "Google Chrome") = 18.24` vs.
///    `fuzzy_score("chrome", "chrome.exe.txt") = 21.22` — a plain file
///    outscores the real app, because "chrome" only starts matching at
///    character 6 of "Google Chrome", forfeiting the +3 bonus a file
///    named starting with "chrome" gets for free. This re-scores the
///    query against each individual word of the title too (`"Chrome"`
///    alone, not just `"Google Chrome"`) and takes whichever scores
///    higher — the same fix the launch brief names directly: "scoring an
///    app's name by its significant words rather than the whole string
///    so 'chrome' matches 'Google Chrome' at full strength." This is a
///    structural fix, not a fudge factor: it corrects a bonus
///    `fuzzy_score` itself already awards for exactly this shape of
///    match, just never against the *right* substring for a
///    multi-word app name.
/// 2. **Even a perfect, idx==0 app match is a near-tie with a
///    same-prefixed file, from the length penalty alone**
///    (`fuzzy_score("terminal", "Terminal") = 30.84` vs.
///    `fuzzy_score("terminal", "terminal.rs") = 30.78` — a 0.06-point
///    spread). No amount of re-scoring the title closes this, because
///    both titles already match identically well by every other term —
///    it's a real ambiguity in the shared scorer, not a bug in it
///    (`fuzzy_score` has no way to know "this title names a launchable
///    application" — that's category information only the provider
///    itself has). [`APP_CATEGORY_BONUS`] is the deliberate,
///    scoped-to-this-one-provider answer: applications are a small,
///    curated, already-installed set — a match against one is inherently
///    higher-intent than an incidental filename match for the same
///    string, which is exactly the "obviously about applications" case
///    the launch brief asks to fix. This is a per-provider category
///    weight (one of the launch brief's own suggested options), not a
///    change to `fuzzy_score` itself — `fuzzy_score` stays untouched, its
///    12 existing tests unaffected, and every other provider's score is
///    exactly what its own `fuzzy_score` call already produced.
///
/// Both pieces are needed together — measured, not assumed: the word
/// rescoring alone still leaves same-length ties (a title "Code" and a
/// file "code.rs" rescore to within the same ~0.06-point length-penalty
/// spread #2 describes), and the flat bonus alone still leaves
/// vendor-prefixed names uncomfortably close to a same-scoring file (the
/// "chrome" case above only clears the file's score by 0.16 with the
/// bonus alone, vs. 3.16 combined). Deliberately scoped to `allocate`,
/// not `fuzzy_score`: this is where the query and every provider's
/// already-computed candidates are both in scope together, without
/// requiring `AppsProvider` (`apps.rs`) to know anything about
/// cross-provider ranking policy.
///
/// **Gated on a real prefix match — caught live against the real
/// evidence table, not assumed.** An early version applied the bonus to
/// *every* app candidate unconditionally. Querying "code" against the
/// real app index surfaced not just "Code" (the real match) but also
/// "Xcode", "Cloudless Voice", and "Cloudflare WARP" ahead of good file
/// matches ("CodexAdapter.ts") — `fuzzy_score`'s subsequence matcher
/// happily finds "code" scattered inside all three, and the flat bonus
/// then promoted that coincidence into a top-8 slot. None of those three
/// titles, or any of their significant words, actually *starts with*
/// "code" — only a real prefix match reflects the "obviously about this
/// application" intent the launch brief describes; a scattered
/// subsequence hit inside an unrelated app name is exactly as incidental
/// as the same hit inside a filename, so it gets no category advantage
/// and is scored by plain `fuzzy_score` like anything else. This also
/// naturally lets a *real* second match through: "T3 Code (Alpha)" has
/// "Code" as a whole word, so it's a genuine prefix match too and keeps
/// its boost.
fn app_category_score(query: &str, title: &str, original_score: f32) -> f32 {
    let query_lower = query.to_lowercase();
    let is_prefix_match = title.to_lowercase().starts_with(&query_lower)
        || title.split_whitespace().any(|word| word.to_lowercase().starts_with(&query_lower));
    if !is_prefix_match {
        return original_score;
    }
    let best_word_score = title
        .split_whitespace()
        .filter_map(|word| fuzzy_score(query, word))
        .fold(f32::MIN, f32::max);
    original_score.max(best_word_score) + APP_CATEGORY_BONUS
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
pub fn allocate(mut providers: Vec<(&str, Vec<Candidate>)>, limit: usize, query: &str) -> Vec<SearchItem> {
    for (id, candidates) in providers.iter_mut() {
        if *id == "app" {
            for candidate in candidates.iter_mut() {
                candidate.score = app_category_score(query, &candidate.item.title, candidate.score);
            }
        }
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

    // Every pre-existing allocate() test below passes "" as the query.
    // fuzzy_score("", anything) is always Some(0.0) (see
    // empty_query_matches_everything_with_zero_base_score above), so
    // app_category_score's word rescoring is a no-op against these fixture
    // titles ("app 0", "app 1", ...) and every "app" candidate just gets a
    // uniform +APP_CATEGORY_BONUS — which changes none of these tests'
    // relative orderings or presence assertions, only their exact score
    // values, which none of them check directly. The dedicated
    // app_category_score tests below use real queries and titles instead.

    #[test]
    fn a_pure_primary_query_still_returns_the_full_limit() {
        let providers = vec![("app", candidates("app", &[10.0; 10])), ("clipboard", Vec::new())];
        let items = allocate(providers, 8, "");
        assert_eq!(items.len(), 8);
        assert!(items.iter().all(|i| i.kind == "app"));
    }

    #[test]
    fn a_secondary_match_is_never_crowded_out_by_many_primary_matches() {
        let providers = vec![("app", candidates("app", &[10.0; 10])), ("clipboard", candidates("clipboard", &[1.0]))];
        let items = allocate(providers, 8, "");
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
        let items = allocate(providers, 8, "");
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
        let items = allocate(providers, 8, "");
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
        let items = allocate(providers, 8, "");
        assert_eq!(items.iter().filter(|i| i.kind == "clipboard").count(), 3);
        assert_eq!(items.iter().filter(|i| i.kind == "app").count(), 1);
    }

    #[test]
    fn results_stay_grouped_by_provider_in_score_order() {
        let providers = vec![("app", candidates("app", &[1.0, 5.0, 3.0]))];
        let items = allocate(providers, 8, "");
        let scores: Vec<&str> = items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(scores, vec!["app-1", "app-2", "app-0"]);
    }

    #[test]
    fn empty_providers_produce_no_results() {
        assert_eq!(allocate(Vec::new(), 8, ""), Vec::new());
        assert_eq!(allocate(vec![("app", Vec::new())], 8, ""), Vec::new());
    }

    fn item_titled(kind: &str, id: &str, title: &str) -> SearchItem {
        SearchItem { title: title.to_string(), ..item(kind, id, title) }
    }

    #[test]
    fn app_category_score_closes_the_near_tie_terminal_leaves_against_a_same_prefixed_file() {
        // Real measured fuzzy_score numbers (see this module's doc comment
        // and docs/evidence/ranking-before-after.md): a 0.06-point spread
        // from the length penalty alone. app_category_score must turn that
        // into a decisive win.
        let app_score = app_category_score("terminal", "Terminal", fuzzy_score("terminal", "Terminal").unwrap());
        let file_score = fuzzy_score("terminal", "terminal.rs").unwrap();
        assert!(app_score > file_score + 1.0, "{app_score} should decisively beat {file_score}");
    }

    #[test]
    fn app_category_score_recovers_a_vendor_prefixed_name_that_fuzzy_score_alone_loses() {
        // fuzzy_score("chrome", "Google Chrome") = 18.24 loses to
        // fuzzy_score("chrome", "chrome.exe.txt") = 21.22 on the raw
        // scorer alone — the vendor prefix forfeits the idx==0 bonus.
        let app_score = app_category_score("chrome", "Google Chrome", fuzzy_score("chrome", "Google Chrome").unwrap());
        let file_score = fuzzy_score("chrome", "chrome.exe.txt").unwrap();
        assert!(app_score > file_score, "{app_score} should beat {file_score}");
    }

    #[test]
    fn allocate_ranks_the_app_above_a_same_prefixed_file_end_to_end() {
        let providers = vec![
            ("app", vec![Candidate { score: fuzzy_score("terminal", "Terminal").unwrap(), item: item_titled("app", "app-terminal", "Terminal") }]),
            (
                "file",
                vec![
                    Candidate { score: fuzzy_score("terminal", "terminal.rs").unwrap(), item: item_titled("file", "file-rs", "terminal.rs") },
                    Candidate { score: fuzzy_score("terminal", "terminal.h").unwrap(), item: item_titled("file", "file-h", "terminal.h") },
                    Candidate { score: fuzzy_score("terminal", "terminal.svg").unwrap(), item: item_titled("file", "file-svg", "terminal.svg") },
                ],
            ),
        ];
        let items = allocate(providers, 8, "terminal");
        assert_eq!(items[0].id, "app-terminal", "the app must be the top result, not a same-prefixed file");
    }

    #[test]
    fn allocate_ranks_a_vendor_prefixed_app_above_a_same_prefixed_file_end_to_end() {
        let providers = vec![
            ("app", vec![Candidate { score: fuzzy_score("chrome", "Google Chrome").unwrap(), item: item_titled("app", "app-chrome", "Google Chrome") }]),
            ("file", vec![Candidate { score: fuzzy_score("chrome", "chrome.exe.txt").unwrap(), item: item_titled("file", "file-chrome", "chrome.exe.txt") }]),
        ];
        let items = allocate(providers, 8, "chrome");
        assert_eq!(items[0].id, "app-chrome", "the vendor-prefixed app must still win");
    }

    #[test]
    fn app_category_score_does_not_boost_a_non_prefix_scattered_match() {
        // Pins the real regression caught live against the real evidence
        // table (see app_category_score's doc comment): querying "code",
        // fuzzy_score happily finds "code" scattered inside "Xcode Notary
        // Helper" too, but "xcode notary helper" and none of its words
        // start with "code" — no boost, unlike a genuine prefix match.
        let raw = fuzzy_score("code", "Xcode Notary Helper").unwrap();
        let scored = app_category_score("code", "Xcode Notary Helper", raw);
        assert_eq!(scored, raw, "a scattered non-prefix match must be left exactly as fuzzy_score scored it");
    }

    #[test]
    fn app_category_score_boosts_a_significant_word_prefix_match_even_mid_title() {
        // "T3 Code (Alpha)" is a genuine prefix match on its own second
        // word, not a coincidental scattered hit — it should still get the
        // category advantage, same as a single-word title would.
        let raw = fuzzy_score("code", "T3 Code (Alpha)").unwrap();
        let scored = app_category_score("code", "T3 Code (Alpha)", raw);
        assert!(scored > raw, "a genuine significant-word prefix match must still be boosted");
    }

    #[test]
    fn allocate_does_not_let_a_scattered_app_match_crowd_out_strong_file_matches() {
        // End-to-end version of the regression above, real scores: with
        // the query "code", "Code" (the real app, a genuine prefix match,
        // fuzzy_score 13.92) must still win the top slot, but "Xcode"
        // (fuzzy_score 8.9, a scattered non-prefix match, no boost) must
        // lose every contested slot to genuinely relevant file matches
        // ("CodexAdapter.ts" and friends, fuzzy_score ~13.7) once the
        // shared budget is scarce enough that they're competing directly
        // — not just rank behind them while still cluttering the list.
        let providers = vec![
            (
                "app",
                vec![
                    Candidate { score: fuzzy_score("code", "Code").unwrap(), item: item_titled("app", "app-code", "Code") },
                    Candidate { score: fuzzy_score("code", "Xcode").unwrap(), item: item_titled("app", "app-xcode", "Xcode") },
                ],
            ),
            (
                "file",
                vec![
                    Candidate { score: fuzzy_score("code", "CodexAdapter.ts").unwrap(), item: item_titled("file", "file-adapter", "CodexAdapter.ts") },
                    Candidate { score: fuzzy_score("code", "CodexProvider.ts").unwrap(), item: item_titled("file", "file-provider", "CodexProvider.ts") },
                    Candidate { score: fuzzy_score("code", "CodexDriver.ts").unwrap(), item: item_titled("file", "file-driver", "CodexDriver.ts") },
                ],
            ),
        ];
        let items = allocate(providers, 4, "code");
        assert_eq!(items[0].id, "app-code", "the real app must still be the top result");
        assert!(!items.iter().any(|i| i.id == "app-xcode"), "a scattered non-prefix match must lose every contested slot to real file matches");
        assert_eq!(items.iter().filter(|i| i.kind == "file").count(), 3, "all three genuinely relevant files must fit instead");
    }
}

