#[derive(Clone, Debug)]
pub struct SearchConfig {
    pub max_query_variants: usize,
    pub max_search_keys_per_candidate: usize,
    pub max_total_key_bytes_per_candidate: usize,
    pub limit: usize,
    pub top_b_for_quality_score: usize,
    pub exact: bool,
    pub extended: bool,
    pub case_sensitive: bool,
    pub disabled: bool,
    pub no_sort: bool,
    pub matcher_algo: MatcherAlgo,
    pub tiebreaks: Vec<Tiebreak>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatcherAlgo {
    Greedy,
    FzfV1,
    FzfV2,
    Nucleo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Tiebreak {
    Length,
    Chunk,
    Pathname,
    Begin,
    End,
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
            matcher_algo: MatcherAlgo::Greedy,
            tiebreaks: vec![Tiebreak::Length, Tiebreak::Index],
        }
    }
}
