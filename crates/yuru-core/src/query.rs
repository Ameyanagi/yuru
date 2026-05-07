use std::collections::HashMap;

use crate::{KeyKind, LangMode, LanguageBackend, QueryVariantKind};

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

    fn build_candidate_keys(&self, _text: &str) -> Vec<crate::SearchKey> {
        Vec::new()
    }

    fn expand_query(&self, query: &str) -> Vec<QueryVariant> {
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

fn key_kind_coverage(kind: QueryVariantKind) -> u16 {
    const ORIGINAL: u16 = 1 << 0;
    const NORMALIZED: u16 = 1 << 1;
    const KANA_READING: u16 = 1 << 2;
    const ROMAJI_READING: u16 = 1 << 3;
    const PINYIN_FULL: u16 = 1 << 4;
    const PINYIN_JOINED: u16 = 1 << 5;
    const PINYIN_INITIALS: u16 = 1 << 6;
    const KOREAN_ROMANIZED: u16 = 1 << 7;
    const KOREAN_INITIALS: u16 = 1 << 8;
    const KOREAN_KEYBOARD: u16 = 1 << 9;
    const LEARNED_ALIAS: u16 = 1 << 10;

    match kind {
        QueryVariantKind::Original | QueryVariantKind::Normalized => {
            ORIGINAL
                | NORMALIZED
                | ROMAJI_READING
                | PINYIN_FULL
                | PINYIN_JOINED
                | KOREAN_ROMANIZED
                | KOREAN_INITIALS
                | KOREAN_KEYBOARD
                | LEARNED_ALIAS
        }
        QueryVariantKind::Kana | QueryVariantKind::RomajiToKana => KANA_READING,
        QueryVariantKind::Pinyin => PINYIN_FULL | PINYIN_JOINED,
        QueryVariantKind::Initials => PINYIN_INITIALS | KOREAN_INITIALS | LEARNED_ALIAS,
    }
}

/// Returns whether a query variant may be scored against a key kind.
pub fn key_kind_allowed(variant: &QueryVariant, kind: KeyKind) -> bool {
    match variant.kind {
        QueryVariantKind::Original | QueryVariantKind::Normalized => matches!(
            kind,
            KeyKind::Original
                | KeyKind::Normalized
                | KeyKind::RomajiReading
                | KeyKind::PinyinFull
                | KeyKind::PinyinJoined
                | KeyKind::KoreanRomanized
                | KeyKind::KoreanInitials
                | KeyKind::KoreanKeyboard
                | KeyKind::LearnedAlias
        ),
        QueryVariantKind::Kana | QueryVariantKind::RomajiToKana => {
            matches!(kind, KeyKind::KanaReading)
        }
        QueryVariantKind::Pinyin => matches!(kind, KeyKind::PinyinFull | KeyKind::PinyinJoined),
        QueryVariantKind::Initials => {
            matches!(
                kind,
                KeyKind::PinyinInitials | KeyKind::KoreanInitials | KeyKind::LearnedAlias
            )
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
    fn kana_variant_only_targets_kana_keys() {
        let variant = QueryVariant::kana("はち");
        assert!(key_kind_allowed(&variant, KeyKind::KanaReading));
        assert!(!key_kind_allowed(&variant, KeyKind::RomajiReading));
    }

    #[test]
    fn pinyin_initial_variant_only_targets_pinyin_initials_and_aliases() {
        let variant = QueryVariant {
            text: "bjdx".to_string(),
            kind: QueryVariantKind::Initials,
            weight: 0,
        };

        assert!(key_kind_allowed(&variant, KeyKind::PinyinInitials));
        assert!(key_kind_allowed(&variant, KeyKind::KoreanInitials));
        assert!(key_kind_allowed(&variant, KeyKind::LearnedAlias));
        assert!(!key_kind_allowed(&variant, KeyKind::KanaReading));
    }

    #[test]
    fn dedup_preserves_same_text_when_it_adds_key_coverage() {
        let variants = dedup_and_limit_variants(
            vec![
                QueryVariant::original("bjdx"),
                QueryVariant::initials("bjdx"),
                QueryVariant::pinyin("bjdx"),
                QueryVariant::initials("bjdx"),
            ],
            8,
        );

        assert_eq!(variants.len(), 2);
        assert_eq!(variants[0].kind, QueryVariantKind::Original);
        assert_eq!(variants[1].kind, QueryVariantKind::Initials);
    }
}
