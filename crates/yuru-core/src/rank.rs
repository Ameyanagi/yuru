use crate::{
    dedup_and_limit_variants, fzf_query, key_kind_allowed, score_exact_text, score_text,
    ExactMatcher, GreedyMatcher, LanguageBackend, MatcherAlgo, MatcherBackend, NucleoMatcher,
    QueryVariant, QueryVariantKind, SearchConfig, SearchStats, Tiebreak,
};
use rayon::prelude::*;
use std::cmp::Ordering;

const STREAMING_TOP_RESULTS_LIMIT: usize = 1024;
const PARALLEL_SEARCH_CHUNK_SIZE: usize = 4096;
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
    fn score(self, pattern: &str, text: &str) -> Option<i64> {
        match self {
            Self::Greedy => score_text(pattern, text),
            Self::Exact => score_exact_text(pattern, text),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoredCandidate {
    pub id: usize,
    pub display: String,
    pub score: i64,
    pub key_kind: crate::KeyKind,
    pub key_index: u32,
}

pub fn search(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> Vec<ScoredCandidate> {
    if config.disabled || config.extended && fzf_query::requires_extended_search(query) {
        let mut matcher = matcher_for_config(config);
        return search_with_stats(query, candidates, backend, matcher.as_mut(), config).0;
    }

    if !config.exact
        && matches!(
            config.matcher_algo,
            MatcherAlgo::FzfV2 | MatcherAlgo::Nucleo
        )
    {
        let mut matcher = matcher_for_config(config);
        return search_with_stats(query, candidates, backend, matcher.as_mut(), config).0;
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
        return Box::new(ExactMatcher);
    }

    match config.matcher_algo {
        MatcherAlgo::Greedy | MatcherAlgo::FzfV1 => Box::new(GreedyMatcher),
        MatcherAlgo::FzfV2 | MatcherAlgo::Nucleo => Box::new(NucleoMatcher::default()),
    }
}

fn search_standard(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    score_mode: ScoreMode,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let variants = dedup_and_limit_variants(backend.expand_query(query), config.max_query_variants);
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
            score_standard_candidate(candidate, &variants, score_mode, config, Some(&mut stats))
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
                if let Some(scored) = score_standard_candidate(
                    candidate,
                    variants,
                    score_mode,
                    config,
                    Some(&mut stats),
                ) {
                    push_scored(scored, &mut results, top_results.as_mut());
                }
            }

            (finish_results(results, top_results, query, config), stats)
        })
        .reduce(
            || (Vec::new(), SearchStats::default()),
            |(mut left_results, mut left_stats), (mut right_results, right_stats)| {
                left_results.append(&mut right_results);
                left_stats.candidates_seen += right_stats.candidates_seen;
                left_stats.keys_seen += right_stats.keys_seen;
                left_stats.fuzzy_calls += right_stats.fuzzy_calls;
                left_stats.quality_score_calls += right_stats.quality_score_calls;
                left_stats.reading_generation_calls += right_stats.reading_generation_calls;
                left_stats.variants_seen = left_stats.variants_seen.max(right_stats.variants_seen);
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
    mut stats: Option<&mut SearchStats>,
) -> Option<ScoredCandidate> {
    let mut best: Option<ScoredCandidate> = None;

    for variant in variants {
        if variant_blocked_by_config(variant.kind, config) {
            continue;
        }

        for (key_index, key) in candidate.keys.iter().enumerate() {
            if key_blocked_by_config(key.kind, config) {
                continue;
            }

            if !key_kind_allowed(variant, key.kind) {
                continue;
            }

            if let Some(stats) = stats.as_deref_mut() {
                stats.keys_seen += 1;
                stats.fuzzy_calls += 1;
            }

            if let Some(base_score) = score_mode.score(&variant.text, &key.text) {
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

pub fn search_with_stats(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    if config.disabled {
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
        return (
            results,
            SearchStats {
                candidates_seen: candidates.len(),
                ..SearchStats::default()
            },
        );
    }

    if config.extended && fzf_query::requires_extended_search(query) {
        return search_extended(query, candidates, backend, matcher, config);
    }

    search_standard_with_matcher(query, candidates, backend, matcher, config)
}

fn search_standard_with_matcher(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let variants = dedup_and_limit_variants(backend.expand_query(query), config.max_query_variants);
    let mut stats = SearchStats {
        variants_seen: variants.len(),
        ..SearchStats::default()
    };
    let mut results = Vec::new();
    let mut top_results = TopResults::enabled(query, config);

    for candidate in candidates {
        stats.candidates_seen += 1;
        let mut best: Option<ScoredCandidate> = None;

        for variant in &variants {
            if variant_blocked_by_config(variant.kind, config) {
                continue;
            }

            for (key_index, key) in candidate.keys.iter().enumerate() {
                if key_blocked_by_config(key.kind, config) {
                    continue;
                }

                if !key_kind_allowed(variant, key.kind) {
                    continue;
                }

                stats.keys_seen += 1;
                stats.fuzzy_calls += 1;

                if let Some(base_score) = matcher.score(&variant.text, &key.text) {
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

        if let Some(scored) = best {
            push_scored(scored, &mut results, top_results.as_mut());
        }
    }

    (finish_results(results, top_results, query, config), stats)
}

fn search_extended(
    query: &str,
    candidates: &[crate::Candidate],
    backend: &dyn LanguageBackend,
    matcher: &mut dyn MatcherBackend,
    config: &SearchConfig,
) -> (Vec<ScoredCandidate>, SearchStats) {
    let mut results = Vec::new();
    let mut top_results = TopResults::enabled(query, config);
    let mut stats = SearchStats::default();

    for candidate in candidates {
        stats.candidates_seen += 1;
        if let Some(scored) =
            fzf_query::score_candidate(query, candidate, backend, matcher, config, &mut stats)
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

#[derive(Clone, Debug)]
struct TopResults {
    limit: usize,
    context: RankContext,
    results: Vec<RankedResult>,
}

impl TopResults {
    fn enabled(query: &str, config: &SearchConfig) -> Option<Self> {
        (!config.no_sort && (1..=STREAMING_TOP_RESULTS_LIMIT).contains(&config.limit)).then(|| {
            Self {
                limit: config.limit,
                context: RankContext::new(query, config),
                results: Vec::with_capacity(config.limit),
            }
        })
    }

    fn push(&mut self, scored: ScoredCandidate) {
        if self.results.len() < self.limit {
            let ranked = RankedResult::new(scored, &self.context);
            self.results.push(ranked);
            return;
        }

        let worst_score = self
            .results
            .iter()
            .map(|result| result.scored.score)
            .min()
            .expect("top results are full");
        if scored.score < worst_score {
            return;
        }

        let worst_index = self.worst_index_for_score(worst_score);
        if scored.score > worst_score {
            self.results[worst_index] = RankedResult::new(scored, &self.context);
            return;
        }

        let ranked = RankedResult::new(scored, &self.context);
        let worst = &self.results[worst_index];
        if compare_ranked_results(&ranked, worst, &self.context).is_lt() {
            self.results[worst_index] = ranked;
        }
    }

    fn worst_index_for_score(&self, score: i64) -> usize {
        self.results
            .iter()
            .enumerate()
            .filter(|(_, result)| result.scored.score == score)
            .max_by(|(_, left), (_, right)| compare_ranked_results(left, right, &self.context))
            .map(|(index, _)| index)
            .expect("score came from existing top results")
    }

    fn finish(mut self) -> Vec<ScoredCandidate> {
        self.results
            .sort_by(|left, right| compare_ranked_results(left, right, &self.context));
        self.results
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

#[derive(Clone, Debug)]
struct ResultRank {
    score: i64,
    length: usize,
    chunk: usize,
    pathname: (usize, usize),
    begin: usize,
    end: usize,
    index: usize,
}

impl ResultRank {
    fn new(scored: &ScoredCandidate, context: &RankContext) -> Self {
        let mut rank = Self {
            score: scored.score,
            length: 0,
            chunk: 0,
            pathname: (0, 0),
            begin: 0,
            end: 0,
            index: scored.id,
        };

        for &criterion in &context.criteria {
            match criterion {
                Tiebreak::Length => rank.length = scored.display.chars().count(),
                Tiebreak::Chunk => rank.chunk = chunk_len(&scored.display, context),
                Tiebreak::Pathname => rank.pathname = pathname_rank(&scored.display, context),
                Tiebreak::Begin => rank.begin = match_begin(&scored.display, context),
                Tiebreak::End => rank.end = match_end_distance(&scored.display, context),
                Tiebreak::Index => rank.index = scored.id,
            }
        }

        rank
    }
}

fn compare_ranked_results(
    left: &RankedResult,
    right: &RankedResult,
    context: &RankContext,
) -> Ordering {
    right.rank.score.cmp(&left.rank.score).then_with(|| {
        for &criterion in &context.criteria {
            let ordering = match criterion {
                Tiebreak::Length => left.rank.length.cmp(&right.rank.length),
                Tiebreak::Chunk => left.rank.chunk.cmp(&right.rank.chunk),
                Tiebreak::Pathname => left.rank.pathname.cmp(&right.rank.pathname),
                Tiebreak::Begin => left.rank.begin.cmp(&right.rank.begin),
                Tiebreak::End => left.rank.end.cmp(&right.rank.end),
                Tiebreak::Index => left.rank.index.cmp(&right.rank.index),
            };
            if ordering != Ordering::Equal {
                return ordering;
            }
        }
        left.scored.display.cmp(&right.scored.display)
    })
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

fn key_blocked_by_config(kind: crate::KeyKind, config: &SearchConfig) -> bool {
    kind == crate::KeyKind::Normalized && (config.case_sensitive || !config.normalize)
}

fn variant_blocked_by_config(kind: QueryVariantKind, config: &SearchConfig) -> bool {
    kind == QueryVariantKind::Normalized && (config.case_sensitive || !config.normalize)
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
mod tests {
    use crate::{build_index, query::PlainBackend};

    use super::*;

    #[test]
    fn sorting_is_deterministic_on_equal_scores() {
        let cfg = SearchConfig::default();
        let candidates = build_index(["abc-one", "abc-two"], &PlainBackend, &cfg);
        let results = search("abc", &candidates, &PlainBackend, &cfg);

        assert_eq!(results[0].display, "abc-one");
        assert_eq!(results[1].display, "abc-two");
    }

    #[test]
    fn search_hot_path_does_not_call_reading_generator() {
        let cfg = SearchConfig::default();
        let candidates = build_index(["東京駅"], &PlainBackend, &cfg);
        let mut matcher = GreedyMatcher;

        let (_results, stats) =
            search_with_stats("tokyo", &candidates, &PlainBackend, &mut matcher, &cfg);

        assert_eq!(stats.reading_generation_calls, 0);
    }

    #[test]
    fn tiebreak_length_prefers_shorter_display_for_equal_scores() {
        let cfg = SearchConfig {
            disabled: true,
            tiebreaks: vec![Tiebreak::Length],
            ..SearchConfig::default()
        };
        let candidates = build_index(["aaaa", "aa"], &PlainBackend, &cfg);
        let results = search("", &candidates, &PlainBackend, &cfg);

        assert_eq!(results[0].display, "aa");
    }

    #[test]
    fn tiebreak_index_prefers_input_order() {
        let cfg = SearchConfig {
            disabled: true,
            tiebreaks: vec![Tiebreak::Index],
            ..SearchConfig::default()
        };
        let candidates = build_index(["aaaa", "aa"], &PlainBackend, &cfg);
        let results = search("", &candidates, &PlainBackend, &cfg);

        assert_eq!(results[0].display, "aaaa");
    }

    #[test]
    fn tiebreak_pathname_prefers_match_in_basename() {
        let cfg = SearchConfig {
            disabled: true,
            tiebreaks: vec![Tiebreak::Pathname],
            ..SearchConfig::default()
        };
        let candidates = build_index(["foo/file.txt", "src/foo.txt"], &PlainBackend, &cfg);
        let results = search("foo", &candidates, &PlainBackend, &cfg);

        assert_eq!(results[0].display, "src/foo.txt");
    }

    #[test]
    fn no_sort_preserves_input_order_after_filtering() {
        let cfg = SearchConfig {
            no_sort: true,
            limit: 2,
            ..SearchConfig::default()
        };
        let candidates = build_index(["zzabc", "abc", "xxabc"], &PlainBackend, &cfg);
        let results = search("abc", &candidates, &PlainBackend, &cfg);

        assert_eq!(
            results
                .iter()
                .map(|result| result.display.as_str())
                .collect::<Vec<_>>(),
            vec!["zzabc", "abc"]
        );
    }

    #[test]
    fn parallel_search_matches_sequential_matcher_results() {
        let cfg = SearchConfig {
            limit: 4,
            ..SearchConfig::default()
        };
        let candidates = build_index(
            [
                "zzabc",
                "abc",
                "src/abc.txt",
                "abc-long-name",
                "a/b/c",
                "prefix-abc",
            ],
            &PlainBackend,
            &cfg,
        );
        let parallel = search("abc", &candidates, &PlainBackend, &cfg);
        let mut matcher = GreedyMatcher;
        let sequential = search_with_stats("abc", &candidates, &PlainBackend, &mut matcher, &cfg).0;

        assert_eq!(parallel, sequential);
    }

    #[test]
    fn streaming_top_results_match_full_sorted_results() {
        let limited_cfg = SearchConfig {
            limit: 3,
            ..SearchConfig::default()
        };
        let full_cfg = SearchConfig {
            limit: usize::MAX,
            ..SearchConfig::default()
        };
        let candidates = build_index(
            [
                "zzabc",
                "abc",
                "src/abc.txt",
                "abc-long-name",
                "a/b/c",
                "prefix-abc",
            ],
            &PlainBackend,
            &full_cfg,
        );

        let limited = search("abc", &candidates, &PlainBackend, &limited_cfg);
        let full = search("abc", &candidates, &PlainBackend, &full_cfg);

        assert_eq!(limited, full[..3]);
    }
}
