use std::collections::HashMap;

use crate::{
    KeyBudget, KeyKind, LangMode, LanguageBackend, QueryBudget, QueryVariantKind, SearchConfig,
};

/// Search text variant produced from the user's query.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryVariant {
    /// Text that will be matched against compatible search keys.
    pub text: String,
    /// Variant type used to filter compatible key kinds.
    pub kind: QueryVariantKind,
    /// Score adjustment for this variant type.
    pub weight: i32,
}

impl QueryVariant {
    /// Creates an original query variant.
    pub fn original(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Original,
            weight: 500,
        }
    }

    /// Creates a normalized query variant.
    pub fn normalized(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Normalized,
            weight: 450,
        }
    }

    /// Creates a kana query variant.
    pub fn kana(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Kana,
            weight: 350,
        }
    }

    /// Creates a romaji-to-kana query variant.
    pub fn romaji_to_kana(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::RomajiToKana,
            weight: 200,
        }
    }

    /// Creates a pinyin query variant.
    pub fn pinyin(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Pinyin,
            weight: 250,
        }
    }

    /// Creates an initials query variant.
    pub fn initials(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Initials,
            weight: 250,
        }
    }
}

#[derive(Clone, Debug, Default)]
/// Backend that only uses original and normalized text.
pub struct PlainBackend;

impl LanguageBackend for PlainBackend {
    fn mode(&self) -> LangMode {
        LangMode::Plain
    }

    fn build_candidate_keys(&self, _text: &str, _budget: KeyBudget) -> Vec<crate::SearchKey> {
        Vec::new()
    }

    fn expand_query(&self, query: &str, _budget: QueryBudget) -> Vec<QueryVariant> {
        base_query_variants(query)
    }
}

/// Builds the language-neutral original and normalized query variants.
pub fn base_query_variants(query: &str) -> Vec<QueryVariant> {
    let mut variants = vec![QueryVariant::original(query)];
    let normalized = crate::normalize::normalize(query);
    if normalized != query {
        variants.push(QueryVariant::normalized(normalized));
    }
    variants
}

/// Deduplicates variants by text and key coverage, then applies the query cap.
pub fn dedup_and_limit_variants(
    variants: Vec<QueryVariant>,
    max_query_variants: usize,
) -> Vec<QueryVariant> {
    let mut seen_coverage_by_text = HashMap::new();
    let mut out = Vec::new();

    for variant in variants {
        let coverage = key_kind_coverage(variant.kind);
        let seen_coverage = seen_coverage_by_text
            .entry(variant.text.clone())
            .or_insert(0u16);
        if coverage & !*seen_coverage != 0 {
            *seen_coverage |= coverage;
            out.push(variant);
        }
        if out.len() >= max_query_variants {
            break;
        }
    }

    out
}

/// Expands and caps query variants for one search run.
pub(crate) fn prepare_query_variants(
    query: &str,
    backend: &dyn LanguageBackend,
    config: &SearchConfig,
) -> Vec<QueryVariant> {
    dedup_and_limit_variants(
        backend.expand_query(query, config.query_budget()),
        config.max_query_variants,
    )
}

/// Returns whether a key kind is disabled by case/normalization settings.
pub(crate) fn key_blocked_by_config(kind: KeyKind, config: &SearchConfig) -> bool {
    kind == KeyKind::Normalized && (config.case_sensitive || !config.normalize)
}

/// Returns whether a query variant is disabled by case/normalization settings.
pub(crate) fn variant_blocked_by_config(kind: QueryVariantKind, config: &SearchConfig) -> bool {
    kind == QueryVariantKind::Normalized && (config.case_sensitive || !config.normalize)
}

fn key_kind_coverage(kind: QueryVariantKind) -> u16 {
    compatible_key_kinds(kind)
        .iter()
        .fold(0, |coverage, kind| coverage | key_kind_bit(*kind))
}

/// Returns whether a query variant may be scored against a key kind.
pub fn key_kind_allowed(variant: &QueryVariant, kind: KeyKind) -> bool {
    compatible_key_kinds(variant.kind).contains(&kind)
}

const ORIGINAL_QUERY_KEYS: &[KeyKind] = &[
    KeyKind::Original,
    KeyKind::Normalized,
    KeyKind::RomajiReading,
    KeyKind::PinyinFull,
    KeyKind::PinyinJoined,
    KeyKind::KoreanRomanized,
    KeyKind::KoreanInitials,
    KeyKind::KoreanKeyboard,
    KeyKind::LearnedAlias,
];
const KANA_QUERY_KEYS: &[KeyKind] = &[KeyKind::KanaReading];
const PINYIN_QUERY_KEYS: &[KeyKind] = &[KeyKind::PinyinFull, KeyKind::PinyinJoined];
const INITIAL_QUERY_KEYS: &[KeyKind] = &[
    KeyKind::PinyinInitials,
    KeyKind::KoreanInitials,
    KeyKind::LearnedAlias,
];

fn compatible_key_kinds(kind: QueryVariantKind) -> &'static [KeyKind] {
    match kind {
        QueryVariantKind::Original | QueryVariantKind::Normalized => ORIGINAL_QUERY_KEYS,
        QueryVariantKind::Kana | QueryVariantKind::RomajiToKana => KANA_QUERY_KEYS,
        QueryVariantKind::Pinyin => PINYIN_QUERY_KEYS,
        QueryVariantKind::Initials => INITIAL_QUERY_KEYS,
    }
}

fn key_kind_bit(kind: KeyKind) -> u16 {
    1 << (kind as u16)
}

#[cfg(test)]
mod tests;
