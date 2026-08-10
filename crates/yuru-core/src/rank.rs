use crate::{
    fzf_query, key_kind_allowed,
    query::{key_blocked_by_config, prepare_query_variants, variant_blocked_by_config},
    score_exact_text, score_text, ExactMatcher, GreedyMatcher, LanguageBackend, MatcherAlgo,
    MatcherBackend, NucleoMatcher, QueryVariant, SearchConfig, SearchStats, Tiebreak,
};
use rayon::prelude::*;
use std::{cmp::Ordering, collections::BinaryHeap};

const STREAMING_TOP_RESULTS_LIMIT: usize = 1024;
#[cfg(not(test))]
const PARALLEL_SEARCH_CHUNK_SIZE: usize = 4096;
#[cfg(test)]
const PARALLEL_SEARCH_CHUNK_SIZE: usize = 2;
#[cfg(not(test))]
const PARALLEL_SEARCH_THRESHOLD: usize = 100_000;
#[cfg(test)]
const PARALLEL_SEARCH_THRESHOLD: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScoreMode {
    Greedy,
    Exact,
}

impl ScoreMode {
    fn score(self, pattern: &str, text: &str, case_sensitive: bool) -> Option<i64> {
        match self {
            Self::Greedy => score_text(pattern, text, case_sensitive),
            Self::Exact => score_exact_text(pattern, text, case_sensitive),
        }
    }
}

/// Candidate result after scoring and ranking.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoredCandidate {
    /// Stable input-order identifier from the indexed candidate.
    pub id: usize,
    /// Display text selected from the indexed candidate.
    pub display: String,
    /// Final score after key and query weights.
    pub score: i64,
    /// Kind of key that produced the best score.
    pub key_kind: crate::KeyKind,
    /// Index of the key that produced the best score.
    pub key_index: u32,
}

/// Searches candidates and returns ranked results.
pub fn search(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> Vec<ScoredCandidate> {
    if config.disabled {
        return search_disabled(query, candidates, config).0;
    }

    if config.extended && fzf_query::requires_extended_search(query) {
        return search_extended_auto(query, candidates, backend, config).0;
    }

    if !config.exact
        && matches!(
            config.matcher_algo,
            MatcherAlgo::FzfV2 | MatcherAlgo::Nucleo
        )
    {
        return search_nucleo_with_stats(query, candidates, backend, config).0;
    }

    let score_mode = if config.exact {
        ScoreMode::Exact
    } else {
        ScoreMode::Greedy
    };
    search_standard(query, candidates, backend, score_mode, config).0
}

fn matcher_for_config(config: &SearchConfig) -> Box<dyn MatcherBackend> {
    if config.exact {
        return Box::new(ExactMatcher::new(config.case_sensitive));
    }

    match config.matcher_algo {
        MatcherAlgo::Greedy | MatcherAlgo::FzfV1 => {
            Box::new(GreedyMatcher::new(config.case_sensitive))
        }
        MatcherAlgo::FzfV2 | MatcherAlgo::Nucleo => {
            Box::new(NucleoMatcher::new(config.case_sensitive))
        }
    }
}

fn search_standard(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    score_mode: ScoreMode,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let variants = prepare_query_variants(query, backend, config);
    if should_search_parallel(candidates.len()) {
        return search_standard_parallel(query, candidates, &variants, score_mode, config);
    }

    let mut stats = SearchStats {
        variants_seen: variants.len(),
        ..SearchStats::default()
    };
    let mut results = Vec::new();
    let mut top_results = TopResults::enabled(query, config);

    for candidate in candidates {
        stats.candidates_seen += 1;
        if let Some(scored) =
            score_standard_candidate(candidate, &variants, score_mode, config, &mut stats)
        {
            push_scored(scored, &mut results, top_results.as_mut());
        }
    }

    (finish_results(results, top_results, query, config), stats)
}

fn should_search_parallel(len: usize) -> bool {
    len >= PARALLEL_SEARCH_THRESHOLD && rayon::current_num_threads() > 1
}

fn search_standard_parallel(
    query: &str,
    candidates: &[crate::Candidate],
    variants: &[QueryVariant],
    score_mode: ScoreMode,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let (mut results, stats) = candidates
        .par_chunks(PARALLEL_SEARCH_CHUNK_SIZE)
        .map(|chunk| {
            let mut stats = SearchStats {
                variants_seen: variants.len(),
                ..SearchStats::default()
            };
            let mut results = Vec::new();
            let mut top_results = TopResults::enabled(query, config);

            for candidate in chunk {
                stats.candidates_seen += 1;
                if let Some(scored) =
                    score_standard_candidate(candidate, variants, score_mode, config, &mut stats)
                {
                    push_scored(scored, &mut results, top_results.as_mut());
                }
            }

            (finish_results(results, top_results, query, config), stats)
        })
        .reduce(
            || (Vec::new(), SearchStats::default()),
            |(mut left_results, mut left_stats), (mut right_results, right_stats)| {
                left_results.append(&mut right_results);
                merge_stats(&mut left_stats, right_stats);
                (left_results, left_stats)
            },
        );

    finalize_results(&mut results, query, config);
    (results, stats)
}

fn score_standard_candidate(
    candidate: &crate::Candidate,
    variants: &[QueryVariant],
    score_mode: ScoreMode,
    config: &SearchConfig,
    stats: &mut SearchStats,
) -> Option<ScoredCandidate> {
    // `ScoreMode` scores through `score_text`/`score_exact_text`, which fold case with
    // `fold_case_char` exactly when the config asks for case-insensitive matching.
    score_candidate_with(
        candidate,
        variants,
        config,
        !config.case_sensitive,
        stats,
        |pattern, text| score_mode.score(pattern, text, config.case_sensitive),
    )
}

fn score_candidate_with(
    candidate: &crate::Candidate,
    variants: &[QueryVariant],
    config: &SearchConfig,
    scorer_folds_case: bool,
    stats: &mut SearchStats,
    mut score_text: impl FnMut(&str, &str) -> Option<i64>,
) -> Option<ScoredCandidate> {
    let mut best: Option<ScoredCandidate> = None;

    for variant in variants {
        if variant_blocked_by_config(variant.kind, config) {
            continue;
        }

        for (key_index, key) in candidate.keys.iter().enumerate() {
            if key_blocked_by_config(key, config, scorer_folds_case) {
                continue;
            }

            if !key_kind_allowed(variant, key.kind) {
                continue;
            }

            stats.keys_seen += 1;
            stats.fuzzy_calls += 1;

            if let Some(base_score) = score_text(&variant.text, &key.text) {
                let score = base_score + i64::from(variant.weight + key.weight);
                let scored = ScoredCandidate {
                    id: candidate.id,
                    display: candidate.display.clone(),
                    score,
                    key_kind: key.kind,
                    key_index: key_index as u32,
                };

                if best
                    .as_ref()
                    .is_none_or(|current| scored.score > current.score)
                {
                    best = Some(scored);
                }
            }
        }
    }

    best
}

fn search_nucleo_with_stats(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let variants = prepare_query_variants(query, backend, config);
    if should_search_parallel(candidates.len()) {
        return search_nucleo_parallel(query, candidates, &variants, config);
    }

    let mut matcher = NucleoMatcher::new(config.case_sensitive);
    search_with_matcher_variants(query, candidates, &variants, &mut matcher, config)
}

fn search_nucleo_parallel(
    query: &str,
    candidates: &[crate::Candidate],
    variants: &[QueryVariant],
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let (mut results, stats) = candidates
        .par_chunks(PARALLEL_SEARCH_CHUNK_SIZE)
        .map(|chunk| {
            let mut matcher = NucleoMatcher::new(config.case_sensitive);
            search_with_matcher_variants(query, chunk, variants, &mut matcher, config)
        })
        .reduce(
            || (Vec::new(), SearchStats::default()),
            |(mut left_results, mut left_stats), (mut right_results, right_stats)| {
                left_results.append(&mut right_results);
                merge_stats(&mut left_stats, right_stats);
                (left_results, left_stats)
            },
        );

    finalize_results(&mut results, query, config);
    (results, stats)
}

fn merge_stats(left: &mut SearchStats, right: SearchStats) {
    left.candidates_seen += right.candidates_seen;
    left.keys_seen += right.keys_seen;
    left.fuzzy_calls += right.fuzzy_calls;
    left.quality_score_calls += right.quality_score_calls;
    left.reading_generation_calls += right.reading_generation_calls;
    left.variants_seen = left.variants_seen.max(right.variants_seen);
}

/// Searches candidates and returns ranked results with execution counters.
pub fn search_with_stats(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    if config.disabled {
        return search_disabled(query, candidates, config);
    }

    if config.extended && fzf_query::requires_extended_search(query) {
        let prepared = fzf_query::PreparedQuery::new(query, backend, config);
        return search_extended(query, candidates, &prepared, matcher, config);
    }

    search_standard_with_matcher(query, candidates, backend, matcher, config)
}

fn search_disabled(
    query: &str,
    candidates: &[crate::Candidate],
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let mut results: Vec<_> = candidates
        .iter()
        .map(|candidate| ScoredCandidate {
            id: candidate.id,
            display: candidate.display.clone(),
            score: 0,
            key_kind: crate::KeyKind::Original,
            key_index: 0,
        })
        .collect();
    finalize_results(&mut results, query, config);
    (
        results,
        SearchStats {
            candidates_seen: candidates.len(),
            ..SearchStats::default()
        },
    )
}

fn search_standard_with_matcher(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let variants = prepare_query_variants(query, backend, config);
    search_with_matcher_variants(query, candidates, &variants, matcher, config)
}

fn search_with_matcher_variants<M: MatcherBackend + ?Sized>(
    query: &str,
    candidates: &[crate::Candidate],
    variants: &[QueryVariant],
    matcher: &mut M,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let mut stats = SearchStats {
        variants_seen: variants.len(),
        ..SearchStats::default()
    };
    let mut results = Vec::new();
    let mut top_results = TopResults::enabled(query, config);

    for candidate in candidates {
        stats.candidates_seen += 1;
        if let Some(scored) =
            score_matcher_candidate(candidate, variants, matcher, config, &mut stats)
        {
            push_scored(scored, &mut results, top_results.as_mut());
        }
    }

    (finish_results(results, top_results, query, config), stats)
}

fn score_matcher_candidate<M: MatcherBackend + ?Sized>(
    candidate: &crate::Candidate,
    variants: &[QueryVariant],
    matcher: &mut M,
    config: &SearchConfig,
    stats: &mut SearchStats,
) -> Option<ScoredCandidate> {
    // The matcher may be caller-owned, so only it can say whether it folds case.
    let scorer_folds_case = matcher.folds_case();
    score_candidate_with(
        candidate,
        variants,
        config,
        scorer_folds_case,
        stats,
        |pattern, text| matcher.score(pattern, text),
    )
}

/// Runs an extended-query search, going parallel for large candidate sets.
///
/// Used by [`search`], which owns matcher selection; [`search_with_stats`] stays
/// sequential so that the caller-supplied matcher is always the one used.
fn search_extended_auto(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let prepared = fzf_query::PreparedQuery::new(query, backend, config);
    if should_search_parallel(candidates.len()) {
        return search_extended_parallel(query, candidates, &prepared, config);
    }

    let mut matcher = matcher_for_config(config);
    search_extended(query, candidates, &prepared, matcher.as_mut(), config)
}

fn search_extended_parallel(
    query: &str,
    candidates: &[crate::Candidate],
    prepared: &fzf_query::PreparedQuery,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let (mut results, stats) = candidates
        .par_chunks(PARALLEL_SEARCH_CHUNK_SIZE)
        .map(|chunk| {
            let mut matcher = matcher_for_config(config);
            search_extended(query, chunk, prepared, matcher.as_mut(), config)
        })
        .reduce(
            || (Vec::new(), SearchStats::default()),
            |(mut left_results, mut left_stats), (mut right_results, right_stats)| {
                left_results.append(&mut right_results);
                merge_stats(&mut left_stats, right_stats);
                (left_results, left_stats)
            },
        );

    finalize_results(&mut results, query, config);
    (results, stats)
}

fn search_extended<M: MatcherBackend + ?Sized>(
    query: &str,
    candidates: &[crate::Candidate],
    prepared: &fzf_query::PreparedQuery,
    matcher: &mut M,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let mut results = Vec::new();
    let mut top_results = TopResults::enabled(query, config);
    let mut stats = SearchStats {
        variants_seen: prepared.variants_seen(),
        ..SearchStats::default()
    };

    for candidate in candidates {
        stats.candidates_seen += 1;
        if let Some(scored) =
            fzf_query::score_candidate(prepared, candidate, matcher, config, &mut stats)
        {
            push_scored(scored, &mut results, top_results.as_mut());
        }
    }

    (finish_results(results, top_results, query, config), stats)
}

fn push_scored(
    scored: ScoredCandidate,
    results: &mut Vec<ScoredCandidate>,
    top_results: Option<&mut TopResults>,
) {
    if let Some(top_results) = top_results {
        top_results.push(scored);
    } else {
        results.push(scored);
    }
}

fn finish_results(
    mut results: Vec<ScoredCandidate>,
    top_results: Option<TopResults>,
    query: &str,
    config: &SearchConfig,
) -> Vec<ScoredCandidate> {
    if let Some(top_results) = top_results {
        return top_results.finish();
    }

    finalize_results(&mut results, query, config);
    results
}

/// Streaming top-k accumulator backed by a max-heap of [`RankedResult`].
///
/// [`RankedResult`]'s `Ord` is best-first, so the heap's root is the *worst* entry kept so
/// far: exactly the eviction candidate. That is why there is no `Reverse` wrapper here.
#[derive(Clone, Debug)]
struct TopResults {
    limit: usize,
    context: RankContext,
    results: BinaryHeap<RankedResult>,
}

impl TopResults {
    fn enabled(query: &str, config: &SearchConfig) -> Option<Self> {
        (!config.no_sort && (1..=STREAMING_TOP_RESULTS_LIMIT).contains(&config.limit)).then(|| {
            Self {
                limit: config.limit,
                context: RankContext::new(query, config),
                results: BinaryHeap::with_capacity(config.limit),
            }
        })
    }

    fn push(&mut self, scored: ScoredCandidate) {
        if self.results.len() < self.limit {
            self.results.push(RankedResult::new(scored, &self.context));
            return;
        }

        // Cheap early-out before paying for the tiebreak keys: the root has the lowest
        // score of everything kept, because score is the leading rank component.
        let worst_score = self
            .results
            .peek()
            .expect("top results are full")
            .rank
            .score;
        if scored.score < worst_score {
            return;
        }

        let ranked = RankedResult::new(scored, &self.context);
        let mut worst = self.results.peek_mut().expect("top results are full");
        if ranked < *worst {
            *worst = ranked;
        }
    }

    fn finish(self) -> Vec<ScoredCandidate> {
        self.results
            .into_sorted_vec()
            .into_iter()
            .map(|ranked| ranked.scored)
            .collect()
    }
}

#[derive(Clone, Debug)]
struct RankedResult {
    scored: ScoredCandidate,
    rank: ResultRank,
}

impl RankedResult {
    fn new(scored: ScoredCandidate, context: &RankContext) -> Self {
        Self {
            rank: ResultRank::new(&scored, context),
            scored,
        }
    }
}

impl Ord for RankedResult {
    /// Orders best-first, matching [`compare_results`]: score descending, then every
    /// tiebreak criterion ascending, then display ascending.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .rank
            .score
            .cmp(&self.rank.score)
            .then_with(|| self.rank.keys.cmp(&other.rank.keys))
            .then_with(|| self.scored.display.cmp(&other.scored.display))
    }
}

impl PartialOrd for RankedResult {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for RankedResult {}

impl PartialEq for RankedResult {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

/// Number of comparison keys a [`ResultRank`] can hold: one per [`Tiebreak`], except
/// [`Tiebreak::Pathname`], which contributes two. [`normalized_tiebreaks`] deduplicates,
/// so no criterion can claim its slots twice.
const RANK_KEY_COUNT: usize = 7;

/// Context-free ranking key for a scored candidate.
///
/// `keys` holds the tiebreak values already laid out in the search's criteria order, so
/// comparing two ranks is one lexicographic array comparison and needs no [`RankContext`].
/// Storing the criteria order this way keeps [`RankedResult`] usable as a heap element
/// without carrying a copy of the criteria list in every entry.
///
/// Every rank in one search comes from the same context, hence fills the same slots, so the
/// unused trailing slots are zero on both sides and never affect a comparison.
#[derive(Clone, Debug)]
struct ResultRank {
    score: i64,
    keys: [usize; RANK_KEY_COUNT],
}

impl ResultRank {
    fn new(scored: &ScoredCandidate, context: &RankContext) -> Self {
        let mut keys = [0; RANK_KEY_COUNT];
        let mut used = 0;

        for &criterion in &context.criteria {
            let (key, extra) = match criterion {
                Tiebreak::Length => (scored.display.chars().count(), None),
                Tiebreak::Chunk => (chunk_len(&scored.display, context), None),
                Tiebreak::Pathname => {
                    let (bucket, begin) = pathname_rank(&scored.display, context);
                    (bucket, Some(begin))
                }
                Tiebreak::Begin => (match_begin(&scored.display, context), None),
                Tiebreak::End => (match_end_distance(&scored.display, context), None),
                Tiebreak::Index => (scored.id, None),
            };

            keys[used] = key;
            used += 1;
            if let Some(extra) = extra {
                keys[used] = extra;
                used += 1;
            }
        }

        Self {
            score: scored.score,
            keys,
        }
    }
}

fn finalize_results(results: &mut Vec<ScoredCandidate>, query: &str, config: &SearchConfig) {
    if config.limit == 0 {
        results.clear();
        return;
    }

    if config.no_sort {
        results.sort_by_key(|result| result.id);
        results.truncate(config.limit);
        return;
    }

    let context = RankContext::new(query, config);
    if config.limit < results.len() {
        results.select_nth_unstable_by(config.limit, |left, right| {
            compare_results(left, right, &context)
        });
        results.truncate(config.limit);
    }
    results.sort_by(|left, right| compare_results(left, right, &context));
}

#[derive(Clone, Debug)]
struct RankContext {
    criteria: Vec<Tiebreak>,
    query: String,
}

impl RankContext {
    fn new(query: &str, config: &SearchConfig) -> Self {
        Self {
            criteria: normalized_tiebreaks(&config.tiebreaks),
            query: normalized_query(query),
        }
    }
}

fn compare_results(
    left: &ScoredCandidate,
    right: &ScoredCandidate,
    context: &RankContext,
) -> Ordering {
    right.score.cmp(&left.score).then_with(|| {
        for &criterion in &context.criteria {
            let ordering = compare_tiebreak(left, right, context, criterion);
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        left.display.cmp(&right.display)
    })
}

fn normalized_tiebreaks(criteria: &[Tiebreak]) -> Vec<Tiebreak> {
    let mut out = Vec::new();
    for criterion in criteria {
        if !out.contains(criterion) {
            out.push(*criterion);
        }
    }
    if !out.contains(&Tiebreak::Index) {
        out.push(Tiebreak::Index);
    }
    out
}

fn compare_tiebreak(
    left: &ScoredCandidate,
    right: &ScoredCandidate,
    context: &RankContext,
    criterion: Tiebreak,
) -> Ordering {
    match criterion {
        Tiebreak::Length => left
            .display
            .chars()
            .count()
            .cmp(&right.display.chars().count()),
        Tiebreak::Chunk => {
            chunk_len(&left.display, context).cmp(&chunk_len(&right.display, context))
        }
        Tiebreak::Pathname => {
            pathname_rank(&left.display, context).cmp(&pathname_rank(&right.display, context))
        }
        Tiebreak::Begin => {
            match_begin(&left.display, context).cmp(&match_begin(&right.display, context))
        }
        Tiebreak::End => match_end_distance(&left.display, context)
            .cmp(&match_end_distance(&right.display, context)),
        Tiebreak::Index => left.id.cmp(&right.id),
    }
}

fn comparable(text: &str) -> String {
    crate::normalize::normalize(text)
}

fn normalized_query(query: &str) -> String {
    query
        .split_whitespace()
        .next()
        .map(comparable)
        .unwrap_or_default()
}

fn match_begin(text: &str, context: &RankContext) -> usize {
    let text = comparable(text);
    if context.query.is_empty() {
        return 0;
    }
    text.find(&context.query).unwrap_or(usize::MAX)
}

fn match_end_distance(text: &str, context: &RankContext) -> usize {
    let text = comparable(text);
    if context.query.is_empty() {
        return 0;
    }
    text.rfind(&context.query)
        .map(|start| text.len().saturating_sub(start + context.query.len()))
        .unwrap_or(usize::MAX)
}

fn chunk_len(text: &str, context: &RankContext) -> usize {
    if context.query.is_empty() {
        return 0;
    }

    text.split_whitespace()
        .filter(|chunk| comparable(chunk).contains(&context.query))
        .map(str::len)
        .min()
        .unwrap_or(usize::MAX)
}

fn pathname_rank(text: &str, context: &RankContext) -> (usize, usize) {
    if context.query.is_empty() {
        return (0, 0);
    }

    let basename = text.rsplit(['/', '\\']).next().unwrap_or(text);
    let basename = comparable(basename);
    if let Some(begin) = basename.find(&context.query) {
        return (0, begin);
    }

    (1, match_begin(text, context))
}

#[cfg(test)]
mod tests;
