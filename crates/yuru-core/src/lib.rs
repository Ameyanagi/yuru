//! Core candidate indexing, fuzzy matching, ranking, and source-span data types
//! for Yuru.
//!
//! This crate is intentionally language-neutral. Japanese, Korean, and Chinese
//! phonetic keys are supplied by separate backend crates through
//! [`LanguageBackend`].

/// Candidate indexing and search-key construction.
pub mod candidate;
/// Search configuration knobs shared by the CLI and TUI.
pub mod config;
/// Internal parser and scorer for fzf-style extended queries.
mod fzf_query;
/// Fuzzy and exact matching backends.
pub mod matcher;
/// Unicode normalization helpers used before matching.
pub mod normalize;
/// Query expansion and key compatibility rules.
pub mod query;
/// Candidate ranking and top-result selection.
pub mod rank;
/// Counters collected while searching.
pub mod stats;

use std::fmt;
use std::str::FromStr;

pub use candidate::{
    build_candidate, build_index, dedup_and_limit_keys, Candidate, MappedText, MappedTextBuilder,
    SearchKey, SourceSpan,
};
pub use config::{KeyBudget, MatcherAlgo, QueryBudget, SearchConfig, Tiebreak};
pub use matcher::{
    match_positions, score_exact_text, score_text, ExactMatcher, GreedyMatcher, MatchPositions,
    MatcherBackend, NucleoMatcher,
};
pub use query::{
    base_query_variants, dedup_and_limit_variants, key_kind_allowed, PlainBackend, QueryVariant,
};
pub use rank::{search, search_with_stats, ScoredCandidate};
pub use stats::SearchStats;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Language backend selected for one search run.
pub enum LangMode {
    /// No language-specific phonetic expansion.
    Plain,
    /// Japanese kana and romaji expansion.
    Japanese,
    /// Korean Hangul romanization, initials, and keyboard expansion.
    Korean,
    /// Chinese pinyin and initials expansion.
    Chinese,
    /// Japanese, Korean, and Chinese expansion together.
    All,
}

impl fmt::Display for LangMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LangMode::Plain => f.write_str("plain"),
            LangMode::Japanese => f.write_str("ja"),
            LangMode::Korean => f.write_str("ko"),
            LangMode::Chinese => f.write_str("zh"),
            LangMode::All => f.write_str("all"),
        }
    }
}

impl FromStr for LangMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "plain" => Ok(LangMode::Plain),
            "ja" | "japanese" => Ok(LangMode::Japanese),
            "ko" | "korean" => Ok(LangMode::Korean),
            "zh" | "chinese" => Ok(LangMode::Chinese),
            "all" => Ok(LangMode::All),
            other => Err(format!("unsupported language mode: {other}")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Kind of indexed key attached to a candidate.
pub enum KeyKind {
    /// Original display text.
    Original,
    /// Normalized display text.
    Normalized,
    /// Japanese kana reading.
    KanaReading,
    /// Japanese romaji reading.
    RomajiReading,
    /// Chinese pinyin syllables separated by spaces.
    PinyinFull,
    /// Chinese pinyin joined without separators.
    PinyinJoined,
    /// Chinese pinyin initials.
    PinyinInitials,
    /// Korean romanized Hangul.
    KoreanRomanized,
    /// Korean Hangul initial consonants.
    KoreanInitials,
    /// Korean keyboard-layout spelling.
    KoreanKeyboard,
    /// User-learned alias key.
    LearnedAlias,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
/// Kind of query expansion produced before scoring.
pub enum QueryVariantKind {
    /// Query text exactly as entered.
    Original,
    /// Normalized query text.
    Normalized,
    /// Kana query text.
    Kana,
    /// Romaji query converted to kana.
    RomajiToKana,
    /// Pinyin query text.
    Pinyin,
    /// Initial-letter query text.
    Initials,
}

/// Language-specific candidate and query expansion.
pub trait LanguageBackend: Send + Sync {
    /// Returns the language mode implemented by this backend.
    fn mode(&self) -> LangMode;

    /// Normalizes candidate display text before the base normalized key is added.
    fn normalize_candidate(&self, text: &str) -> String {
        normalize::normalize(text)
    }

    /// Builds additional language-specific search keys for candidate text.
    fn build_candidate_keys(&self, text: &str, budget: KeyBudget) -> Vec<SearchKey>;

    /// Expands a user query into language-specific query variants.
    fn expand_query(&self, query: &str, budget: QueryBudget) -> Vec<QueryVariant>;
}
