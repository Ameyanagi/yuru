use std::collections::HashSet;

use crate::{KeyKind, LangMode, LanguageBackend, QueryVariantKind};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueryVariant {
    pub text: String,
    pub kind: QueryVariantKind,
    pub weight: i32,
}

impl QueryVariant {
    pub fn original(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Original,
            weight: 500,
        }
    }

    pub fn normalized(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Normalized,
            weight: 450,
        }
    }

    pub fn romaji_to_kana(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::RomajiToKana,
            weight: 200,
        }
    }

    pub fn pinyin(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Pinyin,
            weight: 250,
        }
    }

    pub fn initials(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            kind: QueryVariantKind::Initials,
            weight: 250,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct PlainBackend;

impl LanguageBackend for PlainBackend {
    fn mode(&self) -> LangMode {
        LangMode::Plain
    }

    fn build_candidate_keys(&self, _text: &str) -> Vec<crate::SearchKey> {
        Vec::new()
    }

    fn expand_query(&self, query: &str) -> Vec<QueryVariant> {
        base_query_variants(query)
    }
}

pub fn base_query_variants(query: &str) -> Vec<QueryVariant> {
    let mut variants = vec![QueryVariant::original(query)];
    let normalized = crate::normalize::normalize(query);
    if normalized != query {
        variants.push(QueryVariant::normalized(normalized));
    }
    variants
}

pub fn dedup_and_limit_variants(
    variants: Vec<QueryVariant>,
    max_query_variants: usize,
) -> Vec<QueryVariant> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();

    for variant in variants {
        if seen.insert(variant.text.clone()) {
            out.push(variant);
        }
        if out.len() >= max_query_variants {
            break;
        }
    }

    out
}

pub fn key_kind_allowed(variant: &QueryVariant, kind: KeyKind) -> bool {
    match variant.kind {
        QueryVariantKind::Original | QueryVariantKind::Normalized => matches!(
            kind,
            KeyKind::Original
                | KeyKind::Normalized
                | KeyKind::RomajiReading
                | KeyKind::PinyinFull
                | KeyKind::PinyinJoined
                | KeyKind::LearnedAlias
        ),
        QueryVariantKind::RomajiToKana => matches!(kind, KeyKind::KanaReading),
        QueryVariantKind::Pinyin => matches!(kind, KeyKind::PinyinFull | KeyKind::PinyinJoined),
        QueryVariantKind::Initials => {
            matches!(kind, KeyKind::PinyinInitials | KeyKind::LearnedAlias)
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{KeyKind, QueryVariantKind};

    use super::*;

    #[test]
    fn plain_query_expansion_is_small() {
        let vars = PlainBackend.expand_query("Tokyo");
        assert!(vars.iter().any(|v| v.text == "Tokyo"));
        assert!(vars.iter().any(|v| v.text == "tokyo"));
        assert!(vars.len() <= 2);
    }

    #[test]
    fn empty_query_does_not_panic() {
        let vars = PlainBackend.expand_query("");
        assert!(vars.len() <= 1);
    }

    #[test]
    fn romaji_to_kana_variant_only_targets_kana_keys() {
        let variant = QueryVariant::romaji_to_kana("とうきょう");
        assert!(key_kind_allowed(&variant, KeyKind::KanaReading));
        assert!(!key_kind_allowed(&variant, KeyKind::PinyinJoined));
    }

    #[test]
    fn pinyin_initial_variant_only_targets_pinyin_initials_and_aliases() {
        let variant = QueryVariant {
            text: "bjdx".to_string(),
            kind: QueryVariantKind::Initials,
            weight: 0,
        };

        assert!(key_kind_allowed(&variant, KeyKind::PinyinInitials));
        assert!(key_kind_allowed(&variant, KeyKind::LearnedAlias));
        assert!(!key_kind_allowed(&variant, KeyKind::KanaReading));
    }
}
