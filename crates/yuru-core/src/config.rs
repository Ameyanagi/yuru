/// Search and ranking configuration used by index and query execution.
#[derive(Clone, Debug)]
pub struct SearchConfig {
    /// Maximum number of expanded query variants kept for one query.
    pub max_query_variants: usize,
    /// Maximum number of search keys kept for one candidate.
    pub max_search_keys_per_candidate: usize,
    /// Maximum total UTF-8 bytes for non-base search keys on one candidate.
    pub max_total_key_bytes_per_candidate: usize,
    /// Maximum number of results returned after ranking.
    pub limit: usize,
    /// Number of top candidates to rescore for quality-oriented ordering.
    pub top_b_for_quality_score: usize,
    /// Uses exact substring scoring instead of fuzzy subsequence scoring.
    pub exact: bool,
    /// Enables fzf-style extended query syntax.
    pub extended: bool,
    /// Preserves case during matching when true.
    pub case_sensitive: bool,
    /// Disables filtering and returns candidates in ranking order only.
    pub disabled: bool,
    /// Keeps input order instead of sorting by score.
    pub no_sort: bool,
    /// Adds normalized candidate and query keys.
    pub normalize: bool,
    /// Fuzzy matcher implementation used for scoring.
    pub matcher_algo: MatcherAlgo,
    /// Ordered tiebreak rules applied after score comparison.
    pub tiebreaks: Vec<Tiebreak>,
}

/// Candidate-key generation budget passed to language backends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeyBudget {
    /// Maximum number of generated search keys to produce for one candidate.
    pub max_keys: usize,
    /// Maximum total UTF-8 bytes to spend on generated key text.
    pub max_total_key_bytes: usize,
}

/// Query-variant generation budget passed to language backends.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct QueryBudget {
    /// Maximum number of query variants to produce for one query.
    pub max_variants: usize,
}

impl SearchConfig {
    /// Returns the candidate-key generation budget represented by this config.
    pub fn key_budget(&self) -> KeyBudget {
        KeyBudget {
            max_keys: self.max_search_keys_per_candidate,
            max_total_key_bytes: self.max_total_key_bytes_per_candidate,
        }
    }

    /// Returns the query-variant generation budget represented by this config.
    pub fn query_budget(&self) -> QueryBudget {
        QueryBudget {
            max_variants: self.max_query_variants,
        }
    }
}

impl Default for KeyBudget {
    fn default() -> Self {
        SearchConfig::default().key_budget()
    }
}

impl Default for QueryBudget {
    fn default() -> Self {
        SearchConfig::default().query_budget()
    }
}

/// Matcher implementation selected for fuzzy scoring.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatcherAlgo {
    /// Yuru's default greedy matcher.
    Greedy,
    /// Greedy fzf-style matcher alias.
    FzfV1,
    /// Nucleo-backed quality matcher alias.
    FzfV2,
    /// Nucleo-backed quality matcher.
    Nucleo,
}

/// Secondary sort rule used when scores are equal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tiebreak {
    /// Prefer shorter display strings.
    Length,
    /// Prefer fewer separated match chunks.
    Chunk,
    /// Prefer path-like matches with better pathname position.
    Pathname,
    /// Prefer matches closer to the beginning.
    Begin,
    /// Prefer matches closer to the end.
    End,
    /// Prefer lower original candidate index.
    Index,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            max_query_variants: 8,
            max_search_keys_per_candidate: 8,
            max_total_key_bytes_per_candidate: 1024,
            limit: 10,
            top_b_for_quality_score: 1000,
            exact: false,
            extended: true,
            case_sensitive: false,
            disabled: false,
            no_sort: false,
            normalize: true,
            matcher_algo: MatcherAlgo::Greedy,
            tiebreaks: vec![Tiebreak::Length, Tiebreak::Index],
        }
    }
}
