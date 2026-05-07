use crate::{KeyKind, QueryVariantKind};

use super::*;

#[test]
fn plain_query_expansion_is_small() {
    let vars = PlainBackend.expand_query("Tokyo", QueryBudget::default());
    assert!(vars.iter().any(|v| v.text == "Tokyo"));
    assert!(vars.iter().any(|v| v.text == "tokyo"));
    assert!(vars.len() <= 2);
}

#[test]
fn empty_query_does_not_panic() {
    let vars = PlainBackend.expand_query("", QueryBudget::default());
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
