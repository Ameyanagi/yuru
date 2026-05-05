pub mod candidate;
pub mod config;
pub mod fzf_query;
pub mod matcher;
pub mod normalize;
pub mod query;
pub mod rank;
pub mod stats;

use std::fmt;
use std::str::FromStr;

pub use candidate::{
    build_candidate, build_index, dedup_and_limit_keys, Candidate, SearchKey, SourceSpan,
};
pub use config::{SearchConfig, Tiebreak};
pub use matcher::{
    match_positions, score_exact_text, score_text, ExactMatcher, GreedyMatcher, MatchPositions,
    MatcherBackend,
};
pub use query::{
    base_query_variants, dedup_and_limit_variants, key_kind_allowed, PlainBackend, QueryVariant,
};
pub use rank::{search, search_with_stats, ScoredCandidate};
pub use stats::SearchStats;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LangMode {
    Plain,
    Japanese,
    Chinese,
}

impl fmt::Display for LangMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LangMode::Plain => f.write_str("plain"),
            LangMode::Japanese => f.write_str("ja"),
            LangMode::Chinese => f.write_str("zh"),
        }
    }
}

impl FromStr for LangMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "plain" => Ok(LangMode::Plain),
            "ja" | "japanese" => Ok(LangMode::Japanese),
            "zh" | "chinese" => Ok(LangMode::Chinese),
            other => Err(format!("unsupported language mode: {other}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum KeyKind {
    Original,
    Normalized,
    KanaReading,
    RomajiReading,
    PinyinFull,
    PinyinJoined,
    PinyinInitials,
    LearnedAlias,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum QueryVariantKind {
    Original,
    Normalized,
    RomajiToKana,
    Pinyin,
    Initials,
}

pub trait LanguageBackend: Send + Sync {
    fn mode(&self) -> LangMode;

    fn normalize_candidate(&self, text: &str) -> String {
        normalize::normalize(text)
    }

    fn build_candidate_keys(&self, text: &str) -> Vec<SearchKey>;

    fn expand_query(&self, query: &str) -> Vec<QueryVariant>;
}
