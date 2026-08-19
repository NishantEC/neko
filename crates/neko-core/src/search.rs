//! Fuzzy matching (shared by every provider) and cross-provider ranking.
//! Per-provider scoring (recency boosts, etc.) lives with each provider
//! instead — see `apps::AppsProvider`, `clipboard::ClipboardProvider`,
//! `files::FileProvider` — since "each provider owns its own matching and
//! scoring" (the launch brief's own wording) is exactly the line this
//! module doesn't cross, with one narrow, documented exception:
//! `allocate`'s section-ordering pass reads (never writes)
//! `clipboard::CLIPBOARD_RECENCY_BOOST_CEILING` to keep a clipboard
//! candidate's freshness bonus from being mistaken for query relevance when
//! choosing which *section* leads — see that pass's own doc comment.

use crate::clipboard;
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
    category_score(query, title, original_score, APP_CATEGORY_BONUS)
}

/// The same prefix-gated rescore [`app_category_score`] uses, generalized
/// over the flat bonus applied so [`settings_category_score`] below can
/// share it exactly rather than re-implementing the same word-rescore/gate
/// logic with a different constant — the gating condition (a real prefix
/// match, whole title or a significant word) and the per-word rescore are
/// category-agnostic; only the bonus size is provider-specific.
fn category_score(query: &str, title: &str, original_score: f32, bonus: f32) -> f32 {
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
    original_score.max(best_word_score) + bonus
}

/// The System Settings pane equivalent of [`APP_CATEGORY_BONUS`] — see
/// [`settings_category_score`]'s doc comment for why `settings.rs`'s
/// original "no bonus at all" choice overcorrected, and
/// `docs/evidence/settings-and-clipboard-ranking.md` for the real
/// before/after numbers this was tuned against. Deliberately smaller than
/// `APP_CATEGORY_BONUS` (3.0), not just qualitatively but by a measured
/// margin: for a single-word title that both an app and a pane match
/// equally well (`"bluetooth"` against both "Bluetooth" the pane and
/// "Bluetooth File Exchange" the app), `category_score`'s own per-word
/// rescore gives both candidates the *identical* base score before either
/// bonus is added — so the gap between the two final scores is exactly
/// `APP_CATEGORY_BONUS - SETTINGS_CATEGORY_BONUS`. `1.5` keeps that gap a
/// full 1.5 points, comfortably decisive (matching the ≥1.0-point margin
/// `app_category_score`'s own "terminal" test already treats as
/// "decisive"), while still clearing an unrelated file's raw `fuzzy_score`
/// by a wide margin for a real pane query ("sound" vs. "Sound" beats
/// "background_sound.log" by 4.8 points even with only this smaller bonus).
const SETTINGS_CATEGORY_BONUS: f32 = 1.5;

/// Rescoring applied only to the "settings" provider's own candidates,
/// inside [`allocate`] — the same category-weight idea
/// [`app_category_score`] already established for "app", now extended to
/// the fourth provider. `settings.rs::search`'s own doc comment originally
/// argued no bonus was needed there because plain `fuzzy_score` already
/// satisfied "must not crowd out genuine application matches" — true, but
/// incomplete: unboosted, a short exact pane title ("Sound") also loses to
/// *everything else*, not just to apps. A file that merely contains the
/// query as a substring, or a clipboard entry whose recency boost stacks on
/// top of an incidental hit, both routinely outscored an exact pane match
/// (`docs/evidence/settings-and-clipboard-ranking.md` has the real
/// `fuzzy_score` numbers: "sound" → "Sound" scores only 17.4, well below
/// what an unrelated file or a recent clipboard paste containing "sound"
/// can reach). [`SETTINGS_CATEGORY_BONUS`] restores "an exact/near-exact
/// pane match beats an incidental file/clipboard hit" without touching the
/// one thing `settings.rs` got right the first time — this bonus, like
/// `app_category_score`'s, is gated on a real prefix match, so a scattered
/// non-prefix hit gets no advantage, and it's sized smaller than
/// `APP_CATEGORY_BONUS` so a genuine application match for the same query
/// ("Bluetooth File Exchange" for "bluetooth") still wins outright.
fn settings_category_score(query: &str, title: &str, original_score: f32) -> f32 {
    category_score(query, title, original_score, SETTINGS_CATEGORY_BONUS)
}

/// Clipboard's own guardrail, on top of the ordinary reservation-then-greedy
/// allocation every provider gets: at most half of `limit` (rounded up),
/// applied only once at least one *other* provider also has a candidate for
/// this query — the same "when others have candidates" condition the launch
/// brief itself specifies. A free-form pasted paragraph can score highly for
/// containing the query once, almost anywhere in a lot of unrelated text
/// (`clipboard::clipboard_length_normalization` narrows that gap but can't
/// close it to zero for every possible entry, and a very recent copy's
/// recency boost stacks on top regardless of length) — without a hard cap,
/// once clipboard is the only provider with remaining supply, it wins every
/// single contested slot: a real captain-reported query ("wallpaper")
/// returned nine private clipboard rows out of ten for one exact pane match,
/// which is both a relevance problem and — the captain's own clipboard
/// history holds real invoices and client correspondence — a privacy one.
/// Scoped to "clipboard" specifically (mirrors [`app_category_score`]'s
/// "app"-only gating) rather than a blanket cap on every provider: a
/// provider whose *individual* candidates are all genuinely relevant (many
/// real file matches, say) winning most of the shared budget is the
/// intended behavior of the greedy phase, not a bug — clipboard's problem is
/// that a high score there doesn't reliably mean high relevance the way it
/// does for a curated, title-shaped candidate pool. When the cap leaves
/// slots unclaimed because no other provider has more supply either, those
/// slots simply go unused (a shorter result list) rather than being forced
/// back onto clipboard — same "an earlier section using less than its share
/// doesn't automatically go to a section that doesn't need it" principle
/// [`allocate`]'s own doc comment already establishes for reservations.
fn clipboard_max_slots(limit: usize) -> usize {
    limit.div_ceil(2).max(1)
}

/// Merges every provider's own candidate list into one response of at most
/// `limit` items — the "today's concatenate-with-a-reservation is a
/// stopgap" fix the launch brief asks for, generalized from a hard-coded
/// two-provider special case (apps get whatever's left, clipboard's top
/// match is reserved a slot) to however many providers are registered,
/// with zero provider-specific code anywhere in this function.
///
/// `providers` is given in registration order (`AppState::new`'s own `Vec`
/// order) — that order decides *only* how ties are broken (see the section-
/// ordering pass below), never where a section's header actually lands on
/// screen. **Section order is decided by content strength, not by
/// registration order** — a structural fix, not a scoring one: a previous
/// task (`fe4e4e9`, `neko-ranking-2`) established that score alone can only
/// move rows *within* a section, never move a section itself, since the
/// output used to be `flat_map`'d over `providers` in its given order,
/// always. That meant a query like `"clipboard history"` — which the
/// `command` provider matches almost perfectly (`docs/evidence/
/// settings-and-clipboard-ranking.md`'s "Commands and modes" companion
/// evidence) — still rendered the **Commands** section dead last, beneath
/// **Clipboard**'s own always-reserved rows, purely because `command` is
/// registered after `clipboard` in `AppState::with_test_providers`. Every
/// provider is still treated symmetrically by the *allocation* itself (see
/// the reservation pass below, unchanged); only the final section order is
/// new. `SearchItem`s stay grouped by provider in the output (each
/// provider's own candidates stay contiguous, in that provider's own score
/// order) so the panel's contiguous-run section-header detection
/// (`panel::group_into_sections`) keeps working unchanged — this changes
/// *which* contiguous run comes first, not whether runs exist.
///
/// Three passes:
///
/// 1. **Reservation.** Every provider with at least one candidate reserves
///    one slot, subtracted from `limit` up front — this is what stops a
///    long run of matches from one provider crowding a genuine match from
///    *any other* provider out of the response entirely, the same
///    guarantee `server.rs`'s own tests pin today (there, specifically for
///    clipboard), now symmetric across every registered provider rather
///    than hard-coded to one pair. This has to include the first
///    (registration-order) provider too, not just the ones after it: an
///    earlier version of this function only reserved a floor for providers
///    after the first, reasoning that the first one, "primary," would
///    naturally win most of the greedy phase below anyway — live testing
///    (`docs/evidence/`) proved that reasoning wrong the first time a
///    provider whose matches score consistently higher (file search, on a
///    clean prefix match) was registered: it won every contested slot, not
///    just most of them, leaving the *app* section with zero rows even
///    though real apps matched the query. "No provider crowds another out
///    entirely" has to hold for every provider, independent of section
///    order.
/// 2. **Greedy interleave.** Whatever's left of `limit` is spent one slot
///    at a time on the single highest-scoring not-yet-taken candidate
///    across *every* provider — this is the actual cross-provider row-level
///    ranking: a provider whose matches are more relevant to this
///    particular query earns more of the shared budget than one that only
///    barely cleared its reservation floor, rather than every provider
///    being capped at exactly one row regardless of how well it matched.
///    This pass never changes section order, only how many rows a section
///    gets.
/// 3. **Section ordering.** Once every provider knows how many rows it's
///    keeping, the sections themselves are sorted by the score of their own
///    top (best) surviving candidate — descending, so a section holding an
///    exact-name match (an alias-matched command, a prefix-matched app or
///    settings pane, all of which already carry a category bonus baked into
///    that same score) rises above a section whose best match is an
///    incidental substring hit. This is deliberately the *same* score
///    already used for the row-level greedy interleave above, not a fresh
///    cross-provider normalization: those scores are already established as
///    "roughly comparable enough to rank against each other one row at a
///    time" by passes 1–2, and by every category-bonus doc comment in this
///    file, so reusing them for one more comparison (section vs. section
///    instead of row vs. row) needed no new machinery. **Ties are broken by
///    original registration order** — `sort_by` is a stable sort, and
///    comparing by score alone (no secondary key) leaves equal-scoring
///    providers exactly where they already were in `providers`' input
///    order, which satisfies "ties break deterministically" without a
///    second comparison key to keep in sync. A provider with zero surviving
///    candidates sorts last (its top-candidate score is defined as
///    `f32::NEG_INFINITY`) but contributes zero rows either way, so its
///    exact position is unobservable.
pub fn allocate(mut providers: Vec<(&str, Vec<Candidate>)>, limit: usize, query: &str) -> Vec<SearchItem> {
    for (id, candidates) in providers.iter_mut() {
        if *id == "app" {
            for candidate in candidates.iter_mut() {
                candidate.score = app_category_score(query, &candidate.item.title, candidate.score);
            }
        } else if *id == "settings" {
            for candidate in candidates.iter_mut() {
                candidate.score = settings_category_score(query, &candidate.item.title, candidate.score);
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

    // See [`clipboard_max_slots`]'s doc comment for why clipboard alone
    // gets a hard ceiling on top of the ordinary greedy interleave, and why
    // it's scoped to "clipboard" rather than applied to every provider.
    let clipboard_cap = providers
        .iter()
        .position(|(id, _)| *id == "clipboard")
        .filter(|_| providers.iter().any(|(id, candidates)| *id != "clipboard" && !candidates.is_empty()))
        .map(|clipboard_index| (clipboard_index, clipboard_max_slots(limit)));

    while remaining > 0 {
        let mut best: Option<(usize, f32)> = None;
        for (i, (_, candidates)) in providers.iter().enumerate() {
            if clipboard_cap.is_some_and(|(clipboard_index, cap)| i == clipboard_index && taken[i] >= cap) {
                continue;
            }
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

    let mut sections: Vec<(&str, Vec<Candidate>, usize)> = providers
        .into_iter()
        .zip(taken)
        .map(|((id, candidates), take)| (id, candidates, take))
        .collect();

    // See this function's own doc comment, pass 3: order the *sections* by
    // their own top candidate's score, stable-sorted so equal scores keep
    // `sections`' current (registration) order as the deterministic tiebreak.
    let mut order: Vec<usize> = (0..sections.len()).collect();
    order.sort_by(|&a, &b| {
        let section_rank_score = |i: usize| {
            let (id, candidates, _) = &sections[i];
            let Some(top) = candidates.first().map(|c| c.score) else { return f32::NEG_INFINITY };
            // Clipboard's own top score already carries up to
            // `CLIPBOARD_RECENCY_BOOST_CEILING` of "just copied" freshness
            // (`ClipboardProvider::search`) on top of however well it
            // actually matches the query — legitimate for ranking *rows*
            // (a recent copy surfacing first within Clipboard is the whole
            // point), but not for deciding which *section* leads: a query
            // like "clipboard history" can otherwise put an ordinary, merely
            // fresh clipboard entry ahead of the `Clipboard History`
            // command's own decisive alias match purely because it was
            // copied a moment ago, not because it's more relevant — caught
            // live during this task with a synthetic same-instant repro, not
            // assumed. Subtracting the ceiling here (never touching
            // `candidate.score` itself, so row-level ranking is untouched)
            // means clipboard only leads a section it would have led on
            // match strength alone.
            if *id == "clipboard" { top - clipboard::CLIPBOARD_RECENCY_BOOST_CEILING } else { top }
        };
        section_rank_score(b).total_cmp(&section_rank_score(a))
    });

    order
        .into_iter()
        .flat_map(|i| {
            let (_, candidates, take) = std::mem::take(&mut sections[i]);
            candidates.into_iter().take(take).map(|c| c.item)
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
            enters_mode: None,
            group_label: None,
            actions: Vec::new(),
            source: None,
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

    // --- Defect 1: an exact settings-pane match must outscore an
    // unrelated file/clipboard hit that merely contains the query, without
    // beating a genuine application match for the same query. ---

    #[test]
    fn settings_category_score_beats_an_unrelated_file_that_merely_contains_the_query() {
        // Real measured fuzzy_score numbers (see SETTINGS_CATEGORY_BONUS's
        // doc comment and docs/evidence/settings-and-clipboard-ranking.md):
        // unboosted, the exact pane title "Sound" (17.4) barely clears a
        // file that merely contains "sound" at a word boundary
        // ("background_sound.log", 14.1) — not the decisive win a real,
        // curated pane match deserves.
        let settings_score = settings_category_score("sound", "Sound", fuzzy_score("sound", "Sound").unwrap());
        let file_score = fuzzy_score("sound", "background_sound.log").unwrap();
        assert!(settings_score > file_score + 1.0, "{settings_score} should decisively beat {file_score}");
    }

    #[test]
    fn settings_category_score_does_not_let_a_pane_beat_a_genuine_app_match() {
        // The launch brief's own explicit requirement: "bluetooth" must
        // keep "Bluetooth File Exchange" (the real app) above the
        // "Bluetooth" pane. Both get the identical per-word rescore for
        // this query, so this pins the bonus gap directly.
        let settings_score = settings_category_score("bluetooth", "Bluetooth", fuzzy_score("bluetooth", "Bluetooth").unwrap());
        let app_score = app_category_score("bluetooth", "Bluetooth File Exchange", fuzzy_score("bluetooth", "Bluetooth File Exchange").unwrap());
        assert!(app_score > settings_score, "the real app ({app_score}) must still beat the pane ({settings_score})");
    }

    #[test]
    fn settings_category_score_does_not_boost_a_non_prefix_scattered_match() {
        // Mirrors app_category_score's identical guard: a scattered,
        // non-prefix hit inside a pane title gets no category advantage.
        let raw = fuzzy_score("play", "Displays").unwrap();
        let scored = settings_category_score("play", "Displays", raw);
        assert_eq!(scored, raw, "a scattered non-prefix match must be left exactly as fuzzy_score scored it");
    }

    #[test]
    fn allocate_lets_the_settings_bonus_win_a_contested_slot_end_to_end() {
        // Presence alone (the reservation) doesn't exercise the bonus —
        // every provider's first candidate is guaranteed a slot regardless
        // of score. This pins the case that actually needs the bonus: a
        // *second* real pane match competing with a file for one shared
        // slot beyond both reservations, illustrative round numbers chosen
        // so the outcome flips with the bonus applied (12.5 vs. 12.0, where
        // unboosted it would have been 11.0 vs. 12.0) — the real
        // `fuzzy_score` numbers this was tuned against are in the two
        // dedicated scoring tests above.
        let providers = vec![
            (
                "file",
                vec![
                    Candidate { score: 15.0, item: item_titled("file", "file-a", "soundboard.app") },
                    Candidate { score: 12.0, item: item_titled("file", "file-b", "sound_test.wav") },
                ],
            ),
            (
                "settings",
                vec![
                    Candidate { score: 12.0, item: item_titled("settings", "com.apple.preference.sound", "Sound") },
                    Candidate { score: 11.0, item: item_titled("settings", "com.apple.preference.sound-effects", "Sound Effects") },
                ],
            ),
        ];
        let items = allocate(providers, 3, "sound");
        assert_eq!(items.iter().filter(|i| i.kind == "settings").count(), 2, "the bonus must let the pane's second match win the contested slot");
        assert_eq!(items.iter().filter(|i| i.kind == "file").count(), 1);
    }

    // --- Defect 2: clipboard alone must not be able to claim (nearly) the
    // whole shared budget once another provider also has a candidate. ---

    #[test]
    fn clipboard_max_slots_is_half_the_limit_rounded_up() {
        assert_eq!(clipboard_max_slots(10), 5);
        assert_eq!(clipboard_max_slots(8), 4);
        assert_eq!(clipboard_max_slots(1), 1);
    }

    #[test]
    fn a_single_provider_cannot_take_the_whole_list_while_others_have_candidates() {
        // The real captain-reported shape: nine clipboard entries (each
        // individually outscoring the one real settings match, the same way
        // a long pasted paragraph containing the query can) must not be
        // allowed to fill nearly the entire ten-row budget when a genuine
        // settings match exists too.
        let providers = vec![
            ("clipboard", candidates("clipboard", &[20.0; 9])),
            ("settings", candidates("settings", &[5.0])),
        ];
        let items = allocate(providers, 10, "");
        let clipboard_count = items.iter().filter(|i| i.kind == "clipboard").count();
        assert!(clipboard_count <= 5, "clipboard took {clipboard_count} of 10 rows even though another provider had a match");
        assert_eq!(items.iter().filter(|i| i.kind == "settings").count(), 1, "the other provider's reservation must still survive");
    }

    #[test]
    fn clipboard_is_not_capped_when_it_is_the_only_provider_with_candidates() {
        // If nothing else matched this query, there's no reason to shorten
        // the list — the cap only exists to make room for other real
        // matches, not as a blanket ceiling on clipboard.
        let providers = vec![("clipboard", candidates("clipboard", &[20.0; 9])), ("settings", Vec::new())];
        let items = allocate(providers, 10, "");
        assert_eq!(items.iter().filter(|i| i.kind == "clipboard").count(), 9);
    }

    #[test]
    fn clipboard_cap_still_leaves_room_for_a_secondary_match_with_few_candidates() {
        // The pre-existing "secondary match never crowded out" guarantee
        // (see `a_secondary_match_is_never_crowded_out_by_many_primary_matches`
        // above) must keep holding for clipboard specifically, in whichever
        // role — here as the *primary*, capped provider, not the crowded
        // secondary.
        let providers = vec![("clipboard", candidates("clipboard", &[10.0; 10])), ("app", candidates("app", &[1.0]))];
        let items = allocate(providers, 8, "");
        assert_eq!(items.iter().filter(|i| i.kind == "app").count(), 1, "app's one real match must still survive");
        assert!(items.iter().filter(|i| i.kind == "clipboard").count() <= clipboard_max_slots(8));
    }

    // --- Section order: the captain-reported defect this task fixes.
    // `allocate` used to `flat_map` providers in their given (registration)
    // order unconditionally — score decided *how many* rows a section got,
    // never *where* the section itself sat. ---

    #[test]
    fn section_order_follows_content_strength_not_registration_order() {
        // The real captain-reported shape for "clipboard history": `command`
        // is registered *after* `clipboard` in `AppState::with_test_providers`
        // (app, file, clipboard, settings, command), so the old code always
        // rendered clipboard's section above command's regardless of score —
        // even though the command provider's alias match for "clipboard
        // history" scores far higher than any of a handful of ordinary
        // clipboard entries. Registration order here deliberately puts the
        // weaker section (clipboard) first and the stronger one (command)
        // last, so this only passes if section order is actually driven by
        // score.
        let providers = vec![
            ("clipboard", candidates("clipboard", &[3.0, 2.5, 2.0, 1.5])),
            ("command", vec![Candidate { score: 20.0, item: item_titled("command", "clipboard-history", "Clipboard History") }]),
        ];
        let items = allocate(providers, 8, "clipboard history");
        let first_kind = items.first().map(|i| i.kind.as_str());
        assert_eq!(first_kind, Some("command"), "the stronger section (command) must render first, not the weaker one that merely registered earlier");
        // Every clipboard row must still come after every command row —
        // section order is a hard grouping change, not just "the top row."
        let command_end = items.iter().rposition(|i| i.kind == "command").unwrap();
        let clipboard_start = items.iter().position(|i| i.kind == "clipboard").unwrap();
        assert!(command_end < clipboard_start, "no clipboard row may render above the command section");
    }

    #[test]
    fn section_order_is_not_fooled_by_a_freshly_copied_clipboard_entry_outscoring_a_decisive_command_match() {
        // Caught live during this task, not assumed: a synthetic
        // "clipboard history"-shaped repro (isolated daemon, seeded
        // fixtures) showed a clipboard entry copied moments ago, which
        // merely *starts with* the query text, out-scoring the `Clipboard
        // History` command's own perfect alias match by close to
        // `clipboard::CLIPBOARD_RECENCY_BOOST_CEILING` — purely because it
        // was fresh, not because it was more relevant. These illustrative
        // numbers reproduce that shape: without the discount, clipboard
        // (28.0) would beat command (22.0) into first section; with it,
        // clipboard's own section-ranking score (28.0 - 8.0 = 20.0) loses,
        // as it should — command really is the more decisive match.
        let providers = vec![
            (
                "clipboard",
                vec![Candidate {
                    score: 20.0 + clipboard::CLIPBOARD_RECENCY_BOOST_CEILING,
                    item: item_titled("clipboard", "clip-fresh", "clipboard history feature test note"),
                }],
            ),
            ("command", vec![Candidate { score: 22.0, item: item_titled("command", "clipboard-history", "Clipboard History") }]),
        ];
        let items = allocate(providers, 8, "clipboard history");
        assert_eq!(items.first().map(|i| i.kind.as_str()), Some("command"), "a merely-fresh clipboard entry must not out-rank a more decisive command match for section order");
    }

    #[test]
    fn section_order_still_lets_clipboard_lead_when_it_wins_on_match_strength_alone() {
        // The discount must not become a blanket "clipboard never leads" —
        // if clipboard's match is decisively stronger even after removing
        // its entire possible freshness bonus, it still leads.
        let providers = vec![
            (
                "clipboard",
                vec![Candidate {
                    score: 50.0 + clipboard::CLIPBOARD_RECENCY_BOOST_CEILING,
                    item: item_titled("clipboard", "clip-strong", "clipboard history feature test note"),
                }],
            ),
            ("command", vec![Candidate { score: 22.0, item: item_titled("command", "clipboard-history", "Clipboard History") }]),
        ];
        let items = allocate(providers, 8, "clipboard history");
        assert_eq!(items.first().map(|i| i.kind.as_str()), Some("clipboard"), "a genuinely stronger clipboard match must still be allowed to lead");
    }

    #[test]
    fn section_order_follows_content_strength_for_a_pane_query_too() {
        // The other real captain-reported shape: "displays" must put the
        // Displays pane at or near the top, not beneath unrelated files.
        let providers = vec![
            ("file", candidates("file", &[4.0, 3.5, 3.0, 2.5])),
            (
                "settings",
                vec![Candidate {
                    score: settings_category_score("displays", "Displays", fuzzy_score("displays", "Displays").unwrap()),
                    item: item_titled("settings", "com.apple.preference.displays", "Displays"),
                }],
            ),
        ];
        let items = allocate(providers, 8, "displays");
        assert_eq!(items.first().map(|i| i.kind.as_str()), Some("settings"), "the pane's own section must lead, not the file section");
    }

    #[test]
    fn section_order_ties_break_by_registration_order() {
        // Equal top-candidate scores must resolve to a fixed, repeatable
        // order rather than depending on incidental sort implementation
        // details — `providers`' own given order is the documented tiebreak.
        // Deliberately avoids the "app"/"settings"/"clipboard" ids:
        // `allocate` rescores the first two (the category-bonus pass above)
        // and discounts the third's freshness bonus (the section-ordering
        // pass's own clipboard-specific adjustment) before this comparison
        // ever runs, either of which would turn an intended tie into a real
        // score difference and defeat the point of this test.
        let providers = vec![
            ("file", vec![Candidate { score: 5.0, item: item_titled("file", "file-a", "match") }]),
            ("command", vec![Candidate { score: 5.0, item: item_titled("command", "command-a", "match") }]),
            ("widget", vec![Candidate { score: 5.0, item: item_titled("widget", "widget-a", "match") }]),
        ];
        let items = allocate(providers, 8, "");
        let kinds: Vec<&str> = items.iter().map(|i| i.kind.as_str()).collect();
        assert_eq!(kinds, vec!["file", "command", "widget"], "a three-way tie must resolve to registration order every time");
    }

    #[test]
    fn section_order_ties_break_by_registration_order_is_stable_across_repeated_runs() {
        // Non-jitter guard: the exact same input, run twice, must produce
        // the exact same section order — no reliance on hash-map iteration
        // or anything else that could vary run to run.
        let build = || {
            vec![
                ("command", vec![Candidate { score: 5.0, item: item_titled("command", "command-a", "match") }]),
                ("file", vec![Candidate { score: 5.0, item: item_titled("file", "file-a", "match") }]),
            ]
        };
        let first: Vec<String> = allocate(build(), 8, "").iter().map(|i| i.kind.clone()).collect();
        let second: Vec<String> = allocate(build(), 8, "").iter().map(|i| i.kind.clone()).collect();
        assert_eq!(first, second);
        assert_eq!(first, vec!["command", "file"], "registration order (command before file here) must be the stable tiebreak");
    }
}

